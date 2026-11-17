//! One lazily started librqbit Session per core. SQLite owns items and metadata; librqbit's
//! session index is disposable and its bitfields are only a fast-resume cache (ADR 0011).

use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Session, SessionOptions, TorrentStatsState};
use tokio::sync::OnceCell;

use super::*;
use crate::failure::FailureKind;

/// Kept separately from the item JSON so progress writes and UI snapshots don't copy metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TorrentRecord {
    pub info_hash: String,
    pub metadata: Vec<u8>,
}

/// Embedding configuration. Offline mode uses loopback peers only, with every discovery
/// mechanism disabled; it is also the swarm harness used on all CI runners.
#[derive(Clone, Debug, Default)]
pub struct TorrentNetwork {
    pub offline: bool,
    pub initial_peers: Vec<SocketAddr>,
    pub download_bps: Option<NonZeroU32>,
}

pub struct TorrentService {
    directory: PathBuf,
    port: u16,
    network: TorrentNetwork,
    interval: Duration,
    session: OnceCell<Arc<Session>>,
    retired: std::sync::Mutex<HashSet<usize>>,
    cleanup: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for TorrentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentService").field("started", &self.session.initialized()).finish_non_exhaustive()
    }
}

impl TorrentService {
    pub fn new(directory: PathBuf, port: u16, network: TorrentNetwork, interval: Duration) -> Self {
        Self {
            directory,
            port,
            network,
            interval: interval.max(Duration::from_millis(10)),
            session: OnceCell::new(),
            retired: Default::default(),
            cleanup: Default::default(),
        }
    }

    async fn session(&self) -> Result<Arc<Session>, JobError> {
        self.session
            .get_or_try_init(|| async {
                tokio::fs::create_dir_all(&self.directory).await.map_err(torrent_error)?;
                // Never let librqbit restore jobs on its own, even after a crash. Only SQLite and
                // the scheduler may start them. Keep the hash-keyed .bitv files for fast resume.
                tokio::fs::write(self.directory.join("session.json"), br#"{"torrents":{}}"#).await.map_err(torrent_error)?;
                let offline = self.network.offline;
                let listen_addr = if offline {
                    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
                } else {
                    SocketAddr::from((Ipv6Addr::UNSPECIFIED, self.port))
                };
                let mut opts = SessionOptions {
                    dht: None,
                    disable_trackers: offline,
                    disable_local_service_discovery: offline,
                    fastresume: true,
                    persistence: Some(librqbit::SessionPersistenceConfig::Json { folder: Some(self.directory.clone()) }),
                    listen: Some(librqbit::ListenerOptions {
                        listen_addr,
                        enable_upnp_port_forwarding: !offline,
                        ..Default::default()
                    }),
                    client_name_and_version: Some(format!("Dawnload {}", env!("CARGO_PKG_VERSION"))),
                    ..Default::default()
                };
                if !offline {
                    opts.dht = Some(librqbit::DhtSessionConfig {
                        persistence: Some(librqbit::dht::DhtPersistenceConfig {
                            config_filename: Some(self.directory.join("dht.json")),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                }
                Session::new_with_opts(self.directory.join("downloads"), opts).await.map_err(torrent_error)
            })
            .await
            .cloned()
    }

    pub(crate) fn retire(self: &Arc<Self>, id: ItemId) {
        self.retired.lock().unwrap().insert(id.0 as usize);
        let service = self.clone();
        tokio::spawn(async move {
            service.prune().await;
        });
    }

    async fn prune(&self) {
        let _guard = self.cleanup.lock().await;
        let retired: Vec<_> = self.retired.lock().unwrap().drain().collect();
        if let Some(session) = self.session.get() {
            for id in retired {
                // Remove engine state, never the person's files.
                let _ = session.delete(librqbit::api::TorrentIdOrHash::Id(id), false).await;
            }
        }
    }

    pub async fn shutdown(&self) {
        self.prune().await;
        if let Some(session) = self.session.get() {
            session.stop().await;
        }
    }
}

pub struct TorrentEngine;

impl DownloadEngine for TorrentEngine {
    fn id(&self) -> EngineId {
        EngineId::Torrent
    }
    fn lane(&self) -> Lane {
        Lane::Torrent
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::RESUME | Capabilities::SEEDING | Capabilities::FILE_SELECTION | Capabilities::SWARM
    }
    fn accepts(&self, source: &DownloadSource) -> bool {
        match source {
            DownloadSource::Magnet(_) | DownloadSource::TorrentFile { .. } => true,
            DownloadSource::Web { url } => {
                url::Url::parse(url).ok().and_then(|u| crate::source::url_file_extension(&u)).as_deref() == Some("torrent")
            }
        }
    }
    fn availability(&self, _: &ToolSnapshot, _: &Options) -> Availability {
        Availability::Ready
    }
    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture {
        Box::pin(run(request, context))
    }
}

fn torrent_error(error: impl std::fmt::Display) -> JobError {
    DownloadFailure::new(FailureKind::Other, "The torrent couldn't continue.", Some("The log has the details. Try Resume."))
        .with_detail(error.to_string())
        .into()
}

async fn input(request: &JobRequest, offline: bool) -> Result<AddTorrent<'static>, JobError> {
    if let Some(record) = &request.torrent {
        return Ok(AddTorrent::from_bytes(record.metadata.clone()));
    }
    match &request.source {
        DownloadSource::Magnet(magnet) => Ok(AddTorrent::from_url(magnet.uri.clone())),
        DownloadSource::TorrentFile { path } => Ok(AddTorrent::from_bytes(tokio::fs::read(path).await.map_err(torrent_error)?)),
        DownloadSource::Web { url } => {
            if offline && !url::Url::parse(url).ok().is_some_and(|u| matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))) {
                return Err(torrent_error("Offline mode accepts only loopback metadata URLs."));
            }
            // Use the HTTP engine's client configuration (certificates, proxy and timeouts).
            let response = crate::net::client().get(url).send().await.map_err(|e| DownloadFailure::network(e.to_string()))?;
            if !response.status().is_success() {
                return Err(DownloadFailure::http_status(response.status().as_u16()).into());
            }
            // Metadata is untrusted. Bound memory even for chunked responses.
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            use futures_util::StreamExt;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| DownloadFailure::network(e.to_string()))?;
                if bytes.len() + chunk.len() > 32 * 1024 * 1024 {
                    return Err(torrent_error("Torrent metadata exceeds 32 MiB."));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(AddTorrent::from_bytes(bytes))
        }
    }
}

async fn run(request: JobRequest, mut ctx: JobContext) -> JobResult {
    let service = ctx.torrents.clone().ok_or_else(|| torrent_error("No core torrent service."))?;
    ctx.phase(Status::Pulling, Some("Getting the file list"));
    let session = tokio::select! {
        biased;
        _ = ctx.stop.stopped() => return Err(JobError::Stopped),
        result = service.session() => result?,
    };
    service.prune().await;
    let id = librqbit::api::TorrentIdOrHash::Id(request.item_id.0 as usize);
    let existing = session.get(id);
    let handle = if let Some(handle) = existing {
        handle
    } else {
        let source = tokio::select! {
            biased;
            _ = ctx.stop.stopped() => return Err(JobError::Stopped),
            result = input(&request, service.network.offline) => result?,
        };
        let listing = tokio::select! {
            biased;
            _ = ctx.stop.stopped() => return Err(JobError::Stopped),
            result = session.add_torrent(source, Some(AddTorrentOptions {
                list_only: true,
                initial_peers: Some(service.network.initial_peers.clone()),
                disable_trackers: service.network.offline,
                output_folder: Some(request.options.destination.to_string_lossy().into_owned()),
                ..Default::default()
            })) => result.map_err(torrent_error)?,
        };
        let AddTorrentResponse::ListOnly(listing) = listing else {
            return Err(torrent_error("This torrent already belongs to another item. Pause or remove that item first."));
        };
        let record = TorrentRecord { info_hash: listing.info_hash.as_string(), metadata: listing.torrent_bytes.to_vec() };
        if let Some(stored) = &request.torrent
            && record.info_hash != stored.info_hash
        {
            return Err(torrent_error("The saved torrent's info hash doesn't match its metadata."));
        }
        ctx.emit(JobEvent::Torrent(record));
        ctx.emit(JobEvent::Files(
            listing
                .info
                .iter_file_details()
                .enumerate()
                .map(|(i, file)| ItemFile {
                    path: request.options.destination.join(file.filename.to_pathbuf()).to_string_lossy().into_owned(),
                    size: Some(file.len),
                    bytes_done: Some(0),
                    selected: request.files.get(i).is_none_or(|f| f.selected),
                })
                .collect(),
        ));
        if request.options.ask_for_files && request.files.is_empty() {
            ctx.phase(Status::Pulling, Some("Choose files to download"));
            if let Some(selection) = &mut ctx.selection {
                tokio::select! {
                    biased;
                    _ = ctx.stop.stopped() => return Err(JobError::Stopped),
                    _ = selection.changed() => {},
                }
                // Leave this version unconsumed for monitor to apply before unpausing.
                selection.mark_changed();
            }
        }
        let only_files = (!request.files.is_empty())
            .then(|| request.files.iter().enumerate().filter(|(_, f)| f.selected).map(|(i, _)| i).collect());
        let mut peers = service.network.initial_peers.clone();
        peers.extend(listing.seen_peers);
        // Adding paused avoids starting payload transfer before file selection is known.
        // Don't cancel an add half-way through: it mutates Session ownership. Check stop after.
        let result = session
            .add_torrent(
                AddTorrent::from_bytes(listing.torrent_bytes),
                Some(AddTorrentOptions {
                    paused: true,
                    only_files,
                    overwrite: request.torrent.is_some() || request.resuming,
                    preferred_id: Some(request.item_id.0 as usize),
                    output_folder: Some(request.options.destination.to_string_lossy().into_owned()),
                    initial_peers: Some(peers),
                    disable_trackers: service.network.offline,
                    ratelimits: librqbit::limits::LimitsConfig {
                        download_bps: service.network.download_bps,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .await
            .map_err(torrent_error)?;
        match result {
            AddTorrentResponse::Added(_, handle) => handle,
            _ => return Err(torrent_error("This torrent already belongs to another item.")),
        }
    };
    let result = monitor(&session, &handle, &request, &mut ctx, service.interval).await;
    // Every exit stops traffic, including errors. Keep the paused handle and bitfield for Resume.
    if !handle.is_paused() {
        session.pause(&handle).await.map_err(torrent_error)?;
    }
    result
}

async fn monitor(
    session: &Arc<Session>,
    handle: &Arc<librqbit::ManagedTorrent>,
    request: &JobRequest,
    ctx: &mut JobContext,
    interval: Duration,
) -> JobResult {
    if let Some(title) = handle.name() {
        ctx.emit(JobEvent::Title(title));
    }
    ctx.phase(Status::Pulling, Some("Checking"));
    tokio::select! {
        biased;
        _ = ctx.stop.stopped() => return Err(JobError::Stopped),
        result = handle.wait_until_initialized() => result.map_err(torrent_error)?,
    }
    let mut selected = handle.only_files();
    // A selection made while initialization was running wins over the starting selection.
    if let Some(rx) = &mut ctx.selection
        && rx.has_changed().unwrap_or(false)
    {
        selected = Some(rx.borrow_and_update().clone());
        session
            .update_only_files(handle, &selected.clone().unwrap_or_default().into_iter().collect())
            .await
            .map_err(torrent_error)?;
    }
    if ctx.stop.is_stopped() {
        return Err(JobError::Stopped);
    }
    if handle.is_paused() {
        session.unpause(handle).await.map_err(torrent_error)?;
    }
    let mut phase = Status::Pulling;
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = ctx.stop.stopped() => return Err(JobError::Stopped),
            _ = tick.tick() => {},
        }
        if let Some(rx) = &mut ctx.selection
            && rx.has_changed().unwrap_or(false)
        {
            let indices = rx.borrow_and_update().clone();
            session.update_only_files(handle, &indices.iter().copied().collect::<HashSet<_>>()).await.map_err(torrent_error)?;
            selected = Some(indices);
        }
        let stats = handle.stats();
        if let Some(error) = stats.error {
            return Err(torrent_error(error));
        }
        if matches!(stats.state, TorrentStatsState::Initializing { .. }) {
            continue;
        }
        let files = handle
            .with_metadata(|metadata| {
                metadata
                    .info
                    .iter_file_details()
                    .enumerate()
                    .map(|(i, file)| ItemFile {
                        path: handle.output_folder().join(file.filename.to_pathbuf()).to_string_lossy().into_owned(),
                        size: Some(file.len),
                        bytes_done: Some(stats.file_progress.get(i).copied().unwrap_or(0)),
                        selected: selected.as_ref().is_none_or(|indices| indices.contains(&i)),
                    })
                    .collect::<Vec<_>>()
            })
            .map_err(torrent_error)?;
        let total: u64 = files.iter().filter(|f| f.selected).filter_map(|f| f.size).sum();
        let done: u64 = files.iter().filter(|f| f.selected).filter_map(|f| f.bytes_done).sum();
        let download_rate = stats.live.as_ref().map_or(0.0, |s| s.download_speed.as_bytes() as f64);
        let upload_rate = stats.live.as_ref().map_or(0.0, |s| s.upload_speed.as_bytes() as f64);
        let ratio = if total == 0 { 0.0 } else { stats.uploaded_bytes as f64 / total as f64 };
        ctx.emit(JobEvent::Files(files));
        ctx.emit(JobEvent::Transfer(Transfer {
            fraction: Some(if total == 0 { 0.0 } else { (done as f64 / total as f64).min(1.0) }),
            bytes_done: Some(done),
            bytes_total: Some(total),
            bytes_per_second: Some(download_rate),
            seconds_left: (download_rate > 0.0).then(|| ((total.saturating_sub(done)) as f64 / download_rate) as u64),
            ..Default::default()
        }));
        ctx.emit(JobEvent::Swarm(Swarm {
            peers: stats.live.as_ref().map_or(0, |s| s.snapshot.peer_stats.live),
            seeds: stats.live.as_ref().map_or(0, |s| s.snapshot.peer_stats.live_seeders),
            download_rate,
            upload_rate,
            uploaded: stats.uploaded_bytes,
            ratio,
        }));
        // With no selected files there is no completed download and no reason to seed forever.
        if total == 0 {
            ctx.phase(Status::Pulling, Some("Choose files to download"));
            continue;
        }
        let next = if stats.finished { Status::Seeding } else { Status::Pulling };
        if next != phase || phase == Status::Pulling {
            ctx.phase(next, None);
            phase = next;
        }
        if stats.finished && request.options.seed_ratio_limit.is_some_and(|limit| ratio >= limit) {
            return Ok(JobOutcome::default());
        }
    }
}

//! One lazily started librqbit Session per core. SQLite owns items and metadata; librqbit's
//! session index is disposable and its bitfields are only a fast-resume cache (ADR 0011).

use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Session, SessionOptions, TorrentStatsState};
use tokio::sync::OnceCell;

use super::*;
use crate::failure::FailureKind;
use crate::seeding::{
    SeedClock, SeedConditions, SeedConditionsSource, SeedingSettings, UploadAccounting, UploadCap, UploadEstimate,
};

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
    clock: Arc<dyn SeedClock>,
    conditions_source: Arc<dyn SeedConditionsSource>,
    conditions: tokio::sync::Mutex<Option<(Instant, SeedConditions)>>,
    policy: std::sync::Mutex<(SeedingSettings, UploadEstimate)>,
}

impl std::fmt::Debug for TorrentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentService").field("started", &self.session.initialized()).finish_non_exhaustive()
    }
}

impl TorrentService {
    pub(crate) fn new(
        directory: PathBuf,
        port: u16,
        network: TorrentNetwork,
        interval: Duration,
        (settings, peak): (SeedingSettings, u32),
        clock: Arc<dyn SeedClock>,
        conditions: Arc<dyn SeedConditionsSource>,
    ) -> Self {
        Self {
            directory,
            port,
            network,
            interval: interval.max(Duration::from_millis(10)),
            session: OnceCell::new(),
            retired: Default::default(),
            cleanup: Default::default(),
            clock,
            conditions_source: conditions,
            conditions: Default::default(),
            policy: std::sync::Mutex::new((settings, UploadEstimate::new(peak))),
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

    pub(crate) fn set_settings(&self, settings: SeedingSettings) {
        let mut policy = self.policy.lock().unwrap();
        if policy.0.upload_cap != settings.upload_cap {
            policy.1.reset_window();
        }
        policy.0 = settings;
        if let Some(session) = self.session.get() {
            Self::apply_cap(session, settings.upload_cap.limit(policy.1.peak));
        }
    }

    pub(crate) fn relearn_upload(&self) {
        let mut policy = self.policy.lock().unwrap();
        policy.1 = UploadEstimate::default();
        if let Some(session) = self.session.get() {
            Self::apply_cap(session, policy.0.upload_cap.limit(0));
        }
    }

    pub(crate) fn upload_peak(&self) -> u32 {
        self.policy.lock().unwrap().1.peak
    }

    fn sample_upload(&self, session: &Session) {
        let mut policy = self.policy.lock().unwrap();
        let now = Instant::now();
        if policy.0.upload_cap == UploadCap::Auto && policy.1.wants_sample(now) {
            policy.1.sample(now, session.stats_snapshot().counters.uploaded_bytes);
        }
        Self::apply_cap(session, policy.0.upload_cap.limit(policy.1.peak));
    }

    fn apply_cap(session: &Session, limit: Option<NonZeroU32>) {
        if session.ratelimits.get_upload_bps() != limit {
            session.ratelimits.set_upload_bps(limit);
        }
    }

    async fn courtesy_reason(&self) -> Option<&'static str> {
        if !self.policy.lock().unwrap().0.courtesy {
            return None;
        }
        let mut cached = self.conditions.lock().await;
        if let Some((at, conditions)) = *cached
            && at.elapsed() < Duration::from_secs(2)
        {
            return conditions.pause_reason();
        }
        let source = self.conditions_source.clone();
        let sample = tokio::task::spawn_blocking(move || source.sample()).await.unwrap_or_default();
        *cached = Some((Instant::now(), sample));
        sample.pause_reason()
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

/// Portable component rules, applied on every OS. A Windows drive prefix can replace a
/// destination in PathBuf::push; device names and NTFS streams also have special semantics.
/// Do not sanitize: changing a component could alias another file in the same torrent.
fn validate_component(component: &str) -> Result<(), JobError> {
    let stem = component.split('.').next().unwrap_or_default().trim_end_matches(' ').to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"));
    if component.is_empty()
        || matches!(component, "." | "..")
        || component.ends_with(['.', ' '])
        || device
        || component.chars().any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return Err(DownloadFailure::new(
            FailureKind::Other,
            "The torrent contains an unsafe file name.",
            Some("Use a torrent with ordinary file and folder names."),
        )
        .with_detail(format!("Rejected torrent path component {component:?}"))
        .into());
    }
    Ok(())
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
    if ctx.restoring_seed && request.torrent.is_none() {
        ctx.log("Saved torrent metadata is unavailable. Resume to load it again.");
        return Err(JobError::Stopped);
    }
    if !ctx.restoring_seed {
        ctx.phase(Status::Pulling, Some("Getting the file list"));
    }
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
            })) => result,
        };
        let listing = match listing {
            Ok(listing) => listing,
            Err(error) if ctx.restoring_seed => {
                ctx.log(format!("Saved torrent metadata needs attention: {error:#}"));
                return Err(JobError::Stopped);
            }
            Err(error) => return Err(torrent_error(error)),
        };
        let AddTorrentResponse::ListOnly(listing) = listing else {
            return Err(torrent_error("This torrent already belongs to another item. Pause or remove that item first."));
        };
        // list_only resolves metadata without opening payload storage. Validate decoded path
        // components before publishing paths, persisting metadata, or adding a managed torrent.
        for file in listing.info.iter_file_details() {
            for component in file.filename.iter_components() {
                validate_component(&component)?;
            }
        }
        let record = TorrentRecord { info_hash: listing.info_hash.as_string(), metadata: listing.torrent_bytes.to_vec() };
        if let Some(stored) = &request.torrent
            && record.info_hash != stored.info_hash
        {
            if ctx.restoring_seed {
                ctx.log("The saved torrent's info hash doesn't match its metadata.");
                return Err(JobError::Stopped);
            }
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
    let mut accounting = UploadAccounting::new(ctx.seed_progress);
    let result = monitor(&session, &handle, &request, &mut ctx, &service, &mut accounting).await;
    // Capture the final live sample before the handle drops its counters.
    if let Err(error) = pause_and_checkpoint(&session, &handle, &ctx, &mut accounting).await {
        // A failed pause must not leave an unmanaged uploader behind.
        let _ = session.delete(id, false).await;
        return Err(error);
    }
    result
}

async fn pause_and_checkpoint(
    session: &Session,
    handle: &Arc<librqbit::ManagedTorrent>,
    ctx: &JobContext,
    accounting: &mut UploadAccounting,
) -> Result<(), JobError> {
    if let Some(live) = handle.live() {
        let paused = session.pause(handle).await;
        accounting.sample(live.get_uploaded_bytes());
        accounting.reset_live_counter();
        ctx.emit(JobEvent::SeedProgress(accounting.progress));
        paused.map_err(torrent_error)?;
    } else if !handle.is_paused() && !matches!(handle.stats().state, TorrentStatsState::Error) {
        session.pause(handle).await.map_err(torrent_error)?;
    }
    Ok(())
}

async fn monitor(
    session: &Arc<Session>,
    handle: &Arc<librqbit::ManagedTorrent>,
    request: &JobRequest,
    ctx: &mut JobContext,
    service: &TorrentService,
    accounting: &mut UploadAccounting,
) -> JobResult {
    if let Some(title) = handle.name() {
        ctx.emit(JobEvent::Title(title));
    }
    let mut phase = (if ctx.restoring_seed { Status::Seeding } else { Status::Pulling }, Some("Checking"));
    ctx.phase(phase.0, phase.1);
    tokio::select! {
        biased;
        _ = ctx.stop.stopped() => return Err(JobError::Stopped),
        result = handle.wait_until_initialized() => result.map_err(torrent_error)?,
    }
    let mut selected = handle.only_files();
    // Changes made before Resume were persisted while the cached handle was paused.
    let saved = (!request.files.is_empty())
        .then(|| request.files.iter().enumerate().filter(|(_, f)| f.selected).map(|(i, _)| i).collect::<Vec<_>>());
    if let Some(saved) = saved
        && selected.as_ref() != Some(&saved)
    {
        session.update_only_files(handle, &saved.iter().copied().collect()).await.map_err(torrent_error)?;
        selected = Some(saved);
    }
    let mut restoring_seed = ctx.restoring_seed;
    let mut tick = tokio::time::interval(service.interval);
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
        let has_selection = files.iter().any(|file| file.selected);
        let complete = stats.finished && has_selection;
        let previous_progress = accounting.progress;
        accounting.sample(stats.uploaded_bytes);
        if restoring_seed && !complete {
            ctx.log("Saved torrent files need attention. Resume to download missing data.");
            return Err(JobError::Stopped);
        }
        restoring_seed = false;
        let now = service.clock.now_millis();
        if complete {
            accounting.progress.started_at.get_or_insert(now);
        }
        let limit_reached = complete && accounting.progress.reached(&request.options, total, now);
        let courtesy = if complete && !limit_reached {
            tokio::select! {
                biased;
                _ = ctx.stop.stopped() => return Err(JobError::Stopped),
                reason = service.courtesy_reason() => reason,
            }
        } else {
            None
        };
        if (courtesy.is_some() || limit_reached || !has_selection) && !handle.is_paused() {
            pause_and_checkpoint(session, handle, ctx, accounting).await?;
        } else if !limit_reached && courtesy.is_none() && handle.is_paused() && has_selection {
            if ctx.stop.is_stopped() {
                return Err(JobError::Stopped);
            }
            service.sample_upload(session);
            session.unpause(handle).await.map_err(torrent_error)?;
        }
        service.sample_upload(session);
        let ratio = accounting.progress.ratio(total);
        if accounting.progress != previous_progress {
            ctx.emit(JobEvent::SeedProgress(accounting.progress));
        }
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
            peers: if courtesy.is_some() { 0 } else { stats.live.as_ref().map_or(0, |s| s.snapshot.peer_stats.live) },
            seeds: stats.live.as_ref().map_or(0, |s| s.snapshot.peer_stats.live_seeders),
            download_rate,
            upload_rate: if courtesy.is_some() { 0.0 } else { upload_rate },
            uploaded: accounting.progress.uploaded,
            ratio,
        }));
        let next = if !has_selection {
            (Status::Pulling, Some("Choose files to download"))
        } else if complete {
            (Status::Seeding, courtesy)
        } else {
            (Status::Pulling, None)
        };
        if next != phase {
            ctx.phase(next.0, next.1);
            phase = next;
        }
        if limit_reached {
            return Ok(JobOutcome::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_unicode_and_padding_components_remain_valid() {
        for name in ["Ubuntu 24.04.iso", "日本語", "café", ".pad", "00001", "company", "COM10.txt", "conifer.txt"] {
            validate_component(name).unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pulling_reports_phase_transitions_only() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.bin");
        std::fs::write(&source, vec![17; 256 * 1024]).unwrap();
        let offline =
            || SessionOptions { dht: None, disable_trackers: true, disable_local_service_discovery: true, ..Default::default() };
        let seed = Session::new_with_opts(dir.path().join("seed"), offline()).await.unwrap();
        let (created, _) = seed.create_and_serve_torrent(&source, librqbit::CreateTorrentOptions::default()).await.unwrap();
        // No peer addresses are provided to this downloader: it stays Pulling at zero bytes.
        let session = Session::new_with_opts(dir.path().join("downloads"), offline()).await.unwrap();
        let handle = session
            .add_torrent(
                AddTorrent::from_bytes(created.as_bytes().unwrap()),
                Some(AddTorrentOptions { paused: true, disable_trackers: true, ..Default::default() }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let stop = crate::process::StopSignal::new();
        let mut ctx = JobContext::new(ItemId(1), 1, stop.clone(), sender);
        let request = JobRequest {
            item_id: ItemId(1),
            source: DownloadSource::TorrentFile { path: dir.path().join("unused.torrent") },
            options: Options { seed_ratio_limit: None, ..Default::default() },
            tools: ToolSnapshot::default(),
            files: vec![],
            resuming: false,
            torrent: None,
        };
        let service = TorrentService::new(
            dir.path().join("state"),
            0,
            TorrentNetwork::default(),
            Duration::from_millis(10),
            (SeedingSettings::default(), 0),
            Arc::new(crate::seeding::SystemClock),
            Arc::new(crate::system_conditions::SystemConditions::default()),
        );
        let mut accounting = UploadAccounting::default();
        let (result, phases) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(monitor(&session, &handle, &request, &mut ctx, &service, &mut accounting), async {
                let mut phases = Vec::new();
                let mut samples = 0;
                while let Some((_, _, event)) = receiver.recv().await {
                    match event {
                        JobEvent::Phase(status, detail) => phases.push((status, detail)),
                        JobEvent::Transfer(_) => {
                            samples += 1;
                            if samples == 4 {
                                stop.stop();
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                phases
            })
        })
        .await
        .unwrap();
        session.stop().await;
        seed.stop().await;
        assert_eq!(result, Err(JobError::Stopped));
        assert_eq!(phases, vec![(Status::Pulling, Some("Checking".into())), (Status::Pulling, None)]);
    }
}

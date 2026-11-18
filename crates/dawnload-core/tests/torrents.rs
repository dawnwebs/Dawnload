//! Real librqbit peers, generated data only. No DHT, trackers, LSD, UPnP, DNS or public peers.
//! Runs unchanged on all five CI runners; only loopback sockets are needed.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, Ordering},
};
use std::time::Duration;

use dawnload_core::engine::Lane;
use dawnload_core::engine::torrent::TorrentNetwork;
use dawnload_core::seeding::{SeedClock, SeedConditions, SeedConditionsSource, UploadCap};
use dawnload_core::{Action, Core, CoreConfig, CoreHandle, Item, ItemId, Options, Status};
use librqbit::{CreateTorrentOptions, ListenerOptions, Session, SessionOptions};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[derive(Default)]
struct FakeConditions {
    metered: AtomicBool,
    battery: AtomicBool,
}

impl SeedConditionsSource for FakeConditions {
    fn sample(&self) -> SeedConditions {
        SeedConditions { metered: self.metered.load(Ordering::SeqCst), on_battery: self.battery.load(Ordering::SeqCst) }
    }
}

struct FakeClock(AtomicI64);

impl SeedClock for FakeClock {
    fn now_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Swarm {
    dir: TempDir,
    seeder: Arc<Session>,
    peer: SocketAddr,
    torrent: String,
    magnet: String,
    bytes: Vec<u8>,
}

impl Swarm {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        for (name, salt) in [("one.bin", 1), ("two.bin", 7)] {
            let data: Vec<u8> = (0..1024 * 1024).map(|i| ((i * 31 + i / 997 + salt) % 251) as u8).collect();
            std::fs::write(source.join(name), data).unwrap();
        }
        let seeder = Session::new_with_opts(
            source.clone(),
            SessionOptions {
                dht: None,
                disable_trackers: true,
                disable_local_service_discovery: true,
                listen: Some(ListenerOptions { listen_addr: (Ipv4Addr::LOCALHOST, 0).into(), ..Default::default() }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let (created, handle) = seeder
            .create_and_serve_torrent(&source, CreateTorrentOptions { piece_length: Some(64 * 1024), ..Default::default() })
            .await
            .unwrap();
        handle.wait_until_completed().await.unwrap();
        let bytes = created.as_bytes().unwrap().to_vec();
        let torrent = dir.path().join("fixture.torrent");
        std::fs::write(&torrent, &bytes).unwrap();
        let magnet = format!("magnet:?xt=urn:btih:{}&dn=Offline+fixture", handle.info_hash().as_string());
        Self { peer: seeder.listen_addr().unwrap(), dir, seeder, torrent: torrent.display().to_string(), magnet, bytes }
    }

    fn config(&self) -> CoreConfig {
        let mut config = CoreConfig::new(self.dir.path().join("state"));
        config.check_tools = false;
        config.seed_conditions = Arc::new(FakeConditions::default());
        config.update_interval = Duration::from_millis(25);
        config.torrent_network = TorrentNetwork {
            offline: true,
            initial_peers: vec![self.peer],
            download_bps: Some((512 * 1024).try_into().unwrap()),
        };
        config
    }

    fn core(&self) -> CoreHandle {
        Core::start(self.config()).unwrap()
    }

    fn options(&self) -> Options {
        Options { destination: self.dir.path().join("downloads"), seed_ratio_limit: None, ..Default::default() }
    }

    async fn item(&self, core: &CoreHandle, id: ItemId, predicate: impl Fn(&Item) -> bool) -> Item {
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let detail = core.detail(id).await.unwrap();
                assert_ne!(detail.item.status, Status::Failed, "{:?}\n{:?}", detail.item.failure, detail.log);
                if predicate(&detail.item) {
                    return detail.item;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for item {id}"))
    }

    fn check_files(&self, item: &Item) {
        for file in item.files.iter().filter(|f| f.selected) {
            let actual = std::fs::read(&file.path).unwrap();
            let expected = std::fs::read(self.dir.path().join("source").join(file.name())).unwrap();
            assert_eq!(Sha256::digest(&actual), Sha256::digest(&expected), "{}", file.name());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn magnet_metadata_files_hashes_and_stop_seeding() {
    let swarm = Swarm::new().await;
    let core = swarm.core();
    let id = core.add(&swarm.magnet, Some(swarm.options()), true).await.unwrap();
    let metadata = swarm.item(&core, id, |i| i.files.len() == 2).await;
    assert_eq!(metadata.files.iter().filter_map(|f| f.size).sum::<u64>(), 2 * 1024 * 1024);
    let item = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    assert!(item.swarm.is_some());
    assert_eq!(item.transfer.bytes_done, item.transfer.bytes_total);
    swarm.check_files(&item);
    core.act(Action::StopSeeding { id }).await.unwrap();
    swarm.item(&core, id, |i| i.status == Status::Kept).await;
    // Removing a stopped item releases librqbit ownership, while kept files stay on disk.
    core.act(Action::Remove { ids: vec![id] }).await.unwrap();
    swarm.check_files(&item);
    let again = core
        .add(&swarm.magnet, Some(Options { destination: swarm.dir.path().join("again"), ..swarm.options() }), true)
        .await
        .unwrap();
    let copied = swarm.item(&core, again, |i| i.status == Status::Seeding).await;
    swarm.check_files(&copied);
    core.shutdown().await;
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_file_pause_resume_and_ratio_limit() {
    let swarm = Swarm::new().await;
    let core = swarm.core();
    let mut options = swarm.options();
    options.seed_ratio_limit = Some(0.0);
    let id = core.add(&swarm.torrent, Some(options), true).await.unwrap();
    swarm.item(&core, id, |i| i.transfer.bytes_done.is_some_and(|b| b > 0 && b < 2 * 1024 * 1024)).await;
    core.act(Action::Pause { id }).await.unwrap();
    let paused = swarm.item(&core, id, |i| i.status == Status::Paused).await;
    assert!(paused.transfer.bytes_done.unwrap() < 2 * 1024 * 1024);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(core.detail(id).await.unwrap().item.transfer, paused.transfer);
    core.act(Action::Resume { id }).await.unwrap();
    let item = swarm.item(&core, id, |i| i.status == Status::Kept).await;
    swarm.check_files(&item);
    core.shutdown().await;
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_fastresume_and_deleted_or_corrupt_cache_rebuild_from_sqlite() {
    let swarm = Swarm::new().await;
    let mut core = swarm.core();
    let id = core.add(&swarm.torrent, Some(swarm.options()), true).await.unwrap();
    swarm.item(&core, id, |i| i.transfer.bytes_done.is_some_and(|b| b > 0 && b < 2 * 1024 * 1024)).await;
    let port = core.settings().await.unwrap().torrent_port;
    core.shutdown().await;
    let cache = swarm.dir.path().join("state/torrents");
    assert!(std::fs::read_dir(&cache).unwrap().any(|f| f.unwrap().path().extension().is_some_and(|e| e == "bitv")));
    // The original input disappears: all subsequent launches must use SQLite's BLOB.
    std::fs::remove_file(&swarm.torrent).unwrap();
    for damage in ["none", "deleted", "corrupt"] {
        if damage == "deleted" {
            std::fs::remove_dir_all(&cache).unwrap();
        }
        if damage == "corrupt" {
            std::fs::write(cache.join("session.json"), "not JSON").unwrap();
            for entry in std::fs::read_dir(&cache).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "bitv") {
                    std::fs::write(path, [0u8]).unwrap();
                }
            }
        }
        core = swarm.core();
        assert_eq!(core.settings().await.unwrap().torrent_port, port);
        let initial = core.detail(id).await.unwrap().item;
        assert_eq!(initial.status, Status::Paused);
        core.act(Action::Resume { id }).await.unwrap();
        let item = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
        swarm.check_files(&item);
        core.act(Action::Pause { id }).await.unwrap();
        swarm.item(&core, id, |i| i.status == Status::Paused).await;
        core.shutdown().await;
    }
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_selection_skips_and_changes_while_running_and_survives_restart() {
    let swarm = Swarm::new().await;
    let core = swarm.core();
    let mut options = swarm.options();
    options.ask_for_files = true;
    let id = core.add(&swarm.magnet, Some(options), true).await.unwrap();
    swarm.item(&core, id, |i| i.files.len() == 2).await;
    core.act(Action::SelectFiles { id, indices: vec![0] }).await.unwrap();
    let item = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    assert!(item.files[0].selected);
    assert!(!item.files[1].selected);
    assert_eq!(item.files[1].bytes_done, Some(0));
    swarm.check_files(&item);
    assert!(core.act(Action::SelectFiles { id, indices: vec![99] }).await.is_err());
    core.act(Action::SelectFiles { id, indices: vec![0, 1] }).await.unwrap();
    swarm.item(&core, id, |i| i.status == Status::Pulling).await;
    let both = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    assert!(both.files.iter().all(|f| f.selected));
    swarm.check_files(&both);
    core.act(Action::SelectFiles { id, indices: vec![1] }).await.unwrap();
    core.shutdown().await;
    let core = swarm.core();
    assert!(!core.detail(id).await.unwrap().item.files[0].selected);
    let item = swarm.item(&core, id, |i| i.status == Status::Seeding && i.phase_detail.is_none()).await;
    assert!(!item.files[0].selected);
    assert_eq!(item.transfer.bytes_total, Some(1024 * 1024));
    core.shutdown().await;
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_url_and_seeding_releases_the_real_lane_slot() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let swarm = Swarm::new().await;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let url = format!("http://{}/fixture.torrent", listener.local_addr().unwrap());
    let bytes = swarm.bytes.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let read = socket.read(&mut request).await.unwrap();
        assert!(read > 0);
        socket
            .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).as_bytes())
            .await
            .unwrap();
        socket.write_all(&bytes).await.unwrap();
    });
    let core = swarm.core();
    let mut settings = core.settings().await.unwrap();
    settings.lane_limits.insert(Lane::Torrent, 1);
    core.set_settings(settings).await;
    let first = core.add(&url, Some(swarm.options()), true).await.unwrap();
    let second_source = swarm.dir.path().join("second.bin");
    std::fs::write(&second_source, vec![41u8; 256 * 1024]).unwrap();
    let (created, seed) = swarm
        .seeder
        .create_and_serve_torrent(&second_source, CreateTorrentOptions { piece_length: Some(64 * 1024), ..Default::default() })
        .await
        .unwrap();
    seed.wait_until_completed().await.unwrap();
    let second_path = swarm.dir.path().join("second.torrent");
    std::fs::write(&second_path, created.as_bytes().unwrap()).unwrap();
    let second = core
        .add(
            second_path.to_str().unwrap(),
            Some(Options { destination: swarm.dir.path().join("other"), ..swarm.options() }),
            true,
        )
        .await
        .unwrap();
    assert_eq!(core.detail(second).await.unwrap().item.status, Status::Queued);
    swarm.item(&core, first, |i| i.status == Status::Seeding).await;
    let second_item = swarm.item(&core, second, |i| i.status == Status::Seeding).await;
    assert_eq!(std::fs::read(&second_item.files[0].path).unwrap(), vec![41u8; 256 * 1024]);
    assert_eq!(core.detail(first).await.unwrap().item.status, Status::Seeding);
    core.act(Action::StopSeeding { id: first }).await.unwrap();
    swarm.item(&core, first, |i| i.status == Status::Kept).await;
    core.shutdown().await;
    server.await.unwrap();
    swarm.seeder.stop().await;
}

/// Tiny, tracker-free, generated torrent. One zero byte; arbitrary metadata path components.
fn crafted_torrent(parts: &[&str], single_file: bool) -> Vec<u8> {
    fn string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(format!("{}:", value.len()).as_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    let mut bytes = b"d4:infod".to_vec();
    if single_file {
        bytes.extend_from_slice(b"6:lengthi1e4:name");
        string(&mut bytes, parts[0]);
    } else {
        bytes.extend_from_slice(b"5:filesld6:lengthi1e4:pathl");
        for part in parts {
            string(&mut bytes, part);
        }
        bytes.extend_from_slice(b"eee4:name7:fixture");
    }
    bytes.extend_from_slice(b"12:piece lengthi16384e6:pieces20:");
    bytes.extend(hex::decode("5ba93c9db0cff93f52b521d7420e43f6eda2784f").unwrap());
    bytes.extend_from_slice(b"ee");
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsafe_metadata_paths_are_rejected_before_files_or_payload_are_created() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = CoreConfig::new(dir.path().join("state"));
    config.check_tools = false;
    config.torrent_network.offline = true;
    let core = Core::start(config).unwrap();
    let mut accepted = Vec::new();
    let names =
        ["C:", "C:escape.bin", "media:stream", "CON", "nul.txt", "LPT1.log", "COM¹", "trail.", "trail ", ".", "bad\0name"];
    for (index, name) in names.iter().enumerate() {
        for single in [false, true] {
            let path = dir.path().join(format!("{index}-{single}.torrent"));
            let parts = if single { vec![*name] } else { vec![*name, "payload.bin"] };
            std::fs::write(&path, crafted_torrent(&parts, single)).unwrap();
            let destination = dir.path().join(format!("output-{index}-{single}"));
            let id = core
                .add(
                    path.to_str().unwrap(),
                    Some(Options { destination: destination.clone(), seed_ratio_limit: None, ..Default::default() }),
                    true,
                )
                .await
                .unwrap();
            let item = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let item = core.detail(id).await.unwrap().item;
                    if item.status == Status::Failed || !item.files.is_empty() {
                        break item;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            if item.status != Status::Failed || !item.files.is_empty() || destination.exists() {
                accepted.push(format!("{name:?} (single={single})"));
            }
            core.act(Action::Remove { ids: vec![id] }).await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while core.detail(id).await.is_some() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
    }
    core.shutdown().await;
    assert!(accepted.is_empty(), "unsafe metadata reached Files or filesystem storage: {accepted:?}");
}

/// An actual Windows path-semantics probe: upstream accepts the drive component, and its
/// conversion followed by the destination join discards the chosen download directory.
/// It never opens a file. The rejection test above exercises Dawnload's protective boundary.
#[cfg(windows)]
#[test]
fn upstream_drive_component_can_replace_the_windows_destination() {
    let bytes = crafted_torrent(&["C:", "payload.bin"], false);
    let parsed = librqbit::torrent_from_bytes(&bytes).unwrap();
    let info = parsed.info.data.clone().validate().unwrap();
    let relative = info.iter_file_details().next().unwrap().filename.to_pathbuf();
    let destination = std::path::PathBuf::from(r"C:\Dawnload\chosen-folder");
    assert!(!destination.join(relative).starts_with(&destination));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn courtesy_keeps_downloads_running_and_seed_deadline_survives_restart() {
    let swarm = Swarm::new().await;
    let conditions = Arc::new(FakeConditions::default());
    conditions.battery.store(true, Ordering::SeqCst);
    let clock = Arc::new(FakeClock(AtomicI64::new(1_000_000)));
    let start = || {
        let mut config = swarm.config();
        config.seed_conditions = conditions.clone();
        config.seed_clock = clock.clone();
        Core::start(config).unwrap()
    };
    let core = start();
    let id = core
        .add(&swarm.magnet, Some(Options { seed_time_limit_secs: Some(30), seed_ratio_limit: None, ..swarm.options() }), true)
        .await
        .unwrap();

    let on_battery = swarm.item(&core, id, |i| i.phase_detail.as_deref() == Some("Paused on battery")).await;
    assert_eq!(on_battery.status, Status::Seeding);
    assert_eq!(on_battery.seed_progress.started_at, Some(1_000_000));
    swarm.check_files(&on_battery);
    assert_eq!(on_battery.swarm.unwrap().upload_rate, 0.0);

    conditions.metered.store(true, Ordering::SeqCst);
    conditions.battery.store(false, Ordering::SeqCst);
    swarm.item(&core, id, |i| i.phase_detail.as_deref() == Some("Paused on metered network")).await;
    conditions.metered.store(false, Ordering::SeqCst);
    swarm.item(&core, id, |i| i.status == Status::Seeding && i.phase_detail.is_none()).await;
    core.shutdown().await;

    clock.0.store(1_029_999, Ordering::SeqCst);
    let core = start();
    let resumed = swarm.item(&core, id, |i| i.status == Status::Seeding && i.phase_detail.is_none()).await;
    assert_eq!(resumed.seed_progress.started_at, Some(1_000_000));
    clock.0.store(1_030_000, Ordering::SeqCst);
    swarm.item(&core, id, |i| i.status == Status::Kept).await;
    core.shutdown().await;
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn near_limit_seed_keeps_its_budget_through_pauses_and_restarts() {
    use librqbit::{AddTorrent, AddTorrentOptions};
    let swarm = Swarm::new().await;
    let core = swarm.core();
    let id = core.add(&swarm.torrent, Some(swarm.options()), true).await.unwrap();
    swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    core.act(Action::Pause { id }).await.unwrap();
    swarm.item(&core, id, |i| i.status == Status::Paused).await;
    core.shutdown().await;

    // Start just short of the ratio limit. Preloading a durable checkpoint isolates the
    // boundary regardless of how the scheduler batches loopback packets.
    let config = swarm.config();
    let mut store = dawnload_core::store::Store::open(config.database.as_ref().unwrap()).unwrap();
    let mut item = store.load_items().unwrap().pop().unwrap();
    let selected_bytes = item.kept_bytes().unwrap();
    let previous = selected_bytes - 32 * 1024;
    item.seed_progress.uploaded = previous;
    store.apply(&[dawnload_core::store::StoreOp::Upsert { item: Box::new(item), position: 0 }]).unwrap();
    drop(store);

    let core = swarm.core();
    core.act(Action::SetSeedLimits { id, ratio: Some(1.0), seconds: None }).await.unwrap();
    core.act(Action::Resume { id }).await.unwrap();
    swarm.item(&core, id, |i| i.status == Status::Seeding && i.swarm.as_ref().is_some_and(|s| s.uploaded == previous)).await;
    core.shutdown().await;
    // Launch once with no downloaders and verify the old bytes were neither lost nor added twice.
    let core = swarm.core();
    swarm.item(&core, id, |i| i.status == Status::Seeding && i.swarm.as_ref().is_some_and(|s| s.uploaded == previous)).await;
    core.shutdown().await;

    let receiver = Session::new_with_opts(
        swarm.dir.path().join("leecher"),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions { listen_addr: (Ipv4Addr::LOCALHOST, 0).into(), ..Default::default() }),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let leecher = receiver
        .add_torrent(
            AddTorrent::from_bytes(swarm.bytes.clone()),
            Some(AddTorrentOptions { disable_trackers: true, ..Default::default() }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    let mut config = swarm.config();
    config.torrent_network.initial_peers = vec![receiver.listen_addr().unwrap()];
    let conditions = Arc::new(FakeConditions::default());
    conditions.metered.store(true, Ordering::SeqCst);
    config.seed_conditions = conditions.clone();
    let core = Core::start(config).unwrap();
    let mut settings = core.settings().await.unwrap();
    settings.seeding.upload_cap = UploadCap::Fixed((64 * 1024).try_into().unwrap());
    core.set_settings(settings).await;
    let parked = swarm.item(&core, id, |i| i.phase_detail.as_deref() == Some("Paused on metered network")).await;
    assert_eq!(parked.seed_progress.uploaded, previous);
    assert_eq!(leecher.stats().progress_bytes, 0);
    conditions.metered.store(false, Ordering::SeqCst);
    let kept = swarm.item(&core, id, |i| i.status == Status::Kept).await;
    assert!(kept.seed_progress.uploaded >= selected_bytes);
    assert!(kept.seed_progress.uploaded < previous + selected_bytes / 2, "used more than the remaining budget");
    core.shutdown().await;
    receiver.stop().await;
    swarm.seeder.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restored_seed_with_missing_data_waits_for_manual_resume() {
    let swarm = Swarm::new().await;
    let core = swarm.core();
    let id = core.add(&swarm.torrent, Some(swarm.options()), true).await.unwrap();
    let item = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    core.shutdown().await;
    std::fs::remove_file(&item.files[0].path).unwrap();
    let core = swarm.core();
    swarm.item(&core, id, |i| i.status == Status::Paused).await;
    core.act(Action::Resume { id }).await.unwrap();
    let repaired = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
    swarm.check_files(&repaired);
    core.shutdown().await;
    swarm.seeder.stop().await;
}

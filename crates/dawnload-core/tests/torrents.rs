//! Real librqbit peers, generated data only. No DHT, trackers, LSD, UPnP, DNS or public peers.
//! Runs unchanged on all five CI runners; only loopback sockets are needed.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use dawnload_core::engine::Lane;
use dawnload_core::engine::torrent::TorrentNetwork;
use dawnload_core::{Action, Core, CoreConfig, CoreHandle, Item, ItemId, Options, Status};
use librqbit::{CreateTorrentOptions, ListenerOptions, Session, SessionOptions};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

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
    core.act(Action::Resume { id }).await.unwrap();
    let item = swarm.item(&core, id, |i| i.status == Status::Seeding).await;
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

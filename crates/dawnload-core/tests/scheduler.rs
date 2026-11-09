//! The queue's promises, tested with a scripted engine. The URL path says what the job does:
//! `/ok`, `/fail`, `/panic`, `/net-once` (drops once, then works), `/slow` (runs until stopped),
//! `/seed` (pulls, then seeds until stopped), `/chatty` (hundreds of progress events).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dawnload_core::api::{Action, Mirror, Update};
use dawnload_core::core::{Core, CoreConfig, CoreHandle};
use dawnload_core::engine::{
    Availability, Capabilities, DownloadEngine, JobContext, JobError, JobEvent, JobFuture, JobOutcome, JobRequest, Lane, Registry,
};
use dawnload_core::failure::{DownloadFailure, FailureKind};
use dawnload_core::model::{ItemFile, ItemId, Options, Status, Swarm, Transfer};
use dawnload_core::source::{DownloadSource, EngineId};
use dawnload_core::tools::{ToolManager, ToolSnapshot};

#[derive(Default)]
struct Log {
    starts: HashMap<String, u32>,
    resumed: HashMap<String, bool>,
}

struct Scripted {
    lane: Lane,
    log: Arc<Mutex<Log>>,
}

impl DownloadEngine for Scripted {
    fn id(&self) -> EngineId {
        EngineId::Demo
    }
    fn lane(&self) -> Lane {
        self.lane
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::RESUME | Capabilities::SEEDING
    }
    fn accepts(&self, _: &DownloadSource) -> bool {
        true
    }
    fn availability(&self, _: &ToolSnapshot, _: &Options) -> Availability {
        Availability::Ready
    }
    fn run(&self, request: JobRequest, ctx: JobContext) -> JobFuture {
        let DownloadSource::Web { url } = request.source.clone() else { panic!("web only") };
        let script = url.rsplit('/').next().unwrap().to_string();
        let attempt = {
            let mut log = self.log.lock().unwrap();
            let n = {
                let n = log.starts.entry(url.clone()).or_default();
                *n += 1;
                *n
            };
            log.resumed.insert(url.clone(), request.resuming);
            n
        };
        Box::pin(async move {
            ctx.emit(JobEvent::Title(format!("{script} #{attempt}")));
            ctx.phase(Status::Pulling, None);
            let progress = |done: u64| {
                ctx.emit(JobEvent::Transfer(Transfer { fraction: Some(done as f64 / 100.0), bytes_done: Some(done), bytes_total: Some(100), ..Transfer::default() }));
                ctx.emit(JobEvent::Files(vec![ItemFile { path: format!("/tmp/{script}"), size: Some(100), bytes_done: Some(done), selected: true }]));
            };
            match script.split('-').next().unwrap() {
                "ok" => {
                    progress(50);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(JobOutcome::default())
                }
                "fail" => Err(JobError::Failed(DownloadFailure::new(FailureKind::NotFound, "The page wasn't found (HTTP 404).", Some("Check the link.")))),
                "panic" => panic!("engine bug"),
                "net" if attempt == 1 => Err(JobError::Failed(DownloadFailure::network("reset"))),
                "net" => Ok(JobOutcome::default()),
                "slow" => {
                    progress(if request.resuming { 60 } else { 30 });
                    ctx.stop.stopped().await;
                    Err(JobError::Stopped)
                }
                "seed" => {
                    progress(100);
                    ctx.phase(Status::Seeding, None);
                    ctx.emit(JobEvent::Swarm(Swarm { peers: 3, ratio: 0.2, ..Swarm::default() }));
                    ctx.stop.stopped().await;
                    Err(JobError::Stopped)
                }
                "chatty" => {
                    for i in 0..400u64 {
                        progress(i % 100);
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                    Ok(JobOutcome::default())
                }
                other => panic!("unknown script {other}"),
            }
        })
    }
}

struct Harness {
    core: CoreHandle,
    log: Arc<Mutex<Log>>,
    _dir: tempfile::TempDir,
}

fn config(dir: &std::path::Path, lane: Lane, log: Arc<Mutex<Log>>) -> CoreConfig {
    let mut config = CoreConfig::new(dir.to_path_buf());
    config.registry = Registry::with(vec![Arc::new(Scripted { lane, log })]);
    config.engine_override = Some(EngineId::Demo);
    config.tools = ToolManager::new(dir.join("tools")).with_system_search(vec![]);
    config.check_tools = false;
    config.update_interval = Duration::from_millis(20);
    config
}

async fn harness(lane: Lane, limit: usize) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Log::default()));
    let core = Core::start(config(dir.path(), lane, log.clone())).unwrap();
    let mut settings = core.settings().await.unwrap();
    settings.lane_limits.insert(lane, limit);
    settings.defaults.destination = dir.path().join("downloads");
    core.set_settings(settings).await;
    Harness { core, log, _dir: dir }
}

async fn add(core: &CoreHandle, script: &str, start: bool) -> ItemId {
    core.add(&format!("https://example.test/{script}"), None, start).await.unwrap()
}

async fn status(core: &CoreHandle, id: ItemId) -> Status {
    core.detail(id).await.unwrap().item.status
}

/// Waits until `check` holds, or fails the test after 15 s.
async fn until(core: &CoreHandle, what: &str, check: impl Fn(&HashMap<ItemId, Status>) -> bool) {
    for _ in 0..1500 {
        let snapshot = core.snapshot().await.unwrap();
        let statuses: HashMap<ItemId, Status> = snapshot.items.iter().map(|i| (i.id, i.status)).collect();
        if check(&statuses) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let snapshot = core.snapshot().await.unwrap();
    panic!("timed out waiting for {what}: {:?}", snapshot.items.iter().map(|i| (i.id, i.status, i.phase_detail.clone())).collect::<Vec<_>>());
}

#[tokio::test]
async fn one_failure_never_stops_the_rest() {
    let h = harness(Lane::Web, 1).await;
    let fail = add(&h.core, "fail", false).await;
    let boom = add(&h.core, "panic", false).await;
    let ok = add(&h.core, "ok", true).await;
    until(&h.core, "all finished", |s| s.values().all(|s| matches!(s, Status::Kept | Status::Failed))).await;
    assert_eq!(status(&h.core, fail).await, Status::Failed);
    let crashed = h.core.detail(boom).await.unwrap();
    assert_eq!(crashed.item.status, Status::Failed);
    assert_eq!(crashed.item.failure.unwrap().kind, FailureKind::Internal);
    assert_eq!(status(&h.core, ok).await, Status::Kept);
    let failed = h.core.detail(fail).await.unwrap();
    assert!(failed.log.iter().any(|l| l.starts_with("Failed: The page wasn't found")), "{:?}", failed.log);
}

#[tokio::test]
async fn a_lane_runs_its_limit_and_lanes_are_independent() {
    let h = harness(Lane::Web, 2).await;
    let ids = [add(&h.core, "slow-1", false).await, add(&h.core, "slow-2", false).await, add(&h.core, "slow-3", true).await];
    until(&h.core, "two pulling", |s| s.values().filter(|s| **s == Status::Pulling).count() == 2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let snapshot = h.core.snapshot().await.unwrap();
    assert_eq!(snapshot.items.iter().filter(|i| i.status == Status::Pulling).count(), 2);
    assert_eq!(snapshot.items[2].status, Status::Queued, "the third waits its turn, in order");
    h.core.act(Action::Pause { id: ids[0] }).await.unwrap();
    until(&h.core, "third starts when a slot frees", |s| s[&ids[2]] == Status::Pulling && s[&ids[0]] == Status::Paused).await;
}

#[tokio::test]
async fn seeding_frees_the_slot_and_stop_seeding_reaches_the_job() {
    let h = harness(Lane::Torrent, 1).await;
    let seed = add(&h.core, "seed", true).await;
    until(&h.core, "seeding", |s| s[&seed] == Status::Seeding).await;
    let next = add(&h.core, "slow", true).await;
    until(&h.core, "the next item runs beside the seeding one", |s| s[&next] == Status::Pulling && s[&seed] == Status::Seeding).await;
    let view = h.core.snapshot().await.unwrap().items.into_iter().find(|i| i.id == seed).unwrap();
    assert_eq!(view.swarm.unwrap().peers, 3);
    assert!(view.finished_at.is_some(), "files are kept while it seeds");

    h.core.act(Action::StopSeeding { id: seed }).await.unwrap();
    until(&h.core, "kept after stop seeding", |s| s[&seed] == Status::Kept).await;
    assert!(h.core.detail(seed).await.unwrap().log.iter().any(|l| l == "Stopped seeding."));
    assert_eq!(status(&h.core, next).await, Status::Pulling, "stopping one never touches the other");
    assert!(h.core.act(Action::StopSeeding { id: next }).await.is_err());
}

#[tokio::test]
async fn pause_and_resume_continue_from_partial_files() {
    let h = harness(Lane::Web, 1).await;
    let id = add(&h.core, "slow", true).await;
    until(&h.core, "pulling", |s| s[&id] == Status::Pulling).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    h.core.act(Action::Pause { id }).await.unwrap();
    until(&h.core, "paused", |s| s[&id] == Status::Paused).await;
    let paused = h.core.detail(id).await.unwrap().item;
    assert_eq!(paused.transfer.fraction, Some(0.3), "where it stopped");
    assert_eq!(paused.files[0].bytes_done, Some(30));

    h.core.act(Action::Resume { id }).await.unwrap();
    until(&h.core, "pulling again", |s| s[&id] == Status::Pulling).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let log = h.log.lock().unwrap();
    assert_eq!(log.starts["https://example.test/slow"], 2);
    assert!(log.resumed["https://example.test/slow"], "the engine is told to continue");
}

#[tokio::test(start_paused = true)]
async fn dropped_connections_retry_on_their_own() {
    let h = harness(Lane::Web, 1).await;
    let id = add(&h.core, "net-once", true).await;
    until(&h.core, "waiting to retry", |s| s[&id] == Status::Queued).await;
    let waiting = h.core.detail(id).await.unwrap().item;
    assert_eq!(waiting.phase_detail.as_deref(), Some("Retrying in 5s"));
    until(&h.core, "kept after the retry", |s| s[&id] == Status::Kept).await;
    assert_eq!(h.log.lock().unwrap().starts["https://example.test/net-once"], 2);
}

#[tokio::test]
async fn reorder_and_resume_jump_the_line() {
    let h = harness(Lane::Web, 1).await;
    let a = add(&h.core, "slow-a", false).await;
    let b = add(&h.core, "slow-b", false).await;
    let c = add(&h.core, "slow-c", false).await;
    h.core.act(Action::Move { id: c, index: 0 }).await.unwrap();
    let order: Vec<ItemId> = h.core.snapshot().await.unwrap().items.iter().map(|i| i.id).collect();
    assert_eq!(order, vec![c, a, b]);
    h.core.act(Action::Resume { id: b }).await.unwrap();
    until(&h.core, "b runs first", |s| s[&b] == Status::Pulling && s[&a] == Status::Queued && s[&c] == Status::Queued).await;
    assert!(h.core.act(Action::Move { id: b, index: 0 }).await.is_err(), "running items don't move");
}

#[tokio::test]
async fn the_same_link_twice_is_one_item() {
    let h = harness(Lane::Web, 1).await;
    let first = add(&h.core, "slow", false).await;
    assert_eq!(add(&h.core, "slow", false).await, first);
    assert!(h.core.add("not a link", None, false).await.is_err());
}

#[tokio::test]
async fn subscribers_get_batches_at_a_bounded_rate() {
    let h = harness(Lane::Web, 1).await;
    let mut sub = h.core.subscribe().await.unwrap();
    let mut mirror = Mirror::default();
    mirror.apply(sub.recv().await.unwrap());
    let id = add(&h.core, "chatty", true).await;
    sub.watch(Some(id));

    let started = std::time::Instant::now();
    let mut batches = 0;
    let mut details = 0;
    while started.elapsed() < Duration::from_secs(5) {
        let Ok(Some(update)) = tokio::time::timeout(Duration::from_secs(1), sub.recv()).await else { break };
        if matches!(update, Update::Batch(_)) {
            batches += 1;
        }
        let applied = mirror.apply(update);
        details += applied.detail.is_some() as u32;
        if mirror.item(id).is_some_and(|i| i.status == Status::Kept) {
            break;
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    // ~800 job events in ~1 s, but at most one batch per 20 ms tick.
    assert!(batches as f64 <= elapsed / 0.020 + 3.0, "{batches} batches in {elapsed:.2}s");
    assert!(batches < 200, "{batches}");
    assert!(details > 0, "the watched item's detail arrives");
    assert_eq!(mirror.item(id).unwrap().status, Status::Kept);
    let snapshot = h.core.snapshot().await.unwrap();
    assert_eq!(mirror.counts, snapshot.counts);
    assert_eq!(mirror.order, snapshot.items.iter().map(|i| i.id).collect::<Vec<_>>());
}

#[tokio::test]
async fn nothing_is_lost_across_quit_and_crash() {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Log::default()));
    let (running, waiting, kept);
    {
        let core = Core::start(config(dir.path(), Lane::Web, log.clone())).unwrap();
        assert!(matches!(Core::start(config(dir.path(), Lane::Web, log.clone())), Err(dawnload_core::CoreError::AlreadyRunning(_))));
        kept = add(&core, "ok", true).await;
        until(&core, "kept", |s| s[&kept] == Status::Kept).await;
        running = add(&core, "slow", true).await;
        waiting = add(&core, "slow-2", false).await;
        until(&core, "pulling", |s| s[&running] == Status::Pulling).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        core.shutdown().await;
    }
    {
        let core = Core::start(config(dir.path(), Lane::Web, log.clone())).unwrap();
        let snapshot = core.snapshot().await.unwrap();
        let statuses: Vec<(ItemId, Status)> = snapshot.items.iter().map(|i| (i.id, i.status)).collect();
        assert_eq!(statuses, vec![(kept, Status::Kept), (running, Status::Paused), (waiting, Status::Queued)]);
        assert!(!snapshot.queue.running, "a fresh launch waits for Start Queue");
        let detail = core.detail(running).await.unwrap();
        assert_eq!(detail.item.transfer.fraction, Some(0.3));
        assert!(detail.log.iter().any(|l| l == "Paused."), "{:?}", detail.log);
        drop(core); // dropping every handle shuts down cleanly too
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // A crash: the store says Pulling, nobody stopped it.
    {
        let mut store = dawnload_core::store::Store::open(&dawnload_core::paths::database_path(dir.path())).unwrap();
        let mut items = store.load_items().unwrap();
        items[2].status = Status::Pulling;
        store.apply(&[dawnload_core::store::StoreOp::Upsert { item: Box::new(items[2].clone()), position: 2 }]).unwrap();
    }
    let core = Core::start(config(dir.path(), Lane::Web, log)).unwrap();
    assert_eq!(status(&core, waiting).await, Status::Paused, "running at the crash → Paused");
}

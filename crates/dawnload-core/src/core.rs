//! The core: one actor that owns every item, runs the queue lane by lane, persists everything,
//! and feeds subscribers snapshots and batched diffs. UIs talk to it only through [`CoreHandle`].
//!
//! Rules it keeps:
//! - One failure never stops the rest: a failed, crashed or panicking job fails its item only.
//! - Nothing is lost on quit or crash: items are written as they change; anything that was running
//!   comes back Paused, and Resume continues from the partial files.
//! - A seeding job stays owned by the queue (so Stop Seeding reaches it) but doesn't hold a
//!   download slot in its lane.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::api::{Action, Batch, Change, Counts, ItemDetail, ItemView, QueueState, Snapshot, Update};
use crate::engine::{JobContext, JobError, JobEvent, JobOutcome, JobRequest, JobResult, Lane, Registry};
use crate::explain::{Explanation, explain};
use crate::failure::DownloadFailure;
use crate::model::{Item, ItemFile, ItemId, Options, Status, now_millis};
use crate::process::StopSignal;
use crate::source::{DownloadSource, EngineId};
use crate::store::{Store, StoreOp, StoreWriter};
use crate::tools::{ToolId, ToolManager, ToolSnapshot, ToolStatus};

/// Settings the person changes. Stored in SQLite.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// What new items download with (the intake chips).
    pub defaults: Options,
    /// Download slots per lane.
    pub lane_limits: BTreeMap<Lane, usize>,
    /// Update Dawnload's own tool copies once a day.
    pub auto_update_tools: bool,
    /// Automatic retries for a dropped connection before an item is Failed.
    pub auto_retries: u32,
    /// Tools the person pointed at themselves (Settings › Tools › Choose…). They win over
    /// Dawnload's own copies and the system's.
    pub tool_paths: BTreeMap<ToolId, PathBuf>,
    /// The cookies.txt chosen in Settings › Access. The intake's cookies chip turns it on for new
    /// items (`defaults.cookies`); this keeps the file while the chip is off.
    pub cookies_file: Option<PathBuf>,
    /// How the app looks and behaves. The core only stores these.
    pub app: AppPrefs,
}

/// Light, dark, or whatever the system says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

/// Animation: follow the system's setting, or always reduce it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    #[default]
    System,
    Reduce,
    Full,
}

/// The app's own preferences (Settings › General), kept with the rest so they survive a reinstall
/// of the binary and travel with a portable data folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPrefs {
    pub appearance: Appearance,
    pub motion: Motion,
    /// Closing the window keeps Dawnload running in the tray (or menu bar).
    pub close_to_tray: bool,
    /// Notify when an item is Kept.
    pub notify_kept: bool,
    /// Notify when an item Failed.
    pub notify_failed: bool,
    /// The first-run sheet was seen.
    pub first_run_done: bool,
    pub show_inspector: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        AppPrefs {
            appearance: Appearance::System,
            motion: Motion::System,
            close_to_tray: true,
            notify_kept: true,
            notify_failed: true,
            first_run_done: false,
            show_inspector: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            defaults: Options::default(),
            lane_limits: Lane::ALL.into_iter().map(|l| (l, l.default_limit())).collect(),
            auto_update_tools: true,
            auto_retries: 2,
            tool_paths: BTreeMap::new(),
            cookies_file: None,
            app: AppPrefs::default(),
        }
    }
}

impl Settings {
    pub fn lane_limit(&self, lane: Lane) -> usize {
        self.lane_limits.get(&lane).copied().unwrap_or(lane.default_limit()).max(1)
    }
}

pub struct CoreConfig {
    pub data_dir: PathBuf,
    /// None keeps everything in memory (demos, tests).
    pub database: Option<PathBuf>,
    pub registry: Registry,
    pub tools: ToolManager,
    /// How often subscribers get a batch. 250 ms = 4 per second.
    pub update_interval: Duration,
    /// Route every item to this engine (the demo).
    pub engine_override: Option<EngineId>,
    /// Hold `dawnload.lock` so two cores never run the same queue.
    pub single_instance: bool,
    /// Check tool versions in the background at start.
    pub check_tools: bool,
}

impl CoreConfig {
    pub fn new(data_dir: PathBuf) -> CoreConfig {
        CoreConfig {
            database: Some(crate::paths::database_path(&data_dir)),
            tools: ToolManager::new(crate::paths::tools_dir(&data_dir)),
            data_dir,
            registry: Registry::standard(),
            update_interval: Duration::from_millis(250),
            engine_override: None,
            single_instance: true,
            check_tools: true,
        }
    }

    /// In memory, every item on the demo engine.
    pub fn demo() -> CoreConfig {
        let mut config = CoreConfig::new(std::env::temp_dir().join("dawnload-demo"));
        config.database = None;
        config.engine_override = Some(EngineId::Demo);
        config.single_instance = false;
        config.check_tools = false;
        config
    }
}

#[derive(Debug)]
pub enum CoreError {
    /// Another Dawnload (the app, or `dawnload run`) owns this data folder.
    AlreadyRunning(PathBuf),
    Store(String),
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::AlreadyRunning(dir) => write!(
                f,
                "Another Dawnload is running with {} (the app, or `dawnload run`). Quit it first; the local control API (roadmap phase 6) will let them share.",
                dir.display()
            ),
            CoreError::Store(e) => write!(f, "Can't open Dawnload's state: {e}"),
        }
    }
}

impl std::error::Error for CoreError {}

/// A cheap, cloneable line to the core. When every handle is dropped the core stops its jobs,
/// saves, and ends.
#[derive(Clone)]
pub struct CoreHandle {
    tx: mpsc::UnboundedSender<Command>,
}

/// Updates for one subscriber. Dropping it unsubscribes.
pub struct Subscription {
    id: u64,
    rx: mpsc::Receiver<Update>,
    watch: mpsc::UnboundedSender<(u64, Option<ItemId>)>,
}

impl Subscription {
    pub async fn recv(&mut self) -> Option<Update> {
        self.rx.recv().await
    }

    /// For UI threads that aren't async.
    pub fn blocking_recv(&mut self) -> Option<Update> {
        self.rx.blocking_recv()
    }

    pub fn try_recv(&mut self) -> Option<Update> {
        self.rx.try_recv().ok()
    }

    /// Also send this item's [`ItemDetail`] (log, files) whenever it changes. One at a time.
    pub fn watch(&self, item: Option<ItemId>) {
        let _ = self.watch.send((self.id, item));
    }
}

enum Command {
    Add { text: String, options: Option<Options>, start: bool, reply: oneshot::Sender<Result<ItemId, DownloadFailure>> },
    Explain { text: String, options: Option<Options>, reply: oneshot::Sender<Option<Explanation>> },
    Act { action: Action, reply: oneshot::Sender<Result<(), String>> },
    Subscribe { capacity: usize, reply: oneshot::Sender<Subscription> },
    Snapshot { reply: oneshot::Sender<Snapshot> },
    Detail { id: ItemId, reply: oneshot::Sender<Option<ItemDetail>> },
    Settings { reply: oneshot::Sender<Settings> },
    SetSettings { settings: Box<Settings>, reply: oneshot::Sender<()> },
    ToolSnapshot { reply: oneshot::Sender<ToolSnapshot> },
    ToolsChanged { statuses: Vec<ToolStatus>, notice: Option<String> },
    ToolManager { reply: oneshot::Sender<ToolManager> },
    RefreshTools { check_updates: bool },
    Shutdown { reply: oneshot::Sender<()> },
}

impl CoreHandle {
    fn send<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Command) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(make(tx));
        rx
    }

    /// Adds what was pasted. If the same link is already waiting or running, returns that item.
    pub async fn add(&self, text: &str, options: Option<Options>, start: bool) -> Result<ItemId, DownloadFailure> {
        self.send(|reply| Command::Add { text: text.to_string(), options, start, reply })
            .await
            .unwrap_or_else(|_| Err(DownloadFailure::internal("the core has stopped")))
    }

    /// The detection line for the paste field.
    pub async fn explain(&self, text: &str, options: Option<Options>) -> Option<Explanation> {
        self.send(|reply| Command::Explain { text: text.to_string(), options, reply }).await.ok().flatten()
    }

    pub async fn act(&self, action: Action) -> Result<(), String> {
        self.send(|reply| Command::Act { action, reply }).await.unwrap_or_else(|_| Err("the core has stopped".into()))
    }

    /// Fire and forget, for UI threads.
    pub fn act_now(&self, action: Action) {
        let (reply, _) = oneshot::channel();
        let _ = self.tx.send(Command::Act { action, reply });
    }

    pub async fn subscribe(&self) -> Option<Subscription> {
        self.send(|reply| Command::Subscribe { capacity: 8, reply }).await.ok()
    }

    pub async fn snapshot(&self) -> Option<Snapshot> {
        self.send(|reply| Command::Snapshot { reply }).await.ok()
    }

    pub async fn detail(&self, id: ItemId) -> Option<ItemDetail> {
        self.send(|reply| Command::Detail { id, reply }).await.ok().flatten()
    }

    pub async fn settings(&self) -> Option<Settings> {
        self.send(|reply| Command::Settings { reply }).await.ok()
    }

    pub async fn set_settings(&self, settings: Settings) {
        let _ = self.send(|reply| Command::SetSettings { settings: Box::new(settings), reply }).await;
    }

    pub async fn tool_snapshot(&self) -> Option<ToolSnapshot> {
        self.send(|reply| Command::ToolSnapshot { reply }).await.ok()
    }

    /// Tell subscribers about tools that changed (after an install or update elsewhere).
    pub fn tools_changed(&self, statuses: Vec<ToolStatus>, notice: Option<String>) {
        let _ = self.tx.send(Command::ToolsChanged { statuses, notice });
    }

    /// The core's tool manager, to install or update tools while it runs. Call
    /// [`CoreHandle::tools_changed`] afterwards so subscribers hear about it.
    pub async fn tool_manager(&self) -> Option<ToolManager> {
        self.send(|reply| Command::ToolManager { reply }).await.ok()
    }

    /// Reads every tool's version again (and, with `check_updates`, the newest releases);
    /// subscribers get a `Tools` change when it's done.
    pub fn refresh_tools(&self, check_updates: bool) {
        let _ = self.tx.send(Command::RefreshTools { check_updates });
    }

    /// Stops every job (running items become Paused), saves, and ends the core.
    pub async fn shutdown(&self) {
        let _ = self.send(|reply| Command::Shutdown { reply }).await;
    }
}

pub struct Core;

impl Core {
    /// Starts the core on the current tokio runtime.
    pub fn start(config: CoreConfig) -> Result<CoreHandle, CoreError> {
        let lock = if config.single_instance {
            std::fs::create_dir_all(&config.data_dir).map_err(|e| CoreError::Store(e.to_string()))?;
            let path = config.data_dir.join("dawnload.lock");
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .map_err(|e| CoreError::Store(e.to_string()))?;
            match file.try_lock() {
                Ok(()) => Some(file),
                Err(std::fs::TryLockError::WouldBlock) => return Err(CoreError::AlreadyRunning(config.data_dir.clone())),
                Err(std::fs::TryLockError::Error(e)) => return Err(CoreError::Store(e.to_string())),
            }
        } else {
            None
        };

        let (items, settings, reader, writer) = match &config.database {
            Some(path) => {
                let store = Store::open(path).map_err(|e| CoreError::Store(e.to_string()))?;
                let items: Vec<Item> = store.load_items().map_err(|e| CoreError::Store(e.to_string()))?;
                let settings = store
                    .setting("settings")
                    .ok()
                    .flatten()
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default();
                let writer = StoreWriter::spawn(Store::open(path).map_err(|e| CoreError::Store(e.to_string()))?);
                (items, settings, Some(store), Some(writer))
            }
            None => (Vec::new(), Settings::default(), None, None),
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        let (timer_tx, timer_rx) = mpsc::unbounded_channel();
        let (watch_tx, watch_rx) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        let handle = CoreHandle { tx };
        let tools_checked_at: i64 =
            reader.as_ref().and_then(|r| r.setting("tools_checked_at").ok().flatten()).and_then(|v| v.parse().ok()).unwrap_or(0);

        // Ids are never reused, even after a remove, so `dawnload log 5` always means one item.
        let stored_next: u64 =
            reader.as_ref().and_then(|r| r.setting("next_id").ok().flatten()).and_then(|v| v.parse().ok()).unwrap_or(1);
        let mut tools = config.tools;
        apply_tool_paths(&mut tools, &settings);
        let mut actor = Actor {
            next_id: (items.iter().map(|i| i.id.0).max().unwrap_or(0) + 1).max(stored_next),
            order: items.iter().map(|i| i.id).collect(),
            items: HashMap::new(),
            logs: HashMap::new(),
            running: HashMap::new(),
            not_before: HashMap::new(),
            next_run: 1,
            queue_running: false,
            settings,
            registry: config.registry,
            tools,
            tool_cache: None,
            tool_statuses: Vec::new(),
            engine_override: config.engine_override,
            reader,
            writer,
            subs: Vec::new(),
            next_sub: 1,
            dirty: HashSet::new(),
            removed: Vec::new(),
            order_dirty: false,
            counts_dirty: true,
            detail_dirty: HashSet::new(),
            tools_dirty: false,
            notices: Vec::new(),
            persist_dirty: HashSet::new(),
            revision: 0,
            events_tx,
            done_tx,
            timer_tx,
            watch_tx,
            internal_tx: internal_tx.clone(),
            shutting_down: false,
            _lock: lock,
        };
        // Anything that was running when Dawnload stopped comes back Paused.
        let mut recovered = Vec::new();
        for item in items {
            let was_running = item.status.is_running();
            let item = item.recovered();
            if was_running {
                recovered.push(item.id);
            }
            actor.items.insert(item.id, item);
        }
        for id in recovered {
            actor.persist(id);
        }

        if config.check_tools {
            let due = now_millis() - tools_checked_at > DAY_MS;
            spawn_tool_check(actor.tools.clone(), due && actor.settings.auto_update_tools, internal_tx);
        }
        let channels =
            Channels { commands: rx, events: events_rx, done: done_rx, timers: timer_rx, watch: watch_rx, internal: internal_rx };
        tokio::spawn(actor.run(channels, config.update_interval));
        Ok(handle)
    }
}

/// Reads tool versions in the background; once a day also updates Dawnload's managed copies.
fn spawn_tool_check(manager: ToolManager, auto_update: bool, internal: mpsc::UnboundedSender<Internal>) {
    tokio::spawn(async move {
        let statuses = manager.status().await;
        let _ = internal.send(Internal::Tools { statuses, notice: None });
        if !auto_update {
            return;
        }
        let mut quiet = |_: ToolId, _: u64, _: Option<u64>| {};
        let results = manager.update_managed(&mut quiet).await;
        let _ = internal.send(Internal::ToolsChecked);
        let updated: Vec<String> = results
            .iter()
            .filter_map(|(tool, r)| r.as_ref().ok().and_then(|v| v.as_ref()).map(|v| format!("{} {v}", tool.name())))
            .collect();
        if !updated.is_empty() {
            let statuses = manager.status().await;
            let _ = internal.send(Internal::Tools { statuses, notice: Some(format!("Updated {}.", updated.join(", "))) });
        }
    });
}

/// Reads tool versions (and the newest releases, if asked) without updating anything.
fn spawn_tool_refresh(manager: ToolManager, check_updates: bool, internal: mpsc::UnboundedSender<Internal>) {
    tokio::spawn(async move {
        let statuses = if check_updates { manager.check_updates().await } else { manager.status().await };
        let _ = internal.send(Internal::Tools { statuses, notice: None });
    });
}

/// Custom tool paths from Settings become the tool manager's overrides.
fn apply_tool_paths(tools: &mut ToolManager, settings: &Settings) {
    for tool in ToolId::ALL {
        tools.set_override(tool, settings.tool_paths.get(&tool).cloned());
    }
}

/// Messages from the core's own background tasks.
enum Internal {
    Tools { statuses: Vec<ToolStatus>, notice: Option<String> },
    ToolsChecked,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

struct Running {
    run: u64,
    stop: StopSignal,
    lane: Lane,
    intent: Option<StopIntent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopIntent {
    Pause,
    StopSeeding,
    Remove,
}

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Update>,
    detail: Option<ItemId>,
    needs_snapshot: bool,
}

struct Actor {
    items: HashMap<ItemId, Item>,
    order: Vec<ItemId>,
    logs: HashMap<ItemId, VecDeque<String>>,
    running: HashMap<ItemId, Running>,
    not_before: HashMap<ItemId, tokio::time::Instant>,
    next_id: u64,
    next_run: u64,
    queue_running: bool,
    settings: Settings,
    registry: Registry,
    tools: ToolManager,
    tool_cache: Option<(Instant, ToolSnapshot)>,
    tool_statuses: Vec<ToolStatus>,
    engine_override: Option<EngineId>,
    reader: Option<Store>,
    writer: Option<StoreWriter>,
    subs: Vec<Subscriber>,
    next_sub: u64,
    dirty: HashSet<ItemId>,
    removed: Vec<ItemId>,
    order_dirty: bool,
    counts_dirty: bool,
    detail_dirty: HashSet<ItemId>,
    tools_dirty: bool,
    notices: Vec<String>,
    persist_dirty: HashSet<ItemId>,
    revision: u64,
    events_tx: mpsc::UnboundedSender<(ItemId, u64, JobEvent)>,
    done_tx: mpsc::UnboundedSender<(ItemId, u64, JobResult)>,
    timer_tx: mpsc::UnboundedSender<ItemId>,
    watch_tx: mpsc::UnboundedSender<(u64, Option<ItemId>)>,
    internal_tx: mpsc::UnboundedSender<Internal>,
    shutting_down: bool,
    _lock: Option<std::fs::File>,
}

const LOG_LINES_IN_MEMORY: usize = 400;

struct Channels {
    commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedReceiver<(ItemId, u64, JobEvent)>,
    done: mpsc::UnboundedReceiver<(ItemId, u64, JobResult)>,
    timers: mpsc::UnboundedReceiver<ItemId>,
    watch: mpsc::UnboundedReceiver<(u64, Option<ItemId>)>,
    internal: mpsc::UnboundedReceiver<Internal>,
}

impl Actor {
    async fn run(mut self, channels: Channels, interval: Duration) {
        let Channels { mut commands, mut events, mut done, mut timers, mut watch, mut internal } = channels;
        let mut flush = tokio::time::interval(interval);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut persist = tokio::time::interval(Duration::from_secs(2));
        persist.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut shutdown_reply: Option<oneshot::Sender<()>> = None;
        let mut shutdown_deadline: Option<tokio::time::Instant> = None;

        loop {
            if let Some(deadline) = shutdown_deadline
                && (self.running.is_empty() || tokio::time::Instant::now() >= deadline)
            {
                break;
            }
            tokio::select! {
                command = commands.recv(), if shutdown_deadline.is_none() => match command {
                    Some(Command::Shutdown { reply }) => {
                        shutdown_reply = Some(reply);
                        shutdown_deadline = Some(self.begin_shutdown());
                    }
                    Some(command) => self.command(command),
                    None => shutdown_deadline = Some(self.begin_shutdown()),
                },
                Some((id, run, event)) = events.recv() => {
                    self.event(id, run, event);
                    // Drain what's queued so a burst becomes one pass.
                    for _ in 0..4096 {
                        match events.try_recv() {
                            Ok((id, run, event)) => self.event(id, run, event),
                            Err(_) => break,
                        }
                    }
                }
                Some((id, run, result)) = done.recv() => self.job_done(id, run, result),
                Some(id) = timers.recv() => {
                    self.not_before.remove(&id);
                    self.pump();
                }
                Some((subscriber, item)) = watch.recv() => {
                    if let Some(sub) = self.subs.iter_mut().find(|s| s.id == subscriber) {
                        sub.detail = item;
                    }
                    if let Some(id) = item {
                        self.detail_dirty.insert(id);
                    }
                }
                Some(message) = internal.recv() => self.internal(message),
                _ = flush.tick() => self.flush(),
                _ = persist.tick() => self.persist_progress(),
                _ = tokio::time::sleep(Duration::from_millis(50)), if shutdown_deadline.is_some() => {}
            }
        }

        // Whatever didn't stop in time comes back Paused next launch.
        let leftover: Vec<ItemId> = self.running.keys().copied().collect();
        for id in leftover {
            self.running.remove(&id);
            if let Some(item) = self.items.remove(&id) {
                self.items.insert(id, item.recovered());
            }
            self.persist(id);
        }
        self.persist_progress();
        self.flush();
        if let Some(writer) = &self.writer {
            let writer = writer.clone();
            let _ = tokio::task::spawn_blocking(move || writer.flush_blocking()).await;
        }
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(());
        }
    }

    fn begin_shutdown(&mut self) -> tokio::time::Instant {
        self.shutting_down = true;
        self.queue_running = false;
        for running in self.running.values() {
            running.stop.stop();
        }
        tokio::time::Instant::now() + Duration::from_secs(5)
    }

    // MARK: Commands

    fn command(&mut self, command: Command) {
        match command {
            Command::Add { text, options, start, reply } => {
                let _ = reply.send(self.add(&text, options, start));
            }
            Command::Explain { text, options, reply } => {
                let options = options.unwrap_or_else(|| self.settings.defaults.clone());
                let tools = self.tool_snapshot();
                let _ = reply.send(explain(&text, &options, &tools, &self.registry, self.engine_override));
            }
            Command::Act { action, reply } => {
                let _ = reply.send(self.act(action));
            }
            Command::Subscribe { capacity, reply } => {
                let (tx, rx) = mpsc::channel(capacity.max(2));
                let id = self.next_sub;
                self.next_sub += 1;
                let _ = tx.try_send(Update::Snapshot(self.snapshot()));
                self.subs.push(Subscriber { id, tx, detail: None, needs_snapshot: false });
                let _ = reply.send(Subscription { id, rx, watch: self.watch_tx.clone() });
            }
            Command::Snapshot { reply } => {
                let _ = reply.send(self.snapshot());
            }
            Command::Detail { id, reply } => {
                let _ = reply.send(self.detail(id));
            }
            Command::Settings { reply } => {
                let _ = reply.send(self.settings.clone());
            }
            Command::SetSettings { settings, reply } => {
                let tools_moved = settings.tool_paths != self.settings.tool_paths;
                self.settings = *settings;
                if tools_moved {
                    apply_tool_paths(&mut self.tools, &self.settings);
                    self.tool_cache = None;
                    spawn_tool_refresh(self.tools.clone(), false, self.internal_tx.clone());
                }
                self.store(vec![StoreOp::Setting {
                    key: "settings".into(),
                    value: serde_json::to_string(&self.settings).unwrap_or_default(),
                }]);
                self.pump();
                let _ = reply.send(());
            }
            Command::ToolSnapshot { reply } => {
                let _ = reply.send(self.tool_snapshot());
            }
            Command::ToolsChanged { statuses, notice } => self.internal(Internal::Tools { statuses, notice }),
            Command::ToolManager { reply } => {
                let _ = reply.send(self.tools.clone());
            }
            Command::RefreshTools { check_updates } => {
                self.tool_cache = None;
                spawn_tool_refresh(self.tools.clone(), check_updates, self.internal_tx.clone());
            }
            Command::Shutdown { .. } => unreachable!("handled in run"),
        }
    }

    fn internal(&mut self, message: Internal) {
        match message {
            Internal::Tools { statuses, notice } => {
                if !statuses.is_empty() {
                    self.tool_statuses = statuses;
                    self.tools_dirty = true;
                    self.tool_cache = None;
                }
                if let Some(text) = notice {
                    self.notices.push(text);
                }
            }
            Internal::ToolsChecked => {
                self.store(vec![StoreOp::Setting { key: "tools_checked_at".into(), value: now_millis().to_string() }]);
            }
        }
    }

    fn tool_snapshot(&mut self) -> ToolSnapshot {
        match &self.tool_cache {
            Some((at, snapshot)) if at.elapsed() < Duration::from_secs(3) => snapshot.clone(),
            _ => {
                let snapshot = self.tools.discover();
                self.tool_cache = Some((Instant::now(), snapshot.clone()));
                snapshot
            }
        }
    }

    fn add(&mut self, text: &str, options: Option<Options>, start: bool) -> Result<ItemId, DownloadFailure> {
        let source = DownloadSource::parse(text).ok_or_else(DownloadFailure::not_a_link)?;
        let mut options = options.unwrap_or_else(|| self.settings.defaults.clone());
        if options.destination.is_relative() {
            options.destination = std::env::current_dir().unwrap_or_default().join(&options.destination);
        }
        let engine_id = self.engine_override.unwrap_or_else(|| options.engine.resolve(&source));
        let engine = self.registry.get(engine_id).cloned().ok_or_else(DownloadFailure::torrents_unavailable)?;
        let tools = self.tool_snapshot();
        if let Some(failure) = engine.availability(&tools, &options).failure() {
            return Err(failure);
        }
        if let Some(existing) = self.order.iter().find(|id| {
            self.items.get(id).is_some_and(|i| i.source == source && !matches!(i.status, Status::Kept | Status::Failed))
        }) {
            return Ok(*existing);
        }

        let id = ItemId(self.next_id);
        self.next_id += 1;
        self.store(vec![StoreOp::Setting { key: "next_id".into(), value: self.next_id.to_string() }]);
        let mut item = Item::new(id, source, options);
        item.engine = engine_id;
        self.items.insert(id, item);
        self.order.push(id);
        self.order_dirty = true;
        self.mark(id);
        self.persist(id);
        if start {
            self.queue_running = true;
            self.counts_dirty = true;
        }
        self.pump();
        Ok(id)
    }

    fn act(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::StartQueue => {
                self.queue_running = true;
                self.counts_dirty = true;
                self.pump();
            }
            Action::PauseAll => {
                self.queue_running = false;
                self.counts_dirty = true;
                let ids: Vec<ItemId> = self.running.keys().copied().collect();
                for id in ids {
                    if self.items.get(&id).is_some_and(|i| i.status != Status::Seeding) {
                        self.stop(id, StopIntent::Pause);
                    }
                }
            }
            Action::ResumeAll => {
                let paused: Vec<ItemId> =
                    self.order.iter().copied().filter(|id| self.items[id].status == Status::Paused).collect();
                for id in paused {
                    self.set_status(id, Status::Queued);
                    self.persist(id);
                }
                self.queue_running = true;
                self.pump();
            }
            Action::Pause { id } => {
                let status = self.items.get(&id).ok_or("No such item.")?.status;
                if self.running.contains_key(&id) {
                    self.stop(id, StopIntent::Pause);
                } else if status == Status::Queued {
                    self.not_before.remove(&id);
                    let item = self.items.get_mut(&id).unwrap();
                    item.phase_detail = None;
                    self.set_status(id, Status::Paused);
                    self.persist(id);
                }
            }
            Action::Resume { id } => {
                let item = self.items.get_mut(&id).ok_or("No such item.")?;
                if !item.status.is_editable() {
                    return Err(format!("It's {}, so there's nothing to resume.", item.status.word()));
                }
                item.failure = None;
                item.phase_detail = None;
                item.attempts = 0;
                self.not_before.remove(&id);
                self.set_status(id, Status::Queued);
                self.move_waiting(id, 0);
                self.persist(id);
                self.queue_running = true;
                self.pump();
            }
            Action::Remove { ids } => {
                for id in ids {
                    if self.running.contains_key(&id) {
                        self.stop(id, StopIntent::Remove);
                    } else if self.items.contains_key(&id) {
                        self.delete(id);
                    }
                }
                self.pump();
            }
            Action::StopSeeding { id } => {
                if self.items.get(&id).is_some_and(|i| i.status == Status::Seeding) {
                    self.stop(id, StopIntent::StopSeeding);
                } else {
                    return Err("It isn't seeding.".into());
                }
            }
            Action::Move { id, index } => {
                if self.items.get(&id).map(|i| i.status) != Some(Status::Queued) {
                    return Err("Only waiting items can move.".into());
                }
                self.move_waiting(id, index);
            }
            Action::SetOptions { id, options } => {
                let override_engine = self.engine_override;
                let item = self.items.get_mut(&id).ok_or("No such item.")?;
                if !item.status.is_editable() {
                    return Err("Its options can change once it's paused or finished.".into());
                }
                item.engine = override_engine.unwrap_or_else(|| options.engine.resolve(&item.source));
                item.options = options;
                self.mark(id);
                self.persist(id);
            }
            Action::Clear { status } => {
                if status.is_running() {
                    return Err("Pause them first.".into());
                }
                let ids: Vec<ItemId> = self.order.iter().copied().filter(|id| self.items[id].status == status).collect();
                for id in ids {
                    self.delete(id);
                }
            }
        }
        Ok(())
    }

    /// Moves a waiting item among waiting items. The slots waiting items occupy in the order
    /// stay where they are; only who sits in them changes.
    fn move_waiting(&mut self, id: ItemId, index: usize) {
        let slots: Vec<usize> =
            self.order.iter().enumerate().filter(|(_, i)| self.items[i].status == Status::Queued).map(|(n, _)| n).collect();
        let mut waiting: Vec<ItemId> = slots.iter().map(|n| self.order[*n]).collect();
        let Some(from) = waiting.iter().position(|i| *i == id) else { return };
        let moving = waiting.remove(from);
        waiting.insert(index.min(waiting.len()), moving);
        for (slot, id) in slots.iter().zip(waiting) {
            self.order[*slot] = id;
        }
        self.order_dirty = true;
        self.store(vec![StoreOp::Order(self.order.clone())]);
    }

    fn stop(&mut self, id: ItemId, intent: StopIntent) {
        if let Some(running) = self.running.get_mut(&id) {
            running.intent = Some(intent);
            running.stop.stop();
            if let Some(item) = self.items.get_mut(&id) {
                item.phase_detail = Some(match intent {
                    StopIntent::Pause => "Pausing".into(),
                    StopIntent::StopSeeding => "Stopping".into(),
                    StopIntent::Remove => "Removing".into(),
                });
                self.dirty.insert(id);
            }
        }
    }

    fn delete(&mut self, id: ItemId) {
        if let Some(item) = self.items.remove(&id)
            && !item.status.has_files()
        {
            // Leftovers of an unfinished direct download. Kept files are never touched.
            for file in &item.files {
                for suffix in [".dawnload-part", ".dawnload-state"] {
                    let _ = std::fs::remove_file(format!("{}{suffix}", file.path));
                }
            }
        }
        self.order.retain(|i| *i != id);
        self.logs.remove(&id);
        self.not_before.remove(&id);
        self.dirty.remove(&id);
        self.persist_dirty.remove(&id);
        self.removed.push(id);
        self.order_dirty = true;
        self.counts_dirty = true;
        self.store(vec![StoreOp::Delete(id)]);
    }

    // MARK: The queue

    fn lane_of(&self, item: &Item) -> Lane {
        self.registry.get(item.engine).map(|e| e.lane()).unwrap_or(Lane::Web)
    }

    /// Download slots in use: seeding jobs don't count.
    fn busy(&self, lane: Lane) -> usize {
        self.running
            .iter()
            .filter(|(id, r)| r.lane == lane && self.items.get(id).is_some_and(|i| i.status != Status::Seeding))
            .count()
    }

    fn pump(&mut self) {
        if !self.queue_running || self.shutting_down {
            return;
        }
        let now = tokio::time::Instant::now();
        let mut free: HashMap<Lane, usize> =
            Lane::ALL.into_iter().map(|l| (l, self.settings.lane_limit(l).saturating_sub(self.busy(l)))).collect();
        let candidates: Vec<ItemId> = self
            .order
            .iter()
            .copied()
            .filter(|id| self.items[id].status == Status::Queued && self.not_before.get(id).is_none_or(|t| *t <= now))
            .collect();
        for id in candidates {
            let lane = self.lane_of(&self.items[&id]);
            if let Some(slots) = free.get_mut(&lane)
                && *slots > 0
            {
                *slots -= 1;
                self.start(id);
            }
        }
        let waiting = self.items.values().any(|i| i.status == Status::Queued);
        if self.running.is_empty() && !waiting {
            self.queue_running = false;
            self.counts_dirty = true;
        }
    }

    fn start(&mut self, id: ItemId) {
        let tools = self.tool_snapshot();
        let item = self.items.get(&id).expect("item");
        let Some(engine) = self.registry.get(item.engine).cloned() else {
            self.fail(id, DownloadFailure::torrents_unavailable());
            return;
        };
        if let Some(failure) = engine.availability(&tools, &item.options).failure() {
            self.fail(id, failure);
            return;
        }
        let resuming = item.files.iter().any(|f| f.bytes_done.is_some_and(|b| b > 0));
        let request = JobRequest {
            item_id: id,
            source: item.source.clone(),
            options: item.options.clone(),
            tools,
            files: item.files.clone(),
            resuming,
        };
        let run = self.next_run;
        self.next_run += 1;
        let stop = StopSignal::new();
        let context = JobContext::new(id, run, stop.clone(), self.events_tx.clone());
        let future = engine.run(request, context);
        let done_tx = self.done_tx.clone();
        tokio::spawn(async move {
            // A panic inside an engine fails that item, nothing else.
            let result = match tokio::spawn(future).await {
                Ok(result) => result,
                Err(join) => Err(JobError::Failed(DownloadFailure::internal(if join.is_panic() {
                    "the engine crashed"
                } else {
                    "the job was cancelled"
                }))),
            };
            let _ = done_tx.send((id, run, result));
        });
        self.running.insert(id, Running { run, stop, lane: engine.lane(), intent: None });

        let item = self.items.get_mut(&id).unwrap();
        item.failure = None;
        item.phase_detail = None;
        item.swarm = None;
        item.finished_at = None;
        item.attempts += 1;
        if !resuming {
            item.transfer = Default::default();
        }
        self.set_status(id, Status::Pulling);
        self.log(id, format!("Started at {}{}", clock_time(), if resuming { ", continuing" } else { "" }));
        self.persist(id);
    }

    fn event(&mut self, id: ItemId, run: u64, event: JobEvent) {
        if self.running.get(&id).map(|r| r.run) != Some(run) {
            return;
        }
        let stopping = self.running.get(&id).is_some_and(|r| r.intent.is_some());
        let Some(item) = self.items.get_mut(&id) else { return };
        match event {
            JobEvent::Command(command) => {
                item.command = Some(command.clone());
                self.log(id, format!("$ {command}"));
            }
            JobEvent::Log(line) => self.log(id, line),
            JobEvent::Phase(status, detail) => {
                if stopping {
                    return;
                }
                item.phase_detail = detail;
                if status == Status::Seeding && item.status != Status::Seeding {
                    item.finished_at = Some(now_millis());
                    item.transfer.fraction = Some(1.0);
                    item.transfer.bytes_per_second = None;
                    item.transfer.seconds_left = None;
                    let count = item.kept_files().count();
                    let folder = item.options.destination.display().to_string();
                    self.set_status(id, Status::Seeding);
                    self.log(id, format!("Kept {} in {folder}; seeding.", crate::format::count(count, "file")));
                    self.persist(id);
                    self.pump(); // its download slot is free now
                } else if status != item.status {
                    self.set_status(id, status);
                    self.persist(id);
                } else {
                    self.mark(id);
                }
            }
            JobEvent::Transfer(transfer) => {
                item.transfer = transfer;
                self.mark(id);
                self.persist_dirty.insert(id);
            }
            JobEvent::Title(title) => {
                item.title = Some(title);
                self.mark(id);
                self.persist_dirty.insert(id);
            }
            JobEvent::Files(files) => {
                item.files = files;
                self.mark(id);
                self.detail_dirty.insert(id);
                self.persist_dirty.insert(id);
            }
            JobEvent::Swarm(swarm) => {
                item.swarm = Some(swarm);
                self.mark(id);
            }
        }
    }

    fn job_done(&mut self, id: ItemId, run: u64, result: JobResult) {
        if self.running.get(&id).map(|r| r.run) != Some(run) {
            return;
        }
        let running = self.running.remove(&id).expect("running");
        if running.intent == Some(StopIntent::Remove) {
            self.delete(id);
            self.pump();
            return;
        }
        match result {
            Ok(outcome) => self.finish(id, outcome),
            Err(JobError::Stopped) => {
                let was_seeding = self.items[&id].status == Status::Seeding;
                let item = self.items.get_mut(&id).unwrap();
                item.phase_detail = None;
                item.transfer.bytes_per_second = None;
                item.transfer.seconds_left = None;
                let (status, line) = match running.intent {
                    Some(StopIntent::StopSeeding) => (Status::Kept, "Stopped seeding."),
                    _ if was_seeding => (Status::Paused, "Paused seeding."),
                    _ => (Status::Paused, "Paused."),
                };
                self.set_status(id, status);
                self.log(id, line.into());
                self.persist(id);
            }
            Err(JobError::Failed(failure)) => self.fail(id, failure),
        }
        self.pump();
    }

    fn finish(&mut self, id: ItemId, outcome: JobOutcome) {
        let item = self.items.get_mut(&id).unwrap();
        if !outcome.files.is_empty() {
            item.files = outcome.files.iter().map(ItemFile::on_disk).collect();
        }
        let sizes: Vec<u64> = item.files.iter().filter_map(|f| f.size).collect();
        let t = &mut item.transfer;
        t.fraction = Some(1.0);
        t.bytes_per_second = None;
        t.seconds_left = None;
        if !sizes.is_empty() {
            t.bytes_total = Some(sizes.iter().sum());
            t.bytes_done = t.bytes_total;
        }
        item.phase_detail = None;
        item.finished_at = item.finished_at.or(Some(now_millis()));
        item.attempts = 0;
        let count = item.kept_files().count();
        let folder = item.options.destination.display().to_string();
        let polished = if outcome.polished > 0 { format!(", {} polished", outcome.polished) } else { String::new() };
        self.set_status(id, Status::Kept);
        self.log(id, format!("Kept {} in {folder}{polished}.", crate::format::count(count, "file")));
        self.detail_dirty.insert(id);
        self.persist(id);
    }

    fn fail(&mut self, id: ItemId, failure: DownloadFailure) {
        self.running.remove(&id);
        let retries = self.settings.auto_retries;
        let item = self.items.get_mut(&id).unwrap();
        item.transfer.bytes_per_second = None;
        item.transfer.seconds_left = None;
        if failure.is_transient() && item.attempts <= retries && !self.shutting_down {
            let wait = Duration::from_secs(5 * (1 << item.attempts.saturating_sub(1).min(4)));
            item.phase_detail = Some(format!("Retrying in {}s", wait.as_secs()));
            let sentence = failure.sentence();
            self.set_status(id, Status::Queued);
            self.log(id, format!("{sentence} Trying again in {}s.", wait.as_secs()));
            if let Some(detail) = &failure.detail {
                self.log(id, detail.clone());
            }
            self.not_before.insert(id, tokio::time::Instant::now() + wait);
            let timer = self.timer_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(wait).await;
                let _ = timer.send(id);
            });
            self.persist(id);
            return;
        }
        item.phase_detail = None;
        item.finished_at = Some(now_millis());
        let sentence = failure.sentence();
        let detail = failure.detail.clone();
        item.failure = Some(failure);
        self.set_status(id, Status::Failed);
        self.log(id, format!("Failed: {sentence}"));
        if let Some(detail) = detail {
            self.log(id, detail);
        }
        self.persist(id);
    }

    // MARK: Bookkeeping

    fn set_status(&mut self, id: ItemId, status: Status) {
        if let Some(item) = self.items.get_mut(&id)
            && item.status != status
        {
            item.status = status;
            self.counts_dirty = true;
        }
        self.mark(id);
    }

    fn mark(&mut self, id: ItemId) {
        self.dirty.insert(id);
    }

    fn log(&mut self, id: ItemId, line: String) {
        let log = self.logs.entry(id).or_default();
        log.push_back(line.clone());
        while log.len() > LOG_LINES_IN_MEMORY {
            log.pop_front();
        }
        self.detail_dirty.insert(id);
        self.store(vec![StoreOp::Log { id, line }]);
    }

    fn store(&self, ops: Vec<StoreOp>) {
        if let Some(writer) = &self.writer {
            writer.send(ops);
        }
    }

    fn persist(&mut self, id: ItemId) {
        self.persist_dirty.remove(&id);
        let Some(item) = self.items.get(&id) else { return };
        let position = self.order.iter().position(|i| *i == id).unwrap_or(self.order.len()) as i64;
        self.store(vec![StoreOp::Upsert { item: Box::new(item.clone()), position }]);
    }

    fn persist_progress(&mut self) {
        let ids: Vec<ItemId> = self.persist_dirty.drain().collect();
        let ops: Vec<StoreOp> = ids
            .into_iter()
            .filter_map(|id| {
                let item = self.items.get(&id)?;
                let position = self.order.iter().position(|i| *i == id)? as i64;
                Some(StoreOp::Upsert { item: Box::new(item.clone()), position })
            })
            .collect();
        self.store(ops);
        if self.order_dirty {
            self.store(vec![StoreOp::Order(self.order.clone())]);
        }
    }

    fn counts(&self) -> Counts {
        Counts::of(self.items.values().map(|i| i.status))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision,
            items: self.order.iter().filter_map(|id| self.items.get(id)).map(ItemView::of).collect(),
            counts: self.counts(),
            queue: QueueState { running: self.queue_running },
            tools: self.tool_statuses.clone(),
        }
    }

    fn detail(&mut self, id: ItemId) -> Option<ItemDetail> {
        let item = self.items.get(&id)?.clone();
        if !self.logs.contains_key(&id) {
            let lines = self.reader.as_ref().and_then(|r| r.load_log(id).ok()).unwrap_or_default();
            self.logs.insert(id, lines.into());
        }
        let engine = self.registry.get(item.engine);
        Some(ItemDetail {
            log: self.logs[&id].iter().cloned().collect(),
            capabilities: engine.map(|e| e.capabilities()).unwrap_or_default(),
            lane: engine.map(|e| e.lane()),
            item,
        })
    }

    /// Sends what changed since the last flush, as one batch per subscriber.
    fn flush(&mut self) {
        let watched: Vec<ItemId> = self.subs.iter().filter_map(|s| s.detail).collect();
        let detail_ids: HashSet<ItemId> =
            watched.into_iter().filter(|id| self.detail_dirty.contains(id) || self.dirty.contains(id)).collect();
        let nothing = self.dirty.is_empty()
            && self.removed.is_empty()
            && !self.order_dirty
            && !self.counts_dirty
            && !self.tools_dirty
            && self.notices.is_empty()
            && detail_ids.is_empty();
        if nothing && !self.subs.iter().any(|s| s.needs_snapshot) {
            return;
        }

        let mut changes = Vec::new();
        for id in self.removed.drain(..) {
            changes.push(Change::Remove { id });
        }
        let mut dirty: Vec<ItemId> = self.dirty.drain().collect();
        dirty.sort();
        for id in dirty {
            if let Some(item) = self.items.get(&id) {
                changes.push(Change::Upsert { item: ItemView::of(item) });
            }
        }
        if std::mem::take(&mut self.order_dirty) {
            changes.push(Change::Order { ids: self.order.clone() });
        }
        if std::mem::take(&mut self.counts_dirty) {
            changes.push(Change::Counts { counts: self.counts() });
            changes.push(Change::Queue { queue: QueueState { running: self.queue_running } });
        }
        if std::mem::take(&mut self.tools_dirty) {
            changes.push(Change::Tools { tools: self.tool_statuses.clone() });
        }
        for text in self.notices.drain(..) {
            changes.push(Change::Notice { text });
        }
        self.detail_dirty.clear();
        let details: HashMap<ItemId, ItemDetail> =
            detail_ids.into_iter().filter_map(|id| self.detail(id).map(|d| (id, d))).collect();
        if !changes.is_empty() {
            self.revision += 1;
        }

        let snapshot_needed = self.subs.iter().any(|s| s.needs_snapshot);
        let snapshot = snapshot_needed.then(|| self.snapshot());
        let revision = self.revision;
        self.subs.retain_mut(|sub| {
            if sub.needs_snapshot {
                return match sub.tx.try_send(Update::Snapshot(snapshot.clone().expect("snapshot"))) {
                    Ok(()) => {
                        sub.needs_snapshot = false;
                        true
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => true,
                    Err(mpsc::error::TrySendError::Closed(_)) => false,
                };
            }
            let mut mine = changes.clone();
            if let Some(detail) = sub.detail.and_then(|id| details.get(&id)) {
                mine.push(Change::Detail { detail: Box::new(detail.clone()) });
            }
            if mine.is_empty() {
                return !sub.tx.is_closed();
            }
            match sub.tx.try_send(Update::Batch(Batch { revision, changes: mine })) {
                Ok(()) => true,
                // Behind: drop the backlog idea entirely and send a fresh snapshot when there's room.
                Err(mpsc::error::TrySendError::Full(_)) => {
                    sub.needs_snapshot = true;
                    true
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }
        });
    }
}

fn clock_time() -> String {
    let secs = (now_millis() / 1000) as u64;
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02} UTC")
}

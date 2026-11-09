//! The seam every download goes through. yt-dlp, gallery-dl, the built-in HTTP downloader and
//! (later) librqbit are engines; the queue and the UIs never know which one runs an item.
//!
//! An engine runs one item as one async `run`. Everything the item does happens inside that run,
//! including seeding: a torrent job reports `Phase(Seeding)` once its files are on disk and keeps
//! running until its ratio limit or until it is stopped. The queue stops counting a seeding job
//! against its lane's download slots, but it still owns the job, so Stop Seeding can reach it.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::failure::DownloadFailure;
use crate::model::{ItemFile, ItemId, Options, Status, Swarm, Transfer};
use crate::process::StopSignal;
use crate::source::{DownloadSource, EngineId};
use crate::tools::{ToolId, ToolSnapshot};

pub mod demo;
pub mod gallerydl;
pub mod http;
pub mod torrent;
pub mod ytdlp;

/// What an engine can do. A UI shows a control only when the engine has the capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Capabilities(pub u32);

impl Capabilities {
    /// Picks a format (Best, Compatible, Audio only).
    pub const FORMATS: Capabilities = Capabilities(1 << 0);
    /// Follows a playlist link to every entry.
    pub const PLAYLISTS: Capabilities = Capabilities(1 << 1);
    /// Can present itself as a browser and send cookies.
    pub const SITE_ACCESS: Capabilities = Capabilities(1 << 2);
    /// Runs ffmpeg after the download.
    pub const POLISH: Capabilities = Capabilities(1 << 3);
    /// Picks up where it stopped after Pause.
    pub const RESUME: Capabilities = Capabilities(1 << 4);
    /// Keeps uploading after Kept.
    pub const SEEDING: Capabilities = Capabilities(1 << 5);
    /// Lets the person skip some of an item's files.
    pub const FILE_SELECTION: Capabilities = Capabilities(1 << 6);
    /// Reports peers, seeds, upload and ratio.
    pub const SWARM: Capabilities = Capabilities(1 << 7);
    /// Splits one file across several connections.
    pub const MULTI_CONNECTION: Capabilities = Capabilities(1 << 8);
    /// Verifies a checksum before keeping the file.
    pub const CHECKSUM: Capabilities = Capabilities(1 << 9);

    pub const fn union(self, other: Capabilities) -> Capabilities {
        Capabilities(self.0 | other.0)
    }

    pub const fn contains(self, other: Capabilities) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Capabilities {
    type Output = Capabilities;
    fn bitor(self, rhs: Capabilities) -> Capabilities {
        self.union(rhs)
    }
}

/// Items in one lane share its download slots; lanes run side by side, so a long torrent never
/// blocks a video and a big ISO never blocks a playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// Sites read by yt-dlp and gallery-dl. One at a time by default: sites throttle and block
    /// parallel scrapers, and the person usually wants the top item first.
    Web,
    /// Plain files through the built-in HTTP engine.
    Direct,
    Torrent,
    /// The demo engine (UI prototypes, load tests).
    Demo,
}

impl Lane {
    pub const ALL: [Lane; 4] = [Lane::Web, Lane::Direct, Lane::Torrent, Lane::Demo];

    pub fn default_limit(self) -> usize {
        match self {
            Lane::Web => 1,
            Lane::Direct => 3,
            Lane::Torrent => 3,
            Lane::Demo => 10_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Ready,
    /// A tool it runs isn't installed.
    MissingTool(ToolId),
    /// Dawnload has no implementation yet.
    NotBuilt,
}

impl Availability {
    pub fn failure(&self) -> Option<DownloadFailure> {
        match self {
            Availability::Ready => None,
            Availability::MissingTool(ToolId::Ffmpeg) => Some(DownloadFailure::tool_missing("ffmpeg")),
            Availability::MissingTool(tool) => Some(DownloadFailure::tool_missing(tool.name())),
            Availability::NotBuilt => Some(DownloadFailure::torrents_unavailable()),
        }
    }
}

/// Everything a job needs to run one item.
#[derive(Clone, Debug)]
pub struct JobRequest {
    pub item_id: ItemId,
    pub source: DownloadSource,
    pub options: Options,
    pub tools: ToolSnapshot,
    /// The files the item already knows: a previous run's partial file, or torrent selections.
    pub files: Vec<ItemFile>,
    /// What's in the folder already from an earlier run, so a resumed job can continue it.
    pub resuming: bool,
}

/// What a running job reports. Cheap to send; the core coalesces them before any UI sees them.
#[derive(Clone, Debug, PartialEq)]
pub enum JobEvent {
    Command(String),
    Log(String),
    Phase(Status, Option<String>),
    Transfer(Transfer),
    Title(String),
    Files(Vec<ItemFile>),
    Swarm(Swarm),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobOutcome {
    pub files: Vec<PathBuf>,
    pub polished: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobError {
    /// Stopped by Pause, Remove, Stop Seeding or quit. Partial files stay for Resume.
    Stopped,
    Failed(DownloadFailure),
}

impl From<DownloadFailure> for JobError {
    fn from(failure: DownloadFailure) -> Self {
        JobError::Failed(failure)
    }
}

pub type JobResult = Result<JobOutcome, JobError>;
pub type JobFuture = Pin<Box<dyn Future<Output = JobResult> + Send + 'static>>;

/// A job's line back to the core.
#[derive(Clone, Debug)]
pub struct JobContext {
    pub item_id: ItemId,
    pub run: u64,
    pub stop: StopSignal,
    sender: mpsc::UnboundedSender<(ItemId, u64, JobEvent)>,
}

impl JobContext {
    pub fn new(item_id: ItemId, run: u64, stop: StopSignal, sender: mpsc::UnboundedSender<(ItemId, u64, JobEvent)>) -> Self {
        JobContext { item_id, run, stop, sender }
    }

    pub fn emit(&self, event: JobEvent) {
        let _ = self.sender.send((self.item_id, self.run, event));
    }

    pub fn log(&self, line: impl Into<String>) {
        self.emit(JobEvent::Log(line.into()));
    }

    pub fn phase(&self, status: Status, detail: Option<&str>) {
        self.emit(JobEvent::Phase(status, detail.map(str::to_string)));
    }
}

pub trait DownloadEngine: Send + Sync {
    fn id(&self) -> EngineId;
    fn lane(&self) -> Lane;
    fn capabilities(&self) -> Capabilities;
    fn accepts(&self, source: &DownloadSource) -> bool;
    fn availability(&self, tools: &ToolSnapshot, options: &Options) -> Availability;
    /// Runs until the item is kept, fails or is stopped.
    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture;
}

/// The engines a core runs with.
#[derive(Clone)]
pub struct Registry {
    engines: Vec<Arc<dyn DownloadEngine>>,
}

impl Registry {
    pub fn standard() -> Registry {
        Registry {
            engines: vec![
                Arc::new(ytdlp::YtDlpEngine),
                Arc::new(gallerydl::GalleryDlEngine),
                Arc::new(http::HttpEngine::new()),
                Arc::new(torrent::TorrentEngine),
                Arc::new(demo::DemoEngine::default()),
            ],
        }
    }

    pub fn with(engines: Vec<Arc<dyn DownloadEngine>>) -> Registry {
        Registry { engines }
    }

    pub fn get(&self, id: EngineId) -> Option<&Arc<dyn DownloadEngine>> {
        self.engines.iter().find(|e| e.id() == id)
    }
}

//! What UIs see. The core owns all state; a UI (the app, the CLI, a future local remote-control
//! API) subscribes and gets one [`Snapshot`], then [`Batch`]es of changes at a bounded rate
//! (4 per second by default) no matter how fast tools print progress. A subscriber that falls
//! behind is never queued up: it gets a fresh snapshot instead. Everything here is `serde`, so the
//! same stream can go over a socket as JSON.
//!
//! [`Mirror`] is the client half: apply updates to it and ask it for rows. Every UI uses it, so
//! grouping, ordering and queue positions are the same everywhere.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::{Capabilities, Lane};
use crate::failure::DownloadFailure;
use crate::model::{Item, ItemId, Options, Status, Swarm, Transfer};
use crate::source::{EngineId, SourceKind};
use crate::tools::ToolStatus;

/// One row's worth of an item: everything a list needs, nothing heavy (no log, no file list).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemView {
    pub id: ItemId,
    pub title: String,
    /// The link, magnet or path, as pasted.
    pub source: String,
    pub source_kind: SourceKind,
    /// Host, magnet name or file name.
    pub host: String,
    pub engine: EngineId,
    pub status: Status,
    pub phase_detail: Option<String>,
    pub transfer: Transfer,
    pub swarm: Option<Swarm>,
    pub file_count: usize,
    pub kept_bytes: Option<u64>,
    /// How full the D is: 0…1.
    pub fill: f64,
    pub failure: Option<DownloadFailure>,
    pub options_summary: String,
    /// The folder's name: "Dawnload".
    pub folder_name: String,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    /// The first file it kept (or is writing), for Open and Show in Folder.
    #[serde(default)]
    pub primary_file: Option<PathBuf>,
    /// The folder it downloads into.
    #[serde(default)]
    pub folder: PathBuf,
}

impl ItemView {
    pub fn of(item: &Item) -> ItemView {
        ItemView {
            id: item.id,
            title: item.display_title(),
            source: item.source.as_text(),
            source_kind: item.source.kind(),
            host: item.source.short_name(),
            engine: item.engine,
            status: item.status,
            phase_detail: item.phase_detail.clone(),
            transfer: item.transfer.clone(),
            swarm: item.swarm.clone(),
            file_count: item.kept_files().count(),
            kept_bytes: item.kept_bytes(),
            fill: item.fill(),
            failure: item.failure.clone(),
            options_summary: item.options.summary(item.engine),
            folder_name: item.options.destination.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            created_at: item.created_at,
            finished_at: item.finished_at,
            primary_file: item.kept_files().next().or_else(|| item.files.first()).map(|f| PathBuf::from(&f.path)),
            folder: item.options.destination.clone(),
        }
    }
}

/// Everything about one item, for an inspector. Sent only to subscribers watching that item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDetail {
    pub item: Item,
    pub log: Vec<String>,
    pub capabilities: Capabilities,
    pub lane: Option<Lane>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub all: usize,
    pub active: usize,
    pub queued: usize,
    pub paused: usize,
    pub kept: usize,
    pub failed: usize,
    pub seeding: usize,
}

impl Counts {
    pub fn of(statuses: impl Iterator<Item = Status>) -> Counts {
        let mut c = Counts::default();
        for s in statuses {
            c.all += 1;
            match s {
                Status::Pulling | Status::Polishing => c.active += 1,
                Status::Queued => c.queued += 1,
                Status::Paused => c.paused += 1,
                Status::Kept => c.kept += 1,
                Status::Failed => c.failed += 1,
                Status::Seeding => c.seeding += 1,
            }
        }
        c
    }

    pub fn for_filter(&self, filter: ListFilter) -> usize {
        match filter {
            ListFilter::All => self.all,
            ListFilter::Active => self.active,
            ListFilter::Waiting => self.queued + self.paused,
            ListFilter::Kept => self.kept,
            ListFilter::Failed => self.failed,
            ListFilter::Seeding => self.seeding,
        }
    }
}

/// The queue control's state: Start Queue, Pause All or Resume All.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueState {
    pub running: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: u64,
    /// In queue order.
    pub items: Vec<ItemView>,
    pub counts: Counts,
    pub queue: QueueState,
    pub tools: Vec<ToolStatus>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Upsert is the common case; boxing it would cost more.
pub enum Change {
    Upsert {
        item: ItemView,
    },
    Remove {
        id: ItemId,
    },
    /// The whole queue order (ids), after an add, a move or a removal.
    Order {
        ids: Vec<ItemId>,
    },
    Counts {
        counts: Counts,
    },
    Queue {
        queue: QueueState,
    },
    Detail {
        detail: Box<ItemDetail>,
    },
    Tools {
        tools: Vec<ToolStatus>,
    },
    /// A sentence for a toast: "yt-dlp is up to date (2026.10.26)."
    Notice {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub revision: u64,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Update {
    Snapshot(Snapshot),
    Batch(Batch),
}

/// Everything a UI can ask the core to do to items. Serializable for the remote API.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    StartQueue,
    /// Stops every download (not seeding) and the queue.
    PauseAll,
    ResumeAll,
    /// Pauses a running item, or holds back a waiting one.
    Pause {
        id: ItemId,
    },
    /// Resume a paused item, retry a failed one, or start a waiting one now: it goes to the head
    /// of the line and the queue runs.
    Resume {
        id: ItemId,
    },
    Remove {
        ids: Vec<ItemId>,
    },
    StopSeeding {
        id: ItemId,
    },
    /// Moves a waiting item to `index` among waiting items (0 is up next).
    Move {
        id: ItemId,
        index: usize,
    },
    SetOptions {
        id: ItemId,
        options: Options,
    },
    /// Removes every item in this state (Kept, Failed).
    Clear {
        status: Status,
    },
}

// MARK: - Grouping (shared by every UI)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListFilter {
    All,
    Active,
    Waiting,
    Kept,
    Failed,
    Seeding,
}

impl ListFilter {
    pub const ALL: [ListFilter; 6] =
        [ListFilter::All, ListFilter::Active, ListFilter::Waiting, ListFilter::Kept, ListFilter::Failed, ListFilter::Seeding];

    pub fn title(self) -> &'static str {
        match self {
            ListFilter::All => "All",
            ListFilter::Active => "Active",
            ListFilter::Waiting => "Up next",
            ListFilter::Kept => "Kept",
            ListFilter::Failed => "Failed",
            ListFilter::Seeding => "Seeding",
        }
    }

    pub fn includes(self, status: Status) -> bool {
        match self {
            ListFilter::All => true,
            ListFilter::Active => status.is_active(),
            ListFilter::Waiting => matches!(status, Status::Queued | Status::Paused),
            ListFilter::Kept => status == Status::Kept,
            ListFilter::Failed => status == Status::Failed,
            ListFilter::Seeding => status == Status::Seeding,
        }
    }

    pub fn empty_line(self) -> &'static str {
        match self {
            ListFilter::All => "Paste a link to start.",
            ListFilter::Active => "Nothing running.",
            ListFilter::Waiting => "Nothing waiting.",
            ListFilter::Kept => "Nothing kept yet.",
            ListFilter::Failed => "Nothing failed.",
            ListFilter::Seeding => "Nothing seeding.",
        }
    }
}

/// Groups of the list, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Now,
    UpNext,
    Paused,
    Seeding,
    Finished,
}

impl Section {
    pub const ALL: [Section; 5] = [Section::Now, Section::UpNext, Section::Paused, Section::Seeding, Section::Finished];

    pub fn title(self) -> &'static str {
        match self {
            Section::Now => "Now",
            Section::UpNext => "Up next",
            Section::Paused => "Paused",
            Section::Seeding => "Seeding",
            Section::Finished => "Finished",
        }
    }

    pub fn of(status: Status) -> Section {
        match status {
            Status::Pulling | Status::Polishing => Section::Now,
            Status::Queued => Section::UpNext,
            Status::Paused => Section::Paused,
            Status::Seeding => Section::Seeding,
            Status::Kept | Status::Failed => Section::Finished,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Header {
        section: Section,
        count: usize,
    },
    /// `queue_position` is 1-based among waiting items.
    Item {
        id: ItemId,
        queue_position: Option<usize>,
    },
}

/// What applying an update changed, so a UI touches only that.
#[derive(Clone, Debug, Default)]
pub struct Applied {
    /// Rows may have moved, appeared or gone: rebuild the row list.
    pub structure: bool,
    /// Items whose contents changed (rebuild only these rows if `structure` is false).
    pub changed: Vec<ItemId>,
    pub detail: Option<ItemDetail>,
    pub notices: Vec<String>,
    pub tools: bool,
    /// Items that just finished (Kept, Failed, or Kept and now seeding) in this batch, with
    /// their new status: what a UI notifies about. Never filled by a snapshot, so a relaunch
    /// doesn't announce old news.
    pub finished: Vec<(ItemId, Status)>,
}

/// A UI's copy of the core's state, kept current by applying updates.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    pub revision: u64,
    pub items: HashMap<ItemId, ItemView>,
    pub order: Vec<ItemId>,
    pub counts: Counts,
    pub queue: QueueState,
    pub tools: Vec<ToolStatus>,
}

impl Mirror {
    pub fn apply(&mut self, update: Update) -> Applied {
        let mut applied = Applied::default();
        match update {
            Update::Snapshot(snapshot) => {
                self.revision = snapshot.revision;
                self.order = snapshot.items.iter().map(|i| i.id).collect();
                self.items = snapshot.items.into_iter().map(|i| (i.id, i)).collect();
                self.counts = snapshot.counts;
                self.queue = snapshot.queue;
                self.tools = snapshot.tools;
                applied.structure = true;
                applied.tools = true;
            }
            Update::Batch(batch) => {
                self.revision = batch.revision;
                for change in batch.changes {
                    match change {
                        Change::Upsert { item } => {
                            let old = self.items.get(&item.id);
                            match old {
                                Some(old) if old.status == item.status && old.finished_at == item.finished_at => {}
                                _ => applied.structure = true,
                            }
                            if item.status.is_finished() && !old.is_some_and(|o| o.status.is_finished()) {
                                applied.finished.push((item.id, item.status));
                            }
                            applied.changed.push(item.id);
                            self.items.insert(item.id, item);
                        }
                        Change::Remove { id } => {
                            self.items.remove(&id);
                            self.order.retain(|i| *i != id);
                            applied.structure = true;
                        }
                        Change::Order { ids } => {
                            self.order = ids;
                            applied.structure = true;
                        }
                        Change::Counts { counts } => self.counts = counts,
                        Change::Queue { queue } => self.queue = queue,
                        Change::Detail { detail } => applied.detail = Some(*detail),
                        Change::Tools { tools } => {
                            self.tools = tools;
                            applied.tools = true;
                        }
                        Change::Notice { text } => applied.notices.push(text),
                    }
                }
            }
        }
        applied
    }

    pub fn item(&self, id: ItemId) -> Option<&ItemView> {
        self.items.get(&id)
    }

    /// The list for a filter: sections in order, each with its header row. Waiting items keep
    /// queue order; finished ones are newest first.
    pub fn rows(&self, filter: ListFilter) -> Vec<Row> {
        let mut groups: HashMap<Section, Vec<&ItemView>> = HashMap::new();
        for id in &self.order {
            let Some(item) = self.items.get(id) else { continue };
            if filter.includes(item.status) {
                groups.entry(Section::of(item.status)).or_default().push(item);
            }
        }
        let mut rows = Vec::new();
        for section in Section::ALL {
            let Some(mut items) = groups.remove(&section) else { continue };
            if section == Section::Finished {
                items.sort_by_key(|i| std::cmp::Reverse(i.finished_at.unwrap_or(i.created_at)));
            }
            rows.push(Row::Header { section, count: items.len() });
            for (index, item) in items.iter().enumerate() {
                let queue_position = (section == Section::UpNext).then_some(index + 1);
                rows.push(Row::Item { id: item.id, queue_position });
            }
        }
        rows
    }
}

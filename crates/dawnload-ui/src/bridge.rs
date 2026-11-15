//! The bridge: a task on the core's runtime between the window and the core. It keeps an
//! `api::Mirror`, turns it into what the window shows (see `view`), diffs rows against what's on
//! screen, and hands the UI thread only what changed, at most as often as the core sends
//! batches (4 per second). It also runs what the window asks for that takes time: tool
//! installs, file pickers, adding links.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dawnload_core::api::{Action, ItemDetail, ListFilter, Mirror};
use dawnload_core::core::{Appearance, CoreHandle, Motion, Settings};
use dawnload_core::explain::Tone;
use dawnload_core::instance::Handoff;
use dawnload_core::intake;
use dawnload_core::model::{Cookies, ItemId, Status};
use dawnload_core::source::{DownloadSource, EngineId};
use dawnload_core::tools::{JsRuntime, ToolId};
use tokio::sync::mpsc;

use crate::platform;
use crate::view::{self, Job};
use crate::{Chips, FileRow, FilterTab, InspectorData, Prefs, RowData, ToolRow};

/// What the window (or the OS, or a second launch) asks the bridge.
pub enum UiMessage {
    Paste(String),
    Submit {
        text: String,
        start: bool,
    },
    Filter(usize),
    Select(Option<ItemId>),
    MoveSelection(i32),
    Item {
        id: ItemId,
        action: String,
    },
    Reorder {
        id: ItemId,
        delta: i32,
    },
    QueueAction,
    Command(String),
    SetOption(String, i32),
    ToolAction {
        tool: String,
        action: String,
    },
    InspectorToggled(bool),
    /// A file dropped on the window.
    Dropped(PathBuf),
    Handoff(Handoff),
    // From the bridge's own tasks.
    ToolJob(ToolId, Job),
    ToolsFinished {
        notice: Option<String>,
    },
    Picked {
        what: Pick,
        path: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Pick {
    Folder,
    Cookies,
    Tool(ToolId),
}

/// The tray's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrayState {
    pub busy: bool,
    pub queue_title: String,
    pub queue_enabled: bool,
    pub tooltip: String,
}

/// What the bridge hands the UI thread. Everything is plain data (Send).
#[derive(Default)]
pub struct Frame {
    /// The list's new length (rows are added or dropped at the end), then `patch` rewrites rows.
    /// The list is never reset, so the ListView keeps its scroll position and only redraws
    /// rows that changed and are on screen.
    pub resize: Option<usize>,
    pub patch: Vec<(usize, RowData)>,
    pub filters: Option<Vec<FilterTab>>,
    pub queue_label: Option<String>,
    pub empty_line: Option<String>,
    pub inspector: Option<(InspectorData, Vec<FileRow>)>,
    pub selected: Option<i32>,
    pub detection: Option<(String, i32, i32, bool)>,
    pub paste_text: Option<String>,
    pub chips: Option<Chips>,
    pub prefs: Option<Prefs>,
    pub tools_pill: Option<(String, bool)>,
    pub tools: Option<Vec<ToolRow>>,
    pub missing_tool: Option<String>,
    pub tools_busy: Option<bool>,
    pub tools_note: Option<String>,
    pub js: Option<(String, bool)>,
    pub notice: Option<String>,
    pub copy: Option<String>,
    pub page: Option<i32>,
    /// Appearance (0 system, 1 dark, 2 light) and whether to reduce motion.
    pub look: Option<(i32, bool)>,
    pub close_to_tray: Option<bool>,
    pub show_inspector: Option<bool>,
    pub tray: Option<TrayState>,
    pub show_window: bool,
    pub notify: Vec<(String, String)>,
}

pub struct Bridge {
    pub core: CoreHandle,
    pub real: bool,
    pub tray_available: bool,
    /// Messages the bridge sends itself (task results).
    pub own: mpsc::UnboundedSender<UiMessage>,
}

struct State {
    mirror: Mirror,
    filter: ListFilter,
    selected: Option<ItemId>,
    shown: Vec<RowData>,
    item_rows: Vec<ItemId>,
    detail: Option<ItemDetail>,
    settings: Settings,
    paste: String,
    picked: Option<EngineId>,
    jobs: BTreeMap<ToolId, Job>,
    tools_busy: bool,
    checking_updates: bool,
    js: Option<JsRuntime>,
    tray: TrayState,
}

impl Bridge {
    pub async fn run(self, mut ui: mpsc::UnboundedReceiver<UiMessage>, apply: impl Fn(Frame)) {
        let core = self.core.clone();
        let Some(mut sub) = core.subscribe().await else { return };
        let settings = core.settings().await.unwrap_or_default();
        let mut state = State {
            mirror: Mirror::default(),
            filter: ListFilter::All,
            selected: None,
            shown: Vec::new(),
            item_rows: Vec::new(),
            detail: None,
            settings,
            paste: String::new(),
            picked: None,
            jobs: BTreeMap::new(),
            tools_busy: false,
            checking_updates: false,
            js: None,
            tray: TrayState::default(),
        };
        let mut first = Frame {
            chips: Some(view::chips(&state.settings, None)),
            prefs: Some(view::prefs(&state.settings, self.tray_available)),
            look: Some(look(&state.settings)),
            close_to_tray: Some(state.settings.app.close_to_tray && self.tray_available),
            show_inspector: Some(state.settings.app.show_inspector),
            ..Frame::default()
        };
        if self.real && !state.settings.app.first_run_done {
            first.page = Some(2);
        }
        if !self.real {
            first.tools_pill = Some(("Demo engine".into(), true));
        }
        apply(first);

        loop {
            let mut frame = Frame::default();
            tokio::select! {
                update = sub.recv() => {
                    let Some(update) = update else { return };
                    let applied = state.mirror.apply(update);
                    if let Some(detail) = applied.detail
                        && Some(detail.item.id) == state.selected
                    {
                        state.detail = Some(detail);
                    }
                    if applied.tools && self.real {
                        state.js = core.tool_snapshot().await.and_then(|t| t.js_runtime);
                        if state.checking_updates {
                            state.checking_updates = false;
                            frame.tools_note = Some(update_summary(&state));
                        }
                        self.tools_frame(&state, &mut frame);
                    }
                    // The demo engine finishes hundreds of items a minute: no notifications for it.
                    for (id, status) in applied.finished.into_iter().filter(|_| self.real) {
                        if let Some(note) = view::notification(&state.settings, &state.mirror, id, status) {
                            frame.notify.push(note);
                        }
                    }
                    if let Some(text) = applied.notices.last() {
                        frame.notice = Some(text.clone());
                    }
                    refresh(&mut state, &mut frame);
                }
                message = ui.recv() => {
                    let Some(message) = message else { return };
                    self.handle(message, &mut state, &mut frame, &mut sub).await;
                }
            }
            let tray = TrayState {
                busy: state.mirror.counts.active + state.mirror.counts.queued > 0,
                queue_title: match view::queue_label(&state.mirror) {
                    "" => "Pause All".into(),
                    label => label.into(),
                },
                queue_enabled: !view::queue_label(&state.mirror).is_empty(),
                tooltip: view::tray_tooltip(&state.mirror),
            };
            if tray != state.tray {
                state.tray = tray.clone();
                frame.tray = Some(tray);
            }
            apply(frame);
        }
    }

    fn tools_frame(&self, state: &State, frame: &mut Frame) {
        let statuses = &state.mirror.tools;
        frame.tools_pill = Some(view::tools_pill(statuses));
        frame.tools = Some(view::tool_rows(statuses, &state.jobs, &state.settings));
        frame.missing_tool = Some(view::missing_tool(statuses).map(|t| t.name().to_string()).unwrap_or_default());
        frame.js = Some(view::js_line(state.js.as_ref(), statuses));
        frame.tools_busy = Some(state.tools_busy);
    }

    async fn handle(
        &self,
        message: UiMessage,
        state: &mut State,
        frame: &mut Frame,
        sub: &mut dawnload_core::core::Subscription,
    ) {
        let core = &self.core;
        match message {
            UiMessage::Paste(text) => {
                state.paste = text.clone();
                self.explain(state, frame, &text).await;
            }
            UiMessage::Submit { text, start } => self.submit(state, frame, sub, &text, start).await,
            UiMessage::Filter(index) => {
                state.filter = view::FILTERS.get(index).copied().unwrap_or(ListFilter::All);
                refresh(state, frame);
            }
            UiMessage::Select(id) => {
                state.selected = id;
                state.detail = None;
                sub.watch(id);
                refresh(state, frame);
            }
            UiMessage::MoveSelection(delta) => {
                let current = state.selected.and_then(|id| state.item_rows.iter().position(|i| *i == id));
                let next = match current {
                    Some(i) => (i as i64 + delta as i64).clamp(0, state.item_rows.len() as i64 - 1) as usize,
                    None if !state.item_rows.is_empty() => 0,
                    None => return,
                };
                let id = state.item_rows[next];
                state.selected = Some(id);
                state.detail = None;
                sub.watch(Some(id));
                frame.selected = Some(id.0 as i32);
                refresh(state, frame);
            }
            UiMessage::Item { id, action } => self.item_action(state, frame, sub, id, &action).await,
            UiMessage::Reorder { id, delta } => {
                let waiting: Vec<ItemId> = state
                    .mirror
                    .order
                    .iter()
                    .copied()
                    .filter(|i| state.mirror.item(*i).is_some_and(|v| v.status == Status::Queued))
                    .collect();
                if let Some(pos) = waiting.iter().position(|i| *i == id) {
                    let index = (pos as i64 + delta as i64).clamp(0, waiting.len() as i64 - 1) as usize;
                    let _ = core.act(Action::Move { id, index }).await;
                }
            }
            UiMessage::QueueAction => {
                let action = match view::queue_label(&state.mirror) {
                    "Pause All" => Action::PauseAll,
                    "Start Queue" => Action::StartQueue,
                    _ => Action::ResumeAll,
                };
                let _ = core.act(action).await;
            }
            UiMessage::Command(command) => self.command(state, frame, &command).await,
            UiMessage::SetOption(key, value) => {
                if view::set_option(&mut state.settings, &key, value) {
                    core.set_settings(state.settings.clone()).await;
                    frame.chips = Some(view::chips(&state.settings, state.picked));
                    frame.prefs = Some(view::prefs(&state.settings, self.tray_available));
                    if matches!(key.as_str(), "appearance" | "motion") {
                        frame.look = Some(look(&state.settings));
                    }
                    if key == "close-to-tray" {
                        frame.close_to_tray = Some(state.settings.app.close_to_tray && self.tray_available);
                    }
                    if matches!(key.as_str(), "engine" | "format" | "cookies") && !state.paste.is_empty() {
                        let text = state.paste.clone();
                        self.explain(state, frame, &text).await;
                    }
                }
            }
            UiMessage::ToolAction { tool, action } => {
                let Some(tool) = ToolId::parse(&tool) else { return };
                match action.as_str() {
                    "install" => self.install(state, frame, vec![tool]).await,
                    "choose" => self.pick(Pick::Tool(tool), None),
                    "reset" => {
                        state.settings.tool_paths.remove(&tool);
                        core.set_settings(state.settings.clone()).await;
                        state.jobs.remove(&tool);
                    }
                    "reveal" => {
                        if let Some(path) = state.mirror.tools.iter().find(|s| s.tool == tool).and_then(|s| s.path.clone()) {
                            platform::reveal(&path);
                        }
                    }
                    _ => {}
                }
                self.tools_frame(state, frame);
            }
            UiMessage::InspectorToggled(on) => {
                state.settings.app.show_inspector = on;
                core.set_settings(state.settings.clone()).await;
            }
            UiMessage::Dropped(path) => {
                let links = intake::links_from_file(&path);
                if links.is_empty() {
                    frame.detection =
                        Some((format!("{} isn't a link, a link file or a .torrent.", view::folder_name(&path)), 1, 0, true));
                    return;
                }
                let mut text = state.paste.trim().to_string();
                for link in links {
                    if !text.split_whitespace().any(|w| w == link) {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(&link);
                    }
                }
                frame.paste_text = Some(text.clone());
                frame.page = Some(0);
                state.paste = text.clone();
                self.explain(state, frame, &text).await;
            }
            UiMessage::Handoff(handoff) => {
                if handoff.show || handoff.links.is_empty() {
                    frame.show_window = true;
                }
                let text = handoff.links.join(" ");
                if !text.trim().is_empty() {
                    frame.show_window = true;
                    self.submit(state, frame, sub, &text, true).await;
                }
            }
            UiMessage::ToolJob(tool, job) => {
                state.jobs.insert(tool, job);
                self.tools_frame(state, frame);
            }
            UiMessage::ToolsFinished { notice } => {
                state.tools_busy = false;
                if let Some(text) = notice {
                    frame.notice = Some(text);
                }
                core.refresh_tools(false);
                self.tools_frame(state, frame);
            }
            UiMessage::Picked { what, path } => {
                let Some(path) = path else { return };
                match what {
                    Pick::Folder => {
                        state.settings.defaults.destination = path;
                        frame.notice = Some(format!("New downloads go to {}.", state.settings.defaults.destination.display()));
                    }
                    Pick::Cookies => {
                        state.settings.cookies_file = Some(path.clone());
                        state.settings.defaults.cookies = Cookies::File(path);
                    }
                    Pick::Tool(tool) => {
                        state.settings.tool_paths.insert(tool, path);
                        state.jobs.remove(&tool);
                    }
                }
                core.set_settings(state.settings.clone()).await;
                frame.chips = Some(view::chips(&state.settings, state.picked));
                frame.prefs = Some(view::prefs(&state.settings, self.tray_available));
            }
        }
    }

    async fn explain(&self, state: &mut State, frame: &mut Frame, text: &str) {
        let links = intake::links_in_text(text);
        let explanation = match links.as_slice() {
            [] => None,
            [one] => self.core.explain(one, Some(state.settings.defaults.clone())).await,
            [first, ..] => self.core.explain(first, Some(state.settings.defaults.clone())).await.map(|mut e| {
                e.text = format!("{} links · the first: {}", links.len(), e.text);
                e
            }),
        };
        state.picked = explanation.as_ref().and_then(|e| e.engine);
        frame.chips = Some(view::chips(&state.settings, state.picked));
        frame.detection = Some(match explanation {
            Some(e) => {
                let kind = match (&e.source, e.engine) {
                    (Some(DownloadSource::Magnet(_)), _) => 2,
                    (Some(DownloadSource::TorrentFile { .. }), _) | (Some(_), Some(EngineId::Direct)) => 3,
                    (Some(_), _) => 1,
                    (None, _) => 0,
                };
                (e.text, tone(e.tone), kind, e.can_submit)
            }
            None => (String::new(), 0, 0, true),
        });
    }

    async fn submit(
        &self,
        state: &mut State,
        frame: &mut Frame,
        sub: &mut dawnload_core::core::Subscription,
        text: &str,
        start: bool,
    ) {
        let links = intake::links_in_text(text);
        if links.is_empty() {
            return;
        }
        let mut added = Vec::new();
        let mut failures = Vec::new();
        for link in &links {
            match self.core.add(link, Some(state.settings.defaults.clone()), start).await {
                Ok(id) => added.push(id),
                Err(failure) => failures.push(failure.sentence()),
            }
        }
        if let Some(id) = added.last().copied() {
            state.selected = Some(id);
            state.detail = None;
            frame.selected = Some(id.0 as i32);
            sub.watch(Some(id));
        }
        match (added.len(), failures.as_slice()) {
            (_, []) => {
                frame.paste_text = Some(String::new());
                state.paste.clear();
                state.picked = None;
                frame.chips = Some(view::chips(&state.settings, None));
                let words = if start { "" } else { " · added to the queue" };
                frame.detection = Some((
                    if added.len() > 1 {
                        format!("{} links added{words}.", added.len())
                    } else if !start {
                        "Added to the queue.".into()
                    } else {
                        String::new()
                    },
                    0,
                    0,
                    true,
                ));
            }
            (0, [only]) => frame.detection = Some((only.clone(), 2, 0, true)),
            (n, [first, ..]) => {
                frame.detection = Some((format!("{n} added; {} couldn't be: {first}", failures.len()), 2, 0, true));
            }
        }
    }

    async fn item_action(
        &self,
        state: &mut State,
        frame: &mut Frame,
        sub: &mut dawnload_core::core::Subscription,
        id: ItemId,
        action: &str,
    ) {
        let core = &self.core;
        let Some(item) = state.mirror.item(id).cloned() else { return };
        match action {
            "primary" => match item.status {
                Status::Pulling | Status::Polishing => drop(core.act(Action::Pause { id }).await),
                Status::Queued | Status::Paused | Status::Failed => drop(core.act(Action::Resume { id }).await),
                Status::Seeding => drop(core.act(Action::StopSeeding { id }).await),
                Status::Kept => match &item.primary_file {
                    Some(file) if file.exists() => platform::open_path(file),
                    _ => platform::reveal(&item.folder),
                },
            },
            "pause" => drop(core.act(Action::Pause { id }).await),
            "resume" => drop(core.act(Action::Resume { id }).await),
            "stop-seeding" => drop(core.act(Action::StopSeeding { id }).await),
            "remove" => {
                let _ = core.act(Action::Remove { ids: vec![id] }).await;
                if state.selected == Some(id) {
                    // Select the next row, so Delete can be pressed again.
                    let next = state.item_rows.iter().position(|i| *i == id).and_then(|p| {
                        state.item_rows.get(p + 1).or_else(|| p.checked_sub(1).and_then(|q| state.item_rows.get(q))).copied()
                    });
                    state.selected = next;
                    state.detail = None;
                    sub.watch(next);
                    frame.selected = Some(next.map(|i| i.0 as i32).unwrap_or(-1));
                }
            }
            "open" => {
                if let Some(file) = &item.primary_file {
                    platform::open_path(file);
                }
            }
            "reveal" => match &item.primary_file {
                Some(file) if item.status.has_files() || file.exists() => platform::reveal(file),
                _ => platform::reveal(&item.folder),
            },
            "copy-link" => {
                frame.copy = Some(item.source.clone());
                frame.notice = Some("Link copied.".into());
            }
            "copy-log" => {
                if let Some(detail) = core.detail(id).await {
                    frame.copy = Some(detail.log.join("\n"));
                    frame.notice = Some("Log copied.".into());
                }
            }
            "top" => drop(core.act(Action::Move { id, index: 0 }).await),
            "bottom" => drop(core.act(Action::Move { id, index: usize::MAX }).await),
            _ => {}
        }
    }

    async fn command(&self, state: &mut State, frame: &mut Frame, command: &str) {
        let core = &self.core;
        match command {
            "start-queue" => drop(core.act(Action::StartQueue).await),
            "pause-all" => drop(core.act(Action::PauseAll).await),
            "clear-kept" => drop(core.act(Action::Clear { status: Status::Kept }).await),
            "clear-failed" => drop(core.act(Action::Clear { status: Status::Failed }).await),
            "remove-waiting" => {
                let ids: Vec<ItemId> = state
                    .mirror
                    .items
                    .values()
                    .filter(|i| matches!(i.status, Status::Queued | Status::Paused))
                    .map(|i| i.id)
                    .collect();
                let _ = core.act(Action::Remove { ids }).await;
            }
            "choose-folder" => self.pick(Pick::Folder, Some(state.settings.defaults.destination.clone())),
            "show-folder" => {
                let folder = state.settings.defaults.destination.clone();
                let _ = std::fs::create_dir_all(&folder);
                platform::open_path(&folder);
            }
            "choose-cookies" => {
                self.pick(Pick::Cookies, state.settings.cookies_file.as_ref().and_then(|p| p.parent().map(PathBuf::from)))
            }
            "remove-cookies" => {
                state.settings.cookies_file = None;
                state.settings.defaults.cookies = Cookies::None;
                core.set_settings(state.settings.clone()).await;
                frame.chips = Some(view::chips(&state.settings, state.picked));
                frame.prefs = Some(view::prefs(&state.settings, self.tray_available));
            }
            "check-tools" => {
                if self.real {
                    state.jobs.retain(|_, j| matches!(j, Job::Installing { .. }));
                    core.refresh_tools(false);
                    frame.notice = Some("Checking tools…".into());
                }
            }
            "check-updates" => {
                if self.real {
                    state.checking_updates = true;
                    core.refresh_tools(true);
                    frame.tools_note = Some("Checking for updates…".into());
                }
            }
            "update-tools" => self.update_all(state, frame).await,
            "install-missing" => {
                let missing = view::missing_installable(&state.mirror.tools);
                if !missing.is_empty() {
                    self.install(state, frame, missing).await;
                }
            }
            "first-run-done" => {
                state.settings.app.first_run_done = true;
                core.set_settings(state.settings.clone()).await;
                frame.page = Some(0);
            }
            "open-repo" => platform::open_url(platform::REPO_URL),
            "open-licences" => platform::open_url(platform::LICENCES_URL),
            other => {
                if let Some(path) = other.strip_prefix("reveal-file:") {
                    platform::reveal(std::path::Path::new(path));
                } else if let Some(path) = other.strip_prefix("open-file:") {
                    platform::open_path(std::path::Path::new(path));
                }
            }
        }
    }

    /// Installs tools one after another in a task; progress and the result come back as
    /// `ToolJob` messages.
    async fn install(&self, state: &mut State, frame: &mut Frame, tools: Vec<ToolId>) {
        if state.tools_busy || !self.real {
            return;
        }
        let Some(manager) = self.core.tool_manager().await else { return };
        state.tools_busy = true;
        for tool in &tools {
            state.jobs.insert(*tool, Job::Installing { done: 0, total: None });
        }
        self.tools_frame(state, frame);
        let own = self.own.clone();
        tokio::spawn(async move {
            let mut installed = Vec::new();
            for tool in tools {
                let mut last = Instant::now() - Duration::from_secs(1);
                let tx = own.clone();
                let mut progress = move |t: ToolId, done: u64, total: Option<u64>| {
                    if last.elapsed() >= Duration::from_millis(200) || total.is_some_and(|t| done >= t) {
                        last = Instant::now();
                        let _ = tx.send(UiMessage::ToolJob(
                            if t == ToolId::Ffprobe { ToolId::Ffmpeg } else { t },
                            Job::Installing { done, total },
                        ));
                    }
                };
                let job = match manager.install(tool, &mut progress).await {
                    Ok(version) => {
                        installed.push(format!("{} {version}", tool.name()));
                        Job::Done(format!("Installed {version}, checked against the publisher's checksum."))
                    }
                    Err(e) => Job::Failed(e.to_string()),
                };
                let _ = own.send(UiMessage::ToolJob(tool, job));
            }
            let notice = (!installed.is_empty()).then(|| format!("Installed {}.", installed.join(", ")));
            let _ = own.send(UiMessage::ToolsFinished { notice });
        });
    }

    async fn update_all(&self, state: &mut State, frame: &mut Frame) {
        if state.tools_busy || !self.real {
            return;
        }
        let Some(manager) = self.core.tool_manager().await else { return };
        state.tools_busy = true;
        frame.tools_note = Some("Updating Dawnload's tools…".into());
        self.tools_frame(state, frame);
        let own = self.own.clone();
        tokio::spawn(async move {
            let tx = own.clone();
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut progress = move |t: ToolId, done: u64, total: Option<u64>| {
                if last.elapsed() >= Duration::from_millis(200) || total.is_some_and(|t| done >= t) {
                    last = Instant::now();
                    let _ = tx.send(UiMessage::ToolJob(
                        if t == ToolId::Ffprobe { ToolId::Ffmpeg } else { t },
                        Job::Installing { done, total },
                    ));
                }
            };
            let results = manager.update_managed(&mut progress).await;
            let mut updated = Vec::new();
            for (tool, result) in results {
                let job = match result {
                    Ok(Some(version)) => {
                        updated.push(format!("{} {version}", tool.name()));
                        Job::Done(format!("Updated to {version}."))
                    }
                    Ok(None) => Job::Done("Up to date.".into()),
                    Err(e) => Job::Failed(e.to_string()),
                };
                let _ = own.send(UiMessage::ToolJob(tool, job));
            }
            let notice = Some(if updated.is_empty() {
                "Dawnload's tools are up to date.".to_string()
            } else {
                format!("Updated {}.", updated.join(", "))
            });
            let _ = own.send(UiMessage::ToolsFinished { notice });
        });
    }

    /// Opens a native picker without blocking anything; the answer comes back as `Picked`.
    fn pick(&self, what: Pick, start_in: Option<PathBuf>) {
        let own = self.own.clone();
        tokio::spawn(async move {
            let mut dialog = rfd::AsyncFileDialog::new();
            if let Some(dir) = start_in.filter(|d| d.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            let handle = match what {
                Pick::Folder => dialog.set_title("Choose where Dawnload keeps downloads").pick_folder().await,
                Pick::Cookies => dialog.set_title("Choose a cookies.txt").add_filter("cookies.txt", &["txt"]).pick_file().await,
                Pick::Tool(tool) => dialog.set_title(format!("Choose {}", tool.name())).pick_file().await,
            };
            let _ = own.send(UiMessage::Picked { what, path: handle.map(|h| h.path().to_path_buf()) });
        });
    }
}

fn tone(tone: Tone) -> i32 {
    match tone {
        Tone::Neutral => 0,
        Tone::Warning => 1,
        Tone::Failed => 2,
    }
}

fn look(settings: &Settings) -> (i32, bool) {
    let mode = match settings.app.appearance {
        Appearance::System => 0,
        Appearance::Dark => 1,
        Appearance::Light => 2,
    };
    let reduce = match settings.app.motion {
        Motion::System => platform::system_reduce_motion(),
        Motion::Reduce => true,
        Motion::Full => false,
    };
    (mode, reduce)
}

fn update_summary(state: &State) -> String {
    let updates: Vec<String> = state
        .mirror
        .tools
        .iter()
        .filter(|t| t.update_available && t.tool != ToolId::Ffprobe)
        .map(|t| format!("{} {}", t.tool.name(), t.latest.clone().unwrap_or_default()))
        .collect();
    if updates.is_empty() { "Every tool is up to date.".into() } else { format!("Available: {}.", updates.join(", ")) }
}

/// Rebuilds the visible rows and inspector, and diffs against what the UI shows.
fn refresh(state: &mut State, frame: &mut Frame) {
    let (built, ids) = view::rows(&state.mirror, state.filter);
    if built.len() != state.shown.len() {
        frame.resize = Some(built.len());
    }
    for (i, new) in built.iter().enumerate() {
        if state.shown.get(i) != Some(new) {
            frame.patch.push((i, new.clone()));
        }
    }
    state.shown = built;
    state.item_rows = ids;
    frame.filters = Some(view::filter_tabs(&state.mirror));
    frame.queue_label = Some(view::queue_label(&state.mirror).into());
    frame.empty_line = Some(state.filter.empty_line().into());
    frame.inspector = Some(view::inspector(&state.mirror, state.selected, state.detail.as_ref()));
}

//! Dawnload's desktop window (Phase 2 prototype). Slint draws; dawnload-core owns the state.
//!
//! Threads: Slint's event loop on the main thread; the core and a "bridge" task on a small tokio
//! runtime. The bridge keeps an `api::Mirror`, turns it into rows for the current filter, diffs
//! them against what's on screen and hands the UI thread only the rows that changed, at most as
//! often as the core sends batches (4 per second). The UI thread never formats a byte count.
//!
//! `dawnload-app` runs the demo engine with 500 items. `dawnload-app --real` runs the real core
//! (your data folder, your tools). Measurement switches (see scripts/measure-ui.sh):
//! DAWNLOAD_UI_STATS=1 prints frames and updates per second; DAWNLOAD_UI_AUTOSCROLL=<px/frame>
//! scrolls the list up and down forever; DAWNLOAD_UI_EXIT_AFTER=<seconds> quits;
//! DAWNLOAD_DEMO_ITEMS=<n> changes the demo size.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dawnload_core::api::{Action, ListFilter, Mirror, Row};
use dawnload_core::core::{Core, CoreConfig, CoreHandle, Settings};
use dawnload_core::engine::Lane;
use dawnload_core::explain::Tone;
use dawnload_core::model::{ItemId, Quality, Status};
use dawnload_core::source::{EngineId, EnginePreference, SourceKind};
use dawnload_core::tools::ToolId;
use dawnload_core::{format, present};
use slint::{Model, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;

slint::include_modules!();

const FILTERS: [ListFilter; 6] =
    [ListFilter::All, ListFilter::Active, ListFilter::Waiting, ListFilter::Kept, ListFilter::Failed, ListFilter::Seeding];

fn status_code(status: Status) -> i32 {
    match status {
        Status::Queued => 0,
        Status::Pulling => 1,
        Status::Polishing => 2,
        Status::Kept => 3,
        Status::Failed => 4,
        Status::Paused => 5,
        Status::Seeding => 6,
    }
}

/// What the UI thread asks the bridge.
enum UiMessage {
    Paste(String),
    Download(String),
    Filter(usize),
    Select(Option<ItemId>),
    MoveSelection(i32),
    Primary(ItemId),
    Remove(ItemId),
    QueueAction,
    CycleFormat,
    CycleEngine,
    TogglePolish,
    TogglePlaylist,
}

/// What the bridge hands the UI thread. Everything is plain data (Send).
#[derive(Default)]
struct Frame {
    /// The list's new length (rows are added or dropped at the end), then `patch` rewrites rows.
    /// The list is never reset, so the ListView keeps its scroll position and only redraws
    /// rows that changed and are on screen.
    resize: Option<usize>,
    patch: Vec<(usize, RowData)>,
    filters: Option<Vec<FilterTab>>,
    queue_label: Option<String>,
    empty_line: Option<String>,
    inspector: Option<(InspectorData, Vec<FileRow>)>,
    selected: Option<i32>,
    detection: Option<(String, i32, bool)>,
    clear_paste: bool,
    chips: Option<(String, String, bool, bool, String)>,
    tools: Option<(String, bool)>,
}

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    batches: AtomicU64,
    rows_patched: AtomicU64,
    resets: AtomicU64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let real = std::env::args().any(|a| a == "--real");
    let demo_items: usize = std::env::var("DAWNLOAD_DEMO_ITEMS").ok().and_then(|v| v.parse().ok()).unwrap_or(500);
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;

    let core = runtime.block_on(async {
        let config = if real { CoreConfig::new(dawnload_core::paths::data_dir()) } else { CoreConfig::demo() };
        Core::start(config)
    })?;

    let app = AppWindow::new()?;
    app.set_mode_note(if real { SharedString::new() } else { format!("demo · {demo_items} items").into() });
    let rows = Rc::new(VecModel::<RowData>::default());
    app.set_rows(ModelRc::from(rows.clone()));
    let filters = Rc::new(VecModel::<FilterTab>::default());
    app.set_filters(ModelRc::from(filters.clone()));
    let files = Rc::new(VecModel::<FileRow>::default());
    app.set_inspector_files(ModelRc::from(files.clone()));

    let counters = Arc::new(Counters::default());
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiMessage>();

    // UI → bridge.
    {
        let tx = ui_tx.clone();
        app.on_paste_edited(move |text| drop(tx.send(UiMessage::Paste(text.to_string()))));
        let tx = ui_tx.clone();
        let weak = app.as_weak();
        app.on_download(move || {
            if let Some(app) = weak.upgrade() {
                let _ = tx.send(UiMessage::Download(app.get_paste_text().to_string()));
            }
        });
        let tx = ui_tx.clone();
        app.on_select_filter(move |i| drop(tx.send(UiMessage::Filter(i.max(0) as usize))));
        let tx = ui_tx.clone();
        app.on_select(move |id| drop(tx.send(UiMessage::Select((id >= 0).then_some(ItemId(id as u64))))));
        let tx = ui_tx.clone();
        app.on_move_selection(move |d| drop(tx.send(UiMessage::MoveSelection(d))));
        let tx = ui_tx.clone();
        app.on_primary_action(move |id| drop(tx.send(UiMessage::Primary(ItemId(id as u64)))));
        let tx = ui_tx.clone();
        app.on_remove(move |id| drop(tx.send(UiMessage::Remove(ItemId(id as u64)))));
        let tx = ui_tx.clone();
        app.on_queue_action(move || drop(tx.send(UiMessage::QueueAction)));
        let tx = ui_tx.clone();
        app.on_cycle_format(move || drop(tx.send(UiMessage::CycleFormat)));
        let tx = ui_tx.clone();
        app.on_cycle_engine(move || drop(tx.send(UiMessage::CycleEngine)));
        let tx = ui_tx.clone();
        app.on_toggle_polish(move || drop(tx.send(UiMessage::TogglePolish)));
        let tx = ui_tx.clone();
        app.on_toggle_playlist(move || drop(tx.send(UiMessage::TogglePlaylist)));
    }

    // Bridge → UI: apply frames on the UI thread.
    let apply = {
        let weak = app.as_weak();
        let counters = counters.clone();
        move |frame: Frame| {
            let weak = weak.clone();
            let counters = counters.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                apply_frame(&app, frame, &counters);
            });
        }
    };

    runtime.spawn(bridge(core.clone(), ui_rx, apply, counters.clone(), real));
    if !real {
        runtime.spawn(demo_driver(core.clone(), demo_items));
    }

    // Review switches: DAWNLOAD_UI_THEME=light|dark, DAWNLOAD_UI_SELECT_FIRST=1.
    match std::env::var("DAWNLOAD_UI_THEME").as_deref() {
        Ok("light") => app.global::<Theme>().set_mode(2),
        Ok("dark") => app.global::<Theme>().set_mode(1),
        _ => {}
    }
    app.set_ease_rows(std::env::var("DAWNLOAD_UI_EASE_ROWS").is_ok());
    if std::env::var("DAWNLOAD_UI_SELECT_FIRST").is_ok() {
        let tx = ui_tx.clone();
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::SingleShot, Duration::from_millis(1500), move || {
            let _ = tx.send(UiMessage::MoveSelection(1));
        });
        std::mem::forget(timer);
    }
    install_measurement(&app, counters.clone());
    app.run()?;
    runtime.block_on(core.shutdown());
    Ok(())
}

fn apply_frame(app: &AppWindow, frame: Frame, counters: &Counters) {
    let rows_model = app.get_rows();
    let rows = rows_model.as_any().downcast_ref::<VecModel<RowData>>().expect("rows model");
    if let Some(len) = frame.resize {
        let current = rows.row_count();
        if len > current {
            for _ in current..len {
                rows.push(RowData::default());
            }
        } else {
            for index in (len..current).rev() {
                rows.remove(index);
            }
        }
        counters.resets.fetch_add(1, Ordering::Relaxed);
    }
    for (index, row) in frame.patch {
        if index < rows.row_count() {
            rows.set_row_data(index, row);
            counters.rows_patched.fetch_add(1, Ordering::Relaxed);
        }
    }
    counters.batches.fetch_add(1, Ordering::Relaxed);
    if let Some(tabs) = frame.filters {
        let model = app.get_filters();
        let filters = model.as_any().downcast_ref::<VecModel<FilterTab>>().expect("filters model");
        if filters.row_count() != tabs.len() {
            filters.set_vec(tabs);
        } else {
            for (i, tab) in tabs.into_iter().enumerate() {
                if filters.row_data(i).as_ref() != Some(&tab) {
                    filters.set_row_data(i, tab);
                }
            }
        }
    }
    if let Some(label) = frame.queue_label {
        app.set_queue_label(label.into());
    }
    if let Some(line) = frame.empty_line {
        app.set_empty_line(line.into());
    }
    if let Some((data, files)) = frame.inspector {
        app.set_inspector(data);
        let model = app.get_inspector_files();
        let file_model = model.as_any().downcast_ref::<VecModel<FileRow>>().expect("files model");
        if file_model.iter().ne(files.iter().cloned()) {
            file_model.set_vec(files);
        }
    }
    if let Some(id) = frame.selected {
        app.set_selected_id(id);
    }
    if let Some((text, tone, can_submit)) = frame.detection {
        app.set_detection(text.into());
        app.set_detection_tone(tone);
        app.set_can_submit(can_submit);
    }
    if frame.clear_paste {
        app.set_paste_text(SharedString::new());
    }
    if let Some((format, engine, polish, playlist, folder)) = frame.chips {
        app.set_chip_format(format.into());
        app.set_chip_engine(engine.into());
        app.set_chip_polish(polish);
        app.set_chip_playlist(playlist);
        app.set_chip_folder(folder.into());
    }
    if let Some((text, ok)) = frame.tools {
        app.set_tools_text(text.into());
        app.set_tools_ok(ok);
    }
}

struct BridgeState {
    mirror: Mirror,
    filter: ListFilter,
    selected: Option<ItemId>,
    shown: Vec<RowData>,
    item_rows: Vec<ItemId>,
    detail: Option<dawnload_core::api::ItemDetail>,
    settings: Settings,
    real: bool,
}

async fn bridge(
    core: CoreHandle,
    mut ui: mpsc::UnboundedReceiver<UiMessage>,
    apply: impl Fn(Frame),
    counters: Arc<Counters>,
    real: bool,
) {
    let _ = counters;
    let Some(mut sub) = core.subscribe().await else { return };
    let settings = core.settings().await.unwrap_or_default();
    let mut state = BridgeState {
        mirror: Mirror::default(),
        filter: ListFilter::All,
        selected: None,
        shown: Vec::new(),
        item_rows: Vec::new(),
        detail: None,
        settings,
        real,
    };
    apply(Frame { chips: Some(chips(&state.settings)), ..Frame::default() });

    loop {
        let mut frame = Frame::default();
        tokio::select! {
            update = sub.recv() => {
                let Some(update) = update else { return };
                let applied = state.mirror.apply(update);
                if let Some(detail) = applied.detail
                    && Some(detail.item.id) == state.selected {
                        state.detail = Some(detail);
                    }
                if applied.tools || !state.real {
                    frame.tools = Some(tools_pill(&state));
                }
                refresh(&mut state, &mut frame, applied.structure);
            }
            message = ui.recv() => {
                let Some(message) = message else { return };
                match message {
                    UiMessage::Paste(text) => {
                        let explanation = core.explain(&text, Some(state.settings.defaults.clone())).await;
                        frame.detection = Some(match explanation {
                            Some(e) => (e.text, match e.tone { Tone::Neutral => 0, Tone::Warning => 1, Tone::Failed => 2 }, e.can_submit),
                            None => (String::new(), 0, true),
                        });
                    }
                    UiMessage::Download(text) => match core.add(&text, Some(state.settings.defaults.clone()), true).await {
                        Ok(id) => {
                            frame.clear_paste = true;
                            frame.detection = Some((String::new(), 0, true));
                            state.selected = Some(id);
                            frame.selected = Some(id.0 as i32);
                            sub.watch(Some(id));
                        }
                        Err(failure) => frame.detection = Some((failure.sentence(), 2, true)),
                    },
                    UiMessage::Filter(index) => {
                        state.filter = FILTERS.get(index).copied().unwrap_or(ListFilter::All);
                        refresh(&mut state, &mut frame, true);
                    }
                    UiMessage::Select(id) => {
                        state.selected = id;
                        state.detail = None;
                        sub.watch(id);
                        refresh(&mut state, &mut frame, false);
                    }
                    UiMessage::MoveSelection(delta) => {
                        let current = state.selected.and_then(|id| state.item_rows.iter().position(|i| *i == id));
                        let next = match current {
                            Some(i) => (i as i64 + delta as i64).clamp(0, state.item_rows.len() as i64 - 1) as usize,
                            None if !state.item_rows.is_empty() => 0,
                            None => continue,
                        };
                        let id = state.item_rows[next];
                        state.selected = Some(id);
                        state.detail = None;
                        sub.watch(Some(id));
                        frame.selected = Some(id.0 as i32);
                        refresh(&mut state, &mut frame, false);
                    }
                    UiMessage::Primary(id) => {
                        if let Some(item) = state.mirror.item(id) {
                            match item.status {
                                Status::Pulling | Status::Polishing => drop(core.act(Action::Pause { id }).await),
                                Status::Queued | Status::Paused | Status::Failed => drop(core.act(Action::Resume { id }).await),
                                Status::Seeding => drop(core.act(Action::StopSeeding { id }).await),
                                Status::Kept => open_folder(state.detail.as_ref().map(|d| d.item.options.destination.clone())),
                            }
                        }
                    }
                    UiMessage::Remove(id) => {
                        let _ = core.act(Action::Remove { ids: vec![id] }).await;
                        if state.selected == Some(id) {
                            state.selected = None;
                            state.detail = None;
                            sub.watch(None);
                            frame.selected = Some(-1);
                        }
                    }
                    UiMessage::QueueAction => {
                        let c = &state.mirror.counts;
                        let action = if c.active > 0 || state.mirror.queue.running {
                            Action::PauseAll
                        } else if c.queued > 0 {
                            Action::StartQueue
                        } else {
                            Action::ResumeAll
                        };
                        let _ = core.act(action).await;
                    }
                    UiMessage::CycleFormat | UiMessage::CycleEngine | UiMessage::TogglePolish | UiMessage::TogglePlaylist => {
                        let d = &mut state.settings.defaults;
                        match message {
                            UiMessage::CycleFormat => d.quality = match d.quality { Quality::Best => Quality::Compatible, Quality::Compatible => Quality::Audio, Quality::Audio => Quality::Best },
                            UiMessage::CycleEngine => d.engine = match d.engine {
                                EnginePreference::Auto => EnginePreference::YtDlp,
                                EnginePreference::YtDlp => EnginePreference::GalleryDl,
                                EnginePreference::GalleryDl => EnginePreference::Direct,
                                EnginePreference::Direct => EnginePreference::Auto,
                            },
                            UiMessage::TogglePolish => d.polish = !d.polish,
                            _ => d.playlist = !d.playlist,
                        }
                        core.set_settings(state.settings.clone()).await;
                        frame.chips = Some(chips(&state.settings));
                    }
                }
            }
        }
        apply(frame);
    }
}

/// Rebuilds the visible rows and inspector, and diffs against what the UI shows.
fn refresh(state: &mut BridgeState, frame: &mut Frame, structure: bool) {
    let rows = state.mirror.rows(state.filter);
    let mut built = Vec::with_capacity(rows.len());
    let mut item_rows = Vec::new();
    for row in &rows {
        built.push(match row {
            Row::Header { section, count } => {
                RowData { kind: 0, header: section.title().into(), count: *count as i32, ..RowData::default() }
            }
            Row::Item { id, queue_position } => {
                item_rows.push(*id);
                let item = state.mirror.item(*id).expect("item in mirror");
                RowData {
                    kind: 1,
                    id: id.0 as i32,
                    title: item.title.as_str().into(),
                    subline: present::subline(item, *queue_position).into(),
                    trailing: present::trailing(item).into(),
                    status: status_code(item.status),
                    fill: item.fill as f32,
                    torrent: item.source_kind != SourceKind::Web,
                    ..RowData::default()
                }
            }
        });
    }
    let _ = structure;
    if built.len() != state.shown.len() {
        frame.resize = Some(built.len());
    }
    for (i, new) in built.iter().enumerate() {
        if state.shown.get(i) != Some(new) {
            frame.patch.push((i, new.clone()));
        }
    }
    state.shown = built;
    state.item_rows = item_rows;

    let counts = &state.mirror.counts;
    frame.filters =
        Some(FILTERS.iter().map(|f| FilterTab { title: f.title().into(), count: counts.for_filter(*f) as i32 }).collect());
    frame.queue_label = Some(
        if counts.active > 0 || state.mirror.queue.running {
            "Pause All"
        } else if counts.queued > 0 {
            "Start Queue"
        } else if counts.paused > 0 {
            "Resume All"
        } else {
            ""
        }
        .into(),
    );
    frame.empty_line = Some(state.filter.empty_line().into());
    frame.inspector = Some(inspector(state));
}

fn inspector(state: &BridgeState) -> (InspectorData, Vec<FileRow>) {
    let Some(item) = state.selected.and_then(|id| state.mirror.item(id)) else {
        return (InspectorData::default(), Vec::new());
    };
    let t = &item.transfer;
    let percent = match (item.status, t.fraction) {
        (Status::Pulling | Status::Paused, Some(f)) => format::percent(f),
        _ => String::new(),
    };
    let mut numbers = Vec::new();
    match (t.bytes_done, t.bytes_total) {
        (Some(d), Some(total)) if item.status.is_active() || item.status == Status::Paused => {
            numbers.push(format::bytes_of(d, total))
        }
        _ => {}
    }
    if item.status.is_active() {
        if let Some(speed) = t.bytes_per_second {
            numbers.push(format::speed(speed));
        }
        if let Some(left) = t.seconds_left {
            numbers.push(format::time_left(left));
        }
    }
    if numbers.is_empty() && item.status != Status::Failed {
        numbers.push(present::subline(item, None));
    }
    let swarm = item.swarm.clone().unwrap_or_default();
    let primary = match item.status {
        Status::Pulling | Status::Polishing => "Pause",
        Status::Queued => "Download Now",
        Status::Paused => "Resume",
        Status::Failed => "Retry",
        Status::Seeding => "Stop Seeding",
        Status::Kept => "Show in Folder",
    };
    let detail = state.detail.as_ref().filter(|d| d.item.id == item.id);
    let files: Vec<FileRow> = detail
        .map(|d| {
            d.item
                .files
                .iter()
                .map(|f| FileRow { name: f.name().into(), size: f.size.map(format::bytes).unwrap_or_default().into() })
                .collect()
        })
        .unwrap_or_default();
    let log = detail
        .map(|d| {
            let start = d.log.len().saturating_sub(60);
            d.log[start..].iter().map(|l| l.chars().take(400).collect::<String>()).collect::<Vec<_>>().join("\n")
        })
        .unwrap_or_default();
    let engine = match item.engine {
        EngineId::Direct => "built-in HTTP".to_string(),
        other => other.display_name().to_string(),
    };
    let data = InspectorData {
        present: true,
        id: item.id.0 as i32,
        title: item.title.as_str().into(),
        word: item.status.word().into(),
        status: status_code(item.status),
        fill: item.fill as f32,
        source: item.source.as_str().into(),
        percent: percent.into(),
        numbers: numbers.join(" · ").into(),
        torrent: item.source_kind != SourceKind::Web,
        peers: swarm.peers.to_string().into(),
        seeds: swarm.seeds.to_string().into(),
        down: format::speed(swarm.download_rate).into(),
        up: format::speed(swarm.upload_rate).into(),
        uploaded: format::bytes(swarm.uploaded).into(),
        ratio: format::ratio(swarm.ratio).into(),
        options: item.options_summary.as_str().into(),
        engine: engine.into(),
        failure: item.failure.as_ref().map(|f| f.message.clone()).unwrap_or_default().into(),
        hint: item.failure.as_ref().and_then(|f| f.hint.clone()).unwrap_or_default().into(),
        log: log.into(),
        primary: primary.into(),
    };
    (data, files)
}

fn chips(settings: &Settings) -> (String, String, bool, bool, String) {
    let d = &settings.defaults;
    let engine = match d.engine {
        EnginePreference::Auto => "Auto",
        EnginePreference::YtDlp => "yt-dlp",
        EnginePreference::GalleryDl => "gallery-dl",
        EnginePreference::Direct => "Direct",
    };
    let folder = d.destination.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    (d.quality.title().into(), engine.into(), d.polish, d.playlist, folder)
}

fn tools_pill(state: &BridgeState) -> (String, bool) {
    if !state.real {
        return ("Demo engine".into(), true);
    }
    if state.mirror.tools.is_empty() {
        return ("Checking tools".into(), true);
    }
    for tool in [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::Deno] {
        if state.mirror.tools.iter().any(|t| t.tool == tool && t.path.is_none()) {
            return (format!("{} missing", tool.name()), false);
        }
    }
    ("Tools ready".into(), true)
}

fn open_folder(folder: Option<std::path::PathBuf>) {
    let Some(folder) = folder else { return };
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(folder).spawn();
}

/// Keeps ~`count` demo items alive: web items, a tenth of them torrents that seed; finished ones
/// are cleared and replaced, so the list keeps moving.
async fn demo_driver(core: CoreHandle, count: usize) {
    let mut settings = core.settings().await.unwrap_or_default();
    settings.lane_limits.insert(Lane::Demo, (count / 2).max(1));
    settings.defaults.seed_ratio_limit = Some(1.5);
    core.set_settings(settings).await;
    let mut serial = 0u64;
    let mut add = |n: usize| {
        let core = core.clone();
        let start = serial;
        serial += n as u64;
        async move {
            for i in start..start + n as u64 {
                let text = if i % 10 == 3 {
                    let hash = format!("{:040x}", (i as u128 + 1) * 0x9E37_79B9_7F4A_7C15_u128);
                    format!("magnet:?xt=urn:btih:{}&dn=Open+Movie+Collection+{i}", &hash[hash.len() - 40..])
                } else {
                    format!("https://media.example.org/watch/{i}")
                };
                let _ = core.add(&text, None, true).await;
            }
        }
    };
    add(count).await;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(snapshot) = core.snapshot().await else { return };
        let mut finished: Vec<_> = snapshot.items.iter().filter(|i| matches!(i.status, Status::Kept | Status::Failed)).collect();
        if finished.len() > count / 4 {
            finished.sort_by_key(|i| i.finished_at);
            let ids: Vec<ItemId> = finished.iter().take(20).map(|i| i.id).collect();
            let n = ids.len();
            let _ = core.act(Action::Remove { ids }).await;
            add(n).await;
        }
    }
}

/// FPS counting, auto-scroll and auto-exit for scripts/measure-ui.sh.
fn install_measurement(app: &AppWindow, counters: Arc<Counters>) {
    let stats = std::env::var("DAWNLOAD_UI_STATS").is_ok();
    let autoscroll: Option<f32> = std::env::var("DAWNLOAD_UI_AUTOSCROLL").ok().and_then(|v| v.parse().ok());
    let exit_after: Option<u64> = std::env::var("DAWNLOAD_UI_EXIT_AFTER").ok().and_then(|v| v.parse().ok());

    let frame_counter = counters.clone();
    let _ = app.window().set_rendering_notifier(move |state, _| {
        if let slint::RenderingState::AfterRendering = state {
            frame_counter.frames.fetch_add(1, Ordering::Relaxed);
        }
    });

    thread_local! {
        static TIMERS: RefCell<Vec<slint::Timer>> = const { RefCell::new(Vec::new()) };
    }
    if stats {
        let timer = slint::Timer::default();
        let mut last = (0u64, 0u64, 0u64, 0u64);
        let started = std::time::Instant::now();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let now = (
                counters.frames.load(Ordering::Relaxed),
                counters.batches.load(Ordering::Relaxed),
                counters.rows_patched.load(Ordering::Relaxed),
                counters.resets.load(Ordering::Relaxed),
            );
            eprintln!(
                "stats t={:.0}s fps={} frames_applied={} rows_patched={} list_resizes={}",
                started.elapsed().as_secs_f64(),
                now.0 - last.0,
                now.1 - last.1,
                now.2 - last.2,
                now.3 - last.3
            );
            last = now;
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
    if let Some(speed) = autoscroll {
        let weak = app.as_weak();
        let direction = Rc::new(RefCell::new(1.0f32));
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            // viewport-y runs from 0 down to -(content - visible).
            let rows = app.get_rows().row_count() as f32;
            let max = (rows * 56.0 - app.get_list_height()).max(0.0);
            let mut dir = direction.borrow_mut();
            let mut y = -app.get_list_scroll() + speed * *dir;
            if y >= max {
                y = max;
                *dir = -1.0;
            } else if y <= 0.0 {
                y = 0.0;
                *dir = 1.0;
            }
            app.set_list_scroll(-y);
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
    if let Some(seconds) = exit_after {
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::SingleShot, Duration::from_secs(seconds), || {
            let _ = slint::quit_event_loop();
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
}

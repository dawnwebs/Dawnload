//! What the window shows, built from the core's state. Pure functions from core types to the
//! Slint structs, so they're tested without a window (see the tests at the end).

use std::path::Path;

use dawnload_core::api::{ItemDetail, ListFilter, Mirror, Row};
use dawnload_core::core::{Appearance, Motion, Settings};
use dawnload_core::engine::Lane;
use dawnload_core::model::{Cookies, Quality, Status};
use dawnload_core::source::{EngineId, EnginePreference, SourceKind};
use dawnload_core::tools::{JsRuntime, JsRuntimeKind, ToolId, ToolOrigin, ToolStatus};
use dawnload_core::{format, present};

use crate::{Chips, FileRow, FilterTab, InspectorData, Prefs, RowData, ToolRow};

pub const FILTERS: [ListFilter; 6] =
    [ListFilter::All, ListFilter::Active, ListFilter::Waiting, ListFilter::Kept, ListFilter::Failed, ListFilter::Seeding];

/// Settings' option lists (ui/settings.slint › Choices), index for index.
pub const QUALITIES: [Quality; 3] = [Quality::Best, Quality::Compatible, Quality::Audio];
pub const ENGINES: [EnginePreference; 4] =
    [EnginePreference::Auto, EnginePreference::YtDlp, EnginePreference::GalleryDl, EnginePreference::Direct];
pub const CONNECTIONS: [u8; 5] = [1, 2, 4, 8, 16];
pub const LANE_LIMITS: [usize; 4] = [1, 2, 3, 4];
pub const RETRIES: [u32; 3] = [0, 2, 5];
pub const APPEARANCES: [Appearance; 3] = [Appearance::System, Appearance::Dark, Appearance::Light];
pub const MOTIONS: [Motion; 3] = [Motion::System, Motion::Reduce, Motion::Full];
pub const IMPERSONATIONS: [Option<&str>; 5] =
    [None, Some("chrome-146"), Some("chrome-145"), Some("chrome-142"), Some("safari-18.4")];

/// The tools the Tools screen lists, in order. ffprobe ships with ffmpeg.
pub const TOOLS_SHOWN: [ToolId; 5] = [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::Deno, ToolId::GalleryDl, ToolId::Ffprobe];

fn index_of<T: PartialEq>(list: &[T], value: &T) -> i32 {
    list.iter().position(|v| v == value).map(|i| i as i32).unwrap_or(0)
}

/// The closest option at or below `value` (a lane limit of 5 shows as 4).
fn nearest<T: PartialOrd + Copy>(list: &[T], value: T) -> i32 {
    list.iter().rposition(|v| *v <= value).unwrap_or(0) as i32
}

pub fn status_code(status: Status) -> i32 {
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

/// The rows for a filter, plus the item id behind each item row.
pub fn rows(mirror: &Mirror, filter: ListFilter) -> (Vec<RowData>, Vec<dawnload_core::model::ItemId>) {
    let rows = mirror.rows(filter);
    let mut built = Vec::with_capacity(rows.len());
    let mut ids = Vec::new();
    for row in &rows {
        built.push(match row {
            Row::Header { section, count } => {
                RowData { kind: 0, header: section.title().into(), count: *count as i32, ..RowData::default() }
            }
            Row::Item { id, queue_position } => {
                ids.push(*id);
                let item = mirror.item(*id).expect("item in mirror");
                RowData {
                    kind: 1,
                    id: id.0 as i32,
                    title: item.title.as_str().into(),
                    subline: present::subline(item, *queue_position).into(),
                    trailing: present::trailing(item).into(),
                    status: status_code(item.status),
                    fill: item.fill as f32,
                    loading: item.status == Status::Pulling && item.transfer.fraction.is_none(),
                    torrent: item.source_kind != SourceKind::Web,
                    a11y: present::accessible_label(item).into(),
                    has_file: item.primary_file.is_some(),
                    ..RowData::default()
                }
            }
        });
    }
    (built, ids)
}

pub fn filter_tabs(mirror: &Mirror) -> Vec<FilterTab> {
    // Seeding appears once torrents ship (DESIGN.md); until then it would always be 0.
    let shown = if mirror.counts.seeding > 0 { &FILTERS[..] } else { &FILTERS[..5] };
    shown.iter().map(|f| FilterTab { title: f.title().into(), count: mirror.counts.for_filter(*f) as i32 }).collect()
}

/// Start Queue when items wait and nothing runs, Pause All while something runs, Resume All when
/// only paused items are left.
pub fn queue_label(mirror: &Mirror) -> &'static str {
    let c = &mirror.counts;
    if c.active > 0 || (mirror.queue.running && c.queued > 0) {
        "Pause All"
    } else if c.queued > 0 {
        "Start Queue"
    } else if c.paused > 0 {
        "Resume All"
    } else {
        ""
    }
}

/// The inspector for the selected item.
pub fn inspector(
    mirror: &Mirror,
    selected: Option<dawnload_core::model::ItemId>,
    detail: Option<&ItemDetail>,
) -> (InspectorData, Vec<FileRow>) {
    let Some(item) = selected.and_then(|id| mirror.item(id)) else {
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
        Status::Kept => "",
    };
    let detail = detail.filter(|d| d.item.id == item.id);
    let files: Vec<FileRow> = detail
        .map(|d| {
            d.item
                .files
                .iter()
                .filter(|f| f.selected)
                .map(|f| FileRow {
                    name: f.name().into(),
                    size: f.size.map(format::bytes).unwrap_or_default().into(),
                    path: f.path.as_str().into(),
                })
                .collect()
        })
        .unwrap_or_default();
    let log = detail.map(|d| log_text(&d.log, 60)).unwrap_or_default();
    let engine = match item.engine {
        EngineId::Direct => "Dawnload's HTTP engine".to_string(),
        other => other.display_name().to_string(),
    };
    let data = InspectorData {
        present: true,
        id: item.id.0 as i32,
        title: item.title.as_str().into(),
        word: item.status.word().into(),
        status: status_code(item.status),
        fill: item.fill as f32,
        loading: item.status == Status::Pulling && t.fraction.is_none(),
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
        has_file: item.primary_file.is_some(),
    };
    (data, files)
}

/// The last `lines` log lines, each cut to 400 characters (a tool can print a megabyte of JSON).
pub fn log_text(log: &[String], lines: usize) -> String {
    let start = log.len().saturating_sub(lines);
    log[start..].iter().map(|l| l.chars().take(400).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// The chips under the paste field. `picked` is what Auto resolved the pasted link to.
pub fn chips(settings: &Settings, picked: Option<EngineId>) -> Chips {
    let d = &settings.defaults;
    let engine_label = match (d.engine, picked) {
        (EnginePreference::Auto, Some(engine)) if engine != EngineId::Demo => format!("Auto · {}", engine_word(engine)),
        (EnginePreference::Auto, _) => "Auto".into(),
        (EnginePreference::YtDlp, _) => "yt-dlp".into(),
        (EnginePreference::GalleryDl, _) => "gallery-dl".into(),
        (EnginePreference::Direct, _) => "Direct".into(),
    };
    let resolved = match d.engine {
        EnginePreference::Auto => picked,
        EnginePreference::GalleryDl => Some(EngineId::GalleryDl),
        _ => None,
    };
    Chips {
        format: index_of(&QUALITIES, &d.quality),
        format_enabled: resolved != Some(EngineId::GalleryDl),
        engine: index_of(&ENGINES, &d.engine),
        engine_label: engine_label.into(),
        polish: d.polish,
        playlist: d.playlist,
        impersonate: impersonation_index(d.impersonate.as_deref()),
        cookies_available: settings.cookies_file.is_some(),
        cookies: matches!(d.cookies, Cookies::File(_)),
        folder: folder_name(&d.destination).into(),
    }
}

fn engine_word(engine: EngineId) -> &'static str {
    match engine {
        EngineId::Direct => "Direct",
        other => other.display_name(),
    }
}

pub fn impersonation_index(target: Option<&str>) -> i32 {
    IMPERSONATIONS.iter().position(|t| *t == target).map(|i| i as i32).unwrap_or(0)
}

pub fn folder_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

pub fn prefs(settings: &Settings, tray_available: bool) -> Prefs {
    let d = &settings.defaults;
    Prefs {
        folder: d.destination.display().to_string().into(),
        format: index_of(&QUALITIES, &d.quality),
        engine: index_of(&ENGINES, &d.engine),
        polish: d.polish,
        playlist: d.playlist,
        connections: nearest(&CONNECTIONS, d.connections),
        lane: nearest(&LANE_LIMITS, settings.lane_limit(Lane::Web)),
        retries: nearest(&RETRIES, settings.auto_retries),
        appearance: index_of(&APPEARANCES, &settings.app.appearance),
        motion: index_of(&MOTIONS, &settings.app.motion),
        close_to_tray: settings.app.close_to_tray,
        notify_kept: settings.app.notify_kept,
        notify_failed: settings.app.notify_failed,
        auto_update: settings.auto_update_tools,
        impersonate: impersonation_index(d.impersonate.as_deref()),
        cookies_path: settings.cookies_file.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into(),
        cookies_on: matches!(d.cookies, Cookies::File(_)),
        tray_available,
    }
}

/// Applies one Settings or chip change (`key`, `value` from the UI) and says whether it changed
/// anything the core needs to store.
pub fn set_option(settings: &mut Settings, key: &str, value: i32) -> bool {
    let i = value.max(0) as usize;
    let d = &mut settings.defaults;
    match key {
        "format" => d.quality = QUALITIES.get(i).copied().unwrap_or_default(),
        "engine" => d.engine = ENGINES.get(i).copied().unwrap_or_default(),
        "polish" => d.polish = value != 0,
        "playlist" => d.playlist = value != 0,
        "impersonate" => d.impersonate = IMPERSONATIONS.get(i).copied().flatten().map(String::from),
        "cookies" => {
            d.cookies = match (&settings.cookies_file, value != 0) {
                (Some(path), true) => Cookies::File(path.clone()),
                _ => Cookies::None,
            }
        }
        "connections" => d.connections = CONNECTIONS.get(i).copied().unwrap_or(4),
        "lane" => {
            settings.lane_limits.insert(Lane::Web, LANE_LIMITS.get(i).copied().unwrap_or(1));
        }
        "retries" => settings.auto_retries = RETRIES.get(i).copied().unwrap_or(2),
        "appearance" => settings.app.appearance = APPEARANCES.get(i).copied().unwrap_or_default(),
        "motion" => settings.app.motion = MOTIONS.get(i).copied().unwrap_or_default(),
        "close-to-tray" => settings.app.close_to_tray = value != 0,
        "notify-kept" => settings.app.notify_kept = value != 0,
        "notify-failed" => settings.app.notify_failed = value != 0,
        "auto-update" => settings.auto_update_tools = value != 0,
        _ => return false,
    }
    true
}

/// Where a tool came from, in words.
fn origin_word(origin: Option<ToolOrigin>) -> &'static str {
    match origin {
        Some(ToolOrigin::Managed) => "Dawnload's copy",
        Some(ToolOrigin::System) => "from the system",
        Some(ToolOrigin::Custom) => "your choice",
        None => "",
    }
}

/// One tool's install state while the Tools screen works on it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Installing {
        done: u64,
        total: Option<u64>,
    },
    Failed(String),
    Done(String),
}

/// The Tools screen's rows.
pub fn tool_rows(statuses: &[ToolStatus], jobs: &std::collections::BTreeMap<ToolId, Job>, settings: &Settings) -> Vec<ToolRow> {
    TOOLS_SHOWN
        .iter()
        .map(|tool| {
            let status = statuses.iter().find(|s| s.tool == *tool);
            let job = jobs.get(tool).cloned().unwrap_or_default();
            let found = status.and_then(|s| s.path.as_ref()).is_some();
            let installable = status.is_some_and(|s| s.installable);
            let managed = status.and_then(|s| s.origin) == Some(ToolOrigin::Managed);
            let version = status
                .and_then(|s| s.version.as_ref().map(|v| format!("{v} · {}", origin_word(s.origin))))
                .or_else(|| found.then(|| origin_word(status.and_then(|s| s.origin)).to_string()))
                .unwrap_or_default();
            let (code, loading, fill, message, failed) = match &job {
                Job::Installing { done, total } => {
                    let fill = total.filter(|t| *t > 0).map(|t| *done as f32 / t as f32);
                    let words = match total {
                        Some(t) => format!("Installing · {}", format::bytes_of(*done, *t)),
                        None if *done > 0 => format!("Installing · {}", format::bytes(*done)),
                        None => "Installing…".into(),
                    };
                    (status_code(Status::Pulling), fill.is_none(), fill.unwrap_or(0.0), words, false)
                }
                Job::Failed(message) => (status_code(Status::Failed), false, 1.0, message.clone(), true),
                Job::Done(message) => (status_code(Status::Kept), false, 1.0, message.clone(), false),
                Job::Idle if status.is_none() => (status_code(Status::Queued), false, 0.0, "Checking…".to_string(), false),
                Job::Idle if found => {
                    let note = match status {
                        Some(s) if s.update_available => match (&s.latest, s.origin) {
                            (Some(latest), Some(ToolOrigin::Managed)) => format!("{latest} is available."),
                            (Some(latest), _) => format!("{latest} is available; update it with the package manager it came from, or install Dawnload's copy."),
                            _ => String::new(),
                        },
                        _ => String::new(),
                    };
                    (status_code(Status::Kept), false, 1.0, note, false)
                }
                Job::Idle => {
                    let why = match tool {
                        ToolId::YtDlp => "Not found. Dawnload needs it for video and audio from the web.",
                        ToolId::Ffmpeg => "Not found. Polish and Audio only need it.",
                        ToolId::Ffprobe => "Not found. It comes with ffmpeg.",
                        ToolId::Deno => "Not found. YouTube needs a JavaScript runtime.",
                        ToolId::GalleryDl => "Not found. Galleries and image sets need it.",
                    };
                    (status_code(Status::Failed), false, 1.0, why.to_string(), false)
                }
            };
            let busy = matches!(job, Job::Installing { .. });
            let update = status.is_some_and(|s| s.update_available);
            let action = if *tool == ToolId::Ffprobe || !installable {
                ""
            } else if !found {
                "Install"
            } else if update && managed {
                "Update"
            } else if update || !managed {
                "Install Dawnload's Copy"
            } else {
                ""
            };
            ToolRow {
                id: tool.name().into(),
                name: tool.name().into(),
                purpose: tool.purpose().into(),
                status: code,
                loading,
                fill,
                version: version.into(),
                path: status.and_then(|s| s.path.as_ref()).map(|p| p.display().to_string()).unwrap_or_default().into(),
                message: message.into(),
                failed,
                action: action.into(),
                busy,
                custom: settings.tool_paths.contains_key(tool),
            }
        })
        .collect()
}

/// The line about YouTube's JavaScript runtime, and whether it's fine.
pub fn js_line(runtime: Option<&JsRuntime>, statuses: &[ToolStatus]) -> (String, bool) {
    match runtime {
        Some(rt) => {
            let name = match rt.kind {
                JsRuntimeKind::Deno => "deno",
                JsRuntimeKind::Node => "Node.js",
                JsRuntimeKind::Bun => "Bun",
                JsRuntimeKind::QuickJs => "QuickJS",
            };
            let version = statuses
                .iter()
                .find(|s| s.tool == ToolId::Deno && rt.kind == JsRuntimeKind::Deno)
                .and_then(|s| s.version.clone())
                .map(|v| format!(" {v}"))
                .unwrap_or_default();
            (format!("yt-dlp uses {name}{version} at {}.", rt.path.display()), true)
        }
        None => (
            "None found, so YouTube downloads will fail. Install deno above; Node.js, Bun or QuickJS from your system also work."
                .into(),
            false,
        ),
    }
}

/// The tools pill: "Tools ready", or the first missing tool that matters.
pub fn tools_pill(statuses: &[ToolStatus]) -> (String, bool) {
    if statuses.is_empty() {
        return ("Checking tools".into(), true);
    }
    for tool in [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::Deno] {
        if statuses.iter().any(|t| t.tool == tool && t.path.is_none()) {
            return (format!("{} missing", tool.name()), false);
        }
    }
    if statuses.iter().any(|t| t.update_available && t.origin == Some(ToolOrigin::Managed)) {
        return ("Tool updates".into(), true);
    }
    ("Tools ready".into(), true)
}

/// The tool whose absence turns the empty list into a setup card (yt-dlp: most links need it).
pub fn missing_tool(statuses: &[ToolStatus]) -> Option<ToolId> {
    statuses.iter().find(|s| s.tool == ToolId::YtDlp && s.path.is_none()).map(|s| s.tool)
}

/// The tools first run offers to install: the missing ones Dawnload can install.
pub fn missing_installable(statuses: &[ToolStatus]) -> Vec<ToolId> {
    statuses
        .iter()
        .filter(|s| s.path.is_none() && s.installable && s.tool != ToolId::Ffprobe && s.tool != ToolId::GalleryDl)
        .map(|s| s.tool)
        .collect()
}

/// The tray's tooltip: "Dawnload · 1 pulling · 3 waiting".
pub fn tray_tooltip(mirror: &Mirror) -> String {
    let c = &mirror.counts;
    let mut parts = vec!["Dawnload".to_string()];
    if c.active > 0 {
        parts.push(format!("{} pulling", c.active));
    }
    if c.queued > 0 {
        parts.push(format!("{} waiting", c.queued));
    }
    if c.paused > 0 {
        parts.push(format!("{} paused", c.paused));
    }
    if c.failed > 0 {
        parts.push(format!("{} failed", c.failed));
    }
    parts.join(" · ")
}

/// A notification for an item that just finished: (title, body), or None if this kind is off.
pub fn notification(
    settings: &Settings,
    mirror: &Mirror,
    id: dawnload_core::model::ItemId,
    status: Status,
) -> Option<(String, String)> {
    let item = mirror.item(id)?;
    match status {
        Status::Kept | Status::Seeding if settings.app.notify_kept => {
            Some((format!("Kept in {}", item.folder_name), item.title.clone()))
        }
        Status::Failed if settings.app.notify_failed => {
            let why = item.failure.as_ref().map(|f| f.message.clone()).unwrap_or_else(|| "It failed.".into());
            Some((format!("Failed: {}", item.title), why))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dawnload_core::api::{Batch, Change, Counts, ItemView, QueueState, Snapshot, Update};
    use dawnload_core::model::{Item, ItemId, Options};
    use dawnload_core::source::DownloadSource;

    fn view(id: u64, status: Status) -> ItemView {
        let mut item = Item::new(
            ItemId(id),
            DownloadSource::parse(&format!("https://vimeo.com/{id}")).unwrap(),
            Options { destination: "/x/Dawnload".into(), ..Options::default() },
        );
        item.status = status;
        ItemView::of(&item)
    }

    fn mirror(items: Vec<ItemView>) -> Mirror {
        let mut m = Mirror::default();
        let counts = Counts::of(items.iter().map(|i| i.status));
        m.apply(Update::Snapshot(Snapshot { revision: 1, items, counts, queue: QueueState::default(), tools: vec![] }));
        m
    }

    #[test]
    fn rows_group_and_describe_items() {
        let m = mirror(vec![view(1, Status::Pulling), view(2, Status::Queued), view(3, Status::Queued), view(4, Status::Kept)]);
        let (rows, ids) = rows(&m, ListFilter::All);
        let headers: Vec<_> = rows.iter().filter(|r| r.kind == 0).map(|r| r.header.to_string()).collect();
        assert_eq!(headers, ["Now", "Up next", "Finished"]);
        assert_eq!(ids, [ItemId(1), ItemId(2), ItemId(3), ItemId(4)]);
        let pulling = &rows[1];
        assert!(pulling.loading, "no size yet: the loader");
        assert_eq!(pulling.a11y, "vimeo.com/1, Pulling");
        assert!(rows[3].subline.starts_with("Up next"));
        assert!(rows[4].subline.starts_with("2nd in line"));
        let (waiting, _) = super::rows(&m, ListFilter::Waiting);
        assert_eq!(waiting.iter().filter(|r| r.kind == 1).count(), 2);
    }

    #[test]
    fn the_queue_control_follows_the_queue() {
        assert_eq!(queue_label(&mirror(vec![view(1, Status::Queued)])), "Start Queue");
        assert_eq!(queue_label(&mirror(vec![view(1, Status::Pulling), view(2, Status::Queued)])), "Pause All");
        assert_eq!(queue_label(&mirror(vec![view(1, Status::Paused), view(2, Status::Kept)])), "Resume All");
        assert_eq!(queue_label(&mirror(vec![view(1, Status::Kept)])), "");
        // The Seeding tab appears only with something seeding.
        assert_eq!(filter_tabs(&mirror(vec![view(1, Status::Kept)])).len(), 5);
        assert_eq!(filter_tabs(&mirror(vec![view(1, Status::Seeding)])).len(), 6);
    }

    #[test]
    fn settings_round_trip_through_the_option_keys() {
        let mut s = Settings::default();
        for (key, value) in [
            ("format", 2),
            ("engine", 3),
            ("polish", 0),
            ("playlist", 1),
            ("impersonate", 4),
            ("connections", 3),
            ("lane", 2),
            ("retries", 0),
            ("appearance", 2),
            ("motion", 1),
            ("close-to-tray", 0),
            ("notify-failed", 0),
        ] {
            assert!(set_option(&mut s, key, value), "{key}");
        }
        assert!(!set_option(&mut s, "nonsense", 1));
        assert_eq!(s.defaults.quality, Quality::Audio);
        assert_eq!(s.defaults.engine, EnginePreference::Direct);
        assert_eq!(s.defaults.impersonate.as_deref(), Some("safari-18.4"));
        assert_eq!(s.defaults.connections, 8);
        assert_eq!(s.lane_limit(Lane::Web), 3);
        assert_eq!(s.auto_retries, 0);
        assert_eq!(s.app.appearance, Appearance::Light);
        let p = prefs(&s, true);
        assert_eq!((p.format, p.engine, p.impersonate, p.connections, p.lane, p.retries), (2, 3, 4, 3, 2, 0));
        assert_eq!((p.appearance, p.motion, p.close_to_tray, p.notify_failed, p.notify_kept), (2, 1, false, false, true));

        // The cookies chip only turns on with a file chosen.
        assert!(set_option(&mut s, "cookies", 1));
        assert_eq!(s.defaults.cookies, Cookies::None);
        s.cookies_file = Some("/c/cookies.txt".into());
        set_option(&mut s, "cookies", 1);
        assert_eq!(s.defaults.cookies, Cookies::File("/c/cookies.txt".into()));
        let c = chips(&s, None);
        assert!(c.cookies && c.cookies_available);
    }

    #[test]
    fn chips_say_what_auto_picked() {
        let s = Settings::default();
        assert_eq!(chips(&s, None).engine_label, "Auto");
        assert_eq!(chips(&s, Some(EngineId::YtDlp)).engine_label, "Auto · yt-dlp");
        assert_eq!(chips(&s, Some(EngineId::Direct)).engine_label, "Auto · Direct");
        let gallery = chips(&s, Some(EngineId::GalleryDl));
        assert!(!gallery.format_enabled, "gallery-dl keeps original files");
        assert_eq!(chips(&s, None).folder, "Dawnload");
    }

    fn status(tool: ToolId, found: bool, origin: Option<ToolOrigin>) -> ToolStatus {
        ToolStatus {
            tool,
            purpose: tool.purpose().into(),
            path: found.then(|| format!("/t/{}", tool.name()).into()),
            origin,
            version: found.then(|| "1.0".into()),
            latest: None,
            update_available: false,
            installable: true,
        }
    }

    #[test]
    fn tools_screen_and_pill() {
        let mut statuses = vec![
            status(ToolId::YtDlp, false, None),
            status(ToolId::GalleryDl, false, None),
            status(ToolId::Ffmpeg, true, Some(ToolOrigin::System)),
            status(ToolId::Ffprobe, true, Some(ToolOrigin::System)),
            status(ToolId::Deno, true, Some(ToolOrigin::Managed)),
        ];
        assert_eq!(tools_pill(&statuses), ("yt-dlp missing".to_string(), false));
        assert_eq!(missing_tool(&statuses), Some(ToolId::YtDlp));
        assert_eq!(missing_installable(&statuses), vec![ToolId::YtDlp]);
        let mut jobs = std::collections::BTreeMap::new();
        let rows = tool_rows(&statuses, &jobs, &Settings::default());
        assert_eq!(rows[0].name, "yt-dlp");
        assert_eq!(rows[0].action, "Install");
        assert_eq!(rows[0].status, status_code(Status::Failed));
        assert_eq!(rows[1].version, "1.0 · from the system");
        assert_eq!(rows[1].action, "Install Dawnload's Copy");
        assert_eq!(rows[2].action, "", "Dawnload's own deno, current");
        jobs.insert(ToolId::YtDlp, Job::Installing { done: 5_000_000, total: Some(10_000_000) });
        let rows = tool_rows(&statuses, &jobs, &Settings::default());
        assert!(rows[0].busy && (rows[0].fill - 0.5).abs() < 0.01);
        assert_eq!(rows[0].message, "Installing · 5.0 of 10.0 MB");
        jobs.insert(ToolId::YtDlp, Job::Failed("No checksum published; not installed.".into()));
        let rows = tool_rows(&statuses, &jobs, &Settings::default());
        assert!(rows[0].failed);

        statuses[0] = status(ToolId::YtDlp, true, Some(ToolOrigin::Managed));
        statuses[0].update_available = true;
        statuses[0].latest = Some("2.0".into());
        assert_eq!(tools_pill(&statuses), ("Tool updates".to_string(), true));
        assert_eq!(tool_rows(&statuses, &Default::default(), &Settings::default())[0].action, "Update");
    }

    #[test]
    fn notifications_follow_the_settings() {
        let mut m = mirror(vec![view(1, Status::Pulling)]);
        let mut kept = view(1, Status::Kept);
        kept.title = "Big Buck Bunny".into();
        let applied = m.apply(Update::Batch(Batch { revision: 2, changes: vec![Change::Upsert { item: kept }] }));
        assert_eq!(applied.finished, vec![(ItemId(1), Status::Kept)]);
        let mut s = Settings::default();
        assert_eq!(
            notification(&s, &m, ItemId(1), Status::Kept),
            Some(("Kept in Dawnload".to_string(), "Big Buck Bunny".to_string()))
        );
        s.app.notify_kept = false;
        assert_eq!(notification(&s, &m, ItemId(1), Status::Kept), None);
        assert_eq!(tray_tooltip(&mirror(vec![view(1, Status::Kept)])), "Dawnload");
        assert_eq!(
            tray_tooltip(&mirror(vec![view(1, Status::Pulling), view(2, Status::Queued)])),
            "Dawnload · 1 pulling · 1 waiting"
        );
    }
}

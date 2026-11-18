//! `dawnload`: the core in a terminal. It uses the same API as the app (snapshot + batched
//! diffs through a Mirror), so what you see here is what the app sees.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use dawnload_core::api::{Action, ListFilter, Mirror, Row, Update};
use dawnload_core::core::{Core, CoreConfig, CoreHandle};
use dawnload_core::explain::Tone;
use dawnload_core::model::{Checksum, Cookies, ItemId, Options, Quality, Status};
use dawnload_core::present;
use dawnload_core::seeding::{UploadCap, parse_ratio, parse_seed_days};
use dawnload_core::source::EnginePreference;
use dawnload_core::tools::{ToolId, ToolManager, ToolOrigin};
use dawnload_core::{format, paths};

#[derive(Parser)]
#[command(name = "dawnload", version, about = "Paste anything: video links, galleries, plain files. Dawnload keeps the file.")]
struct Cli {
    /// Where Dawnload keeps its state and tools (default: the app's data folder).
    #[arg(long, global = true, env = "DAWNLOAD_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add links to the queue.
    Add {
        #[arg(required = true)]
        links: Vec<String>,
        #[command(flatten)]
        options: DownloadArgs,
        /// Start the queue too (then `dawnload run` shows it).
        #[arg(long)]
        start: bool,
    },
    /// Add links and download them now, with progress. Exits when they're done.
    Get {
        #[arg(required = true)]
        links: Vec<String>,
        #[command(flatten)]
        options: DownloadArgs,
    },
    /// Say what would happen with a link, without adding it.
    Explain { text: String },
    /// Show the list.
    List {
        #[arg(long, value_enum, default_value = "all")]
        filter: FilterArg,
        /// Machine-readable output (the same JSON the remote API will speak).
        #[arg(long)]
        json: bool,
    },
    /// View or change default torrent seeding limits, courtesy mode and the upload cap.
    TorrentSettings {
        #[command(flatten)]
        limits: SeedArgs,
        /// Pause seeding on battery or a metered network.
        #[arg(long, value_enum)]
        courtesy: Option<OnOff>,
        /// auto, unlimited or a rate in bytes per second (at least 16384).
        #[arg(long)]
        upload_cap: Option<String>,
        /// Clear Auto's saved measurement and learn on the next sustained upload.
        #[arg(long)]
        relearn_upload: bool,
    },
    /// Change a queued, paused or failed torrent's seeding limits.
    SeedLimits {
        id: u64,
        #[command(flatten)]
        limits: SeedArgs,
    },
    /// Keep a seeding torrent's files and end its seeding job.
    StopSeeding { id: u64 },
    /// Run the queue until nothing is left. Ctrl-C saves the queue and exits.
    Run,
    /// Pause an item (or `all`).
    Pause { target: String },
    /// Resume a paused item, retry a failed one, or start a waiting one now (or `all`).
    Resume { target: String },
    /// Retry a failed item.
    Retry { id: u64 },
    /// Remove items from the list (kept files stay on disk).
    Remove {
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// Move a waiting item to a place in line (1 is up next).
    Move { id: u64, position: usize },
    /// Remove every kept or failed item from the list.
    Clear {
        #[arg(value_enum)]
        what: ClearArg,
    },
    /// Show an item's log.
    Log { id: u64 },
    /// yt-dlp, gallery-dl, ffmpeg and deno: status, install, update.
    Tools {
        #[command(subcommand)]
        command: Option<ToolsCommand>,
    },
}

#[derive(Subcommand)]
enum ToolsCommand {
    /// Where each tool is and which version (add --check to look for updates).
    Status {
        #[arg(long)]
        check: bool,
    },
    /// Install verified official builds into Dawnload's folder (default: whatever is missing).
    Install { tools: Vec<String> },
    /// Update the tools Dawnload installed.
    Update,
}

#[derive(Args, Clone, Default)]
struct DownloadArgs {
    /// best, compatible (MP4/M4A) or audio.
    #[arg(short, long, value_enum)]
    format: Option<FormatArg>,
    /// auto, yt-dlp, gallery-dl or direct.
    #[arg(short, long, value_enum)]
    engine: Option<EngineArg>,
    /// Skip the ffmpeg polish pass.
    #[arg(long)]
    no_polish: bool,
    /// Download the whole playlist the link belongs to.
    #[arg(long)]
    playlist: bool,
    /// Folder to keep files in.
    #[arg(short = 'o', long = "to")]
    to: Option<PathBuf>,
    /// A Netscape cookies.txt for sites that need you signed in.
    #[arg(long, conflicts_with = "cookies_from_browser")]
    cookies: Option<PathBuf>,
    /// Use a browser's cookies: firefox, chrome, safari, "chrome:Profile 1"…
    #[arg(long)]
    cookies_from_browser: Option<String>,
    /// Present as a browser (yt-dlp), e.g. chrome-146.
    #[arg(long)]
    impersonate: Option<String>,
    /// Verify a direct download: sha256:HEX, sha512:HEX or bare hex.
    #[arg(long)]
    checksum: Option<String>,
    /// Connections per direct download.
    #[arg(short = 'c', long)]
    connections: Option<u8>,
    #[command(flatten)]
    seeds: SeedArgs,
}

#[derive(Args, Clone, Default)]
struct SeedArgs {
    /// Stop seeding at this ratio. Use "off" to disable this limit.
    #[arg(long, conflicts_with = "seed_until_stopped")]
    seed_ratio: Option<String>,
    /// Stop seeding this many days after completion. Use "off" to disable this limit.
    #[arg(long, conflicts_with = "seed_until_stopped")]
    seed_days: Option<String>,
    /// Disable both seed limits.
    #[arg(long)]
    seed_until_stopped: bool,
}

impl SeedArgs {
    fn apply(&self, options: &mut Options) -> Result<(), String> {
        if self.seed_until_stopped {
            options.seed_ratio_limit = None;
            options.seed_time_limit_secs = None;
        }
        if let Some(ratio) = &self.seed_ratio {
            options.seed_ratio_limit = parse_ratio(ratio)?;
        }
        if let Some(days) = &self.seed_days {
            options.seed_time_limit_secs = parse_seed_days(days)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

fn parse_cap(value: &str) -> Result<UploadCap, String> {
    match value {
        "auto" => Ok(UploadCap::Auto),
        "unlimited" => Ok(UploadCap::Unlimited),
        _ => value
            .parse()
            .ok()
            .map(UploadCap::Fixed)
            .filter(|cap| cap.valid())
            .ok_or_else(|| "Use auto, unlimited or at least 16384 bytes per second.".into()),
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Best,
    Compatible,
    Audio,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineArg {
    Auto,
    YtDlp,
    GalleryDl,
    Direct,
}

#[derive(Clone, Copy, ValueEnum)]
enum FilterArg {
    All,
    Active,
    Waiting,
    Kept,
    Failed,
    Seeding,
}

#[derive(Clone, Copy, ValueEnum)]
enum ClearArg {
    Kept,
    Failed,
}

impl DownloadArgs {
    fn apply(&self, mut options: Options) -> Result<Options, String> {
        if let Some(f) = self.format {
            options.quality = match f {
                FormatArg::Best => Quality::Best,
                FormatArg::Compatible => Quality::Compatible,
                FormatArg::Audio => Quality::Audio,
            };
        }
        if let Some(e) = self.engine {
            options.engine = match e {
                EngineArg::Auto => EnginePreference::Auto,
                EngineArg::YtDlp => EnginePreference::YtDlp,
                EngineArg::GalleryDl => EnginePreference::GalleryDl,
                EngineArg::Direct => EnginePreference::Direct,
            };
        }
        if self.no_polish {
            options.polish = false;
        }
        if self.playlist {
            options.playlist = true;
        }
        if let Some(to) = &self.to {
            options.destination = std::path::absolute(to).map_err(|e| e.to_string())?;
        }
        if let Some(path) = &self.cookies {
            options.cookies = Cookies::File(std::path::absolute(path).map_err(|e| e.to_string())?);
        }
        if let Some(spec) = &self.cookies_from_browser {
            options.cookies = Cookies::Browser(spec.clone());
        }
        if let Some(target) = &self.impersonate {
            options.impersonate = Some(target.clone());
        }
        if let Some(text) = &self.checksum {
            options.checksum = Some(Checksum::parse(text).ok_or("That checksum isn't sha256:HEX, sha512:HEX or bare hex.")?);
        }
        if let Some(n) = self.connections {
            options.connections = n.clamp(1, 16);
        }
        self.seeds.apply(&mut options)?;
        Ok(options)
    }
}

fn filter_of(arg: FilterArg) -> ListFilter {
    match arg {
        FilterArg::All => ListFilter::All,
        FilterArg::Active => ListFilter::Active,
        FilterArg::Waiting => ListFilter::Waiting,
        FilterArg::Kept => ListFilter::Kept,
        FilterArg::Failed => ListFilter::Failed,
        FilterArg::Seeding => ListFilter::Seeding,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.data_dir.is_none()
        && let Some(note) = paths::migrate_legacy_data_dir()
    {
        eprintln!("{note}");
    }
    let data_dir = cli.data_dir.clone().unwrap_or_else(paths::data_dir);
    match run(cli.command, data_dir).await {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn start_core(data_dir: &std::path::Path) -> Result<CoreHandle, String> {
    let mut config = CoreConfig::new(data_dir.to_path_buf());
    config.restore_seeders = false;
    Core::start(config).map_err(|e| e.to_string())
}

fn parse_target(text: &str) -> Result<Option<ItemId>, String> {
    if text == "all" {
        return Ok(None);
    }
    text.parse().map(|n| Some(ItemId(n))).map_err(|_| format!("“{text}” isn't an item number or `all`."))
}

async fn run(command: Command, data_dir: PathBuf) -> Result<ExitCode, String> {
    match command {
        Command::Tools { command } => return tools(command.unwrap_or(ToolsCommand::Status { check: false }), &data_dir).await,
        Command::Explain { text } => {
            let core = start_core(&data_dir)?;
            match core.explain(&text, None).await {
                Some(e) => {
                    let mark = match e.tone {
                        Tone::Neutral => "",
                        Tone::Warning => "! ",
                        Tone::Failed => "✗ ",
                    };
                    println!("{mark}{}", e.text);
                }
                None => println!("Paste a link to a video, a playlist, a gallery, a file or a magnet."),
            }
            core.shutdown().await;
        }
        Command::Add { links, options, start } => {
            let core = start_core(&data_dir)?;
            let defaults = core.settings().await.ok_or("The core stopped.")?.defaults;
            let opts = options.apply(defaults)?;
            let mut failed = false;
            for link in &links {
                match core.add(link, Some(opts.clone()), start).await {
                    Ok(id) => println!("{id}\t{link}"),
                    Err(f) => {
                        failed = true;
                        eprintln!("{link}: {}", f.sentence());
                    }
                }
            }
            if start {
                println!("Added. Run `dawnload run` to download.");
            }
            core.shutdown().await;
            return Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS });
        }
        Command::Get { links, options } => {
            let core = start_core(&data_dir)?;
            let defaults = core.settings().await.ok_or("The core stopped.")?.defaults;
            let opts = options.apply(defaults)?;
            let mut ids = Vec::new();
            let mut failed = false;
            for link in &links {
                match core.add(link, Some(opts.clone()), false).await {
                    Ok(id) => ids.push(id),
                    Err(f) => {
                        failed = true;
                        eprintln!("{link}: {}", f.sentence());
                    }
                }
            }
            if ids.is_empty() {
                core.shutdown().await;
                return Ok(ExitCode::FAILURE);
            }
            for id in &ids {
                core.act(Action::Resume { id: *id }).await.ok();
            }
            let ok = follow(&core, Some(ids)).await;
            core.shutdown().await;
            return Ok(if ok && !failed { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        Command::TorrentSettings { limits, courtesy, upload_cap, relearn_upload } => {
            let cap = upload_cap.as_deref().map(parse_cap).transpose()?;
            let core = start_core(&data_dir)?;
            let mut settings = core.settings().await.ok_or("The core stopped.")?;
            limits.apply(&mut settings.defaults)?;
            if let Some(courtesy) = courtesy {
                settings.seeding.courtesy = matches!(courtesy, OnOff::On);
            }
            if let Some(cap) = cap {
                settings.seeding.upload_cap = cap;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "seed_ratio_limit": settings.defaults.seed_ratio_limit,
                    "seed_time_limit_secs": settings.defaults.seed_time_limit_secs,
                    "courtesy": settings.seeding.courtesy,
                    "upload_cap": settings.seeding.upload_cap,
                }))
                .map_err(|e| e.to_string())?
            );
            core.set_settings(settings).await;
            if relearn_upload {
                core.act(Action::RelearnTorrentUpload).await?;
            }
            core.shutdown().await;
        }
        Command::SeedLimits { id, limits } => {
            let core = start_core(&data_dir)?;
            let id = ItemId(id);
            let mut options = core.detail(id).await.ok_or("No such item.")?.item.options;
            limits.apply(&mut options)?;
            core.act(Action::SetSeedLimits { id, ratio: options.seed_ratio_limit, seconds: options.seed_time_limit_secs })
                .await?;
            core.shutdown().await;
        }
        Command::StopSeeding { id } => {
            let core = start_core(&data_dir)?;
            core.act(Action::StopSeeding { id: ItemId(id) }).await?;
            core.shutdown().await;
        }
        Command::Run => {
            let core = Core::start(CoreConfig::new(data_dir.clone())).map_err(|e| e.to_string())?;
            core.act(Action::ResumeAll).await.ok();
            core.act(Action::StartQueue).await.ok();
            let ok = follow(&core, None).await;
            core.shutdown().await;
            return Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        Command::List { filter, json } => {
            let core = start_core(&data_dir)?;
            let snapshot = core.snapshot().await.ok_or("The core stopped.")?;
            core.shutdown().await;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot).unwrap_or_default());
                return Ok(ExitCode::SUCCESS);
            }
            let mut mirror = Mirror::default();
            mirror.apply(Update::Snapshot(snapshot));
            let filter = filter_of(filter);
            let rows = mirror.rows(filter);
            if rows.is_empty() {
                println!("{}", filter.empty_line());
            }
            for row in rows {
                match row {
                    Row::Header { section, count } => println!("\n{} ({count})", section.title()),
                    Row::Item { id, queue_position } => {
                        let item = mirror.item(id).expect("item");
                        println!("  {:>4}  {:<9}  {}", id, item.status.word(), item.title);
                        println!("        {:<9}  {}", "", present::subline(item, queue_position));
                    }
                }
            }
        }
        Command::Pause { target } => {
            let core = start_core(&data_dir)?;
            let result = match parse_target(&target)? {
                Some(id) => core.act(Action::Pause { id }).await,
                None => core.act(Action::PauseAll).await,
            };
            core.shutdown().await;
            result?;
        }
        Command::Resume { target } => {
            let core = start_core(&data_dir)?;
            let result = match parse_target(&target)? {
                Some(id) => core.act(Action::Resume { id }).await,
                None => core.act(Action::ResumeAll).await,
            };
            // Without `run`, nothing downloads; say so rather than pretend.
            core.shutdown().await;
            result?;
            println!("Queued. Run `dawnload run` to download.");
        }
        Command::Retry { id } => {
            let core = start_core(&data_dir)?;
            let result = core.act(Action::Resume { id: ItemId(id) }).await;
            core.shutdown().await;
            result?;
            println!("Queued. Run `dawnload run` to download.");
        }
        Command::Remove { ids } => {
            let core = start_core(&data_dir)?;
            let result = core.act(Action::Remove { ids: ids.into_iter().map(ItemId).collect() }).await;
            core.shutdown().await;
            result?;
        }
        Command::Move { id, position } => {
            let core = start_core(&data_dir)?;
            let result = core.act(Action::Move { id: ItemId(id), index: position.saturating_sub(1) }).await;
            core.shutdown().await;
            result?;
        }
        Command::Clear { what } => {
            let core = start_core(&data_dir)?;
            let status = match what {
                ClearArg::Kept => Status::Kept,
                ClearArg::Failed => Status::Failed,
            };
            let result = core.act(Action::Clear { status }).await;
            core.shutdown().await;
            result?;
        }
        Command::Log { id } => {
            let core = start_core(&data_dir)?;
            let detail = core.detail(ItemId(id)).await;
            core.shutdown().await;
            let detail = detail.ok_or(format!("There's no item {id}."))?;
            for line in detail.log {
                println!("{line}");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Prints progress until `ids` are finished (or, with None, until the queue stops).
/// Ctrl-C pauses everything and returns. True when nothing failed.
async fn follow(core: &CoreHandle, ids: Option<Vec<ItemId>>) -> bool {
    let Some(mut sub) = core.subscribe().await else { return false };
    let tty = std::io::stdout().is_terminal();
    let mut mirror = Mirror::default();
    let mut last_status = std::collections::HashMap::new();
    let mut status_line = false;
    let mut first = true;
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        let update = tokio::select! {
            update = sub.recv() => match update { Some(u) => u, None => return false },
            _ = &mut ctrl_c => {
                if status_line { println!(); }
                println!("Saving the queue. `dawnload run` resumes it.");
                core.act(Action::PauseAll).await.ok();
                return false;
            }
        };
        mirror.apply(update);
        let watched: Vec<ItemId> = match &ids {
            Some(ids) => ids.clone(),
            None => mirror.order.clone(),
        };

        // Lines for state changes; one live line for progress.
        for id in &watched {
            let Some(item) = mirror.item(*id) else { continue };
            let state = (item.status, (item.status == Status::Seeding).then(|| item.phase_detail.clone()).flatten());
            if last_status.get(id) != Some(&state) {
                last_status.insert(*id, state);
                // `run` starts with the whole list; only say what happens from now on.
                if ids.is_none() && (item.status == Status::Queued || (first && !item.status.is_running())) {
                    continue;
                }
                if status_line {
                    print!("\r\x1b[2K");
                    status_line = false;
                }
                let detail = match item.status {
                    Status::Kept | Status::Failed | Status::Paused | Status::Seeding => {
                        format!(" · {}", present::subline(item, None))
                    }
                    _ => String::new(),
                };
                println!("[{id}] {:<9} {}{detail}", item.status.word(), item.title);
                if let (Status::Failed, Some(f)) = (item.status, &item.failure) {
                    if let Some(hint) = &f.hint {
                        println!("{:>13}{hint}", "");
                    }
                    if let Some(detail) = &f.detail {
                        println!("{:>13}{detail}", "");
                    }
                }
            }
        }
        first = false;
        let active: Vec<_> = watched.iter().filter_map(|id| mirror.item(*id)).filter(|i| i.status.is_running()).collect();
        if tty && let Some(first) = active.first() {
            let more = if active.len() > 1 { format!("  (+{} more)", active.len() - 1) } else { String::new() };
            let line = format!("  {} · {}{more}", truncate(&first.title, 40), present::subline(first, None));
            print!("\r\x1b[2K{line}");
            let _ = std::io::stdout().flush();
            status_line = true;
        }

        let done = match &ids {
            Some(ids) => ids
                .iter()
                .all(|id| mirror.item(*id).is_none_or(|i| matches!(i.status, Status::Kept | Status::Failed | Status::Paused))),
            None => !mirror.queue.running && mirror.counts.active == 0 && mirror.counts.seeding == 0,
        };
        if done {
            if status_line {
                println!();
            }
            return watched
                .iter()
                .filter_map(|id| mirror.item(*id))
                .all(|i| i.status != Status::Failed && (ids.is_none() || i.status != Status::Paused));
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max { text.to_string() } else { format!("{}…", text.chars().take(max - 1).collect::<String>()) }
}

async fn tools(command: ToolsCommand, data_dir: &std::path::Path) -> Result<ExitCode, String> {
    let manager = ToolManager::new(paths::tools_dir(data_dir));
    match command {
        ToolsCommand::Status { check } => {
            let statuses = if check { manager.check_updates().await } else { manager.status().await };
            for s in &statuses {
                let origin = match s.origin {
                    Some(ToolOrigin::Managed) => "Dawnload's copy",
                    Some(ToolOrigin::System) => "system",
                    Some(ToolOrigin::Custom) => "custom path",
                    None => "",
                };
                let state = match (&s.path, &s.version) {
                    (Some(path), version) => format!("{} ({origin}) {}", version.as_deref().unwrap_or("?"), path.display()),
                    (None, _) if s.installable => "not found · `dawnload tools install` adds it".to_string(),
                    (None, _) => "not found".to_string(),
                };
                let update = match (&s.latest, s.update_available, s.origin) {
                    (Some(latest), true, Some(ToolOrigin::Managed)) => format!(" → {latest} available (`dawnload tools update`)"),
                    (Some(latest), true, _) => {
                        format!(" → {latest} available (update it with your package manager, or install Dawnload's copy)")
                    }
                    (Some(_), false, Some(_)) => " · up to date".to_string(),
                    _ => String::new(),
                };
                println!("{:<11} {state}{update}", s.tool.name());
                println!("{:<11} {}", "", s.purpose);
            }
            let snapshot = manager.discover();
            match &snapshot.js_runtime {
                Some(runtime) => {
                    println!("\nJavaScript runtime for YouTube: {} ({})", runtime.kind.ytdlp_name(), runtime.path.display())
                }
                None => println!("\nYouTube needs a JavaScript runtime and none was found: `dawnload tools install deno`."),
            }
        }
        ToolsCommand::Install { tools } => {
            let wanted: Vec<ToolId> = if tools.is_empty() {
                let snapshot = manager.discover();
                let mut missing: Vec<ToolId> =
                    [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::GalleryDl].into_iter().filter(|t| !snapshot.has(*t)).collect();
                if snapshot.js_runtime.is_none() {
                    missing.push(ToolId::Deno);
                }
                if missing.is_empty() {
                    println!("Every tool is already here. `dawnload tools install yt-dlp` installs Dawnload's own copy anyway.");
                }
                missing
            } else {
                tools
                    .iter()
                    .map(|t| ToolId::parse(t).ok_or(format!("“{t}” isn't a tool Dawnload knows.")))
                    .collect::<Result<_, _>>()?
            };
            let mut failed = false;
            for tool in wanted {
                println!("Installing {}…", tool.name());
                let mut last = std::time::Instant::now();
                let mut progress = |t: ToolId, done: u64, total: Option<u64>| {
                    if last.elapsed().as_millis() > 200 {
                        last = std::time::Instant::now();
                        let of = total.map(|t| format!(" of {}", format::bytes(t))).unwrap_or_default();
                        print!("\r\x1b[2K  {} {}{of}", t.name(), format::bytes(done));
                        let _ = std::io::stdout().flush();
                    }
                };
                match manager.install(tool, &mut progress).await {
                    Ok(version) => println!("\r\x1b[2K  {} {version}: verified and installed.", tool.name()),
                    Err(e) => {
                        failed = true;
                        println!("\r\x1b[2K  {}: {e}", tool.name());
                    }
                }
            }
            return Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS });
        }
        ToolsCommand::Update => {
            let mut progress = |_: ToolId, _: u64, _: Option<u64>| {};
            let results = manager.update_managed(&mut progress).await;
            if results.is_empty() {
                println!("Dawnload hasn't installed any tools; system tools update with their package manager.");
            }
            let mut failed = false;
            for (tool, result) in results {
                match result {
                    Ok(Some(version)) => println!("{} updated to {version}.", tool.name()),
                    Ok(None) => println!("{} is up to date.", tool.name()),
                    Err(e) => {
                        failed = true;
                        println!("{}: {e}", tool.name());
                    }
                }
            }
            return Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS });
        }
    }
    Ok(ExitCode::SUCCESS)
}

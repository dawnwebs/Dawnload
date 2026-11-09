//! yt-dlp: video and audio from most sites, with Dawnload's progress markers, smart filenames and
//! an ffmpeg polish pass (faststart) afterwards.

use std::collections::HashSet;
use std::path::PathBuf;

use super::*;
use crate::filename;
use crate::finishing;
use crate::model::{Cookies, Quality};
use crate::process::{RunError, run_tool, shell_preview};
use crate::ytdlp::{self as markers, ProgressTracker, YtDlpLine};

pub struct YtDlpEngine;

impl DownloadEngine for YtDlpEngine {
    fn id(&self) -> EngineId {
        EngineId::YtDlp
    }

    fn lane(&self) -> Lane {
        Lane::Web
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::FORMATS | Capabilities::PLAYLISTS | Capabilities::SITE_ACCESS | Capabilities::POLISH | Capabilities::RESUME
    }

    fn accepts(&self, source: &DownloadSource) -> bool {
        matches!(source, DownloadSource::Web { .. })
    }

    fn availability(&self, tools: &ToolSnapshot, options: &Options) -> Availability {
        if !tools.has(ToolId::YtDlp) {
            return Availability::MissingTool(ToolId::YtDlp);
        }
        if (options.polish || options.quality == Quality::Audio) && !tools.has(ToolId::Ffmpeg) {
            return Availability::MissingTool(ToolId::Ffmpeg);
        }
        Availability::Ready
    }

    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture {
        Box::pin(run(request, context))
    }
}

/// The yt-dlp command line for one item.
pub fn arguments(url: &str, options: &Options, tools: &ToolSnapshot) -> Vec<String> {
    let ffmpeg = tools.path(ToolId::Ffmpeg);
    let mut args: Vec<String> = [
        "--newline",
        "--no-color",
        "--progress",
        "--ignore-config",
        "--no-update",
        "--no-mtime",
        "--continue",
    ]
    .map(String::from)
    .to_vec();
    args.extend(["-P".into(), options.destination.display().to_string()]);
    args.extend(["-o".into(), "%(title).200B [%(id)s].%(ext)s".into()]);
    args.extend(markers::marker_arguments());

    let polish = options.polish || options.quality == Quality::Audio;
    match (polish, ffmpeg) {
        (true, Some(ffmpeg)) => {
            args.extend(["--merge-output-format", "mp4", "--remux-video", "mp4", "--embed-metadata"].map(String::from));
            args.extend(["--ffmpeg-location".into(), ffmpeg.display().to_string()]);
            match options.quality {
                Quality::Best => args.extend(["-f", "bv*+ba/b"].map(String::from)),
                Quality::Compatible => args.extend(["-S", "res,ext:mp4:m4a", "-f", "bv*+ba/b"].map(String::from)),
                Quality::Audio => args.extend(["-x", "--audio-format", "m4a", "--audio-quality", "0"].map(String::from)),
            }
        }
        _ => match options.quality {
            Quality::Best | Quality::Audio => args.extend(["-f", "b"].map(String::from)),
            Quality::Compatible => args.extend(["-S", "res,ext:mp4:m4a", "-f", "b"].map(String::from)),
        },
    }

    // YouTube needs a JavaScript runtime (yt-dlp 2025.11+). Point yt-dlp at the one we found,
    // even when it isn't on PATH (Dawnload's managed deno).
    if let Some(runtime) = &tools.js_runtime {
        args.extend(["--js-runtimes".into(), format!("{}:{}", runtime.kind.ytdlp_name(), runtime.path.display())]);
    }
    if let Some(target) = &options.impersonate {
        args.extend(["--impersonate".into(), target.clone()]);
    }
    match &options.cookies {
        Cookies::File(path) => args.extend(["--cookies".into(), path.display().to_string()]),
        Cookies::Browser(spec) => args.extend(["--cookies-from-browser".into(), spec.clone()]),
        Cookies::None => {}
    }
    args.push(if options.playlist { "--yes-playlist" } else { "--no-playlist" }.into());
    args.push("--".into());
    args.push(url.to_string());
    args
}

async fn run(request: JobRequest, ctx: JobContext) -> JobResult {
    let DownloadSource::Web { url } = &request.source else { return Err(DownloadFailure::torrents_unavailable().into()) };
    let options = &request.options;
    let tools = &request.tools;
    let executable = tools.path(ToolId::YtDlp).ok_or_else(|| DownloadFailure::tool_missing("yt-dlp"))?.to_path_buf();
    if options.quality == Quality::Audio && !tools.has(ToolId::Ffmpeg) {
        return Err(DownloadFailure::ffmpeg_needed().into());
    }
    if options.polish && !tools.has(ToolId::Ffmpeg) {
        return Err(DownloadFailure::tool_missing("ffmpeg").into());
    }
    if let Cookies::File(path) = &options.cookies {
        if std::fs::File::open(path).is_err() {
            return Err(DownloadFailure::cookies_unreadable().into());
        }
    }
    let destination = options.destination.clone();
    std::fs::create_dir_all(&destination).map_err(|_| DownloadFailure::folder_unavailable(&destination.display().to_string()))?;

    let args = arguments(url, options, tools);
    let before = finishing::snapshot(&destination, finishing::MEDIA_EXTENSIONS);
    ctx.emit(JobEvent::Command(shell_preview(&executable, &args)));
    ctx.phase(Status::Pulling, None);
    if crate::source::is_youtube(&request.source) && tools.js_runtime.is_none() {
        ctx.log("No JavaScript runtime found; YouTube may offer fewer formats or fail. Install deno in Settings › Tools.");
    }

    let mut tracker = ProgressTracker::new();
    let mut metadata = Vec::new();
    let result = run_tool(&executable, &args, &tools.env, &destination, &ctx.stop, |line| {
        let parsed = YtDlpLine::parse(line);
        match &parsed {
            YtDlpLine::Item(info) => {
                tracker.consume(&parsed);
                if let Some(title) = info.item_title() {
                    ctx.emit(JobEvent::Title(title.to_string()));
                }
                if let (Some(index), Some(count)) = (info.playlist_index, info.entry_count) {
                    ctx.log(format!("Video {index} of {count}: {}", info.title.as_deref().or(info.id.as_deref()).unwrap_or("")));
                }
                ctx.phase(Status::Pulling, None);
            }
            YtDlpLine::Parts(_) => {
                tracker.consume(&parsed);
            }
            YtDlpLine::Progress(_) => {
                if let Some(sample) = tracker.consume(&parsed) {
                    ctx.emit(JobEvent::Transfer(sample));
                }
            }
            YtDlpLine::Postprocess { started, name, is_polish } => {
                if *started && *is_polish {
                    ctx.phase(Status::Polishing, Some(markers::polish_description(name)));
                    ctx.log(format!("Polishing: {name}"));
                }
            }
            YtDlpLine::Metadata(m) => metadata.push((**m).clone()),
            YtDlpLine::Text(text) => ctx.log(text.clone()),
        }
    })
    .await;

    let output = match result {
        Ok(output) => output,
        Err(RunError::Stopped) => return Err(JobError::Stopped),
        Err(RunError::Spawn(e)) => return Err(DownloadFailure::tool_missing("yt-dlp").with_detail(e.to_string()).into()),
    };
    if output.exit_code != 0 {
        return Err(DownloadFailure::classify("yt-dlp", output.exit_code, &output.output).into());
    }

    let after = finishing::snapshot(&destination, finishing::MEDIA_EXTENSIONS);
    let mut new_files: Vec<PathBuf> = after.difference(&before).cloned().collect();
    new_files.sort();
    let files = if new_files.is_empty() {
        // Already on disk: yt-dlp skipped the download. The files it reported are still this item's.
        let existing: Vec<PathBuf> = dedup(metadata.iter().filter_map(|m| m.filepath.as_deref()).map(PathBuf::from).filter(|p| p.exists()));
        if !existing.is_empty() {
            ctx.log("Already in the folder; nothing new to download.");
        }
        existing
    } else {
        filename::apply_smart_names(&new_files, &after, &metadata, url, &mut |line| ctx.log(line))
    };

    if files.len() == 1 {
        if let Some(stem) = files[0].file_stem() {
            ctx.emit(JobEvent::Title(stem.to_string_lossy().into_owned()));
        }
    }

    let mut polished = 0;
    if options.polish {
        if let Some(ffmpeg) = tools.path(ToolId::Ffmpeg) {
            polished = finishing::polish(&files, ffmpeg, &tools.env, &ctx).await?;
        }
    }
    Ok(JobOutcome { files, polished })
}

fn dedup(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths.filter(|p| seen.insert(p.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{JsRuntime, JsRuntimeKind};

    fn tools(ffmpeg: bool) -> ToolSnapshot {
        let mut t = ToolSnapshot::default();
        t.paths.insert(ToolId::YtDlp, "/bin/yt-dlp".into());
        if ffmpeg {
            t.paths.insert(ToolId::Ffmpeg, "/bin/ffmpeg".into());
        }
        t
    }

    fn joined(args: &[String]) -> String {
        args.join(" ")
    }

    #[test]
    fn best_with_polish() {
        let options = Options { destination: "/dl".into(), ..Options::default() };
        let args = arguments("https://youtu.be/x", &options, &tools(true));
        let text = joined(&args);
        assert!(text.contains("-P /dl"));
        assert!(text.contains("--merge-output-format mp4 --remux-video mp4 --embed-metadata --ffmpeg-location /bin/ffmpeg -f bv*+ba/b"));
        assert!(text.ends_with("--no-playlist -- https://youtu.be/x"));
        assert!(!text.contains("--js-runtimes"));
    }

    #[test]
    fn audio_compatible_playlist_cookies_and_runtime() {
        let mut t = tools(true);
        t.js_runtime = Some(JsRuntime { kind: JsRuntimeKind::Deno, path: "/tools/deno".into() });
        let options = Options {
            quality: Quality::Audio,
            playlist: true,
            impersonate: Some("chrome-146".into()),
            cookies: Cookies::Browser("firefox".into()),
            ..Options::default()
        };
        let text = joined(&arguments("https://x.test/v", &options, &t));
        assert!(text.contains("-x --audio-format m4a --audio-quality 0"));
        assert!(text.contains("--js-runtimes deno:/tools/deno"));
        assert!(text.contains("--impersonate chrome-146"));
        assert!(text.contains("--cookies-from-browser firefox"));
        assert!(text.contains("--yes-playlist"));

        let options = Options { quality: Quality::Compatible, polish: false, cookies: Cookies::File("/c.txt".into()), ..Options::default() };
        let text = joined(&arguments("https://x.test/v", &options, &tools(false)));
        assert!(text.contains("-S res,ext:mp4:m4a -f b"));
        assert!(text.contains("--cookies /c.txt"));
        assert!(!text.contains("--merge-output-format"));
    }

    #[test]
    fn availability() {
        let engine = YtDlpEngine;
        assert_eq!(engine.availability(&ToolSnapshot::default(), &Options::default()), Availability::MissingTool(ToolId::YtDlp));
        assert_eq!(engine.availability(&tools(false), &Options::default()), Availability::MissingTool(ToolId::Ffmpeg));
        let no_polish = Options { polish: false, ..Options::default() };
        assert_eq!(engine.availability(&tools(false), &no_polish), Availability::Ready);
        assert_eq!(engine.availability(&tools(true), &Options::default()), Availability::Ready);
    }
}

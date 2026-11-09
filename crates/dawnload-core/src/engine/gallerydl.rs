//! gallery-dl: galleries and image sets. It prints each file it saves as a path (and `# path`
//! for ones already there), which is how Dawnload follows along.

use std::path::{Path, PathBuf};

use super::*;
use crate::finishing;
use crate::format;
use crate::model::Cookies;
use crate::process::{RunError, run_tool, shell_preview};

pub struct GalleryDlEngine;

impl DownloadEngine for GalleryDlEngine {
    fn id(&self) -> EngineId {
        EngineId::GalleryDl
    }

    fn lane(&self) -> Lane {
        Lane::Web
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::POLISH | Capabilities::RESUME | Capabilities::SITE_ACCESS
    }

    fn accepts(&self, source: &DownloadSource) -> bool {
        matches!(source, DownloadSource::Web { .. })
    }

    fn availability(&self, tools: &ToolSnapshot, _options: &Options) -> Availability {
        if tools.has(ToolId::GalleryDl) { Availability::Ready } else { Availability::MissingTool(ToolId::GalleryDl) }
    }

    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture {
        Box::pin(run(request, context))
    }
}

pub fn arguments(url: &str, options: &Options, ffmpeg_available: bool) -> Vec<String> {
    let mut args: Vec<String> = vec!["--no-colors".into(), "--no-mtime".into(), "--destination".into(), options.destination.display().to_string()];
    if options.polish && ffmpeg_available {
        args.extend(["--ugoira".into(), "mp4".into()]);
    }
    match &options.cookies {
        Cookies::File(path) => args.extend(["--cookies".into(), path.display().to_string()]),
        Cookies::Browser(spec) => args.extend(["--cookies-from-browser".into(), spec.clone()]),
        Cookies::None => {}
    }
    args.push("--".into());
    args.push(url.to_string());
    args
}

/// "instagram / someone" from …/Dawnload/instagram/someone/file.jpg.
pub fn title_for(file: &Path, destination: &Path) -> String {
    let Some(folder) = file.parent() else { return file.display().to_string() };
    match folder.strip_prefix(destination) {
        Ok(relative) => {
            let parts: Vec<String> = relative.components().take(2).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            if parts.is_empty() { file.file_name().unwrap_or_default().to_string_lossy().into_owned() } else { parts.join(" / ") }
        }
        Err(_) => folder.file_name().unwrap_or_default().to_string_lossy().into_owned(),
    }
}

async fn run(request: JobRequest, ctx: JobContext) -> JobResult {
    let DownloadSource::Web { url } = &request.source else { return Err(DownloadFailure::torrents_unavailable().into()) };
    let options = &request.options;
    let tools = &request.tools;
    let executable = tools.path(ToolId::GalleryDl).ok_or_else(|| DownloadFailure::tool_missing("gallery-dl"))?.to_path_buf();
    let destination = options.destination.clone();
    std::fs::create_dir_all(&destination).map_err(|_| DownloadFailure::folder_unavailable(&destination.display().to_string()))?;

    let ffmpeg = tools.path(ToolId::Ffmpeg).map(Path::to_path_buf);
    let args = arguments(url, options, ffmpeg.is_some());
    ctx.emit(JobEvent::Command(shell_preview(&executable, &args)));
    ctx.phase(Status::Pulling, None);

    let mut files: Vec<PathBuf> = Vec::new();
    let mut bytes = 0u64;
    let result = run_tool(&executable, &args, &tools.env, &destination, &ctx.stop, |line| {
        let path_text = line.strip_prefix("# ").unwrap_or(line);
        let path = Path::new(path_text);
        if path.is_absolute() && !files.iter().any(|f| f == path) {
            files.push(path.to_path_buf());
            bytes += std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if files.len() == 1 {
                ctx.emit(JobEvent::Title(title_for(path, &destination)));
            }
            ctx.emit(JobEvent::Files(files.iter().map(ItemFile::on_disk).collect()));
            ctx.emit(JobEvent::Transfer(Transfer { bytes_done: Some(bytes), ..Transfer::default() }));
            ctx.phase(Status::Pulling, Some(&format::count(files.len(), "file")));
        } else {
            ctx.log(line.to_string());
        }
    })
    .await;

    let output = match result {
        Ok(output) => output,
        Err(RunError::Stopped) => return Err(JobError::Stopped),
        Err(RunError::Spawn(e)) => return Err(DownloadFailure::tool_missing("gallery-dl").with_detail(e.to_string()).into()),
    };
    if output.exit_code != 0 {
        if files.is_empty() {
            return Err(DownloadFailure::classify("gallery-dl", output.exit_code, &output.output).into());
        }
        ctx.log(format!("gallery-dl finished with exit {}; {} were kept.", output.exit_code, format::count(files.len(), "file")));
    }

    let mut polished = 0;
    if options.polish {
        if let Some(ffmpeg) = &ffmpeg {
            polished = finishing::polish(&files, ffmpeg, &tools.env, &ctx).await?;
        }
    }
    Ok(JobOutcome { files, polished })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_and_titles() {
        let options = Options { destination: "/dl".into(), cookies: Cookies::File("/c.txt".into()), ..Options::default() };
        assert_eq!(
            arguments("https://imgur.com/a/x", &options, true).join(" "),
            "--no-colors --no-mtime --destination /dl --ugoira mp4 --cookies /c.txt -- https://imgur.com/a/x"
        );
        assert_eq!(title_for(Path::new("/dl/instagram/someone/1.jpg"), Path::new("/dl")), "instagram / someone");
        assert_eq!(title_for(Path::new("/dl/a/b/c/1.jpg"), Path::new("/dl")), "a / b");
        assert_eq!(title_for(Path::new("/dl/1.jpg"), Path::new("/dl")), "1.jpg");
    }
}

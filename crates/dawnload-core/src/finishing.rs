//! What happens to files after a web engine pulls them: find the new ones, and polish MP4s with
//! ffmpeg (faststart: the index moves to the front, so they play before they finish loading).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::engine::{JobContext, JobError};
use crate::model::Status;
use crate::process::{RunError, run_tool};

pub const MEDIA_EXTENSIONS: &[&str] =
    &["mp4", "m4v", "mov", "webm", "mkv", "avi", "flv", "mp3", "m4a", "aac", "opus", "ogg", "flac", "wav"];

/// Media files under `dir` (recursively, skipping hidden files and folders).
pub fn snapshot(dir: &Path, extensions: &[&str]) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() => {
                    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    if extensions.contains(&ext.as_str()) {
                        out.insert(path);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Rewrites MP4s with the index at the front. A file ffmpeg can't polish is kept as it is.
pub async fn polish(files: &[PathBuf], ffmpeg: &Path, env: &[(String, String)], ctx: &JobContext) -> Result<u32, JobError> {
    let candidates: Vec<&PathBuf> = files
        .iter()
        .filter(|f| {
            f.extension().is_some_and(|e| ["mp4", "m4v", "mov"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        })
        .collect();
    let mut count = 0;
    for (index, file) in candidates.iter().enumerate() {
        let detail = if candidates.len() > 1 {
            format!("faststart, file {} of {}", index + 1, candidates.len())
        } else {
            "moving the index to the front".to_string()
        };
        ctx.phase(Status::Polishing, Some(&detail));
        let folder = file.parent().unwrap_or(Path::new("."));
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        let ext = file.extension().unwrap_or_default().to_string_lossy();
        let temp = folder.join(format!(".{stem}.dawnload-polish.{ext}"));
        let args: Vec<String> = vec![
            "-y".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            file.display().to_string(),
            "-map".into(),
            "0".into(),
            "-c".into(),
            "copy".into(),
            "-movflags".into(),
            "+faststart".into(),
            temp.display().to_string(),
        ];
        let result = run_tool(ffmpeg, &args, env, folder, &ctx.stop, |line| ctx.log(line.to_string())).await;
        match result {
            Ok(out) if out.exit_code == 0 => match replace(file, &temp) {
                Ok(()) => count += 1,
                Err(e) => {
                    let _ = std::fs::remove_file(&temp);
                    ctx.log(format!("Skipped faststart for {}: {e}", file.display()));
                }
            },
            Ok(out) => {
                let _ = std::fs::remove_file(&temp);
                ctx.log(format!("Skipped faststart for {} (ffmpeg exit {})", file.display(), out.exit_code));
            }
            Err(RunError::Stopped) => {
                let _ = std::fs::remove_file(&temp);
                return Err(JobError::Stopped);
            }
            Err(RunError::Spawn(e)) => {
                let _ = std::fs::remove_file(&temp);
                ctx.log(format!("Skipped faststart: {e}"));
            }
        }
    }
    Ok(count)
}

/// Swaps `file` for `replacement`, keeping the original if anything goes wrong.
fn replace(file: &Path, replacement: &Path) -> std::io::Result<()> {
    let backup = file.with_extension(format!("{}.dawnload-backup", file.extension().unwrap_or_default().to_string_lossy()));
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(file, &backup)?;
    match std::fs::rename(replacement, file) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::rename(&backup, file);
            Err(e)
        }
    }
}

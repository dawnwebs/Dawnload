//! Smart filenames: a real title instead of `77ii8 [77ii8].mp4`, safe on every OS.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;
use url::Url;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SmartFilenameMetadata {
    pub id: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "fulltitle")]
    pub full_title: Option<String>,
    pub display_id: Option<String>,
    pub uploader: Option<String>,
    pub upload_date: Option<String>,
    #[serde(rename = "webpage_url")]
    pub webpage_url: Option<String>,
    pub filepath: Option<String>,
}

/// The best human title for a file: the extractor's title, unless it's just the id; then a slug
/// from the URL; then uploader and date; then "Video <id>".
pub fn best_title(metadata: &SmartFilenameMetadata, source_url: &str) -> String {
    let rejected: HashSet<String> = [&metadata.id, &metadata.display_id, &metadata.uploader]
        .into_iter()
        .map(|v| normalized_key(v.as_deref()))
        .filter(|k| !k.is_empty())
        .collect();

    for candidate in [&metadata.full_title, &metadata.title] {
        if let Some(title) = meaningful_title(candidate.as_deref(), &rejected) {
            return title;
        }
    }
    for url in [metadata.webpage_url.as_deref(), Some(source_url)] {
        if let Some(title) = title_slug(url).and_then(|slug| meaningful_title(Some(&slug), &rejected)) {
            return title;
        }
    }
    if let (Some(uploader), Some(date)) =
        (meaningful_title(metadata.uploader.as_deref(), &HashSet::new()), display_date(metadata.upload_date.as_deref()))
    {
        return format!("{uploader} — {date}");
    }
    if let Some(id) = cleaned(metadata.id.as_deref().or(metadata.display_id.as_deref())) {
        return format!("Video {id}");
    }
    "Downloaded Video".to_string()
}

fn meaningful_title(value: Option<&str>, rejected: &HashSet<String>) -> Option<String> {
    let title = cleaned(value)?;
    if title.chars().count() < 2 {
        return None;
    }
    let key = normalized_key(Some(&title));
    const GENERIC: [&str; 6] = ["download", "downloadedvideo", "media", "untitled", "video", "watch"];
    if key.is_empty() || GENERIC.contains(&key.as_str()) || rejected.contains(&key) || looks_like_url(&title) {
        return None;
    }
    Some(title)
}

fn title_slug(url: Option<&str>) -> Option<String> {
    let url = Url::parse(url?).ok()?;
    let segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    const MARKERS: [&str; 15] = [
        "clip", "clips", "play", "post", "posts", "reel", "reels", "short", "shorts", "status", "statuses", "v", "video",
        "videos", "watch",
    ];
    for i in 0..segments.len().saturating_sub(1) {
        if MARKERS.contains(&segments[i].to_lowercase().as_str()) {
            return readable_slug(segments[i + 1]);
        }
    }
    None
}

fn readable_slug(slug: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(slug).decode_utf8_lossy();
    let separated: String = decoded.chars().map(|c| if matches!(c, '-' | '_' | '+') { ' ' } else { c }).collect();
    let cleaned = cleaned(Some(&separated))?;
    Some(cleaned.split(' ').map(format_slug_word).collect::<Vec<_>>().join(" "))
}

fn format_slug_word(word: &str) -> String {
    let has_number = word.chars().any(|c| c.is_numeric());
    let only_letters = word.chars().all(|c| c.is_alphabetic());
    if has_number || (only_letters && word.chars().count() <= 3) {
        return word.to_uppercase();
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
        None => String::new(),
    }
}

/// NFC, whitespace collapsed, trimmed of whitespace and `.-_`.
fn cleaned(value: Option<&str>) -> Option<String> {
    let value: String = value?.nfc().collect();
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| c == '.' || c == '-' || c == '_' || c.is_whitespace());
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalized_key(value: Option<&str>) -> String {
    value.unwrap_or_default().to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
}

fn display_date(date: Option<&str>) -> Option<String> {
    let date = date?;
    (date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..]))
}

/// A file name component that is valid on Windows, macOS and Linux, whichever OS the files end
/// up on: no `<>:"/\|?*` or control characters, no trailing dots or spaces, no reserved names.
pub fn safe_component(value: &str) -> String {
    let replaced: String = value
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { ' ' } else { c })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
        "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return format!("{trimmed} file");
    }
    trimmed
}

/// [`safe_component`], cut to `byte_limit` UTF-8 bytes on a character boundary.
pub fn limited_component(value: &str, byte_limit: usize) -> String {
    let mut result = safe_component(value);
    while result.len() > byte_limit {
        result.pop();
    }
    result.trim_end_matches(|c: char| c == '.' || c.is_whitespace()).to_string()
}

/// `name.ext`, or `name 2.ext`, `name 3.ext`… when it's taken on disk or reserved.
pub fn unique_path(desired: &Path, reserved: &HashSet<PathBuf>) -> PathBuf {
    let available = |p: &Path| !reserved.contains(p) && !p.exists();
    if available(desired) {
        return desired.to_path_buf();
    }
    let folder = desired.parent().unwrap_or(Path::new("."));
    let stem = desired.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = desired.extension().map(|s| s.to_string_lossy().into_owned());
    (2..)
        .map(|i| match &ext {
            Some(ext) => folder.join(format!("{stem} {i}.{ext}")),
            None => folder.join(format!("{stem} {i}")),
        })
        .find(|p| available(p))
        .expect("an unused name")
}

/// Renames freshly downloaded files to their smart titles. Returns the final paths, sorted.
pub fn apply_smart_names(
    files: &[PathBuf],
    existing: &HashSet<PathBuf>,
    metadata: &[SmartFilenameMetadata],
    source_url: &str,
    log: &mut dyn FnMut(String),
) -> Vec<PathBuf> {
    if metadata.is_empty() {
        let mut out = files.to_vec();
        out.sort();
        return out;
    }
    let mut reserved = existing.clone();
    let mut remaining = metadata.to_vec();
    let mut renamed = Vec::new();
    let single = files.len() == 1 && remaining.len() == 1;

    for file in files {
        reserved.remove(file);
        let Some(index) = matching_metadata(file, &remaining, single) else {
            reserved.insert(file.clone());
            renamed.push(file.clone());
            continue;
        };
        let meta = remaining.remove(index);
        let name = limited_component(&best_title(&meta, source_url), 180);
        let ext = file.extension().map(|e| e.to_string_lossy().into_owned());
        if name.is_empty() {
            reserved.insert(file.clone());
            renamed.push(file.clone());
            continue;
        }
        let folder = file.parent().unwrap_or(Path::new("."));
        let desired = match &ext {
            Some(ext) => folder.join(format!("{name}.{ext}")),
            None => folder.join(&name),
        };
        if desired == *file {
            reserved.insert(file.clone());
            renamed.push(file.clone());
            continue;
        }
        let target = unique_path(&desired, &reserved);
        match std::fs::rename(file, &target) {
            Ok(()) => {
                log(format!("Named {}", target.file_name().unwrap_or_default().to_string_lossy()));
                reserved.insert(target.clone());
                renamed.push(target);
            }
            Err(error) => {
                log(format!("Couldn't rename {}: {error}", file.display()));
                reserved.insert(file.clone());
                renamed.push(file.clone());
            }
        }
    }
    renamed.sort();
    renamed
}

fn matching_metadata(file: &Path, records: &[SmartFilenameMetadata], single_fallback: bool) -> Option<usize> {
    if let Some(i) = records.iter().position(|m| m.filepath.as_deref().is_some_and(|p| Path::new(p) == file)) {
        return Some(i);
    }
    let stem = normalized_key(file.file_stem().and_then(|s| s.to_str()));
    if let Some(i) = records.iter().position(|m| {
        let id = normalized_key(m.id.as_deref());
        !id.is_empty() && stem.contains(&id)
    }) {
        return Some(i);
    }
    (single_fallback && !records.is_empty()).then_some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, title: &str) -> SmartFilenameMetadata {
        SmartFilenameMetadata { id: Some(id.into()), title: Some(title.into()), ..Default::default() }
    }

    #[test]
    fn uses_real_extractor_title() {
        let m = SmartFilenameMetadata {
            webpage_url: Some("https://spankbang.com/77ii8/video/a+proper+page+title".into()),
            ..meta("77ii8", "A Proper Page Title")
        };
        assert_eq!(best_title(&m, ""), "A Proper Page Title");
    }

    #[test]
    fn rejects_id_title_and_uses_url_slug() {
        let m = SmartFilenameMetadata {
            display_id: Some("77ii8".into()),
            webpage_url: Some("https://spankbang.com/77ii8/video/fc2+ppv+1106157+20+h+w".into()),
            ..meta("77ii8", "77ii8")
        };
        assert_eq!(best_title(&m, ""), "FC2 PPV 1106157 20 H W");
    }

    #[test]
    fn uses_source_url_when_metadata_url_has_no_slug() {
        let m =
            SmartFilenameMetadata { webpage_url: Some("https://spankbang.com/77ii8/video/".into()), ..meta("77ii8", "77ii8") };
        assert_eq!(best_title(&m, "https://spankbang.com/77ii8/video/better+fallback"), "Better Fallback");
    }

    #[test]
    fn falls_back_to_identifier_then_uploader_and_date() {
        assert_eq!(best_title(&meta("77ii8", "77ii8"), ""), "Video 77ii8");
        let m = SmartFilenameMetadata {
            uploader: Some("Road Studio".into()),
            upload_date: Some("20260724".into()),
            ..meta("abc123", "abc123")
        };
        assert_eq!(best_title(&m, ""), "Road Studio — 2026-07-24");
        assert_eq!(best_title(&SmartFilenameMetadata::default(), ""), "Downloaded Video");
    }

    #[test]
    fn safe_on_every_os() {
        assert_eq!(safe_component("a/b:c*d?\"e\"<f>|g\\h"), "a b c d e f g h");
        assert_eq!(safe_component("  Title...  "), "Title");
        assert_eq!(safe_component("CON"), "CON file");
        assert_eq!(safe_component("con.mp4"), "con.mp4 file");
        assert_eq!(safe_component("line\nbreak\ttab"), "line break tab");
        let long = "é".repeat(200);
        let cut = limited_component(&long, 180);
        assert!(cut.len() <= 180 && cut.chars().all(|c| c == 'é'));
    }

    #[test]
    fn renames_on_disk_without_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("Big Buck Bunny.mp4");
        std::fs::write(&taken, b"old").unwrap();
        let file = dir.path().join("abc [xyz].mp4");
        std::fs::write(&file, b"new").unwrap();
        let mut logs = Vec::new();
        let existing: HashSet<PathBuf> = [taken.clone(), file.clone()].into();
        let out = apply_smart_names(std::slice::from_ref(&file), &existing, &[meta("xyz", "Big Buck Bunny")], "", &mut |l| {
            logs.push(l)
        });
        assert_eq!(out, vec![dir.path().join("Big Buck Bunny 2.mp4")]);
        assert_eq!(std::fs::read(&out[0]).unwrap(), b"new");
        assert_eq!(logs, vec!["Named Big Buck Bunny 2.mp4".to_string()]);
    }
}

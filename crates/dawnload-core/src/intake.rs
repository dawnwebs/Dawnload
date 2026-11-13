//! Turning what people paste or drop into things to download: several links in one paste, a
//! dropped `.torrent`, a browser's link file (`.url` on Windows, `.webloc` on macOS, a `.desktop`
//! link on Linux), or a text file with links in it.

use std::path::Path;

use crate::source::DownloadSource;

/// Every downloadable thing in `text`, in order, without repeats. Links are separated by
/// whitespace (a list pasted from a page or a text file). A single entry that isn't a link comes
/// back as it is, so the detection line can say why it isn't one.
pub fn links_in_text(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut links: Vec<String> = Vec::new();
    for word in trimmed.split_whitespace() {
        let word = word.trim_matches(|c: char| matches!(c, '<' | '>' | '"' | '\'' | ',' | ';' | '(' | ')'));
        if DownloadSource::parse(word).is_some() && !links.iter().any(|l| l == word) {
            links.push(word.to_string());
        }
    }
    if links.is_empty() {
        // A path with spaces ("C:\My Files\a.torrent") or not a link at all: keep it whole.
        return vec![trimmed.to_string()];
    }
    links
}

/// What a file dropped on the window (or opened with Dawnload) asks for. `.torrent` files are
/// themselves the source; link files and text files contribute the links inside them.
pub fn links_from_file(path: &Path) -> Vec<String> {
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if extension == "torrent" {
        return vec![path.display().to_string()];
    }
    if !matches!(extension.as_str(), "url" | "webloc" | "desktop" | "txt" | "text" | "") {
        return Vec::new();
    }
    let Ok(meta) = std::fs::metadata(path) else { return Vec::new() };
    if !meta.is_file() || meta.len() > 1 << 20 {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let text = String::from_utf8_lossy(&bytes);
    match extension.as_str() {
        // [InternetShortcut]\nURL=https://…  (Windows) or [Desktop Entry]\nType=Link\nURL=…
        "url" | "desktop" => text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("URL=").map(str::trim))
            .filter(|u| DownloadSource::parse(u).is_some())
            .take(1)
            .map(String::from)
            .collect(),
        // A plist with <key>URL</key><string>https://…</string>.
        "webloc" => text
            .split("<string>")
            .skip(1)
            .filter_map(|s| s.split("</string>").next())
            .map(|s| s.trim().replace("&amp;", "&"))
            .filter(|u| DownloadSource::parse(u).is_some())
            .take(1)
            .collect(),
        _ => links_in_text(&text).into_iter().filter(|l| DownloadSource::parse(l).is_some()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_links_in_one_paste() {
        assert_eq!(
            links_in_text("https://vimeo.com/1\n  https://youtu.be/abc  https://vimeo.com/1, <https://x.org/a.zip>"),
            vec!["https://vimeo.com/1", "https://youtu.be/abc", "https://x.org/a.zip"]
        );
        assert_eq!(links_in_text("not a link"), vec!["not a link"]);
        assert!(links_in_text("   ").is_empty());
        let magnet = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=x";
        assert_eq!(links_in_text(magnet), vec![magnet]);
    }

    #[test]
    fn dropped_files() {
        let dir = tempfile::tempdir().unwrap();
        let torrent = dir.path().join("Ubuntu 24.04.torrent");
        std::fs::write(&torrent, b"d4:infod4:name1:xee").unwrap();
        assert_eq!(links_from_file(&torrent), vec![torrent.display().to_string()]);

        let url = dir.path().join("Video.url");
        std::fs::write(&url, "[InternetShortcut]\r\nURL=https://vimeo.com/76979871\r\n").unwrap();
        assert_eq!(links_from_file(&url), vec!["https://vimeo.com/76979871"]);

        let webloc = dir.path().join("Video.webloc");
        std::fs::write(
            &webloc,
            r#"<?xml version="1.0"?><plist version="1.0"><dict><key>URL</key><string>https://example.org/a?b=1&amp;c=2</string></dict></plist>"#,
        )
        .unwrap();
        assert_eq!(links_from_file(&webloc), vec!["https://example.org/a?b=1&c=2"]);

        let list = dir.path().join("links.txt");
        std::fs::write(&list, "https://a.org/1.zip\nnot a link\nhttps://b.org/2.iso\n").unwrap();
        assert_eq!(links_from_file(&list), vec!["https://a.org/1.zip", "https://b.org/2.iso"]);

        let photo = dir.path().join("photo.jpg");
        std::fs::write(&photo, b"\xff\xd8").unwrap();
        assert!(links_from_file(&photo).is_empty());
    }
}

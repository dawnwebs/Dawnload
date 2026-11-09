//! Where a download comes from, and which engine takes it. "Paste anything": the paste field,
//! drops, the CLI and (later) the browser extension all turn text into a [`DownloadSource`].

use std::fmt;
use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadSource {
    /// An http(s) link: a page yt-dlp or gallery-dl reads, or a plain file.
    Web { url: String },
    /// A `magnet:` link. Needs the torrent engine.
    Magnet(MagnetLink),
    /// A `.torrent` file on disk. Needs the torrent engine.
    TorrentFile { path: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Web,
    Magnet,
    TorrentFile,
}

impl DownloadSource {
    /// Reads pasted or dropped text. `None` when it isn't something Dawnload can download.
    pub fn parse(text: &str) -> Option<DownloadSource> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("magnet:") {
            return MagnetLink::parse(trimmed).map(DownloadSource::Magnet);
        }
        if let Some(path) = torrent_file_path(trimmed) {
            return Some(DownloadSource::TorrentFile { path });
        }
        normalized_download_url(trimmed).map(|url| DownloadSource::Web { url: url.to_string() })
    }

    pub fn kind(&self) -> SourceKind {
        match self {
            DownloadSource::Web { .. } => SourceKind::Web,
            DownloadSource::Magnet(_) => SourceKind::Magnet,
            DownloadSource::TorrentFile { .. } => SourceKind::TorrentFile,
        }
    }

    pub fn is_torrent(&self) -> bool {
        self.kind() != SourceKind::Web
    }

    pub fn url(&self) -> Option<Url> {
        match self {
            DownloadSource::Web { url } => Url::parse(url).ok(),
            _ => None,
        }
    }

    /// The string to show and to copy: the URL, the magnet link, or the file path.
    pub fn as_text(&self) -> String {
        match self {
            DownloadSource::Web { url } => url.clone(),
            DownloadSource::Magnet(magnet) => magnet.uri.clone(),
            DownloadSource::TorrentFile { path } => path.display().to_string(),
        }
    }

    /// Host for web links, the magnet's name, or the file name.
    pub fn short_name(&self) -> String {
        match self {
            DownloadSource::Web { url } => match Url::parse(url).ok().and_then(|u| u.host_str().map(trimmed_host)) {
                Some(host) => host,
                None => url.clone(),
            },
            DownloadSource::Magnet(magnet) => magnet.display_name.clone().unwrap_or_else(|| "Magnet link".into()),
            DownloadSource::TorrentFile { path } => file_name(path),
        }
    }

    /// host + path for web links: the title until the tool reports a real one.
    pub fn provisional_title(&self) -> String {
        match self {
            DownloadSource::Web { url } => {
                let Some(parsed) = Url::parse(url).ok() else { return url.clone() };
                let Some(host) = parsed.host_str() else { return url.clone() };
                let path = if parsed.path() == "/" { "" } else { parsed.path() };
                format!("{}{}", trimmed_host(host), path)
            }
            DownloadSource::Magnet(magnet) => magnet
                .display_name
                .clone()
                .unwrap_or_else(|| format!("Magnet {}", &magnet.info_hash[..12.min(magnet.info_hash.len())])),
            DownloadSource::TorrentFile { path } => path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_name(path)),
        }
    }
}

impl fmt::Display for DownloadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_text())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// "www.youtube.com" → "youtube.com".
pub fn trimmed_host(host: &str) -> String {
    let lower = host.to_ascii_lowercase();
    lower.strip_prefix("www.").map(str::to_string).unwrap_or(lower)
}

/// An http(s) URL with a host, with known tracking noise removed.
pub fn normalized_download_url(text: &str) -> Option<Url> {
    let mut url = Url::parse(text.trim()).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if host == "spankbang.com" || host.ends_with(".spankbang.com") {
        url.set_query(None);
        url.set_fragment(None);
    }
    Some(url)
}

fn torrent_file_path(text: &str) -> Option<PathBuf> {
    let path = if text.len() >= 7 && text[..7].eq_ignore_ascii_case("file://") {
        Url::parse(text).ok()?.to_file_path().ok()?
    } else if let Some(rest) = text.strip_prefix('~') {
        let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
        home.join(rest.trim_start_matches(['/', '\\']))
    } else if text.starts_with('/') || Path::new(text).is_absolute() {
        PathBuf::from(text)
    } else {
        return None;
    };
    let is_torrent = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"));
    is_torrent.then_some(path)
}

/// A parsed `magnet:?xt=urn:btih:…` link.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MagnetLink {
    pub uri: String,
    /// Lowercase hex (v1, 40 chars), base32 as given (32 chars, uppercase), or a v2 multihash.
    pub info_hash: String,
    pub display_name: Option<String>,
    pub trackers: Vec<String>,
    pub exact_length: Option<u64>,
}

impl MagnetLink {
    pub fn parse(text: &str) -> Option<MagnetLink> {
        let trimmed = text.trim();
        if trimmed.len() < 8 || !trimmed[..8].eq_ignore_ascii_case("magnet:?") {
            return None;
        }
        let query = &trimmed[8..];
        let mut hash = None;
        let mut display_name = None;
        let mut trackers = Vec::new();
        let mut exact_length = None;

        for pair in query.split('&') {
            let (key, raw) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode_str(raw).decode_utf8_lossy().into_owned();
            match key.to_ascii_lowercase().as_str() {
                "xt" if hash.is_none() => {
                    let lower = value.to_ascii_lowercase();
                    if let Some(raw) = lower.strip_prefix("urn:btih:").map(|_| &value[9..]) {
                        hash = Some(if raw.len() == 40 { raw.to_ascii_lowercase() } else { raw.to_ascii_uppercase() });
                    } else if lower.starts_with("urn:btmh:") {
                        hash = Some(lower[9..].to_string());
                    }
                }
                "dn" if display_name.is_none() => {
                    // Forms encode spaces as '+'; a literal plus arrives as %2B and is kept.
                    let name = percent_decode_str(&raw.replace('+', " ")).decode_utf8_lossy().trim().to_string();
                    if !name.is_empty() {
                        display_name = Some(name);
                    }
                }
                "tr" if !value.is_empty() => trackers.push(value),
                "xl" if exact_length.is_none() => exact_length = value.parse().ok(),
                _ => {}
            }
        }

        let info_hash = hash.filter(|h| is_valid_hash(h))?;
        Some(MagnetLink { uri: trimmed.to_string(), info_hash, display_name, trackers, exact_length })
    }
}

fn is_valid_hash(hash: &str) -> bool {
    match hash.len() {
        40 => hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        32 => hash.bytes().all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b)),
        68 => hash.starts_with("1220") && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        _ => false,
    }
}

/// The engines Dawnload knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum EngineId {
    YtDlp,
    GalleryDl,
    /// Dawnload's own HTTP downloader for plain files.
    Direct,
    Torrent,
    /// Fake progress for UI prototypes and tests.
    Demo,
}

impl EngineId {
    pub fn display_name(self) -> &'static str {
        match self {
            EngineId::YtDlp => "yt-dlp",
            EngineId::GalleryDl => "gallery-dl",
            EngineId::Direct => "Dawnload",
            EngineId::Torrent => "Torrent",
            EngineId::Demo => "Demo",
        }
    }
}

/// What the Engine chip is set to. `Auto` resolves per link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnginePreference {
    #[default]
    Auto,
    YtDlp,
    GalleryDl,
    Direct,
}

impl EnginePreference {
    pub fn resolve(self, source: &DownloadSource) -> EngineId {
        match source {
            DownloadSource::Magnet(_) | DownloadSource::TorrentFile { .. } => EngineId::Torrent,
            DownloadSource::Web { url } => match self {
                EnginePreference::YtDlp => EngineId::YtDlp,
                EnginePreference::GalleryDl => EngineId::GalleryDl,
                EnginePreference::Direct => EngineId::Direct,
                EnginePreference::Auto => Url::parse(url).map(|u| suggest_web_engine(&u)).unwrap_or(EngineId::YtDlp),
            },
        }
    }
}

/// Sites where gallery-dl does the better job.
const GALLERY_HOSTS: &[&str] = &[
    "artstation.com", "behance.net", "bunkr.si", "danbooru.donmai.us", "deviantart.com", "e621.net",
    "flickr.com", "gelbooru.com", "imgur.com", "kemono.su", "pinterest.com", "pixiv.net", "safebooru.org",
    "wallhaven.cc", "500px.com", "tumblr.com",
];

/// Sites yt-dlp should always read, even when a path looks like a file.
const MEDIA_SITES: &[&str] = &[
    "youtube.com", "youtu.be", "vimeo.com", "twitch.tv", "tiktok.com", "x.com", "twitter.com", "facebook.com",
    "dailymotion.com", "soundcloud.com", "bandcamp.com", "reddit.com", "bilibili.com", "instagram.com",
];

/// File extensions that mean "this link is the file itself".
const DIRECT_EXTENSIONS: &[&str] = &[
    // archives and disk images
    "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "img", "dmg", "bin",
    // installers and packages
    "exe", "msi", "msix", "appimage", "deb", "rpm", "pkg", "apk", "flatpakref", "snap",
    // documents
    "pdf", "epub", "mobi", "cbz", "cbr",
    // media served as plain files
    "mp4", "m4v", "mkv", "webm", "mov", "avi", "mp3", "m4a", "flac", "wav", "ogg", "opus", "aac",
    "jpg", "jpeg", "png", "gif", "webp", "avif", "heic", "tif", "tiff",
];

fn host_matches(host: &str, list: &[&str]) -> bool {
    list.iter().any(|site| host == *site || host.ends_with(&format!(".{site}")))
}

/// The extension of the URL's last path segment, lowercased.
pub fn url_file_extension(url: &Url) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    let (_, ext) = decoded.rsplit_once('.')?;
    (!ext.is_empty() && ext.len() <= 10).then(|| ext.to_ascii_lowercase())
}

/// Auto: gallery sites go to gallery-dl, plain files to Dawnload's HTTP engine, `.torrent` links
/// to the torrent engine, everything else to yt-dlp.
pub fn suggest_web_engine(url: &Url) -> EngineId {
    let Some(host) = url.host_str().map(trimmed_host) else { return EngineId::YtDlp };
    let path = url.path().to_ascii_lowercase();

    if host == "instagram.com" || host.ends_with(".instagram.com") {
        let video = ["/reel/", "/reels/", "/tv/"].iter().any(|p| path.starts_with(p));
        return if video { EngineId::YtDlp } else { EngineId::GalleryDl };
    }
    if host_matches(&host, GALLERY_HOSTS) {
        return EngineId::GalleryDl;
    }
    if host_matches(&host, MEDIA_SITES) {
        return EngineId::YtDlp;
    }
    match url_file_extension(url).as_deref() {
        Some("torrent") => EngineId::Torrent,
        Some(ext) if DIRECT_EXTENSIONS.contains(&ext) => EngineId::Direct,
        _ => EngineId::YtDlp,
    }
}

/// True for YouTube links, which need a JavaScript runtime since yt-dlp 2025.11.
pub fn is_youtube(source: &DownloadSource) -> bool {
    source
        .url()
        .and_then(|u| u.host_str().map(trimmed_host))
        .is_some_and(|h| h == "youtu.be" || h == "youtube.com" || h.ends_with(".youtube.com") || h == "youtube-nocookie.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(text: &str) -> DownloadSource {
        DownloadSource::parse(text).expect("parses")
    }

    #[test]
    fn web_link() {
        let source = web("  https://www.youtube.com/watch?v=abc  ");
        assert_eq!(source, DownloadSource::Web { url: "https://www.youtube.com/watch?v=abc".into() });
        assert_eq!(source.short_name(), "youtube.com");
        assert_eq!(source.provisional_title(), "youtube.com/watch");
        assert!(!source.is_torrent());
        assert!(is_youtube(&source));
    }

    #[test]
    fn rejects_text_that_is_not_a_link() {
        for text in ["hello world", "youtube.com/watch?v=abc", "ftp://example.com/file", "", "   ", "/Users/me/notes.txt"] {
            assert_eq!(DownloadSource::parse(text), None, "{text}");
        }
    }

    #[test]
    fn magnet_with_hex_hash() {
        let text = "magnet:?xt=urn:btih:C9E15763F722F23E98A29DECDFAE341B98D53056&dn=Ubuntu+24.04+Desktop&tr=udp%3A%2F%2Ftracker.example%3A1337&xl=6114770944";
        let source = web(text);
        let DownloadSource::Magnet(magnet) = &source else { panic!("not a magnet") };
        assert_eq!(magnet.info_hash, "c9e15763f722f23e98a29decdfae341b98d53056");
        assert_eq!(magnet.display_name.as_deref(), Some("Ubuntu 24.04 Desktop"));
        assert_eq!(magnet.trackers, vec!["udp://tracker.example:1337".to_string()]);
        assert_eq!(magnet.exact_length, Some(6_114_770_944));
        assert_eq!(source.short_name(), "Ubuntu 24.04 Desktop");
        assert!(source.is_torrent());
    }

    #[test]
    fn magnet_with_base32_hash_and_no_name() {
        let magnet = MagnetLink::parse("magnet:?xt=urn:btih:ZOCMZQIPFFW7OLLMIC5HUB6BPCSDEOQU").unwrap();
        assert_eq!(magnet.info_hash, "ZOCMZQIPFFW7OLLMIC5HUB6BPCSDEOQU");
        assert_eq!(magnet.display_name, None);
        assert_eq!(DownloadSource::Magnet(magnet).short_name(), "Magnet link");
    }

    #[test]
    fn magnet_v2_and_plus_in_name() {
        let hash = format!("1220{}", "ab".repeat(32));
        let magnet = MagnetLink::parse(&format!("MAGNET:?xt=urn:btmh:{hash}&dn=C%2B%2B+Book")).unwrap();
        assert_eq!(magnet.info_hash, hash);
        assert_eq!(magnet.display_name.as_deref(), Some("C++ Book"));
    }

    #[test]
    fn magnet_without_valid_hash_is_rejected() {
        assert_eq!(DownloadSource::parse("magnet:?dn=nothing"), None);
        assert_eq!(DownloadSource::parse("magnet:?xt=urn:btih:1234"), None);
    }

    #[test]
    fn torrent_file_paths() {
        assert_eq!(
            DownloadSource::parse("/Users/me/Downloads/debian.torrent"),
            Some(DownloadSource::TorrentFile { path: "/Users/me/Downloads/debian.torrent".into() })
        );
        #[cfg(unix)]
        assert_eq!(
            DownloadSource::parse("file:///tmp/a%20b.torrent"),
            Some(DownloadSource::TorrentFile { path: "/tmp/a b.torrent".into() })
        );
        let home = web("~/x.TORRENT");
        assert_eq!(home.kind(), SourceKind::TorrentFile);
        assert_eq!(home.provisional_title(), "x");
    }

    #[test]
    fn url_sanitizer() {
        let slug = "https://spankbang.com/77ii8/video/fc2+ppv+1106157+20+h+w";
        assert_eq!(normalized_download_url(slug).unwrap().as_str(), slug);
        assert_eq!(
            normalized_download_url("https://spankbang.com/77ii8/video/?utm_source=test#player").unwrap().as_str(),
            "https://spankbang.com/77ii8/video/"
        );
        let other = "https://example.com/profile/video/clip-name?quality=best";
        assert_eq!(normalized_download_url(other).unwrap().as_str(), other);
        assert!(normalized_download_url("file:///tmp/video.mp4").is_none());
        assert!(normalized_download_url("not a URL").is_none());
    }

    #[test]
    fn sources_round_trip_through_json() {
        let sources = vec![
            web("https://vimeo.com/1"),
            web("magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=x"),
            DownloadSource::TorrentFile { path: "/tmp/x.torrent".into() },
        ];
        let json = serde_json::to_string(&sources).unwrap();
        assert_eq!(serde_json::from_str::<Vec<DownloadSource>>(&json).unwrap(), sources);
    }

    #[test]
    fn auto_engine() {
        let engine = |text: &str, preference: EnginePreference| preference.resolve(&web(text));
        use EnginePreference::*;
        assert_eq!(engine("https://www.youtube.com/watch?v=abc", Auto), EngineId::YtDlp);
        assert_eq!(engine("https://www.instagram.com/p/xyz/", Auto), EngineId::GalleryDl);
        assert_eq!(engine("https://www.instagram.com/reel/xyz/", Auto), EngineId::YtDlp);
        assert_eq!(engine("https://imgur.com/a/abc", Auto), EngineId::GalleryDl);
        assert_eq!(engine("https://www.pixiv.net/en/artworks/1", Auto), EngineId::GalleryDl);
        assert_eq!(engine("https://imgur.com/a/abc", YtDlp), EngineId::YtDlp);
        assert_eq!(engine("https://youtube.com/x", GalleryDl), EngineId::GalleryDl);
        assert_eq!(engine("magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056", Auto), EngineId::Torrent);
        assert_eq!(engine("https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso", Auto), EngineId::Direct);
        assert_eq!(engine("https://example.com/files/Report%20Final.PDF", Auto), EngineId::Direct);
        assert_eq!(engine("https://example.com/debian.torrent", Auto), EngineId::Torrent);
        assert_eq!(engine("https://example.com/watch/clip", Auto), EngineId::YtDlp);
        assert_eq!(engine("https://example.com/stream.m3u8", Auto), EngineId::YtDlp);
        assert_eq!(engine("https://example.com/page", Direct), EngineId::Direct);
    }
}

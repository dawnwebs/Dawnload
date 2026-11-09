//! The data an item is made of. The core owns every [`Item`]; UIs see [`crate::api::ItemView`]s.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::failure::DownloadFailure;
use crate::format;
use crate::source::{DownloadSource, EngineId, EnginePreference};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(pub u64);

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where an item is in its life. The words are the brand's state names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Pulling,
    Polishing,
    Kept,
    Failed,
    Paused,
    /// Torrents: kept on disk and still uploading. The job is still running.
    Seeding,
}

impl Status {
    pub const ALL: [Status; 7] =
        [Status::Queued, Status::Pulling, Status::Polishing, Status::Kept, Status::Failed, Status::Paused, Status::Seeding];

    pub fn word(self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Pulling => "Pulling",
            Status::Polishing => "Polishing",
            Status::Kept => "Kept",
            Status::Failed => "Failed",
            Status::Paused => "Paused",
            Status::Seeding => "Seeding",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Pulling => "pulling",
            Status::Polishing => "polishing",
            Status::Kept => "kept",
            Status::Failed => "failed",
            Status::Paused => "paused",
            Status::Seeding => "seeding",
        }
    }

    pub fn parse(text: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.as_str().eq_ignore_ascii_case(text))
    }

    /// Downloading or finishing: counts against its lane.
    pub fn is_active(self) -> bool {
        matches!(self, Status::Pulling | Status::Polishing)
    }

    /// A job is attached (active or seeding).
    pub fn is_running(self) -> bool {
        matches!(self, Status::Pulling | Status::Polishing | Status::Seeding)
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Status::Kept | Status::Failed | Status::Seeding)
    }

    /// Its options can still change, because nothing is running.
    pub fn is_editable(self) -> bool {
        matches!(self, Status::Queued | Status::Paused | Status::Failed)
    }

    /// The files are on disk.
    pub fn has_files(self) -> bool {
        matches!(self, Status::Kept | Status::Seeding)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[default]
    Best,
    /// MP4 and M4A, plays everywhere.
    Compatible,
    /// M4A audio; needs ffmpeg.
    Audio,
}

impl Quality {
    pub fn title(self) -> &'static str {
        match self {
            Quality::Best => "Best",
            Quality::Compatible => "Compatible",
            Quality::Audio => "Audio only",
        }
    }
}

/// How a site sees you: nothing, a cookies.txt you exported, or a browser's own cookie store.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Cookies {
    #[default]
    None,
    /// A Netscape-format cookies.txt.
    File(PathBuf),
    /// yt-dlp's `--cookies-from-browser` spec: `firefox`, `chrome:Profile 1`, `safari`.
    Browser(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumAlgorithm {
    Sha256,
    Sha512,
}

/// A checksum to verify a direct download against.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: ChecksumAlgorithm,
    /// Lowercase hex.
    pub hex: String,
}

impl Checksum {
    /// `sha256:abc…`, `sha512=abc…`, or bare hex (64 chars is SHA-256, 128 is SHA-512).
    pub fn parse(text: &str) -> Option<Checksum> {
        let text = text.trim();
        let (algorithm, hex) = match text.split_once([':', '=']) {
            Some((name, hex)) => match name.to_ascii_lowercase().replace('-', "").as_str() {
                "sha256" => (Some(ChecksumAlgorithm::Sha256), hex),
                "sha512" => (Some(ChecksumAlgorithm::Sha512), hex),
                _ => return None,
            },
            None => (None, text),
        };
        let hex = hex.trim().to_ascii_lowercase();
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let algorithm = match (algorithm, hex.len()) {
            (Some(ChecksumAlgorithm::Sha256), 64) | (None, 64) => ChecksumAlgorithm::Sha256,
            (Some(ChecksumAlgorithm::Sha512), 128) | (None, 128) => ChecksumAlgorithm::Sha512,
            _ => return None,
        };
        Some(Checksum { algorithm, hex })
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.algorithm {
            ChecksumAlgorithm::Sha256 => "sha256",
            ChecksumAlgorithm::Sha512 => "sha512",
        };
        write!(f, "{name}:{}", self.hex)
    }
}

/// How one item downloads. The intake chips edit the defaults; each item keeps its own copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub engine: EnginePreference,
    pub quality: Quality,
    pub destination: PathBuf,
    /// ffmpeg merges, remuxes to MP4 and moves the index to the front.
    pub polish: bool,
    pub playlist: bool,
    /// yt-dlp `--impersonate` target, e.g. `chrome-146`. None is off.
    pub impersonate: Option<String>,
    pub cookies: Cookies,
    /// Direct downloads: connections per file.
    pub connections: u8,
    /// Direct downloads: verify the file before keeping it.
    pub checksum: Option<Checksum>,
    /// Torrents: stop seeding at this upload ratio. None seeds until stopped.
    pub seed_ratio_limit: Option<f64>,
    /// Torrents: show the file list before the download starts.
    pub ask_for_files: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            engine: EnginePreference::Auto,
            quality: Quality::Best,
            destination: crate::paths::default_download_dir(),
            polish: true,
            playlist: false,
            impersonate: None,
            cookies: Cookies::None,
            connections: 4,
            checksum: None,
            seed_ratio_limit: Some(1.0),
            ask_for_files: false,
        }
    }
}

impl Options {
    /// The words on a Queued row and in the inspector: "Best · Polish · chrome-146".
    pub fn summary(&self, engine: EngineId) -> String {
        let mut parts: Vec<String> = Vec::new();
        match engine {
            EngineId::YtDlp => {
                parts.push(self.quality.title().into());
                if self.polish {
                    parts.push("Polish".into());
                }
                if self.playlist {
                    parts.push("Playlist".into());
                }
                if let Some(target) = &self.impersonate {
                    parts.push(target.clone());
                }
            }
            EngineId::GalleryDl => {
                parts.push("Original files".into());
                if self.polish {
                    parts.push("Polish".into());
                }
            }
            EngineId::Direct => {
                parts.push(format::count(self.connections.max(1) as usize, "connection"));
                if let Some(checksum) = &self.checksum {
                    parts.push(match checksum.algorithm {
                        ChecksumAlgorithm::Sha256 => "SHA-256".into(),
                        ChecksumAlgorithm::Sha512 => "SHA-512".into(),
                    });
                }
            }
            EngineId::Torrent | EngineId::Demo => {
                if let Some(limit) = self.seed_ratio_limit {
                    parts.push(format!("Seed to {}", format::ratio(limit)));
                }
            }
        }
        match &self.cookies {
            Cookies::File(_) if engine != EngineId::Torrent => parts.push("cookies.txt".into()),
            Cookies::Browser(spec) if engine != EngineId::Torrent => parts.push(format!("{spec} cookies")),
            _ => {}
        }
        parts.join(" · ")
    }
}

/// Real progress for one item: bytes across its parts, entries across a playlist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transfer {
    /// 0…1 across the whole item; None when the size isn't known.
    pub fraction: Option<f64>,
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
    pub bytes_per_second: Option<f64>,
    pub seconds_left: Option<u64>,
    pub entry_index: Option<u32>,
    pub entry_count: Option<u32>,
}

/// Torrents: the people this item is trading pieces with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Swarm {
    pub peers: u32,
    pub seeds: u32,
    pub download_rate: f64,
    pub upload_rate: f64,
    pub uploaded: u64,
    pub ratio: f64,
}

/// One file an item made or will make. Torrents have many and can skip some.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemFile {
    /// Absolute path once known, or the path inside the torrent.
    pub path: String,
    pub size: Option<u64>,
    pub bytes_done: Option<u64>,
    /// Torrents: unticked files are skipped.
    pub selected: bool,
}

impl ItemFile {
    pub fn on_disk(path: impl Into<PathBuf>) -> ItemFile {
        let path = path.into();
        let size = std::fs::metadata(&path).ok().map(|m| m.len());
        ItemFile { path: path.display().to_string(), size, bytes_done: size, selected: true }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit(['/', '\\']).next().unwrap_or(&self.path)
    }
}

/// Everything the core knows about one download, except its log (kept apart; it's long).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub source: DownloadSource,
    pub options: Options,
    /// Resolved from `options.engine` for this source.
    pub engine: EngineId,
    pub title: Option<String>,
    pub status: Status,
    /// A few words on what the engine is doing: "merging audio and video", "Checking".
    pub phase_detail: Option<String>,
    #[serde(default)]
    pub transfer: Transfer,
    pub swarm: Option<Swarm>,
    #[serde(default)]
    pub files: Vec<ItemFile>,
    pub failure: Option<DownloadFailure>,
    /// The command that ran last, for the log and "copy command".
    pub command: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub created_at: i64,
    pub finished_at: Option<i64>,
    /// Runs started since the last success or manual retry; drives automatic retries.
    #[serde(default)]
    pub attempts: u32,
}

impl Item {
    pub fn new(id: ItemId, source: DownloadSource, options: Options) -> Item {
        let engine = options.engine.resolve(&source);
        Item {
            id,
            source,
            options,
            engine,
            title: None,
            status: Status::Queued,
            phase_detail: None,
            transfer: Transfer::default(),
            swarm: None,
            files: Vec::new(),
            failure: None,
            command: None,
            created_at: now_millis(),
            finished_at: None,
            attempts: 0,
        }
    }

    pub fn display_title(&self) -> String {
        match &self.title {
            Some(title) if !title.is_empty() => title.clone(),
            _ => self.source.provisional_title(),
        }
    }

    pub fn kept_files(&self) -> impl Iterator<Item = &ItemFile> {
        self.files.iter().filter(|f| f.selected)
    }

    pub fn kept_bytes(&self) -> Option<u64> {
        let sizes: Vec<u64> = self.kept_files().filter_map(|f| f.size).collect();
        if sizes.is_empty() { self.transfer.bytes_total } else { Some(sizes.iter().sum()) }
    }

    /// The fill of the D: real progress, or where it stopped.
    pub fn fill(&self) -> f64 {
        match self.status {
            Status::Kept | Status::Seeding | Status::Polishing | Status::Failed => 1.0,
            Status::Queued => 0.0,
            Status::Pulling | Status::Paused => self.transfer.fraction.unwrap_or(0.0),
        }
    }

    /// What a record that was running when Dawnload stopped comes back as.
    pub fn recovered(mut self) -> Item {
        if self.status.is_running() {
            self.status = Status::Paused;
            self.phase_detail = None;
        }
        self.transfer.bytes_per_second = None;
        self.transfer.seconds_left = None;
        self
    }
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_parsing() {
        let hex = "a".repeat(64);
        assert_eq!(Checksum::parse(&hex).unwrap().algorithm, ChecksumAlgorithm::Sha256);
        assert_eq!(Checksum::parse(&format!("SHA-256:{}", hex.to_uppercase())).unwrap().hex, hex);
        assert_eq!(Checksum::parse(&format!("sha512={}", "b".repeat(128))).unwrap().algorithm, ChecksumAlgorithm::Sha512);
        assert!(Checksum::parse(&format!("sha512:{hex}")).is_none());
        assert!(Checksum::parse("md5:abc").is_none());
        assert!(Checksum::parse("zz").is_none());
    }

    #[test]
    fn running_items_come_back_paused() {
        let source = DownloadSource::parse("https://example.com/a.zip").unwrap();
        let mut item = Item::new(ItemId(1), source, Options::default());
        for status in [Status::Pulling, Status::Polishing, Status::Seeding] {
            item.status = status;
            item.transfer.bytes_per_second = Some(1.0);
            let back = item.clone().recovered();
            assert_eq!(back.status, Status::Paused);
            assert_eq!(back.transfer.bytes_per_second, None);
        }
        item.status = Status::Kept;
        assert_eq!(item.clone().recovered().status, Status::Kept);
    }

    #[test]
    fn options_summary() {
        let mut options = Options { impersonate: Some("chrome-146".into()), ..Options::default() };
        assert_eq!(options.summary(EngineId::YtDlp), "Best · Polish · chrome-146");
        options.cookies = Cookies::Browser("firefox".into());
        assert_eq!(options.summary(EngineId::GalleryDl), "Original files · Polish · firefox cookies");
        assert_eq!(options.summary(EngineId::Direct), "4 connections · firefox cookies");
        assert_eq!(options.summary(EngineId::Torrent), "Seed to 1.00");
    }
}

//! Errors in the brand's voice: what happened, then what to do.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    Forbidden,
    SignInNeeded,
    NotFound,
    UnsupportedSite,
    ToolMissing,
    FfmpegNeeded,
    JsRuntimeNeeded,
    CookiesUnreadable,
    NotALink,
    EngineUnavailable,
    FolderUnavailable,
    /// The connection dropped or timed out. Worth retrying on its own.
    Network,
    ChecksumMismatch,
    /// The file changed on the server between runs.
    ServerChanged,
    /// A bug in Dawnload, caught so it can't stop the rest of the queue.
    Internal,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadFailure {
    pub kind: FailureKind,
    /// What happened, one sentence. Shown on the row.
    pub message: String,
    /// What to do, one sentence. Shown in the inspector.
    pub hint: Option<String>,
    /// The tool's own last error line, if there was one.
    pub detail: Option<String>,
}

impl fmt::Display for DownloadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sentence())
    }
}

impl std::error::Error for DownloadFailure {}

impl DownloadFailure {
    pub fn new(kind: FailureKind, message: impl Into<String>, hint: Option<&str>) -> Self {
        DownloadFailure { kind, message: message.into(), hint: hint.map(str::to_string), detail: None }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn sentence(&self) -> String {
        match &self.hint {
            Some(hint) => format!("{} {}", self.message, hint),
            None => self.message.clone(),
        }
    }

    /// Failures that usually go away on their own; the queue retries these a couple of times.
    pub fn is_transient(&self) -> bool {
        self.kind == FailureKind::Network
    }

    pub fn tool_missing(tool: &str) -> Self {
        Self::new(
            FailureKind::ToolMissing,
            format!("{tool} isn't installed."),
            Some("Dawnload can install it for you: Settings › Tools, or `dawnload tools install`."),
        )
    }

    pub fn ffmpeg_needed() -> Self {
        Self::new(FailureKind::FfmpegNeeded, "Audio only needs ffmpeg.", Some("Install ffmpeg, or choose Best."))
    }

    pub fn js_runtime_needed() -> Self {
        Self::new(
            FailureKind::JsRuntimeNeeded,
            "YouTube needs a JavaScript runtime.",
            Some("Install deno from Settings › Tools, or run `dawnload tools install deno`."),
        )
    }

    pub fn cookies_unreadable() -> Self {
        Self::new(
            FailureKind::CookiesUnreadable,
            "The cookies file can't be read any more.",
            Some("Choose it again in Settings."),
        )
    }

    pub fn not_a_link() -> Self {
        Self::new(FailureKind::NotALink, "That isn't a full link.", Some("Paste one that starts with https://."))
    }

    pub fn torrents_unavailable() -> Self {
        Self::new(
            FailureKind::EngineUnavailable,
            "Torrent downloads aren't built yet.",
            Some("Dawnload can read this link, but it has nothing to download torrents with yet."),
        )
    }

    pub fn folder_unavailable(path: &str) -> Self {
        Self::new(
            FailureKind::FolderUnavailable,
            format!("Dawnload can't write to {path}."),
            Some("Choose another folder in Settings."),
        )
    }

    pub fn network(detail: impl Into<String>) -> Self {
        Self::new(
            FailureKind::Network,
            "The connection dropped.",
            Some("Dawnload tries again on its own; Retry picks up where it stopped."),
        )
        .with_detail(detail)
    }

    pub fn checksum_mismatch(expected: &str, actual: &str) -> Self {
        Self::new(
            FailureKind::ChecksumMismatch,
            "The file doesn't match its checksum.",
            Some("It was not kept. Check the link and the checksum, then retry."),
        )
        .with_detail(format!("expected {expected}, got {actual}"))
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            FailureKind::Internal,
            "Something went wrong inside Dawnload.",
            Some("Retry, and report it if it happens again."),
        )
        .with_detail(detail)
    }

    /// From an HTTP status, for Dawnload's own downloader.
    pub fn http_status(status: u16) -> Self {
        match status {
            401 => {
                Self::new(FailureKind::SignInNeeded, "This needs you to be signed in.", Some("Add your cookies.txt in Settings."))
            }
            403 => Self::new(
                FailureKind::Forbidden,
                "This site refused the request (HTTP 403).",
                Some("Add your cookies.txt in Settings, or try yt-dlp from the Engine menu."),
            ),
            404 | 410 => {
                Self::new(FailureKind::NotFound, format!("The file wasn't found (HTTP {status})."), Some("Check the link."))
            }
            408 | 429 | 500..=599 => Self::new(
                FailureKind::Network,
                format!("The server is busy (HTTP {status})."),
                Some("Dawnload tries again on its own; Retry picks up where it stopped."),
            ),
            _ => Self::new(FailureKind::Other, format!("The server answered HTTP {status}."), Some("The log has the details.")),
        }
    }

    /// Reads a tool's output after it exited with an error.
    pub fn classify(tool: &str, exit_code: i32, output: &str) -> Self {
        let detail = last_error_line(output);
        let haystack = output.to_lowercase();
        let has = |needles: &[&str]| needles.iter().any(|n| haystack.contains(n));
        let make = |kind, message: String, hint: &str| DownloadFailure {
            kind,
            message,
            hint: Some(hint.to_string()),
            detail: detail.clone(),
        };

        if has(&["http error 403", "403: forbidden", "403 forbidden"]) {
            return make(
                FailureKind::Forbidden,
                "This site refused the request (HTTP 403).".into(),
                "Turn on browser impersonation, or add your cookies.txt in Settings.",
            );
        }
        if has(&[
            "sign in to confirm",
            "login required",
            "requires login",
            "log in to",
            "use --cookies",
            "private video",
            "this video is private",
            "members-only",
            "authentication required",
            "http error 401",
        ]) {
            return make(
                FailureKind::SignInNeeded,
                "This needs you to be signed in.".into(),
                "Add your cookies.txt in Settings.",
            );
        }
        if has(&["http error 404", "404: not found", "404 not found"]) {
            return make(FailureKind::NotFound, "The page wasn't found (HTTP 404).".into(), "Check the link.");
        }
        if has(&["unsupported url", "no suitable extractor"]) {
            let hint = if tool == "gallery-dl" {
                "Try yt-dlp for video and audio."
            } else {
                "Try gallery-dl for galleries and image sets."
            };
            return make(FailureKind::UnsupportedSite, format!("{tool} doesn't support this site."), hint);
        }
        if has(&["ffmpeg not found", "ffprobe and ffmpeg not found", "ffmpeg is not installed"]) {
            return make(FailureKind::FfmpegNeeded, "This download needs ffmpeg.".into(), "Install ffmpeg, or turn off Polish.");
        }
        if has(&["no supported javascript runtime", "javascript runtime", "n challenge solving failed", "js runtime"]) {
            let mut failure = Self::js_runtime_needed();
            failure.detail = detail;
            return failure;
        }
        if has(&[
            "timed out",
            "connection reset",
            "connection refused",
            "temporary failure in name resolution",
            "getaddrinfo failed",
            "name or service not known",
            "network is unreachable",
            "connection aborted",
            "incompleteread",
            "remote end closed connection",
        ]) {
            return make(
                FailureKind::Network,
                "The connection dropped.".into(),
                "Dawnload tries again on its own; Retry picks up where it stopped.",
            );
        }

        make(FailureKind::Other, format!("{tool} stopped with an error (exit {exit_code})."), "The log has the details.")
    }
}

fn last_error_line(output: &str) -> Option<String> {
    output.lines().rev().find(|line| line.starts_with("ERROR:") || line.contains("[error]")).map(|line| line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tool_output() {
        let kind = |tool, out| DownloadFailure::classify(tool, 1, out).kind;
        assert_eq!(
            kind("yt-dlp", "ERROR: [generic] Unable to download webpage: HTTP Error 403: Forbidden"),
            FailureKind::Forbidden
        );
        assert_eq!(
            kind("yt-dlp", "ERROR: [youtube] x: Sign in to confirm you’re not a bot. Use --cookies-from-browser"),
            FailureKind::SignInNeeded
        );
        assert_eq!(kind("yt-dlp", "ERROR: Unsupported URL: https://example.com"), FailureKind::UnsupportedSite);
        assert_eq!(kind("yt-dlp", "ERROR: unable to download video data: <urlopen error timed out>"), FailureKind::Network);
        assert_eq!(
            kind(
                "yt-dlp",
                "WARNING: [youtube] No supported JavaScript runtime could be found.\nERROR: x: Requested format is not available"
            ),
            FailureKind::JsRuntimeNeeded
        );
        assert_eq!(
            DownloadFailure::classify("gallery-dl", 64, "[gallery-dl][error] No suitable extractor found for 'https://x'")
                .hint
                .as_deref(),
            Some("Try yt-dlp for video and audio.")
        );

        let other = DownloadFailure::classify("yt-dlp", 2, "noise\nERROR: something odd\n");
        assert_eq!(other.kind, FailureKind::Other);
        assert_eq!(other.message, "yt-dlp stopped with an error (exit 2).");
        assert_eq!(other.detail.as_deref(), Some("ERROR: something odd"));

        let forbidden = DownloadFailure::classify("yt-dlp", 1, "HTTP Error 403");
        assert_eq!(
            forbidden.sentence(),
            "This site refused the request (HTTP 403). Turn on browser impersonation, or add your cookies.txt in Settings."
        );
    }

    #[test]
    fn http_status_mapping() {
        assert_eq!(DownloadFailure::http_status(404).kind, FailureKind::NotFound);
        assert_eq!(DownloadFailure::http_status(403).kind, FailureKind::Forbidden);
        assert_eq!(DownloadFailure::http_status(401).kind, FailureKind::SignInNeeded);
        assert!(DownloadFailure::http_status(503).is_transient());
        assert!(DownloadFailure::http_status(429).is_transient());
        assert!(!DownloadFailure::http_status(418).is_transient());
    }
}

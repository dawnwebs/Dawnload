//! Dawnload's own downloader for plain files: several connections per file (with work stealing,
//! so a slow segment never holds up the end), resume across Pause and restarts, and a checksum
//! check before the file is kept.
//!
//! While it downloads, a file lives as `name.ext.dawnload-part` with a small JSON sidecar
//! `name.ext.dawnload-state` that records the server's validators and each segment's progress.
//! Keeping the state beside the data (as aria2 does) means a copy of the folder resumes too.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::StatusCode;
use reqwest::header::{self, HeaderMap};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::*;
use crate::failure::FailureKind;
use crate::filename;
use crate::model::{Checksum, ChecksumAlgorithm, Cookies};

/// Segments smaller than this aren't split further.
const MIN_SPLIT: u64 = 1024 * 1024;
const RETRIES: u32 = 5;

pub struct HttpEngine {
    client: reqwest::Client,
}

impl HttpEngine {
    pub fn new() -> Self {
        HttpEngine { client: crate::net::client() }
    }
}

impl Default for HttpEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadEngine for HttpEngine {
    fn id(&self) -> EngineId {
        EngineId::Direct
    }

    fn lane(&self) -> Lane {
        Lane::Direct
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::RESUME | Capabilities::MULTI_CONNECTION | Capabilities::CHECKSUM | Capabilities::SITE_ACCESS
    }

    fn accepts(&self, source: &DownloadSource) -> bool {
        matches!(source, DownloadSource::Web { .. })
    }

    fn availability(&self, _tools: &ToolSnapshot, _options: &Options) -> Availability {
        Availability::Ready
    }

    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture {
        Box::pin(run(self.client.clone(), request, context))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Segment {
    start: u64,
    /// Exclusive. `u64::MAX` when the size is unknown.
    end: u64,
    /// Next byte to write.
    pos: u64,
    #[serde(skip)]
    owned: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PartState {
    url: String,
    total: Option<u64>,
    etag: Option<String>,
    last_modified: Option<String>,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct Probe {
    total: Option<u64>,
    ranges: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    filename: Option<String>,
    digest: Option<Checksum>,
}

fn part_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".dawnload-part");
    target.with_file_name(name)
}

fn state_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".dawnload-state");
    target.with_file_name(name)
}

fn header_text(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

/// `attachment; filename="a.zip"; filename*=UTF-8''a%20b.zip` → `a b.zip`.
pub fn content_disposition_filename(value: &str) -> Option<String> {
    let mut plain = None;
    for part in value.split(';').map(str::trim) {
        let Some((key, raw)) = part.split_once('=') else { continue };
        let raw = raw.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                let encoded = raw.splitn(3, '\'').nth(2).unwrap_or(raw);
                let decoded = percent_encoding::percent_decode_str(encoded.trim_matches('"')).decode_utf8_lossy().into_owned();
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
            "filename" => plain = Some(raw.trim_matches('"').to_string()).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    plain
}

/// `Repr-Digest: sha-256=:base64:` or `Digest: SHA-256=base64`.
fn digest_header(headers: &HeaderMap) -> Option<Checksum> {
    for name in ["repr-digest", "content-digest", "digest"] {
        let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) else { continue };
        for item in value.split(',') {
            let Some((algo, encoded)) = item.trim().split_once('=') else { continue };
            let algorithm = match algo.trim().to_ascii_lowercase().as_str() {
                "sha-256" => ChecksumAlgorithm::Sha256,
                "sha-512" => ChecksumAlgorithm::Sha512,
                _ => continue,
            };
            if let Some(bytes) = base64_decode(encoded.trim().trim_matches(':')) {
                return Some(Checksum { algorithm, hex: hex::encode(bytes) });
            }
        }
    }
    None
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| *c != b'=') {
        buffer = (buffer << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A checksum in the link itself: `https://…/file.iso#sha256=…`.
pub fn fragment_checksum(url: &url::Url) -> Option<Checksum> {
    url.fragment().and_then(Checksum::parse)
}

/// The `Cookie` header a Netscape cookies.txt gives for this URL.
pub fn cookie_header(cookies_txt: &str, url: &url::Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path();
    let secure = url.scheme() == "https";
    let pairs: Vec<String> = cookies_txt
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
            if line.starts_with('#') || line.trim().is_empty() {
                return None;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                return None;
            }
            let domain = fields[0].to_ascii_lowercase();
            let bare = domain.trim_start_matches('.');
            let domain_ok = host == bare || (fields[1].eq_ignore_ascii_case("TRUE") || domain.starts_with('.')) && host.ends_with(&format!(".{bare}"));
            let path_ok = path.starts_with(fields[2]);
            let secure_ok = secure || !fields[3].eq_ignore_ascii_case("TRUE");
            (domain_ok && path_ok && secure_ok).then(|| format!("{}={}", fields[5], fields[6].trim_end()))
        })
        .collect();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

struct Request {
    client: reqwest::Client,
    url: String,
    cookie: Option<String>,
}

impl Request {
    fn get(&self) -> reqwest::RequestBuilder {
        let mut builder = self.client.get(&self.url).header(header::ACCEPT_ENCODING, "identity");
        if let Some(cookie) = &self.cookie {
            builder = builder.header(header::COOKIE, cookie.as_str());
        }
        builder
    }
}

fn network(error: reqwest::Error) -> DownloadFailure {
    DownloadFailure::network(error.to_string())
}

async fn probe(request: &Request) -> Result<Probe, DownloadFailure> {
    let response = request.get().header(header::RANGE, "bytes=0-0").send().await.map_err(network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadFailure::http_status(status.as_u16()));
    }
    let headers = response.headers().clone();
    let (ranges, total) = if status == StatusCode::PARTIAL_CONTENT {
        let total = header_text(&headers, header::CONTENT_RANGE).and_then(|v| v.rsplit('/').next().and_then(|t| t.parse().ok()));
        (true, total)
    } else {
        (false, response.content_length())
    };
    drop(response);
    let filename = header_text(&headers, header::CONTENT_DISPOSITION).and_then(|v| content_disposition_filename(&v));
    Ok(Probe {
        total,
        ranges,
        etag: header_text(&headers, header::ETAG),
        last_modified: header_text(&headers, header::LAST_MODIFIED),
        filename,
        digest: digest_header(&headers),
    })
}

fn split_into(total: u64, connections: u64) -> Vec<Segment> {
    let count = connections.clamp(1, (total / MIN_SPLIT).max(1));
    let size = total / count;
    (0..count)
        .map(|i| {
            let start = i * size;
            let end = if i + 1 == count { total } else { start + size };
            Segment { start, end, pos: start, owned: false }
        })
        .collect()
}

/// Picks a segment for a free worker: an unowned one, or the back half of the biggest one left.
fn claim(segments: &mut Vec<Segment>) -> Option<usize> {
    if let Some(i) = segments.iter().position(|s| !s.owned && s.pos < s.end) {
        segments[i].owned = true;
        return Some(i);
    }
    let (i, remaining) = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.owned && s.end != u64::MAX)
        .map(|(i, s)| (i, s.end.saturating_sub(s.pos)))
        .max_by_key(|(_, r)| *r)?;
    if remaining < 2 * MIN_SPLIT {
        return None;
    }
    let mid = segments[i].pos + remaining / 2;
    let end = segments[i].end;
    segments[i].end = mid;
    segments.push(Segment { start: mid, end, pos: mid, owned: true });
    Some(segments.len() - 1)
}

enum WorkerExit {
    Done,
    Stopped,
}

async fn worker(
    request: Arc<Request>,
    part: PathBuf,
    segments: Arc<Mutex<Vec<Segment>>>,
    downloaded: Arc<AtomicU64>,
    ranges: bool,
    stop: StopSignal,
) -> Result<WorkerExit, DownloadFailure> {
    let mut file = tokio::fs::OpenOptions::new().write(true).open(&part).await.map_err(|e| DownloadFailure::internal(e.to_string()))?;
    loop {
        let Some(index) = claim(&mut segments.lock().unwrap()) else { return Ok(WorkerExit::Done) };
        let mut attempt = 0;
        loop {
            let (pos, end) = {
                let s = &segments.lock().unwrap()[index];
                (s.pos, s.end)
            };
            if pos >= end {
                break;
            }
            let mut builder = request.get();
            if ranges {
                let range = if end == u64::MAX { format!("bytes={pos}-") } else { format!("bytes={pos}-{}", end - 1) };
                builder = builder.header(header::RANGE, range);
            }
            let outcome: Result<(), DownloadFailure> = async {
                let response = builder.send().await.map_err(network)?;
                let status = response.status();
                if status == StatusCode::RANGE_NOT_SATISFIABLE {
                    return Err(DownloadFailure::new(FailureKind::ServerChanged, "The file changed on the server.", Some("Retry to start it again.")));
                }
                if !status.is_success() {
                    return Err(DownloadFailure::http_status(status.as_u16()));
                }
                if ranges && status != StatusCode::PARTIAL_CONTENT && pos > 0 {
                    return Err(DownloadFailure::new(FailureKind::ServerChanged, "The server stopped resuming downloads.", Some("Retry to start it again.")));
                }
                file.seek(std::io::SeekFrom::Start(pos)).await.map_err(|e| DownloadFailure::internal(e.to_string()))?;
                let mut stream = response.bytes_stream();
                loop {
                    let chunk = tokio::select! {
                        chunk = stream.next() => chunk,
                        _ = stop.stopped() => return Ok(()),
                    };
                    let Some(chunk) = chunk else { break };
                    let chunk = chunk.map_err(network)?;
                    // Reserve the bytes under the lock (a stealer may have moved `end`), write outside it.
                    let allowed = {
                        let mut all = segments.lock().unwrap();
                        let s = &mut all[index];
                        let allowed = (chunk.len() as u64).min(s.end.saturating_sub(s.pos)) as usize;
                        s.pos += allowed as u64;
                        allowed
                    };
                    if let Err(e) = file.write_all(&chunk[..allowed]).await {
                        return Err(DownloadFailure::folder_unavailable(&part.display().to_string()).with_detail(e.to_string()));
                    }
                    downloaded.fetch_add(allowed as u64, Ordering::Relaxed);
                    if allowed < chunk.len() {
                        break;
                    }
                }
                Ok(())
            }
            .await;

            if stop.is_stopped() {
                file.flush().await.ok();
                return Ok(WorkerExit::Stopped);
            }
            match outcome {
                Ok(()) => {
                    let s = &mut segments.lock().unwrap()[index];
                    if s.end == u64::MAX {
                        // Unknown size: the stream ending is the end.
                        s.end = s.pos;
                    }
                    if s.pos >= s.end {
                        break;
                    }
                    // The server closed early; go round again from where it stopped.
                }
                Err(failure) if failure.is_transient() && attempt < RETRIES => {
                    attempt += 1;
                    let wait = Duration::from_secs(1 << attempt.min(4));
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => {}
                        _ = stop.stopped() => return Ok(WorkerExit::Stopped),
                    }
                }
                Err(failure) => return Err(failure),
            }
            if !ranges {
                // No ranges: a dropped connection means starting over.
                return Err(DownloadFailure::network("the connection dropped and the server can't resume"));
            }
        }
        file.flush().await.map_err(|e| DownloadFailure::internal(e.to_string()))?;
        segments.lock().unwrap()[index].owned = false;
    }
}

fn save_state(path: &Path, state: &PartState, segments: &[Segment]) {
    let mut state = state.clone();
    state.segments = segments.to_vec();
    if let Ok(bytes) = serde_json::to_vec(&state) {
        let tmp = path.with_extension("dawnload-state-tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

async fn hash_file(path: PathBuf, algorithm: ChecksumAlgorithm) -> std::io::Result<String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut buf = vec![0u8; 1 << 20];
        let mut sha256 = sha2::Sha256::new();
        let mut sha512 = sha2::Sha512::new();
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            match algorithm {
                ChecksumAlgorithm::Sha256 => sha256.update(&buf[..n]),
                ChecksumAlgorithm::Sha512 => sha512.update(&buf[..n]),
            }
        }
        Ok(match algorithm {
            ChecksumAlgorithm::Sha256 => hex::encode(sha256.finalize()),
            ChecksumAlgorithm::Sha512 => hex::encode(sha512.finalize()),
        })
    })
    .await
    .map_err(std::io::Error::other)?
}

async fn run(client: reqwest::Client, request: JobRequest, ctx: JobContext) -> JobResult {
    let DownloadSource::Web { url } = &request.source else { return Err(DownloadFailure::torrents_unavailable().into()) };
    let parsed = url::Url::parse(url).map_err(|_| DownloadFailure::not_a_link())?;
    let options = &request.options;
    let destination = options.destination.clone();
    std::fs::create_dir_all(&destination).map_err(|_| DownloadFailure::folder_unavailable(&destination.display().to_string()))?;

    let cookie = match &options.cookies {
        Cookies::File(path) => {
            let text = std::fs::read_to_string(path).map_err(|_| DownloadFailure::cookies_unreadable())?;
            cookie_header(&text, &parsed)
        }
        Cookies::Browser(spec) => {
            ctx.log(format!("Browser cookies ({spec}) work with yt-dlp and gallery-dl; this download goes without them."));
            None
        }
        Cookies::None => None,
    };
    let mut fetch_url = parsed.clone();
    fetch_url.set_fragment(None);
    let req = Arc::new(Request { client, url: fetch_url.to_string(), cookie });
    ctx.emit(JobEvent::Command(format!("GET {}", req.url)));
    ctx.phase(Status::Pulling, Some("connecting"));

    let probe = {
        let mut attempt = 0;
        loop {
            match probe(&req).await {
                Ok(p) => break p,
                Err(f) if f.is_transient() && attempt < 3 && !ctx.stop.is_stopped() => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                }
                Err(f) => return Err(f.into()),
            }
        }
    };
    if ctx.stop.is_stopped() {
        return Err(JobError::Stopped);
    }

    // Where it goes: the previous run's file if there's one to continue, else a fresh unique name.
    let previous = request.files.first().map(|f| PathBuf::from(&f.path)).filter(|p| part_path(p).exists() && state_path(p).exists());
    let target = match previous {
        Some(p) => p,
        None => {
            let name = probe
                .filename
                .clone()
                .or_else(|| parsed.path_segments().and_then(|mut s| s.next_back()).map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()))
                .map(|n| filename::limited_component(&n, 200))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "download".into());
            let mut reserved = std::collections::HashSet::new();
            loop {
                let candidate = filename::unique_path(&destination.join(&name), &reserved);
                if !part_path(&candidate).exists() {
                    break candidate;
                }
                reserved.insert(candidate);
            }
        }
    };
    let part = part_path(&target);
    let state_file = state_path(&target);
    let display_name = target.file_name().unwrap_or_default().to_string_lossy().into_owned();
    ctx.emit(JobEvent::Title(display_name.clone()));

    let fresh = PartState {
        url: req.url.clone(),
        total: probe.total,
        etag: probe.etag.clone(),
        last_modified: probe.last_modified.clone(),
        segments: vec![],
    };
    let mut state: Option<PartState> = std::fs::read(&state_file).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(saved) = &state {
        let same = saved.total == probe.total
            && probe.ranges
            && (saved.etag.is_none() || saved.etag == probe.etag)
            && (saved.last_modified.is_none() || saved.last_modified == probe.last_modified);
        if same {
            let done: u64 = saved.segments.iter().map(|s| s.pos - s.start).sum();
            ctx.log(format!("Continuing {display_name} from {}.", crate::format::bytes(done)));
        } else {
            ctx.log("The file changed on the server (or can't be resumed); starting over.");
            state = None;
        }
    }
    let resumed = state.is_some() && part.exists();
    let mut segments = match (&state, resumed) {
        (Some(saved), true) => saved.segments.iter().cloned().map(|mut s| { s.owned = false; s }).collect(),
        _ => match (probe.total, probe.ranges) {
            (Some(total), true) => split_into(total, options.connections.max(1) as u64),
            (Some(total), false) => vec![Segment { start: 0, end: total, pos: 0, owned: false }],
            (None, _) => vec![Segment { start: 0, end: u64::MAX, pos: 0, owned: false }],
        },
    };
    if !resumed {
        let file = std::fs::File::create(&part).map_err(|e| DownloadFailure::folder_unavailable(&destination.display().to_string()).with_detail(e.to_string()))?;
        if let Some(total) = probe.total {
            file.set_len(total).map_err(|e| DownloadFailure::folder_unavailable(&destination.display().to_string()).with_detail(e.to_string()))?;
        }
    }
    segments.retain(|s| s.pos < s.end || s.end == u64::MAX);
    let resumable = probe.ranges && probe.total.is_some();
    let state = fresh;
    if resumable {
        save_state(&state_file, &state, &segments);
    }

    let initial_done: u64 = match probe.total {
        Some(total) => total - segments.iter().map(|s| s.end - s.pos).sum::<u64>(),
        None => 0,
    };
    let downloaded = Arc::new(AtomicU64::new(initial_done));
    let file_entry = |done: u64| ItemFile { path: target.display().to_string(), size: probe.total, bytes_done: Some(done), selected: true };
    ctx.emit(JobEvent::Files(vec![file_entry(initial_done)]));
    let connections = if resumable { (options.connections.max(1) as usize).max(1) } else { 1 };
    ctx.phase(Status::Pulling, None);
    if resumable && connections > 1 {
        ctx.log(format!("Downloading {display_name} over {connections} connections."));
    }

    let shared = Arc::new(Mutex::new(segments));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..connections {
        workers.spawn(worker(req.clone(), part.clone(), shared.clone(), downloaded.clone(), probe.ranges, ctx.stop.clone()));
    }

    let started = Instant::now();
    let mut last = (Instant::now(), initial_done);
    let mut speed: Option<f64> = None;
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    let mut last_save = Instant::now();
    let mut failure: Option<DownloadFailure> = None;
    let mut stopped = false;

    loop {
        tokio::select! {
            joined = workers.join_next() => match joined {
                None => break,
                Some(Ok(Ok(WorkerExit::Done))) => {}
                Some(Ok(Ok(WorkerExit::Stopped))) => stopped = true,
                Some(Ok(Err(f))) => {
                    if failure.is_none() { failure = Some(f); }
                    ctx.stop.stop(); // the other connections stop at a safe point
                }
                Some(Err(e)) => {
                    if failure.is_none() { failure = Some(DownloadFailure::internal(e.to_string())); }
                    ctx.stop.stop();
                }
            },
            _ = ticker.tick() => {
                let done = downloaded.load(Ordering::Relaxed);
                let now = Instant::now();
                let dt = now.duration_since(last.0).as_secs_f64();
                if dt >= 0.2 {
                    let instant = (done - last.1) as f64 / dt;
                    speed = Some(match speed { Some(s) => s * 0.7 + instant * 0.3, None => instant });
                    last = (now, done);
                }
                let total = probe.total;
                let fraction = total.filter(|t| *t > 0).map(|t| (done as f64 / t as f64).clamp(0.0, 1.0));
                let seconds_left = match (total, speed) {
                    (Some(t), Some(s)) if s > 1.0 && t >= done => Some(((t - done) as f64 / s).round() as u64),
                    _ => None,
                };
                ctx.emit(JobEvent::Transfer(Transfer {
                    fraction, bytes_done: Some(done), bytes_total: total, bytes_per_second: speed.filter(|_| started.elapsed() > Duration::from_millis(400)),
                    seconds_left, entry_index: None, entry_count: None,
                }));
                if resumable && last_save.elapsed() > Duration::from_secs(2) {
                    save_state(&state_file, &state, &shared.lock().unwrap());
                    last_save = Instant::now();
                }
            }
        }
    }

    let segments = shared.lock().unwrap().clone();
    if let Some(failure) = failure {
        if failure.kind == FailureKind::ServerChanged || failure.kind == FailureKind::FolderUnavailable {
            let _ = std::fs::remove_file(&state_file);
            let _ = std::fs::remove_file(&part);
        } else if resumable {
            save_state(&state_file, &state, &segments);
        }
        return Err(failure.into());
    }
    if stopped || ctx.stop.is_stopped() {
        if resumable {
            save_state(&state_file, &state, &segments);
        } else {
            ctx.log("This server can't resume, so Resume starts the file again.");
            let _ = std::fs::remove_file(&part);
        }
        ctx.emit(JobEvent::Files(vec![file_entry(downloaded.load(Ordering::Relaxed))]));
        return Err(JobError::Stopped);
    }
    let incomplete = segments.iter().any(|s| s.pos < s.end);
    if incomplete {
        save_state(&state_file, &state, &segments);
        return Err(DownloadFailure::network("the server closed the connection early").into());
    }
    let written = downloaded.load(Ordering::Relaxed);
    if probe.total.is_none() {
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&part) {
            let _ = f.set_len(written);
        }
    }

    let expected = options.checksum.clone().or_else(|| fragment_checksum(&parsed)).or(probe.digest.clone());
    if let Some(expected) = expected {
        ctx.phase(Status::Pulling, Some("checking the checksum"));
        let actual = hash_file(part.clone(), expected.algorithm).await.map_err(|e| DownloadFailure::internal(e.to_string()))?;
        if actual != expected.hex {
            let _ = std::fs::remove_file(&part);
            let _ = std::fs::remove_file(&state_file);
            return Err(DownloadFailure::checksum_mismatch(&expected.hex, &actual).into());
        }
        ctx.log(format!("Checksum matches ({expected})."));
    }

    let final_path = if target.exists() { filename::unique_path(&target, &Default::default()) } else { target.clone() };
    std::fs::rename(&part, &final_path).map_err(|e| DownloadFailure::folder_unavailable(&destination.display().to_string()).with_detail(e.to_string()))?;
    let _ = std::fs::remove_file(&state_file);
    Ok(JobOutcome { files: vec![final_path], polished: 0 })
}

#[cfg(test)]
pub(crate) mod test_server {
    //! A tiny HTTP/1.1 file server for tests: HEAD/GET, Range, optional throttling.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone)]
    pub struct Server {
        pub url: String,
        pub requests: Arc<AtomicUsize>,
    }

    pub async fn serve(body: Vec<u8>, ranges: bool, delay_per_chunk_ms: u64) -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = Arc::new(body);
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let body = body.clone();
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let mut read = 0;
                    loop {
                        let Ok(n) = socket.read(&mut buf[read..]).await else { return };
                        if n == 0 { return; }
                        read += n;
                        if buf[..read].windows(4).any(|w| w == b"\r\n\r\n") { break; }
                    }
                    let text = String::from_utf8_lossy(&buf[..read]).to_string();
                    let head = text.starts_with("HEAD");
                    let range = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(str::to_string));
                    let total = body.len();
                    let (status, start, end) = match range.filter(|_| ranges) {
                        Some(r) => {
                            let (a, b) = r.trim().split_once('-').unwrap();
                            let a: usize = a.parse().unwrap();
                            let b: usize = if b.is_empty() { total - 1 } else { b.parse::<usize>().unwrap().min(total - 1) };
                            ("206 Partial Content", a, b + 1)
                        }
                        None => ("200 OK", 0, total),
                    };
                    let mut header = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nETag: \"v1\"\r\nContent-Disposition: attachment; filename=\"test file.bin\"\r\nConnection: close\r\n", end - start);
                    if ranges { header.push_str("Accept-Ranges: bytes\r\n"); }
                    if status.starts_with("206") { header.push_str(&format!("Content-Range: bytes {start}-{}/{total}\r\n", end - 1)); }
                    header.push_str("\r\n");
                    if socket.write_all(header.as_bytes()).await.is_err() || head { return; }
                    for chunk in body[start..end].chunks(16 * 1024) {
                        if socket.write_all(chunk).await.is_err() { return; }
                        if delay_per_chunk_ms > 0 { tokio::time::sleep(std::time::Duration::from_millis(delay_per_chunk_ms)).await; }
                    }
                });
            }
        });
        Server { url: format!("http://{addr}/files/test.bin"), requests }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ItemId;
    use tokio::sync::mpsc;

    fn body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn sha256(data: &[u8]) -> String {
        hex::encode(sha2::Sha256::digest(data))
    }

    fn request(url: &str, dir: &Path, files: Vec<ItemFile>) -> JobRequest {
        JobRequest {
            item_id: ItemId(1),
            source: DownloadSource::Web { url: url.into() },
            options: Options { destination: dir.into(), connections: 4, ..Options::default() },
            tools: ToolSnapshot::default(),
            files,
            resuming: false,
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(content_disposition_filename("attachment; filename=\"a.zip\"").as_deref(), Some("a.zip"));
        assert_eq!(content_disposition_filename("attachment; filename=\"a.zip\"; filename*=UTF-8''a%20b.zip").as_deref(), Some("a b.zip"));
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        let url = url::Url::parse(&format!("https://x.test/a.iso#sha256={}", "a".repeat(64))).unwrap();
        assert_eq!(fragment_checksum(&url).unwrap().hex, "a".repeat(64));
        let cookies = "# Netscape HTTP Cookie File\n.example.com\tTRUE\t/\tTRUE\t0\tsid\tabc\n#HttpOnly_files.example.com\tFALSE\t/dl\tFALSE\t0\tk\tv\nother.com\tFALSE\t/\tFALSE\t0\tno\tpe\n";
        let url = url::Url::parse("https://files.example.com/dl/x.zip").unwrap();
        assert_eq!(cookie_header(cookies, &url).as_deref(), Some("sid=abc; k=v"));
        let mut segments = split_into(10 * MIN_SPLIT, 4);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments.last().unwrap().end, 10 * MIN_SPLIT);
        for _ in 0..4 { claim(&mut segments).unwrap(); }
        let stolen = claim(&mut segments).unwrap();
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[stolen].end, 10 * MIN_SPLIT);
        assert!(segments[3].end < segments[stolen].end && segments[3].end == segments[stolen].start);
    }

    async fn run_job(req: JobRequest, stop: StopSignal) -> (JobResult, Vec<JobEvent>) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let ctx = JobContext::new(ItemId(1), 1, stop, tx);
        let result = HttpEngine::new().run(req, ctx).await;
        let mut events = Vec::new();
        while let Ok((_, _, e)) = rx.try_recv() { events.push(e); }
        (result, events)
    }

    #[tokio::test]
    async fn multi_connection_download_with_checksum() {
        let data = body(5 * MIN_SPLIT as usize + 123);
        let server = test_server::serve(data.clone(), true, 0).await;
        let dir = tempfile::tempdir().unwrap();
        let mut req = request(&server.url, dir.path(), vec![]);
        req.options.checksum = Some(Checksum { algorithm: ChecksumAlgorithm::Sha256, hex: sha256(&data) });
        let (result, events) = run_job(req, StopSignal::new()).await;
        let outcome = result.unwrap();
        assert_eq!(outcome.files, vec![dir.path().join("test file.bin")]);
        assert_eq!(std::fs::read(&outcome.files[0]).unwrap(), data);
        assert!(server.requests.load(std::sync::atomic::Ordering::SeqCst) >= 5, "probe + 4 connections");
        assert!(events.iter().any(|e| matches!(e, JobEvent::Log(l) if l.contains("Checksum matches"))));
        assert!(!part_path(&outcome.files[0]).exists() && !state_path(&outcome.files[0]).exists());
    }

    #[tokio::test]
    async fn wrong_checksum_is_not_kept() {
        let data = body(300_000);
        let server = test_server::serve(data, true, 0).await;
        let dir = tempfile::tempdir().unwrap();
        let mut req = request(&server.url, dir.path(), vec![]);
        req.options.checksum = Some(Checksum { algorithm: ChecksumAlgorithm::Sha256, hex: "0".repeat(64) });
        let (result, _) = run_job(req, StopSignal::new()).await;
        assert!(matches!(result, Err(JobError::Failed(f)) if f.kind == FailureKind::ChecksumMismatch));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn pause_then_resume_continues_the_part_file() {
        let data = body(4 * MIN_SPLIT as usize);
        let server = test_server::serve(data.clone(), true, 20).await;
        let dir = tempfile::tempdir().unwrap();

        let stop = StopSignal::new();
        let stopper = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stopper.stop();
        });
        let (result, events) = run_job(request(&server.url, dir.path(), vec![]), stop).await;
        assert_eq!(result, Err(JobError::Stopped));
        let target = dir.path().join("test file.bin");
        assert!(part_path(&target).exists() && state_path(&target).exists());
        let files = events.iter().rev().find_map(|e| if let JobEvent::Files(f) = e { Some(f.clone()) } else { None }).unwrap();
        let partial = files[0].bytes_done.unwrap();
        assert!(partial > 0 && partial < data.len() as u64, "{partial}");

        let (result, events) = run_job(request(&server.url, dir.path(), files), StopSignal::new()).await;
        let outcome = result.unwrap();
        assert_eq!(outcome.files, vec![target.clone()]);
        assert_eq!(std::fs::read(&target).unwrap(), data);
        assert!(events.iter().any(|e| matches!(e, JobEvent::Log(l) if l.starts_with("Continuing"))));
    }

    #[tokio::test]
    async fn server_without_ranges_uses_one_connection() {
        let data = body(2 * MIN_SPLIT as usize);
        let server = test_server::serve(data.clone(), false, 0).await;
        let dir = tempfile::tempdir().unwrap();
        let (result, _) = run_job(request(&server.url, dir.path(), vec![]), StopSignal::new()).await;
        assert_eq!(std::fs::read(&result.unwrap().files[0]).unwrap(), data);
        assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 2, "probe + one stream");
    }

    #[tokio::test]
    async fn missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut s, _)) = listener.accept().await {
                let mut b = [0u8; 4096];
                let _ = s.read(&mut b).await;
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
        });
        let (result, _) = run_job(request(&format!("http://{addr}/x.zip"), dir.path(), vec![]), StopSignal::new()).await;
        assert!(matches!(result, Err(JobError::Failed(f)) if f.kind == FailureKind::NotFound));
    }
}

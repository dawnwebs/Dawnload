//! The tools Dawnload runs (yt-dlp, gallery-dl, ffmpeg, ffprobe, a JavaScript runtime): finding
//! them, installing verified official builds into the app data folder, and keeping them fresh.
//!
//! Resolution order for each tool: a path the person set > Dawnload's managed copy > the system
//! (PATH, Homebrew, winget, scoop, ~/.deno). Managed copies win over system ones because Dawnload
//! keeps them current; yt-dlp in particular breaks within weeks when it's stale.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::process::{StopSignal, run_tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolId {
    YtDlp,
    GalleryDl,
    Ffmpeg,
    Ffprobe,
    Deno,
}

impl ToolId {
    pub const ALL: [ToolId; 5] = [ToolId::YtDlp, ToolId::GalleryDl, ToolId::Ffmpeg, ToolId::Ffprobe, ToolId::Deno];

    pub fn name(self) -> &'static str {
        match self {
            ToolId::YtDlp => "yt-dlp",
            ToolId::GalleryDl => "gallery-dl",
            ToolId::Ffmpeg => "ffmpeg",
            ToolId::Ffprobe => "ffprobe",
            ToolId::Deno => "deno",
        }
    }

    pub fn parse(text: &str) -> Option<ToolId> {
        ToolId::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(text))
    }

    pub fn purpose(self) -> &'static str {
        match self {
            ToolId::YtDlp => "Video and audio from most sites",
            ToolId::GalleryDl => "Galleries and image sets",
            ToolId::Ffmpeg => "Merging, remuxing and audio",
            ToolId::Ffprobe => "Reading media for yt-dlp",
            ToolId::Deno => "JavaScript runtime yt-dlp needs for YouTube",
        }
    }

    pub fn executable(self) -> String {
        if cfg!(windows) { format!("{}.exe", self.name()) } else { self.name().to_string() }
    }

    fn version_arguments(self) -> Vec<String> {
        match self {
            ToolId::Ffmpeg | ToolId::Ffprobe => vec!["-version".into()],
            _ => vec!["--version".into()],
        }
    }

    /// ffmpeg and ffprobe ship together; installing one installs both.
    fn package(self) -> ToolId {
        if self == ToolId::Ffprobe { ToolId::Ffmpeg } else { self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolOrigin {
    /// Installed and kept up to date by Dawnload.
    Managed,
    /// Found on the system (PATH, Homebrew, winget…). Its package manager updates it.
    System,
    /// A path the person chose.
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JsRuntimeKind {
    Deno,
    Node,
    Bun,
    QuickJs,
}

impl JsRuntimeKind {
    /// The name yt-dlp's `--js-runtimes` uses.
    pub fn ytdlp_name(self) -> &'static str {
        match self {
            JsRuntimeKind::Deno => "deno",
            JsRuntimeKind::Node => "node",
            JsRuntimeKind::Bun => "bun",
            JsRuntimeKind::QuickJs => "quickjs",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsRuntime {
    pub kind: JsRuntimeKind,
    pub path: PathBuf,
}

/// The tools found at one moment, plus the environment to run them in.
#[derive(Clone, Debug, Default)]
pub struct ToolSnapshot {
    pub paths: BTreeMap<ToolId, PathBuf>,
    pub origins: BTreeMap<ToolId, ToolOrigin>,
    pub js_runtime: Option<JsRuntime>,
    pub env: Vec<(String, String)>,
}

impl ToolSnapshot {
    pub fn path(&self, tool: ToolId) -> Option<&Path> {
        self.paths.get(&tool).map(PathBuf::as_path)
    }

    pub fn has(&self, tool: ToolId) -> bool {
        self.paths.contains_key(&tool)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: ToolId,
    pub purpose: String,
    pub path: Option<PathBuf>,
    pub origin: Option<ToolOrigin>,
    pub version: Option<String>,
    /// Filled by an update check.
    pub latest: Option<String>,
    pub update_available: bool,
    /// Dawnload can install a managed copy on this OS.
    pub installable: bool,
}

/// What Dawnload installed, so updates know what's there.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub tools: BTreeMap<ToolId, ManagedTool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManagedTool {
    pub version: String,
    pub path: PathBuf,
    pub source: String,
    pub sha256: String,
    pub installed_at: i64,
}

#[derive(Debug)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

fn err(message: impl Into<String>) -> ToolError {
    ToolError(message.into())
}

/// Progress while installing: (tool, bytes done, bytes total).
pub type InstallProgress<'a> = &'a mut (dyn FnMut(ToolId, u64, Option<u64>) + Send);

#[derive(Clone, Debug)]
pub struct ToolManager {
    dir: PathBuf,
    overrides: BTreeMap<ToolId, PathBuf>,
    /// Folders to search for system tools. None means the real PATH plus the usual places.
    system_search: Option<Vec<PathBuf>>,
}

impl ToolManager {
    pub fn new(tools_dir: PathBuf) -> Self {
        ToolManager { dir: tools_dir, overrides: BTreeMap::new(), system_search: None }
    }

    /// Only these folders count as "the system" (tests).
    pub fn with_system_search(mut self, folders: Vec<PathBuf>) -> Self {
        self.system_search = Some(folders);
        self
    }

    pub fn set_override(&mut self, tool: ToolId, path: Option<PathBuf>) {
        match path {
            Some(p) => self.overrides.insert(tool, p),
            None => self.overrides.remove(&tool),
        };
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn bin_dir(&self) -> PathBuf {
        self.dir.join("bin")
    }

    fn venv_dir(&self, tool: ToolId) -> PathBuf {
        self.dir.join("venv").join(tool.name())
    }

    fn venv_executable(&self, tool: ToolId) -> PathBuf {
        let venv = self.venv_dir(tool);
        if cfg!(windows) { venv.join("Scripts").join(tool.executable()) } else { venv.join("bin").join(tool.executable()) }
    }

    pub fn manifest(&self) -> Manifest {
        std::fs::read(self.dir.join("manifest.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_manifest(&self, manifest: &Manifest) -> Result<(), ToolError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| err(format!("Can't create {}: {e}", self.dir.display())))?;
        let tmp = self.dir.join("manifest.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(manifest).unwrap_or_default()).map_err(|e| err(e.to_string()))?;
        std::fs::rename(&tmp, self.dir.join("manifest.json")).map_err(|e| err(e.to_string()))
    }

    /// The folders searched for system tools, in order.
    pub fn search_folders(&self) -> Vec<PathBuf> {
        if let Some(folders) = &self.system_search {
            return folders.clone();
        }
        let mut folders: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        let home = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
        // A GUI app doesn't inherit the shell's PATH, so look where package managers put things.
        if cfg!(target_os = "macos") {
            folders.extend(["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(PathBuf::from));
        } else if cfg!(windows) {
            if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
                folders.push(local.join("Microsoft").join("WinGet").join("Links"));
            }
            if let Some(home) = &home {
                folders.push(home.join("scoop").join("shims"));
            }
            folders.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
        } else {
            folders.extend(["/usr/local/bin", "/usr/bin", "/home/linuxbrew/.linuxbrew/bin", "/snap/bin"].map(PathBuf::from));
            if let Some(home) = &home {
                folders.push(home.join(".local").join("bin"));
            }
        }
        if let Some(home) = &home {
            folders.push(home.join(".deno").join("bin"));
            folders.push(home.join(".bun").join("bin"));
        }
        let mut seen = std::collections::HashSet::new();
        folders.retain(|f| seen.insert(f.clone()));
        folders
    }

    fn find_on_system(&self, executable: &str) -> Option<PathBuf> {
        self.search_folders().into_iter().map(|f| f.join(executable)).find(|p| is_executable(p))
    }

    fn managed_path(&self, tool: ToolId) -> Option<PathBuf> {
        let bin = self.bin_dir().join(tool.executable());
        if is_executable(&bin) {
            return Some(bin);
        }
        let venv = self.venv_executable(tool);
        is_executable(&venv).then_some(venv)
    }

    /// Finds every tool. Only looks at files; fast enough to call before each download.
    pub fn discover(&self) -> ToolSnapshot {
        let mut snapshot = ToolSnapshot::default();
        for tool in ToolId::ALL {
            let found = self
                .overrides
                .get(&tool)
                .filter(|p| is_executable(p))
                .map(|p| (p.clone(), ToolOrigin::Custom))
                .or_else(|| self.managed_path(tool).map(|p| (p, ToolOrigin::Managed)))
                .or_else(|| self.find_on_system(&tool.executable()).map(|p| (p, ToolOrigin::System)));
            if let Some((path, origin)) = found {
                snapshot.paths.insert(tool, path);
                snapshot.origins.insert(tool, origin);
            }
        }
        snapshot.js_runtime =
            snapshot.path(ToolId::Deno).map(|p| JsRuntime { kind: JsRuntimeKind::Deno, path: p.to_path_buf() }).or_else(|| {
                [(JsRuntimeKind::Node, "node"), (JsRuntimeKind::Bun, "bun"), (JsRuntimeKind::QuickJs, "qjs")]
                    .into_iter()
                    .find_map(|(kind, name)| {
                        let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
                        self.find_on_system(&exe).map(|path| JsRuntime { kind, path })
                    })
            });

        // Tools find each other (yt-dlp → ffmpeg, deno) through PATH: managed first.
        let mut path_dirs = vec![self.bin_dir()];
        path_dirs.extend(snapshot.paths.values().filter_map(|p| p.parent().map(Path::to_path_buf)));
        path_dirs.extend(self.search_folders());
        let mut seen = std::collections::HashSet::new();
        path_dirs.retain(|d| seen.insert(d.clone()));
        if let Ok(joined) = std::env::join_paths(&path_dirs) {
            snapshot.env.push(("PATH".into(), joined.to_string_lossy().into_owned()));
        }
        // Keep tool output machine-readable and unbuffered.
        snapshot.env.push(("PYTHONUNBUFFERED".into(), "1".into()));
        snapshot.env.push(("PYTHONIOENCODING".into(), "utf-8".into()));
        snapshot
    }

    /// Each tool's path, origin and version. Runs `--version`, so it takes a moment.
    pub async fn status(&self) -> Vec<ToolStatus> {
        let snapshot = self.discover();
        let manifest = self.manifest();
        let mut out = Vec::new();
        for tool in ToolId::ALL {
            let path = snapshot.path(tool).map(Path::to_path_buf);
            let origin = snapshot.origins.get(&tool).copied();
            let version = match (&path, origin) {
                (Some(_), Some(ToolOrigin::Managed)) if manifest.tools.contains_key(&tool) => {
                    Some(manifest.tools[&tool].version.clone())
                }
                (Some(p), _) => version_of(tool, p).await,
                _ => None,
            };
            out.push(ToolStatus {
                tool,
                purpose: tool.purpose().to_string(),
                path,
                origin,
                version,
                latest: None,
                update_available: false,
                installable: release_plan(tool.package()).is_some(),
            });
        }
        out
    }

    /// `status` plus the newest release of each tool Dawnload can install.
    pub async fn check_updates(&self) -> Vec<ToolStatus> {
        let client = crate::net::client();
        let mut statuses = self.status().await;
        let mut latest_by_package: BTreeMap<ToolId, Option<String>> = BTreeMap::new();
        for status in &mut statuses {
            let package = status.tool.package();
            if let std::collections::btree_map::Entry::Vacant(e) = latest_by_package.entry(package) {
                let latest = match release_plan(package) {
                    Some(plan) => resolve(&client, &plan).await.ok().map(|r| r.version),
                    None => None,
                };
                e.insert(latest);
            }
            status.latest = latest_by_package[&package].clone();
            status.update_available = match (&status.latest, &status.version, status.origin) {
                (Some(latest), Some(current), Some(_)) => !same_version(latest, current),
                _ => false,
            };
        }
        statuses
    }

    /// Installs (or updates) Dawnload's own copy of a tool, verified against the publisher's
    /// checksum and test-run before it replaces anything. Returns the version installed.
    pub async fn install(&self, tool: ToolId, progress: InstallProgress<'_>) -> Result<String, ToolError> {
        let package = tool.package();
        let plan = release_plan(package).ok_or_else(|| {
            err(format!("Dawnload can't install {} on this system yet; install it with your package manager.", tool.name()))
        })?;
        let client = crate::net::client();
        let release = resolve(&client, &plan).await?;
        let tmp = self.dir.join("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| err(format!("Can't create {}: {e}", tmp.display())))?;
        std::fs::create_dir_all(self.bin_dir()).map_err(|e| err(e.to_string()))?;
        let mut manifest = self.manifest();

        if let ReleaseKind::PipVenv { package: pip_name } = &plan.kind {
            let path = self.install_venv(&client, tool, pip_name, &release.version, &tmp).await;
            let _ = std::fs::remove_dir_all(&tmp);
            let path = path?;
            manifest.tools.insert(
                tool,
                ManagedTool {
                    version: release.version.clone(),
                    path,
                    source: format!("pypi:{pip_name}=={}", release.version),
                    sha256: String::new(),
                    installed_at: crate::model::now_millis(),
                },
            );
            self.save_manifest(&manifest)?;
            return Ok(release.version);
        }

        for download in &release.downloads {
            let archive_path =
                tmp.join(format!("download-{}", download.provides.first().map(|(_, t)| t.name()).unwrap_or("tool")));
            let actual = fetch_to_file(&client, &download.url, &archive_path, &mut |done, total| {
                progress(download.provides[0].1, done, total)
            })
            .await?;
            if !actual.eq_ignore_ascii_case(&download.sha256) {
                return Err(err(format!(
                    "The download of {} didn't match its published checksum (expected {}, got {actual}). Nothing was installed.",
                    download.url, download.sha256
                )));
            }
            let archive = download.archive;
            let archive_for_extract = archive_path.clone();
            let members: Vec<(String, ToolId)> = download.provides.clone();
            let out_dir = tmp.clone();
            let extracted = tokio::task::spawn_blocking(move || extract(&archive_for_extract, archive, &members, &out_dir))
                .await
                .map_err(|e| err(e.to_string()))??;

            for (tool_id, file) in extracted {
                make_executable(&file)?;
                // Test-run it before it replaces anything.
                if version_of(tool_id, &file).await.is_none() {
                    return Err(err(format!(
                        "The downloaded {} didn't run on this system. Nothing was installed.",
                        tool_id.name()
                    )));
                }
                let target = self.bin_dir().join(tool_id.executable());
                std::fs::rename(&file, &target)
                    .or_else(|_| std::fs::copy(&file, &target).map(|_| ()))
                    .map_err(|e| err(format!("Can't put {} in place (is it running?): {e}", target.display())))?;
                manifest.tools.insert(
                    tool_id,
                    ManagedTool {
                        version: release.version.clone(),
                        path: target,
                        source: download.url.clone(),
                        sha256: download.sha256.clone(),
                        installed_at: crate::model::now_millis(),
                    },
                );
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        self.save_manifest(&manifest)?;
        Ok(release.version)
    }

    /// Puts a Python package in its own virtual environment. The environment is made without pip
    /// and pip runs from its own wheel, fetched from PyPI and checked against PyPI's digest:
    /// Debian and Ubuntu leave `ensurepip` out of their Python (it's in `python3-venv`, which a
    /// stock desktop doesn't have), so `python3 -m venv` with pip fails there.
    async fn install_venv(
        &self,
        client: &reqwest::Client,
        tool: ToolId,
        package: &str,
        version: &str,
        tmp: &Path,
    ) -> Result<PathBuf, ToolError> {
        let folders = self.search_folders();
        let python = ["python3", "python"]
            .iter()
            .map(|name| if cfg!(windows) { format!("{name}.exe") } else { name.to_string() })
            .flat_map(|name| folders.iter().map(move |folder| folder.join(&name)))
            // On a clean Windows, python.exe in …\Microsoft\WindowsApps is a Store stand-in that
            // only prints "Python was not found" and offers the Store. It isn't Python.
            .find(|path| is_executable(path) && !is_store_alias(path))
            .ok_or_else(|| {
                err(format!(
                    "{} needs Python 3 on this system; install Python, or install {} with your package manager.",
                    tool.name(),
                    tool.name()
                ))
            })?;
        let venv = self.venv_dir(tool);
        let stop = StopSignal::new();
        let cwd = self.dir.clone();
        let run = |exe: PathBuf, args: Vec<String>| {
            let stop = stop.clone();
            let cwd = cwd.clone();
            async move {
                run_tool(&exe, &args, &[], &cwd, &stop, |_| {})
                    .await
                    .map_err(|e| err(format!("{e:?}")))
                    .and_then(|o| if o.exit_code == 0 { Ok(o) } else { Err(err(o.output)) })
            }
        };
        let wheel = pip_wheel(&get_json(client, "https://pypi.org/pypi/pip/json").await?)
            .ok_or_else(|| err("PyPI listed no pip wheel"))?;
        let wheel_path = tmp.join(&wheel.filename);
        let actual = fetch_to_file(client, &wheel.url, &wheel_path, &mut |_, _| {}).await?;
        if !actual.eq_ignore_ascii_case(&wheel.sha256) {
            return Err(err(format!(
                "The download of {} didn't match PyPI's checksum (expected {}, got {actual}). Nothing was installed.",
                wheel.url, wheel.sha256
            )));
        }
        run(python, vec!["-m".into(), "venv".into(), "--clear".into(), "--without-pip".into(), venv.display().to_string()])
            .await?;
        let venv_python = if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python") };
        // pip runs from its wheel (pip's documented bootstrap) and installs into the environment
        // whose Python runs it.
        let pip = wheel_path.join("pip").display().to_string();
        run(
            venv_python,
            vec![
                pip,
                "install".into(),
                "--upgrade".into(),
                "--disable-pip-version-check".into(),
                "--no-input".into(),
                format!("{package}=={version}"),
            ],
        )
        .await?;
        let exe = self.venv_executable(tool);
        if version_of(tool, &exe).await.is_none() {
            return Err(err(format!("{} installed but doesn't run.", tool.name())));
        }
        Ok(exe)
    }

    /// Updates every managed tool that has a newer release. Returns (tool, new version).
    pub async fn update_managed(&self, progress: InstallProgress<'_>) -> Vec<(ToolId, Result<Option<String>, ToolError>)> {
        let manifest = self.manifest();
        let client = crate::net::client();
        let mut results = Vec::new();
        let mut done_packages = std::collections::HashSet::new();
        for (tool, managed) in manifest.tools {
            let package = tool.package();
            if !done_packages.insert(package) {
                continue;
            }
            let Some(plan) = release_plan(package) else { continue };
            let result = match resolve(&client, &plan).await {
                Ok(release) if same_version(&release.version, &managed.version) => Ok(None),
                Ok(_) => self.install(tool, progress).await.map(Some),
                Err(e) => Err(e),
            };
            results.push((tool, result));
        }
        results
    }
}

fn same_version(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches(['v', 'V']).to_string();
    norm(a) == norm(b)
}

pub fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

fn make_executable(path: &Path) -> Result<(), ToolError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|e| err(e.to_string()))?;
    }
    let _ = path;
    Ok(())
}

/// The tool's version, short: "2026.10.26", "9.0.2", "2.9.8".
pub async fn version_of(tool: ToolId, path: &Path) -> Option<String> {
    let stop = StopSignal::new();
    let cwd = std::env::temp_dir();
    let args = tool.version_arguments();
    let run = run_tool(path, &args, &[], &cwd, &stop, |_| {});
    let output = tokio::time::timeout(Duration::from_secs(60), run).await.ok()?.ok()?;
    if output.exit_code != 0 {
        return None;
    }
    parse_version(tool, &output.output)
}

pub fn parse_version(tool: ToolId, output: &str) -> Option<String> {
    let first = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let words: Vec<&str> = first.split_whitespace().collect();
    let version = match tool {
        ToolId::Ffmpeg | ToolId::Ffprobe => words.iter().position(|w| *w == "version").and_then(|i| words.get(i + 1)).copied(),
        ToolId::Deno => words.get(1).copied(),
        _ => words.first().copied(),
    }?;
    Some(version.chars().take(32).collect())
}

// MARK: - Where official builds come from

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Archive {
    Raw,
    Zip,
    TarXz,
}

#[derive(Clone, Debug)]
enum ReleaseKind {
    /// A GitHub release: `latest`, or a fixed tag that the publisher moves (FFmpeg-Builds).
    GitHub { repo: &'static str, tag: Option<&'static str> },
    /// A Forgejo/Gitea release (Codeberg).
    Forgejo { api: &'static str },
    /// Martin Riedl's FFmpeg build server: a redirect to the newest build with a .sha256 beside it.
    RedirectWithSidecar { urls: Vec<(String, String, ToolId)> },
    /// A Python package in its own virtual environment (no official binary for this OS).
    PipVenv { package: &'static str },
}

/// (asset name, archive kind, [(member file name, tool)]).
type PlannedAsset = (String, Archive, Vec<(String, ToolId)>);

#[derive(Clone, Debug)]
struct ReleasePlan {
    kind: ReleaseKind,
    /// (asset name, archive kind, [(member file name, tool)]).
    assets: Vec<PlannedAsset>,
    /// A checksum list in the release, used when the asset itself has no digest.
    sums_file: Option<&'static str>,
}

struct Download {
    url: String,
    sha256: String,
    archive: Archive,
    provides: Vec<(String, ToolId)>,
}

struct Release {
    version: String,
    downloads: Vec<Download>,
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// Windows' App Execution Aliases (`…\Microsoft\WindowsApps\python.exe`): stand-ins that open the
/// Microsoft Store when the real program isn't installed.
fn is_store_alias(path: &Path) -> bool {
    path.to_string_lossy().to_ascii_lowercase().replace('\\', "/").contains("/microsoft/windowsapps/")
}

fn release_plan(package: ToolId) -> Option<ReleasePlan> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    match package {
        ToolId::YtDlp => {
            let asset = match (os, arch) {
                ("macos", _) => "yt-dlp_macos",
                ("linux", "x86_64") => "yt-dlp_linux",
                ("linux", "aarch64") => "yt-dlp_linux_aarch64",
                ("windows", "x86_64") => "yt-dlp.exe",
                ("windows", "aarch64") => "yt-dlp_arm64.exe",
                _ => return None,
            };
            Some(ReleasePlan {
                kind: ReleaseKind::GitHub { repo: "yt-dlp/yt-dlp", tag: None },
                assets: vec![(asset.into(), Archive::Raw, vec![(asset.into(), ToolId::YtDlp)])],
                sums_file: Some("SHA2-256SUMS"),
            })
        }
        ToolId::Deno => {
            let triple = match (os, arch) {
                ("macos", "aarch64") => "aarch64-apple-darwin",
                ("macos", "x86_64") => "x86_64-apple-darwin",
                ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
                ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
                ("windows", "x86_64") => "x86_64-pc-windows-msvc",
                ("windows", "aarch64") => "aarch64-pc-windows-msvc",
                _ => return None,
            };
            Some(ReleasePlan {
                kind: ReleaseKind::GitHub { repo: "denoland/deno", tag: None },
                assets: vec![(format!("deno-{triple}.zip"), Archive::Zip, vec![(exe("deno"), ToolId::Deno)])],
                sums_file: None,
            })
        }
        ToolId::GalleryDl => match (os, arch) {
            // Windows on ARM runs the x64 gallery-dl.exe through Windows' own x64 emulation,
            // which beats needing Python (a clean Windows has none).
            ("linux", "x86_64") | ("windows", "x86_64") | ("windows", "aarch64") => {
                let asset = if os == "windows" { "gallery-dl.exe" } else { "gallery-dl.bin" };
                Some(ReleasePlan {
                    kind: ReleaseKind::Forgejo { api: "https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases/latest" },
                    assets: vec![(asset.into(), Archive::Raw, vec![(asset.into(), ToolId::GalleryDl)])],
                    sums_file: Some("SHA256SUMS"),
                })
            }
            _ => Some(ReleasePlan { kind: ReleaseKind::PipVenv { package: "gallery-dl" }, assets: vec![], sums_file: None }),
        },
        ToolId::Ffmpeg | ToolId::Ffprobe => {
            let both = |prefix: &str| {
                vec![
                    (format!("{prefix}{}", exe("ffmpeg")), ToolId::Ffmpeg),
                    (format!("{prefix}{}", exe("ffprobe")), ToolId::Ffprobe),
                ]
            };
            let github = |asset: &str, archive| ReleasePlan {
                kind: ReleaseKind::GitHub { repo: "FFmpeg-Builds/FFmpeg-Builds", tag: Some("latest") },
                assets: vec![(asset.into(), archive, both(""))],
                sums_file: Some("checksums.sha256"),
            };
            match (os, arch) {
                ("windows", "x86_64") => Some(github("ffmpeg-master-latest-win64-gpl.zip", Archive::Zip)),
                ("windows", "aarch64") => Some(github("ffmpeg-master-latest-winarm64-gpl.zip", Archive::Zip)),
                ("linux", "x86_64") => Some(github("ffmpeg-master-latest-linux64-gpl.tar.xz", Archive::TarXz)),
                ("linux", "aarch64") => Some(github("ffmpeg-master-latest-linuxarm64-gpl.tar.xz", Archive::TarXz)),
                ("macos", arch) => {
                    let arch = if arch == "aarch64" { "arm64" } else { "amd64" };
                    let url =
                        |name: &str| format!("https://ffmpeg.martin-riedl.de/redirect/latest/macos/{arch}/release/{name}.zip");
                    Some(ReleasePlan {
                        kind: ReleaseKind::RedirectWithSidecar {
                            urls: vec![
                                (url("ffmpeg"), "ffmpeg".into(), ToolId::Ffmpeg),
                                (url("ffprobe"), "ffprobe".into(), ToolId::Ffprobe),
                            ],
                        },
                        assets: vec![],
                        sums_file: None,
                    })
                }
                _ => None,
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct Wheel {
    filename: String,
    url: String,
    sha256: String,
}

/// The universal (`py3-none-any`) wheel in a PyPI project's JSON (`/pypi/<name>/json`).
fn pip_wheel(info: &serde_json::Value) -> Option<Wheel> {
    info["urls"].as_array()?.iter().find_map(|file| {
        let filename = file["filename"].as_str()?;
        if file["packagetype"].as_str() != Some("bdist_wheel") || !filename.ends_with("-py3-none-any.whl") {
            return None;
        }
        Some(Wheel {
            filename: filename.to_string(),
            url: file["url"].as_str()?.to_string(),
            sha256: file["digests"]["sha256"].as_str().filter(|h| h.len() == 64)?.to_string(),
        })
    })
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, ToolError> {
    let mut request = client.get(url).header("Accept", "application/json");
    if url.starts_with("https://api.github.com/")
        && let Ok(token) = std::env::var("GITHUB_TOKEN")
    {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|e| err(format!("Can't reach {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(err(format!("{url} answered HTTP {}", response.status().as_u16())));
    }
    let bytes = response.bytes().await.map_err(|e| err(format!("Unreadable answer from {url}: {e}")))?;
    serde_json::from_slice(&bytes).map_err(|e| err(format!("Unreadable answer from {url}: {e}")))
}

async fn get_text(client: &reqwest::Client, url: &str) -> Result<String, ToolError> {
    let response = client.get(url).send().await.map_err(|e| err(format!("Can't reach {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(err(format!("{url} answered HTTP {}", response.status().as_u16())));
    }
    response.text().await.map_err(|e| err(e.to_string()))
}

/// `hash  name` lines (sha256sum format) → the hash for `name`.
pub fn hash_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        let file_name = file.rsplit('/').next().unwrap_or(file);
        (file_name == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

async fn resolve(client: &reqwest::Client, plan: &ReleasePlan) -> Result<Release, ToolError> {
    match &plan.kind {
        ReleaseKind::GitHub { repo, tag } => {
            let url = match tag {
                Some(tag) => format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"),
                None => format!("https://api.github.com/repos/{repo}/releases/latest"),
            };
            let release = get_json(client, &url).await?;
            from_release_json(client, plan, &release, tag.is_some()).await
        }
        ReleaseKind::Forgejo { api } => {
            let release = get_json(client, api).await?;
            from_release_json(client, plan, &release, false).await
        }
        ReleaseKind::RedirectWithSidecar { urls } => {
            let no_redirect = crate::net::client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| err(e.to_string()))?;
            let mut downloads = Vec::new();
            let mut version = String::new();
            for (url, member, tool) in urls {
                // GET, not HEAD: this server answers HEAD with 404 for some clients.
                let response = no_redirect.get(url).send().await.map_err(|e| err(format!("Can't reach {url}: {e}")))?;
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|l| l.to_str().ok())
                    .ok_or_else(|| err(format!("{url} didn't point at a build")))?;
                let final_url = reqwest::Url::parse(url).and_then(|u| u.join(location)).map_err(|e| err(e.to_string()))?;
                // …/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip → 9.0.2
                let build = final_url.path_segments().and_then(|s| s.rev().nth(1)).unwrap_or_default().to_string();
                version = build.split_once('_').map(|(_, v)| v.to_string()).unwrap_or(build);
                let sums = get_text(client, &format!("{final_url}.sha256")).await?;
                let file = final_url.path_segments().and_then(|mut s| s.next_back()).unwrap_or_default().to_string();
                let sha256 = hash_from_sums(&sums, &file)
                    .or_else(|| sums.split_whitespace().next().filter(|h| h.len() == 64).map(str::to_string))
                    .ok_or_else(|| err(format!("No checksum published for {final_url}")))?;
                downloads.push(Download {
                    url: final_url.to_string(),
                    sha256,
                    archive: Archive::Zip,
                    provides: vec![(exe(member), *tool)],
                });
            }
            Ok(Release { version, downloads })
        }
        ReleaseKind::PipVenv { package } => {
            let info = get_json(client, &format!("https://pypi.org/pypi/{package}/json")).await?;
            let version = info["info"]["version"].as_str().ok_or_else(|| err("PyPI gave no version"))?.to_string();
            Ok(Release { version, downloads: vec![] })
        }
    }
}

async fn from_release_json(
    client: &reqwest::Client,
    plan: &ReleasePlan,
    release: &serde_json::Value,
    rolling: bool,
) -> Result<Release, ToolError> {
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let find = |name: &str| assets.iter().find(|a| a["name"].as_str() == Some(name));
    let mut sums: Option<String> = None;
    let mut downloads = Vec::new();
    let mut version = release["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();

    for (name, archive, provides) in &plan.assets {
        let asset = find(name).ok_or_else(|| err(format!("The latest release has no {name}")))?;
        let url = asset["browser_download_url"].as_str().ok_or_else(|| err("asset without URL"))?.to_string();
        let mut sha256 = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string);
        if sha256.is_none()
            && let Some(sums_name) = plan.sums_file
        {
            if sums.is_none() {
                let sums_asset = find(sums_name).ok_or_else(|| err(format!("The release has no {sums_name}")))?;
                let sums_url = sums_asset["browser_download_url"].as_str().unwrap_or_default();
                sums = Some(get_text(client, sums_url).await?);
            }
            sha256 = sums.as_deref().and_then(|s| hash_from_sums(s, name));
        }
        let sha256 =
            sha256.ok_or_else(|| err(format!("No checksum published for {name}; Dawnload won't install it unverified.")))?;
        if rolling {
            // A moving tag (FFmpeg-Builds `latest`): the build's identity is its date and digest.
            let date = asset["updated_at"].as_str().unwrap_or_default().get(..10).unwrap_or_default();
            version = format!("{date}-{}", &sha256[..8]);
        }
        downloads.push(Download { url, sha256, archive: *archive, provides: provides.clone() });
    }
    Ok(Release { version, downloads })
}

async fn fetch_to_file(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> Result<String, ToolError> {
    let response = client.get(url).send().await.map_err(|e| err(format!("Can't reach {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(err(format!("{url} answered HTTP {}", response.status().as_u16())));
    }
    let total = response.content_length();
    let mut file = std::fs::File::create(path).map_err(|e| err(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| err(format!("The download of {url} stopped: {e}")))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| err(e.to_string()))?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.flush().map_err(|e| err(e.to_string()))?;
    Ok(hex::encode(hasher.finalize()))
}

fn extract(
    archive_path: &Path,
    archive: Archive,
    members: &[(String, ToolId)],
    out_dir: &Path,
) -> Result<Vec<(ToolId, PathBuf)>, ToolError> {
    let mut out = Vec::new();
    match archive {
        Archive::Raw => {
            let (_, tool) = &members[0];
            let target = out_dir.join(format!("new-{}", tool.executable()));
            std::fs::rename(archive_path, &target).map_err(|e| err(e.to_string()))?;
            out.push((*tool, target));
        }
        Archive::Zip => {
            let file = std::fs::File::open(archive_path).map_err(|e| err(e.to_string()))?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| err(format!("Unreadable zip: {e}")))?;
            for (member, tool) in members {
                let index = (0..zip.len())
                    .find(|i| {
                        zip.by_index(*i).ok().is_some_and(|f| {
                            f.is_file() && f.name().ok().is_some_and(|n| n.rsplit('/').next() == Some(member.as_str()))
                        })
                    })
                    .ok_or_else(|| err(format!("{member} isn't in the archive")))?;
                let mut entry = zip.by_index(index).map_err(|e| err(e.to_string()))?;
                let target = out_dir.join(format!("new-{}", tool.executable()));
                let mut writer = std::fs::File::create(&target).map_err(|e| err(e.to_string()))?;
                std::io::copy(&mut entry, &mut writer).map_err(|e| err(e.to_string()))?;
                out.push((*tool, target));
            }
        }
        Archive::TarXz => {
            let tar_path = out_dir.join("archive.tar");
            {
                let mut input = std::io::BufReader::new(std::fs::File::open(archive_path).map_err(|e| err(e.to_string()))?);
                let mut output = std::io::BufWriter::new(std::fs::File::create(&tar_path).map_err(|e| err(e.to_string()))?);
                lzma_rs::xz_decompress(&mut input, &mut output).map_err(|e| err(format!("Unreadable .tar.xz: {e:?}")))?;
                output.flush().map_err(|e| err(e.to_string()))?;
            }
            let mut tar = tar::Archive::new(std::fs::File::open(&tar_path).map_err(|e| err(e.to_string()))?);
            for entry in tar.entries().map_err(|e| err(e.to_string()))? {
                let mut entry = entry.map_err(|e| err(e.to_string()))?;
                let path = entry.path().map_err(|e| err(e.to_string()))?.into_owned();
                let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let in_bin = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "bin");
                if let Some((_, tool)) = members.iter().find(|(m, _)| *m == file_name).filter(|_| in_bin) {
                    let target = out_dir.join(format!("new-{}", tool.executable()));
                    let mut writer = std::fs::File::create(&target).map_err(|e| err(e.to_string()))?;
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf).map_err(|e| err(e.to_string()))?;
                    writer.write_all(&buf).map_err(|e| err(e.to_string()))?;
                    out.push((*tool, target));
                }
            }
            if out.len() != members.len() {
                return Err(err("The archive didn't contain the expected tools"));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version(ToolId::YtDlp, "2026.10.26\n").as_deref(), Some("2026.10.26"));
        assert_eq!(parse_version(ToolId::Ffmpeg, "ffmpeg version 9.0.2 Copyright (c) 2000-2026").as_deref(), Some("9.0.2"));
        assert_eq!(
            parse_version(ToolId::Deno, "deno 2.9.8 (stable, release, aarch64-apple-darwin)\nv8 14").as_deref(),
            Some("2.9.8")
        );
        assert_eq!(parse_version(ToolId::GalleryDl, "1.32.19").as_deref(), Some("1.32.19"));
        assert!(same_version("v2.9.8", "2.9.8"));
    }

    #[test]
    fn skips_windows_store_aliases() {
        assert!(is_store_alias(Path::new(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\python.exe")));
        assert!(is_store_alias(Path::new("/c/Users/me/AppData/Local/Microsoft/windowsapps/python3.exe")));
        assert!(!is_store_alias(Path::new(r"C:\Users\me\AppData\Local\Programs\Python\Python313\python.exe")));
        assert!(!is_store_alias(Path::new("/usr/bin/python3")));
    }

    #[test]
    fn finds_pips_universal_wheel() {
        let info = serde_json::json!({ "urls": [
            { "filename": "pip-26.3.tar.gz", "packagetype": "sdist", "url": "https://files/pip-26.3.tar.gz",
              "digests": { "sha256": "a".repeat(64) } },
            { "filename": "pip-26.3-py3-none-any.whl", "packagetype": "bdist_wheel", "url": "https://files/pip-26.3-py3-none-any.whl",
              "digests": { "sha256": "b".repeat(64) } },
        ]});
        assert_eq!(
            pip_wheel(&info),
            Some(Wheel {
                filename: "pip-26.3-py3-none-any.whl".into(),
                url: "https://files/pip-26.3-py3-none-any.whl".into(),
                sha256: "b".repeat(64),
            })
        );
        let unhashed = serde_json::json!({ "urls": [
            { "filename": "pip-26.3-py3-none-any.whl", "packagetype": "bdist_wheel", "url": "https://x", "digests": {} },
        ]});
        assert_eq!(pip_wheel(&unhashed), None);
        assert_eq!(pip_wheel(&serde_json::json!({})), None);
    }

    #[test]
    fn reads_checksum_lists() {
        let sums = "0d7e  SHORT\n767ba9dce7052aa196f8b12652f433d76de128ba168966fa84b6394d5e6c6795  yt-dlp_linux\nE51E69E89489F199530F13D32A4342DA3DDB2CF34842F364603A066DB05AFBF3 *yt-dlp\n";
        assert_eq!(
            hash_from_sums(sums, "yt-dlp_linux").as_deref(),
            Some("767ba9dce7052aa196f8b12652f433d76de128ba168966fa84b6394d5e6c6795")
        );
        assert_eq!(
            hash_from_sums(sums, "yt-dlp").as_deref(),
            Some("e51e69e89489f199530f13d32a4342da3ddb2cf34842f364603a066db05afbf3")
        );
        assert_eq!(hash_from_sums(sums, "SHORT"), None);
    }

    #[test]
    fn discovery_prefers_custom_then_managed_then_system() {
        let root = tempfile::tempdir().unwrap();
        let system = root.path().join("system");
        let managed = root.path().join("tools");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::create_dir_all(managed.join("bin")).unwrap();
        let write_exe = |path: &Path| {
            std::fs::write(path, "#!/bin/sh\necho 1.0\n").unwrap();
            make_executable(path).unwrap();
        };
        write_exe(&system.join(ToolId::YtDlp.executable()));
        write_exe(&system.join(ToolId::Ffmpeg.executable()));
        write_exe(&managed.join("bin").join(ToolId::YtDlp.executable()));

        let mut manager = ToolManager::new(managed.clone()).with_system_search(vec![system.clone()]);
        let snapshot = manager.discover();
        assert_eq!(snapshot.origins[&ToolId::YtDlp], ToolOrigin::Managed);
        assert_eq!(snapshot.origins[&ToolId::Ffmpeg], ToolOrigin::System);
        assert!(!snapshot.has(ToolId::GalleryDl));
        assert!(snapshot.js_runtime.is_none());

        let custom = root.path().join("my-yt-dlp");
        write_exe(&custom);
        manager.set_override(ToolId::YtDlp, Some(custom.clone()));
        let snapshot = manager.discover();
        assert_eq!(snapshot.path(ToolId::YtDlp), Some(custom.as_path()));
        assert_eq!(snapshot.origins[&ToolId::YtDlp], ToolOrigin::Custom);

        write_exe(&system.join(ToolId::Deno.executable()));
        let runtime = manager.discover().js_runtime.unwrap();
        assert_eq!(runtime.kind, JsRuntimeKind::Deno);
        let env = manager.discover().env;
        let path_var = &env.iter().find(|(k, _)| k == "PATH").unwrap().1;
        assert!(path_var.starts_with(&managed.join("bin").display().to_string()));
    }

    #[test]
    fn every_desktop_platform_has_a_plan_for_the_core_tools() {
        for tool in [ToolId::YtDlp, ToolId::Ffmpeg, ToolId::Deno, ToolId::GalleryDl] {
            assert!(release_plan(tool).is_some(), "{tool:?} on {}", std::env::consts::OS);
        }
    }
}

# 0005 · Dawnload installs, verifies and updates its tools

Status: Accepted (2026-11-09)

## Context

yt-dlp, gallery-dl and ffmpeg stay external processes (licence boundaries, and they move faster
than we ever could). But a stale yt-dlp breaks within weeks. This was proven while building this
phase: Homebrew's yt-dlp 2026.07.04 got HTTP 403 from YouTube, while 2026.10.26 downloaded the same
video. Since 2025.11 yt-dlp also needs a JavaScript runtime (deno by default) for YouTube.
Windows and Linux users won't have Homebrew, and a GUI app on macOS doesn't inherit the shell's
PATH.

## Decision

- **Resolution order per tool:** a path the person set (Custom) > Dawnload's own copy (Managed,
  `<data>/tools/bin`) > the system (PATH plus where package managers put things: Homebrew,
  MacPorts, winget links, scoop shims, Chocolatey, `~/.local/bin`, `~/.deno/bin`, snap).
  Managed copies win over system ones because Dawnload keeps them current.
- **Official builds only, verified before they run:**

  | Tool | Windows / Linux | macOS | Verified by |
  |---|---|---|---|
  | yt-dlp | GitHub `yt-dlp/yt-dlp` standalone binaries | `yt-dlp_macos` | GitHub asset digest, fallback `SHA2-256SUMS` |
  | deno | GitHub `denoland/deno` zips | same | GitHub asset digest |
  | ffmpeg + ffprobe | `FFmpeg-Builds/FFmpeg-Builds` `latest` (GPL builds; the repo moved from BtbN) | martin-riedl.de build server | asset digest / `checksums.sha256`; `.sha256` beside each build |
  | gallery-dl | Codeberg `mikf/gallery-dl` (moved from GitHub; its GitHub releases are now empty) `gallery-dl.exe` / `gallery-dl.bin` | a Python venv with `pip install gallery-dl==<latest>` (no official macOS binary) | `SHA256SUMS`; PyPI hashes over TLS |

  A download whose checksum doesn't match, or that has no published checksum, is not installed.
  Each new binary must run `--version` before it replaces the old one (atomic rename).
  `tools/manifest.json` records version, source URL and SHA-256.
- **Updates:** `dawnload tools update` (and, in the app, once a day in the background when
  `auto_update_tools` is on) updates managed copies only. System copies are reported with the
  newer version available and left to their package manager.
- **JavaScript runtime:** deno first (yt-dlp's default), else node, bun or QuickJS from the
  system. The yt-dlp engine always passes `--js-runtimes <kind>:<path>`, so a managed deno works
  without being on PATH. The detection line warns *YouTube needs a JavaScript runtime* when none
  is found, and failures that mention it map to `FailureKind::JsRuntimeNeeded` with a hint.
- **Cookies:** `Cookies::File` (Netscape cookies.txt → `--cookies`, and parsed into a `Cookie`
  header for the HTTP engine) and `Cookies::Browser` (`--cookies-from-browser`, yt-dlp and
  gallery-dl).

## Alternatives

- **Bundle the tools in the installer**: fresh on day one, stale on day thirty, and it puts
  GPL ffmpeg inside our package.
- **Let yt-dlp self-update (`-U`)**: works only for its own binary and not for pip or package
  manager installs; we pass `--no-update` and update it ourselves, verified.
- **Require a package manager**: fine for developers, a wall for most Windows users.

## Consequences

- Tested on macOS (arm64): installs of yt-dlp 2026.10.26, deno 2.9.8, ffmpeg/ffprobe 9.0.2 and
  gallery-dl 1.32.19 (venv), all verified; the YouTube download worked only after the managed
  yt-dlp was installed. **Not run on Windows or Linux**: those asset names and the `.tar.xz`
  extraction path are written from the release listings and only unit-checked.
- ffmpeg's server answers HEAD with 404 for some clients, so its redirect is resolved with GET.
- GitHub's unauthenticated API allows 60 requests an hour per IP; the daily check uses a handful.
  `GITHUB_TOKEN` is honoured if set.
- Release sources move (BtbN → FFmpeg-Builds, GitHub → Codeberg happened within months); the
  table lives in one function (`tools::release_plan`) and needs watching.

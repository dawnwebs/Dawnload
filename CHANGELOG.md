# Changelog

All notable changes to Dawnload will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project will follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Native macOS 13+ SwiftUI app with `yt-dlp`, `gallery-dl`, and `ffmpeg` backends
- Backend switch between `yt-dlp` and `gallery-dl` inside the app
- Quality selector: Best, Compatible, Audio (M4A), plus ffmpeg polish
- Browser impersonation profiles: Chrome 146, 145, 142, Safari 18.4
- Netscape `cookies.txt` support for authenticated sessions
- Playlist toggle (`--yes-playlist`)
- Collapsible sequential download queue
- Smart collision-safe filename resolution after each download
- Tool availability detection across `/opt/homebrew/bin`, `/usr/local/bin`, and `PATH`
- Live progress, status, and downloader log view
- `Scripts/install.sh` one-command installer with Homebrew + toolchain checks
- `Scripts/package-app.sh` native `.app` packaging with generated icon
- Core test suite for URL sanitizing, metadata parsing, and smart filenames
- The redesign on the brand kit (docs/DESIGN.md): Pine and Paper themes, the Cut D
  status glyphs with progress filling the D, the paste field with a detection line,
  sticky option chips, one grouped list with filters, an inspector with files,
  options and per-item log, first run, missing-tools setup, and native Settings
- Pause and resume from partial files, retry, reorder, and a list saved between launches
- Progress across video and audio parts and playlist entries, with speed and time left
- Errors that say what happened and what to do
- Engine Auto: gallery-dl for gallery sites, yt-dlp for the rest
- A `DownloadEngine` seam with a torrent placeholder: magnet links and `.torrent` files
  are recognised, and items model files, per-file selection, swarm stats and seeding
- Links passed on the command line are downloaded
- The app icon from docs/brand/app-icon/, and Unbounded and Martian Mono bundled (SIL OFL)

### Changed

- The Playlist toggle passes `--no-playlist` when off, so watch links that carry a list
  download one video unless Playlist is on

### Removed

- The "wallpaper glass" appearance

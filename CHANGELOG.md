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
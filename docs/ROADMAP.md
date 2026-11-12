# Roadmap

Dawnload becomes one cross-platform app: a Rust core shared everywhere and a Slint UI, for
Windows, Linux and macOS. **Paste anything** (a video link, a gallery, a magnet, a `.torrent`, a
plain file URL) into one queue with one design; tools that never go stale; light enough to leave
running in the tray. Decisions behind this plan are in [adr/](adr/README.md).

## 0 · Ship the Swift redesign to Mac users, then freeze it

The SwiftUI redesign (DESIGN.md, the brand kit, the engine seam) goes out to current Mac users.
After that the Swift app only gets fixes, until the Rust app matches it on macOS (end of phase 3).
Nothing in the Rust work touches `Sources/`, `Tests/` or `Package.swift`.

## 1 · Core and CLI · done (2026-11-09)

`crates/dawnload-core` and the `dawnload` command.
- Sources and Auto routing, the detection line ([0007](adr/0007-paste-anything-routing.md)).
- Engines: yt-dlp (progress markers, failure mapping, smart filenames, ffmpeg faststart,
  playlists, JS runtime, cookies), gallery-dl, a built-in multi-connection HTTP engine with resume
  and checksums ([0006](adr/0006-built-in-http-engine.md)), a torrent seam, a demo engine.
- Tool manager: find, install, verify and update yt-dlp, gallery-dl, ffmpeg/ffprobe and deno
  ([0005](adr/0005-tool-manager.md)).
- The queue: lanes, pause/resume from partial files, automatic retries, continue on error,
  reorder; seeding inside the job ([0003](adr/0003-seeding-inside-the-job.md)); SQLite state that
  survives quit and crash ([0004](adr/0004-sqlite-state-and-single-instance.md)).
- The UI-facing API: snapshot plus batched diffs, `Mirror` ([0002](adr/0002-core-owns-state-snapshot-and-diffs.md)).
- CI on Windows, Linux and macOS with a dependency licence check.

CI builds and tests pass on Windows and Linux (x64 and ARM64) and macOS. On a clean Windows 11
ARM VM (2026-11-12) the CLI installed its tools, downloaded a video and a resumed HTTP file; that
run found and fixed four Windows issues ([0008](adr/0008-slint-prototype-verdict.md)). The same on
a stock Ubuntu 24.04 ARM VM (2026-11-12): tools, the video and the resumed HTTP file all worked
once gallery-dl's install stopped needing `python3-venv`; an orphaned part file after a lost
state is now restarted in place.

## 2 · UI prototype · done (2026-11-09)

`crates/dawnload-ui`: the main window in Slint on the real update path, 500 demo items,
measured. **Verdict: Slint confirmed** on macOS: 96 MB idle, 13.6% of one core while scrolling
500 updating rows at full rate ([0008](adr/0008-slint-prototype-verdict.md)).
The owner confirmed Slint and its royalty-free licence on 2026-11-10 ([0009](adr/0009-licensing.md)).
Windows 11 on ARM (VM, 2026-11-12): 80 MB idle, 84 MB and 27% of one core while scrolling 500
updating rows at 53–63 frames/s. Ubuntu 24.04 on ARM (VM, Wayland, 2026-11-12): 115 MB RSS of
which 55 MB private (65 MB PSS), ~0% idle, 22% of one core scrolling at 63 frames/s; X11 through
Xwayland the same. Still worth a run: real Xorg, x64 hardware, Fedora or KDE.

## 3 · The full app, Windows first

Gate: phase 2 numbers on Windows and Linux.
- Everything in DESIGN.md: chip menus, Settings (General, Access, Tools, Torrents), first run,
  empty and missing-tool states, drag to reorder, context menus, drop links and `.torrent` files,
  the loader animation, Reduce Motion, keyboard map, accessibility with Narrator, Orca, VoiceOver.
- Tools screen on the tool manager: install, update, choose a custom path, the JS runtime.
- Tray icon (Slint's system tray) with pause/resume and a quiet badge; close to tray.
- Notifications on Kept and Failed; open file / show in folder per OS.
- **Required:** an About window that shows Slint's `AboutSlint` element. Without it the build
  doesn't meet the Slint Royalty-free License's attribution term ([0009](adr/0009-licensing.md)).
- Windows first (largest audience), then Linux (X11 and Wayland), then macOS parity, at which
  point the Swift app is retired.

## 4 · Torrents on librqbit

librqbit embedded in-process behind the existing `DownloadEngine` seam (libtorrent stays the
backup behind the same trait). No home-grown BitTorrent engine and no built-in torrent search.
- Magnets, `.torrent` files and links; file selection; Checking; swarm stats; seeding to a ratio or
  time limit; Stop Seeding ([0003](adr/0003-seeding-inside-the-job.md) already handles the
  lifecycle).
- Contribute upstream what we need: rarest-first piece picking and per-torrent rate limits.
- The Seeding tab and glyph (designed already); the first-use note about sharing.

## 5 · Signing, installers, auto-update, public beta

- Windows: signed MSIX/MSI, **winget**.
- Linux: **Flathub** and **AppImage**.
- macOS: **notarized DMG** and a **Homebrew cask**.
- Auto-update for Dawnload itself (signed update manifests), separate from the tool updates.
- Crash reporting that is opt-in and local-first; public beta.

## 6 · Beyond the window

- **Local control API** on the core's snapshot/diff stream and `Action`s (JSON over a local
  socket, token-protected). It also lets the CLI talk to a running app.
- **Browser extension** ("Send to Dawnload") through native messaging to that API.
- **RSS rules** (watch a feed, add matching items).
- **Phone remote** through the same API.

## Decisions to revisit

- Web-lane concurrency defaults to 1 (DESIGN.md). If people ask for parallel video downloads,
  raise the default to 2; it's a setting already.
- Auto routing guesses plain files by extension. A short network probe after typing stops would
  catch extension-less file links.
- Item log lines keep the time in UTC until the core has a local-time formatter.

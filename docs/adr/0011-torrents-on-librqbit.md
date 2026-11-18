# 0011 · Torrents on librqbit: one session, courteous seeding, safe by default

Status: Accepted (2026-11-17). M1 merged. M2 implementation is in ADR 0014. M3–M5 are planned.
This supersedes ADR 0003's quit rule for seeders. The build plan is
[plans/phase-4-torrents.md](../plans/phase-4-torrents.md).

## Context

Phase 4 adds torrents behind the engine seam the core already has: `TorrentEngine` accepts
magnets, `.torrent` files and `.torrent` links and reports "not built yet"; ADR 0003's seeding
lifecycle (a job's run is its whole life, seeding frees its download slot, Stop Seeding reaches
the job) is built and tested; the UI already draws Seeding, swarm stats, a file list with
checkboxes and a Torrents settings page, on demo data. ADR 0001 chose librqbit (Apache-2.0,
ADR 0009) over libtorrent and a home-grown engine.

librqbit 9.1.0 (2026-10-29), read from source for this record:
- **Has:** magnets with DHT and PEX, `.torrent` files, trackers, uTP, fast resume, pause/resume,
  file selection (`only_files`) changeable while running, per-torrent rate limits at add time,
  global limits changeable while running, streaming with seek (`api_stream`), torrent creation,
  binding to a network device (`bind_device_name`), UPnP port forwarding, IP block/allow lists,
  `initial_peers` (peers given directly, no tracker or DHT), a rustls build (`rust-tls`).
- **Lacks:** rarest-first piece picking (it takes pieces in file order and steals pieces stuck on
  slow peers), seeding to a ratio or time limit, changing one torrent's rate limit after it's
  added, BitTorrent v2, and protocol encryption (MSE/PE).

## Decision

**Engine shape**
- **One `librqbit::Session` per core**, owned by a `TorrentService` that starts lazily with the
  first torrent item and shuts down with the core. Each Dawnload item is one managed torrent;
  its job holds the handle and maps librqbit's stats to `Transfer` and `Swarm` events.
- **Dawnload's SQLite stays the source of truth for items.** librqbit keeps piece state with its
  own fast-resume persistence under `<data>/torrents/`. SQLite stores each item's info hash, its
  `.torrent` metadata bytes and its file selection, so a lost or corrupt librqbit state rebuilds
  from the database plus a piece check, never by losing the item.
- **Built lean:** `default-features = false, features = ["rust-tls"]`, no HTTP API, no web UI,
  no Prometheus. It joins the core's rustls/ring stack and the cargo-deny licence gate.

**Seeding: courteous by default**
- **Seed to a ratio of 1.0 or for 7 days, whichever comes first** (both editable globally and per
  item; "until I stop it" is a choice, not the default). Dawnload enforces this itself from
  librqbit's stats, because librqbit has no limit of its own.
- **Courtesy mode, on by default:** seeding pauses while the connection is metered (Windows
  metered networks, NetworkManager's metered flag, macOS Low Data Mode) or the laptop is on
  battery, and resumes when that ends. Downloads are never paused by it.
- **Upload cap "Auto"** by default: 70% of the fastest upload Dawnload has measured, so seeding
  never saturates a home uplink and makes browsing laggy. Fixed and unlimited are choices.
- **Seeding survives a restart.** An item seeding at quit resumes seeding at launch (with the
  limits above), instead of coming back Paused. Dawnload already lives in the tray; giving back
  shouldn't depend on keeping it open.

**Safe by default**
- **No built-in search, ever** (ADR 0001). The first torrent shows DESIGN.md's note once:
  *Only download and seed torrents you have the right to share. Seeding uploads pieces of the
  files to other people.*
- **"Only through this connection" (VPN kill switch).** Settings › Torrents lets people pick a
  network connection. Torrents bind to it (`bind_device_name`, where the OS supports it), and
  Dawnload pauses every torrent the moment that connection goes away, before traffic could leave
  another way. Off by default; the setting says plainly what it does and doesn't do.
- **Executables are flagged, never opened for you.** Files inside a torrent with executable or
  installer types (`.exe .msi .bat .cmd .scr .ps1 .lnk .app .pkg .dmg .sh .AppImage .deb .rpm`)
  get a warning badge in the file list and in the inspector; Open on them asks first. On Windows
  every finished file gets the Mark of the Web (`Zone.Identifier`), so SmartScreen and Office
  check them, for torrents and for the HTTP engine alike.
- **Smart file selection:** magnets show "Getting the file list" while metadata arrives, then a
  chooser with everything ticked except the extras (`.nfo`, `.txt` under 64 KB, `.url`, sample
  videos under 100 MB, padding files). The existing "show the file list first" option makes it
  wait for the person; otherwise the download starts with the smart default.
- **Network defaults:** a random listening port per install (49152–65535), kept across
  launches; UPnP on; DHT, PEX and local peer discovery on; the IP block list off (a choice).

**Bold extras, kept in scope**
- **Play while it downloads.** Video and audio files in a torrent get **Play** as soon as their
  first pieces arrive: Dawnload serves the file from librqbit's seekable stream on
  `127.0.0.1` with a random per-file token and hands that URL to the system player (or the
  `mpv`/VLC it finds). Streaming raises that file's piece priority; the rest keep going.
- **Open magnet links and `.torrent` files with Dawnload:** an opt-in in Settings that registers
  the handlers per OS where an unpackaged app can (Windows per-user registry, a Linux desktop
  entry's `MimeType`); macOS waits for the app bundle.

**Upstream first, fork last**
- Rarest-first piece picking and changing a torrent's rate limit while running are written as
  upstream pull requests to librqbit, in parallel with phase 4. Phase 4 ships without
  rarest-first if they aren't merged in time; a Dawnload fork is the last resort, only if
  upstream declines and swarm tests show a real cost.

## Alternatives

- **Rarest-first before shipping, in a fork:** best swarm behaviour on day one, but a fork of a
  fast-moving engine is a maintenance tax we'd pay forever. In-order picking plus stealing is
  what librqbit's own users run today.
- **Unlimited seeding by default** (qBittorrent's default): better for swarms, worse for people on
  capped or metered plans, and the main reason people uninstall torrent clients from laptops.
- **Ratio-only limit:** a torrent nobody wants never reaches 1.0 and would seed forever.
- **libtorrent via `cxx`:** encryption, BitTorrent v2 and rarest-first today, but a C++ build
  with Boost and OpenSSL on five targets, against ADR 0001's one-language core. Still the backup
  behind the same engine trait.
- **A separate torrent daemon (transmission-daemon, rqbit's server):** a second process, port and
  auth secret per install, and awkward on Windows.

## Consequences

- No protocol encryption; throttling may warrant an upstream MSE/PE contribution or revisiting
  libtorrent. No BitTorrent v2-only support; hybrid torrents use their v1 half.
- **M1 built and validated (2026-11-17), PR #4 merged on 2026-11-18:** one lazy Session,
  SQLite metadata/selection, live selection, Checking, swarm stats, fast resume/cache recovery,
  ratio completion and Stop Seeding. The desktop remains gated until M3. See
  [0012](0012-torrent-state-is-a-cache.md) for ownership/cache decisions.
- **Tests:** 86 local tests, fmt, clippy and cargo-deny pass on this ARM64 Mac; no VM used.
  All six checks passed on `d82bad7` in [run 44095865946](https://github.com/dawnwebs/Dawnload/actions/runs/44095865946):
  Ubuntu 24.04 x64/ARM, Windows x64/ARM, macOS and licences. Offline swarms cover the M1 lifecycle.
  Both Windows runners confirmed upstream drive-prefix escape semantics and the guarded
  rejection test (non-writing probe, [0013](0013-portable-torrent-file-names.md)). Phase-event
  deduplication has a real-engine regression. All six checks passed on the final PR head.
- **Public exit:** the complete Ubuntu 24.04.3 server ISO matches
  [Canonical's SHA-256](https://releases.ubuntu.com/24.04.3/SHA256SUMS), resumes after quit, and
  seeds verified pieces to a separate local peer. Public peers requested no uploads during
  observation; qBittorrent/Transmission interop is not established. The review-fix CLI repeated
  resume/upload successfully. Exact hashes, bytes, methods and limits are in
  [local measurements](../measurements/phase-4-m1-macos.json).
- **Memory:** pre-review integration core mean RSS: 8.94 MiB with no torrents; 12.65 MiB with
  five throttled loopback downloads, excluding the separate seeder (8 s warm-up, 20 samples).
  Empty desktop physical footprint: 102 MiB. These do not establish the five-torrent desktop
  or VM ~100 MB/+40 MB budget; they are not new measurements of the review fixes.
- **Build impact:** local review-fix CLI 19,796,656 bytes; desktop 43,381,728 bytes, about
  +9.6 MB each over pre-M1. The required `rust-tls` feature also selects AWS-LC via reqwest 0.13,
  alongside ring. The full increase cannot all be attributed to crypto without a controlled
  comparison. [Five-runner measurements](../measurements/phase-4-m1-ci-d82bad7.json) record
  sizes and release-step times; cache/runner differences prevent controlled timing comparisons.
- **M2:** lifetime accounting, deadline, courtesy/cap controls and seeding restart are
  implemented in [0014](0014-persistent-seeding-policy.md). Downloads still return Paused.
  Local format, Clippy, licence and all 92 workspace tests pass. The Ubuntu ISO resume/upload
  check passed on this Mac with the OS adapter active. Core mean RSS was 9.49 MiB with no
  torrents and 15.20 MiB with five throttled loopback downloads, excluding the seeder. The
  M2 integration binaries measured 19,951,232 bytes for the CLI and 43,887,696 for the desktop.
  [M2 measurements](../measurements/phase-4-m2-macos.json) record the method and build stage.
  Five-runner CI and native metering and battery transitions still need acceptance.
- **Open:** M3 UI/chooser scheduling, M4 safety and M5 playback;
  both VM acceptance runs, interop and production memory/CPU. Upstream rarest-first, live rate
  limits and provider-neutral rustls remain queued. No fork. See the
  [review disposition](../reviews/phase-4-m1-claude-review.md) and [plan](../plans/phase-4-torrents.md).

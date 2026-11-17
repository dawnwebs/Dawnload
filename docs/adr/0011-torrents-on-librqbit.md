# 0011 · Torrents on librqbit: one session, courteous seeding, safe by default

Status: Accepted (2026-11-17); M1 implementation in progress, M2–M5 planned. Supersedes ADR 0003's last bullet (an
item seeding at quit now resumes seeding at launch, below). The build plan is
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

- No protocol encryption: some ISPs throttle unencrypted BitTorrent. Settings says so; if it
  proves common, MSE/PE is the next upstream contribution (or the reason to revisit libtorrent).
- No BitTorrent v2-only torrents. Hybrid torrents work through their v1 half.
- Memory: the ~100 MB idle target holds with no torrents; with five active torrents the budget
  is +40 MB, measured in the VMs before phase 4 is called done.
- The CI swarm test (a seeder session and Dawnload's session joined through `initial_peers`,
  no network) makes torrent behaviour testable on all five runners.
- ADR 0003's quit rule changes for seeding only: downloads still come back Paused.

### M1 implementation and evidence (2026-11-17)

Implemented in the core and CLI; desktop torrent intake stays disabled until M3. See
[0012](0012-torrent-state-is-a-cache.md) for the cache/SQLite boundary, duplicate-hash ownership
and destination rules. Metadata URLs use the same pooled HTTP client as direct downloads.
Seeding ends at the existing per-item ratio limit or Stop Seeding; the 7-day limit, cumulative
upload accounting, courtesy mode, automatic cap and seeding restart are M2 work. No search added.

On this Mac (MacBookPro18,1, ARM64, 16 GiB; rustc 1.98.0): all 83 workspace tests and clippy
passed, including five offline generated-data swarm cases covering every M1 lifecycle scenario,
metadata URLs, removing/re-adding a hash and corrupt cache recovery. `cargo deny check licenses`
passed. No VM was started. The public Ubuntu ISO exit test passed below. Five-runner CI
is still pending acceptance; its results must be recorded before calling M1 complete.

Release core memory probe (`cargo build --release -p dawnload-core --example torrent_memory`):
a separate seeder process, five 4 MiB generated files, loopback, 16 KiB/s per download; 8 s
warm-up then 20 RSS samples one second apart. Zero torrents: mean RSS 8.94 MiB, peak 8.95 MiB,
physical footprint 3.83 MiB. Five active: mean RSS 12.65 MiB, peak 13.25 MiB, physical footprint
7.44 MiB. Reproduce with `torrent_memory seed DIR`, then `torrent_memory probe DIR 0` or `5`;
measure the probe PID after READY and send Ctrl-C for clean shutdown. The seeder is excluded.
These small-fixture core results do not establish the full UI's ~100 MB/+40 MB VM budget.

`rust-tls` pulls in AWS-LC through reqwest 0.13 in addition to ring (contrary to the initial
expectation above); the requested feature set is unchanged. M1 CI publishes release build
seconds and uncompressed binary sizes in its job summaries and artifacts. The pre-M1 release
baseline on this Mac is 10,188,672 bytes for the CLI and 33,783,488 bytes for the desktop;
first build from an empty target directory took 194 seconds. Final-source release sizes:
19,794,656 bytes for the CLI (+9,605,984) and 43,364,016 bytes for the desktop (+9,580,528).
The incremental final release step took 154.10 s; the initial integration step took 325.72 s.
These times use different caches and are not a controlled performance comparison. CI records
its own build timing on every runner.

The desktop's empty-list release run on this Mac measured mean RSS 84.75 MiB, peak 107.78 MiB
and physical footprint 102.0 MiB (same 8 s warm-up/20-sample method). Five real desktop torrents
remain gated until M3, so the core-only figure above must not be added to this footprint and
presented as a measured five-torrent desktop result. Raw local results are in
[phase-4-m1-macos.json](../measurements/phase-4-m1-macos.json).


Open: M2–M5, Windows/Ubuntu ARM VM acceptance, qBittorrent/Transmission interop, production
swarm memory/CPU, and both upstream contributions. Protocol encryption and v2 remain unsupported.

Public Ubuntu exit test on this Mac: `dawnload get <magnet> --to <temporary folder>` downloaded
all 3,303,444,480 bytes of Ubuntu 24.04.3 live-server amd64 from the public swarm. A deliberate
stop/relaunch continued from fast resume with the final release CLI. The finished ISO's SHA-256
was `c3514bf0056180d09376462a7a1b4f213c1d6e8ea67fae5c25099c6fd3d8274b`, matching
[Canonical's published checksums](https://releases.ubuntu.com/24.04.3/SHA256SUMS).
It reached Seeding; public peers requested no upload during 120 s. A separate librqbit 9.1
Session on loopback then requested pieces from Dawnload: Dawnload reported 11,063,296 bytes
uploaded and the peer verified 9,605,120 bytes before stopping. This proves upload behavior,
not public-swarm demand or qBittorrent/Transmission interop. The CLI stopped cleanly afterwards.
The torrent/checksum URLs, info hash and results are in the raw measurement JSON linked above.

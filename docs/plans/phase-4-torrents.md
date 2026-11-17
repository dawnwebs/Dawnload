# Phase 4 plan: torrents on librqbit

Decisions: [ADR 0011](../adr/0011-torrents-on-librqbit.md). Lifecycle contract: [ADR 0003](../adr/0003-seeding-inside-the-job.md).
Seam today: `crates/dawnload-core/src/engine/torrent.rs` (accepts magnets and `.torrent`,
reports "not built yet").

Each milestone leaves `main` shippable: CI green on all five runners, the app usable, nothing
half-wired visible to people (unfinished UI stays behind `--demo` or a Settings flag until its
milestone closes). One pull request per milestone.

## M1 · The engine works (core only)

Implementation status (2026-11-17): built, locally tested and measured; the public Ubuntu ISO download,
checksum, stop/relaunch and upload verification passed on this Mac. All six checks passed on
`bfea393`. Review fixes for portable torrent names and repeated phase events now require a new
green run, including a native Windows path probe. M2 has not started. Measurements and remaining
gates are in ADR 0011's Consequences. This milestone is not marked done until those gates pass.

- Add `librqbit = { version = "9.1", default-features = false, features = ["rust-tls"] }`; check
  `cargo deny`, binary size and build time on the five runners.
- `TorrentService` in the core: one lazy `Session` per core, `<data>/torrents/` for librqbit's
  persistence and DHT state, the port chosen once per install and stored in Settings.
- `TorrentEngine::run`: add by magnet / `.torrent` bytes / `.torrent` URL (fetched by the HTTP
  engine's client); emit `Phase(Pulling, "Getting the file list")` until metadata, then `Files`;
  `Transfer` and `Swarm` from librqbit's stats at the core's batching rate; `Phase(Pulling,
  "Checking")` while verifying; `Phase(Seeding)` when complete; `Ok` at the seed limits;
  `Err(Stopped)` on Pause / Stop Seeding / quit.
- SQLite: info hash, metadata bytes and file selection per item; rebuild librqbit's state from
  them when its persistence is missing.
- File selection while running (`update_only_files`) through a new `Action::SelectFiles`.
- **Tests (offline, every runner):** a swarm harness that creates a torrent from a temp folder in
  a seeder `Session` and joins Dawnload's `Session` to it with `initial_peers`. Cases: magnet →
  metadata → files; `.torrent` file; download completes and hashes match; pause/resume mid-way;
  quit/relaunch resumes from fast-resume; deleted librqbit state rebuilds from SQLite; file
  selection skips unticked files and changes while running; Stop Seeding; the lane's slot is
  freed at Seeding (ADR 0003's test, now with the real engine).
- Exit: `dawnload get <magnet>` downloads and seeds a public Ubuntu ISO torrent on this Mac.

## M2 · Seeding policy

- Seed limits: ratio (default 1.0) or time (default 7 days), first one wins; per-item override in
  the inspector's Options; "until I stop it" as a choice.
- Courtesy mode: an OS signal for metered networks (Windows `NetworkCostType`,
  NetworkManager's `Metered`, macOS `NWPath.isConstrained` / `isExpensive`) and battery; pauses
  uploading, never downloads; shown in the Seeding subline ("Paused on battery").
- Upload cap "Auto": track the highest sustained upload rate, cap at 70% through
  `ratelimits.set_upload_bps`; Fixed and Unlimited choices.
- Seeding survives restart: items in Seeding at quit start seeding again at launch (supersedes
  ADR 0003's last bullet for seeding; downloads still come back Paused).
- Tests: ratio and time limits end the job with `Ok` (time faked through a clock trait); courtesy
  pause/resume through a fake network-cost source; restart keeps seeding.
- **Cumulative accounting acceptance:** persist lifetime uploaded bytes in SQLite and carry them
  across Pause/Resume and quit/relaunch without counting a run twice. A near-limit torrent must
  stop after the remaining upload budget, even across repeated restarts. Per-run librqbit stats
  are not the lifetime counter; changing the selected-size denominator must have a defined test.

## M3 · The torrent experience (UI)

- The file chooser in the inspector: tree with sizes, Select all / none, smart default (extras
  unticked), executable badges, "Getting the file list" state for magnets.
- Settings › Torrents: seed limits, courtesy mode, upload cap, download/active limits, port with
  a "Test port" result, UPnP, DHT/PEX/LSD, IP block list URL, "Open magnet links and .torrent
  files with Dawnload".
- The first-use sharing note (once, from DESIGN.md); the Seeding tab and filter live; header
  totals (↓ and ↑) when anything torrent is active; the tray badge counts seeding separately.
- Open magnet / `.torrent` handlers: Windows per-user registry (`HKCU\Software\Classes`), Linux
  desktop entry `MimeType` (`x-scheme-handler/magnet`, `application/x-bittorrent`), second launch
  hands the link to the running app (already built in phase 3).
- **Chooser scheduling acceptance:** waiting for a file choice must not consume a torrent
  download slot. Keep the waiting job owned and stoppable; when a choice is accepted, reacquire
  capacity through the scheduler before starting payload traffic. Test a full set of waiting
  choosers alongside a ready torrent and cancellation while waiting.
- Accessibility: the file tree as a list with checkboxes and labels; the swarm numbers as text.

## M4 · Safety

- "Only through this connection": list network interfaces per OS, bind with
  `bind_device_name` where supported, watch the interface and pause every torrent within one
  second of it going down; show the state in the header ("Torrents paused: VPN off").
- Mark of the Web on Windows for every finished file, torrent and HTTP engine (`Zone.Identifier`
  with `ZoneId=3` and the source URL when it isn't a magnet).
- Executable files: badge, and Open asks first.
- Tests: the kill switch with a fake interface watcher; MOTW written and read back on the Windows
  runners.

## M5 · Play while it downloads

- A loopback HTTP server in the UI process (bound to `127.0.0.1`, random port, per-file random
  token, Range support) on top of librqbit's `api_stream`; it lives only while something plays.
- **Play** on video and audio files once the first pieces are in; opens the system player, or
  mpv/VLC when found; streaming raises that file's piece priority.
- Tests: Range requests and seeking against the swarm harness; the token is required.

## Upstream track (in parallel with M1–M5)

- librqbit PR: rarest-first piece picking (with in-order kept for streaming and when asked).
- librqbit PR: change a torrent's rate limits while it runs.
- librqbit proposal: a provider-neutral rustls feature (`reqwest/rustls-no-provider`) so
  embedders such as Dawnload can supply ring without also compiling AWS-LC. Keep existing
  `rust-tls` behavior compatible; verify feature combinations and measure provider-only size
  differences before attributing all torrent binary growth to crypto. M1 keeps the required
  published 9.1 `rust-tls` feature set; no local fork or dependency patch.
- Later, if throttling shows up in practice: MSE/PE protocol encryption.
- Phase 4 doesn't wait for these. A fork is the last resort (ADR 0011).

## Real-world testing (one VM at a time)

- Legal torrents only: Ubuntu and Debian ISOs, Fedora, Blender open movies, archive.org's
  public-domain collections.
- Interop: Dawnload downloads from qBittorrent and Transmission seeders and seeds back to them
  (in the VMs, alternating, never both running).
- Windows 11 ARM VM, then Ubuntu 24.04 ARM VM: a magnet end to end, file selection, pause and
  quit/relaunch, seeding to the limit, courtesy mode on a metered connection, the kill switch,
  Play while downloading, the magnet handler from a browser.
- Measure: idle with no torrents (target unchanged, ~100 MB); five active torrents (budget
  +40 MB); CPU while seeding at the cap. Record in ADR 0008's style.

## Done when

- M1–M5 merged, CI green on all five runners.
- The real-world checks above pass in both VMs and are written into ADR 0011's Consequences.
- `docs/ROADMAP.md` phase 4 marked built, with what's open (upstream PRs, encryption, v2).

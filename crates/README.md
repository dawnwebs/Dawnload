# Cross-platform Dawnload (Rust)

The cross-platform Dawnload: a Rust core, a CLI and a Slint UI. It lives beside the Swift app;
see [docs/ROADMAP.md](../docs/ROADMAP.md) for where it's going and [docs/adr/](../docs/adr/README.md)
for why it's built this way.

| Crate | What it is |
|---|---|
| `dawnload-core` | Sources, engines, tools, the queue, SQLite state and the UI-facing API. No UI code. |
| `dawnload-cli` | The `dawnload` command, a client of the core. |
| `dawnload-ui` | The `dawnload-app` window (Phase 2 prototype), Slint over the core. |

## Build and test

Rust 1.89 or newer. On Linux, Slint needs `libfontconfig1-dev libxkbcommon-dev libwayland-dev
libudev-dev libdbus-1-dev` (Debian/Ubuntu names).

```bash
cargo build --workspace
```

```bash
cargo test --workspace
```

## The CLI

```bash
cargo run -p dawnload-cli -- get "https://www.youtube.com/watch?v=SkVqJ1SGeL0" --format compatible
```

- `get <link>…` adds and downloads now with progress; `add`, then `run`, for the queue.
- `list [--filter …] [--json]`, `pause|resume <id|all>`, `retry`, `remove`, `move <id> <place>`,
  `clear kept|failed`, `log <id>`, `explain <text>`.
- `tools status [--check]`, `tools install [tool…]`, `tools update`.
- Options: `--format best|compatible|audio`, `--engine auto|yt-dlp|gallery-dl|direct`,
  `--no-polish`, `--playlist`, `--to DIR`, `--cookies FILE`, `--cookies-from-browser firefox`,
  `--impersonate chrome-146`, `--checksum sha256:HEX`, `--connections N`.
- State lives in the app's data folder (`~/Library/Application Support/Dawnload`,
  `%LOCALAPPDATA%\Dawnload`, `~/.local/share/dawnload`); `--data-dir` or `DAWNLOAD_DATA_DIR`
  points elsewhere. Only one core runs per data folder, so quit the app before changing the queue
  from the CLI.

## The window

```bash
cargo run --release -p dawnload-ui
```

That's the demo: 500 fake items, a tenth of them torrents that seed. `-- --real` runs the real
core with your tools and data folder. Switches for reviews and measurement:
`DAWNLOAD_UI_THEME=light|dark`, `DAWNLOAD_UI_SELECT_FIRST=1`, `DAWNLOAD_DEMO_ITEMS=<n>`,
`DAWNLOAD_UI_STATS=1` (frames and updates per second on stderr), `DAWNLOAD_UI_AUTOSCROLL=<px>`,
`DAWNLOAD_UI_EASE_ROWS=1`, `DAWNLOAD_UI_EXIT_AFTER=<s>`. `scripts/measure-ui.sh` (macOS, Linux)
and `scripts/measure-ui.ps1` (Windows) run the standard measurement.

## Where things are

- `core/src/source.rs`: what was pasted, and Auto routing. `explain.rs`: the detection line.
- `core/src/engine/`: `mod.rs` is the seam (capabilities, lanes, `JobEvent`); one file per engine.
- `core/src/ytdlp.rs`, `filename.rs`, `finishing.rs`, `failure.rs`: ports of the Swift parsing,
  smart filenames, polish and error wording, with their tests.
- `core/src/tools.rs`: discovery, official release sources, verification, updates.
- `core/src/core.rs`: the actor and scheduler. `store.rs`: SQLite. `api.rs`: snapshot, diffs,
  `Mirror`. `present.rs`: the row text from DESIGN.md.
- `core/tests/scheduler.rs`: the queue's promises, with a scripted engine.
- `ui/ui/app.slint`: the window. `ui/src/main.rs`: the bridge from the core to Slint.

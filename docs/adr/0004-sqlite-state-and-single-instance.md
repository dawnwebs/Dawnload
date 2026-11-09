# 0004 · SQLite for state, one core per data folder

Status: Accepted (2026-11-09)

## Context

The Swift app saves the list as one JSON file, debounced. That rewrites everything on every change
and can lose the last moments on a crash. The cross-platform app will hold thousands of items
(torrents, playlists, RSS later), and a quit or crash must lose nothing.

## Decision

- **SQLite (bundled via `rusqlite`, WAL, `synchronous=NORMAL`)** at
  `<data>/dawnload.sqlite3`. Tables: `items` (id, position, status, a JSON `data` body, updated
  time), `logs` (capped at 400 lines per item), `settings` (key/value JSON). `user_version` is
  the schema version; a database from a newer Dawnload is refused, not misread.
- **JSON body per item.** The columns are what we query (position, status); the rest is the
  `Item` as JSON, so adding a field needs no migration. Rows that fail to parse are skipped, not
  fatal.
- **A writer thread with batched transactions.** The actor sends `StoreOp`s; one thread drains
  whatever is queued into a single transaction. Status changes are written immediately;
  progress numbers every 2 seconds.
- **Recovery rule.** On start, anything that was Pulling, Polishing or Seeding becomes Paused. The
  engines continue from partial files (yt-dlp `--continue`, the HTTP engine's
  `.dawnload-part` + `.dawnload-state`, librqbit's own resume data).
- **Ids are never reused** (a `next_id` setting), so `dawnload log 5` always means one item.
- **One core per data folder.** The core takes an OS file lock on `<data>/dawnload.lock`
  (`std::fs::File::try_lock`). A second app or `dawnload run` gets a clear message instead of two
  schedulers running the same queue. Phase 6's local control API will let the CLI and the
  browser extension talk to the running core instead.

## Alternatives

- **JSON file**: simple; rewrites everything and has no transactions.
- **sled / redb**: pure Rust, but we want ad-hoc queries (history search, stats) and SQLite's
  tooling and track record.
- **Several processes sharing SQLite**: possible with WAL, but two schedulers would double-start
  items; one owner plus an API is simpler to reason about.

## Consequences

- Tested: `nothing_is_lost_across_quit_and_crash` (quit, crash simulation, ids, the lock),
  `round_trips_items_order_logs_and_settings`, `caps_logs_per_item`, `refuses_a_newer_schema`.
- SQLite adds ~1.5 MB to the binary and a C compile; it's public domain.
- Until phase 6, CLI commands that change state need the app to be closed.

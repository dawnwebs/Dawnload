# 0002 · The core owns all state; UIs get a snapshot and batched diffs

Status: Accepted (2026-11-09)

## Context

A busy queue produces thousands of progress lines a second (500 demo items emit ~2,500 events/s;
one yt-dlp download prints several lines a second). The Swift app applied each event on the main
thread with an animation, which is fine for one download and wasteful for hundreds. Phase 3's UI,
the CLI and a future local remote-control API (phone remote, browser extension) all need the same
view of the queue, and none of them should re-implement grouping or wording.

## Decision

- **One actor owns everything.** `core::Core` runs as a single tokio task that owns every item,
  the queue order, logs and settings. Jobs send events into it; UIs send commands
  (`CoreHandle::add`, `act(Action)`, `explain`, …). No locks around the model; no UI ever
  mutates an item directly.
- **Subscribe, then diffs at a bounded rate.** `CoreHandle::subscribe()` returns a
  `Subscription` whose first message is an `Update::Snapshot` and then `Update::Batch`es. The core
  marks what changed and flushes once per `update_interval` (250 ms, 4 per second), so a
  subscriber sees at most 4 batches a second no matter how many events arrive. A batch carries
  `Upsert` (an `ItemView`: one row's worth, no log), `Remove`, `Order`, `Counts`, `Queue`,
  `Tools`, `Notice`, and `Detail` for the one item that subscriber `watch`es (its log and
  files, for an inspector).
- **Slow subscribers never build a backlog.** Each subscriber has a small bounded channel. If it's
  full, the subscriber is marked and gets a fresh `Snapshot` when it has room again, instead of a
  queue of stale batches.
- **`api::Mirror` is the client half.** Every UI applies updates to a `Mirror` and asks it for
  `rows(filter)`: sections in order (Now, Up next, Paused, Seeding, Finished), queue positions,
  newest-first finished items. `present::subline` gives the row text from DESIGN.md. So the app,
  the CLI and a remote show the same thing.
- **Everything is `serde`.** `Update`, `Action`, `Explanation`, `ItemView` serialize to JSON as
  they are; the remote API in phase 6 is a socket that forwards them, with no new model.
  `dawnload list --json` already prints a `Snapshot`.

## Alternatives

- **Shared model behind a mutex, UI polls**: simple, but every reader locks the writer and polls
  even when nothing changed.
- **Push every event to the UI**: what the Swift app does; costs scale with event rate, not with
  what's on screen.
- **Observable properties per item (signals)**: fine in one process, doesn't cross a socket, and
  couples the core to a UI framework.
- **A database as the bus (UIs read SQLite)**: durable, but latency and write amplification on
  every progress tick.

## Consequences

- The Slint prototype receives 4 batches a second and patches only changed rows; 500 items
  updating cost ~5% of one core including the demo engine itself (ADR 0008).
- Adding a UI means writing a `Mirror` consumer; adding a remote means a transport.
- The rate is a knob (`CoreConfig::update_interval`); tests run at 20 ms.
- Commands are asynchronous messages, so a UI never blocks on disk or network; replies come back
  through oneshot channels.

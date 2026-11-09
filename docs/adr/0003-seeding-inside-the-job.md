# 0003 · Seeding happens inside the job's run; lanes and slots

Status: Accepted (2026-11-09)

## Context

The Swift engine seam let a job return `continuesSeeding: true` and keep running. But
`AppModel.finish()` then removed the job from `jobs` and from its lane, so nothing held a
reference to the still-seeding job: Stop Seeding couldn't reach it, and the lane counted a slot as
free while work continued. Separately, one lane for everything web meant a 4 GB ISO would block
a two-minute video.

## Decision

- **A job's `run` is its whole life.** An engine's future resolves only when the item is done
  with: kept, failed, or stopped. A torrent job emits `JobEvent::Phase(Seeding)` once its files
  are on disk and keeps running (emitting `Swarm`) until it reaches
  `options.seed_ratio_limit`, then returns `Ok`; or it's stopped and returns `Err(Stopped)`.
- **The core keeps the job until its future ends.** On `Phase(Seeding)` the item becomes
  Seeding (files kept, `finished_at` set) but stays in `running` with its `StopSignal`, so
  `Action::StopSeeding` reaches it. How a stop ends depends on why it was asked: Stop
  Seeding → Kept; Pause → Paused; Remove → deleted; quit → Paused, so it resumes next launch.
- **Seeding doesn't hold a download slot.** A lane's busy count is its running jobs that aren't
  seeding; when a job starts seeding, the queue starts the next item in that lane immediately.
- **Lanes per kind of work, with per-lane limits in Settings:** `Web` (yt-dlp, gallery-dl;
  1 by default, as DESIGN.md says: sites throttle parallel scrapers and people expect the top item
  first), `Direct` (built-in HTTP; 3), `Torrent` (3), `Demo`.
- **Continue on error.** A failure, a missing tool, a panic inside an engine (caught by running
  each job in its own task) fails that item and the queue moves on. Network failures
  (`FailureKind::Network`, HTTP 408/429/5xx) retry automatically with backoff (5 s, 10 s; 2 tries
  by default) before the item is Failed.

## Alternatives

- **Separate "seeder" registry after Kept**: the Swift shape; two owners for one item, and the
  bug above.
- **Seeding as a new job started after Kept**: re-checks pieces and reopens files for nothing.
- **One global concurrency limit**: simpler, but lets one kind of work starve another.

## Consequences

- Tested: `seeding_frees_the_slot_and_stop_seeding_reaches_the_job`,
  `one_failure_never_stops_the_rest` (including a panicking engine),
  `dropped_connections_retry_on_their_own`, `a_lane_runs_its_limit_and_lanes_are_independent`.
- Pause All stops downloads, not seeding (seeding is giving back, not taking).
- librqbit (phase 4) maps naturally: one managed torrent per job, its stats as `Swarm` events.
- An item seeding when Dawnload quits comes back Paused; Resume re-checks and seeds again.

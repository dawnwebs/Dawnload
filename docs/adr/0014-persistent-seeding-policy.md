# 0014 · Persist the seeding budget and restart intent

Status: Accepted (2026-11-18), implementing M2 of ADR 0011.

## Context

librqbit tracks uploads on the live torrent and resets the counter when it is paused. Its session
cache cannot decide which Dawnload items are allowed to resume. The engine also applies rate
limits per request block, so a cap smaller than one block can prevent uploads entirely.

## Decision

- An item's SQLite record checkpoints its first completion time and lifetime uploaded bytes.
  The seed ratio divides lifetime bytes by the current selection's logical size. Changing the
  selection changes the denominator and keeps the credit already earned. Uploads during the
  download count toward the ratio. Limits apply once the current selection is complete.
- The default deadline is seven calendar days from first completion. It includes time offline,
  on courtesy pause or on manual pause. A changed file selection keeps the original deadline.
  A ratio of 1.0 is the default. Each limit can be disabled independently. With both disabled,
  the person stops seeding explicitly. New items copy the global defaults at intake.
- A running job checkpoints absolute upload totals. It samples the previous live counter again
  on pause before starting a fresh counter. When reading M1 rows, Dawnload credits the recorded
  upload count and completion time where available. That count may represent only the final
  M1 run. M2 checkpoints are authoritative when present.
- The queue restarts items recorded as Seeding. Before allowing payload traffic it loads the
  saved metadata, checks its hash, waits for librqbit to initialize, and confirms the selected
  files are complete and within budget. librqbit uses its usual fast-resume verification and
  checks pieces when the cache cannot be used. Missing payload or metadata leaves the item
  Paused. Explicit Pause, Stop Seeding and Remove take precedence over restart. Queue
  maintenance commands defer startup traffic, and the desktop gate stays in place until M3.
- Courtesy mode parks completed handles on battery power or a costly network while keeping the
  job in Seeding with a reason. One sampled result is shared across the session for two
  seconds. macOS uses Network.framework and IOKit, Windows uses the connection profile and
  system power status, and Linux reads NetworkManager and UPower over the system bus. A signal
  the platform does not supply does not trigger courtesy. An incomplete selection can
  download under the same conditions.
- Auto measures session-wide uploads over a five-second window and applies 70% of the measured
  rate, with a 16 KiB/s floor. The first sustained sample becomes the saved estimate. A
  connection change can make the saved estimate stale. The user can relearn explicitly,
  which clears the estimate and temporarily allows uncapped upload while measuring. Fixed
  and Unlimited are also available. The cap applies to all torrent uploads in the session.

## Alternatives

Charging only time actively seeding would let a torrent with no interested peers remain scheduled
through repeated long pauses. Starting a separate background job for courtesy or restart would
give the same torrent two lifecycles. Periodically lifting Auto's cap would detect faster
connections at the cost of spikes while people are using the network.

## Consequences

- An upload near the limit may finish a few in-flight blocks before the next sample. Normal
  stops checkpoint final observed totals. Abrupt termination can lose the final checkpoint
  interval, and librqbit does not join every socket write before pausing.
- The CLI can edit global defaults and a queued, paused or failed item's limits. The inspector
  editor is available in demo while the M3 desktop feature is gated. A seeder whose selection
  expands can need a download slot again. M3's chooser work must coordinate that slot with
  the queue.
- OS adapters use [Apple's network path API](https://developer.apple.com/documentation/network/nw_path_t),
  [Windows connection costs](https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.connectioncost),
  [NetworkManager metering values](https://networkmanager.dev/docs/api/latest/nm-dbus-types.html)
  and [UPower power status](https://upower.freedesktop.org/docs/UPower.html).

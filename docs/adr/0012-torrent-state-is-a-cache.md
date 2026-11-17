# 0012 · Torrent metadata in SQLite; librqbit's index is a cache

Status: Accepted (2026-11-17), implementing ADR 0011 M1.

## Context

librqbit 9.1 restores every entry in its JSON session database when a Session starts, including
uploads. Dawnload's SQLite may say those items are paused, stopped, removed or failed. Letting
both databases restore work would give jobs two owners and could start traffic outside the
scheduler. Metadata is also too large to include in every item's progress JSON or UI snapshot.

## Decision

- Schema 2 adds `torrents(item_id, info_hash, metadata BLOB)`, with an item foreign key and
  cascading deletion. File selection remains in the item's SQLite JSON, in metadata order.
  The actor caches metadata for immediate Resume; the writer persists it independently of
  progress. `JobEvent::Torrent` carries metadata as soon as it is resolved, before payload starts.
- Before starting the one lazy Session, replace `<data>/torrents/session.json` with an empty
  index. Keep librqbit's hash-keyed `.bitv` files. Only a scheduled job adds its torrent, from
  SQLite's metadata when available; librqbit validates fast resume and checks pieces when the
  cache is missing or corrupt. DHT persistence is also inside `<data>/torrents/`.
- Add torrents paused, apply selection, then unpause. A job's exit always pauses the managed
  torrent; removing an item retires the handle and cache without deleting the person's files.
  A paused job reuses its handle. Because librqbit permits one handle per info hash, a second
  item for the same hash fails clearly instead of sharing ownership. Removing the first item
  allows adding it again. A torrent with known files keeps its destination; changing it means
  removing and adding the item again, with overwrite protection for the new destination.
- The CLI enables the engine in M1. The desktop registry excludes it until M3, so unfinished
  controls and the missing sharing note cannot start real torrents. Demo torrents still work.
- The port is random in 49152–65535 and persisted once per data folder. A port change takes
  effect on the next launch. Production enables DHT, trackers, PEX, LSD and UPnP. Offline tests
  disable discovery, trackers and UPnP, bind loopback, and supply only generated seeder peers.

## Alternatives

Restoring librqbit's index and pausing it afterwards leaves a window for unwanted traffic.
A custom persistence backend would avoid resetting the index, but librqbit 9.1 does not expose
its persistence trait. Putting metadata in item JSON needlessly rewrites it with progress.

## Consequences

- One owner of the lifecycle, one session, and recoverable items even if the entire librqbit
  directory disappears. SQLite BLOBs never travel in row updates or inspector snapshots.
- A core drains job events before handling completion, so a quick success cannot discard its
  last metadata or file events. Shutdown releases the data-folder lock before acknowledging.
- Seeding restart, lifetime upload accounting, time limits and courtesy controls remain M2.
  Until then quit returns seeding items as Paused, exactly as the M1 milestone boundary requires.
- The requested `rust-tls` feature activates reqwest 0.13's AWS-LC dependency, in addition to
  ring. No native TLS/OpenSSL, HTTP API, web UI or Prometheus features are enabled. The build
  therefore needs AWS-LC's C toolchain even though Dawnload installs ring as its TLS provider.

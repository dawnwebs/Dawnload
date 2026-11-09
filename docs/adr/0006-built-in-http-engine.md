# 0006 · A built-in HTTP engine for plain files

Status: Accepted (2026-11-09)

## Context

"Paste anything" includes plain file links: an ISO, an installer, a PDF, a zip. yt-dlp can fetch
those through its generic extractor, but with one connection, no checksum check and resume that
depends on its own `.part` logic. A downloader people leave running needs this to be first-class.

## Decision

`engine::http::HttpEngine` (lane `Direct`), on `reqwest` with rustls (ring provider) and the OS
certificate store:

- **Probe** with `GET Range: bytes=0-0`: a 206 means ranges work and gives the size; also ETag,
  Last-Modified, `Content-Disposition` filename (`filename*` preferred) and
  `Repr-Digest`/`Digest` headers.
- **Several connections per file** (4 by default, `options.connections`, up to 16) over segments
  of at least 1 MiB, with **work stealing**: a connection that finishes takes the back half of the
  largest segment still running, so one slow connection never holds up the end. Each connection
  writes through its own file handle into a pre-sized `name.ext.dawnload-part`; bytes are
  reserved under a lock before they're written, so a moved segment end never overlaps.
- **Resume** from a JSON sidecar `name.ext.dawnload-state` (URL, size, validators, each segment's
  progress), saved every 2 s and on stop. It's reused only if size and validators still match;
  otherwise the file starts over and the log says why. Servers without ranges get one
  connection and start over after a stop (the log says that too).
- **Checksums** from, in order: the item's option (`--checksum sha256:…`), the link's fragment
  (`…/file.iso#sha256=…`), or the server's digest header. SHA-256 and SHA-512. A mismatch deletes
  the part file and fails the item with expected and actual hashes; nothing unverified is kept.
- **Retries** per connection with backoff for dropped connections and 5xx; 404/403/401 fail at
  once with the brand's sentences; 416 means the file changed.
- A Netscape cookies.txt becomes a `Cookie` header for matching domains.

## Alternatives

- **yt-dlp's generic extractor**: no multi-connection, no checksum, one more process.
- **aria2c as an external tool**: excellent, but another binary to install and update, and JSON-RPC
  to drive; our own engine is ~650 lines (plus tests) and testable in-process.
- **Static segments only**: simpler, but the last segment's slow tail dominates.

## Consequences

- Tested against a local server (multi-connection with checksum, wrong checksum, pause and
  resume, a server without ranges, 404) and for real on macOS: Big Buck Bunny from
  download.blender.org (121 MB and 725 MB zips; the 725 MB one interrupted with Ctrl-C at 8%,
  resumed by `dawnload run` from 63.7 MB over 4 connections, `unzip -t` clean), and deno's zip
  from GitHub with its published SHA-256 (match kept, wrong hash rejected).
- Not yet: bandwidth limits, mirrors/metalink, HTTP auth prompts, proxies beyond the system
  setting. They fit the same engine.

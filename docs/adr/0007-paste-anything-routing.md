# 0007 · "Paste anything": routing and the detection line

Status: Accepted (2026-11-09)

## Context

The pitch is one field for everything: a video page, a gallery, a magnet, a `.torrent` file or
link, a plain file URL. People shouldn't have to know which engine does what, but they should see
what will happen before it happens (DESIGN.md › Intake).

## Decision

- `DownloadSource::parse` (ported from Swift, same tests): http(s) links (with the URL clean-ups
  the Swift app had), `magnet:` links (hex, base32 and v2 hashes, `dn`, `tr`, `xl`), and
  `.torrent` paths (`/…`, `~/…`, `file://…`, Windows absolute paths).
- `EnginePreference::Auto` resolves per link, in this order: magnets and `.torrent` files and
  `.torrent` links → Torrent; Instagram posts → gallery-dl, reels → yt-dlp; known gallery hosts →
  gallery-dl; known media sites (YouTube, Vimeo, TikTok…) → yt-dlp even if a path looks like a
  file; a path ending in a file extension (archives, disk images, installers, documents, plain
  media files) → the built-in HTTP engine; everything else → yt-dlp. The Engine chip can force
  yt-dlp, gallery-dl or Direct.
- `explain::explain` produces the detection line and whether Download is enabled, from the
  source, the resolved engine and the tools found:
  `youtube.com · yt-dlp will download this (Auto)`,
  `releases.example.org · debian-13.iso · Dawnload will download this file directly over 4 connections (Auto)`,
  `Magnet link · Ubuntu 24.04 · Torrent downloads aren't built yet.`,
  `vimeo.com · yt-dlp isn't installed. Dawnload can install it for you: …`, and the YouTube
  JavaScript-runtime warning.
- **Adding is idempotent:** the same link while an item for it is still waiting, running or
  paused returns that item rather than a duplicate (good for the browser extension later).

## Alternatives

- **Probe every link over the network** (HEAD for Content-Type): more accurate for extension-less
  file URLs, but the field would wait on the network while typing. A future refinement can probe
  after the person stops typing.
- **Always yt-dlp**: what the Swift app does for web links; slower and unverified for plain files.

## Consequences

- Wrong guesses are cheap: the Engine chip overrides, and the yt-dlp failure for an unsupported
  page says to try gallery-dl (and vice versa).
- The torrent route exists today and reports "not built yet"; phase 4 only changes availability.

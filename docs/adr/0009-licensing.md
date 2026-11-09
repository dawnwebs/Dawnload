# 0009 · Licensing of dependencies and tools

Status: Accepted, **needs the owner's review** (2026-11-09)

## Context

Dawnload is MIT. The cross-platform app links Rust crates into one binary and runs external tools
as separate processes. We want the code to stay MIT and the shipped app to be distributable
without surprises.

## Decision

- **Rust dependencies** are MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, Unlicense, BSL-1.0 or
  CC0. `deny.toml` lists what's allowed and CI runs `cargo deny check licenses`. Exceptions:
  - **Slint** (14 crates): `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
    LicenseRef-Slint-Software-3.0`. Dawnload uses the **Royalty-free License 2.0**, which allows
    desktop apps at no cost but **requires attribution**: show Slint's `AboutSlint` element (for
    example in an About window) or a "Made with Slint" badge on the website or download page.
    It does not cover embedded devices. Our own source stays MIT; the shipped binary carries this
    term. Phase 3 adds the About window with `AboutSlint`.
  - **option-ext** (MPL-2.0), pulled in by `directories`. File-level copyleft; we don't modify it,
    so nothing extra is owed. Can be dropped by replacing `directories` (~60 lines) if wanted.
  - **webpki-root-certs** (CDLA-Permissive-2.0): Mozilla's root list as data.
- **SQLite** is public domain (bundled through `rusqlite`).
- **External tools are never linked or bundled.** yt-dlp (Unlicense), gallery-dl (GPL-2.0),
  ffmpeg (the GPL builds from FFmpeg-Builds and martin-riedl.de) and deno (MIT) are downloaded
  by the person's own copy of Dawnload from their publishers (ADR 0005) and run as separate
  processes. Dawnload doesn't redistribute them, so their licences don't reach Dawnload's code
  or installer.
- **librqbit** (Apache-2.0) is the phase-4 torrent engine, linked in-process: compatible.
  libtorrent (BSD-3-Clause) as the backup is too.

## Alternatives

- **Slint under GPL-3.0**: would make the shipped app GPL. No.
- **Slint's paid licence**: removes the attribution requirement; not needed now.
- **A permissive toolkit (egui, iced, Tauri)**: see ADR 0001; the licence alone didn't outweigh
  Slint's results.

## Consequences

- **Owner to review:** whether the Slint royalty-free terms (attribution, the desktop-only scope)
  are acceptable for Dawnload's distribution (winget, Flathub, Homebrew cask, a website). If not,
  phase 3 should switch to the fallback before building more UI.
- `cargo deny` hasn't been run locally (not installed here); CI will run it.

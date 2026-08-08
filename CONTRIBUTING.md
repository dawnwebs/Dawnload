# Contributing to Dawnload

First off: thank you for being here. Dawnload is a Dawn Labs open-source project built for
the community, and every contribution — from a typo fix to a new extractor — makes the
dawn a little brighter.

## Ground rules

- Be kind. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md); kindness is the compile-time default.
- Download the web responsibly. We do not support bypassing paywalls, DRM, or access you
  were not given.
- Keep the app native. No Electron, no webviews, no unnecessary dependencies. A Mac app
  should feel like a Mac app.
- Keep the code small and readable. Small functions, descriptive names, no cleverness
  that has to be explained twice.

## Code of conduct

Everyone participating in discussions, issues, and pull requests agrees to abide by our
[Contributor Covenant](CODE_OF_CONDUCT.md).

## Finding your first issue

Look for issues labeled `good first issue` or `help wanted`. Never found one? Open a new
issue describing what you want to build — the maintainers are friendly, and most ideas
become issues within a day.

## The lay of the land

The repo is a vanilla Swift Package (no Xcode project — `swift build` is all you need).

```
Sources/DawnloadCore/       Pure logic, zero UI: URL sanitizing, metadata parsing, filenames
Sources/Dawnload/           The SwiftUI app: view model, process runner, tool detection, UI
Tests/DawnloadCoreTests/    Tests for the Core. (App-layer UI is not unit-tested yet.)
Scripts/                    install.sh + package-app.sh + make-icon.swift
```

**The golden rule:** anything you can put in `DawnloadCore` belongs in `DawnloadCore`.
Logic that does not need a window is logic that can be tested on a CI runner without a Mac.

## Setting up your dev environment

Requirements:

- macOS 13 Ventura or newer
- Swift toolchain 5.9+ (Xcode Command Line Tools are enough)
- `yt-dlp`, `gallery-dl`, `ffmpeg` for anything that actually downloads
  (`brew install yt-dlp gallery-dl ffmpeg`)

Build and test:

```sh
swift build
swift test
```

Run the app:

```sh
swift run Dawnload
```

Package a `.app` bundle to test the "real app" experience:

```sh
./Scripts/package-app.sh
open .build/Dawnload.app
```

## Workflow

1. **Fork** the repo and clone your fork.
2. Create a branch with a descriptive name: `fix/queue-crash`, `feature/speedometer`, `docs/readme-faq`.
3. Make your change. Add **tests for anything in DawnloadCore**. Small PRs land faster —
   if it feels too big, split it.
4. Run the whole suite and make sure it is green:

```sh
swift test
```

5. Commit with a clear message. We keep a linear history and squash on merge, so no
   worries about messy intermediate commits:

```sh
git add .
git commit -m "fix: stop the queue from emptying when a single URL fails"
```

6. Push to your fork and open a pull request against `main`.
7. In the PR description, tell the story: what, why, how. Screenshots for UI changes
   are gold.
8. Maintainers will review within a few days. Small comments? Fix them. Every
   conversation gets answered.

## Style

- We follow Swift API Design Guidelines, but pragmatic: `SmartFilenameResolver.swift` is
  a noun class, `URLSanitizer.swift` is a tool; functions do one thing.
- No comments unless they explain a "why" that the code cannot. Naming beats commenta.
- Error messages that float to the UI are sentences with lowercase start.
- Keep line width under 120 characters. Indent with 4 spaces.
- Scripts are `bash` with `set -euo pipefail`. Respect existing style.

## Tests

```sh
swift test
```

Tests live in `Tests/DawnloadCoreTests/`. Name test files after the type they test
(`URLSanitizerTests.swift`). Every new Core behavior needs a bubbling test: both the
happy path and the ugly path.

## Issue and PR labels

Maintainers use the standard GitHub labels. If you take on an open issue, say so in a
comment first so we avoid double-assignment.

## Maintainers

If you want to become a long-term maintainer (not just a contributor), open an issue
with a short intro and an area you love. Dawn Labs grants maintenership generously to
proven contributors.

---

Questions before you commit? Open an issue — the sun is always rising on this repo.
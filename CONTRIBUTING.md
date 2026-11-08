# Contributing to Dawnload

Thanks for contributing. This document covers the project setup, what we ask
of contributors, and how to submit changes.

Please also read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Everyone
participating in issues, discussions, and pull requests agrees to follow it.

## Ground rules

- No code that bypasses DRM, paywalls, or access you were not given. Mirror the
  responsibility note in the README.
- Keep the app native and dependencies light. No Electron, no webviews, nothing
  that adds weight without adding value.
- Anything that does not need a window belongs in `DawnloadCore`, where it can
  be unit tested on any machine. If you put logic in the app layer, explain why.

## Project layout

The repo is a vanilla Swift Package — there is no Xcode project, `swift build`
is all you need.

```
Sources/DawnloadCore/       Pure logic: sources, yt-dlp progress, failures, filenames
Sources/Dawnload/           SwiftUI app: design system, engines, model, views
Tests/DawnloadCoreTests/    Unit tests for DawnloadCore
Scripts/                    install.sh, package-app.sh, make-icon.swift
```

## Setting up

Requirements:

- macOS 13 Ventura or newer
- Swift 5.9+ (Xcode Command Line Tools are enough)
- `yt-dlp`, `gallery-dl`, `ffmpeg` to test anything that actually downloads
  (`brew install yt-dlp gallery-dl ffmpeg`)

Build, test, and run:

```sh
swift build
swift test
swift run Dawnload
```

To test the packaged app:

```sh
./Scripts/package-app.sh
open .build/Dawnload.app
```

## Submitting a change

1. Fork the repo and clone your fork.
2. Create a branch with a descriptive name, e.g. `fix/queue-restart`,
   `feature/htm-video`, `docs/faq`.
3. Make your change. If it touches `DawnloadCore`, add tests — both the happy
   path and the failure path. Tests live in `Tests/DawnloadCoreTests/` and are
   named after the type they cover, like `URLSanitizerTests.swift`.
4. Run the suite and leave it green:

   ```sh
   swift test
   ```

5. Commit with a short message that states what changed, e.g.
   `fix: stop the queue from emptying when a single URL fails`.
6. Push to your fork and open a pull request against `main`. Describe what you
   changed and why; include a screenshot if it touches UI.

Small PRs review faster. If a change is large, split it into a few PRs.

## Style

- Follow the Swift API Design Guidelines. Keep functions small and names
  descriptive.
- Comments only when they explain a "why" the code can't; naming beats
  commenting.
- 4-space indentation, lines up to 120 characters.
- Shell scripts are `bash` with `set -euo pipefail`, matching the existing
  scripts.
- Commit messages: lowercase, imperative, under ~70 characters.

## Picking up an issue

Look for issues labeled `good first issue` or `help wanted`. If you want to
work on an issue, say so in a comment so two people don't start on the same
one. Questions and ideas also work — open an issue and we'll answer.
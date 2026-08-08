# Dawnload

A native macOS app for downloading media from the web. Paste a URL, choose a
backend, pick your options, and download. The heavy lifting is done by three
well-known open source tools — `yt-dlp`, `gallery-dl`, and `ffmpeg` — so there
is no server component and no account to create.

Developed in the open by Dawn Labs. MIT licensed.

![macOS 13+](https://img.shields.io/badge/platform-macOS%2013%2B-black)
[![License](https://img.shields.io/badge/license-MIT-white)](LICENSE)
[![Swift](https://img.shields.io/badge/Swift-5.9-white)](Package.swift)

## Features

- Downloads videos and media from any URL supported by `yt-dlp` or `gallery-dl`
- Switch between backends inside the app
- Quality options for `yt-dlp`: Best, Compatible (prefers MP4/M4A), and Audio (M4A)
- `ffmpeg` polish: merging, remuxing, metadata embedding, and a final faststart pass
- Browser impersonation for sites that reject normal requests (Chrome 146/145/142,
  Safari 18.4)
- Optional Netscape-format `cookies.txt` for sites that need a logged-in session
- Smart filenames: prefers meaningful page titles, falls back to URL slugs, and
  never overwrites existing files (duplicates get a `Title 2.mp4` suffix)
- Playlist toggle for URLs that resolve to a single video or a playlist
- A queue that downloads multiple URLs in order; each item keeps its own options
- Finds `yt-dlp`, `gallery-dl`, and `ffmpeg` in `/opt/homebrew/bin`,
  `/usr/local/bin`, and the app's `PATH`

## Requirements

- macOS 13 (Ventura) or newer
- `yt-dlp`, `gallery-dl`, and `ffmpeg`:

  ```sh
  brew install yt-dlp gallery-dl ffmpeg
  ```

- Xcode Command Line Tools (or full Xcode) and Swift 5.9+ to build

## Installing the app

Dawnload is distributed as source — you build it on your own Mac. The installer
does the whole thing; it checks for missing tools, prompts you when it needs to
install them, compiles the app, and puts it in `/Applications`.

1. If you don't have Homebrew yet, install it: <https://brew.sh>
2. Clone the repo and run the installer:

   ```sh
   git clone https://github.com/brenoandrew/Dawnload.git
   cd Dawnload
   ./Scripts/install.sh
   ```

   Or run it without cloning:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/brenoandrew/Dawnload/main/Scripts/install.sh | bash
   ```

3. The first time you open the app, right-click it and choose **Open**.
   Dawnload is not notarized yet, so Gatekeeper will ask once.

To build manually instead:

```sh
swift run Dawnload                        # run from the terminal
./Scripts/package-app.sh                  # or build a .app bundle
open .build/Dawnload.app
```

## Using Dawnload

1. Paste a URL, including `https://`.
2. Choose the backend: `yt-dlp` for videos and playlists, `gallery-dl` for
   galleries and image sets.
3. Choose a quality option when using `yt-dlp`: Best, Compatible, or Audio.
4. Leave **ffmpeg polish** enabled; it handles merging and keeps the output
   compatible with most players.
5. If a site returns HTTP 403, enable browser impersonation (Chrome 146 is the
   default). If that isn't enough, export a `cookies.txt` file from your browser
   and select it in Settings.
6. Enable **Download playlist** when a URL can resolve to a playlist and you
   want all items.
7. Click **Download** to start immediately, or **Add** to put the URL in the
   queue and download queued items in order.

Keyboard shortcuts: Cmd+D downloads, Cmd+Shift+A adds to the queue,
Cmd+Shift+S starts the queue, Cmd+O chooses a destination, Cmd+R refreshes tool
detection.

## Troubleshooting

- **A tool shows "Not found"** — install it (`brew install yt-dlp gallery-dl
  ffmpeg`) and click **Refresh** in the app.
- **Downloads fail with HTTP 403** — enable browser impersonation, try a
  different profile, then add `cookies.txt` from Settings, then update
  `yt-dlp` (`brew upgrade yt-dlp`).
- **Impersonation profiles are missing** — update or reinstall `yt-dlp`; the
  profiles require a build with `curl_cffi` impersonation support. Check what
  your install supports with `yt-dlp --list-impersonate-targets`.

## Project layout

```
Package.swift                 Swift package manifest
Sources/DawnloadCore/         URL normalization, metadata parsing, filename logic (no UI)
Sources/Dawnload/             SwiftUI app: view model, download runner, tool detection
Tests/DawnloadCoreTests/      Unit tests for DawnloadCore
Scripts/package-app.sh        Builds .build/Dawnload.app
Scripts/install.sh            One-command installer for end users
Scripts/make-icon.swift       Generates the app icon
```

Logic that does not need a window lives in `DawnloadCore` so it can be unit
tested on any CI runner.

## Contributing

Pull requests and issues are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md)
for setup instructions and the contribution workflow, and please read
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Acknowledgements

Dawnload wraps three open source projects:

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — video and playlist extraction
- [gallery-dl](https://github.com/mikf/gallery-dl) — gallery and social media extraction
- [ffmpeg](https://ffmpeg.org) — media processing

## License

MIT. See [LICENSE](LICENSE).

Only download media you have the right to access, and respect site terms and
creators' permissions.

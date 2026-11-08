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
- Picks the engine for you (Auto: `gallery-dl` for gallery sites, `yt-dlp` for
  the rest), or lets you choose
- Quality options for `yt-dlp`: Best, Compatible (prefers MP4/M4A), and Audio (M4A)
- `ffmpeg` polish: merging, remuxing, metadata embedding, and a final faststart pass
- Browser impersonation for sites that reject normal requests (Chrome 146/145/142,
  Safari 18.4)
- Optional Netscape-format `cookies.txt` for sites that need a logged-in session
- Smart filenames: prefers meaningful page titles, falls back to URL slugs, and
  never overwrites existing files (duplicates get a `Title 2.mp4` suffix)
- Playlist toggle for URLs that resolve to a single video or a playlist
- One list that is also the queue: items download in order, can be reordered,
  paused and resumed (from their partial files), retried, and each keeps its own
  options. The list is saved between launches
- Real progress across video and audio parts and playlist entries, with size,
  speed and time left, and a per-item log of what the tool did
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

1. Paste a link, including `https://`. The line under the field says which
   engine will download it.
2. The chips under the field set how the next download runs, and they stick:
   format (Best, Compatible, Audio only), engine (Auto, `yt-dlp`,
   `gallery-dl`), **Polish** (ffmpeg merging, remuxing and faststart),
   **Playlist**, browser impersonation, `cookies.txt`, and the folder.
3. If a site returns HTTP 403, turn on impersonation (Chrome 146 first). If
   that isn't enough, export a `cookies.txt` from your browser and choose it in
   Settings › Access.
4. Press Return (or **Download**) to start now, or Option-Return to add the
   link to the queue and start it later with **Start Queue**.
5. Select an item to see its files, options and log in the inspector. Hover a
   row for Show in Finder, Pause, Resume or Retry; everything is also in the
   context menu and the Download menu.

Keyboard shortcuts:

| Shortcut | Action |
|---|---|
| Return or Cmd+D | Download now |
| Option-Return or Cmd+Shift+A | Add to Queue |
| Cmd+Shift+V | Paste and Download |
| Cmd+Shift+S / Cmd+. | Start Queue / Pause All |
| Cmd+Return | Open, Pause, Resume or Retry the selected item |
| Cmd+Shift+R | Show the selected item in Finder |
| Cmd+Delete | Remove the selected item |
| Cmd+L | Focus the link field |
| Cmd+Shift+O | Choose the download folder |
| Cmd+R | Check tools again |
| Option-Cmd+I | Show or hide the inspector |
| Cmd+1 to Cmd+5 | All, Active, Up next, Kept, Failed |

From the command line, `open -a Dawnload --args <link>` adds a link and
downloads it.

The design language (colours, type, the status glyphs, states, voice) is in
[docs/DESIGN.md](docs/DESIGN.md); the brand kit is in
[docs/brand/](docs/brand/README.md).

## Troubleshooting

- **A tool shows "Not found"** — install it (`brew install yt-dlp gallery-dl
  ffmpeg`) and click **Check Again** (Cmd+R) in the app.
- **Downloads fail with HTTP 403** — enable browser impersonation, try a
  different profile, then add `cookies.txt` from Settings, then update
  `yt-dlp` (`brew upgrade yt-dlp`).
- **Impersonation profiles are missing** — update or reinstall `yt-dlp`; the
  profiles require a build with `curl_cffi` impersonation support. Check what
  your install supports with `yt-dlp --list-impersonate-targets`.

## Project layout

```
Package.swift                 Swift package manifest
Sources/DawnloadCore/         Sources (links, magnets, .torrent), yt-dlp progress, failures,
                              metadata, filenames, formatting (no UI)
Sources/Dawnload/App/         The app scene, menus and shortcuts
Sources/Dawnload/Design/      Brand tokens, the Cut D, the wordmark, shared components
Sources/Dawnload/Engine/      DownloadEngine and its jobs: yt-dlp, gallery-dl, the torrent seam
Sources/Dawnload/Model/       Items, options, the queue (AppModel), persistence, demo data
Sources/Dawnload/Views/       Window, intake, list, rows, inspector, first run, Settings
Sources/Dawnload/Resources/   Unbounded and Martian Mono (SIL OFL)
Tests/DawnloadCoreTests/      Unit tests for DawnloadCore
Scripts/package-app.sh        Builds .build/Dawnload.app
Scripts/install.sh            One-command installer for end users
Scripts/make-icon.swift       Builds the app icon from docs/brand/app-icon/

To review every state without downloading anything, run
`swift run Dawnload -DawnloadDemo YES` (add `-DawnloadTorrents YES` for the
torrent states, `-appearance light` for Paper).
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

<!-- Dawnload — open source project by Dawn Labs. Replace REPO_URL with the real repository URL before publishing. -->

# Dawnload

> Every URL gets its dawn.
>
> Paste any media link into Dawnload and watch it come to life on your Mac — a native, free,
> open-source downloader built by **Dawn Labs** on top of the strongest open tools the
> community makes: `yt-dlp`, `gallery-dl`, and `ffmpeg`.

**Dawnload is a small app with a big philosophy**: the best downloaders in the world are
already open source. We are not here to reinvent them — we are here to wrap them in a
beautiful native macOS window so your grandma could download a playlist without reading a
man page.
>

---

![macOS 13+](https://img.shields.io/badge/platform-macOS%2013%2B-black)
[![License](https://img.shields.io/badge/license-MIT-white)](LICENSE)
[![Swift](https://img.shields.io/badge/Swift-5.9-white)](Package.swift)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-black)](CONTRIBUTING.md)

## What makes Dawnload different

- **Native, not a wrapper.** A real SwiftUI Mac app. Keyboard shortcuts, a proper menu, a collapsible queue, and no Electron weight anywhere.
- **Zero extra accounts.** No login, no cloud, no telemetry. Your downloads stay on your Mac.
- **Choose your engine.** `yt-dlp` for video sites and playlists, `gallery-dl` for galleries and image sets, `ffmpeg` for the polish.
- **Impersonation when sites get rude.** Chrome 146 / 145 / 142 and Safari 18.4 profiles for gatekeeping sites, plus automatic `cookies.txt` support when you need a real browser session.
- **Filenames that make sense.** Dawnload resurrects the title from the URL, rejects boring "just the ID" names, and never overwrites your files — duplicates get a smart `Title 2.mp4`.
- **A queue that respects you.** Add ten links, press one button, walk away.

---

## Install it in 60 seconds (no terminal skills needed)

Dawnload is built from source on your own Mac — the installer takes you along for the ride. Here is the whole thing, step by step:

### Step 1 — Homebrew

Open **Terminal** (Spotlight → type `Terminal` → Enter), paste this, and press Enter:

```sh
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Follow the prompts. Homebrew is the App Store of command-line tools — the download engines we use all ship through it.

> Already have Homebrew? Skip to Step 3 — [`Scripts/install.sh`](Scripts/install.sh) checks everything else for you.

### Step 2 — The download engines

```sh
brew install yt-dlp gallery-dl ffmpeg
```

That is the entire stack. Three open-source command-line giants. Dawnload will protect them like a referee at a powerlifting meet.

### Step 3 — Clone & install

Install the app and let the installer do the rest:

```sh
git clone https://github.com/dawn-labs/Dawnload.git
cd Dawnload
./Scripts/install.sh
```

Or install it from a single command, without ever touching the repo:

```sh
curl -fsSL https://raw.githubusercontent.com/dawn-labs/Dawnload/main/Scripts/install.sh | bash
```

> **What does the installer do?** It checks for macOS 13+, installs the Xcode command line
> tools if needed, verifies Homebrew tools, compiles Dawnload.app, and places it in
> `/Applications`. If something is missing, it tells you exactly what to do.

### Step 4 — First launch

1. Open **Dawnload** from Launchpad or Applications.
2. Right-click → **Open** the very first time you see Gatekeeper (we are not notarized yet — Apple is still getting to know us).

---

## Use it

1. Copy a media URL from anywhere — your browser, your clipboard, your memory.
2. Paste it into Dawnload (including `https://`).
3. Pick the backend: `yt-dlp` for video/playlists, `gallery-dl` for image sets/galleries.
4. Pick quality: **Best**, **Compatible**, or **Audio** (M4A).
5. Leave *ffmpeg polish* on — it is the difference between "works" and "plays everywhere".
6. Getting a 403? Enable *Browser impersonation* (Chrome 146 is the default profile).
7. STILL a 403? Log into the site in your browser, export `cookies.txt`, and point Dawnload at it in Settings.
8. Hit **Download**. Or **Add** to the queue and run them all in order.

Every queued item remembers its own options and destination — queue your links the night
before and let the downloads dawn on you in the morning.

---

## Install for developers

Dawnload is a vanilla Swift Package — an Xcode project file does not exist, and none is needed:

```sh
cd Dawnload
swift build
swift test
```

Run the app directly from the terminal:

```sh
swift run Dawnload
```

---

## Where does the magic live?

```
Package.swift               Swift package manifest (macOS 13+, Swift 5.9+)
Sources/DawnloadCore/       Testable URL normalization + smart filename logic
Sources/Dawnload/           The SwiftUI app: view model, runner, tools, UI
Tests/DawnloadCoreTests/    Regression tests for the Core logic
Scripts/package-app.sh      Builds .build/Dawnload.app with icon + Info.plist
Scripts/install.sh          The one-command installer humans actually use
```

The architecture is deliberately `Core + App`:

- **DawnloadCore** is pure Swift, no UI — easy to unit test, easy to grow.
- **Dawn** (the app) shells out to `yt-dlp` / `gallery-dl` / `ffmpeg` and parses their
  progress metadata to keep you informed — no wrappers, no web APIs, no middlemen we
  don't own.

---

## Contributing

We built Dawnload on the shoulders of the open-source community, and we are set.
Pull requests are welcome, issues are celebrated, and no contribution is too small —
docs, typos, tests, new extractors, UI polish.

Start here: **[CONTRIBUTING.md](CONTRIBUTING.md)**

Also read our **[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)** — kindness is the compile-time default.

---

## Built on open shoulders

Dawnload literally could not exist without the people who pour their weekends into:

- **yt-dlp** — the YouTube-dl successor, a dozen communities of extractors. [link](https://github.com/yt-dlp/yt-dlp)
- **gallery-dl** — the gallery and social-media workhorse. [link](https://github.com/mikf/gallery-dl)
- **ffmpeg** — the tool that does everything on every platform. [link](https://ffmpeg.org)

Rise and shine — the web is yours. Dawn Labs.

---

## License

[`MIT`](LICENSE) © 2026 Dawn Labs and Dawnload contributors.

Please download the web responsibly — only media you have the right to keep, and respect
site terms, creators, and rate limits. Dawn is a tool; you are the user.
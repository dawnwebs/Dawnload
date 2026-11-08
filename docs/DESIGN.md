# Design

How Dawnload looks, behaves and talks on the Mac. The brand (the mark, colours,
type, glyphs and their rules) lives in [brand/README.md](brand/README.md); this
file is the design language the app implements. The code that implements it is
`Sources/Dawnload/Design/` (tokens, the Cut D, components) and
`Sources/Dawnload/Views/` (screens).

## The idea

Dawnload does one thing: **you paste a link and it keeps the file.** Everything
in the window serves that sentence. The paste field is the biggest control, the
list below it is every download you have asked for, and each one wears the
**Cut D**: the D fills from the top while it pulls, the arrow becomes a check
when it is kept, and the cut changes for every other state.

**One colour, many jobs.** Signal green is the D, the Download button,
progress and Kept. Hierarchy comes from size, fill and position, not from more
hues. Coral is the only other colour, and it means Failed.

**Machine words in mono, human words in SF Pro.** URLs, file names, sizes,
speeds, formats and log lines are what the machine owns: Martian Mono. Titles,
labels, buttons and sentences are SF Pro. Unbounded appears once per window at
most, for the one headline or the one big number.

**A Mac app first.** Native menus with shortcuts for every action, a real
`Settings` window, a selectable list with the keyboard, context menus, drag to
reorder, drop to add, an inspector on the trailing edge. Custom drawing is for
the brand's own things (the D, the paste field, the chips), not for re-making
system controls.

**Ready for torrents.** A download is an *item* with a *source* (a web link, a
magnet link or a `.torrent` file), one or more *files*, transfer numbers and,
for torrents, a *swarm*. Engines (yt-dlp, gallery-dl, and later a torrent
engine) sit behind one interface. The screens below already have a place for
every torrent fact, so adding the engine adds behaviour, not a redesign.

## Colour

Roles, not hexes, in code (`DLColor`). Dark is the default: Dawnload lives on
Pine. Light mode follows the system, or the Appearance setting.

| Role | Dark | Light | Use |
|---|---|---|---|
| `canvas` | Pine `#07140E` | Paper `#F2F8F3` | The window |
| `raised` | Grove `#0F2219` | white | The paste field, the inspector, selected rows, chips that are on |
| `rule` | Moss line `#1B3428` | Dew `#DCE9DF` | Hairlines, outlines, chip borders |
| `track` | Moss line `#1B3428` | Dew `#DCE9DF` | The empty part of the D, empty progress |
| `text` | Paper `#F2F8F3` | Pine `#07140E` | Primary text |
| `text2` | Sage `#9DC4AE` | Lichen `#4C6B5A` | Secondary text, sublines, Queued and Paused glyphs |
| `action` | Signal `#17D36E` | Signal `#17D36E` | The Download button fill. **One per view** |
| `onAction` | Pine | Pine | Text and glyphs on Signal. Never white |
| `link` | Signal | Forest `#0B7A3E` | Links, focus, toggles that are on, the selection spine |
| `progress` | Signal | Signal | The D's fill |
| `kept` | Signal | Forest | Kept glyph and the word Kept |
| `failed` | Coral `#FF6B5C` | Coral deep `#B8261A` | Failed glyph and the error sentence |

Contrast: Sage on Pine 9.8:1, Lichen on Paper 5.5:1, Pine on Signal 9.5:1,
Forest on Paper 5.0:1. Under Increase Contrast the `rule` lines step up to
`text2` so outlines stay visible. **Colour never carries a state alone**: every
state has its glyph and its word.

## Type

| Role | Face | Use |
|---|---|---|
| `display` | Unbounded ExtraBold, −2% tracking | The empty-state headline, the first-run headline, the big percent in the inspector. Never running text |
| `title` | SF Pro, title3 semibold | Inspector title, Settings section heads |
| `body` / `label` | SF Pro, body / callout | Row titles (semibold), buttons, sentences |
| `mono` | Martian Mono Medium 11–14 | The paste field, sublines, sizes, speeds, paths, the log |
| `monoStrong` | Martian Mono Bold | Counts in the filter tabs, the percent on a row |

Unbounded and Martian Mono are bundled under the SIL OFL in
`Sources/Dawnload/Resources/Fonts/` (static instances; Unbounded is subset to
Latin) and registered at launch. If registration fails the roles fall back to
SF Pro Rounded heavy and SF Mono, so nothing breaks.

## Shape, space and depth

- **Two radii, from the D.** The round side is the pill (`DLRadius.pill`):
  buttons, chips, filter tabs. The spine's corner, 6 pt (`DLRadius.spine`), is
  for the paste field, rows, file rows and the log. Cards and the first-run
  sheet use 20 pt (`DLRadius.card`).
- **Spacing** is a 4 pt grid: 4, 8, 12, 16, 24, 32. Window gutters are 24 pt;
  rows are 60 pt tall with 12 pt between the glyph and the text.
- **Flat.** No shadows, no gradients, no glass under content. Depth is one step
  of fill (`canvas` → `raised`) and a hairline. The only glow is focus.
- **The spine.** A selected row and a focused field show a 3 pt `link`-coloured
  bar on their leading edge: the D's spine.

## The Cut D in the product

`CutD` draws the mark from the same geometry as the brand generator
(`docs/brand/source/geo.py`), so it is crisp at every size: radius 1, spine
0.53, corner 0.18, cuts punched with round caps.

| State | Cut | Fill | Colour | Word |
|---|---|---|---|---|
| Queued | three dots, D in outline | none | `text2` | Queued, Up next |
| Pulling | the arrow | from the top, to real progress | `progress` over `track` | Pulling |
| Pulling, size unknown | the arrow passing down through the D (the loader) | none | `progress` | Pulling |
| Polishing | two strokes merging into one | full | `progress` | Polishing |
| Kept | the check | full | `kept` | Kept |
| Failed | the bang | full | `failed` | Failed |
| Paused | two bars | to where it stopped | `text2` over `track` | Paused |
| Seeding *(torrents)* | the arrow pointing **up** | full | `kept` | Seeding |

Seeding is the one new cut: the same arrow turned to point up, because a
seeding torrent is giving back what it kept. The logo's arrow still always
points down; the up arrow is a status glyph only. It should join the brand kit
(`states/dawnload-seeding.svg`) when torrents ship.

Sizes: 16 pt in menus and the tools pill, 32 pt on rows, 44 pt in the
inspector, 96 pt in the empty state.

## The window

One window, default 1080 × 720, minimum 780 × 540. The title bar is hidden; the
header row carries the traffic lights, the drag area, the mark and wordmark and
two controls.

```
┌──────────────────────────────────────────────────────────────┬───────────────┐
│ ● ● ●   [D] dawnload                       ● Tools ready  [▣] │               │
│                                                              │  Inspector    │
│ ┌ [🔗] https://…                                [↓ Download]┐ │  (selected    │
│ └────────────────────────────────────────────────────────────┘ │   item: glyph,│
│  youtube.com · yt-dlp will download this                     │   progress,   │
│  (Best ▾) (Auto · yt-dlp ▾) (✓ Polish) (Playlist) (Chrome 146▾) │   files,     │
│  (cookies.txt)                                  (→ Dawnload ▾)│   swarm,      │
│                                                              │   options,    │
│  All 12  Active 1  Up next 3  Kept 7  Failed 1   [Start Queue]│   log)        │
│ ─────────────────────────────────────────────────────────────│               │
│ [D] Big Buck Bunny                                      62%  │               │
│     62% · 28.3 of 45.6 MB · 2.3 MB/s · 0:15 left             │               │
│ [D] Sintel trailer                                           │               │
│     Up next · vimeo.com · Best · Polish                       │               │
└──────────────────────────────────────────────────────────────┴───────────────┘
```

1. **Header** (52 pt): the mark at 18 pt and the drawn wordmark (never typed),
   then on the trailing side the **tools pill** (a dot and "Tools ready", or
   the first missing tool: "yt-dlp missing"; click opens Settings › Tools) and
   the inspector toggle.
2. **Intake**: the paste field, the detection line and the options chips.
3. **The list**: filter tabs with counts, the queue control, and the rows.
4. **Inspector** (300 pt, trailing, toggled with ⌥⌘I): everything about the
   selected item. It hides itself below 960 pt of window width and comes back
   as a sheet on double-click.

### Intake

- **The paste field** is 56 pt tall on `raised`, 6 pt radius, a `rule`
  hairline; focused, the hairline becomes 2 pt `link` and the spine shows. The
  text is Martian Mono 14. The placeholder: *Paste a link to a video, a
  playlist or a gallery*. On the leading side, a glyph for what was detected
  (link, magnet, document). On the trailing side, inside the field, the
  **Download** pill: Signal, Pine label, the arrow before the word.
- **The detection line** under the field says what will happen, in plain words,
  before anything happens:
  - `youtube.com · yt-dlp will download this`
  - `instagram.com · gallery-dl will download this` (Auto picked gallery-dl)
  - `Magnet link · Torrent downloads aren't built yet` (no engine yet)
  - `That isn't a full link. Paste one that starts with https://.` (in
    `failed`, only after a Download was tried)
- **Options chips** set how the *next* item downloads, and they stick: the
  values are the defaults Settings shows. Each item keeps its own copy.
  - **Format** menu: Best (highest quality), Compatible (MP4 and M4A), Audio
    only (M4A; needs ffmpeg). With gallery-dl it reads "Original files" and is
    disabled.
  - **Engine** menu: Auto, yt-dlp, gallery-dl. Auto picks gallery-dl for known
    gallery sites and yt-dlp for the rest, and the chip shows what it picked.
  - **Polish** toggle: ffmpeg merges, remuxes to MP4 and moves the index to the
    front (faststart).
  - **Playlist** toggle (yt-dlp): download the whole playlist the link belongs
    to.
  - **Impersonate** menu (yt-dlp): Off, Chrome 146, 145, 142, Safari 18.4.
  - **cookies.txt** toggle, shown once a cookies file is set in Settings.
  - **Folder** menu, trailing: the folder's name; Choose Folder…, Show in
    Finder.
  Chips that are on sit on `raised` with a `link` check; chips that are off are
  outline only, with `text2` labels. Disabled chips say why in their tooltip.
- **Drop** a link, a text clipping or a `.torrent` file anywhere on the window
  to put it in the field; the window shows a 2 pt `link` outline while
  something is over it.

### The list

- **Filter tabs**: All, Active, Up next, Kept, Failed (and Seeding, once
  torrents ship), each with its count in mono. ⌘1 to ⌘5. A tab with nothing in
  it still shows, with a quiet 0.
- **The queue control**, trailing: *Start Queue* when items wait and nothing
  runs, *Pause All* while something runs, *Resume All* when only paused items
  are left. The ⋯ menu: Clear Kept, Clear Failed, Remove Waiting.
- **Order**: what is active, then what waits (in queue order, draggable), then
  paused, then finished (newest first). The web lane runs **one item at a
  time**, top first; a torrent lane will run beside it.
- **A row** is 60 pt: the 32 pt glyph, the title (SF Pro semibold 13, middle
  truncation), and the subline (Martian Mono 11, `text2`):

  | State | Subline | Trailing |
  |---|---|---|
  | Queued | `Up next · vimeo.com · Best · Polish` or `3rd in line · …` | drag handle on hover |
  | Pulling | `62% · 28.3 of 45.6 MB · 2.3 MB/s · 0:15 left` | the percent, Martian Mono Bold |
  | Pulling a playlist | `Video 3 of 12 · 2.3 MB/s` | the percent |
  | Polishing | `Polishing · merging audio and video` | — |
  | Kept | `Kept in Dawnload · 1 file · 45.6 MB` | Show in Finder on hover |
  | Failed | the error sentence, in `failed` | Retry on hover |
  | Paused | `Paused at 62% · 28.3 of 45.6 MB` | Resume on hover |
  | Seeding | `Seeding · ↑ 1.2 MB/s · ratio 0.84 · 12 peers` | Stop Seeding on hover |

  Hover actions are shortcuts for the context menu, never the only way: every
  one is also in the context menu and the Download menu.
- **Selection** is the system list selection with our look: `raised` fill and
  the spine. Double-click or Return opens a Kept file, or the inspector for
  anything else; Delete removes the row (and stops it, if it runs).

### Inspector

For the selected item, top to bottom:

1. **Status**: the 44 pt glyph, the state word, the title (selectable), the
   source in mono with Copy Link.
2. **Progress**: the percent in Unbounded while it pulls, the bytes, speed and
   time left; the bar is the D, not a second bar.
3. **Swarm** *(torrents)*: peers, seeds, download and upload speed, uploaded,
   ratio. Hidden for web items.
4. **Files**: every file the item made or will make, with sizes. Kept files
   reveal in Finder on click. For torrents, each file has a checkbox while the
   item can still change; unticked files are skipped (per-file selection).
5. **Options**: this item's engine, format and switches. Editable while it is
   Queued, Paused or Failed (then Retry uses them); read-only while it runs.
6. **Log**: the command that ran and the tool's output for this item, Martian
   Mono 10.5, selectable, with Copy Log. Log lines never appear anywhere else.
7. **Actions**: the one action that fits the state (Download Now, Pause,
   Resume, Retry, Show in Finder, Stop Seeding) and Remove.

With nothing selected: *Select a download to see its files, options and log.*

## States of the window

- **Empty, tools ready.** The 96 pt mark in `track` with a Signal arrow, then in
  Unbounded *Paste a link to start.*, then *Video, audio and galleries from the
  web, kept on your Mac.* and the two shortcuts that matter: ⇧⌘V Paste and
  Download, ⌥↩ Add to Queue.
- **Empty filter.** One quiet line: *Nothing kept yet.* / *Nothing failed.* /
  *Nothing waiting.* / *Nothing running.*
- **A tool is missing.** The empty state becomes a setup card: *Dawnload needs
  yt-dlp to download from the web.* (or ffmpeg, gallery-dl), the Homebrew
  command in mono with Copy, and Check Again (⌘R). With items in the list, the
  tools pill says what is missing and the detection line explains when the next
  download needs it.
- **First run.** A sheet, once: the mark and wordmark, *Keep what you find.* in
  Unbounded, then three short rows (the tools found on this Mac, each with a
  Kept or Failed glyph; where files go, with Change…; and *Dawnload is for
  media you have the right to keep. It runs yt-dlp, gallery-dl and ffmpeg on
  this Mac; nothing goes through a server.*), then Start.

## Settings

A native `Settings` window (⌘,) with grouped forms tinted with `link`:

- **General**: the download folder; the defaults for new downloads (format,
  engine, Polish, Playlist; the same values the chips set); Appearance (System,
  Dark, Light).
- **Access**: browser impersonation (default on or off, and the browser);
  cookies.txt (choose, replace, remove), with *Export cookies in Netscape format
  from a browser session you own. Dawnload passes the file to yt-dlp on this
  Mac and nowhere else.*; and the HTTP 403 steps.
- **Tools**: yt-dlp, gallery-dl, ffmpeg (and the torrent engine, once chosen):
  glyph, version, path or *Not found*, and the Homebrew command. Check Again.
- **Torrents** *(once an engine ships)*: seed until ratio, seed for at most,
  ask which files before downloading, listen port.

## Errors

An error says what happened, then what to do, in one or two sentences. The row
shows the first sentence; the inspector shows both and the log.

| Cause | What it says |
|---|---|
| HTTP 403 | This site refused the request (HTTP 403). Turn on browser impersonation, or add your cookies.txt in Settings. |
| Sign-in needed | This needs you to be signed in. Add your cookies.txt in Settings. |
| HTTP 404 | The page wasn't found (HTTP 404). Check the link. |
| Unsupported site | yt-dlp doesn't support this site. Try gallery-dl for galleries and image sets. |
| Tool missing | yt-dlp isn't installed. Install it with Homebrew, then check again. |
| Audio without ffmpeg | Audio only needs ffmpeg. Install ffmpeg, or choose Best. |
| Cookies unreadable | The cookies file can't be read any more. Choose it again in Settings. |
| Not a link | That isn't a full link. Paste one that starts with https://. |
| Torrent, no engine | Torrent downloads aren't built yet. |
| Anything else | yt-dlp stopped with an error (exit 1). The log has the details. |

## Torrents, designed ahead

Nothing below runs yet; it is what the model and screens already make room for.

- **Input.** Paste a magnet link into the same field; drop or open (⌘O) a
  `.torrent` file. Detection reads the magnet's name (`dn`) and info hash and
  says *Magnet link · Ubuntu 24.04 ISO*. `DownloadSource` in DawnloadCore
  parses all three sources and is tested.
- **Multi-file items.** An item has `files`, each with a path, a size, its own
  progress and `isSelected`. A torrent with one file looks like a web item; with
  more, the subline says `4 of 12 files · …` and the inspector lists them with
  checkboxes.
- **Swarm.** `SwarmStats`: peers, seeds, upload speed, uploaded bytes, ratio.
  Shown in the inspector's Swarm section and in the Seeding subline.
- **Seeding is a state after Kept.** The files are on disk (so the item counts
  as kept and Show in Finder works), and it keeps uploading until the ratio or
  time limit in Settings, or Stop Seeding. Then it becomes Kept. The Seeding
  filter tab appears.
- **Checking** (verifying pieces on resume) is shown as Pulling with the loader
  and the word *Checking*.
- **Lanes.** Each engine names a lane. The web lane runs one item at a time;
  the torrent lane runs several, beside it, so a long torrent never blocks a
  video.
- **Voice.** *Only download and seed torrents you have the right to share.
  Seeding uploads pieces of the files to other people.* on first torrent use.

In code: `DownloadEngine` (an engine declares its lane, its capabilities —
pause, seeding, file selection, swarm stats — and which sources it accepts)
and `DownloadJob` (one run of one item, emitting `JobEvent`s). `YTDlpEngine`
and `GalleryDlEngine` are the two engines today; `TorrentEngine` is a
placeholder that accepts magnets and `.torrent` files and reports itself
unavailable. A real engine replaces its `makeJob`.

## Motion

- **The pull.** Pressing Download pushes the arrow in the button down 3 pt and
  lets it spring back: 260 ms, `cubic-bezier(0.3, 0, 0, 1)` (`DLMotion.pull`).
- **Settle.** New rows arrive from above and fade in: 520 ms,
  `cubic-bezier(0.16, 1, 0.3, 1)` (`DLMotion.settle`).
- **The fill** follows real progress, easing linearly over 200 ms between
  samples. It never runs ahead of the tool and never loops.
- **The loader** (the arrow passing down through the D) is the only repeating
  motion, and only while the size is unknown.
- **State changes** cross-fade the cut in 180 ms. Kept has no confetti and
  Failed doesn't shake.
- **Reduce Motion**: no translation, no loader (a still Pulling glyph), no
  settle; cross-fades stay.

## Voice

Plain, short, specific. Verbs: **Paste, Download, Keep.** States: Queued,
Pulling, Polishing, Kept, Failed, Paused (and Seeding). Done says where the
file is: *Kept in Dawnload.* Empty states invite the one action. No hype, no
piracy words ("rip", "grab anything"), no exclamation marks, no emoji, and
never "Oops". Buttons are title case (Show in Finder, Start Queue), sentences
end with a full stop, and the app name is **Dawnload**.

## Accessibility

- Every glyph has the state word as its accessibility label; a row reads as one
  element: *Big Buck Bunny, Pulling, 62 percent, 15 seconds left*. Actions are
  exposed as accessibility actions (Show in Finder, Retry, Remove).
- Progress is an accessibility value, so VoiceOver can read it on request
  without announcing every sample.
- Every action has a menu command and most have a shortcut; nothing needs the
  pointer or hover.
- Contrast meets 4.5:1 for all text in both appearances; Increase Contrast
  strengthens outlines. Nothing depends on colour alone.
- Reduce Motion is honoured as above. There is no transparency to reduce.

## Keyboard

| Shortcut | Action |
|---|---|
| ⌘L | Focus the paste field |
| Return / ⌘D | Download (add and start) |
| ⌥Return / ⇧⌘A | Add to Queue (add, don't start) |
| ⇧⌘V | Paste and Download |
| ⇧⌘S | Start Queue |
| ⌘. | Pause All |
| ⌘O | Open Torrent File… *(once torrents ship)* |
| ⇧⌘O | Choose Download Folder… |
| ⌘R | Check Tools Again |
| ⇧⌘R | Show in Finder (selected) |
| ⌘Return | Retry or Resume (selected) |
| ⌘⌫ / Delete | Remove (selected) |
| ⌥⌘I | Show or hide the inspector |
| ⌘1 – ⌘5 | All, Active, Up next, Kept, Failed |
| ⌘, | Settings |

## macOS conventions

- The list is the system list (selection, keyboard, drag to reorder, context
  menus). Settings is a native `Settings` scene with forms. Panels are
  `NSOpenPanel`.
- The Dock icon shows a badge with the number of items active or waiting.
- The list is saved to `~/Library/Application Support/Dawnload/items.json`.
  Items that were running when the app quit come back Paused; yt-dlp and
  gallery-dl continue from their partial files when resumed.
- The app never asks for a network permission it doesn't need, and never
  phones home.

## Design review checklist

- Is Download the only Signal-filled control in view?
- Does every state have its glyph *and* its word? Does it read in light mode?
- Is every machine string (URL, path, size, speed, log) in Martian Mono, and is
  Unbounded used at most once?
- Does the error say what happened and what to do?
- Can the whole flow run from the keyboard? Does VoiceOver read each row as one
  sentence?
- With Reduce Motion on, is anything still moving that shouldn't?
- Would a torrent fit here without a new screen?

# 0010 · The desktop app's shell: one window, the tray, one instance, OS services

Status: Accepted (2026-11-13), for the owner to review

## Context

Phase 3 turns the Slint prototype (ADR 0008) into the app people run every day, Windows first,
then Linux, then macOS. DESIGN.md was written for the Mac (a native Settings window, the menu
bar, the Dock badge, Finder). Windows and Linux have none of those in the same shape, and Slint
draws everything itself, so each OS service (tray, notifications, file manager, single instance,
drag and drop, the title bar) needed a decision about how the app reaches it, with the targets
from ADR 0008 still in force: about 100 MB idle and smooth scrolling.

## Decision

- **One window, pages inside it.** Settings (General, Access, Tools, Torrents, About), the first-run
  card and the About card are pages over the main window, not windows of their own. A second
  Slint window would cost another renderer context (tens of MB on the OpenGL renderer), its own
  copy of the theme (Slint globals aren't shared between windows) and a second set of
  focus and close rules per OS. Esc or the back button returns to the list; Ctrl+, (⌘, on
  macOS) opens Settings, as DESIGN.md asks.
- **A ⋯ menu in the header, not a menu bar,** on Windows and Linux: Settings, Check Tools Again,
  Start Queue, Pause All, About, Quit. Every item also has a shortcut, a context-menu entry or
  a button. The native macOS menu bar (Slint's `MenuBar`) is left for macOS parity.
- **Menus are Slint's own** (`ContextMenuArea` with `Menu`): chip menus, row context menus, the
  queue's ⋯ and the header's ⋯. The current value carries a check *icon*, not a checkable item:
  a checkable `MenuItem` flips its own `checked` and loses its binding to the setting.
- **Preferences live in the core's `Settings`** (`AppPrefs`: appearance, motion, close to tray,
  notifications, first run, inspector; `tool_paths`; `cookies_file`), in SQLite with everything
  else, so a portable data folder carries them and the CLI could read them.
- **The tray is Slint's `SystemTrayIcon`** (Shell_NotifyIcon on Windows, StatusNotifierItem on
  Linux, NSStatusItem on macOS): Show Dawnload, Start Queue / Pause All / Resume All, Settings,
  About, Quit. The quiet badge is a second icon with a dot while something pulls or waits, and
  the tooltip says how many. **Closing the window keeps Dawnload in the tray** by default, where
  there is a tray: on Linux the app asks D-Bus for a StatusNotifierWatcher (KDE, and GNOME with
  the AppIndicator extension that Ubuntu ships) and, without one, closing quits and the setting
  says why.
- **One app per data folder, and a second launch hands over.** ADR 0004's lock stays the rule.
  The first app also listens on a local socket: a Unix socket in the data folder, or a named
  pipe named after a hash of it on Windows. A second `dawnload-app <link…>` sends one JSON line
  (`{"links": […], "show": true}`) and exits; the first adds the links and comes to the front.
  If the lock is held by something that doesn't listen (`dawnload run`), a small window says so.
  This is the seed of phase 6's local control API, not a replacement for it.
- **Notifications** on Kept and Failed (both on by default, each can be turned off): WinRT toasts
  on Windows, under an AppUserModelID registered in the user's own registry hive (HKCU, no
  installer needed); `org.freedesktop.Notifications` over D-Bus on Linux; AppleScript's
  `display notification` on macOS until the app is a signed bundle (phase 5). The demo engine
  never notifies.
- **Identity:** the app ID is `io.github.dawnwebs.Dawnload` (reverse DNS of where the project
  lives, Flathub's rule for GitHub projects). It is the Wayland app ID, Windows' AUMID and the
  desktop entry's name. On Linux the plain binary writes its own
  `~/.local/share/applications/io.github.dawnwebs.Dawnload.desktop` and icon (rewritten only when
  they differ; not inside Flatpak; `DAWNLOAD_NO_DESKTOP_ENTRY` turns it off), so the dock shows
  "Dawnload" and its icon. The Windows .exe carries its icon and version resource.
- **The winit window, through Slint's `unstable-winit-030` accessor,** for what Slint doesn't
  expose: files dropped on the window, and the title bar's theme (`set_theme`), so the in-app
  Light/Dark choice also colours Windows' title bar and the Wayland decorations. This pins us to
  winit 0.30's API; a Slint upgrade that moves winit means touching `main.rs` only.
- **Files: open and show.** Windows: `ShellExecuteW` and `explorer /select,`; Linux: `xdg-open`
  and `org.freedesktop.FileManager1.ShowItems`, falling back to opening the folder; macOS:
  `open` and `open -R`. Dialogs (folder, cookies.txt, a tool) are `rfd`'s native pickers (the XDG
  portal on Linux).
- **Drag and drop:** dropped `.torrent`, `.url`, `.webloc`, `.desktop` and `.txt` files put their
  links in the paste field (DESIGN.md: into the field, not straight into the queue). Waiting rows
  reorder by their drag handle or Alt+↑/↓.
- **Real core by default.** `dawnload-app` is the app; `--demo` or `DAWNLOAD_DEMO_ITEMS` runs the
  demo engine for development and the measurement scripts.

## Alternatives

- **Separate Settings and About windows**: closer to DESIGN.md's Mac wording, but more memory
  and per-OS window behaviour for no gain on Windows or Linux, where in-window settings are
  common. macOS parity can still give Settings its own window if the owner wants it.
- **`tray-icon` / `notify-rust` / `single-instance` crates**: each duplicates something already
  in the tree (Slint's tray, zbus, the `windows` crate, tokio's pipes) or pulls a second event
  loop; the hand-written pieces are small and tested where they can be.
- **A global `Ctrl+Shift+V` hotkey, a browser extension, `magnet:` registration**: later phases
  (torrents in phase 4, the extension in phase 6). Registering as the magnet handler before
  torrents work would only produce "not built yet".

## Consequences

- Links dragged from a browser don't arrive: winit 0.30 reports dropped *files* only, and on
  Wayland not even those. Pasting works everywhere; dropping a link file or a `.torrent` works on
  Windows, X11 and macOS.
- On Windows the AUMID registration writes two values under
  `HKCU\Software\Classes\AppUserModelId\io.github.dawnwebs.Dawnload`; an installer (phase 5)
  should own them and remove them on uninstall.
- The plain Linux binary writes a desktop entry pointing at wherever it was run from; packages
  (Flathub, AppImage) will ship their own.

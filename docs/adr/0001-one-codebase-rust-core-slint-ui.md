# 0001 · One codebase: a Rust core and a Slint UI

Status: Accepted (2026-11-09)

## Context

Dawnload is a SwiftUI app for macOS. We expect the larger audience on Windows and Linux, and we
want one product everywhere: one queue, one design, the same behaviour. A downloader is mostly
engine (processes, HTTP, files, state, later BitTorrent) under a modest UI that must stay light
enough to leave running in the tray all day.

## Decision

- **`dawnload-core` (Rust)** holds everything except pixels: sources, engines, the tool manager,
  the queue and scheduler, persistence and the UI-facing API. It's a library with no UI
  dependency, so the app, the `dawnload` CLI, tests, and later a local remote-control API and
  the browser extension's native host all use the same code.
- **`dawnload-ui` (Slint)** draws the whole window with one toolkit on every OS (the
  qBittorrent approach), compiled to native code with no webview and no JavaScript runtime.
- **`dawnload-cli`** is a thin client of the core; it doubles as the scriptable interface and as a
  test bed that proves the API is complete without a GUI.
- The Cargo workspace sits at the repo root beside the Swift package. The Swift app stays as it
  is on macOS until the Rust app matches it (roadmap phase 0 and 3); nothing under `Sources/`,
  `Tests/` or `Package.swift` depends on or is touched by the Rust code.
- Tauri stays the fallback if Slint fails a real-world test (accessibility, IME, platform
  integration); because the core is a library with a serializable API (ADR 0002), switching UI
  toolkits would replace one crate, not the product.

## Alternatives

- **Keep native per OS** (SwiftUI + WinUI + GTK): best platform feel, three UIs to build and keep
  in sync for a small team. Rejected on cost.
- **Tauri / Electron**: fastest UI iteration with web skills; a webview per window means ~150–300
  MB on Windows (WebView2) and a second runtime to secure and update. Fallback only.
- **Qt (C++ or via bindings)**: mature and accessible; LGPL obligations, a heavy C++ toolchain and
  weak Rust bindings.
- **egui / iced**: pure Rust and light; immediate-mode or young retained toolkits with less
  polish for text input, accessibility and theming than Slint today.
- **Flutter**: good UI velocity; Dart beside Rust means FFI for everything and a bigger runtime.

## Consequences

- Most of the product's logic is now testable without a screen (59 tests at the time of writing).
- UI work happens in `.slint` markup with Rust glue; designers can read the markup.
- Platform polish (tray, notifications, file associations, drag and drop, the macOS menu bar)
  is ours to add per OS in phase 3; Slint covers some of it (it ships a system-tray feature).
- Slint's licence is not MIT; see ADR 0009.

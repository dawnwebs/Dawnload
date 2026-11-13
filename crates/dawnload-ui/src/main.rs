//! Dawnload's desktop app. Slint draws; dawnload-core owns the state.
//!
//! Threads: Slint's event loop on the main thread; the core and the bridge (src/bridge.rs) on a
//! small tokio runtime. The bridge turns the core's batched diffs into what changed on screen and
//! hands only that to the UI thread; the UI thread never formats a byte count.
//!
//! `dawnload-app [link…]` runs the real core on the person's data folder. One app runs per data
//! folder: a second launch hands its links to the first and exits (dawnload_core::instance).
//! `--demo` (or DAWNLOAD_DEMO_ITEMS) runs the demo engine for development and measurement; see
//! src/demo.rs. Review switches: DAWNLOAD_UI_THEME=light|dark, DAWNLOAD_UI_SELECT_FIRST=1,
//! DAWNLOAD_UI_PAGE and DAWNLOAD_UI_TAB.

// Release builds on Windows are GUI apps, so no console window opens behind Dawnload. Redirected
// stderr (the measurement scripts) still works; debug builds keep the console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod bridge;
mod demo;
mod platform;
mod view;

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dawnload_core::core::{Core, CoreConfig, CoreError};
use dawnload_core::instance::{self, Handoff};
use dawnload_core::model::ItemId;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;

use bridge::{Bridge, Frame, UiMessage};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo_items: Option<usize> = std::env::var("DAWNLOAD_DEMO_ITEMS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or_else(|| args.iter().any(|a| a == "--demo").then_some(500));
    let real = demo_items.is_none();
    let links = instance::links_from_args(args.iter().filter(|a| *a != "--real" && *a != "--demo").cloned());
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let _enter = runtime.enter();
    if let Some(note) = dawnload_core::paths::migrate_legacy_data_dir() {
        eprintln!("dawnload-app: {note}");
    }
    let data_dir = dawnload_core::paths::data_dir();

    // One app per data folder: hand what we were given to the running one.
    let handoff = Handoff { links, show: true };
    if real && runtime.block_on(instance::send(&data_dir, &handoff)).is_ok() {
        return Ok(());
    }

    let core = if real {
        let mut started = Core::start(CoreConfig::new(data_dir.clone()));
        // Two launches at once: the other one may still be setting up its listener.
        for _ in 0..10 {
            let Err(CoreError::AlreadyRunning(_)) = &started else { break };
            std::thread::sleep(Duration::from_millis(200));
            if runtime.block_on(instance::send(&data_dir, &handoff)).is_ok() {
                return Ok(());
            }
            started = Core::start(CoreConfig::new(data_dir.clone()));
        }
        match started {
            Ok(core) => core,
            Err(e) => return show_error(&e.to_string()),
        }
    } else {
        Core::start(CoreConfig::demo())?
    };
    let handoffs = if real { instance::listen(&data_dir).ok() } else { None };

    platform::init_identity(&data_dir);
    let app = AppWindow::new()?;
    platform::set_app_id();
    let (mod_key, reveal, computer) = platform::words();
    let theme = app.global::<Theme>();
    theme.set_mod_key(mod_key.into());
    theme.set_reveal(reveal.into());
    theme.set_computer(computer.into());
    theme.set_no_preference_is_light(cfg!(target_os = "linux"));
    app.set_version(env!("CARGO_PKG_VERSION").into());
    app.set_mode_note(match demo_items {
        Some(n) => format!("demo · {n} items").into(),
        None => SharedString::new(),
    });
    let rows = Rc::new(VecModel::<RowData>::default());
    app.set_rows(ModelRc::from(rows.clone()));
    app.set_filters(ModelRc::from(Rc::new(VecModel::<FilterTab>::default())));
    app.set_inspector_files(ModelRc::from(Rc::new(VecModel::<FileRow>::default())));
    app.set_tools(ModelRc::from(Rc::new(VecModel::<ToolRow>::default())));

    let counters = Arc::new(demo::Counters::default());
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiMessage>();
    wire_callbacks(&app, &ui_tx);

    // The tray, where the desktop has one.
    let tray_available = platform::tray_available();
    let tray = if tray_available { Tray::new().ok() } else { None };
    if let Some(tray) = &tray {
        let weak = app.as_weak();
        tray.on_show_window(move || show_window(&weak));
        let tx = ui_tx.clone();
        tray.on_queue(move || drop(tx.send(UiMessage::QueueAction)));
        let weak = app.as_weak();
        tray.on_settings(move || {
            show_window(&weak);
            if let Some(app) = weak.upgrade() {
                app.set_page(1);
            }
        });
        let weak = app.as_weak();
        tray.on_about(move || {
            show_window(&weak);
            if let Some(app) = weak.upgrade() {
                app.set_page(3);
            }
        });
        tray.on_quit(|| drop(slint::quit_event_loop()));
    }

    // Closing the window keeps Dawnload in the tray, when there is one and that's the setting.
    let close_to_tray = Arc::new(AtomicBool::new(false));
    {
        let close_to_tray = close_to_tray.clone();
        app.window().on_close_requested(move || {
            if close_to_tray.load(Ordering::Relaxed) {
                CloseRequestResponse::HideWindow
            } else {
                let _ = slint::quit_event_loop();
                CloseRequestResponse::HideWindow
            }
        });
    }

    // Files dragged over and dropped on the window (winit reports files, not text).
    {
        let tx = ui_tx.clone();
        let weak = app.as_weak();
        app.window().on_winit_window_event(move |_, event| {
            let hover = |on: bool| {
                if let Some(app) = weak.upgrade() {
                    app.set_drop_hover(on);
                }
            };
            match event {
                winit::event::WindowEvent::HoveredFile(_) => hover(true),
                winit::event::WindowEvent::HoveredFileCancelled => hover(false),
                winit::event::WindowEvent::DroppedFile(path) => {
                    hover(false);
                    let _ = tx.send(UiMessage::Dropped(path.clone()));
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    // Bridge → UI: apply frames on the UI thread.
    let apply = {
        let weak = app.as_weak();
        let tray = tray.as_ref().map(|t| t.as_weak());
        let counters = counters.clone();
        let close_to_tray = close_to_tray.clone();
        move |frame: Frame| {
            let weak = weak.clone();
            let tray = tray.clone();
            let counters = counters.clone();
            let close_to_tray = close_to_tray.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                apply_frame(&app, tray.and_then(|t| t.upgrade()).as_ref(), frame, &counters, &close_to_tray);
            });
        }
    };
    let bridge = Bridge { core: core.clone(), real, tray_available, own: ui_tx.clone() };
    runtime.spawn(bridge.run(ui_rx, apply));
    if let Some(mut handoffs) = handoffs {
        let tx = ui_tx.clone();
        runtime.spawn(async move {
            while let Some(handoff) = handoffs.recv().await {
                let _ = tx.send(UiMessage::Handoff(handoff));
            }
        });
    }
    if !handoff.links.is_empty() {
        let _ = ui_tx.send(UiMessage::Handoff(handoff));
    }
    if let Some(count) = demo_items {
        runtime.spawn(demo::demo_driver(core.clone(), count));
    }

    match std::env::var("DAWNLOAD_UI_THEME").as_deref() {
        Ok("light") => theme.set_mode(2),
        Ok("dark") => theme.set_mode(1),
        _ => {}
    }
    app.set_ease_rows(std::env::var("DAWNLOAD_UI_EASE_ROWS").is_ok());
    // Review switches: DAWNLOAD_UI_PAGE=1|2|3 opens Settings, first run or About after the bridge
    // has set things up; DAWNLOAD_UI_TAB picks the Settings tab.
    if let Some(page) = std::env::var("DAWNLOAD_UI_PAGE").ok().and_then(|v| v.parse::<i32>().ok()) {
        let tab = std::env::var("DAWNLOAD_UI_TAB").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        let weak = app.as_weak();
        slint::Timer::single_shot(Duration::from_millis(800), move || {
            if let Some(app) = weak.upgrade() {
                app.set_settings_tab(tab);
                app.set_page(page);
            }
        });
    }
    if std::env::var("DAWNLOAD_UI_SELECT_FIRST").is_ok() {
        let tx = ui_tx.clone();
        slint::Timer::single_shot(Duration::from_millis(1500), move || {
            let _ = tx.send(UiMessage::MoveSelection(1));
        });
    }
    demo::install_measurement(&app, counters.clone());

    app.show()?;
    slint::run_event_loop_until_quit()?;
    let _ = app.hide();
    drop(tray);
    runtime.block_on(core.shutdown());
    Ok(())
}

fn wire_callbacks(app: &AppWindow, ui_tx: &mpsc::UnboundedSender<UiMessage>) {
    let tx = ui_tx.clone();
    app.on_paste_edited(move |text| drop(tx.send(UiMessage::Paste(text.to_string()))));
    let tx = ui_tx.clone();
    app.on_submit(move |text, start| drop(tx.send(UiMessage::Submit { text: text.to_string(), start })));
    let tx = ui_tx.clone();
    app.on_set_option(move |key, value| drop(tx.send(UiMessage::SetOption(key.to_string(), value))));
    let tx = ui_tx.clone();
    app.on_command(move |name| {
        if name == "quit" {
            let _ = slint::quit_event_loop();
        } else {
            let _ = tx.send(UiMessage::Command(name.to_string()));
        }
    });
    let tx = ui_tx.clone();
    app.on_tool_action(move |tool, action| {
        drop(tx.send(UiMessage::ToolAction { tool: tool.to_string(), action: action.to_string() }))
    });
    let tx = ui_tx.clone();
    app.on_select_filter(move |i| drop(tx.send(UiMessage::Filter(i.max(0) as usize))));
    let tx = ui_tx.clone();
    app.on_select(move |id| drop(tx.send(UiMessage::Select((id >= 0).then_some(ItemId(id as u64))))));
    let tx = ui_tx.clone();
    app.on_move_selection(move |d| drop(tx.send(UiMessage::MoveSelection(d))));
    let tx = ui_tx.clone();
    app.on_item_action(move |id, action| drop(tx.send(UiMessage::Item { id: ItemId(id as u64), action: action.to_string() })));
    let tx = ui_tx.clone();
    app.on_reorder(move |id, delta| drop(tx.send(UiMessage::Reorder { id: ItemId(id as u64), delta })));
    let tx = ui_tx.clone();
    app.on_queue_action(move || drop(tx.send(UiMessage::QueueAction)));
    let tx = ui_tx.clone();
    app.on_inspector_toggled(move |on| drop(tx.send(UiMessage::InspectorToggled(on))));
}

fn show_window(weak: &slint::Weak<AppWindow>) {
    let Some(app) = weak.upgrade() else { return };
    let _ = app.show();
    app.window().set_minimized(false);
    app.window().with_winit_window(|w| w.focus_window());
}

/// The title bar follows the in-app appearance where the platform lets it (Windows' immersive
/// dark mode, the Wayland decorations, macOS' appearance).
fn set_title_bar_theme(app: &AppWindow, mode: i32) {
    let theme = match mode {
        1 => Some(winit::window::Theme::Dark),
        2 => Some(winit::window::Theme::Light),
        _ => None,
    };
    app.window().with_winit_window(|w| w.set_theme(theme));
}

fn apply_frame(app: &AppWindow, tray: Option<&Tray>, frame: Frame, counters: &demo::Counters, close_to_tray: &AtomicBool) {
    let rows_model = app.get_rows();
    let rows = rows_model.as_any().downcast_ref::<VecModel<RowData>>().expect("rows model");
    if let Some(len) = frame.resize {
        let current = rows.row_count();
        if len > current {
            for _ in current..len {
                rows.push(RowData::default());
            }
        } else {
            for index in (len..current).rev() {
                rows.remove(index);
            }
        }
        counters.resets.fetch_add(1, Ordering::Relaxed);
    }
    for (index, row) in frame.patch {
        if index < rows.row_count() {
            rows.set_row_data(index, row);
            counters.rows_patched.fetch_add(1, Ordering::Relaxed);
        }
    }
    counters.batches.fetch_add(1, Ordering::Relaxed);
    if let Some(tabs) = frame.filters {
        set_vec(&app.get_filters(), tabs);
    }
    if let Some(label) = frame.queue_label {
        app.set_queue_label(label.into());
    }
    if let Some(line) = frame.empty_line {
        app.set_empty_line(line.into());
    }
    if let Some((data, files)) = frame.inspector {
        app.set_inspector(data);
        set_vec(&app.get_inspector_files(), files);
    }
    if let Some(id) = frame.selected {
        app.set_selected_id(id);
    }
    if let Some((text, tone, kind, can_submit)) = frame.detection {
        app.set_detection(text.into());
        app.set_detection_tone(tone);
        app.set_detection_kind(kind);
        app.set_can_submit(can_submit);
    }
    if let Some(text) = frame.paste_text {
        app.set_paste_text(text.into());
    }
    if let Some(chips) = frame.chips {
        app.set_chips(chips);
    }
    if let Some(prefs) = frame.prefs {
        app.set_prefs(prefs);
    }
    if let Some((text, ok)) = frame.tools_pill {
        app.set_tools_text(text.into());
        app.set_tools_ok(ok);
    }
    if let Some(tools) = frame.tools {
        set_vec(&app.get_tools(), tools);
    }
    if let Some(name) = frame.missing_tool {
        app.set_missing_tool(name.into());
    }
    if let Some(busy) = frame.tools_busy {
        app.set_tools_busy(busy);
    }
    if let Some(note) = frame.tools_note {
        app.set_tools_note(note.into());
    }
    if let Some((line, ok)) = frame.js {
        app.set_js_line(line.into());
        app.set_js_ok(ok);
    }
    if let Some(text) = frame.copy {
        app.invoke_copy_text(text.into());
    }
    if let Some(page) = frame.page {
        app.set_page(page);
    }
    if let Some((mode, reduce)) = frame.look {
        // DAWNLOAD_UI_THEME (review) wins over the setting.
        let mode = match std::env::var("DAWNLOAD_UI_THEME").as_deref() {
            Ok("dark") => 1,
            Ok("light") => 2,
            _ => mode,
        };
        app.global::<Theme>().set_mode(mode);
        app.global::<Theme>().set_reduce_motion(reduce);
        set_title_bar_theme(app, mode);
    }
    if let Some(on) = frame.close_to_tray {
        close_to_tray.store(on && tray.is_some(), Ordering::Relaxed);
    }
    if let Some(on) = frame.show_inspector {
        app.set_show_inspector(on);
    }
    if let (Some(state), Some(tray)) = (frame.tray, tray) {
        tray.set_busy(state.busy);
        tray.set_queue_title(state.queue_title.into());
        tray.set_queue_enabled(state.queue_enabled);
        tray.set_tip(state.tooltip.into());
    }
    if frame.show_window {
        show_window(&app.as_weak());
    }
    if !frame.notify.is_empty() {
        NOTIFIER.with(|n| {
            let notifier = n.get_or_init(|| platform::Notifier::new(&dawnload_core::paths::data_dir()));
            for (title, body) in frame.notify {
                notifier.notify(title, body);
            }
        });
    }
    if let Some(text) = frame.notice {
        app.set_notice(text.clone().into());
        let weak = app.as_weak();
        slint::Timer::single_shot(Duration::from_secs(5), move || {
            if let Some(app) = weak.upgrade()
                && app.get_notice() == text.as_str()
            {
                app.set_notice(SharedString::new());
            }
        });
    }
}

thread_local! {
    static NOTIFIER: std::cell::OnceCell<platform::Notifier> = const { std::cell::OnceCell::new() };
}

/// Replaces a VecModel's rows, touching only rows that changed.
fn set_vec<T: Clone + PartialEq + 'static>(model: &ModelRc<T>, items: Vec<T>) {
    let Some(vec) = model.as_any().downcast_ref::<VecModel<T>>() else { return };
    if vec.row_count() != items.len() {
        vec.set_vec(items);
    } else {
        for (i, item) in items.into_iter().enumerate() {
            if vec.row_data(i).as_ref() != Some(&item) {
                vec.set_row_data(i, item);
            }
        }
    }
}

fn show_error(message: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("dawnload-app: {message}");
    let window = MessageWindow::new()?;
    window.set_message(message.into());
    window.on_dismiss(|| drop(slint::quit_event_loop()));
    window.run()?;
    Ok(())
}

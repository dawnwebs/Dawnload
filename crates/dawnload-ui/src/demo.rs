//! The demo engine's driver and the measurement switches (Scripts/measure-ui.sh, .ps1).
//!
//! `dawnload-app --demo` (or any DAWNLOAD_DEMO_ITEMS) runs the window on an in-memory core with
//! the demo engine: `DAWNLOAD_DEMO_ITEMS=<n>` items (500 by default), a tenth of them torrents
//! that seed. DAWNLOAD_UI_STATS=1 prints frames and updates per second; DAWNLOAD_UI_AUTOSCROLL=<px
//! per frame> scrolls the list up and down forever; DAWNLOAD_UI_EXIT_AFTER=<seconds> quits.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dawnload_core::api::Action;
use dawnload_core::core::CoreHandle;
use dawnload_core::engine::Lane;
use dawnload_core::model::{ItemId, Status};
use slint::{ComponentHandle, Model};

use crate::AppWindow;

#[derive(Default)]
pub struct Counters {
    pub frames: AtomicU64,
    pub batches: AtomicU64,
    pub rows_patched: AtomicU64,
    pub resets: AtomicU64,
}

/// Keeps ~`count` demo items alive: web items, a tenth of them torrents that seed; finished ones
/// are cleared and replaced, so the list keeps moving.
pub async fn demo_driver(core: CoreHandle, count: usize) {
    let mut settings = core.settings().await.unwrap_or_default();
    settings.lane_limits.insert(Lane::Demo, (count / 2).max(1));
    settings.defaults.seed_ratio_limit = Some(1.5);
    core.set_settings(settings).await;
    let mut serial = 0u64;
    let mut add = |n: usize| {
        let core = core.clone();
        let start = serial;
        serial += n as u64;
        async move {
            for i in start..start + n as u64 {
                let text = if i % 10 == 3 {
                    let hash = format!("{:040x}", (i as u128 + 1) * 0x9E37_79B9_7F4A_7C15_u128);
                    format!("magnet:?xt=urn:btih:{}&dn=Open+Movie+Collection+{i}", &hash[hash.len() - 40..])
                } else {
                    format!("https://media.example.org/watch/{i}")
                };
                let _ = core.add(&text, None, true).await;
            }
        }
    };
    add(count).await;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(snapshot) = core.snapshot().await else { return };
        let mut finished: Vec<_> = snapshot.items.iter().filter(|i| matches!(i.status, Status::Kept | Status::Failed)).collect();
        if finished.len() > count / 4 {
            finished.sort_by_key(|i| i.finished_at);
            let ids: Vec<ItemId> = finished.iter().take(20).map(|i| i.id).collect();
            let n = ids.len();
            let _ = core.act(Action::Remove { ids }).await;
            add(n).await;
        }
    }
}

/// FPS counting, auto-scroll and auto-exit for Scripts/measure-ui.sh.
pub fn install_measurement(app: &AppWindow, counters: Arc<Counters>) {
    let stats = std::env::var("DAWNLOAD_UI_STATS").is_ok();
    let autoscroll: Option<f32> = std::env::var("DAWNLOAD_UI_AUTOSCROLL").ok().and_then(|v| v.parse().ok());
    let exit_after: Option<u64> = std::env::var("DAWNLOAD_UI_EXIT_AFTER").ok().and_then(|v| v.parse().ok());

    let frame_counter = counters.clone();
    let _ = app.window().set_rendering_notifier(move |state, _| {
        if let slint::RenderingState::AfterRendering = state {
            frame_counter.frames.fetch_add(1, Ordering::Relaxed);
        }
    });

    thread_local! {
        static TIMERS: RefCell<Vec<slint::Timer>> = const { RefCell::new(Vec::new()) };
    }
    if stats {
        let timer = slint::Timer::default();
        let mut last = (0u64, 0u64, 0u64, 0u64);
        let started = std::time::Instant::now();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let now = (
                counters.frames.load(Ordering::Relaxed),
                counters.batches.load(Ordering::Relaxed),
                counters.rows_patched.load(Ordering::Relaxed),
                counters.resets.load(Ordering::Relaxed),
            );
            eprintln!(
                "stats t={:.0}s fps={} frames_applied={} rows_patched={} list_resizes={}",
                started.elapsed().as_secs_f64(),
                now.0 - last.0,
                now.1 - last.1,
                now.2 - last.2,
                now.3 - last.3
            );
            last = now;
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
    if let Some(speed) = autoscroll {
        let weak = app.as_weak();
        let direction = Rc::new(RefCell::new(1.0f32));
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
            let Some(app) = weak.upgrade() else { return };
            // viewport-y runs from 0 down to -(content - visible).
            let rows = app.get_rows().row_count() as f32;
            let max = (rows * 56.0 - app.get_list_height()).max(0.0);
            let mut dir = direction.borrow_mut();
            let mut y = -app.get_list_scroll() + speed * *dir;
            if y >= max {
                y = max;
                *dir = -1.0;
            } else if y <= 0.0 {
                y = 0.0;
                *dir = 1.0;
            }
            app.set_list_scroll(-y);
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
    if let Some(seconds) = exit_after {
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::SingleShot, Duration::from_secs(seconds), || {
            let _ = slint::quit_event_loop();
        });
        TIMERS.with(|t| t.borrow_mut().push(timer));
    }
}

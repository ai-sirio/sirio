//! Probe for #147: the blocking `evaluate_script` path off Linux.
//!
//! Opens the browser surface, waits for the page to settle, then calls
//! `BrowserSurface::evaluate_script` from the main thread and reports how
//! long it took and what it returned.
//!
//! **Read the result carefully.** This drives the *blocking* helper, which
//! is exactly the path #147 diagnosed and which still burns its full timeout
//! off Linux — that is expected, not a regression. Run on Windows at
//! `2de17bd2` it reports:
//!
//! ```text
//! PROBE script="document.title = 'PROBE_RAN_OK'; 1+1" elapsed_ms=5004 result=Err("evaluate_script timed out")
//! ```
//!
//! The shipped path is `evaluate_script_async`, added by #142, which is what
//! `browser.eval` dispatches through (`tiller/src/main.rs`). A probe for
//! *that* would have to drive the event channel the async call answers on,
//! and does not exist yet.
//!
//! So this example documents the defect and the shape of its evidence; it is
//! not a regression test for the fix.

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use std::time::{Duration, Instant};
use tiller_theme::Theme;

#[path = "../src/browser.rs"]
mod browser;

use browser::BrowserSurface;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| BrowserSurface::new("https://example.com", window, cx)),
            )
            .expect("open browser eval probe window");
        cx.activate(true);

        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(12))
                .await;
            let _ = handle.update(cx, |surface, _window, _cx| {
                for script in ["document.title = 'PROBE_RAN_OK'; 1+1", "document.title"] {
                    let started = Instant::now();
                    let result = surface.evaluate_script(script, Duration::from_secs(5));
                    println!(
                        "PROBE script={script:?} elapsed_ms={} result={result:?}",
                        started.elapsed().as_millis()
                    );
                }
            });
        })
        .detach();
    });
}

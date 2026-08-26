//! Probe for #147: does `evaluate_script` deadlock off Linux?
//!
//! Opens the browser surface, waits for the page to settle, then calls
//! `BrowserSurface::evaluate_script` from the main thread and reports how
//! long it took and what it returned. A deadlock shows up as an elapsed
//! time equal to the timeout plus a `timed out` error.

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

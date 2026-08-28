//! Run the P72 feasibility spike on a real X11 display.

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use sirio_theme::Theme;

#[path = "../src/browser.rs"]
mod browser;

use browser::BrowserSpike;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1434.0), px(780.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| BrowserSpike::new(window, cx)),
        )
        .expect("open P72 browser spike window");
        cx.activate(true);
    });
}

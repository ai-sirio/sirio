//! Run the B-02 browser surface on a real X11 display.

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use tiller_theme::Theme;

#[path = "../src/browser.rs"]
mod browser;

use browser::BrowserSurface;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| BrowserSurface::new("https://example.com", window, cx)),
        )
        .expect("open B-02 browser surface window");
        cx.activate(true);
    });
}

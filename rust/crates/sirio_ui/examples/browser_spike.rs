//! Run the P72 feasibility spike on a real X11 display.

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use sirio_ui::app_platform::application;
use sirio_theme::Theme;

#[path = "../src/browser.rs"]
mod browser;

// `browser.rs` pumps GTK through `crate::mcp_widget::pump_gtk`, and here the crate root is this
// example, so the library's function is re-exported under that path.
#[cfg(target_os = "linux")]
mod mcp_widget {
    pub use sirio_ui::mcp_widget::pump_gtk;
}

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

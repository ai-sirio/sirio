//! Dev harness: renders the full-width Changes tab as a standalone window,
//! so the surface can be captured and judged without the shell wiring.
//! Run: cargo run -p sirio_ui --bin changes_preview -- <repo-root>

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use sirio_theme::Theme;
use sirio_ui::changes::ChangesTab;

fn main() {
    sirio_ui::app_platform::application().run(|cx: &mut App| {
        Theme::init(cx);
        // Evidence for the fonts pass: the families are resolved once at
        // runtime from what is installed, never hard-coded (P16).
        eprintln!(
            "ui family: {}\nmono family: {}",
            Theme::get(cx).typography.ui_family,
            Theme::get(cx).typography.code_family
        );
        let cwd = std::env::args()
            .nth(1)
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(std::env::temp_dir);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // The focused window keeps its full frame rate under power-saving profiles, as on
                // bezel 0.3.8; gpui-pre's inactive (30 fps) and thermal (60 fps) limits are accepted.
                low_power_frame_interval: None,
                ..Default::default()
            },
            |_, cx| cx.new(|cx| ChangesTab::new(cwd, cx)),
        )
        .expect("failed to open the changes preview window");
    });
}

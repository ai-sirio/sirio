use std::path::PathBuf;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use gpui_platform::application;
use tiller_theme::Theme;
use tiller_ui::right_panel::RightPanel;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let root = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
            .canonicalize()
            .expect("resolve checkout root");
        let bounds = Bounds::centered(None, size(px(405.0), px(833.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: false,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| RightPanel::new(root.clone())),
        )
        .expect("open right panel demo window");
        cx.activate(true);
    });
}

use std::path::PathBuf;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use gpui_platform::application;
use sirio_terminal::TerminalView;
use sirio_theme::{Theme, ThemeMode};

fn main() {
    let working_directory = std::env::current_dir().expect("current directory");
    application().run(move |cx: &mut App| {
        Theme::init(cx);
        if std::env::var("SIRIO_THEME").as_deref() == Ok("light") {
            Theme::set_mode(ThemeMode::Light, cx);
        }
        let bounds = Bounds::centered(None, size(px(1100.), px(700.)), cx);
        let cwd: PathBuf = working_directory.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(12.))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| {
                let cwd = cwd.clone();
                cx.new(|cx| TerminalView::new(cwd, cx).expect("start terminal"))
            },
        )
        .expect("open terminal window");
        cx.activate(true);
    });
}

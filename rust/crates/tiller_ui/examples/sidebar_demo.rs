use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use gpui_platform::application;
use tiller_theme::Theme;
use tiller_ui::sidebar::Sidebar;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(325.0), px(833.0)), cx);
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
            |_, cx| cx.new(|cx| Sidebar::new_for_demo(cx)),
        )
        .expect("open sidebar demo window");
        cx.activate(true);
    });
}

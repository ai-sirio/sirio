use gpui::{
    App, AppContext, Bounds, Context, Entity, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, point, prelude::*, px, size,
};
use gpui_platform::application;
use sirio_theme::{Theme, ThemeMode};
use sirio_ui::settings::Settings;

struct SettingsDemo {
    settings: Entity<Settings>,
}

impl Render for SettingsDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(div().h(px(32.0)).flex_none().bg(theme.background))
            .child(div().flex_1().child(self.settings.clone()))
    }
}

fn main() {
    application().run(|cx: &mut App| {
        Theme::install(ThemeMode::Dark, cx);
        let bounds = Bounds::centered(None, size(px(1470.0), px(833.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| {
                let settings = cx.new(Settings::new);
                cx.new(|_| SettingsDemo { settings })
            },
        )
        .expect("open settings demo window");
        cx.activate(true);
    });
}

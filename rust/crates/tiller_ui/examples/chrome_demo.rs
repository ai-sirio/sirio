use gpui::{
    App, AppContext, Bounds, Context, Entity, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, point, prelude::*, px, size,
};
use gpui_platform::application;
use tiller_theme::Theme;
use tiller_ui::sidebar::icons::TillerAssets;
use tiller_ui::{status_bar::StatusBar, tab_bar::TabBar, titlebar::Titlebar};

struct ChromeDemo {
    titlebar: Entity<Titlebar>,
    tabbar: Entity<TabBar>,
    statusbar: Entity<StatusBar>,
}

impl Render for ChromeDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let canvas = gpui::rgb(0x131417);
        let background = gpui::rgb(0x1b1c1f);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(canvas)
            .child(
                div()
                    .h(px(32.0))
                    .w_full()
                    .bg(canvas)
                    .child(div().mt(px(2.0)).child(self.titlebar.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .child(div().w(px(325.0)).bg(canvas))
                    .child(div().w(px(1.0)).bg(gpui::black()))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(self.tabbar.clone())
                            .child(div().flex_1().bg(background)),
                    )
                    .child(div().w(px(1.0)).bg(gpui::black()))
                    .child(div().w(px(405.0)).bg(background)),
            )
            .child(self.statusbar.clone())
    }
}

fn main() {
    application().with_assets(TillerAssets).run(|cx: &mut App| {
        Theme::init(cx);
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
                let titlebar = cx.new(Titlebar::new);
                let tabbar = cx.new(|cx| {
                    TabBar::new(cx).on_new_tab(|action| {
                        println!("new-tab action: {action:?}");
                    })
                });
                let statusbar = cx.new(|_| {
                    StatusBar::new_with_default_context()
                        .on_settings(|| println!("settings"))
                        .on_refresh(|| println!("refresh"))
                });
                cx.new(|_| ChromeDemo {
                    titlebar,
                    tabbar,
                    statusbar,
                })
            },
        )
        .expect("open chrome demo window");
        cx.activate(true);
    });
}

use gpui::{
    App, AppContext, Bounds, Context, Entity, Render, Window, WindowBounds, WindowDecorations,
    WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;
use tiller_theme::{Theme, ThemeMode};
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
            .child(self.titlebar.clone())
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
        // COSMIC-02: `Theme` now carries the COSMIC token layer directly, so
        // forcing `TILLER_COSMIC_MODE=light|dark` for screenshot capture
        // forces the whole theme — waku colors and COSMIC tokens resolve
        // together, the same as every other surface. Unset follows the
        // system portal like the real app shell does. Installed before
        // `Titlebar::new` runs so its own lazy `Theme::init` bootstrap sees
        // a global already present and leaves this choice alone.
        match std::env::var("TILLER_COSMIC_MODE").as_deref() {
            Ok("light") => Theme::install(ThemeMode::Light, cx),
            Ok("dark") => Theme::install(ThemeMode::Dark, cx),
            _ => Theme::init(cx),
        }
        let bounds = Bounds::centered(None, size(px(1470.0), px(833.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // P76: no `TitlebarOptions`/`traffic_light_position` — that
                // API asks the OS to draw macOS-style lights, and neither
                // comet nor Tiller has OS-drawn lights available on Linux.
                // `titlebar: None` plus an explicit `Client` decoration
                // request is the bare, X11-realistic window: nothing but
                // our own drawn `Titlebar`, traffic lights included.
                titlebar: None,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_, cx| {
                let titlebar = cx.new(|cx| {
                    Titlebar::new(cx)
                        .with_title("Tiller")
                        .with_subtitle("tiller-linux @ linux/gpui-waku")
                });
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

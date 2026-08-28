//! Opens the chat surface wired to a real ACP agent.
//!
//! Agent defaults to the same `npx` command as `sirio_acp`'s smoke test;
//! override with `TILLER_ACP_PROGRAM=/path/to/agent`. The window is centered
//! unless `CHAT_DEMO_AT=x,y` places it at explicit screen coordinates (handy
//! for driving the demo with cliclick on a busy screen).

use gpui::{App, Bounds, Point, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_platform::application;
use sirio_theme::Theme;
use sirio_ui::chat::Chat;

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);

        let bounds = match std::env::var("CHAT_DEMO_AT") {
            Ok(position) => {
                let mut parts = position.split(',');
                let x = parts
                    .next()
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(100.0);
                let y = parts
                    .next()
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(100.0);
                Bounds::from_anchor_and_size(
                    gpui::Anchor::TopLeft,
                    Point::new(px(x), px(y)),
                    size(px(738.0), px(833.0)),
                )
            }
            Err(_) => Bounds::centered(None, size(px(738.0), px(833.0)), cx),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    Chat::launch_from_env(cx).expect("set TILLER_ACP_PROGRAM to run the chat demo")
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

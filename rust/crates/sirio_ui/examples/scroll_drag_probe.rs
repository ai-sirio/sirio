//! Native probe for dragging a scrollbar thumb: the three bar implementations the app draws,
//! side by side in one window (`sirio_ui::probes::scroll_drag_view`).
//! `Scripts/Tests/test-scroll-drag-probe.sh` presses each thumb with a real pointer, drags it,
//! and reads the frame, so a PR that changes what draws a bar is judged by whether the thumb
//! still follows the pointer.
//!
//! The layout is a contract the script reads: a 900x600 window, three panes of 300x600 from
//! the left edge, a vertical bar at the right edge of panes 1 and 2 and a horizontal bar along
//! the bottom of pane 3. Pane 1's bar is Ely's (a 12 px strip, its 6 px thumb in the strip's left
//! half); the other two are 10 px thick.
//!
//! The appearance is dark, pinned. Env: `SIRIO_REDUCED_MOTION` (a debug build snaps every
//! animation to its rest state, as in `sirio`'s `main`).
use gpui::{
    AnyView, App, Bounds, Context, Render, Window, WindowBounds, WindowOptions, prelude::*, px,
    size,
};
use sirio_theme::{Theme, ThemeMode};

const WINDOW_WIDTH: f32 = 900.0;
const WINDOW_HEIGHT: f32 = 600.0;

/// A window's root has to be an `Entity`; the probe's view is an `AnyView`.
struct Root(AnyView);

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(sirio_ui::ely::AppAssets)
        .run(|cx: &mut App| {
            // Same motion setup as the surfaces probe: bezel parks its animation clock while no
            // window of the app is active, and a display with no window manager never activates
            // one, so a thumb's hover fade would stop half-way.
            {
                use bezel::motion::AppExt as _;
                cx.set_pause_when_inactive(false);
                if cfg!(debug_assertions) && std::env::var_os("SIRIO_REDUCED_MOTION").is_some() {
                    cx.set_reduce_motion(true);
                }
            }
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            Theme::init(cx);
            // Dark, whatever the desktop's colour scheme says: `Theme::init` follows the portal
            // on Linux, and the frames must not depend on the host. Said in probe.log.
            Theme::set_mode(ThemeMode::Dark, cx);
            println!("appearance: dark");
            sirio_ui::ely::init(cx);
            let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Scroll Drag Probe");
                    let view = sirio_ui::probes::scroll_drag_view(window, cx);
                    cx.new(|_| Root(view))
                },
            )
            .expect("open the scroll drag probe window");
            cx.activate(true);
        });
}

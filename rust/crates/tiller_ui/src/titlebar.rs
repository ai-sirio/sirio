//! The transparent, GPUI-owned controls that sit beside macOS traffic lights.
//!
//! Heights and sizes follow waku's measured scale: a 48px bar, 26px
//! controls, 6px radius, 14px glyphs, 6px gaps. On Linux nothing occupies
//! the macOS traffic-light zone, so the first control starts at waku's own
//! header inset (14px) instead of the 78px traffic-light clearance; a later
//! macOS pass can restore the clearance behind a `cfg`.

use gpui::{
    Context, EventEmitter, FontWeight, MouseButton, Render, Window, WindowControlArea, div,
    prelude::*, px, text,
};
use tiller_theme::Theme;

pub(crate) const HEIGHT: f32 = 48.0;
pub(crate) const TRAFFIC_LIGHT_INSET: f32 = 14.0;
pub(crate) const CONTROL_SIZE: f32 = 26.0;
pub(crate) const CONTROL_GAP: f32 = 6.0;
const TRAILING_INSET: f32 = 14.0;

/// The small title-strip control set used by the window shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitlebarEvent {
    ToggleSidebar,
    ToggleRightPanel,
}

pub struct Titlebar {
    should_move: bool,
}

impl Titlebar {
    /// Creates the default chrome state used by the demo and the app shell.
    pub fn new(_: &mut Context<Self>) -> Self {
        Self { should_move: false }
    }
}

impl EventEmitter<TitlebarEvent> for Titlebar {}

impl Render for Titlebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();

        let control = |id: &'static str, glyph: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .w(px(CONTROL_SIZE))
                .h(px(CONTROL_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .text_size(px(14.0))
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .child(text!(id = format!("titlebar-glyph-{id}"), glyph))
        };

        let sidebar = entity.clone();
        let right_panel = entity.clone();

        div()
            .id("tiller-titlebar")
            .window_control_area(WindowControlArea::Drag)
            .w_full()
            .h(px(HEIGHT))
            .flex()
            .items_center()
            .bg(theme.canvas)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = false),
            )
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.should_move {
                    this.should_move = false;
                    window.start_window_move();
                }
            }))
            .child(
                div()
                    .pl(px(TRAFFIC_LIGHT_INSET))
                    .w(px(TRAFFIC_LIGHT_INSET + CONTROL_SIZE))
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        control("titlebar-sidebar", "◧")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                sidebar.update(cx, |_, cx| cx.emit(TitlebarEvent::ToggleSidebar));
                            }),
                    ),
            )
            .child(div().flex_1().h_full())
            .child(
                div()
                    .pr(px(TRAILING_INSET))
                    .flex()
                    .gap(px(CONTROL_GAP))
                    .items_center()
                    .child(
                        control("titlebar-right-panel", "◨")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                right_panel
                                    .update(cx, |_, cx| cx.emit(TitlebarEvent::ToggleRightPanel));
                            }),
                    ),
            )
            .font_weight(FontWeight::NORMAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;
    use tiller_theme::Theme;

    #[gpui::test]
    async fn titlebar_controls_emit_shell_visibility_events(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let titlebar =
            cx.update(|window, _| window.root::<Titlebar>().flatten().expect("titlebar root"));
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&titlebar, move |_, event: &TitlebarEvent, _| {
                collected.borrow_mut().push(*event);
            })
            .detach();
        });

        let sidebar = cx
            .debug_bounds("titlebar-sidebar")
            .expect("sidebar visibility control is drawn");
        cx.simulate_click(sidebar.center(), Modifiers::none());
        cx.run_until_parked();
        let right_panel = cx
            .debug_bounds("titlebar-right-panel")
            .expect("right-panel visibility control is drawn");
        cx.simulate_click(right_panel.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[
                TitlebarEvent::ToggleSidebar,
                TitlebarEvent::ToggleRightPanel,
            ]
        );
    }
}

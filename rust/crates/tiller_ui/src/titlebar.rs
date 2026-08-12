//! The transparent, GPUI-owned controls that sit beside macOS traffic lights.

use gpui::{
    Context, FontWeight, MouseButton, Render, Window, WindowControlArea, div, prelude::*, px, text,
};
use tiller_theme::Theme;

const HEIGHT: f32 = 28.0;
const TRAFFIC_LIGHT_INSET: f32 = 78.0;
const CONTROL_SIZE: f32 = 24.0;
const CONTROL_GAP: f32 = 2.0;
const TRAILING_INSET: f32 = 16.0;

/// The small title-strip control set used by the window shell.
pub struct Titlebar {
    sidebar_visible: bool,
    right_panel_visible: bool,
    split_enabled: bool,
    permissions_visible: bool,
    should_move: bool,
}

impl Titlebar {
    /// Creates the default chrome state used by the demo and the app shell.
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            sidebar_visible: true,
            right_panel_visible: true,
            split_enabled: false,
            permissions_visible: false,
            should_move: false,
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        self.right_panel_visible = !self.right_panel_visible;
        cx.notify();
    }

    fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.split_enabled = !self.split_enabled;
        cx.notify();
    }

    fn toggle_permissions(&mut self, cx: &mut Context<Self>) {
        self.permissions_visible = !self.permissions_visible;
        cx.notify();
    }
}

impl Render for Titlebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();

        let control = |id: &'static str, glyph: &'static str| {
            div()
                .id(id)
                .w(px(CONTROL_SIZE))
                .h(px(CONTROL_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .text_size(px(13.0))
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .child(text!(id = format!("titlebar-glyph-{id}"), glyph))
        };

        let sidebar = entity.clone();
        let right_panel = entity.clone();
        let split = entity.clone();
        let permissions = entity.clone();

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
                                sidebar.update(cx, |this, cx| this.toggle_sidebar(cx));
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
                                right_panel.update(cx, |this, cx| this.toggle_right_panel(cx));
                            }),
                    )
                    .child(
                        control("titlebar-split", "◫")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                split.update(cx, |this, cx| this.toggle_split(cx));
                            }),
                    )
                    .child(
                        control("titlebar-permissions", "◉")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, cx| {
                                permissions.update(cx, |this, cx| this.toggle_permissions(cx));
                            }),
                    ),
            )
            .font_weight(FontWeight::NORMAL)
    }
}

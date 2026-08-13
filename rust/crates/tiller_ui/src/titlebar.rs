//! The transparent, GPUI-owned controls that sit beside macOS traffic lights.
//!
//! Heights and sizes follow waku's measured scale: a 48px bar, 26px
//! controls, 14px glyphs. On Linux nothing occupies the macOS traffic-light
//! zone, so the first control starts at waku's own header inset (14px)
//! instead of the 78px traffic-light clearance; a later macOS pass can
//! restore the clearance behind a `cfg`. `HEIGHT`, `CONTROL_SIZE`,
//! `CONTROL_GAP` and `TRAFFIC_LIGHT_INSET` are pinned by
//! `conformance.rs`'s `bars_and_rows_use_the_measured_density` and stay
//! exactly as measured — this surface's *geometry* is waku's, its *color,
//! hierarchy and radius* are COSMIC's.
//!
//! This is Tiller's COSMIC-01/02 reference surface (see
//! `docs/linux-rewrite/COSMIC-DESIGN.md`): it reads `Theme::get(cx).cosmic`,
//! demonstrating the container hierarchy (the bar is a `primary` layer over
//! the window's `background`, with a hairline `divider` marking the seam),
//! the semantic `icon_button` component for its two ghost controls, the
//! corner-radius scale, and one live spacing step (the trailing inset).
//! Before COSMIC-02 this read a second, standalone `CosmicTheme` global of
//! its own; that seam is retired (`Theme` carries `cosmic` directly now),
//! so this surface reads the same global every other surface does.

use gpui::{
    Context, EventEmitter, FontWeight, MouseButton, Render, Window, WindowControlArea, div,
    prelude::*, px, text,
};
use tiller_theme::Theme;

pub(crate) const HEIGHT: f32 = 48.0;
pub(crate) const TRAFFIC_LIGHT_INSET: f32 = 14.0;
pub(crate) const CONTROL_SIZE: f32 = 26.0;
pub(crate) const CONTROL_GAP: f32 = 6.0;

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
    ///
    /// Installs [`Theme`] if nothing has installed it yet, so this surface
    /// renders correctly whether it is opened standalone (a test,
    /// `chrome_demo`) or inside the app shell.
    pub fn new(cx: &mut Context<Self>) -> Self {
        if !cx.has_global::<Theme>() {
            Theme::init(cx);
        }
        Self { should_move: false }
    }
}

impl EventEmitter<TitlebarEvent> for Titlebar {}

impl Render for Titlebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cosmic = Theme::get(cx).cosmic;
        let bar = cosmic.containers.primary;
        let icon_button = cosmic.semantic.icon_button;
        let control_radius = px(cosmic.radii.radius_xs[0]);
        let trailing_inset = px(cosmic.spacing.xs as f32);
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
                .rounded(control_radius)
                .text_size(px(14.0))
                .text_color(icon_button.on)
                .hover(|style| style.bg(icon_button.hover))
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
            .bg(bar.base)
            .border_b(px(1.0))
            .border_color(bar.divider)
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
                    .pr(trailing_inset)
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
    use tiller_theme::ThemeMode;

    #[gpui::test]
    async fn titlebar_controls_emit_shell_visibility_events(cx: &mut TestAppContext) {
        // `Titlebar::new` self-installs `CosmicTheme` when nothing else has;
        // no explicit init call needed here.
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

    /// UI-tier evidence for the dark half of the COSMIC restyle: installs
    /// `Theme` explicitly *before* the entity exists, so `Titlebar`'s lazy
    /// bootstrap in `new` is a no-op and this only passes if `render` truly
    /// reads back the already-installed global's `cosmic` field rather than
    /// a hardcoded value.
    #[gpui::test]
    async fn titlebar_draws_the_cosmic_primary_container_in_dark_mode(cx: &mut TestAppContext) {
        cx.update(|cx| Theme::install(ThemeMode::Dark, cx));
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let resolved = cx.update(|_, cx| Theme::get(cx).cosmic);
        assert!(resolved.is_dark, "installed Dark must resolve to is_dark");

        let sidebar = cx
            .debug_bounds("titlebar-sidebar")
            .expect("sidebar control is drawn under the dark cosmic theme");
        let right_panel = cx
            .debug_bounds("titlebar-right-panel")
            .expect("right-panel control is drawn under the dark cosmic theme");
        assert_eq!(sidebar.size.height, px(CONTROL_SIZE));
        assert_eq!(right_panel.size.height, px(CONTROL_SIZE));
    }

    /// The light half of the same proof — the brief is explicit that a
    /// dark-only pass is "half a design system", so this is a distinct
    /// named test, not a variant of the dark one.
    #[gpui::test]
    async fn titlebar_draws_the_cosmic_primary_container_in_light_mode(cx: &mut TestAppContext) {
        cx.update(|cx| Theme::install(ThemeMode::Light, cx));
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let resolved = cx.update(|_, cx| Theme::get(cx).cosmic);
        assert!(
            !resolved.is_dark,
            "installed Light must resolve to is_dark == false"
        );
        assert_ne!(
            resolved.containers.primary.base,
            Theme::dark().cosmic.containers.primary.base,
            "light and dark primary containers must not collapse to the same fill"
        );

        let sidebar = cx
            .debug_bounds("titlebar-sidebar")
            .expect("sidebar control is drawn under the light cosmic theme");
        let right_panel = cx
            .debug_bounds("titlebar-right-panel")
            .expect("right-panel control is drawn under the light cosmic theme");
        assert_eq!(sidebar.size.height, px(CONTROL_SIZE));
        assert_eq!(right_panel.size.height, px(CONTROL_SIZE));
    }
}

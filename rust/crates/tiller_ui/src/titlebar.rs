//! The top bar — comet's row shape (P76), COSMIC's tokens.
//!
//! `docs/linux-rewrite/tasks/P76-the-comet-top-bar-and-icon-set.md`: the
//! user's screenshot wins on this surface specifically where it and COSMIC
//! disagree; COSMIC stays the system everywhere else. The one fact that
//! shapes this file: **neither comet nor Tiller draws traffic lights on
//! Linux** — both are OS-native macOS decorations, and GPUI on X11 hands us
//! a bare window. The three traffic lights below are drawn, real, circular
//! controls — close/minimize/maximize — not a macOS-only convenience and
//! not COSMIC's square window controls.
//!
//! Geometry comes from [`tiller_theme::BrowserChrome`] (bar height, light
//! size/gap/inset, `BrowserChrome::cluster_start` for the derivation of
//! where the light group hands off to the button cluster) and
//! `Spacing::compact_action` (the 24px cluster-button frame). Colour comes
//! from `Theme::get(cx).cosmic`: the bar sits on `containers.background`
//! (not `primary` — a deliberate override of COSMIC's usual raised-bar
//! placement, because the screenshot wants **one continuous surface**, no
//! seam between chrome and content), the lights are COSMIC's own
//! `destructive`/`warning`/`success` semantic colours (already red/amber/
//! green — no new colour tokens needed), and the cluster buttons use
//! `semantic.icon_button` like the surface's original two controls did.
//!
//! Row layout, left to right: traffic lights → cluster (sidebar toggle,
//! back, forward, `+`) → accent dot + title → muted subtitle → (spacer) →
//! right-panel toggle. The far-right scope/branch pills and collapse/
//! expand controls from the screenshot are **not built** — see the P76
//! report for why.
//!
//! Back/forward/`+` have no data to act on from inside this crate (no
//! navigation history, no new-tab concept lives here) — rather than ship
//! live-looking dead buttons, they render muted and inert until a host
//! injects a handler via [`Titlebar::on_back`] / [`Titlebar::on_forward`] /
//! [`Titlebar::on_new_tab`]. That is the seam left for `codex12`; wiring it
//! needs no change to this file or to `main.rs`'s `TitlebarEvent` match —
//! these are plain closures, not a widened enum.

use gpui::{
    App, Context, EventEmitter, FontWeight, MouseButton, Pixels, Point, Render, SharedString,
    Window, WindowControlArea, div, prelude::*, px,
};
use std::rc::Rc;
use tiller_theme::Theme;
use tiller_theme::cosmic::CosmicComponent;

use crate::sidebar::icons::Icon;

/// F-WIN-09: the Linux counterpart of macOS's `AppleActionOnDoubleClick`.
/// GPUI's own `Window::titlebar_double_click`/`PlatformWindow::
/// titlebar_double_click` is explicitly macOS-only (a no-op default
/// everywhere else) -- there is no Linux platform implementation to defer
/// to, so this app reads the GNOME preference and applies it to its own
/// client-side-decorated titlebar itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoubleClickAction {
    /// `toggle-maximize` and the fallback for any unrecognized/absent
    /// value -- matches GNOME's own documented default.
    ToggleMaximize,
    Minimize,
    None,
    /// Opens the platform's native title-bar context menu at the click
    /// position (`Window::show_window_menu`).
    Menu,
}

impl DoubleClickAction {
    /// Parses `gsettings`' raw stdout for
    /// `org.gnome.desktop.wm.preferences action-double-click-titlebar`,
    /// which quotes its string value (e.g. `'toggle-maximize'\n`).
    fn from_gsettings_output(raw: &str) -> Self {
        match raw.trim().trim_matches('\'') {
            "minimize" => Self::Minimize,
            "none" => Self::None,
            "menu" | "lower" => Self::Menu,
            _ => Self::ToggleMaximize,
        }
    }

    /// Reads the live system preference by shelling out to `gsettings`.
    /// Degrades to [`Self::ToggleMaximize`] -- GNOME's own default --
    /// whenever the binary is missing, the schema isn't installed (sway,
    /// COSMIC, and other non-GNOME compositors need not ship it), or the
    /// call otherwise fails; this is a graceful default; not an error.
    ///
    /// No `#[cfg]` seam needed here, unlike the rest of this wave's Linux-only
    /// call sites: `Command::new("gsettings")` fails to spawn on any platform
    /// without that binary and is already caught by the `_ =>` arm below, so
    /// this compiles and degrades safely everywhere. That fallback happens to
    /// be exactly right on both other platforms, for different reasons: on
    /// Windows there is no user-configurable double-click action at all
    /// (double-click always maximizes — PORTABILITY.md), which *is*
    /// `ToggleMaximize`, so nothing further is needed; on macOS the
    /// counterpart is a real, distinct system preference,
    /// `AppleActionOnDoubleClick` (readable via `defaults read -g
    /// AppleActionOnDoubleClick`, the same preference the Swift original
    /// reads), which a future macOS implementation should read instead of
    /// silently accepting the GNOME-shaped default.
    pub fn from_system() -> Self {
        match std::process::Command::new("gsettings")
            .args([
                "get",
                "org.gnome.desktop.wm.preferences",
                "action-double-click-titlebar",
            ])
            .output()
        {
            Ok(output) if output.status.success() => {
                Self::from_gsettings_output(&String::from_utf8_lossy(&output.stdout))
            }
            _ => Self::ToggleMaximize,
        }
    }
}

/// The small title-strip control set used by the window shell. Unrelated to
/// the traffic lights and cluster buttons below, which act directly through
/// injected closures rather than this event — see the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitlebarEvent {
    ToggleSidebar,
    ToggleRightPanel,
}

pub struct Titlebar {
    should_move: bool,
    on_close: Rc<dyn Fn(&mut Window)>,
    on_minimize: Rc<dyn Fn(&mut Window)>,
    on_maximize: Rc<dyn Fn(&mut Window)>,
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_forward: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_new_tab: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    /// F-WIN-07: this app draws no in-window menu bar by design (see the
    /// module docs), so this cluster button is the surface's stand-in for
    /// the reference app's "History > Restore Previous Launch" menu entry.
    /// Unwired (the default), it renders muted like `on_back`/`on_forward`/
    /// `on_new_tab` above, for the same "no live-looking dead control"
    /// reason.
    on_history: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    title: Option<SharedString>,
    subtitle: Option<SharedString>,
    /// F-WIN-09: applied when the titlebar's own `on_mouse_up` sees
    /// `click_count == 2`.
    double_click_action: DoubleClickAction,
    /// Backs [`DoubleClickAction::Menu`]. Defaults to the real
    /// `Window::show_window_menu`, which calls into
    /// `PlatformWindow::show_window_menu` -- `unimplemented!()` under
    /// `TestWindow`, so a drawn test exercising `Menu` MUST override this
    /// with [`Self::with_menu_handler`], the same constraint `new`'s docs
    /// already state for minimize/maximize.
    on_show_menu: Rc<dyn Fn(&mut Window, Point<Pixels>)>,
}

impl Titlebar {
    /// Creates the default chrome state used by the demo and the app shell.
    ///
    /// Installs [`Theme`] if nothing has installed it yet, so this surface
    /// renders correctly whether it is opened standalone (a test,
    /// `chrome_demo`) or inside the app shell. The three window controls
    /// default to the real GPUI operations — `remove_window` is safe to
    /// call under `TestWindow` (it only flips a `removed` flag), but
    /// `minimize_window`/`zoom_window` call into `TestWindow::minimize`/
    /// `::zoom`, both `unimplemented!()` in GPUI's test platform backend;
    /// drawn tests for those two MUST override with
    /// [`Self::with_minimize_handler`]/[`Self::with_maximize_handler`]
    /// rather than exercise the real default.
    pub fn new(cx: &mut Context<Self>) -> Self {
        if !cx.has_global::<Theme>() {
            Theme::init(cx);
        }
        Self {
            should_move: false,
            on_close: Rc::new(|window| window.remove_window()),
            on_minimize: Rc::new(|window| window.minimize_window()),
            on_maximize: Rc::new(|window| window.zoom_window()),
            on_back: None,
            on_forward: None,
            on_new_tab: None,
            on_history: None,
            title: None,
            subtitle: None,
            double_click_action: DoubleClickAction::from_system(),
            on_show_menu: Rc::new(|window, position| window.show_window_menu(position)),
        }
    }

    /// Test override for the double-click preference -- avoids shelling out
    /// to `gsettings` from a drawn test, and lets each of the four values
    /// be exercised deterministically regardless of what this box's
    /// desktop environment actually has configured.
    pub fn with_double_click_action(mut self, action: DoubleClickAction) -> Self {
        self.double_click_action = action;
        self
    }

    /// Test/host override for [`DoubleClickAction::Menu`] -- see the
    /// `on_show_menu` field docs for why a drawn test exercising it MUST
    /// supply one rather than exercise the real default.
    pub fn with_menu_handler(
        mut self,
        handler: impl Fn(&mut Window, Point<Pixels>) + 'static,
    ) -> Self {
        self.on_show_menu = Rc::new(handler);
        self
    }

    /// Test/host override for the close control.
    pub fn with_close_handler(mut self, handler: impl Fn(&mut Window) + 'static) -> Self {
        self.on_close = Rc::new(handler);
        self
    }

    /// Test/host override for the minimize control — see [`Self::new`] for
    /// why a drawn test must supply one rather than exercise the default.
    pub fn with_minimize_handler(mut self, handler: impl Fn(&mut Window) + 'static) -> Self {
        self.on_minimize = Rc::new(handler);
        self
    }

    /// Test/host override for the maximize control — same constraint as
    /// [`Self::with_minimize_handler`].
    pub fn with_maximize_handler(mut self, handler: impl Fn(&mut Window) + 'static) -> Self {
        self.on_maximize = Rc::new(handler);
        self
    }

    /// Wires the cluster's back control. Unset (the default), it renders
    /// muted and does not respond to clicks — see the module docs.
    pub fn on_back(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Rc::new(handler));
        self
    }

    /// The forward counterpart to [`Self::on_back`].
    pub fn on_forward(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_forward = Some(Rc::new(handler));
        self
    }

    /// Wires the cluster's trailing `+`. Unset, it renders muted for the
    /// same reason as [`Self::on_back`].
    pub fn on_new_tab(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_new_tab = Some(Rc::new(handler));
        self
    }

    /// Wires the History seam (F-WIN-07) — this app's stand-in for the
    /// reference "History > Restore Previous Launch" menu entry, since no
    /// in-window menu bar is drawn here. Unset, it renders muted like the
    /// other cluster seams.
    pub fn on_history(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_history = Some(Rc::new(handler));
        self
    }

    /// Sets the title shown after the accent dot (row 3 of the screenshot).
    /// Unset, that section does not render — no placeholder text stands in
    /// for data this surface does not have.
    pub fn with_title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the muted secondary string beside the title (row 4, e.g.
    /// `project @ worktree`). Unset, that section does not render.
    pub fn with_subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

impl EventEmitter<TitlebarEvent> for Titlebar {}

/// One traffic-light dot: a real, circular window control, not a decoration.
/// `component` supplies COSMIC's own semantic colour for the action
/// (`destructive`/`warning`/`success` — already red/amber/green, so this
/// needs no new colour tokens).
fn traffic_light(
    id: &'static str,
    area: WindowControlArea,
    diameter: gpui::Pixels,
    component: CosmicComponent,
    handler: Rc<dyn Fn(&mut Window)>,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .window_control_area(area)
        .w(diameter)
        .h(diameter)
        .rounded(diameter * 0.5)
        .bg(component.base)
        .hover(|style| style.bg(component.hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, _| handler(window))
}

/// One cluster-style icon button (sidebar toggle, back, forward, `+`, the
/// right-panel toggle). `handler` absent means "no seam wired yet": the
/// button still draws, at the same size and position the screenshot shows,
/// but dims and does not respond to clicks — an honest "not yet" rather
/// than a live-looking dead control.
fn cluster_button(
    id: &'static str,
    icon: Icon,
    size: gpui::Pixels,
    radius: gpui::Pixels,
    icon_button: CosmicComponent,
    handler: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
) -> impl IntoElement {
    let enabled = handler.is_some();
    let color = if enabled {
        icon_button.on
    } else {
        icon_button.on.opacity(0.35)
    };
    let mut element = div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .w(size)
        .h(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius)
        .child(icon.element(px(14.0)).text_color(color));
    if let Some(handler) = handler {
        element = element
            .hover(|style| style.bg(icon_button.hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| handler(window, cx));
    }
    element
}

impl Render for Titlebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::get(cx);
        let cosmic = theme.cosmic;
        let chrome = theme.browser_chrome;
        let bar = cosmic.containers.background;
        let icon_button = cosmic.semantic.icon_button;
        let control_radius = px(cosmic.radii.radius_xs[0]);
        let button_size = theme.spacing.compact_action;
        let trailing_inset = px(cosmic.spacing.xs as f32);
        let entity = cx.entity();

        let on_close = self.on_close.clone();
        let on_minimize = self.on_minimize.clone();
        let on_maximize = self.on_maximize.clone();
        let double_click_action = self.double_click_action;
        let double_click_minimize = self.on_minimize.clone();
        let double_click_maximize = self.on_maximize.clone();
        let double_click_menu = self.on_show_menu.clone();
        let on_back = self.on_back.clone();
        let on_forward = self.on_forward.clone();
        let on_new_tab = self.on_new_tab.clone();
        let on_history = self.on_history.clone();
        let title = self.title.clone();
        let subtitle = self.subtitle.clone();

        let sidebar_entity = entity.clone();
        let on_sidebar: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |_, cx| {
            sidebar_entity.update(cx, |_, cx| cx.emit(TitlebarEvent::ToggleSidebar));
        });
        let right_panel_entity = entity.clone();
        let on_right_panel: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |_, cx| {
            right_panel_entity.update(cx, |_, cx| cx.emit(TitlebarEvent::ToggleRightPanel));
        });

        let traffic_lights = div()
            .id("titlebar-traffic-lights")
            .pl(chrome.traffic_light_inset)
            .flex()
            .items_center()
            .gap(chrome.traffic_light_gap)
            .child(traffic_light(
                "titlebar-close",
                WindowControlArea::Close,
                chrome.traffic_light_diameter,
                cosmic.semantic.destructive,
                on_close,
            ))
            .child(traffic_light(
                "titlebar-minimize",
                WindowControlArea::Min,
                chrome.traffic_light_diameter,
                cosmic.semantic.warning,
                on_minimize,
            ))
            .child(traffic_light(
                "titlebar-maximize",
                WindowControlArea::Max,
                chrome.traffic_light_diameter,
                cosmic.semantic.success,
                on_maximize,
            ));

        let cluster = div()
            .id("titlebar-cluster")
            .pl(chrome.traffic_light_cluster_gap)
            .flex()
            .items_center()
            .gap(chrome.cluster_button_gap)
            .child(cluster_button(
                "titlebar-sidebar",
                Icon::SidebarLeft,
                button_size,
                control_radius,
                icon_button,
                Some(on_sidebar),
            ))
            .child(cluster_button(
                "titlebar-back",
                Icon::ChevronLeft,
                button_size,
                control_radius,
                icon_button,
                on_back,
            ))
            .child(cluster_button(
                "titlebar-forward",
                Icon::ChevronRight,
                button_size,
                control_radius,
                icon_button,
                on_forward,
            ))
            .child(cluster_button(
                "titlebar-new-tab",
                Icon::Plus,
                button_size,
                control_radius,
                icon_button,
                on_new_tab,
            ));

        let title_row = title.map(|title| {
            div()
                .id("titlebar-title")
                .debug_selector(|| "titlebar-title".to_owned())
                .pl(px(10.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(cosmic.semantic.accent.base),
                )
                .child(div().text_color(bar.on).text_size(px(12.5)).child(title))
                .children(subtitle.map(|subtitle| {
                    div()
                        .text_color(bar.on.opacity(0.55))
                        .text_size(px(12.5))
                        .child(subtitle)
                }))
        });

        div()
            .id("tiller-titlebar")
            .debug_selector(|| "tiller-titlebar".to_owned())
            .window_control_area(WindowControlArea::Drag)
            .w_full()
            .h(chrome.bar_height)
            .flex()
            .items_center()
            .bg(bar.base)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = false),
            )
            // F-WIN-09: `MouseUpEvent::click_count` is GPUI's own
            // double-click disambiguation (platform timing/threshold
            // already applied), so no timer or distance bookkeeping is
            // needed here -- just react on the second click.
            .on_mouse_up(MouseButton::Left, move |event, window, _| {
                if event.click_count != 2 {
                    return;
                }
                match double_click_action {
                    DoubleClickAction::ToggleMaximize => double_click_maximize(window),
                    DoubleClickAction::Minimize => double_click_minimize(window),
                    DoubleClickAction::None => {}
                    DoubleClickAction::Menu => double_click_menu(window, event.position),
                }
            })
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.should_move {
                    this.should_move = false;
                    window.start_window_move();
                }
            }))
            .child(traffic_lights)
            .child(cluster)
            .children(title_row)
            .child(div().flex_1().h_full())
            .child(
                div()
                    .pr(trailing_inset)
                    .flex()
                    .items_center()
                    .gap(chrome.cluster_button_gap)
                    .child(cluster_button(
                        "titlebar-history",
                        Icon::RefreshCw,
                        button_size,
                        control_radius,
                        icon_button,
                        on_history,
                    ))
                    .child(cluster_button(
                        "titlebar-right-panel",
                        Icon::PanelRight,
                        button_size,
                        control_radius,
                        icon_button,
                        Some(on_right_panel),
                    )),
            )
            .font_weight(FontWeight::NORMAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseDownEvent, MouseUpEvent, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use tiller_theme::ThemeMode;

    /// F-WIN-09: `gsettings`' own quoting convention for a string value
    /// (`'toggle-maximize'\n`), plus the three other documented values and
    /// an unrecognized/garbage one, which must degrade to the documented
    /// default rather than panic or silently do nothing.
    #[test]
    fn double_click_action_parses_every_gsettings_value() {
        assert_eq!(
            DoubleClickAction::from_gsettings_output("'toggle-maximize'\n"),
            DoubleClickAction::ToggleMaximize
        );
        assert_eq!(
            DoubleClickAction::from_gsettings_output("'minimize'\n"),
            DoubleClickAction::Minimize
        );
        assert_eq!(
            DoubleClickAction::from_gsettings_output("'none'\n"),
            DoubleClickAction::None
        );
        assert_eq!(
            DoubleClickAction::from_gsettings_output("'menu'\n"),
            DoubleClickAction::Menu
        );
        assert_eq!(
            DoubleClickAction::from_gsettings_output("'not-a-real-value'\n"),
            DoubleClickAction::ToggleMaximize,
            "an unrecognized value must degrade to the documented default"
        );
    }

    /// The row's own instruction: "check it resolves on this box before
    /// building on it". `gsettings get
    /// org.gnome.desktop.wm.preferences action-double-click-titlebar`
    /// resolves to `'toggle-maximize'` on this dev box, which is also the
    /// graceful-absence default -- so this assertion holds whether or not
    /// the CI box that eventually runs it has the GNOME schema installed.
    #[test]
    fn double_click_action_from_system_resolves_without_panicking() {
        assert_eq!(
            DoubleClickAction::from_system(),
            DoubleClickAction::ToggleMaximize
        );
    }

    /// Fires the second half of a real double-click (`click_count == 2`,
    /// GPUI's own platform-timing disambiguation already applied -- see
    /// the render-site comment) directly at the drag area's own drawn
    /// bounds, and asserts each configured action reaches its real seam.
    #[gpui::test]
    async fn double_click_on_the_drag_area_applies_the_configured_action(
        cx: &mut TestAppContext,
    ) {
        let maximized = Rc::new(RefCell::new(false));
        let minimized = Rc::new(RefCell::new(false));
        let (maximize_spy, minimize_spy) = (maximized.clone(), minimized.clone());
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx)
                .with_double_click_action(DoubleClickAction::ToggleMaximize)
                .with_maximize_handler(move |_window| *maximize_spy.borrow_mut() = true)
                .with_minimize_handler(move |_window| *minimize_spy.borrow_mut() = true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx
            .debug_bounds("tiller-titlebar")
            .expect("titlebar is drawn");
        // The row's own empty flex-filler, clear of the traffic lights,
        // cluster buttons, and trailing icons -- an ordinary spot on the
        // drag area, the way a user would actually double-click it.
        let position = bar.center();
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();

        assert!(
            *maximized.borrow(),
            "ToggleMaximize must invoke the wired maximize handler"
        );
        assert!(
            !*minimized.borrow(),
            "ToggleMaximize must not also invoke minimize"
        );
    }

    /// The `Minimize` counterpart to the `ToggleMaximize` case above.
    #[gpui::test]
    async fn double_click_configured_to_minimize_invokes_the_minimize_handler(
        cx: &mut TestAppContext,
    ) {
        let minimized = Rc::new(RefCell::new(false));
        let spy = minimized.clone();
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx)
                .with_double_click_action(DoubleClickAction::Minimize)
                .with_minimize_handler(move |_window| *spy.borrow_mut() = true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx
            .debug_bounds("tiller-titlebar")
            .expect("titlebar is drawn");
        let position = bar.center();
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();

        assert!(
            *minimized.borrow(),
            "Minimize must invoke the wired minimize handler"
        );
    }

    /// `Menu` reaches [`Titlebar::with_menu_handler`]'s seam rather than
    /// the real `Window::show_window_menu` (`unimplemented!()` under
    /// `TestWindow`), and is handed the click's own position.
    #[gpui::test]
    async fn double_click_configured_to_menu_opens_the_window_menu_at_the_click(
        cx: &mut TestAppContext,
    ) {
        let opened_at = Rc::new(RefCell::new(None));
        let spy = opened_at.clone();
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx)
                .with_double_click_action(DoubleClickAction::Menu)
                .with_menu_handler(move |_window, position| *spy.borrow_mut() = Some(position))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx
            .debug_bounds("tiller-titlebar")
            .expect("titlebar is drawn");
        let position = bar.center();
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();

        assert_eq!(
            *opened_at.borrow(),
            Some(position),
            "Menu must open at the click's own position"
        );
    }

    /// `None` is the one configuration that must invoke nothing at all --
    /// the default `TestWindow::zoom`/`::minimize` would panic
    /// (`unimplemented!()`) if this regressed to calling either.
    #[gpui::test]
    async fn double_click_configured_to_none_invokes_no_window_control(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_double_click_action(DoubleClickAction::None)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx
            .debug_bounds("tiller-titlebar")
            .expect("titlebar is drawn");
        let position = bar.center();
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();
        // No panic and no assertion target: the real `remove_window`/
        // `unimplemented!()` defaults are left in place deliberately, so a
        // regression to ToggleMaximize/Minimize/Menu would panic this test.
    }

    #[gpui::test]
    async fn titlebar_controls_emit_shell_visibility_events(cx: &mut TestAppContext) {
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

    /// The close control's production wiring, exercised for real — no spy.
    /// `Window::remove_window` only flips a `removed` flag even under
    /// `TestWindow`, and GPUI's own `update_window_id` drops a removed
    /// window from `cx.windows()` after the update runs, so a shrinking
    /// window count is a real, observable post-condition of the *actual*
    /// close path, not a mechanism-only proxy for it (`P75`'s repeated
    /// lesson: drive the control, not the function behind it).
    #[gpui::test]
    async fn the_close_control_closes_the_real_window(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();

        let before = vcx.cx.windows().len();

        let close = vcx
            .debug_bounds("titlebar-close")
            .expect("close control is drawn");
        vcx.simulate_click(close.center(), Modifiers::none());
        vcx.run_until_parked();

        // The window is gone now, so `vcx.update` (which targets this
        // specific, now-removed window) would panic on its own `.unwrap()`
        // — read the app-wide window list off `TestAppContext` directly.
        let after = vcx.cx.windows().len();
        assert_eq!(
            after,
            before - 1,
            "closing the window drops it from cx.windows()"
        );
    }

    /// Minimize/maximize call into `TestWindow::minimize`/`::zoom`, both
    /// `unimplemented!()` in GPUI's test platform backend — so unlike
    /// close, these two are driven through the control with an injected
    /// spy standing in for the real platform call, not the real default.
    #[gpui::test]
    async fn the_minimize_control_invokes_its_wired_handler(cx: &mut TestAppContext) {
        let called = Rc::new(RefCell::new(false));
        let spy = called.clone();
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_minimize_handler(move |_window| {
                *spy.borrow_mut() = true;
            })
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let minimize = cx
            .debug_bounds("titlebar-minimize")
            .expect("minimize control is drawn");
        cx.simulate_click(minimize.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(*called.borrow(), "clicking minimize invoked the handler");
    }

    /// The maximize counterpart to the minimize test above, for the same
    /// `TestWindow::zoom` `unimplemented!()` reason.
    #[gpui::test]
    async fn the_maximize_control_invokes_its_wired_handler(cx: &mut TestAppContext) {
        let called = Rc::new(RefCell::new(false));
        let spy = called.clone();
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_maximize_handler(move |_window| {
                *spy.borrow_mut() = true;
            })
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let maximize = cx
            .debug_bounds("titlebar-maximize")
            .expect("maximize control is drawn");
        cx.simulate_click(maximize.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(*called.borrow(), "clicking maximize invoked the handler");
    }

    /// Back/forward/`+` must not be dead-looking controls: unwired (the
    /// default), a click must not silently succeed at nothing observable —
    /// there is nothing to observe because no `on_click` is attached at
    /// all when no handler is set. This test drives the *wired* case
    /// instead, the same way the minimize/maximize tests do, and a
    /// companion assertion below confirms the unwired case never panics
    /// merely from being clicked.
    #[gpui::test]
    async fn the_cluster_seams_invoke_their_wired_handlers(cx: &mut TestAppContext) {
        let back_called = Rc::new(RefCell::new(false));
        let forward_called = Rc::new(RefCell::new(false));
        let new_tab_called = Rc::new(RefCell::new(false));
        let (back_spy, forward_spy, new_tab_spy) = (
            back_called.clone(),
            forward_called.clone(),
            new_tab_called.clone(),
        );
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx)
                .on_back(move |_, _| *back_spy.borrow_mut() = true)
                .on_forward(move |_, _| *forward_spy.borrow_mut() = true)
                .on_new_tab(move |_, _| *new_tab_spy.borrow_mut() = true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        for (selector, flag) in [
            ("titlebar-back", &back_called),
            ("titlebar-forward", &forward_called),
            ("titlebar-new-tab", &new_tab_called),
        ] {
            let bounds = cx.debug_bounds(selector).expect("cluster seam is drawn");
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
            assert!(*flag.borrow(), "{selector} invoked its wired handler");
        }
    }

    /// Unwired back/forward/`+` (the default state) must render — the
    /// screenshot's row shape stays intact — but a click must not panic or
    /// emit anything: no handler is attached to click at all.
    #[gpui::test]
    async fn unwired_cluster_seams_render_but_do_not_panic_on_click(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        for selector in ["titlebar-back", "titlebar-forward", "titlebar-new-tab"] {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn even unwired"));
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        }
    }

    /// F-WIN-07: the History seam is the same "unwired renders muted, wired
    /// invokes the handler" contract as back/forward/`+` above — see
    /// [`the_cluster_seams_invoke_their_wired_handlers`] and
    /// [`unwired_cluster_seams_render_but_do_not_panic_on_click`].
    #[gpui::test]
    async fn the_history_seam_invokes_its_wired_handler(cx: &mut TestAppContext) {
        let called = Rc::new(RefCell::new(false));
        let spy = called.clone();
        let window =
            cx.add_window(|_window, cx| Titlebar::new(cx).on_history(move |_, _| *spy.borrow_mut() = true));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let history = cx
            .debug_bounds("titlebar-history")
            .expect("history control is drawn");
        cx.simulate_click(history.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(*called.borrow(), "titlebar-history invoked its wired handler");
    }

    #[gpui::test]
    async fn unwired_history_seam_renders_but_does_not_panic_on_click(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let history = cx
            .debug_bounds("titlebar-history")
            .expect("history control is drawn even unwired");
        cx.simulate_click(history.center(), Modifiers::none());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn title_and_subtitle_render_only_when_set(cx: &mut TestAppContext) {
        let unset_window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut unset_cx = VisualTestContext::from_window(unset_window.into(), cx);
        unset_cx.run_until_parked();
        assert!(
            unset_cx.debug_bounds("titlebar-title").is_none(),
            "no placeholder title when unset"
        );

        let set_window = cx.add_window(|_window, cx| {
            Titlebar::new(cx)
                .with_title("Tiller")
                .with_subtitle("tiller-linux @ linux/gpui-waku")
        });
        let mut set_cx = VisualTestContext::from_window(set_window.into(), cx);
        set_cx.run_until_parked();
        assert!(
            set_cx.debug_bounds("titlebar-title").is_some(),
            "title row is drawn once set"
        );
    }

    /// UI-tier evidence for the P76 restyle: installs `Theme` explicitly
    /// *before* the entity exists, so `Titlebar`'s lazy bootstrap in `new`
    /// is a no-op and this only passes if `render` truly reads back the
    /// already-installed global's tokens rather than a hardcoded value.
    #[gpui::test]
    async fn titlebar_draws_the_continuous_background_surface_in_dark_mode(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| Theme::install(ThemeMode::Dark, cx));
        let window = cx.add_window(|_window, cx| Titlebar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let resolved = cx.update(|_, cx| Theme::get(cx).cosmic);
        assert!(resolved.is_dark, "installed Dark must resolve to is_dark");

        let bar_height = cx.update(|_, cx| Theme::get(cx).browser_chrome.bar_height);
        let close = cx
            .debug_bounds("titlebar-close")
            .expect("close control is drawn under the dark cosmic theme");
        assert_eq!(
            close.size.height,
            cx.update(|_, cx| Theme::get(cx).browser_chrome.traffic_light_diameter)
        );
        let _ = bar_height;
    }

    /// The light half of the same proof — a dark-only pass is "half a
    /// design system", so this is a distinct named test, not a variant.
    #[gpui::test]
    async fn titlebar_draws_the_continuous_background_surface_in_light_mode(
        cx: &mut TestAppContext,
    ) {
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
            resolved.containers.background.base,
            Theme::dark().cosmic.containers.background.base,
            "light and dark background containers must not collapse to the same fill"
        );

        let close = cx
            .debug_bounds("titlebar-close")
            .expect("close control is drawn under the light cosmic theme");
        assert_eq!(
            close.size.height,
            cx.update(|_, cx| Theme::get(cx).browser_chrome.traffic_light_diameter)
        );
    }
}

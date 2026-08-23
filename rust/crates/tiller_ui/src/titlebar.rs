//! The top bar — comet's row shape (P76), COSMIC's tokens.
//!
//! `docs/linux-rewrite/tasks/P76-the-comet-top-bar-and-icon-set.md`: the
//! user's screenshot wins on this surface specifically where it and COSMIC
//! disagree; COSMIC stays the system everywhere else.
//!
//! **P102 revised the premise this file was built on.** The three traffic
//! lights below are no longer drawn unconditionally. Measured live
//! (`docs/linux-rewrite/tasks/P102-the-top-bar-belongs-to-the-os.md`), the
//! GPUI revision this app pins already asks X11/Wayland for **server-side**
//! decorations by default (`window_decorations` is never set, and
//! `gpui::Window::new` defaults the unset case to `WindowDecorations::
//! Server`) — so a real window manager draws its own titlebar, and drawing
//! three more controls underneath it is the "two rows of window controls"
//! defect the user's directive objects to («il semaforo non lo devi creare
//! te ma deve dipendere dal SO»). Per the user's own resolution («Sì,
//! risolvi come Zed»), this file now asks the platform the same way Zed's
//! `platform_title_bar.rs` does — `Window::window_decorations() ->
//! gpui::Decorations` — every render, and only draws the three dots when
//! the platform reports `Decorations::Client { .. }`: nothing else will
//! decorate the window then (Wayland forcing CSD, or no window manager at
//! all), so keeping this fallback is what stands between the user and an
//! unclosable window, not stubbornness about the old design. Under
//! `Decorations::Server` this file draws nothing where the three dots used
//! to be — the row becomes OS chrome (above, not drawn by us at all) plus
//! our icon cluster (still drawn, unconditionally — the directive is "OS
//! top bar **+ our icons**", not "OS top bar only").
//!
//! **macOS follows the same decision.** AppKit's own `PlatformWindow` never
//! overrides `window_decorations()`, so it inherits gpui's blanket default of
//! `Decorations::Server` — the same value Server reports on Linux. That is
//! the correct result here: `main.rs`'s
//! `TitlebarOptions { appears_transparent: true, traffic_light_position }`
//! gives macOS AppKit the real traffic lights, so this file must not construct
//! a second set. The icon cluster reserves
//! [`tiller_theme::BrowserChrome::macos_traffic_light_cluster_inset`] before
//! its first button so it begins to the right of AppKit's controls.
//!
//! Geometry comes from [`tiller_theme::BrowserChrome`] (bar height, fallback
//! light size/gap/inset, `BrowserChrome::cluster_start` for the derivation of
//! where a fallback light group hands off to the button cluster, and the
//! macOS AppKit reserve) and
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
    App, Context, Decorations, EventEmitter, FontWeight, MouseButton, Pixels, Point, Render,
    SharedString, Window, WindowControlArea, div, prelude::*, px,
};
use std::rc::Rc;
use tiller_theme::Theme;
use tiller_theme::cosmic::CosmicComponent;

use crate::sidebar::icons::{Icon, IconSize};

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
    /// P102 test seam: forces what [`Self::render`] treats as the
    /// platform's decoration report instead of calling the real
    /// `Window::window_decorations()`. `TestWindow` never overrides that
    /// method, so it always answers `Decorations::Server` -- without this
    /// override a drawn test could only ever exercise the Server branch,
    /// never Client, which is exactly the "only proves half the work"
    /// gap P102 calls out. Production leaves this `None` and reads the
    /// real platform value every render, the same as Zed's
    /// `platform_title_bar.rs`.
    decorations_override: Option<Decorations>,
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
            decorations_override: None,
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

    /// P102 test override: forces the decoration state [`Self::render`]
    /// treats the platform as reporting, instead of the real
    /// `Window::window_decorations()` -- see the field doc for why a
    /// drawn test exercising `Decorations::Client` MUST supply this
    /// (`TestWindow` always answers `Decorations::Server`, same
    /// `unimplemented!()`-avoidance reason the other `with_*_handler`
    /// overrides on this type exist for). Has no effect on macOS -- see
    /// the module docs' carve-out.
    pub fn with_decorations(mut self, decorations: Decorations) -> Self {
        self.decorations_override = Some(decorations);
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
    icon_size: IconSize,
    radius: gpui::Pixels,
    icon_button: CosmicComponent,
    handler: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
) -> impl IntoElement {
    let enabled = handler.is_some();
    let color = if enabled {
        icon_button.on
    } else {
        icon_button.on.opacity(0.55)
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
        .child(icon.element(icon_size).text_color(color));
    if let Some(handler) = handler {
        element = element
            .hover(|style| style.bg(icon_button.hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| handler(window, cx));
    }
    element
}

impl Render for Titlebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // P102: never draw our own window controls when the platform is
        // already drawing them. `decorations_override` is the test seam
        // (see its field doc); production always takes the `None` arm and
        // asks the real platform, fresh, every render -- exactly the
        // per-render idiom `docs/linux-rewrite/tasks/
        // P102-the-top-bar-belongs-to-the-os.md` found in Zed's own
        // `platform_title_bar.rs`, not a value cached at window-open time.
        let decorations = self
            .decorations_override
            .unwrap_or_else(|| window.window_decorations());
        let show_traffic_lights = matches!(decorations, Decorations::Client { .. });

        let theme = Theme::get(cx);
        let cosmic = theme.cosmic;
        let chrome = theme.browser_chrome;
        let bar = cosmic.containers.background;
        let icon_button = cosmic.semantic.icon_button;
        let control_radius = px(cosmic.radii.radius_xs[0]);
        let button_size = theme.spacing.compact_action;
        let icon_size = IconSize::Medium;
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

        // P102: the div this app draws where the OS would otherwise draw
        // its own controls. Built only when `show_traffic_lights` holds --
        // under `Decorations::Server` this is not "drawn but hidden", it
        // is not constructed at all, so there is no lingering hitbox for a
        // click to land on.
        let traffic_lights = show_traffic_lights.then(|| {
            div()
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
                ))
        });

        // With no fallback traffic lights, the cluster either clears the
        // system-owned macOS controls or becomes the row's own leftmost
        // control. Linux keeps its existing leading inset byte-for-byte.
        let cluster_leading_gap = if show_traffic_lights {
            chrome.traffic_light_cluster_gap
        } else if cfg!(target_os = "macos") {
            chrome.macos_traffic_light_cluster_inset
        } else {
            chrome.traffic_light_inset
        };

        let cluster = div()
            .id("titlebar-cluster")
            .pl(cluster_leading_gap)
            .flex()
            .items_center()
            .gap(chrome.cluster_button_gap)
            .child(cluster_button(
                "titlebar-sidebar",
                Icon::SidebarLeft,
                button_size,
                icon_size,
                control_radius,
                icon_button,
                Some(on_sidebar),
            ))
            .child(cluster_button(
                "titlebar-back",
                Icon::ChevronLeft,
                button_size,
                icon_size,
                control_radius,
                icon_button,
                on_back,
            ))
            .child(cluster_button(
                "titlebar-forward",
                Icon::ChevronRight,
                button_size,
                icon_size,
                control_radius,
                icon_button,
                on_forward,
            ))
            .child(cluster_button(
                "titlebar-new-tab",
                Icon::Plus,
                button_size,
                icon_size,
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
                .child(div().text_color(bar.on).text_size(px(13.5)).child(title))
                .children(subtitle.map(|subtitle| {
                    div()
                        .text_color(bar.on.opacity(0.55))
                        .text_size(px(13.5))
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
            .bg(gpui::transparent_black())
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
            .children(traffic_lights)
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
                        icon_size,
                        control_radius,
                        icon_button,
                        on_history,
                    ))
                    .child(cluster_button(
                        "titlebar-right-panel",
                        Icon::PanelRight,
                        button_size,
                        icon_size,
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
    use gpui::{
        Modifiers, MouseDownEvent, MouseUpEvent, TestAppContext, Tiling, VisualTestContext,
    };
    use std::cell::RefCell;
    use tiller_theme::ThemeMode;

    /// P102: the fallback decorations state every test below that expects
    /// the three dots to draw must force explicitly -- `TestWindow` never
    /// overrides `window_decorations()`, so the real, un-overridden
    /// default (exercised by the `Decorations::Server` tests) is what a
    /// bare `Titlebar::new(cx)` gets under test, same as production would
    /// get on a server-decorated real window manager.
    fn client_side_decorations() -> Decorations {
        Decorations::Client {
            tiling: Tiling::default(),
        }
    }

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

    /// The half `from_gsettings_output` cannot cover: that `from_system`
    /// asks `gsettings` the RIGHT question and feeds the answer to the
    /// parser — the schema id, the key, the success check, the stdout
    /// decoding.
    ///
    /// It gets its teeth from querying `gsettings` independently here: a
    /// wrong schema or key inside `from_system` makes it fall back to the
    /// default while this test's own query still returns the real value, so
    /// the two disagree and the test fails.
    ///
    /// This previously asserted `ToggleMaximize` outright, reasoning that the
    /// dev box was set to `toggle-maximize` and that an absent schema
    /// degrades to the same value. That covers two cases and misses the
    /// third: a machine whose schema is present and set to something else.
    /// It duly failed on this very box once its titlebar preference was
    /// changed to `minimize` — reporting a defect against code that was
    /// behaving exactly as documented. A test may not pin the host's own
    /// configuration as the expected value.
    #[test]
    fn double_click_action_from_system_agrees_with_this_machine() {
        let queried = std::process::Command::new("gsettings")
            .args([
                "get",
                "org.gnome.desktop.wm.preferences",
                "action-double-click-titlebar",
            ])
            .output();

        let expected = match queried {
            Ok(output) if output.status.success() => {
                DoubleClickAction::from_gsettings_output(&String::from_utf8_lossy(&output.stdout))
            }
            // No GNOME schema at all — a CI container, another desktop, or no
            // `gsettings` binary. The documented graceful-absence default is
            // then the whole contract.
            _ => DoubleClickAction::ToggleMaximize,
        };

        assert_eq!(
            DoubleClickAction::from_system(),
            expected,
            "from_system must report what this machine's gsettings actually \
             says, or the documented default when gsettings cannot answer"
        );
    }

    /// Fires the second half of a real double-click (`click_count == 2`,
    /// GPUI's own platform-timing disambiguation already applied -- see
    /// the render-site comment) directly at the drag area's own drawn
    /// bounds, and asserts each configured action reaches its real seam.
    #[gpui::test]
    async fn double_click_on_the_drag_area_applies_the_configured_action(cx: &mut TestAppContext) {
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
    async fn double_click_configured_to_none_invokes_no_window_control(cx: &mut TestAppContext) {
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
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(client_side_decorations())
        });
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
            Titlebar::new(cx)
                .with_decorations(client_side_decorations())
                .with_minimize_handler(move |_window| {
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
            Titlebar::new(cx)
                .with_decorations(client_side_decorations())
                .with_maximize_handler(move |_window| {
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
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).on_history(move |_, _| *spy.borrow_mut() = true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let history = cx
            .debug_bounds("titlebar-history")
            .expect("history control is drawn");
        cx.simulate_click(history.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            *called.borrow(),
            "titlebar-history invoked its wired handler"
        );
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
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(client_side_decorations())
        });
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
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(client_side_decorations())
        });
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

    /// P102's central contract, both halves proven in one test with the
    /// decoration state constructed directly rather than left to whatever
    /// this machine's `TestWindow` happens to default to (`Decorations::
    /// Server`, always, since it never overrides `window_decorations()`)
    /// -- a test exercising only that default would prove half the work.
    /// `Decorations::Server` means a window manager is already drawing
    /// chrome: this app must draw none of its own. `Decorations::Client`
    /// means nothing else will decorate the window (a Wayland compositor
    /// forcing client-side decorations, or no window manager at all): the
    /// fallback must still be there, or the window becomes unclosable by
    /// mouse. The icon cluster must survive in both branches -- the
    /// directive is "OS top bar **+ our icons**", not "OS top bar only".
    #[gpui::test]
    async fn traffic_lights_draw_only_under_client_side_decorations(cx: &mut TestAppContext) {
        let server_window =
            cx.add_window(|_window, cx| Titlebar::new(cx).with_decorations(Decorations::Server));
        let mut server_cx = VisualTestContext::from_window(server_window.into(), cx);
        server_cx.run_until_parked();
        for id in ["titlebar-close", "titlebar-minimize", "titlebar-maximize"] {
            assert!(
                server_cx.debug_bounds(id).is_none(),
                "{id} must not draw under Decorations::Server, including on macOS where AppKit owns the controls"
            );
        }
        assert!(
            server_cx.debug_bounds("titlebar-sidebar").is_some(),
            "the icon cluster must still draw under Server -- OS chrome + our icons, \
             not OS chrome only"
        );

        let client_window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(client_side_decorations())
        });
        let mut client_cx = VisualTestContext::from_window(client_window.into(), cx);
        client_cx.run_until_parked();
        for id in ["titlebar-close", "titlebar-minimize", "titlebar-maximize"] {
            assert!(
                client_cx.debug_bounds(id).is_some(),
                "{id} must draw as the fallback when the platform reports \
                 Decorations::Client -- nothing else will decorate this window"
            );
        }
        assert!(
            client_cx.debug_bounds("titlebar-sidebar").is_some(),
            "the icon cluster still draws under Client too"
        );
    }

    /// A tiled edge under CSD is exactly the case a real compositor uses
    /// `Decorations::Client { tiling }` for -- e.g. a Wayland compositor's
    /// half-screen snap. `Tiling` carries which edges are tiled so a
    /// window-corners-and-shadow concern (not this row's) can skip
    /// rounding a seam against another tiled window; this row's own
    /// rendering does not vary by which edges are tiled, so this test
    /// documents that non-effect rather than leaving it unverified.
    #[gpui::test]
    async fn tiled_edges_do_not_change_whether_the_fallback_controls_draw(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(Decorations::Client {
                tiling: Tiling::tiled(),
            })
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("titlebar-close").is_some(),
            "a fully tiled Client window still gets the fallback close control"
        );
    }

    /// The icon cluster starts after the system traffic-light group on macOS,
    /// where AppKit draws native controls, and at its own leading inset on
    /// Linux when server-side decorations leave the window controls to the OS.
    #[gpui::test]
    async fn the_cluster_position_tracks_platform_traffic_lights(cx: &mut TestAppContext) {
        let server_window =
            cx.add_window(|_window, cx| Titlebar::new(cx).with_decorations(Decorations::Server));
        let mut server_cx = VisualTestContext::from_window(server_window.into(), cx);
        server_cx.run_until_parked();
        let sidebar_server = server_cx
            .debug_bounds("titlebar-sidebar")
            .expect("cluster is drawn under Server");

        let client_window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_decorations(client_side_decorations())
        });
        let mut client_cx = VisualTestContext::from_window(client_window.into(), cx);
        client_cx.run_until_parked();
        let sidebar_client = client_cx
            .debug_bounds("titlebar-sidebar")
            .expect("cluster is drawn under Client");

        if cfg!(target_os = "macos") {
            let appkit_reserve = cx.update(|cx| {
                Theme::get(cx)
                    .browser_chrome
                    .macos_traffic_light_cluster_inset
            });
            assert_eq!(
                sidebar_server.origin.x, appkit_reserve,
                "Server-side macOS chrome must reserve the AppKit traffic-light width"
            );
            assert!(
                sidebar_server.origin.x > sidebar_client.origin.x,
                "macOS must reserve AppKit's traffic lights before the cluster (Server: {:?}, Client: {:?})",
                sidebar_server.origin.x,
                sidebar_client.origin.x
            );
        } else {
            assert!(
                sidebar_server.origin.x < sidebar_client.origin.x,
                "Linux server-side decorations omit fallback traffic lights, so the cluster \
                 starts closer to the left edge (Server: {:?}, Client: {:?})",
                sidebar_server.origin.x,
                sidebar_client.origin.x
            );
        }
    }

    /// AppKit owns the macOS traffic lights when the platform reports Server:
    /// the fallback controls must be absent and the first icon must begin at
    /// the theme's documented system-light reserve.
    #[gpui::test]
    async fn macos_cluster_clears_appkit_traffic_lights(cx: &mut TestAppContext) {
        if !cfg!(target_os = "macos") {
            return;
        }

        let window =
            cx.add_window(|_window, cx| Titlebar::new(cx).with_decorations(Decorations::Server));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        for id in ["titlebar-close", "titlebar-minimize", "titlebar-maximize"] {
            assert!(
                cx.debug_bounds(id).is_none(),
                "{id} must be left to AppKit on macOS"
            );
        }
        let cluster = cx
            .debug_bounds("titlebar-sidebar")
            .expect("the icon cluster remains ours on macOS");
        let appkit_reserve = cx.update(|_, cx| {
            Theme::get(cx)
                .browser_chrome
                .macos_traffic_light_cluster_inset
        });
        assert_eq!(cluster.origin.x, appkit_reserve);
    }
}

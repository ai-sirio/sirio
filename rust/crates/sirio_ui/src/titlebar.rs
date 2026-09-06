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
//! [`sirio_theme::BrowserChrome::macos_traffic_light_cluster_inset`] before
//! its first button so it begins to the right of AppKit's controls.
//!
//! **Windows is the one platform where Server does not mean "the OS drew
//! it".** gpui_windows suppresses the native caption when
//! `TitlebarOptions.appears_transparent` is set (which `main.rs` sets) and,
//! like AppKit, never overrides `window_decorations()` — so the trait default
//! answers `Server` for a window that in fact has *no* window controls at
//! all. Taking that report at face value left Windows builds with no way to
//! close/minimize/maximize by mouse. Windows therefore gets **the classic
//! Windows caption buttons at the trailing edge**, not a second set of
//! macOS-style dots: see [`WindowControls::WindowsCaption`] and
//! [`caption_button`].
//!
//! # One value, not two compile-time branches
//!
//! What this row draws is [`WindowControls`], resolved once per render from
//! the platform's decoration report and this build's host. It replaced a
//! pair of `cfg!(target_os = ...)` branches — one for whether to draw our
//! own controls, one for where the icon cluster starts — and the single
//! remaining production `cfg!` is confined to `HostPlatform::current`.
//!
//! That is not tidiness. A `cfg!` is not a seam: you cannot alter its
//! behaviour without editing at that spot, so the tests around it had to
//! branch on `cfg!` too, and could only ever verify the arm they were
//! compiled into. One of them (`macos_cluster_clears_appkit_traffic_lights`)
//! opened with an early `return` on every non-macOS host and asserted
//! nothing at all on the Linux CI. Driving [`WindowControls`] through
//! [`Titlebar::with_window_controls`] lets every host exercise all four
//! outcomes.
//!
//! Geometry comes from [`sirio_theme::BrowserChrome`] (bar height, fallback
//! light size/gap/inset, `BrowserChrome::cluster_start` for the derivation of
//! where a fallback light group hands off to the button cluster, and the
//! macOS AppKit reserve), [`sirio_theme::WindowsCaption`] (caption-button
//! width and glyph size) and
//! `Spacing::compact_action` (the 24px cluster-button frame). Colour comes
//! from `Theme::get(cx)` like every other surface: the row itself paints
//! *no* fill and lets the window surface through (a deliberate choice,
//! because the screenshot wants **one continuous surface**, no seam between
//! chrome and content), its text is the theme's `text`, the
//! lights are the theme's own `danger`/`warning`/`success` (already red/
//! amber/green — no new colour tokens needed), and the cluster buttons read
//! [`IconButtonColors`], which is `text` on `element_hover`/`element_active`.
//! The caption buttons read it too, except for the close button's red, which
//! is a *system* constant and lives in [`sirio_theme::WindowsCaption`] —
//! that type's docs explain why a theme `danger` cannot stand in for it.
//!
//! Row layout, left to right: traffic lights (Linux CSD only) → cluster
//! (sidebar toggle, back, forward, `+`) → accent dot + title → muted
//! subtitle → (spacer) → history, right-panel toggle → the Windows caption
//! buttons (Windows only), flush to the trailing edge. The far-right
//! scope/branch pills and collapse/expand controls from the screenshot are
//! **not built** — see the P76 report for why.
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
    Rgba, SharedString, Window, WindowControlArea, div, prelude::*, px,
};
use bezel::theme::Theme as BezelTheme;
use sirio_theme::{BrowserChrome, Theme, WindowsCaption};
use std::rc::Rc;
use std::sync::OnceLock;

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
    /// The platform seam: forces what [`Self::render`] draws where the
    /// window controls belong, instead of resolving it from the real
    /// `Window::window_decorations()` and this build's host. `TestWindow`
    /// never overrides `window_decorations()`, so it always answers
    /// `Decorations::Server`, and the host is fixed at compile time --
    /// without this override a drawn test could only ever exercise the one
    /// combination it was compiled into. Production leaves this `None` and
    /// re-resolves every render, the same per-render idiom as Zed's
    /// `platform_title_bar.rs`, not a value cached at window-open time.
    window_controls_override: Option<WindowControls>,
    /// How many times gpui asked this view to render. Test-observable only:
    /// the host caches the bar, and a still title bar must not render at
    /// all while a spinner elsewhere keeps the window drawing.
    render_count: u64,
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
            window_controls_override: None,
            render_count: 0,
        }
    }

    /// How many times this view has rendered. Only a test should read it:
    /// it exists so the host can prove a cached, still bar is reused across
    /// frames rather than re-rendered.
    pub fn render_count(&self) -> u64 {
        self.render_count
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

    /// Test/host override for the platform seam -- see the field doc for
    /// why a drawn test exercising anything other than this host's own
    /// resolution MUST supply one. Replaces the former
    /// `with_decorations`: `Decorations` alone could not express what this
    /// row draws, because the answer also depended on a `cfg!` no test
    /// could cross.
    pub fn with_window_controls(mut self, controls: WindowControls) -> Self {
        self.window_controls_override = Some(controls);
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

/// Which platform this build runs on.
///
/// This type exists so [`WindowControls::resolve`] can be a **pure
/// function**. A `cfg!` is not a seam — you cannot alter its behaviour
/// without editing at that spot — so the tests that used to branch on
/// `cfg!` could only ever verify the one branch they happened to be
/// compiled into, and the macOS assertions never ran anywhere. Confining
/// the single production `cfg!` to [`Self::current`] leaves everything
/// above it exercisable for every host, from any host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPlatform {
    Linux,
    Macos,
    Windows,
}

impl HostPlatform {
    /// The one `cfg!` left in this module's production path.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// What this row draws where the window controls belong, and what it must
/// reserve for controls somebody else draws.
///
/// One value answers both questions the platform used to answer through
/// two separate compile-time branches — whether to draw our own controls,
/// and where the icon cluster's leading edge sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControls {
    /// A window manager drew its own titlebar *above* this row: draw
    /// nothing, reserve nothing. P102's central case on Linux.
    OsDrawnAbove,
    /// AppKit owns the real controls, and draws them *inside* this row's
    /// left edge: draw nothing, but reserve their width so our cluster
    /// begins clear of them.
    MacosNative,
    /// Nothing else will decorate this window — a Wayland compositor
    /// forcing client-side decorations, or no window manager at all. Our
    /// three dots at the leading edge are then the only controls the
    /// window will ever have; without them it cannot be closed by mouse.
    TrafficLights,
    /// The platform suppressed its own caption: the Windows caption
    /// buttons at the **trailing** edge. gpui_windows hides the native
    /// caption when `TitlebarOptions.appears_transparent` is set (which
    /// `main.rs` sets) and never overrides
    /// `PlatformWindow::window_decorations()`, so it answers the trait
    /// default `Server` for a window that in fact has no controls at all —
    /// which is why Windows resolves here under *either* decoration
    /// report.
    WindowsCaption,
}

impl WindowControls {
    /// The full mapping, stated host-independently. Windows collapses both
    /// decoration reports onto the same answer (see
    /// [`Self::WindowsCaption`]); on the other two platforms `Client`
    /// means "nothing else will decorate this window" and `Server` means
    /// the opposite, exactly as P102 established.
    fn resolve(decorations: Decorations, host: HostPlatform) -> Self {
        match host {
            HostPlatform::Windows => Self::WindowsCaption,
            HostPlatform::Macos => match decorations {
                Decorations::Server => Self::MacosNative,
                Decorations::Client { .. } => Self::TrafficLights,
            },
            HostPlatform::Linux => match decorations {
                Decorations::Server => Self::OsDrawnAbove,
                Decorations::Client { .. } => Self::TrafficLights,
            },
        }
    }

    /// The left padding applied to the icon cluster. Note this is a *gap*
    /// under [`Self::TrafficLights`] — the dots are drawn as a preceding
    /// sibling, so the cluster's own edge lands at
    /// [`BrowserChrome::cluster_start`] — and an absolute inset from the
    /// window edge in every other case, where nothing precedes it.
    fn cluster_leading_gap(self, chrome: &BrowserChrome) -> Pixels {
        match self {
            Self::TrafficLights => chrome.traffic_light_cluster_gap,
            Self::MacosNative => chrome.macos_traffic_light_cluster_inset,
            Self::OsDrawnAbove | Self::WindowsCaption => chrome.traffic_light_inset,
        }
    }
}

/// Segoe's own caption glyphs, the same code points Windows draws in its
/// native caption and the ones Zed's `platform_windows.rs` uses.
const GLYPH_MINIMIZE: &str = "\u{e921}";
const GLYPH_MAXIMIZE: &str = "\u{e922}";
const GLYPH_RESTORE: &str = "\u{e923}";
const GLYPH_CLOSE: &str = "\u{e8bb}";

const SEGOE_FLUENT_ICONS: &str = "Segoe Fluent Icons";
const SEGOE_MDL2_ASSETS: &str = "Segoe MDL2 Assets";

/// Resolved once per process — [`gpui::TextSystem::all_font_names`] builds
/// and sorts a `Vec<String>` of every installed family, which must not run
/// on every render.
static CAPTION_FONT_FAMILY: OnceLock<&'static str> = OnceLock::new();

/// Which Segoe icon family carries the caption glyphs on this machine.
///
/// Zed picks this from the OS build number via `RtlGetVersion`. Asking the
/// text system which families it actually has answers the real question
/// instead of a proxy for it — a slimmed Windows image or a future family
/// rename breaks the proxy but not this — and needs neither the `windows`
/// crate nor an `unsafe` block.
///
/// The fallback is **not** a safety net: `font_fallbacks` maps *code
/// points* to families and is only built once the base family has already
/// resolved, so a family that is not installed renders as tofu rather than
/// falling through. Measured; see the prototype on
/// `prototype/caption-glyphs`. Picking the family up front is what avoids
/// that, and if neither family exists the glyphs draw as tofu while the
/// buttons keep working — their hit areas are geometric, so the window
/// stays closable.
fn caption_font_family(cx: &App) -> &'static str {
    CAPTION_FONT_FAMILY.get_or_init(|| {
        if cx
            .text_system()
            .all_font_names()
            .iter()
            .any(|name| name == SEGOE_FLUENT_ICONS)
        {
            SEGOE_FLUENT_ICONS
        } else {
            SEGOE_MDL2_ASSETS
        }
    })
}

/// The middle caption button shows "restore" once the window is maximized,
/// the same swap Windows itself makes. Pure, because `TestWindow::
/// is_maximized` is hardcoded to `false` — a drawn test can never reach
/// the other state, and widening `Titlebar`'s interface with an override
/// for one `bool` would cost more than it proves.
fn maximize_glyph(is_maximized: bool) -> &'static str {
    if is_maximized {
        GLYPH_RESTORE
    } else {
        GLYPH_MAXIMIZE
    }
}

/// The hover shade of a traffic light.
///
/// COSMIC shipped a hand-picked hover for each semantic colour, about 12%
/// darker than its resting fill (`#FFA09A` -> `#E0948F`). The theme carries
/// one value per meaning and no interaction states, so the relationship is
/// stated here as the arithmetic COSMIC's own pairs describe rather than
/// re-picked by eye.
const LIGHT_HOVER_SHADE: f32 = 0.88;

fn darkened(color: Rgba, factor: f32) -> Rgba {
    Rgba {
        r: color.r * factor,
        g: color.g * factor,
        b: color.b * factor,
        a: color.a,
    }
}

/// One traffic-light dot: a real, circular window control, not a decoration.
/// `fill` is the theme's semantic colour for the action (`danger`/`warning`/
/// `success` — already red/amber/green, so this needs no new colour tokens).
fn traffic_light(
    id: &'static str,
    area: WindowControlArea,
    diameter: gpui::Pixels,
    fill: Rgba,
    handler: Rc<dyn Fn(&mut Window)>,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .window_control_area(area)
        .w(diameter)
        .h(diameter)
        .rounded(diameter * 0.5)
        .bg(fill)
        .hover(|style| style.bg(darkened(fill, LIGHT_HOVER_SHADE)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, _| handler(window))
}

/// The three colours a chrome icon button draws with.
///
/// This replaces COSMIC's `Component`, which carried six fields where this
/// file read three. It is local rather than a theme type because nothing
/// outside the titlebar draws a control with its own resting/hover/pressed
/// set — the rest of the app composes those from `element_hover` and
/// `element_active` at the call site, which is what this does too.
#[derive(Clone, Copy)]
struct IconButtonColors {
    /// The glyph colour, drawn on the bar itself — these buttons have no
    /// resting fill.
    on: Rgba,
    /// Fill on hover.
    hover: Rgba,
    /// Fill while pressed.
    pressed: Rgba,
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
    icon_button: IconButtonColors,
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

/// One Windows caption button.
///
/// **No `on_click`, and no `stop_propagation`** — deliberately, and this is
/// the one place in this file where that is the correct shape.
///
/// On Windows the platform acts on these itself. `window_control_area`
/// turns the element into a non-client area
/// (`gpui_windows::events::handle_hit_test_msg` maps it to `HTCLOSE` /
/// `HTMAXBUTTON` / `HTMINBUTTON`), and `handle_nc_mouse_up_msg` then runs
/// `ShowWindowAsync(SW_MINIMIZE)`, the `SW_MAXIMIZE`/`SW_NORMAL` toggle,
/// and `PostMessageW(WM_CLOSE)`.
///
/// Adding our own handler *as well* fires both, so a click would maximize
/// and immediately restore. Wiring one *instead* is worse than it sounds:
/// `Window::zoom_window` reaches `WindowsWindow::zoom`, which is
/// `ShowWindowAsync(SW_MAXIMIZE)` **unconditionally**, and gpui exposes no
/// `restore` — a hand-wired maximize button could never un-maximize
/// without pulling the `windows` crate and an `unsafe` block into this
/// crate. `.occlude()` is what Zed uses here for the same reason: it
/// blocks the drag area underneath without consuming the platform's own
/// handling.
///
/// Declaring the area is not only about clicks: `HTMAXBUTTON` is what
/// raises Windows 11's Snap Layouts flyout on hover.
///
/// Only the close button departs from the theme's own `icon_button`
/// colours, because only its red is a system constant — see
/// [`sirio_theme::WindowsCaption`].
#[allow(clippy::too_many_arguments)]
fn caption_button(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    enabled: bool,
    is_close: bool,
    caption: WindowsCaption,
    bar_height: Pixels,
    icon_button: IconButtonColors,
) -> impl IntoElement {
    let (hover_bg, hover_on, pressed_bg, pressed_on) = if is_close {
        (
            caption.close_hover,
            caption.close_on,
            caption.close_pressed,
            caption.close_on,
        )
    } else {
        (
            icon_button.hover,
            icon_button.on,
            icon_button.pressed,
            icon_button.on,
        )
    };
    // The same muting `cluster_button` uses for an unwired seam — one
    // idiom for "this control is present but cannot act", not a second.
    let resting_on = if enabled {
        icon_button.on
    } else {
        icon_button.on.opacity(0.55)
    };

    let mut element = div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .occlude()
        .window_control_area(area)
        .w(caption.button_width)
        .h(bar_height)
        .flex()
        .items_center()
        .justify_center()
        .text_size(caption.glyph_size)
        .text_color(resting_on)
        .child(glyph);
    if enabled {
        element = element
            .hover(|style| style.bg(hover_bg).text_color(hover_on))
            .active(|style| style.bg(pressed_bg).text_color(pressed_on));
    }
    element
}

impl Render for Titlebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("Titlebar.render", cx.entity_id().as_u64());
        self.render_count = self.render_count.wrapping_add(1);
        // P102: never draw our own window controls when the platform is
        // already drawing them. `decorations_override` is the test seam
        // (see its field doc); production always takes the `None` arm and
        // asks the real platform, fresh, every render -- exactly the
        // per-render idiom `docs/linux-rewrite/tasks/
        // P102-the-top-bar-belongs-to-the-os.md` found in Zed's own
        // `platform_title_bar.rs`, not a value cached at window-open time.
        let controls = self.window_controls_override.unwrap_or_else(|| {
            WindowControls::resolve(window.window_decorations(), HostPlatform::current())
        });

        let theme = Theme::get(cx);
        let chrome = theme.browser_chrome;
        let caption = theme.windows_caption;
        let bar_on = theme.text;
        let icon_button = IconButtonColors {
            on: theme.text,
            hover: theme.element_hover,
            pressed: theme.element_active,
        };
        let control_radius = px(BezelTheme::BASE_RADIUS * 0.5); // 4.0
        let button_size = theme.spacing.compact_action;
        let icon_size = IconSize::Medium;
        let trailing_inset = px(BezelTheme::SPACE_MD); // 12.0
        let caption_family = caption_font_family(cx);
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
        let traffic_lights = (controls == WindowControls::TrafficLights).then(|| {
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
                    theme.danger,
                    on_close,
                ))
                .child(traffic_light(
                    "titlebar-minimize",
                    WindowControlArea::Min,
                    chrome.traffic_light_diameter,
                    theme.warning,
                    on_minimize,
                ))
                .child(traffic_light(
                    "titlebar-maximize",
                    WindowControlArea::Max,
                    chrome.traffic_light_diameter,
                    theme.success,
                    on_maximize,
                ))
        });

        // With no fallback traffic lights, the cluster either clears the
        // system-owned macOS controls or becomes the row's own leftmost
        // control. Linux keeps its existing leading inset byte-for-byte.
        let cluster_leading_gap = controls.cluster_leading_gap(&chrome);

        // The Windows caption buttons ride at the trailing edge, flush to
        // the window border: no right padding, so the top-right corner
        // stays clickable edge-to-edge (Fitts). Built only under
        // `WindowsCaption` -- under every other value they are not
        // constructed at all, so no lingering hitbox is left behind, the
        // same discipline the traffic lights follow above.
        let caption_buttons = (controls == WindowControls::WindowsCaption).then(|| {
            div()
                .id("titlebar-window-caption")
                .debug_selector(|| "titlebar-window-caption".to_owned())
                .flex()
                .items_center()
                .font_family(caption_family)
                .child(caption_button(
                    "titlebar-minimize",
                    GLYPH_MINIMIZE,
                    WindowControlArea::Min,
                    window.is_minimizable(),
                    false,
                    caption,
                    chrome.bar_height,
                    icon_button,
                ))
                .child(caption_button(
                    "titlebar-maximize",
                    maximize_glyph(window.is_maximized()),
                    WindowControlArea::Max,
                    window.is_resizable(),
                    false,
                    caption,
                    chrome.bar_height,
                    icon_button,
                ))
                .child(caption_button(
                    "titlebar-close",
                    GLYPH_CLOSE,
                    WindowControlArea::Close,
                    true,
                    true,
                    caption,
                    chrome.bar_height,
                    icon_button,
                ))
        });

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
                    // The dot beside the title used to be the COSMIC desktop
                    // accent. On Linux that is the user's own choice, but the
                    // other two platforms have no COSMIC to ask and fell back
                    // to the transcribed cyan — a colour nobody picked,
                    // marking nothing. It takes the titlebar's own foreground,
                    // the same neutral the title text beside it already uses,
                    // which also keeps this surface inside one palette instead
                    // of mixing the COSMIC roles with the shell's.
                    div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(bar_on),
                )
                .child(
                    div()
                        .text_color(bar_on)
                        .text_size(theme.typography.scaled(13.5))
                        .child(title),
                )
                .children(subtitle.map(|subtitle| {
                    div()
                        .text_color(bar_on.opacity(0.55))
                        .text_size(theme.typography.scaled(13.5))
                        .child(subtitle)
                }))
        });

        div()
            .id("sirio-titlebar")
            .debug_selector(|| "sirio-titlebar".to_owned())
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
            .children(caption_buttons)
            .font_weight(FontWeight::NORMAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Modifiers, MouseDownEvent, MouseUpEvent, TestAppContext, Tiling, VisualTestContext,
    };
    use sirio_theme::ThemeMode;
    use std::cell::RefCell;

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

    /// The full decoration × host mapping, as a table.
    ///
    /// **Every row runs on every host.** That is the whole reason
    /// [`WindowControls::resolve`] is pure: this assertion used to read
    /// `assert_eq!(show_window_controls(&Server), cfg!(target_os =
    /// "windows"))`, which compared the code against itself and could only
    /// ever exercise the branch it was compiled into.
    #[test]
    fn window_controls_resolve_covers_every_host_and_decoration_report() {
        let table = [
            (
                HostPlatform::Linux,
                Decorations::Server,
                WindowControls::OsDrawnAbove,
            ),
            (
                HostPlatform::Linux,
                client_side_decorations(),
                WindowControls::TrafficLights,
            ),
            (
                HostPlatform::Macos,
                Decorations::Server,
                WindowControls::MacosNative,
            ),
            (
                HostPlatform::Macos,
                client_side_decorations(),
                WindowControls::TrafficLights,
            ),
            // Windows collapses both reports: gpui_windows suppresses the
            // native caption yet still answers the trait default `Server`,
            // so a window that reports Server there has no controls at all.
            (
                HostPlatform::Windows,
                Decorations::Server,
                WindowControls::WindowsCaption,
            ),
            (
                HostPlatform::Windows,
                client_side_decorations(),
                WindowControls::WindowsCaption,
            ),
        ];

        for (index, (host, decorations, expected)) in table.into_iter().enumerate() {
            assert_eq!(
                WindowControls::resolve(decorations, host),
                expected,
                "row {index}: {host:?}"
            );
        }
    }

    /// The glyph swap Windows itself makes once the window is maximized.
    /// Pure, because `TestWindow::is_maximized` is hardcoded to `false` --
    /// no drawn test can reach the other state.
    #[test]
    fn the_maximize_glyph_swaps_to_restore_when_maximized() {
        assert_eq!(maximize_glyph(false), GLYPH_MAXIMIZE);
        assert_eq!(maximize_glyph(true), GLYPH_RESTORE);
        assert_ne!(
            GLYPH_MAXIMIZE, GLYPH_RESTORE,
            "the two states must be visually distinguishable"
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
            .debug_bounds("sirio-titlebar")
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
            .debug_bounds("sirio-titlebar")
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
            .debug_bounds("sirio-titlebar")
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
            .debug_bounds("sirio-titlebar")
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
            Titlebar::new(cx).with_window_controls(WindowControls::TrafficLights)
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
                .with_window_controls(WindowControls::TrafficLights)
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
                .with_window_controls(WindowControls::TrafficLights)
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
                .with_title("Sirio")
                .with_subtitle("sirio-linux @ linux/gpui-waku")
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
            Titlebar::new(cx).with_window_controls(WindowControls::TrafficLights)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let appearance = cx.update(|_, cx| Theme::get(cx).appearance);
        assert_eq!(appearance, sirio_theme::Appearance::Dark);

        let bar_height = cx.update(|_, cx| Theme::get(cx).browser_chrome.bar_height);
        let close = cx
            .debug_bounds("titlebar-close")
            .expect("close control is drawn under the dark theme");
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
            Titlebar::new(cx).with_window_controls(WindowControls::TrafficLights)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx.update(|_, cx| Theme::get(cx).surface);
        assert_eq!(
            cx.update(|_, cx| Theme::get(cx).appearance),
            sirio_theme::Appearance::Light
        );
        assert_ne!(
            bar,
            Theme::dark().surface,
            "light and dark bar surfaces must not collapse to the same fill"
        );

        let close = cx
            .debug_bounds("titlebar-close")
            .expect("close control is drawn under the light theme");
        assert_eq!(
            close.size.height,
            cx.update(|_, cx| Theme::get(cx).browser_chrome.traffic_light_diameter)
        );
    }

    /// P102's central contract, now stated for **all four** outcomes
    /// instead of the two a given host happened to compile.
    ///
    /// `OsDrawnAbove` means a window manager is already drawing chrome:
    /// this app must draw none of its own. `MacosNative` leaves the real
    /// controls to AppKit, likewise drawing none. `TrafficLights` means
    /// nothing else will ever decorate this window, so the fallback must
    /// be there or it becomes unclosable by mouse. `WindowsCaption` draws
    /// its own for the same reason.
    ///
    /// The icon cluster must survive every one of them -- the directive is
    /// "OS top bar **+ our icons**", not "OS top bar only".
    #[gpui::test]
    async fn what_the_row_draws_follows_the_resolved_window_controls(cx: &mut TestAppContext) {
        for (controls, draws_our_own) in [
            (WindowControls::OsDrawnAbove, false),
            (WindowControls::MacosNative, false),
            (WindowControls::TrafficLights, true),
            (WindowControls::WindowsCaption, true),
        ] {
            let window =
                cx.add_window(|_window, cx| Titlebar::new(cx).with_window_controls(controls));
            let mut window_cx = VisualTestContext::from_window(window.into(), cx);
            window_cx.run_until_parked();

            for id in ["titlebar-close", "titlebar-minimize", "titlebar-maximize"] {
                assert_eq!(
                    window_cx.debug_bounds(id).is_some(),
                    draws_our_own,
                    "{id} under {controls:?}"
                );
            }
            assert!(
                window_cx.debug_bounds("titlebar-sidebar").is_some(),
                "the icon cluster must draw under {controls:?} -- OS chrome + our icons, \
                 not OS chrome only"
            );
        }
    }

    /// A tiled edge under CSD is exactly the case a real compositor uses
    /// `Decorations::Client { tiling }` for -- e.g. a Wayland compositor's
    /// half-screen snap. `Tiling` carries which edges are tiled so a
    /// window-corners-and-shadow concern (not this row's) can skip
    /// rounding a seam against another tiled window; this row's own
    /// rendering does not vary by which edges are tiled, so this test
    /// documents that non-effect rather than leaving it unverified.
    #[test]
    fn tiled_edges_do_not_change_what_the_row_draws() {
        assert_eq!(
            WindowControls::resolve(
                Decorations::Client {
                    tiling: Tiling::tiled(),
                },
                HostPlatform::Linux,
            ),
            WindowControls::TrafficLights,
            "a fully tiled Client window still gets the fallback controls"
        );
        assert_eq!(
            WindowControls::resolve(
                Decorations::Client {
                    tiling: Tiling::tiled(),
                },
                HostPlatform::Macos,
            ),
            WindowControls::TrafficLights
        );
    }

    /// Where the icon cluster's leading edge lands, for all four outcomes.
    ///
    /// The expected values are the literals `sirio_theme`'s own
    /// `browser_chrome_matches_the_comet_measured_spec` pins, not a second
    /// call into `cluster_leading_gap` -- a test that recomputes the
    /// expectation the way the code does can never disagree with it.
    #[gpui::test]
    async fn the_cluster_position_follows_the_resolved_window_controls(cx: &mut TestAppContext) {
        for (controls, expected) in [
            // Nothing precedes the cluster: it starts at the row's own inset.
            (WindowControls::OsDrawnAbove, px(10.0)),
            // The caption buttons live at the *trailing* edge, so the
            // leading edge is untouched -- the same x as OsDrawnAbove.
            (WindowControls::WindowsCaption, px(10.0)),
            // AppKit's own controls are reserved before ours.
            (WindowControls::MacosNative, px(80.0)),
            // Our dots precede it: 10 + 3*12 + 2*8 + 8 = 70.
            (WindowControls::TrafficLights, px(70.0)),
        ] {
            let window =
                cx.add_window(|_window, cx| Titlebar::new(cx).with_window_controls(controls));
            let mut window_cx = VisualTestContext::from_window(window.into(), cx);
            window_cx.run_until_parked();

            let cluster = window_cx
                .debug_bounds("titlebar-sidebar")
                .unwrap_or_else(|| panic!("cluster is drawn under {controls:?}"));
            assert_eq!(
                cluster.origin.x, expected,
                "cluster leading edge under {controls:?}"
            );
        }
    }

    /// AppKit owns the macOS traffic lights: our fallback must be absent
    /// and the first icon must begin at the theme's documented reserve.
    ///
    /// This test used to open with `if !cfg!(target_os = "macos") {
    /// return; }`, which made it a no-op that asserted nothing on Linux,
    /// on Windows, and on CI -- a green test proving nothing anywhere the
    /// project actually builds. Driving the outcome through the seam is
    /// what lets it run.
    #[gpui::test]
    async fn macos_cluster_clears_appkit_traffic_lights(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_window_controls(WindowControls::MacosNative)
        });
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

    /// The Windows caption buttons: three of them, in Windows' own order,
    /// each the theme's `button_width` wide, filling the row's height, and
    /// **flush to the trailing edge**. The flush edge is not cosmetic --
    /// a padded corner cannot be hit by throwing the pointer into it,
    /// which is how a Windows user closes a window.
    #[gpui::test]
    async fn windows_caption_buttons_sit_flush_at_the_trailing_edge(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| {
            Titlebar::new(cx).with_window_controls(WindowControls::WindowsCaption)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx
            .debug_bounds("sirio-titlebar")
            .expect("titlebar is drawn");
        let (caption, bar_height) = cx.update(|_, cx| {
            let theme = Theme::get(cx);
            (theme.windows_caption, theme.browser_chrome.bar_height)
        });

        let minimize = cx
            .debug_bounds("titlebar-minimize")
            .expect("minimize is drawn");
        let maximize = cx
            .debug_bounds("titlebar-maximize")
            .expect("maximize is drawn");
        let close = cx.debug_bounds("titlebar-close").expect("close is drawn");

        for (name, bounds) in [
            ("minimize", minimize),
            ("maximize", maximize),
            ("close", close),
        ] {
            assert_eq!(bounds.size.width, caption.button_width, "{name} width");
            assert_eq!(
                bounds.size.height, bar_height,
                "{name} fills the row height"
            );
        }

        assert!(
            minimize.origin.x < maximize.origin.x && maximize.origin.x < close.origin.x,
            "Windows' own order is minimize, maximize, close (got {:?}, {:?}, {:?})",
            minimize.origin.x,
            maximize.origin.x,
            close.origin.x
        );
        assert_eq!(
            close.origin.x + close.size.width,
            bar.origin.x + bar.size.width,
            "the close button must reach the window's trailing edge"
        );

        let right_panel = cx
            .debug_bounds("titlebar-right-panel")
            .expect("the app's own trailing icons survive");
        assert!(
            right_panel.origin.x + right_panel.size.width <= minimize.origin.x,
            "the app's icons stay to the left of the window controls"
        );
    }
}

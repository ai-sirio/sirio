//! Sirio's shared color, spacing, and typography tokens.
//!
//! Sirio keeps its chrome quiet through a compact cool-tinted shell hierarchy:
//! a translucent frame surrounds opaque panel surfaces, selected rows, and
//! shell borders. Within that hierarchy, generic hover, pressed, and divider
//! washes remain neutral veils; focus and active chrome are spelled with
//! contrast rather than hue, and semantic hues retain their existing state
//! meanings. Colour is spent on two things only: data (a diff, a git status, an
//! agent's brand) and attention (needs-input, error). The brand coral no longer
//! has a role — see [`SirioColors::brand_coral`].
//!
//! Where each value comes from is recorded in
//! `docs/THEME-PROVENANCE.md`. The re-runnable
//! `./Scripts/measure-theme.py` script reproduces the **historical**
//! Waku measurements only. Current shell values are audited there through the
//! IntelliJ screenshot fingerprint, dimensions, sampling method, and pixel
//! rectangles. Tokens the frames cannot settle say so at their own definition
//! and name our source instead.
//!
//! A resolved [`Theme`] is installed as a GPUI global so views can retrieve
//! the same tokens from their render context.
//!
//! # Dark mode
//!
//! On Linux, GPUI resolves the system appearance through the XDG desktop
//! portal's `color-scheme` setting, and the answer arrives *asynchronously*
//! (it is even absent when no portal runs). GPUI's `window_appearance()`
//! therefore reports `Light` until the portal has been heard from — which
//! made Sirio start in light on every portal-less Linux session. Sirio
//! queries the portal itself (`ashpd`, the same crate gpui_linux uses) and
//! treats a missing or silent portal as **dark**, not light. macOS keeps the
//! old behavior: `NSAppearance` is synchronous and authoritative there.

use gpui::{App, FontWeight, Global, Hsla, Pixels, Rgba, Size, WindowAppearance, px, rgb, size};
use std::borrow::Cow;
use std::collections::HashSet;

use std::ops::Deref;
use std::sync::OnceLock;

/// The appearance selected by Sirio's appearance setting.
///
/// This is bezel's enum under Sirio's name. It used to be a third
/// `System`/`Light`/`Dark` declaration beside bezel's and the persisted one;
/// the name is kept because it reads better next to `Appearance` (the
/// *resolved* one) and because it keeps `sirio_persistence::AppearanceMode`
/// unambiguous at the one place both are in scope, `sirio`'s `main.rs`.
mod base_color;
mod presets;

pub use ely_palette;

pub use base_color::BaseColor;

pub use bezel_theme::appearance::AppearanceMode as ThemeMode;

/// The resolved light/dark appearance of an active [`Theme`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    /// The light palette.
    Light,
    /// The dark palette.
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
        }
    }
}

/// Resolves a selected mode against the window appearance.
///
/// A free function rather than an inherent method: [`ThemeMode`] is bezel's
/// type now, so this crate cannot add methods to it.
fn resolve_mode(mode: ThemeMode, system_appearance: WindowAppearance) -> Appearance {
    match mode {
        ThemeMode::System => system_appearance.into(),
        ThemeMode::Light => Appearance::Light,
        ThemeMode::Dark => Appearance::Dark,
    }
}

/// Linux-specific resolution for `System`.
///
/// GPUI's `WindowAppearance` starts at `Light` and only becomes meaningful
/// once the XDG portal answers — or never, when no portal is running. A light
/// appearance at startup is therefore *not evidence of a light desktop*: it is
/// the unresolved default. Resolve it to dark, and let the portal query in
/// [`Theme::init`] correct to light when the portal actually says so.
#[cfg(target_os = "linux")]
fn resolve_mode_linux(mode: ThemeMode, system_appearance: WindowAppearance) -> Appearance {
    match mode {
        ThemeMode::System => match system_appearance {
            // The portal (or another platform channel) has spoken.
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
            // Light here means "not heard from yet" or "no portal".
            // Dark wins either way.
            _ => Appearance::Dark,
        },
        ThemeMode::Light => Appearance::Light,
        ThemeMode::Dark => Appearance::Dark,
    }
}

/// The colours Ely's `Palette` has no field for, or whose value differs
/// from the Ely field of the same meaning (spec §3.2). Each field's doc says
/// where it is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SirioColors {
    /// Opaque fallback behind the app shell and window canvas, and behind the
    /// working columns when translucency is unavailable.
    pub canvas: Hsla,
    /// Translucent window-frame material. Its RGB value is paired with
    /// [`SirioColors::canvas`] for platforms without translucency.
    pub frame_surface: Hsla,
    /// Terminal surface — the pane surface in dark mode and paper-white in
    /// light mode. Kept under its own name because the terminal renderer also
    /// uses it as the ANSI default background.
    pub terminal_surface: Hsla,
    /// A sheet an event opens over the shell and that reads like a panel of
    /// its own: the New Worktree prompt and the Clone/Create project forms.
    /// The same value as `ely.bg`, but **never faded** by
    /// [`Theme::with_translucency_at`] — translucency belongs to the main
    /// window's background, and a sheet the desktop shows through is
    /// unreadable exactly when it is asking for input.
    pub dialog_surface: Hsla,
    /// A card floating over the frame that an event puts up: the modal
    /// sheet (Set Title, Close confirm) and the toasts. The same value as
    /// `ely.surface`, never faded, for the reason
    /// [`SirioColors::dialog_surface`] gives.
    pub floating_surface: Hsla,
    /// Generic hover wash — 5% neutral. Also the transcript row hover: a step
    /// lighter than `ely.hover`, because transcript rows
    /// are wider and a 6% wash over that area reads as a block.
    pub overlay: Hsla,
    /// Pressed wash — 9% neutral, so press reads as more than hover.
    pub overlay_strong: Hsla,
    /// Stronger divider, for seams that separate rather than merely delimit,
    /// and the neutral rail down a task, edit or tool card.
    pub border_strong: Hsla,
    /// Keyboard-focus ring on a text field — bezel's `ring`, the translucent
    /// hairline every bezel input, select and control lights up with. A veil
    /// on the surface's own tone (white on dark, black on light), never the
    /// body text colour: that painted an opaque white frame on the dark theme.
    pub ring: Hsla,
    /// Faintest text step — placeholder copy and disabled labels, below
    /// `ely.fg_subtle`.
    pub text_dim: Hsla,
    /// Soft danger fill (stop button hover).
    pub danger_muted: Hsla,
    /// Quantity blue: quota meters, and the clone and update progress bars.
    /// Blue means "how much", which is why a progress bar is never painted in
    /// a status hue — a bar filling up is not an alert.
    pub quantity: Hsla,
    /// Light fill for primary buttons, dark glyph on top.
    pub solid: Hsla,
    /// Glyph on primary buttons.
    pub on_solid: Hsla,
    /// Brand coral. No role paints it any more: the shell's focus rings,
    /// caret, selection and active chrome are neutral, and every colour left in
    /// the UI is a status, a diff, a quantity or an agent's own brand.
    ///
    /// What still reads it is the `Coral` entry of the agent-colour picker,
    /// which needs a real coral to offer. Kept as a token rather than inlined
    /// as a literal there, so the picker keeps drawing from `Theme` — and so
    /// the two invariants this value carries (it clears AA on its own surface,
    /// and it is not any agent's brand) still have something to hold.
    pub brand_coral: Hsla,
    /// Inline `code` rounded wash, and the band under a diff hunk — the same
    /// wash, because a hunk is code too.
    pub code_wash: Hsla,
    /// Tree guide stroke, including its source alpha.
    pub tree_guide: Hsla,
    /// Addition diff accent — the success hue.
    pub diff_add: Hsla,
    /// Addition diff background — translucent success wash.
    pub diff_add_bg: Hsla,
    /// Deletion diff accent — the danger hue.
    pub diff_del: Hsla,
    /// Deletion diff background — translucent danger wash.
    pub diff_del_bg: Hsla,
}

impl SirioColors {
    /// Every field with its name, in declaration order.
    pub fn named(&self) -> [(&'static str, Hsla); 21] {
        [
            ("canvas", self.canvas),
            ("frame_surface", self.frame_surface),
            ("terminal_surface", self.terminal_surface),
            ("dialog_surface", self.dialog_surface),
            ("floating_surface", self.floating_surface),
            ("overlay", self.overlay),
            ("overlay_strong", self.overlay_strong),
            ("border_strong", self.border_strong),
            ("ring", self.ring),
            ("text_dim", self.text_dim),
            ("danger_muted", self.danger_muted),
            ("quantity", self.quantity),
            ("solid", self.solid),
            ("on_solid", self.on_solid),
            ("brand_coral", self.brand_coral),
            ("code_wash", self.code_wash),
            ("tree_guide", self.tree_guide),
            ("diff_add", self.diff_add),
            ("diff_add_bg", self.diff_add_bg),
            ("diff_del", self.diff_del),
            ("diff_del_bg", self.diff_del_bg),
        ]
    }
}

/// All adaptive colors used by Sirio.
///
/// Names say what a value *does* in the design system (`ely.surface`,
/// `ely.sunken`, `sirio.overlay`, …) rather than which component consumes it.
/// The component-named layer this file used to carry alongside it — the
/// `tab_focus_accent`/`filter_field_bg` aliases — has been collapsed into the
/// roles it resolved to.
///
/// Where each value comes from — measured off a reference frame, or chosen by
/// us because no frame could settle it — is in
/// `docs/THEME-PROVENANCE.md`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Ely's vocabulary, holding Sirio's values: what every Ely component
    /// reads, and what Sirio's own call sites read where meaning and value
    /// coincide (spec §3.2).
    pub ely: ely_palette::Palette,
    /// The colours Ely has no field for.
    pub sirio: SirioColors,
}

fn bezel_theme_for(base_color: BaseColor, appearance: Appearance) -> bezel_theme::Theme {
    let mut theme = bezel_theme::Theme::branded(
        &bezel_theme::Brand {
            tint: base_color.tint(),
            ..Default::default()
        },
        match appearance {
            Appearance::Dark => bezel_theme::Appearance::Dark,
            Appearance::Light => bezel_theme::Appearance::Light,
        },
    );
    if let Some(ladder) = base_color.grey_ladder(appearance) {
        paint_grey_ladder(&mut theme, ladder);
    } else if base_color == BaseColor::Notte && appearance == Appearance::Dark {
        paint_notte_ladder(&mut theme);
    }
    theme
}

/// The one place lightness moves.
///
/// bezel's `Brand::apply` rotates hue and never lightness, and the Notte
/// preset was given four surfaces that are *lighter* than bezel's dark page
/// (`#202127` is above bezel's raised card), so no tint reaches them. They
/// are painted here, after `branded`, onto the seven surface tokens and
/// nothing else: the veils (`element_hover`, `border`, `input_bg`, …) are
/// white-alpha washes that compose over whatever is beneath them, and the
/// text ladder and semantic hues stay bezel's. These values in `base_color.rs`
/// now feed only `to_bezel_theme`; Sirio's source is `presets.rs`. Keep the
/// matching ladders in both files aligned until sub-project 7.
///
/// Mutation of the local, in bezel's own `Brand::apply` style; struct-update
/// syntax would need every one of bezel's 72 fields restated.
fn paint_notte_ladder(theme: &mut bezel_theme::Theme) {
    let ladder = base_color::NOTTE_LADDER;
    theme.bg = opaque_hsla(ladder.page);
    theme.surface = opaque_hsla(ladder.surface);
    // A card sits on the page, as it does in bezel's own dark
    // (`surface_card` #0E0E0E beside `surface` #0D0D0D).
    theme.surface_card = opaque_hsla(ladder.surface);
    theme.surface_raised = opaque_hsla(ladder.raised);
    theme.surface_dialog = opaque_hsla(ladder.raised);
    theme.surface_overlay = opaque_hsla(ladder.raised);
    theme.surface_raised_hover = opaque_hsla(ladder.raised_hover);
}

/// The solid grey ladders of `Neutral` and `Onice`, same mechanism as
/// [`paint_notte_ladder`]. These `base_color.rs` values feed only
/// `to_bezel_theme`; Sirio's matching values are hand-maintained in
/// `presets.rs`. Keep both copies aligned until sub-project 7.
fn paint_grey_ladder(theme: &mut bezel_theme::Theme, ladder: base_color::GreyLadder) {
    theme.bg = opaque_hsla(ladder.page);
    theme.surface = opaque_hsla(ladder.surface);
    theme.surface_card = opaque_hsla(ladder.surface);
    theme.surface_raised = opaque_hsla(ladder.raised);
    theme.surface_dialog = opaque_hsla(ladder.raised);
    theme.surface_overlay = opaque_hsla(ladder.raised);
    theme.surface_raised_hover = opaque_hsla(ladder.raised_hover);
    theme.input_bg = opaque_hsla(ladder.input);
    theme.element_hover = opaque_hsla(ladder.hover);
}

/// Opaque `0xRRGGBB` as the `Hsla` bezel's tokens are stored in.
fn opaque_hsla(hex: u32) -> gpui::Hsla {
    gpui::Hsla::from(rgb_hex(hex))
}

/// Sirio's source-code palette — the one place this crate owns colour rather
/// than re-exporting bezel's.
///
/// bezel's own `SyntaxPalette` is derived from the git history graph's lane
/// hues (indigo, pink, emerald, amber). That reads well as *lanes*, but it
/// paints a function call the same indigo as `fn`, and a type the same amber
/// as a number — distinctions a reader of code relies on. The editor was
/// asked to read like Zed, so the token hues below are Zed's One Dark and
/// One Light, which is the reference this surface is measured against.
///
/// Four entries deliberately stay the theme's own rather than Zed's, so the
/// palette still tracks appearance and base-colour changes: `variable` and
/// `parameter` take the body text colour, `punctuation` the dimmed one, and
/// `invalid` the theme's danger. Everything else is a fixed hue.
///
/// This source-code palette is Sirio's own. Adaptive colours are also
/// hand-maintained in `presets.rs`; a bezel change affects its widgets through
/// `to_bezel_theme`, not Sirio's preset values.
mod zed_syntax {
    //! One Dark / One Light token hues, by role.
    pub const DARK_RED: u32 = 0xE06C75;
    pub const DARK_ORANGE: u32 = 0xD19A66;
    pub const DARK_YELLOW: u32 = 0xE5C07B;
    pub const DARK_GREEN: u32 = 0x98C379;
    pub const DARK_CYAN: u32 = 0x56B6C2;
    pub const DARK_BLUE: u32 = 0x61AFEF;
    pub const DARK_PURPLE: u32 = 0xC678DD;
    pub const DARK_COMMENT: u32 = 0x5C6370;

    pub const LIGHT_RED: u32 = 0xE45649;
    pub const LIGHT_ORANGE: u32 = 0x986801;
    pub const LIGHT_YELLOW: u32 = 0xC18401;
    pub const LIGHT_GREEN: u32 = 0x50A14F;
    pub const LIGHT_CYAN: u32 = 0x0184BC;
    pub const LIGHT_BLUE: u32 = 0x4078F2;
    pub const LIGHT_PURPLE: u32 = 0xA626A4;
    pub const LIGHT_COMMENT: u32 = 0xA0A1A7;
}

impl Theme {
    /// The palette the file editor paints source tokens with.
    ///
    /// Returns bezel's `SyntaxPalette` *type* — the shape every highlighter
    /// and renderer already speaks — carrying Sirio's hues. Swapping the
    /// values rather than the type is what keeps `sirio_ui` free of a second
    /// palette vocabulary.
    pub fn syntax_palette(&self) -> bezel_theme::SyntaxPalette {
        use zed_syntax::*;
        let dark = self.appearance == Appearance::Dark;
        let pick = |on_dark: u32, on_light: u32| {
            opaque_hsla(if dark { on_dark } else { on_light })
        };
        let red = pick(DARK_RED, LIGHT_RED);
        let orange = pick(DARK_ORANGE, LIGHT_ORANGE);
        let yellow = pick(DARK_YELLOW, LIGHT_YELLOW);
        let green = pick(DARK_GREEN, LIGHT_GREEN);
        let cyan = pick(DARK_CYAN, LIGHT_CYAN);
        let blue = pick(DARK_BLUE, LIGHT_BLUE);
        let purple = pick(DARK_PURPLE, LIGHT_PURPLE);
        let comment = pick(DARK_COMMENT, LIGHT_COMMENT);
        // The theme's own, so a base-colour change still moves the text.
        let text = self.colors.ely.fg;
        let dimmed = self.colors.ely.fg_subtle;

        bezel_theme::SyntaxPalette {
            comment,
            keyword: purple,
            string: green,
            string_special: cyan,
            escape: cyan,
            number: orange,
            boolean: orange,
            type_name: yellow,
            type_builtin: yellow,
            constructor: yellow,
            function: blue,
            function_builtin: blue,
            macro_name: purple,
            property: red,
            constant: orange,
            variable: text,
            variable_special: red,
            parameter: text,
            operator: cyan,
            // Muted on purpose: brackets and commas are structure, and
            // painting them at full strength is what makes a dense line
            // read as noise.
            punctuation: dimmed,
            tag: red,
            attribute: orange,
            label: orange,
            invalid: self.colors.ely.danger,
        }
    }
}

impl ThemeColors {
    /// The fill of a menu an interaction puts up over the shell: the file and
    /// terminal context menus, and any popover that must be *read* rather than
    /// merely seen.
    ///
    /// A menu is an event-opened surface in exactly the sense
    /// [`SirioColors::floating_surface`] describes, so it must not fade with
    /// the panels — a menu the desktop shows through is unreadable at the one
    /// moment it is being asked to be read.
    pub fn menu_surface(&self) -> Hsla {
        self.sirio.floating_surface
    }

    fn for_appearance(appearance: Appearance, base: BaseColor) -> Self {
        let (ely, sirio) = presets::preset(base, appearance);
        Self { ely, sirio }
    }
}

/// Spacing and geometry tokens, re-valued to waku's measured scale
/// (4/6/7/8/12/13 radius steps; 48px bars; 2/4/6/8/10/12/14/20 spacing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// Gap between adjacent panels in the compact shell.
    pub shell_gap: Pixels,
    /// Inset between the shell and the window frame.
    pub shell_outer_inset: Pixels,
    /// Floating-card corner radius (waku's default control radius).
    pub card_corner_radius: Pixels,
    /// Gap between cards and the window edge.
    pub card_gap: Pixels,
    /// Floating-card shadow radius.
    pub card_shadow_radius: Pixels,
    /// Floating-card shadow vertical offset.
    pub card_shadow_y_offset: Pixels,
    /// Title-strip height — waku's 48px header/sidebar titlebar.
    pub title_strip_height: Pixels,
    /// Left inset of the first title-strip control. waku's macOS titlebar
    /// reserves ~78px for traffic lights; on Linux nothing occupies that
    /// zone, so the controls start at the waku header inset of 14px.
    pub traffic_light_inset: Pixels,
    /// Title-strip icon size (waku header icons: 14px).
    pub title_strip_icon_size: Pixels,
    /// Title-bar button frame (waku header buttons: 26px).
    pub titlebar_control_frame: Size<Pixels>,
    /// Spacing between title-bar controls (waku chrome gaps: 6-8px).
    pub titlebar_control_spacing: Pixels,
    /// Bottom usage bar height (waku's 40px sidebar footer).
    pub bottom_bar_height: Pixels,
    /// Context-menu width — COSMIC's `m` spacing step (24px) short of the
    /// next step up would be too narrow for a label plus a shortcut hint,
    /// so this is a fixed 240px, not a step on the spacing scale itself.
    /// Named for `codex12`'s tab context menu (`QUEUE.md`, P60/P63).
    pub menu_width: Pixels,
    /// Hairline stroke *thickness* — distinct from `Colors::hairline`,
    /// which is the hairline's colour. One geometric pixel, COSMIC's own
    /// hairline weight.
    pub hairline_thickness: Pixels,
    /// The compact, icon-only action button used inside dense rows and
    /// cards — smaller than `titlebar_control_frame` (waku's 26px chrome
    /// buttons), sized for a control that sits *in* content rather than
    /// *above* it. Comet's 24px top-bar cluster buttons (P76) are this
    /// token's first consumer; a settings row's "Refresh"/color-swatch
    /// controls (P75) are the next.
    pub compact_action: Pixels,
}

impl Default for Spacing {
    /// Same rule as [`Radii::default`]: the measurements do not move (T2).
    /// These literals preserve the values bezel's spacing steps produced.
    fn default() -> Self {
        Self {
            shell_gap: px(4.0), // bezel SPACE_XS
            shell_outer_inset: px(4.0), // bezel SPACE_XS
            card_corner_radius: px(6.0), // bezel SPACE_MD * 0.5
            card_gap: px(10.0), // bezel SPACE_MD * 0.833_333_3
            card_shadow_radius: px(18.0), // bezel SPACE_LG * 1.125
            card_shadow_y_offset: px(6.0), // bezel SPACE_MD * 0.5
            title_strip_height: px(48.0), // bezel SPACE_LG * 3.0
            traffic_light_inset: px(14.0), // bezel SPACE_LG * 0.875
            title_strip_icon_size: px(14.0), // bezel SPACE_LG * 0.875
            titlebar_control_frame: size(px(26.0), px(26.0)), // bezel SPACE_LG * 1.625 square
            titlebar_control_spacing: px(6.0), // bezel SPACE_MD * 0.5
            bottom_bar_height: px(40.0), // bezel SPACE_LG * 2.5
            menu_width: px(240.0), // bezel SPACE_LG * 15.0
            hairline_thickness: px(1.0), // bezel SPACE_XS * 0.25
            compact_action: px(24.0), // bezel SPACE_LG * 1.5
        }
    }
}

/// Corner-radius tokens, re-valued to waku's measured de-facto scale
/// (`docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` §A.2):
/// 4/5/6/7/8/10/12/13, plus `rounded_full` for pills and dots.
///
/// A radius that matters should resolve through these tokens, not a fresh
/// literal at the call site — the conformance suite pins the tokens, so a
/// future drift is caught in one place. The one deliberate exception is
/// the "full circle" idiom: a dot or pill spells `radius = half its box`
/// (a 6×6 dot at 3, a 44×22 swatch at 11) — numerically identical to
/// `rounded_full`, and left inline because naming each would invent tokens
/// for the same value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// Corner radius for a shell panel (7px).
    pub shell_panel: Pixels,
    /// Chips-in-rows, code-wash quads, sidebar close buttons (4px).
    pub chip: Pixels,
    /// Activity chips, focus-ring proxies, the active segment of a
    /// segmented control (5px).
    pub chip_active: Pixels,
    /// The default control radius: icon buttons, text fields, chips,
    /// tooltips (6px).
    pub control: Pixels,
    /// Row cards: session cards, settings rows, status badges (7px).
    pub row_card: Pixels,
    /// Code blocks, tool cards (8px).
    pub code_block: Pixels,
    /// Toasts, prompt cards (10px).
    pub toast: Pixels,
    /// User message pills, menus, Sirio's settings cards (12px).
    pub user_pill: Pixels,
    /// The composer card (13px).
    pub composer: Pixels,
}

impl Default for Radii {
    /// The values are unchanged (spec decision T2); they are literals that
    /// preserve the values bezel's `Brand::radius` produced. Ratios
    /// that do not land on one of bezel's five named corners carry an explicit
    /// multiplier rather than being rounded to the nearest named one —
    /// rounding would move the UI, which T2 forbids.
    fn default() -> Self {
        Self {
            shell_panel: px(7.0), // bezel BASE_RADIUS * 0.875
            chip: px(4.0), // bezel BASE_RADIUS * 0.5
            chip_active: px(5.0), // bezel BASE_RADIUS * 0.625
            control: px(6.0), // bezel BASE_RADIUS * 0.75
            row_card: px(7.0), // bezel BASE_RADIUS * 0.875
            code_block: px(8.0), // bezel BASE_RADIUS
            toast: px(10.0), // bezel BASE_RADIUS * 1.25
            user_pill: px(12.0), // bezel BASE_RADIUS * 1.5
            composer: px(13.0), // bezel BASE_RADIUS * 1.625
        }
    }
}

/// The comet-derived top-bar chrome (P76): the traffic-light cluster and the
/// button cluster beside it. Measured from comet's own source
/// (`Theme::TITLEBAR_HEIGHT`, `CLUSTER_BUTTONS_WIDTH` — dimensions, not
/// code, transcribed under `docs/linux-rewrite/tasks/P76-*`), except the
/// traffic lights themselves and [`Self::cluster_start`]: comet never draws
/// them (macOS decorates its own), so there is nothing there to measure —
/// [`Self::cluster_start`] documents the derivation.
///
/// Deliberately separate from `Spacing`'s `title_strip_height` /
/// `traffic_light_inset` / `titlebar_control_*` fields, which stay exactly
/// as they were: that waku-era vocabulary is read by `tab_bar.rs`,
/// `right_panel.rs`, `file_view.rs`, `changes.rs`, `sirio_terminal` and
/// `main.rs` for their own compact chrome, none of which this brief owns —
/// changing those values would silently reflow surfaces P76 has no mandate
/// to touch.
///
/// **P102 fallback-only fields.** `traffic_light_diameter`,
/// `traffic_light_gap`, `traffic_light_cluster_gap` and
/// [`Self::cluster_start`] exist only to lay out the three dots
/// `titlebar.rs::traffic_light` draws — and per
/// `docs/linux-rewrite/tasks/P102-the-top-bar-belongs-to-the-os.md`, that
/// happens in exactly one case: `titlebar.rs`'s `WindowControls::
/// TrafficLights`, i.e. the platform reported that nothing else will ever
/// decorate this window.
///
/// The other three outcomes read none of them, and differ only in the
/// cluster's leading edge: `WindowControls::MacosNative` reserves
/// `macos_traffic_light_cluster_inset` for AppKit's own controls, while
/// `OsDrawnAbove` (a window manager drew its titlebar above this row) and
/// `WindowsCaption` (the caption buttons live at the *trailing* edge, so
/// nothing precedes the cluster) both start at `traffic_light_inset`. See
/// `WindowControls::cluster_leading_gap`.
///
/// `bar_height` and `cluster_button_gap` are unaffected by any of it —
/// they size the row and the always-drawn icon cluster respectively,
/// neither of which is a window control. `bar_height` doubles as the
/// caption buttons' height (their width is
/// [`WindowsCaption::button_width`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrowserChrome {
    /// The bar's total height (comet: 38px).
    pub bar_height: Pixels,
    /// Diameter of one traffic-light dot. Not a comet measurement — comet's
    /// lights are OS-drawn on macOS and absent on Linux, so there is no
    /// dimension to transcribe. 12px matches the widely-documented real-world
    /// size of macOS's own dots, chosen independently for legibility at a
    /// 38px bar height (12 / 38 ≈ 0.32 of the bar), not copied from any
    /// comet inset constant.
    ///
    /// P102 fallback-only: read only when `titlebar.rs` draws the dots at
    /// all — see the struct docs.
    pub traffic_light_diameter: Pixels,
    /// Edge-to-edge gap between adjacent lights (8px — with a 12px diameter
    /// this gives a 20px centre-to-centre pitch, the commonly cited macOS
    /// spacing; derived for legibility, not read from comet, which never
    /// lays this out itself).
    ///
    /// P102 fallback-only: read only when `titlebar.rs` draws the dots at
    /// all — see the struct docs.
    pub traffic_light_gap: Pixels,
    /// Left inset of the first light. Reuses comet's own **non-macOS**
    /// baseline (`cluster_buttons_start(is_macos: false, ..) == 10.0`) —
    /// "how close to the edge do our own, non-OS-drawn controls sit" — rather
    /// than macOS's `{14, 15}` inset, which is calibrated to Apple's dot
    /// size and chrome, not ours.
    ///
    /// Not P102 fallback-only: `titlebar.rs` reads this in **both**
    /// branches, as the fallback lights' own inset when they are drawn and
    /// as the icon cluster's leading inset when Linux server-side
    /// decorations leave the row without fallback lights.
    pub traffic_light_inset: Pixels,
    /// Leading inset for the icon cluster when macOS AppKit owns the real
    /// traffic lights. `main.rs` configures AppKit's
    /// `traffic_light_position` to 12pt from the left edge. On this machine,
    /// `standardWindowButton` frames measured close=`x=9,w=14`,
    /// minimize=`x=32,w=14`, and zoom=`x=55,w=14`: a 14pt button frame and
    /// 9pt inter-button gaps. With the configured position, AppKit's group
    /// ends at `12 + (3 * 14) + (2 * 9) = 72pt`; adding the theme's 8px
    /// separating rhythm gives `72 + 8 = 80px`.
    ///
    /// The 14pt/9pt values are measured from `standardWindowButton` frames
    /// on macOS and are OS-version-dependent. gpui re-measures the close and
    /// minimize frames on every layout pass rather than hardcoding them, so
    /// this token is a conservative reservation, not a contract. It is read
    /// only on macOS when `Decorations::Server` is reported, so our cluster
    /// begins to the right of AppKit's controls.
    pub macos_traffic_light_cluster_inset: Pixels,
    /// Gap between the last light and the first cluster button (8px — the
    /// same rhythm as `traffic_light_gap`, so the light group reads as one
    /// unit and the handoff to the cluster does not look accidental).
    ///
    /// P102 fallback-only: read only when `titlebar.rs` draws the dots at
    /// all — see the struct docs.
    pub traffic_light_cluster_gap: Pixels,
    /// Gap between adjacent cluster buttons — comet's own measurement
    /// (`CLUSTER_BUTTONS_WIDTH = 24.0 * 3.0 + 2.0 * 2.0`, i.e. 2px gaps).
    /// Button *size* is `Spacing::compact_action`, not duplicated here —
    /// one 24px "small icon action" token for the whole app, not a second
    /// name for the same number.
    pub cluster_button_gap: Pixels,
}

impl BrowserChrome {
    /// Where the button cluster starts, in px from the window's left edge,
    /// **when the three traffic-light dots are drawn** — P102 fallback-only,
    /// same as the fields it derives from (see the struct docs); with no
    /// dots drawn the cluster starts at `traffic_light_inset` instead, not
    /// this value. **Derived, not copied from comet's 88px** (that number
    /// is macOS's own OS-drawn inset, sized to Apple's dot geometry, which
    /// this bar does not use).
    ///
    /// `traffic_light_inset + 3 lights + 2 inter-light gaps +
    /// traffic_light_cluster_gap` = `10 + 3×12 + 2×8 + 8` = **70px** at the
    /// default values — between comet's own two boundary numbers (10px with
    /// no lights, 88px with macOS's own), which is the expected place for a
    /// bar that, unlike either reference, draws smaller lights of its own.
    pub fn cluster_start(&self) -> Pixels {
        self.traffic_light_inset
            + self.traffic_light_diameter * 3.0
            + self.traffic_light_gap * 2.0
            + self.traffic_light_cluster_gap
    }
}

impl Default for BrowserChrome {
    fn default() -> Self {
        Self {
            bar_height: px(38.0),
            traffic_light_diameter: px(12.0),
            traffic_light_gap: px(8.0),
            traffic_light_inset: px(10.0),
            macos_traffic_light_cluster_inset: px(80.0),
            traffic_light_cluster_gap: px(8.0),
            cluster_button_gap: px(2.0),
        }
    }
}

/// The Windows caption buttons' own spec — the little `titlebar.rs` draws
/// where the platform suppressed its native caption (see that module's
/// `WindowControls::WindowsCaption`).
///
/// # Why this is not COSMIC
///
/// Only what the design system genuinely cannot express lives here. The
/// neutral buttons — minimize and maximize — read the theme's own
/// `element_hover` like every other icon button on this bar, and that is
/// fidelity, not a compromise: Windows 11 draws *their* hover as a neutral
/// veil that follows the light/dark theme. The close button is the one
/// exception, because its red is a **system constant** that says "this
/// closes the window" in every Windows app regardless of the app's theme.
///
/// The theme's `danger` cannot stand in for it. `danger` is a colour for
/// *text and marks on a surface*, sized for legibility against the page;
/// the Windows close red is a saturated **fill** carrying white text. The
/// two run in opposite contrast directions, so substituting one for the
/// other does not give a different red, it gives a close button whose glyph
/// disappears into its own fill.
///
/// # Measured, not transcribed
///
/// Zed's `platform_windows.rs` hardcodes `#E81123`, the older Win32/UWP
/// value. These numbers were sampled from the pixels of a real native
/// close button on Windows 11 (build 26200) with the cursor held over it,
/// and agree with WinUI's own `CloseButtonBackgroundPointerOver`. Zed also
/// *derives* its pressed state as `hover.opacity(0.8)`; the measurement
/// puts the real ratio nearer 0.92, so [`Self::close_pressed`] carries the
/// sampled value rather than a derivation that reads visibly duller.
///
/// # Appearance-independent
///
/// Unlike [`ThemeColors`], this group is a single `default()` for both
/// appearances: the red is a system constant, and white-on-saturated-red
/// is forced by contrast. Everything here that *should* follow the theme —
/// the resting glyph, the neutral hover, the disabled treatment — comes
/// from `icon_button`, which is already resolved per appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowsCaption {
    /// Width of one caption button. Zed's own measurement; the height is
    /// [`BrowserChrome::bar_height`], so the button fills the row and the
    /// top-right corner stays clickable edge-to-edge (Fitts).
    pub button_width: Pixels,
    /// Type size for the Segoe glyph drawn inside the button.
    pub glyph_size: Pixels,
    /// Close-button fill on hover. Sampled: `#C42B1C`.
    pub close_hover: Rgba,
    /// Close-button fill while held. Sampled: `#B42A1B` — **not**
    /// `close_hover` at 0.8, see the struct docs.
    pub close_pressed: Rgba,
    /// Glyph colour drawn over `close_hover`/`close_pressed`.
    pub close_on: Rgba,
}

impl Default for WindowsCaption {
    fn default() -> Self {
        Self {
            button_width: px(36.0),
            glyph_size: px(10.0),
            close_hover: rgb(0xC42B1C),
            close_pressed: rgb(0xB42A1B),
            close_on: rgb(0xFFFFFF),
        }
    }
}

/// Interface and code type-scale tokens: waku's measured scale with a
/// uniform **+1px** applied. waku's 13.5px body reads at 14.5, its 12px
/// UI chrome at 13, its 20px display at 21; the line heights move with
/// them so the leading ratios hold.
///
/// The offset is applied in ONE place — [`Typography::default_scale`] —
/// because every heading step in [`Typography::for_base_size`] is
/// expressed as a delta from 13.5 and inherits it for free. Only the
/// three tokens that deliberately do not follow the base (`ui_size` and
/// the two line heights) carry their raised values literally.
///
/// Each field below names waku's measurement first and what Sirio
/// renders second: since the offset, those are two different numbers
/// everywhere, and collapsing them would lose the provenance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// Body base size, in points (waku markdown body: 13.5px; Sirio
    /// renders 14.5px).
    pub base_size: Pixels,
    /// Code size, in points (waku code body: 11.5px; Sirio renders 13px).
    pub code_size: Pixels,
    /// Code line height (waku: 17.5px; Sirio renders 19px).
    pub code_line_height: Pixels,
    /// Code font weight (waku's embedded JetBrains Mono renders NORMAL).
    pub code_weight: FontWeight,
    /// The monospace family code renders in — the one answer to "what is our
    /// mono font", resolved once at runtime from the families actually
    /// installed (see [`Theme::resolve_code_family`]) instead of five string
    /// literals. `SFMono-Regular` does not exist on Linux; the fallback
    /// chain picks what does.
    pub code_family: &'static str,
    /// The sans-serif family the application UI renders in — the one answer
    /// to "what is our UI font", resolved once at runtime from the families
    /// actually installed (see [`Theme::resolve_ui_family`]). JetBrains Sans
    /// leads off macOS, Apple's SF Pro leads on it.
    pub ui_family: &'static str,
    /// Display size (waku empty-state headline: 20px MEDIUM; Sirio
    /// renders 21px).
    pub large_title: Pixels,
    /// Markdown h2 size (waku 17px; Sirio renders 18px).
    pub title: Pixels,
    /// Markdown h3 size (waku 15px; Sirio renders 16px).
    pub title2: Pixels,
    /// Markdown h4 size (waku 14px; Sirio renders 15px).
    pub title3: Pixels,
    /// Headline size (15px).
    ///
    /// This was exactly `base_size` — the conformance test used to spell
    /// it "headline is the body size" — and it no longer is: it sits one
    /// step above the body and coincides with [`Typography::title3`], so
    /// an h4 and a headline render alike.
    pub headline: Pixels,
    /// Callout/subheadline size (14px).
    pub callout: Pixels,
    /// Footnote/UI chrome size (13px).
    pub footnote: Pixels,
    /// Caption-2 size (14px).
    ///
    /// The one type token that does NOT sit on a waku-measured step. It
    /// was raised from the measured 10.5 for legibility, which leaves it
    /// larger than [`Typography::footnote`] (13) — sharing `callout`'s
    /// step — so despite the name, this is no longer the smallest size in
    /// the scale. Reach for
    /// `footnote` when what you want is "the small one"; reach for
    /// `caption2` when you want the size the dense Sirio-only strips
    /// (status bar, tab bar, toolbar labels) actually render at.
    pub caption2: Pixels,
    /// Default UI chrome size (waku: 11.5px for chips, buttons, rows;
    /// Sirio renders 13px).
    pub ui_size: Pixels,
    /// Body line height (waku markdown body: 21px; Sirio renders 22px).
    pub body_line_height: Pixels,
    /// UI chrome line height (waku rows: 14-16px, 16 the reading default;
    /// Sirio renders 17px).
    pub ui_line_height: Pixels,
}

impl Typography {
    /// Returns the default scale: waku's 13.5px body plus the app-wide
    /// +1px.
    ///
    /// This single number moves the whole interface. Every heading step
    /// in [`Self::for_base_size`] is written as an offset from 13.5, so
    /// raising the base raises them together; `IconSize::resolve` reads
    /// `base_size` for the same delta, so glyphs keep pace with the text
    /// beside them instead of shrinking against it.
    ///
    /// What it does NOT move: `ui_size` and the two line heights are
    /// fixed in `for_base_size`, and the `text_size(px(…))` literals
    /// scattered through the UI crates, which resolve nothing. Both had
    /// to be raised by hand alongside this.
    pub fn default_scale() -> Self {
        Self::for_base_size(14.5)
    }

    /// Returns the type scale for a chosen body base size, preserving waku's
    /// step relationships (headings scale off the body, chrome stays fixed
    /// at 13).
    pub fn for_base_size(base_size: f32) -> Self {
        let delta = base_size - 13.5;
        let scaled = |points: f32| px((points + delta).max(6.0));

        Self {
            base_size: px(base_size),
            code_size: scaled(12.0),
            code_line_height: px(19.0),
            code_weight: FontWeight::NORMAL,
            code_family: code_family(),
            ui_family: ui_family(),
            large_title: scaled(20.0),
            title: scaled(17.0),
            title2: scaled(15.0),
            title3: scaled(14.0),
            headline: scaled(14.0),
            callout: scaled(13.0),
            footnote: scaled(12.0),
            // Off the measured scale on purpose — see the field doc.
            caption2: scaled(13.0),
            ui_size: px(13.0),
            body_line_height: px(22.0),
            ui_line_height: px(17.0),
        }
    }

    /// Returns the type scale for the persisted interface font size. The
    /// persisted value is the 13pt UI chrome size; the body scale retains the
    /// existing +1.5px design offset and every explicit UI size moves by the
    /// same delta.
    pub fn for_interface_size(interface_size: f32) -> Self {
        let delta = interface_size - 13.0;
        let mut typography = Self::for_base_size(14.5 + delta);
        typography.code_line_height = px(19.0 + delta);
        typography.ui_size = px(13.0 + delta);
        typography.body_line_height = px(22.0 + delta);
        typography.ui_line_height = px(17.0 + delta);
        typography
    }

    /// Shifts an explicit UI text size by the persisted interface-size delta.
    pub fn scaled(&self, points: f32) -> Pixels {
        px((points + f32::from(self.base_size) - 14.5).max(6.0))
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::default_scale()
    }
}

// ── the font families ────────────────────────────────────────────────────
//
// Three families are resolved once, at runtime, from what is actually
// installed — never hard-coded: the UI (sans) family, the code (mono)
// family, and the terminal (Nerd Font mono) family. The old code-font
// comment tells the story that motivated the whole section: five call
// sites used to ask for `SFMono-Regular` by name, a family that does not
// exist on Linux, and GPUI fell back silently so every code span rendered
// in a family chosen by the font stack rather than one we picked — and it
// looked fine, which is why it survived. The same class of defect as the
// "SF Symbols" file-icon entry in Settings: a macOS assumption that is
// invisible until you look for it.
//
// One list per role, no platform split: Geist and Geist Mono (SIL OFL 1.1)
// lead everywhere, registered by `register_ui_fonts` before the first
// window opens, so they are always present. A theme that changes face per
// platform cannot be reviewed as one design, which is why the Apple faces sit
// at the tail as a last resort rather than leading on macOS. The JetBrains
// faces (JetBrains Mono — SIL OFL 1.1; JetBrains Sans — Apache 2.0 — both open
// source) follow as the fontconfig-detected fallback. Each list was verified
// against
// `fc-list : family` on the Linux build machine rather than assumed.

/// The sans-serif families to prefer, in order, when resolving the UI font.
///
/// "Geist" leads on every platform, macOS included: `register_ui_fonts`
/// registers it with real 500/600/700 statics, so it is always present here.
/// Apple's SF faces are kept at the tail as a last resort rather than led with,
/// because a theme that changes face per platform cannot be reviewed as one
/// design.
pub const UI_FAMILY_CANDIDATES: &[&str] = &[
    "Geist",
    "JetBrains Sans",
    "Inter",
    "Ubuntu",
    "Noto Sans",
    "Cantarell",
    "DejaVu Sans",
    "SF Pro",
    "Helvetica Neue",
];

/// Monospace families to prefer, in order, when resolving the code font.
///
/// "Geist Mono" leads for the same reason "Geist" leads
/// [`UI_FAMILY_CANDIDATES`]: `register_ui_fonts` registers it before the first
/// window opens. Behind it is the family the visual bar is set in (waku's
/// JetBrains Mono), then common good monospaced faces, then whatever the
/// system's generic "monospace" resolves to (fontconfig's alias on Linux, always
/// present). A candidate that is not installed is skipped — never guessed
/// at.
pub const CODE_FAMILY_CANDIDATES: &[&str] = &[
    "Geist Mono",
    "JetBrains Mono",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "SF Mono",
];

/// The terminal family Sirio ships: JetBrains Mono patched by Nerd Fonts
/// (v3.5.1, `patched-fonts/JetBrainsMono/Ligatures`), the `Mono` build in
/// which every icon and Powerline glyph is exactly one cell wide. Bundled
/// for the same reason Sirio bundles Geist — so the terminal's face does not
/// depend on what the host happens to have installed. The 2026-09-12
/// Windows screenshot is what that dependence looked like: a stock box has
/// none of the JetBrains, Nerd or Linux families below, every candidate was
/// skipped, and fontdb's built-in generic answered "Courier New" — a slab
/// serif with no Nerd Font glyph at all, so Claude Code's status-line pills
/// (`U+E0B6`/`U+E0B4`) painted as tofu. DirectWrite's system fallback does
/// not cover the Private Use Area those glyphs live in, so only the primary
/// face can supply them; a symbols-only fallback would not do either, since
/// gpui's Windows `FontFallbacks` are looked up in the *system* collection,
/// never the registered one.
///
/// Four static faces rather than a variable file: gpui's cosmic-text path
/// (Linux) rasterizes a variable font at its default instance only, and
/// DirectWrite matches weight and style against real faces without
/// synthesising an oblique — the agent TUIs use both bold and italic. Name
/// and bytes are pinned to each other by
/// `bundled_terminal_faces_name_the_family_and_cover_the_agent_glyphs`.
/// Licence: SIL OFL 1.1 (`rust/assets/fonts/OFL.txt`); provenance in
/// `THIRD_PARTY_NOTICES.md`.
pub const BUNDLED_TERMINAL_FAMILY: &str = "JetBrainsMono Nerd Font Mono";

/// The same four faces under the family name their legacy (`nameID 1`)
/// record carries, for a text system that reports that one rather than the
/// typographic (`nameID 16`) name above.
pub const BUNDLED_TERMINAL_FAMILY_LEGACY_NAME: &str = "JetBrainsMono NFM";

static TERMINAL_FONT_REGULAR: &[u8] =
    include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
static TERMINAL_FONT_BOLD: &[u8] =
    include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
static TERMINAL_FONT_ITALIC: &[u8] =
    include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
static TERMINAL_FONT_BOLD_ITALIC: &[u8] =
    include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

static UI_FONT_GEIST: &[u8] = include_bytes!("../../../assets/fonts/Geist.ttf");
static UI_FONT_GEIST_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Geist-Medium.ttf");
static UI_FONT_GEIST_SEMIBOLD: &[u8] =
    include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf");
static UI_FONT_GEIST_BOLD: &[u8] = include_bytes!("../../../assets/fonts/Geist-Bold.ttf");
static UI_FONT_GEIST_MONO: &[u8] = include_bytes!("../../../assets/fonts/GeistMono.ttf");

/// The UI faces: Geist, its static 500/600/700 weights, and Geist Mono.
pub fn bundled_ui_fonts() -> Vec<Cow<'static, [u8]>> {
    [
        UI_FONT_GEIST,
        UI_FONT_GEIST_MEDIUM,
        UI_FONT_GEIST_SEMIBOLD,
        UI_FONT_GEIST_BOLD,
        UI_FONT_GEIST_MONO,
    ]
    .into_iter()
    .map(Cow::Borrowed)
    .collect()
}

/// Registers the UI faces. Must run before [`Theme::init`], like
/// [`register_bundled_terminal_font`]: `Theme::install` resolves the UI and
/// code families from `all_font_names()` once.
pub fn register_ui_fonts(cx: &App) -> anyhow::Result<()> {
    cx.text_system().add_fonts(bundled_ui_fonts())
}

/// The bundled terminal faces, regular, bold, italic and bold italic, as
/// the bytes `TextSystem::add_fonts` takes.
pub fn bundled_terminal_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(TERMINAL_FONT_REGULAR),
        Cow::Borrowed(TERMINAL_FONT_BOLD),
        Cow::Borrowed(TERMINAL_FONT_ITALIC),
        Cow::Borrowed(TERMINAL_FONT_BOLD_ITALIC),
    ]
}

/// Registers the bundled terminal faces with the text system. Must run
/// before [`Theme::init`] for the same reason `register_ui_fonts`
/// must: `Theme::install` resolves and remembers `TERMINAL_FAMILY` from
/// `all_font_names()` on its first call, and a face registered afterwards
/// is never seen. Failure is non-fatal: the candidate chain below still
/// resolves, now to a Windows- or Linux-native family rather than to the
/// generic answer.
pub fn register_bundled_terminal_font(cx: &App) -> anyhow::Result<()> {
    cx.text_system().add_fonts(bundled_terminal_fonts())
}

/// Monospace families to prefer, in order, when resolving the terminal
/// font. The terminal renders agent TUIs whose glyph set needs a Nerd Font
/// (MesloLGS Nerd Font Mono is what Claude Code itself asks to be
/// installed), so the bundled [`BUNDLED_TERMINAL_FAMILY`] leads — it is
/// registered before the theme resolves, so it is normally the answer —
/// with the user-installed Nerd Font builds of JetBrains Mono, the plain
/// JetBrains Mono, then the platforms' own monospace faces behind it.
/// Cascadia Mono, Cascadia Code and Consolas are the Windows-native
/// entries: absent everywhere else, they exist so that a Windows box on
/// which the bundled registration failed still lands on a real terminal
/// face rather than on fontdb's generic "Courier New".
#[cfg(not(target_os = "macos"))]
pub const TERMINAL_FAMILY_CANDIDATES: &[&str] = &[
    BUNDLED_TERMINAL_FAMILY,
    BUNDLED_TERMINAL_FAMILY_LEGACY_NAME,
    "JetBrainsMono Nerd Font",
    "JetBrains Mono NL Nerd Font",
    "MesloLGS Nerd Font Mono",
    "JetBrains Mono",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// Monospace families to prefer, in order, when resolving the terminal
/// font on macOS — Apple's own SF Mono first, the bundled and installed
/// Nerd Font faces after. See the non-macOS list for the Windows-native
/// entries.
#[cfg(target_os = "macos")]
pub const TERMINAL_FAMILY_CANDIDATES: &[&str] = &[
    "SF Mono",
    BUNDLED_TERMINAL_FAMILY,
    BUNDLED_TERMINAL_FAMILY_LEGACY_NAME,
    "JetBrainsMono Nerd Font",
    "JetBrains Mono NL Nerd Font",
    "MesloLGS Nerd Font Mono",
    "JetBrains Mono",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// The resolved UI family, remembered once. The first caller wins: in the
/// app that is [`Theme::resolve_ui_family`] with the real installed list;
/// in tests it is whichever theme accessor runs first, which gets the
/// system's generic sans-serif answer — a real family either way.
static UI_FAMILY: OnceLock<String> = OnceLock::new();

/// The resolved code family, remembered once. The first caller wins: in the
/// app that is [`Theme::resolve_code_family`] with the real installed list;
/// in tests it is whichever theme accessor runs first, which gets the
/// system's generic monospace answer — a real family either way.
static CODE_FAMILY: OnceLock<String> = OnceLock::new();

/// The resolved terminal family, remembered once. The first caller wins: in
/// the app that is [`Theme::resolve_terminal_family`] with the real
/// installed list; in tests it is whichever theme accessor runs first,
/// which gets the system's generic monospace answer — a real family either
/// way.
static TERMINAL_FAMILY: OnceLock<String> = OnceLock::new();

/// Resolves the sans-serif family from a set of installed family names.
///
/// `installed` should be the runtime font list — GPUI's
/// `TextSystem::all_font_names()`, which on Linux reports the same families
/// as `fc-list : family`. Returns the first [`UI_FAMILY_CANDIDATES`] entry
/// present, falling back to the system's generic sans-serif answer.
pub fn resolve_ui_family(installed: &HashSet<String>) -> String {
    for candidate in UI_FAMILY_CANDIDATES {
        if installed.contains(*candidate) {
            return (*candidate).to_string();
        }
    }
    system_sans_family()
}

/// Resolves the monospace family from a set of installed family names.
///
/// `installed` should be the runtime font list — GPUI's
/// `TextSystem::all_font_names()`, which on Linux reports the same families
/// as `fc-list : family`. Returns the first [`CODE_FAMILY_CANDIDATES`]
/// entry present, falling back to the system's generic monospace answer.
pub fn resolve_code_family(installed: &HashSet<String>) -> String {
    for candidate in CODE_FAMILY_CANDIDATES {
        if installed.contains(*candidate) {
            return (*candidate).to_string();
        }
    }
    system_monospace_family()
}

/// Resolves the terminal (Nerd Font) family from a set of installed family
/// names. Returns the first [`TERMINAL_FAMILY_CANDIDATES`] entry present,
/// falling back to the system's generic monospace answer.
pub fn resolve_terminal_family(installed: &HashSet<String>) -> String {
    for candidate in TERMINAL_FAMILY_CANDIDATES {
        if installed.contains(*candidate) {
            return (*candidate).to_string();
        }
    }
    system_monospace_family()
}

/// The family the system maps the generic "sans-serif" to.
///
/// Queried through fontdb — the same database gpui's Linux text system
/// loads — so the answer is the system's own (fontconfig's `sans-serif`
/// alias on Linux, e.g. `fc-match sans-serif`), not a hard-coded family
/// that happens to exist here. Non-Linux keeps fontdb's built-in generic.
pub fn system_sans_family() -> String {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    database.family_name(&fontdb::Family::SansSerif).to_string()
}

/// The family the system maps the generic "monospace" to.
///
/// Queried through fontdb — the same database gpui's Linux text system
/// loads — so the answer is the system's own (fontconfig's `monospace`
/// alias on Linux, e.g. `fc-match monospace`), not a hard-coded family that
/// happens to exist here. Non-Linux keeps fontdb's built-in generic.
pub fn system_monospace_family() -> String {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    database.family_name(&fontdb::Family::Monospace).to_string()
}

/// The resolved UI font family, resolving the system's generic sans-serif
/// answer if nothing has been resolved yet.
pub fn ui_family() -> &'static str {
    UI_FAMILY.get_or_init(system_sans_family).as_str()
}

/// The resolved code font family, resolving the system's generic monospace
/// answer if nothing has been resolved yet.
pub fn code_family() -> &'static str {
    CODE_FAMILY.get_or_init(system_monospace_family).as_str()
}

/// The resolved terminal font family, resolving the system's generic
/// monospace answer if nothing has been resolved yet.
pub fn terminal_family() -> &'static str {
    TERMINAL_FAMILY
        .get_or_init(system_monospace_family)
        .as_str()
}

/// The resolved Sirio theme stored as a GPUI global.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The requested appearance mode.
    pub mode: ThemeMode,
    /// The active resolved appearance.
    pub appearance: Appearance,
    /// The neutral family the greys are tinted with. Carried on the theme so
    /// every reinstall (`install`, `set_mode`, `with_translucency`, the
    /// portal follower) preserves it by construction — the same reason
    /// `translucency_enabled` lives here.
    pub base_color: BaseColor,
    /// Adaptive color tokens.
    pub colors: ThemeColors,
    /// Spacing and geometry tokens.
    pub spacing: Spacing,
    /// Corner-radius tokens (waku's measured scale).
    pub radii: Radii,
    /// The comet-derived top-bar chrome (P76): traffic-light and
    /// button-cluster geometry, distinct from `Spacing`'s waku-era chrome
    /// tokens (see [`BrowserChrome`]'s own docs for why they don't merge).
    pub browser_chrome: BrowserChrome,
    /// The Windows caption buttons' own spec — geometry plus the one
    /// colour COSMIC cannot express (see [`WindowsCaption`]). Read only by
    /// the `WindowControls::WindowsCaption` branch of `titlebar.rs`.
    pub windows_caption: WindowsCaption,
    /// Typography tokens.
    pub typography: Typography,
    /// Shared opacity for translucent surfaces.
    pub translucent_surface_opacity: f32,
    /// Whether the structural surfaces are faded to
    /// [`Theme::translucent_surface_opacity`] for rendering over native
    /// window blur. Carried on the theme so every reinstall (`install`,
    /// `set_mode`, the portal follower) preserves it by construction.
    pub translucency_enabled: bool,
}

impl Global for Theme {}

impl Deref for Theme {
    type Target = ThemeColors;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl Theme {
    /// Returns the theme installed in the GPUI application global.
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Colour of commit-graph lane `index`, cycling.
    ///
    /// Deliberately an accessor over hues this palette already measures
    /// rather than six new tokens: the palettes are pinned against a frozen
    /// provenance record (see `dark_palette_matches_recorded_provenance`),
    /// so every added colour is a colour someone has to measure and justify.
    /// The graph only needs its lanes to be mutually distinguishable.
    pub fn graph_lane(&self, index: usize) -> Hsla {
        let lanes: [Hsla; 6] = [
            self.ely.fg,
            self.ely.fg_subtle,
            self.ely.success,
            self.ely.warning,
            self.ely.danger,
            // Not `favorite`: it is `warning`, which is already lane four.
            // The coral is the one hue in the theme no other lane can collide
            // with, because no role paints it.
            self.sirio.brand_coral,
        ];
        lanes[index % lanes.len()]
    }

    /// Installs a theme, resolving `System` against the current window
    /// appearance. On Linux the resolution is dark-biased until the portal
    /// is heard from (see [`ThemeMode::resolve_system`]).
    /// Pushes this theme's appearance into bezel's process-wide mirror.
    ///
    /// bezel's `ink`, `wash` and `hairline` are free functions with no `cx`,
    /// so they cannot read the theme they are painting for; they read a
    /// mirror that defaults to Dark. It has to be pushed from every place
    /// that *installs* a theme, which is not the same set as the public
    /// entry points — `set_mode` delegates to `install`, but `follow_portal`
    /// swaps the global on its own.
    pub fn sync_appearance(&self) {
        bezel_theme::set_current_appearance(match self.appearance {
            Appearance::Light => bezel_theme::Appearance::Light,
            Appearance::Dark => bezel_theme::Appearance::Dark,
        });
    }

    /// The bezel theme built from `base_color.rs`'s ladders. Sirio's matching
    /// preset values are maintained separately in `presets.rs`; keep ladder
    /// edits aligned in both until sub-project 7.
    pub fn to_bezel_theme(&self) -> bezel_theme::Theme {
        bezel_theme_for(self.base_color, self.appearance)
    }

    /// Installs this theme's branded palette into bezel's registry and keeps
    /// bezel's context-free appearance mirror in sync.
    pub fn install_into_bezel(&self, cx: &mut gpui::App) {
        bezel_theme::Theme::install_custom(self.to_bezel_theme(), cx);
        self.sync_appearance();
    }

    pub fn install(mode: ThemeMode, cx: &mut App) {
        Self::resolve_font_families(cx);
        // Both the base colour and the translucency flag are recovered from
        // whatever is already installed: `install` is called from mode
        // switches and from the portal follower, neither of which knows or
        // should know about the other two axes.
        let previous = cx.try_global::<Self>();
        let base = previous.map_or_else(BaseColor::default, |theme| theme.base_color);
        let translucency = previous.is_some_and(|theme| theme.translucency_enabled);
        let opacity = previous.map_or(Self::surface_opacity(true), |theme| {
            theme.translucent_surface_opacity
        });
        let typography = previous.map_or_else(Typography::default, |theme| theme.typography);
        #[cfg(target_os = "linux")]
        let mut theme = Self::for_mode_linux(mode, cx.window_appearance(), base);
        #[cfg(not(target_os = "linux"))]
        let mut theme = Self::for_mode(mode, cx.window_appearance(), base);
        theme.typography = typography;
        let theme = theme.with_translucency_at(translucency, opacity);
        theme.install_into_bezel(cx);
        cx.set_global(theme);
    }

    /// Resolves and remembers the UI, code and terminal families from the
    /// families the runtime text system actually has installed. Idempotent
    /// — the first call wins — and called from [`Theme::install`] so the
    /// app has every answer before the first frame. The pure, testable
    /// forms are the free `resolve_ui_family`, `resolve_code_family` and
    /// `resolve_terminal_family`.
    pub fn resolve_font_families(cx: &App) {
        let installed: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
        let _ = UI_FAMILY
            .get_or_init(|| resolve_ui_family(&installed))
            .as_str();
        let _ = CODE_FAMILY
            .get_or_init(|| resolve_code_family(&installed))
            .as_str();
        let _ = TERMINAL_FAMILY
            .get_or_init(|| resolve_terminal_family(&installed))
            .as_str();
    }

    /// Resolves and remembers the code family from the families the runtime
    /// text system actually has installed. Idempotent — the first call
    /// wins — and kept as a convenience accessor beside the all-at-once
    /// [`Theme::resolve_font_families`] (which is what [`Theme::install`]
    /// calls) for callers that only need the mono answer. The pure,
    /// testable form is the free [`resolve_code_family`].
    pub fn resolve_code_family(cx: &App) -> &'static str {
        let installed: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
        CODE_FAMILY
            .get_or_init(|| resolve_code_family(&installed))
            .as_str()
    }

    /// Installs a replacement theme mode in the GPUI global.
    pub fn set_mode(mode: ThemeMode, cx: &mut App) {
        Self::install(mode, cx);
        #[cfg(target_os = "linux")]
        if mode == ThemeMode::System {
            // Returning to System re-asks the portal instead of trusting the
            // possibly-stale appearance cached at startup.
            Self::follow_portal(cx);
        }
    }

    /// The base colour the installed theme is painting with, or the default
    /// when no theme is installed yet.
    pub fn current_base_color(cx: &App) -> BaseColor {
        cx.try_global::<Self>()
            .map_or_else(BaseColor::default, |theme| theme.base_color)
    }

    /// Installs a replacement base colour, keeping the current mode.
    ///
    /// The mode is read back rather than passed in because the picker that
    /// calls this changes one axis and must not restate the other — a caller
    /// that guessed `System` here would undo an explicit Light or Dark.
    pub fn set_base_color(base: BaseColor, cx: &mut App) {
        let mode = cx
            .try_global::<Self>()
            .map_or(ThemeMode::System, |theme| theme.mode);
        let previous = cx.try_global::<Self>();
        let translucency = previous.is_some_and(|theme| theme.translucency_enabled);
        let opacity = previous.map_or(Self::surface_opacity(true), |theme| {
            theme.translucent_surface_opacity
        });
        let typography = previous.map_or_else(Typography::default, |theme| theme.typography);
        #[cfg(target_os = "linux")]
        let mut theme = Self::for_mode_linux(mode, cx.window_appearance(), base);
        #[cfg(not(target_os = "linux"))]
        let mut theme = Self::for_mode(mode, cx.window_appearance(), base);
        theme.typography = typography;
        let theme = theme.with_translucency_at(translucency, opacity);
        theme.install_into_bezel(cx);
        cx.set_global(theme);
    }

    /// Installs the system-following theme and starts the portal follower.
    ///
    /// The first frames are dark on Linux (see [`ThemeMode::resolve_system`]
    /// for why); once the XDG portal answers — or proves absent — the theme
    /// is re-resolved. A missing or silent portal keeps the dark fallback.
    pub fn init(cx: &mut App) {
        Self::install(ThemeMode::System, cx);
        #[cfg(target_os = "linux")]
        Self::follow_portal(cx);
    }

    /// Re-resolves a `System` theme from the XDG desktop portal's
    /// `color-scheme` setting, the same source GPUI's own appearance uses.
    /// No portal (or an error) leaves the current theme — the dark
    /// fallback — untouched.
    #[cfg(target_os = "linux")]
    fn follow_portal(cx: &mut App) {
        // zbus connects to D-Bus through its own blocking executor, whose
        // threads must never wake GPUI's single-threaded test scheduler
        // (it asserts on cross-thread activity). The real app never runs on
        // a thread named like a test, so this reliably skips test runs.
        if in_test_harness() {
            return;
        }
        cx.spawn(async move |cx| {
            let preference = portal_color_scheme().await;
            cx.update(|cx| {
                let Some(preference) = preference else { return };
                if cx.global::<Theme>().mode == ThemeMode::System {
                    let current = *cx.global::<Theme>();
                    let mut next =
                        Theme::for_appearance(ThemeMode::System, preference, current.base_color);
                    next.typography = current.typography;
                    let next = next.with_translucency_at(
                        current.translucency_enabled,
                        current.translucent_surface_opacity,
                    );
                    next.install_into_bezel(cx);
                    cx.set_global(next);
                }
            });
        })
        .detach();
    }

    /// Returns a theme resolved for a requested mode and system appearance.
    pub fn for_mode(mode: ThemeMode, system_appearance: WindowAppearance, base: BaseColor) -> Self {
        let appearance = resolve_mode(mode, system_appearance);
        Self::for_appearance(mode, appearance, base)
    }

    /// Linux resolution of `System`: dark until the portal speaks.
    #[cfg(target_os = "linux")]
    fn for_mode_linux(
        mode: ThemeMode,
        system_appearance: WindowAppearance,
        base: BaseColor,
    ) -> Self {
        let appearance = resolve_mode_linux(mode, system_appearance);
        Self::for_appearance(mode, appearance, base)
    }

    /// Returns a light theme without requiring a GPUI application context.
    pub fn light() -> Self {
        Self::for_appearance(ThemeMode::Light, Appearance::Light, BaseColor::Neutral)
    }

    /// Returns a dark theme without requiring a GPUI application context.
    pub fn dark() -> Self {
        Self::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Neutral)
    }

    /// Returns a system-mode theme resolved against the supplied appearance.
    ///
    /// On Linux this is the dark-biased resolution: a light appearance is
    /// the unresolved default until the portal is heard from.
    pub fn system(system_appearance: WindowAppearance, base: BaseColor) -> Self {
        #[cfg(target_os = "linux")]
        return Self::for_mode_linux(ThemeMode::System, system_appearance, base);
        #[cfg(not(target_os = "linux"))]
        Self::for_mode(ThemeMode::System, system_appearance, base)
    }

    /// Applies the persisted interface font size to the installed theme.
    pub fn set_interface_font_size(value: i32, cx: &mut App) {
        let mut theme = *cx.global::<Self>();
        theme.typography = Typography::for_interface_size(value.clamp(12, 18) as f32);
        cx.set_global(theme);
    }

    /// Returns the surface opacity used when translucency is enabled.
    ///
    /// The fade is 0.70 over a 0.85 (dark) / 0.80 (light) frame. The layers
    /// *stack*: a terminal pane paints terminal_surface over the panel's
    /// surface over the frame material, so the composite in panel areas
    /// covers ~96% of the backdrop (~99% where a third layer stacks), and
    /// the frame strips — title strip, status bar, the gaps between panels —
    /// let 15–20% of the blurred desktop through. The frame is the number
    /// that matters: the strips are the only place text sits directly on
    /// it, and over a white desktop (blur averages the backdrop to one tone)
    /// a 0.35 frame washed the dark shell's strips out to light grey and a
    /// 0.70 one left `fg_muted` at 3.1:1 there (2026-09-07); 0.85 clears
    /// WCAG AA with room. The panel fade barely moves that contrast (≥6:1
    /// across 0.70–0.90), so it stays where the glass still reads. Held by
    /// `translucent_shell_keeps_wcag_aa_over_an_opposing_desktop`.
    pub fn surface_opacity(translucency_enabled: bool) -> f32 {
        if translucency_enabled { 0.70 } else { 1.0 }
    }

    /// Returns the theme resolved for this theme's `mode` and `appearance`,
    /// with the structural surfaces faded to
    /// [`Theme::translucent_surface_opacity`] when `enabled` is true.
    ///
    /// Re-deriving from `mode` + `appearance` is the point: the opaque base
    /// is always recoverable, so toggling translucency never needs to
    /// remember what the unfaded surfaces were. The frame material
    /// (`frame_surface`) is already translucent by design and is not faded
    /// again; washes, borders, selection, and text keep full opacity so a
    /// translucent panel keeps its contrast. A surface that already carries an
    /// alpha is *scaled*, not overwritten — see
    /// `fading_an_already_translucent_surface_does_not_make_it_more_opaque`.
    /// The surfaces an event opens over the shell
    /// ([`SirioColors::dialog_surface`], [`SirioColors::floating_surface`])
    /// are not faded either: translucency is the main window's background,
    /// never a dialog's — see
    /// `event_opened_surfaces_stay_opaque_when_the_panels_fade`.
    pub fn with_translucency(self, enabled: bool) -> Self {
        self.with_translucency_at(enabled, Self::surface_opacity(true))
    }

    /// [`Theme::with_translucency`] with the fade's opacity chosen by the
    /// caller instead of [`Theme::surface_opacity`]'s default. The platform
    /// shell picks it: how much of the desktop a blurred backdrop should let
    /// through depends on how tinted the platform's own blur already is. The
    /// chosen opacity is remembered on the theme
    /// ([`Theme::translucent_surface_opacity`]) so every reinstall keeps it
    /// alongside the flag.
    pub fn with_translucency_at(self, enabled: bool, opacity: f32) -> Self {
        let mut theme = Self::for_appearance(self.mode, self.appearance, self.base_color);
        theme.typography = self.typography;
        theme.translucency_enabled = enabled;
        theme.translucent_surface_opacity = opacity;
        if !enabled {
            return theme;
        }
        // The same surfaces under their new names, plus the copies Ely was
        // handed *after* fading: overlay and tooltip_bg are surface_raised,
        // on_accent is surface (spec §3.5).
        let fade_hsla = |surface: Hsla| Hsla {
            a: surface.a * opacity,
            ..surface
        };
        let ely = &mut theme.colors.ely;
        for surface in [
            &mut ely.bg,
            &mut ely.surface,
            &mut ely.sunken,
            &mut ely.overlay,
            &mut ely.tooltip_bg,
            &mut ely.on_accent,
        ] {
            *surface = fade_hsla(*surface);
        }
        theme.colors.sirio.terminal_surface = fade_hsla(theme.colors.sirio.terminal_surface);
        // `dialog_surface` and `floating_surface` are deliberately absent:
        // a sheet or toast an event puts up stays opaque over the blur.
        theme
    }

    fn for_appearance(mode: ThemeMode, appearance: Appearance, base: BaseColor) -> Self {
        let colors = ThemeColors::for_appearance(appearance, base);
        Self {
            mode,
            appearance,
            base_color: base,
            colors,
            spacing: Spacing::default(),
            radii: Radii::default(),
            browser_chrome: BrowserChrome::default(),
            windows_caption: WindowsCaption::default(),
            typography: Typography::default(),
            translucent_surface_opacity: Self::surface_opacity(true),
            translucency_enabled: false,
        }
    }
}

/// Whether the current thread is a `cargo test` harness thread.
///
/// libtest names every test thread `<path>::tests::<name>`, so a thread
/// name containing `::tests::` reliably identifies a test run. GPUI's test
/// scheduler is single-threaded and asserts on cross-thread wakeups; the
/// portal query connects to D-Bus through zbus's own blocking executor,
/// which would trip that assertion. The real app never runs on a thread
/// named like a test.
#[cfg(target_os = "linux")]
fn in_test_harness() -> bool {
    looks_like_test_harness(
        std::thread::current().name(),
        std::env::current_exe().ok().as_deref(),
    )
}

/// Whether this process is a test binary, decided from the two signals a
/// library can see from inside a dependency. `cfg!(test)` is useless here:
/// it is set only for the crate being tested, and `sirio_theme` is always
/// the dependency, never that crate.
///
/// The thread name alone was the original check, and it missed: it required
/// the segment `tests::`, which assumes every test lives in a `mod tests`.
/// `sirio_ui`'s `changes::perf_baseline` declares its tests directly in the
/// module, so its threads are named `changes::perf_baseline::<test>` and the
/// portal query really ran — waking GPUI's single-threaded test scheduler
/// from zbus's blocking pool and panicking the test as non-deterministic.
///
/// The executable path is the signal that does not depend on how a module
/// spells its tests: cargo and nextest both run test binaries out of
/// `target/<profile>/deps/`, while every real Sirio build — the dev binary
/// at `target/<profile>/sirio`, the installed one, the one inside
/// `Sirio.app` — sits somewhere else.
fn looks_like_test_harness(thread_name: Option<&str>, current_exe: Option<&std::path::Path>) -> bool {
    if thread_name.is_some_and(|name| name.starts_with("tests::") || name.contains("::tests::")) {
        return true;
    }
    current_exe.is_some_and(|exe| {
        exe.parent()
            .is_some_and(|dir| dir.file_name().is_some_and(|dir| dir == "deps"))
    })
}

/// Queries the XDG desktop portal for the preferred color scheme.
///
/// Returns `None` when no portal is reachable or the query fails — the
/// caller keeps the dark fallback. `NoPreference` is an *answer* (the portal
/// is there, the desktop has no opinion) and resolves to light, matching
/// GPUI's own mapping.
#[cfg(target_os = "linux")]
async fn portal_color_scheme() -> Option<Appearance> {
    let settings = ashpd::desktop::settings::Settings::new().await.ok()?;
    Some(appearance_from_color_scheme(
        settings.color_scheme().await.ok()?,
    ))
}

/// Maps the portal's `color-scheme` enum to an appearance. Kept separate so
/// the mapping is testable without a live portal.
#[cfg(target_os = "linux")]
fn appearance_from_color_scheme(scheme: ashpd::desktop::settings::ColorScheme) -> Appearance {
    use ashpd::desktop::settings::ColorScheme;
    match scheme {
        ColorScheme::PreferDark => Appearance::Dark,
        ColorScheme::PreferLight | ColorScheme::NoPreference => Appearance::Light,
    }
}

/// A supported agent's own brand colour — deliberately **not** a [`Theme`]
/// token.
///
/// The reference keeps these two things apart on purpose. `Theme`'s eight
/// semantic tokens say what a thing *means* (needs input, done, error…);
/// a brand colour says *whose* it is, and must therefore stay stable across
/// appearances and never be reachable from the semantic palette. Swift makes
/// the same split explicitly, in `App/AgentAccentColor.swift`'s own words:
/// it is "unrelated to `AgentIcon.color(for:)`, which is a different,
/// pre-existing mapping".
///
/// The values are Swift's `AgentAccentColor.defaultHexByAgentId` — the one
/// reference table that (a) is written as literal brand hexes rather than
/// platform system colours, (b) covers all five catalog agents, and (c) the
/// Rust settings surface already ships as `SettingsSnapshot::agent_colors`
/// defaults. Two of them are corroborated by the marks themselves:
/// `AgentIcon.claudeOrange` fills the Claude mark with this exact `D97757`,
/// and `9B4DFF` is the middle stop of `OmpShape`'s `ED4ABF → 9B4DFF →
/// 5AD8E6` gradient.
///
/// **Why this exists at all.** The worktree row's running indicator used to
/// take its tint from the eight-token `AgentAccentColor` picker enum, where
/// Claude resolved to `Amber` — i.e. to `theme.warning` itself. A
/// Claude worktree that was *running* therefore painted the byte-identical
/// `#E0B36A` as a worktree that **needed input**, leaving a 3×3 triple dot
/// and a 6×6 single dot as the only difference between two states a user has
/// to tell apart at sidebar scale. Swift has no such collision: needs-input
/// is `.dot(.amber)` and Claude-running is `RunningDots` in Claude's own
/// colour. Routing brand identity through its own table restores that, for
/// every agent, by construction rather than by careful token choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentBrandColor {
    Claude,
    Codex,
    OpenCode,
    Pi,
    Omp,
    /// Any agent id outside the catalog. Swift's `AgentAccentColor.fallbackHex`
    /// — "future adapters land here until given a real default".
    Unknown,
}

impl AgentBrandColor {
    /// Resolves an `AgentCatalog` id to its brand. The `-acp` suffix is
    /// stripped first, matching `AgentIcon.normalizedId`, so the ACP-bridged
    /// variant of an agent wears the same colour as the CLI it fronts.
    pub fn for_agent_id(agent_id: &str) -> Self {
        match agent_id.strip_suffix("-acp").unwrap_or(agent_id) {
            "claude" => Self::Claude,
            "codex" => Self::Codex,
            "opencode" => Self::OpenCode,
            "pi" => Self::Pi,
            "omp" => Self::Omp,
            _ => Self::Unknown,
        }
    }

    /// The brand hex, as an opaque `0xRRGGBB` literal. Appearance-invariant:
    /// a brand does not have a light and a dark variant, and Swift stores
    /// exactly one hex per agent for the same reason.
    pub fn hex(self) -> u32 {
        match self {
            Self::Claude => 0xD97757,
            Self::Codex => 0x0A84FF,
            Self::OpenCode => 0xFF9500,
            Self::Pi => 0x34C759,
            Self::Omp => 0x9B4DFF,
            Self::Unknown => 0x8E8E93,
        }
    }

    /// The brand colour itself.
    pub fn color(self) -> Rgba {
        rgb_hex(self.hex())
    }
}

/// Opaque sRGB color from a `0xRRGGBB` literal.
fn rgb_hex(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far the frozen body text sits back from bezel's full-contrast rung
    /// toward its surface. Moved here from the derivation it used to
    /// parameterise: `body_text_is_softened_off_bezels_full_contrast` still
    /// compares the frozen `ely.fg` with the rule, so the constant and mixer
    /// live with the preset agreement test.
    const TEXT_SOFTENING: f32 = 0.10;

    /// Mixes `fraction` of `target` into `color`, opaquely.
    fn toward(color: Rgba, target: Rgba, fraction: f32) -> Rgba {
        Rgba {
            r: color.r + (target.r - color.r) * fraction,
            g: color.g + (target.g - color.g) * fraction,
            b: color.b + (target.b - color.b) * fraction,
            a: color.a,
        }
    }

    mod looks_like_test_harness {
        use std::path::Path;

        /// The name the original guard was written for.
        #[test]
        fn a_thread_inside_a_mod_tests_is_a_test() {
            assert!(super::super::looks_like_test_harness(
                Some("changes::tests::a_drawn_frame_is_counted"),
                None
            ));
        }

        /// The case that got through and panicked the suite: `perf_baseline`
        /// declares its tests in the module itself, so no `tests::` segment
        /// ever appears in the thread name.
        #[test]
        fn a_test_declared_outside_a_mod_tests_is_still_a_test() {
            assert!(
                super::super::looks_like_test_harness(
                    Some("changes::perf_baseline::drawn_frames_count_rebuilds_and_payload_copies_separately"),
                    Some(Path::new("/w/rust/target/debug/deps/sirio_ui-2f1c9a")),
                ),
                "a test binary under deps/ is a test harness whatever its threads are named"
            );
        }

        /// An installed copy must follow the portal like any real run.
        #[test]
        fn an_installed_binary_is_not_a_test_harness() {
            assert!(!super::super::looks_like_test_harness(
                Some("main"),
                Some(Path::new("/usr/bin/sirio"))
            ));
        }
    }

    #[test]
    fn named_covers_every_sirio_color_field() {
        let named = Theme::dark().sirio.named();
        assert_eq!(
            named.len(),
            std::mem::size_of::<SirioColors>() / std::mem::size_of::<Hsla>(),
            "SirioColors::named() must include every field"
        );
    }

    #[test]
    fn a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone() {
        // Decision B4: the coral is Sirio's identity, anchored by two
        // measured constraints. The dark terminal now follows the pane's
        // tinted surface; the light terminal intentionally remains paper.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let slate = ThemeColors::for_appearance(appearance, BaseColor::Slate);

            assert_ne!(slate.sirio.canvas, neutral.sirio.canvas, "{appearance:?} page takes the tint");
            assert_ne!(slate.ely.bg, neutral.ely.bg);
            // The borders stay verbatim, on purpose: bezel 0.1.3 tints only
            // opaque achromatic ink — `Brand::apply`: "Translucent ink is
            // skipped because it paints over whatever is beneath it, which is
            // tinted already" — and both borders are 8–10% veils. Their
            // relationship to bezel's preset values is checked across every
            // base by `frozen_presets_match_their_bezel_ladders`.
            assert_eq!(slate.ely.border, neutral.ely.border);

            assert_eq!(
                slate.sirio.brand_coral, neutral.sirio.brand_coral,
                "{appearance:?} coral is Sirio's, not bezel's to rotate"
            );
            if appearance == Appearance::Light {
                // Tinted panes keep the paper terminal; Neutral follows its
                // own approved page instead.
                assert_ne!(
                    slate.sirio.terminal_surface, neutral.sirio.terminal_surface,
                    "light: tinted panes keep paper, neutral follows its page"
                );
            }
        }
    }

    #[test]
    fn dark_terminal_surface_matches_the_pane_surface() {
        for base in BaseColor::ALL {
            let theme = Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, base);

            if matches!(base, BaseColor::Neutral | BaseColor::Onice) {
                // The grey ladders' central panes are their own plane, darker
                // than the sidebars.
                assert_eq!(
                    theme.sirio.terminal_surface, theme.sirio.canvas,
                    "{base:?} dark terminal is the central pane"
                );
            } else {
                assert_eq!(
                    theme.sirio.terminal_surface, theme.ely.bg,
                    "dark terminal background must match the pane for {base:?}"
                );
            }
        }
    }

    #[test]
    fn translucency_preserves_the_chosen_base_colour() {
        // `with_translucency` rebuilds the palette from the theme's own
        // fields; the base colour has to be one of them or the toggle
        // silently resets the user's choice.
        let theme = Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Slate);
        let translucent = theme.with_translucency(true);
        assert_eq!(translucent.base_color, BaseColor::Slate);
        assert_eq!(
            translucent.colors.ely.border, theme.colors.ely.border,
            "a non-faded token keeps the tinted value"
        );
    }

    /// The bezel theme Notte builds, read through the path bezel's own
    /// widgets use, so the test covers both consumers of the builder.
    fn notte_bezel(appearance: Appearance) -> bezel_theme::Theme {
        let mode = match appearance {
            Appearance::Dark => ThemeMode::Dark,
            Appearance::Light => ThemeMode::Light,
        };
        Theme::for_appearance(mode, appearance, BaseColor::Notte).to_bezel_theme()
    }

    /// bezel's palette rotated onto Notte's tint and nothing else — what
    /// Notte would be if it were only a sixth base colour.
    fn notte_tint_only(appearance: bezel_theme::Appearance) -> bezel_theme::Theme {
        bezel_theme::Theme::branded(
            &bezel_theme::Brand {
                tint: BaseColor::Notte.tint(),
                ..Default::default()
            },
            appearance,
        )
    }

    #[test]
    fn notte_light_is_only_a_tint() {
        // Four dark surfaces were given and no light ones; inventing a light
        // ladder was rejected (spec N3).
        let notte = notte_bezel(Appearance::Light);
        let tinted = notte_tint_only(bezel_theme::Appearance::Light);
        for (name, ours, theirs) in [
            ("bg", notte.bg, tinted.bg),
            ("surface", notte.surface, tinted.surface),
            ("surface_card", notte.surface_card, tinted.surface_card),
            (
                "surface_raised",
                notte.surface_raised,
                tinted.surface_raised,
            ),
            (
                "surface_dialog",
                notte.surface_dialog,
                tinted.surface_dialog,
            ),
            (
                "surface_overlay",
                notte.surface_overlay,
                tinted.surface_overlay,
            ),
            (
                "surface_raised_hover",
                notte.surface_raised_hover,
                tinted.surface_raised_hover,
            ),
            ("text", notte.text, tinted.text),
            ("border", notte.border, tinted.border),
        ] {
            assert_eq!(ours, theirs, "light {name}");
        }
    }

    #[test]
    fn notte_body_text_clears_aaa_on_every_surface() {
        let sirio = ThemeColors::for_appearance(Appearance::Dark, BaseColor::Notte);
        let bezel = notte_bezel(Appearance::Dark);
        for (name, surface) in [
            ("bg", sirio.sirio.canvas),
            ("surface", sirio.ely.bg),
            ("surface_raised", sirio.ely.surface),
            (
                "surface_raised_hover",
                bezel.surface_raised_hover,
            ),
        ] {
            let ratio = contrast_ratio(sirio.ely.fg, surface);
            assert!(ratio >= 7.0, "text on {name} is {ratio:.1}:1, below AAA");
        }
    }

    #[test]
    fn notte_depth_ladder_reads_as_depth() {
        // Ordering only: the given hover step is small, and that is the
        // user's ladder as given (spec, Risks).
        let bezel = notte_bezel(Appearance::Dark);
        let rungs = [
            ("bg", bezel.bg),
            ("surface", bezel.surface),
            ("surface_raised", bezel.surface_raised),
            ("surface_raised_hover", bezel.surface_raised_hover),
        ];
        for pair in rungs.windows(2) {
            let (lower, upper) = (pair[0], pair[1]);
            assert!(
                relative_luminance(Rgba::from(lower.1)) < relative_luminance(Rgba::from(upper.1)),
                "{} should sit below {}",
                lower.0,
                upper.0
            );
        }
    }

    /// The primary text rung is bezel's, softened — not bezel's as-is, and not
    /// a hand-picked hex either.
    ///
    /// bezel paints body text at full contrast against its page. Sirio's
    /// surfaces carry more text per screen (a sidebar, a tab strip and a file
    /// tree at once), where that reads as glare. The target is the contrast
    /// Sirio shipped before adopting bezel, so this pins the *relationship* —
    /// softened toward the surface, still past AAA, still clearly ahead of
    /// `fg_muted`.
    #[test]
    fn body_text_is_softened_off_bezels_full_contrast() {
        for (appearance, bezel) in [
            (Appearance::Dark, bezel_theme::Theme::dark()),
            (Appearance::Light, bezel_theme::Theme::light()),
        ] {
            let sirio = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let full = bezel.text;

            assert_ne!(
                sirio.ely.fg, full,
                "{appearance:?} text must not be bezel's full-contrast rung"
            );
            // Softer than bezel, but not into the muted rung's territory: the
            // ladder still has a visible first step.
            let step = contrast_ratio(sirio.ely.fg, sirio.ely.bg);
            assert!(
                step > contrast_ratio(sirio.ely.fg_muted, sirio.ely.bg),
                "{appearance:?} text ({step:.1}:1) must stay ahead of text_muted"
            );
            assert!(
                step > 7.0,
                "{appearance:?} text is {step:.1}:1 — softening must not spend WCAG AAA"
            );
            assert!(
                step < contrast_ratio(full, sirio.ely.bg),
                "{appearance:?} text must be softer than bezel's"
            );
        }
    }

    #[test]
    fn frozen_presets_match_their_bezel_ladders() {
        const RGB_TOLERANCE: f32 = 0.5 / 255.0;

        for base in BaseColor::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let (ely, sirio) = presets::preset(base, appearance);
                let bezel = bezel_theme_for(base, appearance);
                let softened_body = Hsla::from(toward(
                    Rgba::from(bezel.text),
                    Rgba::from(bezel.surface),
                    TEXT_SOFTENING,
                ));
                let pairs = [
                    ("ely.bg", ely.bg, bezel.surface),
                    ("ely.surface", ely.surface, bezel.surface_raised),
                    ("sirio.canvas", sirio.canvas, bezel.bg),
                    ("ely.sunken", ely.sunken, bezel.input_bg),
                    ("ely.hover", ely.hover, bezel.element_hover),
                    ("ely.border", ely.border, bezel.border),
                    ("ely.fg", ely.fg, softened_body),
                    ("ely.fg_muted", ely.fg_muted, bezel.text_muted),
                    ("ely.fg_subtle", ely.fg_subtle, bezel.text_faint),
                    ("sirio.text_dim", sirio.text_dim, bezel.text_dim),
                ];

                for (name, preset, bezel_origin) in pairs {
                    let preset = Rgba::from(preset);
                    let bezel_round_trip = Hsla::from(Rgba::from(bezel_origin));
                    let bezel = Rgba::from(bezel_round_trip);
                    for (channel, preset, bezel) in [
                        ("red", preset.r, bezel.r),
                        ("green", preset.g, bezel.g),
                        ("blue", preset.b, bezel.b),
                    ] {
                        assert!(
                            (preset - bezel).abs() <= RGB_TOLERANCE,
                            "{base:?} {appearance:?} {name} {channel} differs: preset {preset}, bezel {bezel}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn graph_lane_colours_cycle_and_stay_distinct() {
        for theme in [Theme::dark(), Theme::light()] {
            let lanes: Vec<_> = (0..6).map(|index| theme.graph_lane(index)).collect();

            for (position, first) in lanes.iter().enumerate() {
                for second in lanes.iter().skip(position + 1) {
                    assert_ne!(first, second, "lane colours must be distinguishable");
                }
            }
            assert_eq!(
                theme.graph_lane(6),
                theme.graph_lane(0),
                "the palette cycles"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn libtest_root_thread_name_is_recognized_as_test_harness() {
        let thread = std::thread::Builder::new()
            .name("tests::theme_portal_guard".into())
            .spawn(in_test_harness)
            .expect("spawn named test-harness probe");
        assert!(
            thread.join().expect("named test-harness probe completes"),
            "libtest names test threads tests::<name>, without a leading ::"
        );
    }

    /// WCAG 2.1 relative luminance of an opaque colour.
    fn relative_luminance(color: impl Into<Rgba>) -> f32 {
        let color = color.into();
        let channel = |c: f32| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    }

    fn contrast_ratio(one: impl Into<Rgba>, other: impl Into<Rgba>) -> f32 {
        let (a, b) = (relative_luminance(one), relative_luminance(other));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn shell_body_text_meets_wcag_aa_on_its_panel() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (role, text) in [("primary", theme.ely.fg), ("secondary", theme.ely.fg_muted)] {
                let ratio = contrast_ratio(text, theme.ely.bg);
                assert!(
                    ratio >= 4.5,
                    "{label} {role} text contrast on the panel is {ratio:.2}:1, under WCAG AA"
                );
            }
        }
    }

    #[test]
    fn interface_font_size_shifts_the_whole_typography_scale() {
        let typography = Typography::for_interface_size(16.0);

        assert_eq!(typography.base_size, px(17.5));
        assert_eq!(typography.code_size, px(16.0));
        assert_eq!(typography.code_line_height, px(22.0));
        assert_eq!(typography.ui_size, px(16.0));
        assert_eq!(typography.body_line_height, px(25.0));
        assert_eq!(typography.ui_line_height, px(20.0));
        assert_eq!(typography.scaled(12.0), px(15.0));
    }

    /// Composites `over` (which may be translucent) onto `under`, so a wash
    /// can be judged the way a reader actually sees it.
    fn composite(over: impl Into<Rgba>, under: impl Into<Rgba>) -> Hsla {
        let (over, under) = (over.into(), under.into());
        let a = over.a;
        Hsla::from(Rgba {
            r: over.r * a + under.r * (1.0 - a),
            g: over.g * a + under.g * (1.0 - a),
            b: over.b * a + under.b * (1.0 - a),
            a: 1.0,
        })
    }

    /// A well is always tellable from the page it is cut into, and from a card
    /// raised above it.
    ///
    /// Sirio derived `inset` by darkening the page, so the ladder ran
    /// card > page > well by luminance. bezel builds the well by lightening
    /// instead — in dark it is a translucent white veil, in light it is pure
    /// white on a grey page — so the ordering is the other way up and a
    /// luminance comparison no longer names the property. What has to hold is
    /// the one the ordering existed for: the three planes stay distinct.
    #[test]
    fn the_depth_ladder_reads_as_depth() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let page = composite(theme.ely.sunken, theme.ely.bg);
            let card = composite(theme.ely.sunken, theme.ely.surface);
            assert_ne!(
                page, theme.ely.bg,
                "{label}: the well vanishes into the page"
            );
            assert_ne!(
                card, theme.ely.surface,
                "{label}: the well vanishes into a card"
            );
            assert_ne!(
                theme.ely.bg, theme.ely.surface,
                "{label}: the page and a raised card are the same plane"
            );
        }
    }

    /// An inverted chip reads against its own fill.
    ///
    /// Sirio used to build the pair by mirroring the other appearance's page
    /// and text, which made the property an identity. bezel picks `solid` and
    /// `on_solid` independently, so what is left to hold is the reason the
    /// mirror existed: a tooltip or keycap has to be legible.
    #[test]
    fn an_inverted_chip_is_legible_on_its_own_fill() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let ratio = contrast_ratio(theme.sirio.on_solid, theme.sirio.solid);
            assert!(
                ratio >= 4.5,
                "{label}: on_solid against solid is {ratio:.2}:1, under WCAG AA"
            );
        }
    }

    /// Selected text stays readable through its own selection wash.
    ///
    /// Selection used to be the coral turned down, and the coral is a mid-lightness
    /// coral, so this is the constraint that fixes *how far* down: the two
    /// alphas are the loudest each appearance can take while the glyphs under
    /// them still clear WCAG AA.
    #[test]
    fn selection_stays_under_its_text() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let seen = composite(theme.ely.selection, theme.ely.bg);
            let ratio = contrast_ratio(theme.ely.fg, seen);
            assert!(
                ratio >= 4.5,
                "{label}: selected text reads at {ratio:.2}:1 through its own wash"
            );
        }
    }

    /// The coral must stay legible on the surface it is painted on, in both
    /// appearances.
    ///
    /// This is the rule the light coral is *derived* from rather than a
    /// property observed after the fact, which is the point: the value it
    /// replaced was carried over from another project's source and managed
    /// only 3.73:1 here, so nothing but a test keeps a future edit from
    /// drifting back under the line.
    #[test]
    fn brand_coral_clears_contrast_on_its_own_surface() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let ratio = contrast_ratio(theme.sirio.brand_coral, theme.ely.bg);
            assert!(
                ratio >= 4.5,
                "{label}: coral contrast against its surface is {ratio:.2}:1, under WCAG AA 4.5:1"
            );
        }
    }

    /// The coral must never be one of the agent brands.
    ///
    /// A tab row paints its coral and its agent's mark at the same time, so
    /// a coral that lands on a brand makes that mark stop distinguishing
    /// anything — the row looks identically tinted whichever agent is running.
    /// This is not hypothetical: Claude's `#D97757` is the nearest brand to
    /// where the coral sits, so the tempting move of reusing our own Swift's
    /// Claude fill is exactly the one that breaks it.
    #[test]
    fn brand_coral_is_not_any_agent_brand() {
        let brands = [
            AgentBrandColor::Claude,
            AgentBrandColor::Codex,
            AgentBrandColor::OpenCode,
            AgentBrandColor::Pi,
            AgentBrandColor::Omp,
            AgentBrandColor::Unknown,
        ];
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for brand in brands {
                assert_ne!(
                    theme.sirio.brand_coral,
                    Hsla::from(brand.color()),
                    "{label}: the coral is {brand:?}'s brand, so that agent's mark \
                     no longer marks anything"
                );
            }
        }
    }

    #[test]
    fn every_adaptive_token_differs_between_light_and_dark() {
        let light = Theme::light().colors;
        let dark = Theme::dark().colors;
        let tokens = [
            ("frame_surface", light.sirio.frame_surface, dark.sirio.frame_surface),
            ("bg", light.sirio.canvas, dark.sirio.canvas),
            ("surface", light.ely.bg, dark.ely.bg),
            ("border_opaque", light.ely.border, dark.ely.border),
            (
                "terminal_surface",
                light.sirio.terminal_surface,
                dark.sirio.terminal_surface,
            ),
            ("warning", light.ely.warning, dark.ely.warning),
            ("success", light.ely.success, dark.ely.success),
            ("danger", light.ely.danger, dark.ely.danger),
            ("border", light.ely.border, dark.ely.border),
            ("element_hover", light.ely.hover, dark.ely.hover),
            ("element_active", light.ely.active, dark.ely.active),
            ("selection", light.ely.selection, dark.ely.selection),
            ("text", light.ely.fg, dark.ely.fg),
            ("text_muted", light.ely.fg_muted, dark.ely.fg_muted),
            ("text_faint", light.ely.fg_subtle, dark.ely.fg_subtle),
            ("tree_guide", light.sirio.tree_guide, dark.sirio.tree_guide),
            ("git_untracked", light.ely.fg_subtle, dark.ely.fg_subtle),
            ("diff_add", light.sirio.diff_add, dark.sirio.diff_add),
            ("diff_add_bg", light.sirio.diff_add_bg, dark.sirio.diff_add_bg),
            ("diff_del", light.sirio.diff_del, dark.sirio.diff_del),
            ("diff_del_bg", light.sirio.diff_del_bg, dark.sirio.diff_del_bg),
            ("file_link", light.ely.link, dark.ely.link),
            ("surface_raised", light.ely.surface, dark.ely.surface),
            ("input_bg", light.ely.sunken, dark.ely.sunken),
            ("overlay", light.sirio.overlay, dark.sirio.overlay),
            ("overlay_strong", light.sirio.overlay_strong, dark.sirio.overlay_strong),
            ("border_strong", light.sirio.border_strong, dark.sirio.border_strong),
            ("ring", light.sirio.ring, dark.sirio.ring),
            ("text_dim", light.sirio.text_dim, dark.sirio.text_dim),
            ("brand_coral", light.sirio.brand_coral, dark.sirio.brand_coral),
            ("accent", light.sirio.quantity, dark.sirio.quantity),
            ("selection", light.ely.selection, dark.ely.selection),
            ("code_wash", light.sirio.code_wash, dark.sirio.code_wash),
            ("solid", light.sirio.solid, dark.sirio.solid),
            ("on_solid", light.sirio.on_solid, dark.sirio.on_solid),
            ("favorite", light.ely.warning, dark.ely.warning),
            ("danger_muted", light.sirio.danger_muted, dark.sirio.danger_muted),
        ];

        for (name, light, dark) in tokens {
            assert_ne!(light, dark, "adaptive token {name} did not change");
        }
    }

    #[test]
    fn system_mode_follows_window_appearance() {
        assert_eq!(
            Theme::system(WindowAppearance::Dark, BaseColor::Neutral).appearance,
            Appearance::Dark
        );
        assert_eq!(
            Theme::system(WindowAppearance::VibrantDark, BaseColor::Neutral).appearance,
            Appearance::Dark
        );
        // A light appearance is the unresolved default on Linux (the portal
        // answer arrives asynchronously), so it resolves to dark there; on
        // macOS `NSAppearance` is synchronous and authoritative.
        #[cfg(target_os = "linux")]
        assert_eq!(
            Theme::system(WindowAppearance::Light, BaseColor::Neutral).appearance,
            Appearance::Dark,
            "Linux must start dark until the portal is heard from"
        );
        #[cfg(not(target_os = "linux"))]
        assert_eq!(
            Theme::system(WindowAppearance::Light, BaseColor::Neutral).appearance,
            Appearance::Light
        );
    }

    /// Fading a surface that is *already* translucent must not make it more
    /// opaque than it was.
    ///
    /// The regression this pins: `fade` used to overwrite alpha rather than
    /// scale it, which was invisible while every faded surface was opaque
    /// (1.0 -> 0.85). bezel's dark `input_bg` is a 3% white veil, so
    /// overwriting turned the filter field into an 85% *white* fill on a
    /// near-black sidebar.
    #[test]
    fn fading_an_already_translucent_surface_does_not_make_it_more_opaque() {
        // Pinned on Slate: Neutral's input is an opaque fill since its own
        // ladder, so the veil regression this guards needs a tinted base.
        let base = Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Slate);
        assert!(
            base.ely.sunken.a < 0.5,
            "precondition: dark input_bg is a veil, not a fill (got {})",
            base.ely.sunken.a
        );

        let translucent = base.with_translucency(true);
        assert!(
            translucent.ely.sunken.a <= base.ely.sunken.a,
            "fading made the veil more opaque: {} -> {}",
            base.ely.sunken.a,
            translucent.ely.sunken.a
        );
    }

    /// Translucency belongs to the main window's background alone. A surface
    /// an event opens over the shell — the New Worktree prompt, a project
    /// form, a modal sheet, a toast — paints one of the two tokens below,
    /// and neither may fade with the structural panels: a sheet the desktop
    /// shows through is unreadable exactly when it is asking for input.
    #[test]
    fn event_opened_surfaces_stay_opaque_when_the_panels_fade() {
        for base in [Theme::dark(), Theme::light()] {
            assert_eq!(
                base.sirio.dialog_surface, base.ely.bg,
                "opaque twin of the panel surface"
            );
            assert_eq!(
                base.sirio.floating_surface, base.ely.surface,
                "opaque twin of the raised surface"
            );
            for translucent in [
                base.with_translucency(true),
                base.with_translucency_at(true, 0.70),
                base.with_translucency_at(true, 0.9),
            ] {
                assert!(
                    translucent.ely.bg.a < base.ely.bg.a,
                    "precondition: panels fade"
                );
                assert!(
                    translucent.ely.surface.a < base.ely.surface.a,
                    "precondition: raised cards fade"
                );
                assert_eq!(translucent.sirio.dialog_surface, base.sirio.dialog_surface);
                assert_eq!(translucent.sirio.floating_surface, base.sirio.floating_surface);
                assert_eq!(translucent.sirio.dialog_surface.a, 1.0);
                assert_eq!(translucent.sirio.floating_surface.a, 1.0);
            }
        }
    }

    /// The distinctions bezel's own palette does not make. Its hues come
    /// from the git graph's lanes, which paint `fn` and a call site the same
    /// indigo and a type the same amber as a number — the collapse this
    /// palette exists to undo.
    #[test]
    fn source_tokens_keep_the_roles_zed_distinguishes() {
        use bezel_theme::HighlightKind as Kind;
        for base in [Theme::dark(), Theme::light()] {
            let palette = base.syntax_palette();
            let distinct = [
                (Kind::Keyword, Kind::Function),
                (Kind::TypeName, Kind::Number),
                (Kind::Number, Kind::String),
                (Kind::Property, Kind::Variable),
                (Kind::Punctuation, Kind::Variable),
            ];
            for (left, right) in distinct {
                assert_ne!(
                    palette.color(left),
                    palette.color(right),
                    "{left:?} and {right:?} must not share a colour"
                );
            }
            assert_eq!(
                palette.color(Kind::Variable),
                base.ely.fg,
                "a plain identifier stays the body text colour"
            );
        }
    }

    /// A context menu is opened by an interaction and must stay readable
    /// over a blurred shell. It is the same contract
    /// `event_opened_surfaces_stay_opaque_when_the_panels_fade` holds for
    /// dialogs and toasts — a menu is no less event-opened than a toast.
    #[test]
    fn a_menu_stays_opaque_over_a_blurred_shell() {
        for base in [Theme::dark(), Theme::light()] {
            let opaque = base.menu_surface();
            let translucent = base.with_translucency(true);
            let translucent_menu = translucent.menu_surface();
            assert!(
                translucent.ely.surface.a < 1.0,
                "the fixture must actually fade the panels, or this proves nothing"
            );
            assert_eq!(
                translucent_menu.a,
                1.0,
                "a context menu the desktop shows through is unreadable"
            );
            assert_eq!(
                (translucent_menu.h, translucent_menu.s, translucent_menu.l),
                (opaque.h, opaque.s, opaque.l),
                "staying opaque must not change the menu's tone"
            );
        }
    }

    #[test]
    fn translucency_fades_every_surface_ely_receives() {
        for base in BaseColor::ALL {
            for appearance in [Appearance::Dark, Appearance::Light] {
                let mode = match appearance {
                    Appearance::Dark => ThemeMode::Dark,
                    Appearance::Light => ThemeMode::Light,
                };
                let opaque = Theme::for_appearance(mode, appearance, base);
                let faded = opaque.with_translucency_at(true, 0.7);
                let (o, f) = (opaque.colors, faded.colors);
                let pairs = [
                    ("ely.bg", o.ely.bg, f.ely.bg),
                    ("ely.surface", o.ely.surface, f.ely.surface),
                    ("ely.sunken", o.ely.sunken, f.ely.sunken),
                    ("ely.overlay", o.ely.overlay, f.ely.overlay),
                    ("ely.tooltip_bg", o.ely.tooltip_bg, f.ely.tooltip_bg),
                    ("ely.on_accent", o.ely.on_accent, f.ely.on_accent),
                    (
                        "sirio.terminal_surface",
                        o.sirio.terminal_surface,
                        f.sirio.terminal_surface,
                    ),
                ];
                for (name, before, after) in pairs {
                    assert_eq!(
                        after.a,
                        before.a * 0.7,
                        "{base:?}/{appearance:?}: {name} not faded"
                    );
                    assert_eq!((after.h, after.s, after.l), (before.h, before.s, before.l), "{name}");
                }
                // An event-opened sheet and the frame material never fade.
                assert_eq!(f.sirio.dialog_surface, o.sirio.dialog_surface);
                assert_eq!(f.sirio.floating_surface, o.sirio.floating_surface);
                assert_eq!(f.sirio.frame_surface, o.sirio.frame_surface);
                // Text and washes keep full strength.
                assert_eq!(f.ely.fg, o.ely.fg);
                assert_eq!(f.ely.border, o.ely.border);
            }
        }
    }

    #[test]
    fn with_translucency_at_fades_to_the_given_opacity_and_remembers_it() {
        for base in [Theme::dark(), Theme::light()] {
            let subtle = base.with_translucency_at(true, 0.9);
            assert!(subtle.translucency_enabled);
            assert_eq!(subtle.translucent_surface_opacity, 0.9);
            assert!((subtle.ely.bg.a - base.ely.bg.a * 0.9).abs() < 1e-6);
            assert!((subtle.sirio.terminal_surface.a - base.sirio.terminal_surface.a * 0.9).abs() < 1e-6);
            // Re-deriving from the faded theme keeps the caller's opacity, so
            // a mode switch that rebuilds the palette cannot fall back to the
            // default fade.
            let again = subtle.with_translucency_at(
                subtle.translucency_enabled,
                subtle.translucent_surface_opacity,
            );
            assert_eq!(again.ely.bg, subtle.ely.bg);
            let opaque = subtle.with_translucency_at(false, 0.9);
            assert!(!opaque.translucency_enabled);
            assert_eq!(opaque.ely.bg, base.ely.bg);
            assert_eq!(
                opaque.translucent_surface_opacity, 0.9,
                "disabling keeps the opacity for the next enable"
            );
        }
    }

    #[test]
    fn with_translucency_fades_structural_surfaces_and_is_reversible() {
        for base in [Theme::dark(), Theme::light()] {
            let opacity = Theme::surface_opacity(true);
            let translucent = base.with_translucency(true);

            assert!(translucent.translucency_enabled);
            let faded = |s: Hsla| Hsla {
                a: s.a * opacity,
                ..s
            };
            assert_eq!(translucent.ely.bg, faded(base.ely.bg));
            assert_eq!(translucent.ely.surface, faded(base.ely.surface));
            assert_eq!(translucent.ely.sunken, faded(base.ely.sunken));
            assert_eq!(translucent.sirio.terminal_surface, faded(base.sirio.terminal_surface));

            assert_eq!(
                translucent.sirio.frame_surface, base.sirio.frame_surface,
                "the frame material is already translucent and is not faded twice"
            );
            assert_eq!(translucent.sirio.canvas, base.sirio.canvas, "the opaque fallback stays opaque");
            assert_eq!(
                translucent.ely.border, base.ely.border,
                "borders stay crisp on a translucent panel"
            );
            assert_eq!(translucent.ely.fg, base.ely.fg, "text is untouched");

            assert_eq!(
                translucent.with_translucency(true),
                translucent,
                "re-derivation is idempotent"
            );
            assert_eq!(
                translucent.with_translucency(false),
                base,
                "the opaque base is fully recoverable from mode + appearance"
            );
        }
    }

    /// The translucent shell must stay legible over the desktop that fights
    /// its appearance hardest: a white desktop under the dark shell, a black
    /// one under the light shell. Blur averages the backdrop to one tone, so
    /// that tone is what the veils composite onto. Seen 2026-09-07: a 0.35
    /// frame over white washed the dark status strip to light grey.
    #[test]
    fn translucent_shell_keeps_wcag_aa_over_an_opposing_desktop() {
        let white = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let black = Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        for (label, theme, desktop) in [
            ("dark", Theme::dark().with_translucency(true), white),
            ("light", Theme::light().with_translucency(true), black),
        ] {
            let strip = composite(theme.sirio.frame_surface, desktop);
            let panel = composite(theme.ely.bg, strip);
            let terminal = composite(theme.sirio.terminal_surface, panel);
            for (place, under, text) in [
                ("frame strip", strip, theme.ely.fg_muted),
                ("panel", panel, theme.ely.fg_muted),
                ("terminal", terminal, theme.ely.fg),
            ] {
                let ratio = contrast_ratio(text, under);
                assert!(
                    ratio >= 4.5,
                    "{label} {place} over an opposing desktop: text contrast {ratio:.2}:1, under WCAG AA"
                );
            }
        }
    }

    /// The code family picks the first installed candidate in preference
    /// order — never the first family the machine happens to have.
    #[test]
    fn code_family_resolution_prefers_candidates_in_order() {
        let installed: HashSet<String> = ["DejaVu Sans Mono", "Fira Mono", "Hack"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(resolve_code_family(&installed), "Fira Mono");
    }

    /// The UI family picks the first installed candidate in preference
    /// order, and the JetBrains Sans face (open source, Apache 2.0) wins
    /// when it is actually installed.
    #[test]
    fn ui_family_resolution_prefers_candidates_in_order() {
        let installed: HashSet<String> = ["DejaVu Sans", "Noto Sans", "Inter"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(resolve_ui_family(&installed), "Inter");
    }

    /// The UI family falls back to the system's generic sans-serif answer
    /// — a family fontdb actually loaded, i.e. one that exists here.
    #[test]
    fn ui_family_resolution_falls_back_to_a_family_that_exists() {
        let resolved = resolve_ui_family(&HashSet::new());
        assert_eq!(resolved, system_sans_family());
        assert!(!resolved.is_empty());
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let exists = database
            .faces()
            .any(|face| face.families.iter().any(|family| family.0 == resolved));
        assert!(
            exists,
            "resolved family {resolved:?} is not an installed face"
        );
    }

    /// The family list DirectWrite reports on a stock Windows 11 install,
    /// captured on 2026-09-12 from the maintainer's box: the system faces
    /// and nothing else — no JetBrains, Nerd or Linux family.
    fn stock_windows_families() -> HashSet<String> {
        [
            "Arial",
            "Cascadia Code",
            "Cascadia Mono",
            "Consolas",
            "Courier New",
            "Lucida Console",
            "Segoe UI",
            "Segoe UI Symbol",
            "Times New Roman",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    /// The app registers the bundled face before the theme resolves, so on
    /// a stock Windows box the installed list is the system faces plus
    /// [`BUNDLED_TERMINAL_FAMILY`] — and that one must win over every
    /// system face, Cascadia Mono included.
    #[test]
    fn terminal_family_on_a_stock_windows_box_picks_the_bundled_nerd_font() {
        let mut installed = stock_windows_families();
        installed.insert(BUNDLED_TERMINAL_FAMILY.to_string());
        assert_eq!(resolve_terminal_family(&installed), BUNDLED_TERMINAL_FAMILY);

        let mut legacy_name_only = stock_windows_families();
        legacy_name_only.insert(BUNDLED_TERMINAL_FAMILY_LEGACY_NAME.to_string());
        assert_eq!(
            resolve_terminal_family(&legacy_name_only),
            BUNDLED_TERMINAL_FAMILY_LEGACY_NAME
        );
    }

    /// Were the bundled registration ever skipped, a stock Windows box must
    /// still land on a Windows-native monospace face, never on the generic
    /// answer: fontdb's built-in generic on Windows is "Courier New", the
    /// slab serif the 2026-09-12 screenshot showed Claude Code rendered in.
    #[test]
    fn terminal_family_on_a_stock_windows_box_never_lands_on_courier_new() {
        let resolved = resolve_terminal_family(&stock_windows_families());
        assert_ne!(resolved, "Courier New");
        assert_eq!(resolved, "Cascadia Mono");
    }

    /// The Nerd Font glyphs the agent TUIs draw their chrome with and that
    /// no system fallback can supply, since they live in the Private Use
    /// Area: the Powerline arrows and the rounded caps Claude Code's
    /// status-line pills are made of.
    const AGENT_TUI_PRIVATE_USE_GLYPHS: &[char] = &['\u{E0B0}', '\u{E0B2}', '\u{E0B4}', '\u{E0B6}'];

    /// The bundled bytes and [`BUNDLED_TERMINAL_FAMILY`] must agree — the
    /// candidate list leads with a family the app itself registers, so a
    /// wrong file would silently fall through to the next candidate — and
    /// must be the four faces the TUIs need, each carrying the Private Use
    /// glyphs at a letter's own advance: the terminal shapes a whole row at
    /// once, so a wider glyph would push every cell after it off the grid.
    /// Checked with ttf-parser, the parser under fontdb.
    #[test]
    fn bundled_terminal_faces_name_the_family_and_cover_the_agent_glyphs() {
        let faces = bundled_terminal_fonts();
        assert_eq!(faces.len(), 4, "regular, bold, italic and bold italic");
        let mut styles = HashSet::new();
        for bytes in &faces {
            let face = ttf_parser::Face::parse(bytes, 0).expect("bundled face parses");
            let family_names: HashSet<String> = face
                .names()
                .into_iter()
                .filter(|name| {
                    name.name_id == ttf_parser::name_id::FAMILY
                        || name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                })
                .filter_map(|name| name.to_string())
                .collect();
            assert!(
                family_names.contains(BUNDLED_TERMINAL_FAMILY),
                "bundled face is named {family_names:?}, not {BUNDLED_TERMINAL_FAMILY:?}"
            );
            assert!(
                family_names.contains(BUNDLED_TERMINAL_FAMILY_LEGACY_NAME),
                "bundled face is named {family_names:?}, not {BUNDLED_TERMINAL_FAMILY_LEGACY_NAME:?}"
            );
            styles.insert((face.is_bold(), face.is_italic()));

            let letter = face.glyph_index('m').expect("the face has an 'm'");
            let letter_advance = face
                .glyph_hor_advance(letter)
                .expect("the 'm' has an advance");
            for &glyph in AGENT_TUI_PRIVATE_USE_GLYPHS {
                let id = face
                    .glyph_index(glyph)
                    .unwrap_or_else(|| panic!("bundled face lacks U+{:04X}", glyph as u32));
                assert_eq!(
                    face.glyph_hor_advance(id),
                    Some(letter_advance),
                    "U+{:04X} is not one cell wide",
                    glyph as u32
                );
            }
        }
        assert_eq!(
            styles.len(),
            4,
            "expected regular, bold, italic and bold italic, got {styles:?}"
        );
    }

    /// Linux rasterises a variable font at its default instance only, so the
    /// static 500/600/700 faces are what let it paint medium, semibold and
    /// bold. A wrong or missing file shows up nowhere but on screen.
    #[test]
    fn bundled_ui_faces_are_geist_in_their_weights() {
        let faces: Vec<(String, u16)> = bundled_ui_fonts()
            .iter()
            .map(|bytes| {
                let face = ttf_parser::Face::parse(bytes, 0).expect("a font file");
                let names: Vec<(u16, String)> = face
                    .names()
                    .into_iter()
                    .filter(|name| {
                        (name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                            || name.name_id == ttf_parser::name_id::FAMILY)
                            && name.is_unicode()
                    })
                    .filter_map(|name| name.to_string().map(|value| (name.name_id, value)))
                    .collect();
                let family = names
                    .iter()
                    .find(|(name_id, _)| *name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
                    .or_else(|| {
                        names
                            .iter()
                            .find(|(name_id, _)| *name_id == ttf_parser::name_id::FAMILY)
                    })
                    .map(|(_, family)| family.clone())
                    .expect("a family name");
                (family, face.weight().to_number())
            })
            .collect();
        for weight in [500, 600, 700] {
            assert!(
                faces.iter().any(|(family, w)| family == "Geist" && *w == weight),
                "no static Geist {weight} in {faces:?}"
            );
        }
        assert!(faces.iter().any(|(family, _)| family == "Geist Mono"), "{faces:?}");
        assert_eq!(faces.len(), 5, "{faces:?}");
    }

    /// With nothing installed, the fallback is the system's generic
    /// monospace answer — and that answer must be a face fontdb actually
    /// loaded, i.e. a family that exists here (on this machine fontconfig
    /// maps "monospace" to DejaVu Sans Mono).
    #[test]
    fn code_family_resolution_falls_back_to_a_family_that_exists() {
        let resolved = resolve_code_family(&HashSet::new());
        assert_eq!(resolved, system_monospace_family());
        assert!(!resolved.is_empty());
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let exists = database
            .faces()
            .any(|face| face.families.iter().any(|family| family.0 == resolved));
        assert!(
            exists,
            "resolved family {resolved:?} is not an installed face"
        );
    }
}

#[cfg(test)]
mod agent_brand_tests {
    use super::*;

    /// `AgentIcon.normalizedId`: the ACP-bridged variant of an agent is the
    /// same brand, not an unknown one wearing the grey fallback.
    #[test]
    fn acp_suffix_is_stripped_before_matching() {
        assert_eq!(
            AgentBrandColor::for_agent_id("claude-acp"),
            AgentBrandColor::Claude
        );
        assert_eq!(
            AgentBrandColor::for_agent_id("opencode-acp"),
            AgentBrandColor::OpenCode
        );
    }

    /// The regression this type exists for. `AgentAccentColor::Amber`
    /// resolved to `theme.warning`, so a **running** Claude worktree
    /// painted the byte-identical `#E0B36A` as one that **needed input** —
    /// two states separated by nothing but a 3×3 versus a 6×6 dot cluster.
    /// No brand colour may equal any status token, in either appearance.
    #[test]
    fn no_brand_colour_collides_with_a_status_token() {
        let brands = [
            AgentBrandColor::Claude,
            AgentBrandColor::Codex,
            AgentBrandColor::OpenCode,
            AgentBrandColor::Pi,
            AgentBrandColor::Omp,
            AgentBrandColor::Unknown,
        ];
        for theme in [Theme::dark(), Theme::light()] {
            let statuses = [
                ("warning", theme.ely.warning),
                ("success", theme.ely.success),
                ("danger", theme.ely.danger),
            ];
            for brand in brands {
                let brand_color = Rgba::from(brand.color());
                for (name, status) in statuses {
                    let status = Rgba::from(status);
                    let separation = (brand_color.r - status.r)
                        .abs()
                        .max((brand_color.g - status.g).abs())
                        .max((brand_color.b - status.b).abs());
                    assert!(
                        separation > 0.5 / 255.0,
                        "{brand:?} is too close to {name}: a running worktree \
                         would be indistinguishable from that status"
                    );
                }
            }
        }
    }

    /// Brands are appearance-invariant by design: `hex()` takes no
    /// appearance, so light and dark cannot drift apart the way an
    /// `adaptive` token pair can.
    #[test]
    fn brands_are_distinct_from_each_other() {
        let brands = [
            AgentBrandColor::Claude,
            AgentBrandColor::Codex,
            AgentBrandColor::OpenCode,
            AgentBrandColor::Pi,
            AgentBrandColor::Omp,
            AgentBrandColor::Unknown,
        ];
        for (index, left) in brands.iter().enumerate() {
            for right in &brands[index + 1..] {
                assert_ne!(
                    left.hex(),
                    right.hex(),
                    "{left:?} and {right:?} would be indistinguishable"
                );
            }
        }
    }
}

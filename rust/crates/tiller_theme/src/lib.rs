//! Tiller's shared color, spacing, and typography tokens.
//!
//! The values in this crate are measured from waku's visual system — see
//! `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` §A.2. The design
//! contract, in waku's own words: neutral graphite surfaces in the spirit of
//! Cursor — color is reserved for meaning. Selected, hovered, and pressed
//! rows are a ~6% neutral layer; the coral accent is used only for brand
//! moments (logo, caret, focus, live activity), never as structure.
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
//! made Tiller start in light on every portal-less Linux session. Tiller
//! queries the portal itself (`ashpd`, the same crate gpui_linux uses) and
//! treats a missing or silent portal as **dark**, not light. macOS keeps the
//! old behavior: `NSAppearance` is synchronous and authoritative there.

use gpui::{App, FontWeight, Global, Pixels, Rgba, Size, WindowAppearance, px, size};
use std::collections::HashSet;

/// Pop!_OS COSMIC design tokens, consumed by [`Theme::cosmic`] — every
/// surface that reads `Theme::get(cx)` gets a resolved [`cosmic::CosmicTheme`]
/// with it. See `docs/linux-rewrite/COSMIC-DESIGN.md`.
pub mod cosmic;
use std::ops::Deref;
use std::sync::OnceLock;

/// The appearance selected by Tiller's appearance setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the current system/window appearance.
    #[default]
    System,
    /// Always use the light palette.
    Light,
    /// Always use the dark palette.
    Dark,
}

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

impl ThemeMode {
    fn resolve(self, system_appearance: WindowAppearance) -> Appearance {
        match self {
            Self::System => system_appearance.into(),
            Self::Light => Appearance::Light,
            Self::Dark => Appearance::Dark,
        }
    }

    /// Linux-specific resolution for `System`.
    ///
    /// GPUI's `WindowAppearance` starts at `Light` and only becomes
    /// meaningful once the XDG portal answers — or never, when no portal is
    /// running. A light appearance at startup is therefore *not evidence of
    /// a light desktop*: it is the unresolved default. Resolve it to dark,
    /// and let the portal query in [`Theme::init`] correct to light when the
    /// portal actually says so.
    #[cfg(target_os = "linux")]
    fn resolve_system(self, system_appearance: WindowAppearance) -> Appearance {
        match self {
            Self::System => match system_appearance {
                // The portal (or another platform channel) has spoken.
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
                // Light here means "not heard from yet" or "no portal".
                // Dark wins either way.
                _ => Appearance::Dark,
            },
            Self::Light => Appearance::Light,
            Self::Dark => Appearance::Dark,
        }
    }
}

/// All adaptive colors used by Tiller.
///
/// Token names keep the roles the UI has always consumed; the values are
/// waku's measured palette. The waku role names themselves (accent, gauge,
/// selection, raised, composer, inset, overlay, border_strong, code_wash,
/// inverse, …) exist alongside so components can move to them without
/// re-measuring anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Panel surface for the tab bar, workspace column, right panel and
    /// settings — waku `surface` (`#1A1A1A` dark / `#F6F5F6` light). The
    /// chrome merges with the reading surface; only the sidebar steps a
    /// hair darker (see [`ThemeColors::sidebar`]).
    pub background: Rgba,
    /// Window canvas behind the working column (`#1A1A1A` dark,
    /// `#F6F5F6` light — waku `canvas`/`surface`).
    pub canvas: Rgba,
    /// Terminal surface — paper-white in light, near-black in dark (waku
    /// `terminal`: `#151515` / `#FFFFFF`).
    pub terminal_surface: Rgba,
    /// Brand accent — waku `accent`, a coral used only for meaning (focus
    /// rings, caret, live activity, selected states): `#E2795B` / `#C85F44`.
    pub tab_focus_accent: Rgba,
    /// Waiting-for-input status — waku `warning` `#E0B36A` / `#A66B20`.
    pub tab_needs_input: Rgba,
    /// Completed status — waku `success` `#62C987` / `#2F8F52`.
    pub tab_done: Rgba,
    /// Errored status — waku `danger` `#E2726A` / `#C64A42`.
    pub tab_error: Rgba,
    /// Chat transcript surface (waku `surface`).
    pub chat_surface: Rgba,
    /// Tint used by the sidebar material (same as `background`).
    pub chrome_tint: Rgba,
    /// Tab-chip underline — waku `border_strong`.
    pub tab_chip_underline: Rgba,
    /// Shared one-pixel border/divider stroke — waku `border`
    /// (`hsla(220,10%,90%,0.07)` dark / `hsla(220,10%,12%,0.08)` light).
    pub hairline: Rgba,
    /// Hover fill for sidebar rows — waku `sidebar_item_background`, a 6%
    /// neutral layer, not a color.
    pub row_hover: Rgba,
    /// Hover fill for transcript rows — waku `overlay` (5% neutral).
    pub chat_row_hover: Rgba,
    /// Selected row fill — waku's selected rows are the same 6% neutral
    /// wash as hover (`sidebar_item_background`), never a color. Text
    /// selection is a different concept: see [`ThemeColors::selection`].
    pub selection_fill: Rgba,
    /// Focused-field border — waku's focus ring is the accent, not a second
    /// blue.
    pub selection_ring: Rgba,
    /// Row title text — waku `text` `#E2E2E2` / `#242424`.
    pub title: Rgba,
    /// Selected row title text — a step brighter than `title`.
    pub title_selected: Rgba,
    /// Secondary row text — waku `text_secondary` `#A3A3A3` / `#666666`.
    pub subtitle: Rgba,
    /// Caption/meta text — waku `text_tertiary` `#7D7D7D` / `#858585`.
    pub meta: Rgba,
    /// Primary pill fill — waku `raised` `#232323` / `#ECECEC`.
    pub primary_pill_bg: Rgba,
    /// Filter field fill — waku `inset` `#151515` / `#E6E6E6`.
    pub filter_field_bg: Rgba,
    /// Tree guide stroke, including its source alpha.
    pub tree_guide: Rgba,
    /// Staged-file status color (waku `success`).
    pub git_staged: Rgba,
    /// Modified-file status color (waku `warning`).
    pub git_modified: Rgba,
    /// Untracked-file status color (waku `gauge` blue).
    pub git_untracked: Rgba,
    /// Conflict-file status color (waku `danger`).
    pub git_conflict: Rgba,
    /// Addition diff accent (waku `success`).
    pub diff_addition: Rgba,
    /// Addition diff background — translucent success wash.
    pub diff_addition_background: Rgba,
    /// Deletion diff accent (waku `danger`).
    pub diff_deletion: Rgba,
    /// Deletion diff background — translucent danger wash.
    pub diff_deletion_background: Rgba,
    /// Hunk diff background — waku `code_wash`.
    pub diff_hunk_background: Rgba,
    /// Chat card fill — waku `raised`.
    pub card_fill: Rgba,
    /// Recessed code/diff fill — waku `inset`.
    pub code_inset_fill: Rgba,
    /// Composer primary text (waku `text`).
    pub primary_text_color: Rgba,
    /// Clickable file-link color (waku `gauge` blue).
    pub file_link: Rgba,
    /// Task-card accent rail.
    pub rail_task: Rgba,
    /// Question-card accent rail (waku `warning`).
    pub rail_question: Rgba,
    /// Edit-card accent rail (waku `success`).
    pub rail_edit: Rgba,
    /// Tool-card accent rail.
    pub rail_tool: Rgba,

    // ── waku role tokens (measured values, verbatim) ──────────────────────
    /// The opaque sidebar fill chosen for Linux. waku's sidebar is
    /// transparent over macOS vibrancy; Linux has no vibrancy, so this
    /// promotes waku's own drag-state fill (`sidebar_drag_background`
    /// `#181818` dark / `#F3F3F3` light) to the resting fill — a hair
    /// darker than the surface it separates from, plus the 1px
    /// `sidebar_border` hairline.
    pub sidebar: Rgba,
    /// Floating cards, popovers, tooltips — waku `raised`.
    pub raised: Rgba,
    /// Composer card fill — waku `composer` `#212121` / `#FFFFFF`.
    pub composer: Rgba,
    /// Recessed wells — waku `inset`.
    pub inset: Rgba,
    /// Generic hover wash (5% neutral) — waku `overlay`.
    pub overlay: Rgba,
    /// Pressed wash (9% neutral) — waku `overlay_strong`.
    pub overlay_strong: Rgba,
    /// Stronger divider — waku `border_strong`.
    pub border_strong: Rgba,
    /// The 1px seam between sidebar and content — waku `sidebar_border`.
    pub sidebar_border: Rgba,
    /// Faintest text step — waku `text_ghost` `#575757` / `#A4A4A4`.
    pub text_ghost: Rgba,
    /// Brand coral — waku `accent` (same value as `tab_focus_accent`).
    pub accent: Rgba,
    /// Quota-meter blue — waku `gauge` `#3B82F6` / `#2563EB`.
    pub gauge: Rgba,
    /// Selected *row* fill: the 6% neutral wash waku applies to selected,
    /// hovered and pressed rows alike. Components that paint a selected row
    /// or tab chip should use this; [`ThemeColors::selection`] is reserved
    /// for painted text-selection under glyphs.
    pub selected_fill: Rgba,
    /// Text-selection wash — the familiar browser blue, painted under
    /// glyphs: `hsla(211,100%,50%,0.55)` / `0.35`. Never used for row
    /// chrome.
    pub selection: Rgba,
    /// Inline `code` foreground — waku `code_text` `#E0A882` / `#9A5528`.
    pub code_text: Rgba,
    /// Inline `code` rounded wash — waku `code_wash`.
    pub code_wash: Rgba,
    /// Light fill for primary buttons, dark glyph on top — waku `inverse`.
    pub inverse: Rgba,
    /// Glyph on primary buttons — waku `on_inverse`.
    pub on_inverse: Rgba,
    /// Star/favorite amber — waku `favorite` `#EAB308` / `#CA8A04`.
    pub favorite: Rgba,
    /// Soft danger fill (stop button hover) — waku `danger_soft`.
    pub danger_soft: Rgba,
}

impl ThemeColors {
    /// Picks the dark or light variant. Call sites pass dark first, then
    /// light, matching the "dark / light" order every palette table in the
    /// reference document uses.
    fn adaptive(dark: Rgba, light: Rgba, appearance: Appearance) -> Rgba {
        match appearance {
            Appearance::Light => light,
            Appearance::Dark => dark,
        }
    }

    fn for_appearance(appearance: Appearance) -> Self {
        let accent = Self::adaptive(rgb_hex(0xE2795B), rgb_hex(0xC85F44), appearance);
        let warning = Self::adaptive(rgb_hex(0xE0B36A), rgb_hex(0xA66B20), appearance);
        let success = Self::adaptive(rgb_hex(0x62C987), rgb_hex(0x2F8F52), appearance);
        let danger = Self::adaptive(rgb_hex(0xE2726A), rgb_hex(0xC64A42), appearance);
        let gauge = Self::adaptive(rgb_hex(0x3B82F6), rgb_hex(0x2563EB), appearance);
        let text = Self::adaptive(rgb_hex(0xE2E2E2), rgb_hex(0x242424), appearance);
        let text_secondary = Self::adaptive(rgb_hex(0xA3A3A3), rgb_hex(0x666666), appearance);
        let text_tertiary = Self::adaptive(rgb_hex(0x7D7D7D), rgb_hex(0x858585), appearance);
        let text_ghost = Self::adaptive(rgb_hex(0x575757), rgb_hex(0xA4A4A4), appearance);
        let surface = Self::adaptive(rgb_hex(0x1A1A1A), rgb_hex(0xF6F5F6), appearance);
        let raised = Self::adaptive(rgb_hex(0x232323), rgb_hex(0xECECEC), appearance);
        let inset = Self::adaptive(rgb_hex(0x151515), rgb_hex(0xE6E6E6), appearance);
        let composer = Self::adaptive(rgb_hex(0x212121), color(1.0, 1.0, 1.0, 1.0), appearance);
        let terminal_surface =
            Self::adaptive(rgb_hex(0x151515), color(1.0, 1.0, 1.0, 1.0), appearance);
        let sidebar = Self::adaptive(rgb_hex(0x181818), rgb_hex(0xF3F3F3), appearance);
        let border = Self::adaptive(
            hsla(220.0, 0.10, 0.90, 0.07),
            hsla(220.0, 0.10, 0.12, 0.08),
            appearance,
        );
        let border_strong = Self::adaptive(
            hsla(220.0, 0.10, 0.90, 0.14),
            hsla(220.0, 0.10, 0.12, 0.15),
            appearance,
        );
        let sidebar_border = Self::adaptive(
            hsla(126.93, 0.000_000_1, 0.16077, 1.0),
            hsla(0.0, 0.0, 0.078, 0.12),
            appearance,
        );
        let row_hover = Self::adaptive(
            hsla(0.0, 0.0, 0.941, 0.06),
            hsla(0.0, 0.0, 0.078, 0.06),
            appearance,
        );
        let overlay = Self::adaptive(
            hsla(220.0, 0.10, 0.90, 0.05),
            hsla(220.0, 0.10, 0.12, 0.05),
            appearance,
        );
        let overlay_strong = Self::adaptive(
            hsla(220.0, 0.10, 0.90, 0.09),
            hsla(220.0, 0.10, 0.12, 0.09),
            appearance,
        );
        let selection = Self::adaptive(
            hsla(211.0, 1.0, 0.50, 0.55),
            hsla(211.0, 1.0, 0.50, 0.35),
            appearance,
        );
        let code_text = Self::adaptive(rgb_hex(0xE0A882), rgb_hex(0x9A5528), appearance);
        let code_wash = Self::adaptive(
            hsla(220.0, 0.10, 0.90, 0.08),
            hsla(220.0, 0.10, 0.12, 0.07),
            appearance,
        );
        let inverse = Self::adaptive(rgb_hex(0xE7E9EC), rgb_hex(0x202227), appearance);
        let on_inverse = Self::adaptive(rgb_hex(0x17181C), rgb_hex(0xF8F8F9), appearance);
        let favorite = Self::adaptive(rgb_hex(0xEAB308), rgb_hex(0xCA8A04), appearance);
        let danger_soft = Self::adaptive(
            hsla(4.0, 0.55, 0.63, 0.10),
            hsla(4.0, 0.55, 0.52, 0.10),
            appearance,
        );

        Self {
            background: surface,
            canvas: surface,
            terminal_surface,
            tab_focus_accent: accent,
            tab_needs_input: warning,
            tab_done: success,
            tab_error: danger,
            chat_surface: surface,
            chrome_tint: sidebar,
            tab_chip_underline: border_strong,
            hairline: border,
            row_hover,
            chat_row_hover: overlay,
            selection_fill: selection,
            selection_ring: accent,
            title: text,
            title_selected: Self::adaptive(rgb_hex(0xF5F5F5), rgb_hex(0x101010), appearance),
            subtitle: text_secondary,
            meta: text_tertiary,
            primary_pill_bg: raised,
            filter_field_bg: inset,
            tree_guide: Self::adaptive(
                color(1.0, 1.0, 1.0, 0.14),
                color(0.0, 0.0, 0.0, 0.12),
                appearance,
            ),
            git_staged: success,
            git_modified: warning,
            git_untracked: gauge,
            git_conflict: danger,
            diff_addition: success,
            diff_addition_background: Self::adaptive(
                hsla(144.0, 0.47, 0.58, 0.15),
                hsla(144.0, 0.47, 0.38, 0.15),
                appearance,
            ),
            diff_deletion: danger,
            diff_deletion_background: Self::adaptive(
                hsla(4.0, 0.55, 0.63, 0.14),
                hsla(4.0, 0.55, 0.52, 0.14),
                appearance,
            ),
            diff_hunk_background: code_wash,
            card_fill: raised,
            code_inset_fill: inset,
            primary_text_color: text,
            file_link: gauge,
            rail_task: Self::adaptive(
                color(0.49, 0.42, 0.84, 1.0),
                color(0.36, 0.30, 0.68, 1.0),
                appearance,
            ),
            rail_question: warning,
            rail_edit: success,
            rail_tool: Self::adaptive(
                color(0.40, 0.42, 0.50, 1.0),
                color(0.55, 0.57, 0.65, 1.0),
                appearance,
            ),
            sidebar,
            raised,
            composer,
            inset,
            overlay,
            overlay_strong,
            border_strong,
            sidebar_border,
            text_ghost,
            accent,
            gauge,
            selection,
            selected_fill: row_hover,
            code_text,
            code_wash,
            inverse,
            on_inverse,
            favorite,
            danger_soft,
        }
    }
}

/// Spacing and geometry tokens, re-valued to waku's measured scale
/// (4/6/7/8/12/13 radius steps; 48px bars; 2/4/6/8/10/12/14/20 spacing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
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
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            card_corner_radius: px(6.0),
            card_gap: px(10.0),
            card_shadow_radius: px(18.0),
            card_shadow_y_offset: px(6.0),
            title_strip_height: px(48.0),
            traffic_light_inset: px(14.0),
            title_strip_icon_size: px(14.0),
            titlebar_control_frame: size(px(26.0), px(26.0)),
            titlebar_control_spacing: px(6.0),
            bottom_bar_height: px(40.0),
            menu_width: px(240.0),
            hairline_thickness: px(1.0),
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
    /// User message pills, menus, Tiller's settings cards (12px).
    pub user_pill: Pixels,
    /// The composer card (13px).
    pub composer: Pixels,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            chip: px(4.0),
            chip_active: px(5.0),
            control: px(6.0),
            row_card: px(7.0),
            code_block: px(8.0),
            toast: px(10.0),
            user_pill: px(12.0),
            composer: px(13.0),
        }
    }
}

/// Interface and code type-scale tokens, re-valued to waku's measured scale:
/// 11.5px UI chrome, 13.5px body, 20px display, 21px body line height
/// (~1.56), a monospace family resolved from what the system actually has at
/// 11.5px for code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// Body base size, in points (waku markdown body: 13.5px).
    pub base_size: Pixels,
    /// Code size, in points (waku code body: 11.5px).
    pub code_size: Pixels,
    /// Code line height (waku: 17.5px).
    pub code_line_height: Pixels,
    /// Code font weight (waku's embedded JetBrains Mono renders NORMAL).
    pub code_weight: FontWeight,
    /// The monospace family code renders in — the one answer to "what is our
    /// mono font", resolved once at runtime from the families actually
    /// installed (see [`Theme::resolve_code_family`]) instead of five string
    /// literals. `SFMono-Regular` does not exist on Linux; the fallback
    /// chain picks what does.
    pub code_family: &'static str,
    /// Display size (waku empty-state headline: 20px MEDIUM).
    pub large_title: Pixels,
    /// Markdown h2 size (13.5 × 1.28 → 17px).
    pub title: Pixels,
    /// Markdown h3 size (13.5 × 1.14 → 15px).
    pub title2: Pixels,
    /// Markdown h4 size (13.5 × 1.05 → 14px).
    pub title3: Pixels,
    /// Body/headline size (13.5px).
    pub headline: Pixels,
    /// Callout/subheadline size (12.5px).
    pub callout: Pixels,
    /// Footnote/UI chrome size (11.5px).
    pub footnote: Pixels,
    /// Caption-2 size (10.5px).
    pub caption2: Pixels,
    /// Default UI chrome size (waku: 11.5px for chips, buttons, rows).
    pub ui_size: Pixels,
    /// Body line height (waku markdown body: 21px).
    pub body_line_height: Pixels,
    /// UI chrome line height (waku rows: 14-16px; 16 is the reading default).
    pub ui_line_height: Pixels,
}

impl Typography {
    /// Returns the default 13.5-point scale.
    pub fn default_scale() -> Self {
        Self::for_base_size(13.5)
    }

    /// Returns the type scale for a chosen body base size, preserving waku's
    /// step relationships (headings scale off the body, chrome stays fixed
    /// at 11.5).
    pub fn for_base_size(base_size: f32) -> Self {
        let delta = base_size - 13.5;
        let scaled = |points: f32| px((points + delta).max(6.0));

        Self {
            base_size: px(base_size),
            code_size: scaled(11.5),
            code_line_height: px(17.5),
            code_weight: FontWeight::NORMAL,
            code_family: code_family(),
            large_title: scaled(20.0),
            title: scaled(17.0),
            title2: scaled(15.0),
            title3: scaled(14.0),
            headline: scaled(13.5),
            callout: scaled(12.5),
            footnote: scaled(11.5),
            caption2: scaled(10.5),
            ui_size: px(11.5),
            body_line_height: px(21.0),
            ui_line_height: px(16.0),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::default_scale()
    }
}

// ── the code font family ─────────────────────────────────────────────────
//
// Five call sites used to ask for `SFMono-Regular` by name, a family that
// does not exist on Linux. GPUI falls back silently, so every code span,
// diff line and file view rendered in a family chosen by the font stack
// rather than one we picked — and it looked fine, which is why it survived.
// This is the same class of defect as the "SF Symbols" file-icon entry in
// Settings: a macOS assumption that is invisible until you look for it.
//
// One resolution, at runtime, from what is actually installed; one answer
// to "what is our mono font", sitting next to the code-font weight in
// [`Typography`].

/// Monospace families to prefer, in order, when resolving the code font.
///
/// First the family the visual bar is set in (waku's JetBrains Mono), then
/// common good monospaced faces, then whatever the system's generic
/// "monospace" resolves to (fontconfig's alias on Linux, always present).
/// A candidate that is not installed is skipped — never guessed at. The
/// list was verified against `fc-list : family` on the Linux build machine
/// rather than assumed.
pub const CODE_FAMILY_CANDIDATES: &[&str] = &[
    "JetBrains Mono",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// The resolved code family, remembered once. The first caller wins: in the
/// app that is [`Theme::resolve_code_family`] with the real installed list;
/// in tests it is whichever theme accessor runs first, which gets the
/// system's generic monospace answer — a real family either way.
static CODE_FAMILY: OnceLock<String> = OnceLock::new();

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

/// The resolved code font family, resolving the system's generic monospace
/// answer if nothing has been resolved yet.
pub fn code_family() -> &'static str {
    CODE_FAMILY.get_or_init(system_monospace_family).as_str()
}

/// The resolved Tiller theme stored as a GPUI global.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The requested appearance mode.
    pub mode: ThemeMode,
    /// The active resolved appearance.
    pub appearance: Appearance,
    /// Adaptive color tokens.
    pub colors: ThemeColors,
    /// Spacing and geometry tokens.
    pub spacing: Spacing,
    /// Corner-radius tokens (waku's measured scale).
    pub radii: Radii,
    /// Pop!_OS COSMIC design tokens — container hierarchy, semantic
    /// colours, and the COSMIC spacing/radii scales, resolved for the same
    /// `(mode, appearance)` as the rest of this theme via
    /// [`cosmic::CosmicTheme::resolve`]. Every surface that already reads
    /// `Theme::get(cx)` gets these for free.
    pub cosmic: cosmic::CosmicTheme,
    /// Typography tokens.
    pub typography: Typography,
    /// Shared opacity for translucent surfaces.
    pub translucent_surface_opacity: f32,
    /// Compatibility accessor for the original scaffold and the app shell.
    pub canvas: Rgba,
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

    /// Installs a theme, resolving `System` against the current window
    /// appearance. On Linux the resolution is dark-biased until the portal
    /// is heard from (see [`ThemeMode::resolve_system`]).
    pub fn install(mode: ThemeMode, cx: &mut App) {
        Self::resolve_code_family(cx);
        #[cfg(target_os = "linux")]
        let theme = Self::for_mode_linux(mode, cx.window_appearance());
        #[cfg(not(target_os = "linux"))]
        let theme = Self::for_mode(mode, cx.window_appearance());
        cx.set_global(theme);
    }

    /// Resolves and remembers the code family from the families the runtime
    /// text system actually has installed. Idempotent — the first call
    /// wins — and called from [`Theme::install`] so the app has the answer
    /// before the first frame. The pure, testable form is the free
    /// [`resolve_code_family`].
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
                    cx.set_global(Theme::for_appearance(ThemeMode::System, preference));
                }
            });
        })
        .detach();
    }

    /// Returns a theme resolved for a requested mode and system appearance.
    pub fn for_mode(mode: ThemeMode, system_appearance: WindowAppearance) -> Self {
        let appearance = mode.resolve(system_appearance);
        Self::for_appearance(mode, appearance)
    }

    /// Linux resolution of `System`: dark until the portal speaks.
    #[cfg(target_os = "linux")]
    fn for_mode_linux(mode: ThemeMode, system_appearance: WindowAppearance) -> Self {
        let appearance = mode.resolve_system(system_appearance);
        Self::for_appearance(mode, appearance)
    }

    /// Returns a light theme without requiring a GPUI application context.
    pub fn light() -> Self {
        Self::for_appearance(ThemeMode::Light, Appearance::Light)
    }

    /// Returns a dark theme without requiring a GPUI application context.
    pub fn dark() -> Self {
        Self::for_appearance(ThemeMode::Dark, Appearance::Dark)
    }

    /// Returns a system-mode theme resolved against the supplied appearance.
    ///
    /// On Linux this is the dark-biased resolution: a light appearance is
    /// the unresolved default until the portal is heard from.
    pub fn system(system_appearance: WindowAppearance) -> Self {
        #[cfg(target_os = "linux")]
        return Self::for_mode_linux(ThemeMode::System, system_appearance);
        #[cfg(not(target_os = "linux"))]
        Self::for_mode(ThemeMode::System, system_appearance)
    }

    /// Returns the surface opacity used when translucency is enabled.
    pub fn surface_opacity(translucency_enabled: bool) -> f32 {
        if translucency_enabled { 0.96 } else { 1.0 }
    }

    fn for_appearance(mode: ThemeMode, appearance: Appearance) -> Self {
        let colors = ThemeColors::for_appearance(appearance);
        Self {
            mode,
            appearance,
            canvas: colors.canvas,
            colors,
            spacing: Spacing::default(),
            radii: Radii::default(),
            cosmic: cosmic::CosmicTheme::resolve(mode, appearance),
            typography: Typography::default(),
            translucent_surface_opacity: 0.96,
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
    std::thread::current()
        .name()
        .is_some_and(|name| name.contains("::tests::"))
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

fn color(r: f32, g: f32, b: f32, a: f32) -> Rgba {
    Rgba { r, g, b, a }
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

/// Converts a CSS-style `hsla(h, s, l, a)` value (h in **degrees**, s/l/a
/// in 0..1) to sRGB. Used so waku's measured hsl tokens are written the way
/// the reference writes them.
fn hsla(h: f32, s: f32, l: f32, a: f32) -> Rgba {
    let h = (h.rem_euclid(360.0)) / 360.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 1.0 / 6.0 {
        (c, x, 0.0)
    } else if h < 2.0 / 6.0 {
        (x, c, 0.0)
    } else if h < 3.0 / 6.0 {
        (0.0, c, x)
    } else if h < 4.0 / 6.0 {
        (0.0, x, c)
    } else if h < 5.0 / 6.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    Rgba {
        r: r + m,
        g: g + m,
        b: b + m,
        a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_color(actual: Rgba, expected: (f32, f32, f32, f32)) {
        for (actual, expected, name) in [
            (actual.r, expected.0, "r"),
            (actual.g, expected.1, "g"),
            (actual.b, expected.2, "b"),
            (actual.a, expected.3, "a"),
        ] {
            assert!(
                (actual - expected).abs() < 0.004,
                "{name}: {actual} != {expected}"
            );
        }
    }

    /// waku's measured dark palette (docs/linux-rewrite/03 §A.2).
    #[test]
    fn dark_palette_matches_waku() {
        let theme = Theme::dark();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(
            theme.canvas,
            f(
                0x1A as f32 / 255.0,
                0x1A as f32 / 255.0,
                0x1A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.background,
            f(
                0x1A as f32 / 255.0,
                0x1A as f32 / 255.0,
                0x1A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.sidebar,
            f(
                0x18 as f32 / 255.0,
                0x18 as f32 / 255.0,
                0x18 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.terminal_surface,
            f(
                0x15 as f32 / 255.0,
                0x15 as f32 / 255.0,
                0x15 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.raised,
            f(
                0x23 as f32 / 255.0,
                0x23 as f32 / 255.0,
                0x23 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.composer,
            f(
                0x21 as f32 / 255.0,
                0x21 as f32 / 255.0,
                0x21 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.inset,
            f(
                0x15 as f32 / 255.0,
                0x15 as f32 / 255.0,
                0x15 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.accent,
            f(
                0xE2 as f32 / 255.0,
                0x79 as f32 / 255.0,
                0x5B as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_focus_accent,
            f(
                0xE2 as f32 / 255.0,
                0x79 as f32 / 255.0,
                0x5B as f32 / 255.0,
            ),
        );
        expect_color(
            theme.gauge,
            f(
                0x3B as f32 / 255.0,
                0x82 as f32 / 255.0,
                0xF6 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.title,
            f(
                0xE2 as f32 / 255.0,
                0xE2 as f32 / 255.0,
                0xE2 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.subtitle,
            f(
                0xA3 as f32 / 255.0,
                0xA3 as f32 / 255.0,
                0xA3 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.meta,
            f(
                0x7D as f32 / 255.0,
                0x7D as f32 / 255.0,
                0x7D as f32 / 255.0,
            ),
        );
        expect_color(
            theme.text_ghost,
            f(
                0x57 as f32 / 255.0,
                0x57 as f32 / 255.0,
                0x57 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_needs_input,
            f(
                0xE0 as f32 / 255.0,
                0xB3 as f32 / 255.0,
                0x6A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_done,
            f(
                0x62 as f32 / 255.0,
                0xC9 as f32 / 255.0,
                0x87 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_error,
            f(
                0xE2 as f32 / 255.0,
                0x72 as f32 / 255.0,
                0x6A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.favorite,
            f(
                0xEA as f32 / 255.0,
                0xB3 as f32 / 255.0,
                0x08 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.code_text,
            f(
                0xE0 as f32 / 255.0,
                0xA8 as f32 / 255.0,
                0x82 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.inverse,
            f(
                0xE7 as f32 / 255.0,
                0xE9 as f32 / 255.0,
                0xEC as f32 / 255.0,
            ),
        );
        expect_color(
            theme.on_inverse,
            f(
                0x17 as f32 / 255.0,
                0x18 as f32 / 255.0,
                0x1C as f32 / 255.0,
            ),
        );

        // The 6% neutral hover wash and the translucent border/selection.
        expect_color(theme.row_hover, (0.941, 0.941, 0.941, 0.06));
        // hsla(220,10%,90%) ≈ rgb(0.890, 0.897, 0.910).
        expect_color(theme.overlay, (0.890, 0.897, 0.910, 0.05));
        // hsla(211,100%,50%) is the browser blue #007BFF.
        expect_color(theme.selection, (0.0, 0.483, 1.0, 0.55));
    }

    /// waku's measured light palette.
    #[test]
    fn light_palette_matches_waku() {
        let theme = Theme::light();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(
            theme.canvas,
            f(
                0xF6 as f32 / 255.0,
                0xF5 as f32 / 255.0,
                0xF6 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.background,
            f(
                0xF6 as f32 / 255.0,
                0xF5 as f32 / 255.0,
                0xF6 as f32 / 255.0,
            ),
        );
        expect_color(theme.terminal_surface, f(1.0, 1.0, 1.0));
        expect_color(
            theme.raised,
            f(
                0xEC as f32 / 255.0,
                0xEC as f32 / 255.0,
                0xEC as f32 / 255.0,
            ),
        );
        expect_color(
            theme.accent,
            f(
                0xC8 as f32 / 255.0,
                0x5F as f32 / 255.0,
                0x44 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.gauge,
            f(
                0x25 as f32 / 255.0,
                0x63 as f32 / 255.0,
                0xEB as f32 / 255.0,
            ),
        );
        expect_color(
            theme.title,
            f(
                0x24 as f32 / 255.0,
                0x24 as f32 / 255.0,
                0x24 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.subtitle,
            f(
                0x66 as f32 / 255.0,
                0x66 as f32 / 255.0,
                0x66 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.meta,
            f(
                0x85 as f32 / 255.0,
                0x85 as f32 / 255.0,
                0x85 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_needs_input,
            f(
                0xA6 as f32 / 255.0,
                0x6B as f32 / 255.0,
                0x20 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_done,
            f(
                0x2F as f32 / 255.0,
                0x8F as f32 / 255.0,
                0x52 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.tab_error,
            f(
                0xC6 as f32 / 255.0,
                0x4A as f32 / 255.0,
                0x42 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.code_text,
            f(
                0x9A as f32 / 255.0,
                0x55 as f32 / 255.0,
                0x28 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.inverse,
            f(
                0x20 as f32 / 255.0,
                0x22 as f32 / 255.0,
                0x27 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.on_inverse,
            f(
                0xF8 as f32 / 255.0,
                0xF8 as f32 / 255.0,
                0xF9 as f32 / 255.0,
            ),
        );
    }

    #[test]
    fn every_adaptive_token_differs_between_light_and_dark() {
        let light = Theme::light().colors;
        let dark = Theme::dark().colors;
        let tokens = [
            ("background", light.background, dark.background),
            ("canvas", light.canvas, dark.canvas),
            ("sidebar", light.sidebar, dark.sidebar),
            (
                "terminal_surface",
                light.terminal_surface,
                dark.terminal_surface,
            ),
            (
                "tab_focus_accent",
                light.tab_focus_accent,
                dark.tab_focus_accent,
            ),
            (
                "tab_needs_input",
                light.tab_needs_input,
                dark.tab_needs_input,
            ),
            ("tab_done", light.tab_done, dark.tab_done),
            ("tab_error", light.tab_error, dark.tab_error),
            ("chat_surface", light.chat_surface, dark.chat_surface),
            ("chrome_tint", light.chrome_tint, dark.chrome_tint),
            (
                "tab_chip_underline",
                light.tab_chip_underline,
                dark.tab_chip_underline,
            ),
            ("hairline", light.hairline, dark.hairline),
            ("row_hover", light.row_hover, dark.row_hover),
            ("chat_row_hover", light.chat_row_hover, dark.chat_row_hover),
            ("selection_fill", light.selection_fill, dark.selection_fill),
            ("selected_fill", light.selected_fill, dark.selected_fill),
            ("selection", light.selection, dark.selection),
            ("selection_ring", light.selection_ring, dark.selection_ring),
            ("title", light.title, dark.title),
            ("title_selected", light.title_selected, dark.title_selected),
            ("subtitle", light.subtitle, dark.subtitle),
            ("meta", light.meta, dark.meta),
            (
                "primary_pill_bg",
                light.primary_pill_bg,
                dark.primary_pill_bg,
            ),
            (
                "filter_field_bg",
                light.filter_field_bg,
                dark.filter_field_bg,
            ),
            ("tree_guide", light.tree_guide, dark.tree_guide),
            ("git_staged", light.git_staged, dark.git_staged),
            ("git_modified", light.git_modified, dark.git_modified),
            ("git_untracked", light.git_untracked, dark.git_untracked),
            ("git_conflict", light.git_conflict, dark.git_conflict),
            ("diff_addition", light.diff_addition, dark.diff_addition),
            (
                "diff_addition_background",
                light.diff_addition_background,
                dark.diff_addition_background,
            ),
            ("diff_deletion", light.diff_deletion, dark.diff_deletion),
            (
                "diff_deletion_background",
                light.diff_deletion_background,
                dark.diff_deletion_background,
            ),
            (
                "diff_hunk_background",
                light.diff_hunk_background,
                dark.diff_hunk_background,
            ),
            ("card_fill", light.card_fill, dark.card_fill),
            (
                "code_inset_fill",
                light.code_inset_fill,
                dark.code_inset_fill,
            ),
            (
                "primary_text_color",
                light.primary_text_color,
                dark.primary_text_color,
            ),
            ("file_link", light.file_link, dark.file_link),
            ("rail_task", light.rail_task, dark.rail_task),
            ("rail_question", light.rail_question, dark.rail_question),
            ("rail_edit", light.rail_edit, dark.rail_edit),
            ("rail_tool", light.rail_tool, dark.rail_tool),
            ("raised", light.raised, dark.raised),
            ("composer", light.composer, dark.composer),
            ("inset", light.inset, dark.inset),
            ("overlay", light.overlay, dark.overlay),
            ("overlay_strong", light.overlay_strong, dark.overlay_strong),
            ("border_strong", light.border_strong, dark.border_strong),
            ("sidebar_border", light.sidebar_border, dark.sidebar_border),
            ("text_ghost", light.text_ghost, dark.text_ghost),
            ("accent", light.accent, dark.accent),
            ("gauge", light.gauge, dark.gauge),
            ("selection", light.selection, dark.selection),
            ("code_text", light.code_text, dark.code_text),
            ("code_wash", light.code_wash, dark.code_wash),
            ("inverse", light.inverse, dark.inverse),
            ("on_inverse", light.on_inverse, dark.on_inverse),
            ("favorite", light.favorite, dark.favorite),
            ("danger_soft", light.danger_soft, dark.danger_soft),
        ];

        for (name, light, dark) in tokens {
            assert_ne!(light, dark, "adaptive token {name} did not change");
        }
    }

    #[test]
    fn system_mode_follows_window_appearance() {
        assert_eq!(
            Theme::system(WindowAppearance::Dark).appearance,
            Appearance::Dark
        );
        assert_eq!(
            Theme::system(WindowAppearance::VibrantDark).appearance,
            Appearance::Dark
        );
        // A light appearance is the unresolved default on Linux (the portal
        // answer arrives asynchronously), so it resolves to dark there; on
        // macOS `NSAppearance` is synchronous and authoritative.
        #[cfg(target_os = "linux")]
        assert_eq!(
            Theme::system(WindowAppearance::Light).appearance,
            Appearance::Dark,
            "Linux must start dark until the portal is heard from"
        );
        #[cfg(not(target_os = "linux"))]
        assert_eq!(
            Theme::system(WindowAppearance::Light).appearance,
            Appearance::Light
        );
    }

    #[test]
    fn spacing_and_typography_match_waku() {
        let spacing = Spacing::default();
        assert_eq!(spacing.card_corner_radius, px(6.0));
        assert_eq!(spacing.card_gap, px(10.0));
        assert_eq!(spacing.title_strip_height, px(48.0));
        assert_eq!(spacing.traffic_light_inset, px(14.0));
        assert_eq!(spacing.title_strip_icon_size, px(14.0));
        assert_eq!(spacing.titlebar_control_frame, size(px(26.0), px(26.0)));
        assert_eq!(spacing.titlebar_control_spacing, px(6.0));
        assert_eq!(spacing.bottom_bar_height, px(40.0));

        let typography = Typography::default();
        assert_eq!(typography.base_size, px(13.5));
        assert_eq!(typography.code_size, px(11.5));
        assert_eq!(typography.code_line_height, px(17.5));
        assert_eq!(typography.code_weight, FontWeight::NORMAL);
        assert_eq!(typography.large_title, px(20.0));
        assert_eq!(typography.title, px(17.0));
        assert_eq!(typography.title2, px(15.0));
        assert_eq!(typography.title3, px(14.0));
        assert_eq!(typography.headline, px(13.5));
        assert_eq!(typography.callout, px(12.5));
        assert_eq!(typography.footnote, px(11.5));
        assert_eq!(typography.caption2, px(10.5));
        assert_eq!(typography.ui_size, px(11.5));
        assert_eq!(typography.body_line_height, px(21.0));
        assert_eq!(typography.ui_line_height, px(16.0));
        assert_eq!(Theme::dark().translucent_surface_opacity, 0.96);
        assert_eq!(Theme::surface_opacity(true), 0.96);
        assert_eq!(Theme::surface_opacity(false), 1.0);
    }

    /// The radius tokens are waku's measured de-facto scale §A.2 — the
    /// exact steps, in the exact roles. A component's radius should come
    /// from this set; anything outside it is a value nobody measured.
    #[test]
    fn radii_match_waku() {
        let radii = Theme::dark().radii;
        assert_eq!(radii.chip, px(4.0));
        assert_eq!(radii.chip_active, px(5.0));
        assert_eq!(radii.control, px(6.0));
        assert_eq!(radii.row_card, px(7.0));
        assert_eq!(radii.code_block, px(8.0));
        assert_eq!(radii.toast, px(10.0));
        assert_eq!(radii.user_pill, px(12.0));
        assert_eq!(radii.composer, px(13.0));

        // Radii are geometry, not appearance: both palettes share them.
        assert_eq!(Theme::light().radii, Theme::dark().radii);
    }

    #[test]
    fn hsl_conversion_is_exact_for_known_values() {
        // hsla(211, 100%, 50%, 0.55) is the browser-selection blue #007BFF.
        expect_color(hsla(211.0, 1.0, 0.50, 0.55), (0.0, 0.483, 1.0, 0.55));
        // hsla(220, 10%, 90%, 0.07) is a near-white neutral (waku border).
        let border = hsla(220.0, 0.10, 0.90, 0.07);
        assert!(border.r > 0.88 && border.r < 0.92, "r={}", border.r);
        assert!(border.g > 0.88 && border.g < 0.92, "g={}", border.g);
        assert!(border.b > 0.88 && border.b < 0.92, "b={}", border.b);
        assert_eq!(border.a, 0.07);
    }

    /// The portal mapping: dark only for an explicit dark preference;
    /// light and no-preference both resolve light, matching GPUI's own
    /// `window_appearance_from_color_scheme`.
    #[cfg(target_os = "linux")]
    #[test]
    fn portal_color_scheme_mapping() {
        use ashpd::desktop::settings::ColorScheme;
        assert_eq!(
            appearance_from_color_scheme(ColorScheme::PreferDark),
            Appearance::Dark
        );
        assert_eq!(
            appearance_from_color_scheme(ColorScheme::PreferLight),
            Appearance::Light
        );
        assert_eq!(
            appearance_from_color_scheme(ColorScheme::NoPreference),
            Appearance::Light
        );
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

    /// The family the visual bar is set in (JetBrains Mono) wins when it is
    /// actually installed.
    #[test]
    fn code_family_resolution_takes_the_visual_bar_family_first() {
        let installed: HashSet<String> = ["JetBrains Mono", "DejaVu Sans Mono"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(resolve_code_family(&installed), "JetBrains Mono");
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

    /// The theme carries one code-family answer next to the code weight, and
    /// it is a real, non-empty family even before a GPUI app resolves it.
    #[test]
    fn typography_carries_a_code_family_token() {
        let family = Theme::dark().typography.code_family;
        assert!(!family.is_empty());
        assert_eq!(Theme::light().typography.code_family, family);
    }

    /// COSMIC-02: `Theme::dark()`/`light()` must carry a `cosmic` field
    /// whose `is_dark` agrees with the theme's own appearance — the seam
    /// that makes `Theme` the single source COSMIC tokens flow through,
    /// rather than a second, independently-resolved theme a caller could
    /// forget to install.
    #[test]
    fn theme_dark_and_light_carry_agreeing_cosmic_tokens() {
        assert!(Theme::dark().cosmic.is_dark);
        assert!(!Theme::light().cosmic.is_dark);
    }

    /// `Theme::for_mode` must resolve `.cosmic` for the same mode, not a
    /// stale or independently-defaulted one — `System` under a light
    /// window appearance should carry a light COSMIC container hierarchy.
    #[test]
    fn theme_for_mode_resolves_cosmic_for_the_same_appearance() {
        let theme = Theme::for_mode(ThemeMode::Light, WindowAppearance::Light);
        assert!(!theme.cosmic.is_dark);
        assert_eq!(theme.appearance, Appearance::Light);

        let theme = Theme::for_mode(ThemeMode::Dark, WindowAppearance::Dark);
        assert!(theme.cosmic.is_dark);
        assert_eq!(theme.appearance, Appearance::Dark);
    }

    /// A drawn pixel traces to a COSMIC token through `theme.cosmic.spacing`
    /// and `.radii`, not just the container colours — both scales must be
    /// populated, not left at some other default.
    #[test]
    fn theme_cosmic_carries_the_cosmic_spacing_and_radii_scales() {
        let theme = Theme::dark();
        assert_eq!(theme.cosmic.spacing, cosmic::CosmicSpacing::default());
        assert_eq!(theme.cosmic.radii, cosmic::CosmicRadii::default());
    }

    /// The two tokens `codex12` asked for in `QUEUE.md` (P60/P63): a menu
    /// width wide enough for a label plus a shortcut hint, and a hairline
    /// *thickness* distinct from `Colors::hairline`'s colour.
    #[test]
    fn spacing_carries_menu_width_and_hairline_thickness() {
        let spacing = Spacing::default();
        assert_eq!(spacing.menu_width, px(240.0));
        assert_eq!(spacing.hairline_thickness, px(1.0));
    }
}

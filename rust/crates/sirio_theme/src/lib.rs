//! Sirio's shared color, spacing, and typography tokens.
//!
//! Sirio keeps its chrome quiet through a compact cool-tinted shell hierarchy:
//! a translucent frame surrounds opaque panel surfaces, selected rows, and
//! shell borders. Within that hierarchy, generic hover, pressed, and divider
//! washes remain neutral veils; focus and active chrome are spelled with
//! contrast rather than hue, and semantic hues retain their existing state
//! meanings. Colour is spent on two things only: data (a diff, a git status, an
//! agent's brand) and attention (needs-input, error). The brand coral no longer
//! has a role — see [`ThemeColors::accent`].
//!
//! Where each value comes from is recorded in
//! `docs/linux-rewrite/THEME-PROVENANCE.md`. The re-runnable
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

use gpui::{App, FontWeight, Global, Pixels, Rgba, Size, WindowAppearance, px, rgb, size};
use std::collections::HashSet;

/// Pop!_OS COSMIC design tokens, consumed by [`Theme::cosmic`] — every
/// surface that reads `Theme::get(cx)` gets a resolved [`cosmic::CosmicTheme`]
/// with it. See `docs/linux-rewrite/COSMIC-DESIGN.md`.
pub mod cosmic;
use std::ops::Deref;
use std::sync::OnceLock;

/// The appearance selected by Sirio's appearance setting.
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

/// All adaptive colors used by Sirio.
///
/// Two naming layers live here on purpose. The first names what a component
/// *is* (`tab_focus_accent`, `filter_field_bg`) and is what the UI has always
/// consumed; the second names what a value *does* in the design system
/// (`accent`, `raised`, `inset`, `overlay`, …) and is where the first ones
/// resolve to. Components can migrate from the former to the latter without
/// anything being re-derived.
///
/// Where each value comes from — measured off a reference frame, or chosen by
/// us because no frame could settle it — is in
/// `docs/linux-rewrite/THEME-PROVENANCE.md`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Translucent window-frame material. Its RGB value is paired with
    /// [`ThemeColors::frame_fallback`] for platforms without translucency.
    pub frame_surface: Rgba,
    /// Opaque fallback behind the app shell and window canvas.
    pub frame_fallback: Rgba,
    /// Opaque reading and sidebar surface inside the shell.
    pub panel_surface: Rgba,
    /// Opaque separator between shell panels.
    pub panel_border: Rgba,
    /// Focus ring for shell panels — a neutral one step brighter than
    /// [`ThemeColors::panel_border`], not the accent. It is deliberately below
    /// [`ThemeColors::title`]: a focused pane has to be findable, not loud.
    pub panel_focus_ring: Rgba,
    /// Compatibility alias for [`ThemeColors::panel_surface`], used by the tab
    /// bar, workspace column, right panel, and settings.
    pub background: Rgba,
    /// Compatibility alias for [`ThemeColors::frame_fallback`], used behind
    /// the working columns when translucency is unavailable.
    pub canvas: Rgba,
    /// Terminal surface — paper-white in light and the pre-shell dark well in
    /// dark mode, retained independently of the shell panel hierarchy.
    pub terminal_surface: Rgba,
    /// Active-chrome tint: the tab strip's underline and dirty dot, a menu's
    /// checkmark, a running worktree's badge. With colour reserved for data and
    /// attention, contrast is the only channel left to say "this one is
    /// active", so this is the full text neutral rather than a step below it.
    pub tab_focus_accent: Rgba,
    /// Waiting-for-input status.
    pub tab_needs_input: Rgba,
    /// Completed status.
    pub tab_done: Rgba,
    /// Errored status.
    pub tab_error: Rgba,
    /// Compatibility alias for [`ThemeColors::panel_surface`]; the transcript
    /// reads directly on the central panel rather than a floating card.
    pub chat_surface: Rgba,
    /// Legacy sidebar material tint, aliased to [`ThemeColors::panel_surface`].
    pub chrome_tint: Rgba,
    /// Tab-chip underline, aliased to [`ThemeColors::panel_border`].
    pub tab_chip_underline: Rgba,
    /// Shared one-pixel border/divider stroke: a near-white neutral at 7-8%,
    /// so it reads as a seam rather than a line.
    pub hairline: Rgba,
    /// Hover fill for sidebar rows — a 6% neutral layer, not a colour.
    pub row_hover: Rgba,
    /// Hover fill for transcript rows — 5% neutral, a step lighter than
    /// `row_hover` because transcript rows are wider and a 6% wash over that
    /// area reads as a block.
    pub chat_row_hover: Rgba,
    /// Selected-row fill, aliased to [`ThemeColors::selected_fill`]. Text
    /// selection is a different concept: see [`ThemeColors::selection`].
    pub selection_fill: Rgba,
    /// Focused-field border — the same neutral the rest of the active chrome
    /// uses, and never a second blue.
    pub selection_ring: Rgba,
    /// Row title text.
    pub title: Rgba,
    /// Selected row title text, aliased to [`ThemeColors::title`].
    pub title_selected: Rgba,
    /// Secondary row text.
    pub subtitle: Rgba,
    /// Raw sampled meta text, reserved for nonessential metadata and disabled
    /// labels. Body-size secondary text uses [`ThemeColors::subtitle`], which
    /// clears WCAG AA on the panel surface.
    pub meta: Rgba,
    /// Primary pill fill.
    pub primary_pill_bg: Rgba,
    /// Resting fill of a control the user clicks. Must stay distinguishable
    /// from `raised`; that is the property this token exists to preserve.
    pub primary_action_bg: Rgba,
    /// Filter field fill.
    pub filter_field_bg: Rgba,
    /// Tree guide stroke, including its source alpha.
    pub tree_guide: Rgba,
    /// Staged-file status color — the success hue.
    pub git_staged: Rgba,
    /// Modified-file status color — the warning hue.
    pub git_modified: Rgba,
    /// Untracked-file status color — the gauge blue.
    pub git_untracked: Rgba,
    /// Conflict-file status color — the danger hue.
    pub git_conflict: Rgba,
    /// Addition diff accent — the success hue.
    pub diff_addition: Rgba,
    /// Addition diff background — translucent success wash.
    pub diff_addition_background: Rgba,
    /// Deletion diff accent — the danger hue.
    pub diff_deletion: Rgba,
    /// Deletion diff background — translucent danger wash.
    pub diff_deletion_background: Rgba,
    /// Hunk diff background — the same wash inline code sits on.
    pub diff_hunk_background: Rgba,
    /// Chat card fill — cards are raised above the transcript.
    pub card_fill: Rgba,
    /// Recessed code/diff fill — the inverse move: code sits *in* the card.
    pub code_inset_fill: Rgba,
    /// Composer primary text.
    pub primary_text_color: Rgba,
    /// Clickable file-link color — the gauge blue, the one place blue means
    /// "you can click this" rather than "this is a quantity".
    pub file_link: Rgba,
    /// Task-card rail — neutral. A card's kind is already spelled by its icon
    /// and title; the rail only has to separate the card from the transcript.
    pub rail_task: Rgba,
    /// Question-card rail — the warning hue. The one card kind that is waiting
    /// on the reader, and so the one that keeps its colour.
    pub rail_question: Rgba,
    /// Edit-card rail — neutral, like [`ThemeColors::rail_task`].
    pub rail_edit: Rgba,
    /// Tool-card rail — neutral, like [`ThemeColors::rail_task`].
    pub rail_tool: Rgba,

    // ── Role tokens: what a value does, rather than who consumes it ───────
    /// The sidebar fill, retained as an alias for [`ThemeColors::panel_surface`]
    /// while existing consumers migrate to the semantic shell role.
    pub sidebar: Rgba,
    /// Floating cards, popovers, tooltips: a step *above* the surface.
    pub raised: Rgba,
    /// Composer card fill.
    pub composer: Rgba,
    /// Recessed wells: a step *below* the surface.
    pub inset: Rgba,
    /// Generic hover wash — 5% neutral.
    pub overlay: Rgba,
    /// Pressed wash — 9% neutral, so press reads as more than hover.
    pub overlay_strong: Rgba,
    /// Stronger divider, for seams that separate rather than merely delimit.
    pub border_strong: Rgba,
    /// Legacy sidebar seam, aliased to [`ThemeColors::panel_border`].
    pub sidebar_border: Rgba,
    /// Faintest text step — placeholder copy and disabled labels, below
    /// [`ThemeColors::meta`].
    pub text_ghost: Rgba,
    /// Brand coral. No role paints it any more: the shell's focus rings,
    /// caret, selection and active chrome are neutral, and every colour left in
    /// the UI is a status, a diff, a quantity or an agent's own brand.
    ///
    /// What still reads it is the `Coral` entry of the agent-colour picker,
    /// which needs a real coral to offer. Kept as a token rather than inlined
    /// as a literal there, so the picker keeps drawing from `Theme` — and so
    /// the two invariants this value carries (it clears AA on its own surface,
    /// and it is not any agent's brand) still have something to hold.
    pub accent: Rgba,
    /// Quantity blue: quota meters, and the clone and update progress bars.
    /// Blue means "how much", which is why a progress bar is never painted in
    /// a status hue — a bar filling up is not an alert.
    pub gauge: Rgba,
    /// Approved selected-row fill, aliased by [`ThemeColors::selection_fill`].
    /// [`ThemeColors::selection`] remains reserved for text-selection under
    /// glyphs.
    pub selected_fill: Rgba,
    /// Text-selection wash, painted *under* glyphs: the top rung of the veil
    /// ladder, neutral in both appearances. Never used for row chrome — that
    /// is [`ThemeColors::selection_fill`].
    pub selection: Rgba,
    /// The text-insertion caret, everywhere one blinks: composer, address bar,
    /// command palette, every inline rename field. Its own role rather than a
    /// reuse of [`ThemeColors::title`], so that fifteen call sites say what
    /// they mean and the caret can be re-tinted in one place.
    pub caret: Rgba,
    /// Inline `code` foreground. The rounded [`ThemeColors::code_wash`] behind
    /// it already separates code from prose, so the glyphs stay the ordinary
    /// text neutral instead of spending a second signal on the same job.
    pub code_text: Rgba,
    /// Inline `code` rounded wash.
    pub code_wash: Rgba,
    /// Light fill for primary buttons, dark glyph on top.
    pub inverse: Rgba,
    /// Glyph on primary buttons.
    pub on_inverse: Rgba,
    /// Star/favorite amber.
    pub favorite: Rgba,
    /// Soft danger fill (stop button hover).
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
        // Sirio's accent. Part measured, part chosen, and the seam between
        // the two is the whole point — see `THEME-PROVENANCE.md`.
        //
        // Measured: the hue, 24.3°, taken from the warm family the reference
        // frames actually render (their inline-code tone, `#E0A882`, agreeing
        // across three independent spans). Neither frame contains an accent to
        // sample directly — both show one idle chat with no logo, caret, focus
        // ring or activity dot, and a search of the whole frame finds zero
        // pixels within 37 units of any coral — so hue is as much as looking
        // can settle.
        //
        // Chosen: saturation 0.70 and lightness 0.60/0.40, against two
        // constraints rather than taste. Each variant clears WCAG AA on the
        // surface it is painted on (6.62:1 dark, 4.61:1 light — the light one
        // is the first lightness step that does), held by
        // `accent_clears_contrast_on_its_own_surface`. And both stay clear of
        // every `AgentBrandColor`, held by
        // `worktree_activity_colours_name_the_agent_and_never_a_status`: a tab
        // shows its accent and its agent's mark side by side, so an accent
        // that lands on a brand makes the mark stop meaning anything. Claude's
        // `#D97757` is the near one at 22 units, which is also why the obvious
        // shortcut — reusing our own Swift's Claude fill for the accent — is
        // the one coral this app cannot have.
        let accent = Self::adaptive(rgb_hex(0xE08B52), rgb_hex(0xAD581F), appearance);
        // The state hues are not a fresh design problem: Sirio already
        // shipped them. These four are the sRGB components of
        // `App/AppTheme.swift`'s `tabNeedsInput`, `tabDone`, `tabError` and
        // `tabFocusAccent`, transcribed digit for digit from our own macOS
        // app, where they mark the same four things on the same tab strip.
        // They are written as the float triples the Swift declares rather than
        // as hex so the two files can be diffed by eye.
        //
        // Nothing here could have come off the reference frames anyway: both
        // show one idle chat session — no error, no progress gauge, no starred
        // row, no terminal — so there is no pixel of any of these states to
        // sample. Reusing our own is the strictly better answer than inventing
        // a second vocabulary for a meaning we had already fixed.
        //
        // `gauge` is the exception worth naming: in Swift this blue is the
        // focus accent. The Rust accent is coral, which freed the blue, and a
        // progress bar is the one place left that wants a cool hue.
        let warning = Self::adaptive(
            color(0.95, 0.72, 0.28, 1.0),
            color(0.67, 0.42, 0.02, 1.0),
            appearance,
        );
        let success = Self::adaptive(
            color(0.48, 0.78, 0.57, 1.0),
            color(0.10, 0.45, 0.22, 1.0),
            appearance,
        );
        let danger = Self::adaptive(
            color(0.94, 0.43, 0.47, 1.0),
            color(0.68, 0.12, 0.17, 1.0),
            appearance,
        );
        let gauge = Self::adaptive(
            color(0.55, 0.64, 1.00, 1.0),
            color(0.24, 0.38, 0.78, 1.0),
            appearance,
        );
        // A starred row is a louder `warning`, not a fifth colour: same hue,
        // same lightness, all the chroma the pair allows. Held by
        // `favorite_is_the_warning_hue_at_full_chroma`.
        let favorite = {
            let (hue, lightness) = hue_and_lightness(warning);
            hsla(hue, 1.0, lightness, 1.0)
        };
        let frame_fallback = Self::adaptive(rgb_hex(0x222427), rgb_hex(0xDCE5E9), appearance);
        let frame_surface = match appearance {
            Appearance::Dark => softened(frame_fallback, 0.88),
            Appearance::Light => softened(frame_fallback, 0.82),
        };
        let panel_surface = Self::adaptive(rgb_hex(0x18191A), rgb_hex(0xF4F7F8), appearance);
        let panel_border = Self::adaptive(rgb_hex(0x27292D), rgb_hex(0xCCD8DD), appearance);
        let selected_fill = Self::adaptive(rgb_hex(0x2D2F34), rgb_hex(0xD7E2E7), appearance);
        let text = Self::adaptive(rgb_hex(0xCBCDD4), rgb_hex(0x313A40), appearance);
        let text_secondary = Self::adaptive(rgb_hex(0x85888F), rgb_hex(0x667379), appearance);
        let text_tertiary = Self::adaptive(rgb_hex(0x686B71), rgb_hex(0x68757B), appearance);
        let text_ghost = Self::adaptive(rgb_hex(0x575757), rgb_hex(0xA4A4A4), appearance);
        let raised = Self::adaptive(rgb_hex(0x1D1E21), rgb_hex(0xFBFCFC), appearance);
        // One step *into* the page, and derived from the measured surface for
        // the same reason `sidebar` is: a well is a relationship to the page
        // it is cut into, so it should move when the page does. The two
        // factors differ because the move is not symmetric — dark has 26 units
        // of headroom below the surface and can take a big step, light has 246
        // and would go grey long before it read as a well. Both were picked to
        // make the well legible at a glance and neither is a measurement;
        // `the_depth_ladder_reads_as_depth` holds the ordering.
        let inset = Self::adaptive(
            scaled(panel_surface, 0.72),
            scaled(panel_surface, 0.93),
            appearance,
        );
        let composer = raised;
        // A terminal is the deepest thing on the page in dark, and paper in
        // light — the same two extremes `inset` already names.
        let terminal_surface = Self::adaptive(
            scaled(rgb_hex(SURFACE_DARK), 0.72),
            color(1.0, 1.0, 1.0, 1.0),
            appearance,
        );
        // Everything from here to `danger_soft` is a veil off the ladder — see
        // [`veil`] for why washes cannot be measured and must come from one
        // rule instead.
        let border = veil(VEIL_LOW, appearance);
        let border_strong = veil(VEIL_HIGH, appearance);
        // Measured off the seam itself, which is two frame pixels wide — one
        // logical pixel at 2x — and flat at 200/200 in both variants, so these
        // are solid values and not a blend of the surfaces either side.
        let sidebar_border = panel_border;
        let row_hover = veil(VEIL_LOW, appearance);
        // A chat row is most of the width of the pane. The same veil a sidebar
        // row uses would read as a change of surface at that size, so the
        // large-area hover sits one rung lower.
        let overlay = veil(VEIL_FAINT, appearance);
        let overlay_strong = veil(VEIL_MID, appearance);
        // A selection wash sits under its own text, so it has two jobs at
        // once: be visible, and not swallow the glyphs. The top rung of the
        // veil ladder is the strongest wash that still does both in either
        // appearance; `selection_stays_under_its_text` holds the second half.
        let selection = veil(VEIL_HIGH, appearance);
        let code_text = text;
        let code_wash = veil(VEIL_LOW, appearance);
        // An inverted chip — a tooltip, a keycap — is literally the other
        // appearance's page, so it is the same measured pair, swapped. No new
        // number, and it stays right by construction if either is ever
        // re-measured.
        let inverse = Self::adaptive(rgb_hex(0xF4F7F8), rgb_hex(0x18191A), appearance);
        let on_inverse = Self::adaptive(rgb_hex(0x313A40), rgb_hex(0xCBCDD4), appearance);
        let danger_soft = softened(danger, VEIL_MID);

        Self {
            frame_surface,
            frame_fallback,
            panel_surface,
            panel_border,
            panel_focus_ring: text_secondary,
            background: panel_surface,
            canvas: frame_fallback,
            terminal_surface,
            tab_focus_accent: text,
            tab_needs_input: warning,
            tab_done: success,
            tab_error: danger,
            chat_surface: panel_surface,
            chrome_tint: panel_surface,
            tab_chip_underline: panel_border,
            hairline: border,
            row_hover,
            chat_row_hover: overlay,
            selection_fill: selected_fill,
            selection_ring: text,
            title: text,
            title_selected: text,
            subtitle: text_secondary,
            meta: text_tertiary,
            primary_pill_bg: raised,
            primary_action_bg: selected_fill,
            filter_field_bg: inset,
            tree_guide: veil(VEIL_MID, appearance),
            git_staged: success,
            git_modified: warning,
            git_untracked: gauge,
            git_conflict: danger,
            diff_addition: success,
            // The band under a diff line is the line's own colour turned down,
            // never a second green or a second red — see [`softened`].
            diff_addition_background: softened(success, VEIL_MID),
            diff_deletion: danger,
            diff_deletion_background: softened(danger, VEIL_MID),
            diff_hunk_background: code_wash,
            card_fill: raised,
            code_inset_fill: inset,
            primary_text_color: text,
            file_link: gauge,
            rail_task: border_strong,
            rail_question: warning,
            rail_edit: border_strong,
            rail_tool: border_strong,
            sidebar: panel_surface,
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
            caret: text,
            selected_fill,
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
    fn default() -> Self {
        Self {
            shell_gap: px(4.0),
            shell_outer_inset: px(4.0),
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
            compact_action: px(24.0),
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
    fn default() -> Self {
        Self {
            shell_panel: px(7.0),
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
/// neutral buttons — minimize and maximize — read `cosmic.semantic.
/// icon_button` like every other icon button on this bar, and that is
/// fidelity, not a compromise: Windows 11 draws *their* hover as a neutral
/// veil that follows the light/dark theme. The close button is the one
/// exception, because its red is a **system constant** that says "this
/// closes the window" in every Windows app regardless of the app's theme.
///
/// `cosmic.semantic.destructive` cannot stand in for it. In dark mode that
/// token is `#FFA09A` — a pale salmon whose `on` colour is **black**,
/// i.e. a light fill carrying dark text; the Windows red is a saturated
/// fill carrying **white** text. The two run in opposite contrast
/// directions, so substituting one for the other does not give a different
/// red, it gives a pink close button with a black glyph. In light mode the
/// same token flips to `#890418`, a dark maroon, so the two appearances
/// would not even resemble each other.
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
// Platform split: on macOS the Apple/Xcode faces (SF Mono, SF Pro) lead;
// elsewhere the JetBrains faces (JetBrains Mono — SIL OFL 1.1; JetBrains
// Sans — Apache 2.0 — both open source) lead. Each list was verified
// against `fc-list : family` on the Linux build machine rather than
// assumed.

/// The sans-serif families to prefer, in order, when resolving the UI font.
#[cfg(not(target_os = "macos"))]
pub const UI_FAMILY_CANDIDATES: &[&str] = &[
    "JetBrains Sans",
    "Inter",
    "Ubuntu",
    "Noto Sans",
    "Cantarell",
    "DejaVu Sans",
];

/// The sans-serif families to prefer, in order, when resolving the UI font
/// on macOS — Apple's own SF Pro (Xcode's UI face) first.
#[cfg(target_os = "macos")]
pub const UI_FAMILY_CANDIDATES: &[&str] = &[
    "SF Pro",
    "SF Pro Text",
    "SF Pro Display",
    "JetBrains Sans",
    "Inter",
    "Helvetica Neue",
];

/// Monospace families to prefer, in order, when resolving the code font.
///
/// First the family the visual bar is set in (waku's JetBrains Mono), then
/// common good monospaced faces, then whatever the system's generic
/// "monospace" resolves to (fontconfig's alias on Linux, always present).
/// A candidate that is not installed is skipped — never guessed at.
#[cfg(not(target_os = "macos"))]
pub const CODE_FAMILY_CANDIDATES: &[&str] = &[
    "JetBrains Mono",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// Monospace families to prefer, in order, when resolving the code font on
/// macOS — Apple's own SF Mono (Xcode's editor face) first.
#[cfg(target_os = "macos")]
pub const CODE_FAMILY_CANDIDATES: &[&str] = &[
    "SF Mono",
    "JetBrains Mono",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// Monospace families to prefer, in order, when resolving the terminal
/// font. The terminal renders agent TUIs whose glyph set needs a Nerd Font
/// (MesloLGS Nerd Font Mono is what Claude Code itself asks to be
/// installed), so the Nerd Font builds of JetBrains Mono lead, with the
/// plain JetBrains Mono and the generic chain behind them.
#[cfg(not(target_os = "macos"))]
pub const TERMINAL_FAMILY_CANDIDATES: &[&str] = &[
    "JetBrainsMono Nerd Font",
    "JetBrains Mono NL Nerd Font",
    "MesloLGS Nerd Font Mono",
    "JetBrains Mono",
    "Fira Mono",
    "Hack",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// Monospace families to prefer, in order, when resolving the terminal
/// font on macOS — Apple's own SF Mono first, the Nerd Font faces after.
#[cfg(target_os = "macos")]
pub const TERMINAL_FAMILY_CANDIDATES: &[&str] = &[
    "SF Mono",
    "JetBrainsMono Nerd Font",
    "JetBrains Mono NL Nerd Font",
    "MesloLGS Nerd Font Mono",
    "JetBrains Mono",
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

    /// Colour of commit-graph lane `index`, cycling.
    ///
    /// Deliberately an accessor over hues this palette already measures
    /// rather than six new tokens: the palettes are pinned against a frozen
    /// provenance record (see `dark_palette_matches_recorded_provenance`),
    /// so every added colour is a colour someone has to measure and justify.
    /// The graph only needs its lanes to be mutually distinguishable.
    pub fn graph_lane(&self, index: usize) -> Rgba {
        let lanes = [
            self.tab_focus_accent,
            self.git_untracked,
            self.tab_done,
            self.tab_needs_input,
            self.tab_error,
            self.favorite,
        ];
        lanes[index % lanes.len()]
    }

    /// Installs a theme, resolving `System` against the current window
    /// appearance. On Linux the resolution is dark-biased until the portal
    /// is heard from (see [`ThemeMode::resolve_system`]).
    pub fn install(mode: ThemeMode, cx: &mut App) {
        Self::resolve_font_families(cx);
        #[cfg(target_os = "linux")]
        let theme = Self::for_mode_linux(mode, cx.window_appearance());
        #[cfg(not(target_os = "linux"))]
        let theme = Self::for_mode(mode, cx.window_appearance());
        let translucency = cx
            .try_global::<Self>()
            .is_some_and(|theme| theme.translucency_enabled);
        cx.set_global(theme.with_translucency(translucency));
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
                    let next = Theme::for_appearance(ThemeMode::System, preference)
                        .with_translucency(cx.global::<Theme>().translucency_enabled);
                    cx.set_global(next);
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
    ///
    /// The fade is 0.85: strong enough to read as real translucency (the
    /// original Swift-era 0.96 was imperceptible), light enough that a
    /// panel at 0.85 alpha still holds its text at WCAG AA against the
    /// blurred backdrop — the frame material behind it is a near-identical
    /// grey in both appearances, so the composite barely shifts.
    pub fn surface_opacity(translucency_enabled: bool) -> f32 {
        if translucency_enabled { 0.85 } else { 1.0 }
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
    /// translucent panel keeps its contrast.
    pub fn with_translucency(self, enabled: bool) -> Self {
        let mut theme = Self::for_appearance(self.mode, self.appearance);
        theme.translucency_enabled = enabled;
        if !enabled {
            return theme;
        }
        let opacity = Self::surface_opacity(true);
        let fade = |surface: Rgba| softened(surface, opacity);
        theme.colors.panel_surface = fade(theme.colors.panel_surface);
        theme.colors.background = theme.colors.panel_surface;
        theme.colors.sidebar = theme.colors.panel_surface;
        theme.colors.chat_surface = theme.colors.panel_surface;
        theme.colors.chrome_tint = theme.colors.panel_surface;
        theme.colors.raised = fade(theme.colors.raised);
        theme.colors.composer = theme.colors.raised;
        theme.colors.inset = fade(theme.colors.inset);
        theme.colors.terminal_surface = fade(theme.colors.terminal_surface);
        theme
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
    std::thread::current()
        .name()
        .is_some_and(|name| name.starts_with("tests::") || name.contains("::tests::"))
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
/// Claude resolved to `Amber` — i.e. to `theme.tab_needs_input` itself. A
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

/// Scales an opaque colour's channels toward black, keeping alpha.
///
/// Used where one variant of a token is *our derivation* of another rather
/// than a second independent measurement. Writing the relationship down is the
/// point: a derived value can be checked against its source, and a pasted hex
/// cannot be checked against anything.
fn scaled(color: Rgba, factor: f32) -> Rgba {
    Rgba {
        r: color.r * factor,
        g: color.g * factor,
        b: color.b * factor,
        a: color.a,
    }
}

/// A neutral veil at `alpha`: white over the dark palette, black over the
/// light one.
///
/// Generic hairlines, hovers, overlays, guides, and code/hunk washes use these
/// neutral veils. The approved shell frame, panels, selected rows, and panel
/// borders are separate cool-tinted roles and must not be folded into this
/// helper. A veil is translucent while a screenshot is flat, so compositing
/// cannot recover its source rgba; centralizing these generic washes in one
/// rule avoids inventing independent structural colours.
fn veil(alpha: f32, appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => color(1.0, 1.0, 1.0, alpha),
        Appearance::Light => color(0.0, 0.0, 0.0, alpha),
    }
}

/// The alpha ladder every [`veil`] and soft fill is drawn from. Four rungs,
/// each half again the one below it (0.05 · 0.08 · 0.12 · 0.18), which is the
/// smallest step that stays visible when two of them meet along an edge. Held
/// by `the_veil_ladder_is_geometric`.
const VEIL_FAINT: f32 = 0.05;
const VEIL_LOW: f32 = 0.08;
const VEIL_MID: f32 = 0.12;
const VEIL_HIGH: f32 = 0.18;

/// The former dark page, retained solely to preserve the terminal's established
/// dark well while the shell moves to its new panel surface.
const SURFACE_DARK: u32 = 0x1A_1A_1A;

/// The same colour at a lower opacity.
///
/// Used for the soft fills that sit *under* text of the same meaning — a
/// deleted diff line under red text, a danger banner under a danger label. The
/// fill is not a second red to choose; it is the one red already chosen,
/// turned down.
fn softened(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

/// HSL hue in degrees, and HSL lightness in 0..1, of an sRGB colour.
///
/// The inverse of [`hsla`], and only used to state one token as a
/// transformation of another rather than as a fresh number.
fn hue_and_lightness(c: Rgba) -> (f32, f32) {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    let delta = max - min;
    let lightness = (max + min) / 2.0;
    if delta <= f32::EPSILON {
        return (0.0, lightness);
    }
    let hue = if max == c.r {
        60.0 * (((c.g - c.b) / delta).rem_euclid(6.0))
    } else if max == c.g {
        60.0 * ((c.b - c.r) / delta + 2.0)
    } else {
        60.0 * ((c.r - c.g) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), lightness)
}

/// Converts a CSS-style `hsla(h, s, l, a)` value (h in **degrees**, s/l/a in
/// 0..1) to sRGB. Washes and overlays are written this way because they are
/// specified as "N% neutral at M% opacity", which hsl states directly and hex
/// cannot state at all.
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
    fn relative_luminance(color: Rgba) -> f32 {
        let channel = |c: f32| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    }

    fn contrast_ratio(one: Rgba, other: Rgba) -> f32 {
        let (a, b) = (relative_luminance(one), relative_luminance(other));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn intellij_shell_palette_matches_the_approved_reference() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_eq!(dark.frame_fallback, rgb_hex(0x222427));
        assert_eq!(
            dark.frame_surface,
            Rgba {
                r: 0x22 as f32 / 255.0,
                g: 0x24 as f32 / 255.0,
                b: 0x27 as f32 / 255.0,
                a: 0.88,
            }
        );
        assert_eq!(dark.panel_surface, rgb_hex(0x18191A));
        assert_eq!(dark.selected_fill, rgb_hex(0x2D2F34));
        assert_eq!(dark.panel_border, rgb_hex(0x27292D));
        assert_eq!(dark.raised, rgb_hex(0x1D1E21));
        assert_eq!(dark.inset, scaled(dark.panel_surface, 0.72));
        assert_eq!(dark.title, rgb_hex(0xCBCDD4));
        assert_eq!(dark.subtitle, rgb_hex(0x85888F));
        assert_eq!(dark.meta, rgb_hex(0x686B71));

        assert_eq!(light.frame_fallback, rgb_hex(0xDCE5E9));
        assert_eq!(
            light.frame_surface,
            Rgba {
                r: 0xDC as f32 / 255.0,
                g: 0xE5 as f32 / 255.0,
                b: 0xE9 as f32 / 255.0,
                a: 0.82,
            }
        );
        assert_eq!(light.panel_surface, rgb_hex(0xF4F7F8));
        assert_eq!(light.selected_fill, rgb_hex(0xD7E2E7));
        assert_eq!(light.panel_border, rgb_hex(0xCCD8DD));
        assert_eq!(light.raised, rgb_hex(0xFBFCFC));
        assert_eq!(light.inset, scaled(light.panel_surface, 0.93));
        assert_eq!(light.title, rgb_hex(0x313A40));
        assert_eq!(light.subtitle, rgb_hex(0x667379));
        assert_eq!(light.meta, rgb_hex(0x68757B));

        for theme in [dark, light] {
            assert_eq!(theme.background, theme.panel_surface);
            assert_eq!(theme.sidebar, theme.panel_surface);
            assert_eq!(theme.chat_surface, theme.panel_surface);
            assert_eq!(theme.chrome_tint, theme.panel_surface);
            assert_eq!(theme.canvas, theme.frame_fallback);
            assert_eq!(theme.selection_fill, theme.selected_fill);
            assert_eq!(theme.title_selected, theme.title);
            assert_eq!(theme.sidebar_border, theme.panel_border);
            assert_eq!(theme.tab_chip_underline, theme.panel_border);
            assert_eq!(theme.composer, theme.raised);
            assert_eq!(theme.card_fill, theme.raised);
            assert_eq!(theme.primary_pill_bg, theme.raised);
            assert_ne!(theme.primary_action_bg, theme.raised);
            assert_eq!(theme.filter_field_bg, theme.inset);
            assert_eq!(theme.code_inset_fill, theme.inset);
            // Active chrome, the caret and inline code all resolve to the
            // text neutral; the pane's focus ring sits one step below it, so a
            // focused pane is findable without shouting.
            assert_eq!(theme.tab_focus_accent, theme.title);
            assert_eq!(theme.selection_ring, theme.tab_focus_accent);
            assert_eq!(theme.caret, theme.tab_focus_accent);
            assert_eq!(theme.code_text, theme.title);
            assert_eq!(theme.panel_focus_ring, theme.subtitle);
            assert_ne!(theme.panel_focus_ring, theme.panel_border);
            // The coral is still a value the theme hands out — the agent-colour
            // picker offers it — but no role paints it any more. This is the
            // assertion that catches an accent creeping back into the chrome.
            assert_ne!(theme.tab_focus_accent, theme.accent);
            assert_eq!(theme.primary_text_color, theme.title);
        }
    }

    #[test]
    fn shell_body_text_meets_wcag_aa_on_its_panel() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (role, text) in [("primary", theme.title), ("secondary", theme.subtitle)] {
                let ratio = contrast_ratio(text, theme.panel_surface);
                assert!(
                    ratio >= 4.5,
                    "{label} {role} text contrast on the panel is {ratio:.2}:1, under WCAG AA"
                );
            }
        }
    }

    #[test]
    fn shell_geometry_is_compact_and_consistent() {
        let spacing = Spacing::default();
        let radii = Radii::default();
        assert_eq!(spacing.shell_gap, px(4.0));
        assert_eq!(spacing.shell_outer_inset, px(4.0));
        assert_eq!(radii.shell_panel, px(7.0));
    }

    /// Composites `over` (which may be translucent) onto `under`, so a wash
    /// can be judged the way a reader actually sees it.
    fn composite(over: Rgba, under: Rgba) -> Rgba {
        let a = over.a;
        Rgba {
            r: over.r * a + under.r * (1.0 - a),
            g: over.g * a + under.g * (1.0 - a),
            b: over.b * a + under.b * (1.0 - a),
            a: 1.0,
        }
    }

    /// Every rung of the veil ladder is half again the rung below it.
    ///
    /// The ladder is the *only* free parameter left in the wash tokens — each
    /// of the ten is `veil(rung)` and nothing else — so this is where the
    /// arbitrariness is concentrated, deliberately, in four numbers with a
    /// stated relationship instead of twenty without one.
    #[test]
    fn the_veil_ladder_is_geometric() {
        let ladder = [VEIL_FAINT, VEIL_LOW, VEIL_MID, VEIL_HIGH];
        for pair in ladder.windows(2) {
            let ratio = pair[1] / pair[0];
            assert!(
                (1.4..=1.65).contains(&ratio),
                "{} -> {} is a {ratio:.2}x step, not the declared ~1.5x",
                pair[0],
                pair[1]
            );
        }
    }

    /// Veil-backed structural washes remain neutral.
    ///
    /// The cool-tinted shell surfaces have their own approved roles. This test
    /// deliberately covers only the generic washes backed by [`veil`]: a
    /// hairline, hover, overlay, guide, or hunk wash communicates structure,
    /// not an additional semantic colour.
    #[test]
    fn veil_backed_structural_washes_are_neutral() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (name, c) in [
                ("hairline", theme.hairline),
                ("row_hover", theme.row_hover),
                ("chat_row_hover", theme.chat_row_hover),
                ("overlay", theme.overlay),
                ("overlay_strong", theme.overlay_strong),
                ("tree_guide", theme.tree_guide),
                ("diff_hunk_background", theme.diff_hunk_background),
            ] {
                assert!(
                    (c.r - c.g).abs() < 0.01 && (c.g - c.b).abs() < 0.01,
                    "{label} veil-backed {name} is tinted: ({}, {}, {})",
                    c.r,
                    c.g,
                    c.b
                );
            }
        }
    }

    /// A soft fill is its own meaning's colour turned down, never a second
    /// colour picked to sit near it.
    #[test]
    fn soft_fills_are_their_own_meanings_colour() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (name, fill, parent) in [
                ("danger_soft", theme.danger_soft, theme.diff_deletion),
                (
                    "diff_deletion_background",
                    theme.diff_deletion_background,
                    theme.diff_deletion,
                ),
                (
                    "diff_addition_background",
                    theme.diff_addition_background,
                    theme.diff_addition,
                ),
            ] {
                assert!(
                    (fill.r - parent.r).abs() < 0.004
                        && (fill.g - parent.g).abs() < 0.004
                        && (fill.b - parent.b).abs() < 0.004,
                    "{label} {name} is a different hue from the text it sits under"
                );
                assert!(fill.a < 1.0, "{label} {name} should be a wash");
            }
        }
    }

    /// A well is always deeper than a card, and deeper than the page, in both
    /// appearances — the ordering `inset` is derived to produce.
    #[test]
    fn the_depth_ladder_reads_as_depth() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            assert!(
                relative_luminance(theme.inset) < relative_luminance(theme.background),
                "{label}: inset is not below the page"
            );
            assert!(
                relative_luminance(theme.inset) < relative_luminance(theme.raised),
                "{label}: inset is not below a raised card"
            );
        }
    }

    /// The star is a louder `warning`, not a fifth colour.
    #[test]
    fn favorite_is_the_warning_hue_at_full_chroma() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let (star_hue, star_light) = hue_and_lightness(theme.favorite);
            let (warn_hue, warn_light) = hue_and_lightness(theme.tab_needs_input);
            assert!(
                (star_hue - warn_hue).abs() < 1.0,
                "{label}: star hue {star_hue} left warning's {warn_hue}"
            );
            assert!(
                (star_light - warn_light).abs() < 0.01,
                "{label}: star lightness {star_light} left warning's {warn_light}"
            );
            let chroma = |c: Rgba| c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b);
            assert!(
                chroma(theme.favorite) >= chroma(theme.tab_needs_input),
                "{label}: the star is not the louder of the two"
            );
        }
    }

    /// An inverted chip is the other appearance's page, and its text.
    #[test]
    fn inverse_is_the_other_appearances_page() {
        let dark = Theme::dark();
        let light = Theme::light();
        expect_color(
            dark.inverse,
            (
                light.background.r,
                light.background.g,
                light.background.b,
                1.0,
            ),
        );
        expect_color(
            light.inverse,
            (dark.background.r, dark.background.g, dark.background.b, 1.0),
        );
        expect_color(
            dark.on_inverse,
            (light.title.r, light.title.g, light.title.b, 1.0),
        );
        expect_color(
            light.on_inverse,
            (dark.title.r, dark.title.g, dark.title.b, 1.0),
        );
    }

    /// Selected text stays readable through its own selection wash.
    ///
    /// Selection is the accent turned down, and the accent is a mid-lightness
    /// coral, so this is the constraint that fixes *how far* down: the two
    /// alphas are the loudest each appearance can take while the glyphs under
    /// them still clear WCAG AA.
    #[test]
    fn selection_stays_under_its_text() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let seen = composite(theme.selection, theme.background);
            let ratio = contrast_ratio(theme.title, seen);
            assert!(
                ratio >= 4.5,
                "{label}: selected text reads at {ratio:.2}:1 through its own wash"
            );
        }
    }

    /// The four state hues are still the ones the original macOS app shipped.
    ///
    /// They are not measurable — neither reference frame contains an error, a
    /// gauge, a starred row or a terminal — so their provenance is that Sirio
    /// already had them, for the same four meanings on the same tab strip.
    ///
    /// This used to read `App/AppTheme.swift` at compile time (`include_str!`)
    /// so drift between two *live* files would fail the build. The Swift app
    /// was removed once the Rust port covered the inventory; `App/AppTheme.swift`
    /// can no longer drift because it no longer exists in the tree. The four
    /// triples below are copied byte-for-byte from `static let tab* = dynamic(...)`
    /// in `App/AppTheme.swift` at commit 5430d7bf, the last commit containing the
    /// Swift tree (`git show 5430d7bf:App/AppTheme.swift`) — this is now a frozen
    /// provenance record, not a live cross-check, and that is a deliberate
    /// narrowing of what the test proves, not an oversight.
    #[test]
    fn the_state_hues_are_the_ones_the_swift_app_shipped() {
        // (light r, light g, light b, dark r, dark g, dark b)
        let declared: [(&str, (f32, f32, f32), (f32, f32, f32)); 4] = [
            ("tabNeedsInput", (0.67, 0.42, 0.02), (0.95, 0.72, 0.28)),
            ("tabDone", (0.10, 0.45, 0.22), (0.48, 0.78, 0.57)),
            ("tabError", (0.68, 0.12, 0.17), (0.94, 0.43, 0.47)),
            ("tabFocusAccent", (0.24, 0.38, 0.78), (0.55, 0.64, 1.00)),
        ];
        let declared_for = |token: &str, dark: bool| -> (f32, f32, f32) {
            let (_, light, dark_rgb) = declared
                .iter()
                .find(|(name, _, _)| *name == token)
                .unwrap_or_else(|| panic!("{token} is missing from the frozen Swift record"));
            if dark { *dark_rgb } else { *light }
        };

        for (dark_mode, theme) in [(true, Theme::dark()), (false, Theme::light())] {
            for (token, ours) in [
                ("tabNeedsInput", theme.tab_needs_input),
                ("tabDone", theme.tab_done),
                ("tabError", theme.tab_error),
                ("tabFocusAccent", theme.gauge),
            ] {
                let (r, g, b) = declared_for(token, dark_mode);
                expect_color(ours, (r, g, b, 1.0));
            }
        }
    }

    /// The accent must stay legible on the surface it is painted on, in both
    /// appearances.
    ///
    /// This is the rule the light accent is *derived* from rather than a
    /// property observed after the fact, which is the point: the value it
    /// replaced was carried over from another project's source and managed
    /// only 3.73:1 here, so nothing but a test keeps a future edit from
    /// drifting back under the line.
    #[test]
    fn accent_clears_contrast_on_its_own_surface() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let ratio = contrast_ratio(theme.accent, theme.background);
            assert!(
                ratio >= 4.5,
                "{label}: accent contrast against its surface is {ratio:.2}:1, under WCAG AA 4.5:1"
            );
        }
    }

    /// The accent must never be one of the agent brands.
    ///
    /// A tab row paints its accent and its agent's mark at the same time, so
    /// an accent that lands on a brand makes that mark stop distinguishing
    /// anything — the row looks identically tinted whichever agent is running.
    /// This is not hypothetical: Claude's `#D97757` is the nearest brand to
    /// where the accent sits, so the tempting move of reusing our own Swift's
    /// Claude fill is exactly the one that breaks it.
    #[test]
    fn accent_is_not_any_agent_brand() {
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
                    theme.accent,
                    brand.color(),
                    "{label}: the accent is {brand:?}'s brand, so that agent's mark \
                     no longer marks anything"
                );
            }
        }
    }

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

    /// The dark palette, against `docs/linux-rewrite/THEME-PROVENANCE.md`.
    ///
    /// The assertions cover the current shell values recorded in provenance.
    /// `./Scripts/measure-theme.py` reproduces historical Waku values
    /// only; the current shell is audited via the IntelliJ screenshot
    /// fingerprint, dimensions, method, and sampling rectangles documented
    /// there.
    #[test]
    fn dark_palette_matches_recorded_provenance() {
        let theme = Theme::dark();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(
            theme.canvas,
            f(
                0x22 as f32 / 255.0,
                0x24 as f32 / 255.0,
                0x27 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.background,
            f(
                0x18 as f32 / 255.0,
                0x19 as f32 / 255.0,
                0x1A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.sidebar,
            f(
                0x18 as f32 / 255.0,
                0x19 as f32 / 255.0,
                0x1A as f32 / 255.0,
            ),
        );
        // The well: the measured 0x1A surface stepped down by the declared
        // 0.72, spelled out here rather than restated as a hex so the test
        // fails if either half of the derivation moves.
        let well = 0x1A as f32 * 0.72 / 255.0;
        expect_color(theme.terminal_surface, f(well, well, well));
        expect_color(
            theme.raised,
            f(
                0x1D as f32 / 255.0,
                0x1E as f32 / 255.0,
                0x21 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.composer,
            f(
                0x1D as f32 / 255.0,
                0x1E as f32 / 255.0,
                0x21 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.inset,
            f(
                0x18 as f32 * 0.72 / 255.0,
                0x19 as f32 * 0.72 / 255.0,
                0x1A as f32 * 0.72 / 255.0,
            ),
        );
        expect_color(
            theme.accent,
            f(
                0xE0 as f32 / 255.0,
                0x8B as f32 / 255.0,
                0x52 as f32 / 255.0,
            ),
        );
        // Active chrome is the text neutral, not the coral.
        expect_color(
            theme.tab_focus_accent,
            f(
                0xCB as f32 / 255.0,
                0xCD as f32 / 255.0,
                0xD4 as f32 / 255.0,
            ),
        );
        // Swift `AppTheme.tabFocusAccent`, dark.
        expect_color(theme.gauge, f(0.55, 0.64, 1.00));
        expect_color(
            theme.title,
            f(
                0xCB as f32 / 255.0,
                0xCD as f32 / 255.0,
                0xD4 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.subtitle,
            f(
                0x85 as f32 / 255.0,
                0x88 as f32 / 255.0,
                0x8F as f32 / 255.0,
            ),
        );
        expect_color(
            theme.meta,
            f(
                0x68 as f32 / 255.0,
                0x6B as f32 / 255.0,
                0x71 as f32 / 255.0,
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
        // The three state hues, digit for digit from `App/AppTheme.swift`'s
        // `tabNeedsInput` / `tabDone` / `tabError`, dark variants.
        expect_color(theme.tab_needs_input, f(0.95, 0.72, 0.28));
        expect_color(theme.tab_done, f(0.48, 0.78, 0.57));
        expect_color(theme.tab_error, f(0.94, 0.43, 0.47));
        // `tab_needs_input`'s hue (39.4°) and lightness (0.615) at s = 1.
        expect_color(theme.favorite, f(1.0, 0.7357, 0.23));
        expect_color(
            theme.code_text,
            f(
                0xCB as f32 / 255.0,
                0xCD as f32 / 255.0,
                0xD4 as f32 / 255.0,
            ),
        );
        // An inverted chip in dark is the *light* page and the light page's
        // text — the measured pair, swapped.
        expect_color(
            theme.inverse,
            f(
                0xF4 as f32 / 255.0,
                0xF7 as f32 / 255.0,
                0xF8 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.on_inverse,
            f(
                0x31 as f32 / 255.0,
                0x3A as f32 / 255.0,
                0x40 as f32 / 255.0,
            ),
        );

        // Hairlines, hover, and overlay remain neutral veils; panel seams use
        // the separate opaque shell role.
        expect_color(theme.row_hover, (1.0, 1.0, 1.0, VEIL_LOW));
        expect_color(theme.overlay, (1.0, 1.0, 1.0, VEIL_FAINT));
        expect_color(theme.hairline, (1.0, 1.0, 1.0, VEIL_LOW));
        expect_color(
            theme.tab_chip_underline,
            f(
                0x27 as f32 / 255.0,
                0x29 as f32 / 255.0,
                0x2D as f32 / 255.0,
            ),
        );
        // Selection is the top rung of the veil ladder, neither a turned-down
        // accent nor a borrowed browser blue.
        expect_color(theme.selection, (1.0, 1.0, 1.0, VEIL_HIGH));
    }

    /// The light palette, against `docs/linux-rewrite/THEME-PROVENANCE.md`.
    #[test]
    fn light_palette_matches_recorded_provenance() {
        let theme = Theme::light();
        let f = |r, g, b| (r, g, b, 1.0);

        expect_color(
            theme.canvas,
            f(
                0xDC as f32 / 255.0,
                0xE5 as f32 / 255.0,
                0xE9 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.background,
            f(
                0xF4 as f32 / 255.0,
                0xF7 as f32 / 255.0,
                0xF8 as f32 / 255.0,
            ),
        );
        expect_color(theme.terminal_surface, f(1.0, 1.0, 1.0));
        expect_color(
            theme.raised,
            f(
                0xFB as f32 / 255.0,
                0xFC as f32 / 255.0,
                0xFC as f32 / 255.0,
            ),
        );
        expect_color(
            theme.accent,
            f(
                0xAD as f32 / 255.0,
                0x58 as f32 / 255.0,
                0x1F as f32 / 255.0,
            ),
        );
        // Swift `AppTheme.tabFocusAccent`, light.
        expect_color(theme.gauge, f(0.24, 0.38, 0.78));
        expect_color(
            theme.title,
            f(
                0x31 as f32 / 255.0,
                0x3A as f32 / 255.0,
                0x40 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.subtitle,
            f(
                0x66 as f32 / 255.0,
                0x73 as f32 / 255.0,
                0x79 as f32 / 255.0,
            ),
        );
        expect_color(
            theme.meta,
            f(
                0x68 as f32 / 255.0,
                0x75 as f32 / 255.0,
                0x7B as f32 / 255.0,
            ),
        );
        // `App/AppTheme.swift`'s three state hues, light variants.
        expect_color(theme.tab_needs_input, f(0.67, 0.42, 0.02));
        expect_color(theme.tab_done, f(0.10, 0.45, 0.22));
        expect_color(theme.tab_error, f(0.68, 0.12, 0.17));
        // `tab_needs_input`'s hue (36.9°) and lightness (0.345) at s = 1.
        expect_color(theme.favorite, f(0.69, 0.4246, 0.0));
        // Light veils are pure black, same ladder.
        expect_color(theme.row_hover, (0.0, 0.0, 0.0, VEIL_LOW));
        expect_color(theme.overlay, (0.0, 0.0, 0.0, VEIL_FAINT));
        expect_color(theme.hairline, (0.0, 0.0, 0.0, VEIL_LOW));
        expect_color(
            theme.tab_chip_underline,
            f(
                0xCC as f32 / 255.0,
                0xD8 as f32 / 255.0,
                0xDD as f32 / 255.0,
            ),
        );
        expect_color(theme.selection, (0.0, 0.0, 0.0, VEIL_HIGH));
        // The well, mirrored: the light page has far less room below it, so
        // the step is 0.93 rather than dark's 0.72.
        expect_color(
            theme.inset,
            f(
                0xF4 as f32 * 0.93 / 255.0,
                0xF7 as f32 * 0.93 / 255.0,
                0xF8 as f32 * 0.93 / 255.0,
            ),
        );
        expect_color(
            theme.code_text,
            f(
                0x31 as f32 / 255.0,
                0x3A as f32 / 255.0,
                0x40 as f32 / 255.0,
            ),
        );
        // And in light, an inverted chip is the *dark* page and its text.
        expect_color(
            theme.inverse,
            f(
                0x18 as f32 / 255.0,
                0x19 as f32 / 255.0,
                0x1A as f32 / 255.0,
            ),
        );
        expect_color(
            theme.on_inverse,
            f(
                0xCB as f32 / 255.0,
                0xCD as f32 / 255.0,
                0xD4 as f32 / 255.0,
            ),
        );
    }

    #[test]
    fn every_adaptive_token_differs_between_light_and_dark() {
        let light = Theme::light().colors;
        let dark = Theme::dark().colors;
        let tokens = [
            ("frame_surface", light.frame_surface, dark.frame_surface),
            ("frame_fallback", light.frame_fallback, dark.frame_fallback),
            ("panel_surface", light.panel_surface, dark.panel_surface),
            ("panel_border", light.panel_border, dark.panel_border),
            (
                "panel_focus_ring",
                light.panel_focus_ring,
                dark.panel_focus_ring,
            ),
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
                "primary_action_bg",
                light.primary_action_bg,
                dark.primary_action_bg,
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
            ("caret", light.caret, dark.caret),
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
        assert_eq!(typography.base_size, px(14.5));
        assert_eq!(typography.code_size, px(13.0));
        assert_eq!(typography.code_line_height, px(19.0));
        assert_eq!(typography.code_weight, FontWeight::NORMAL);
        assert_eq!(typography.large_title, px(21.0));
        assert_eq!(typography.title, px(18.0));
        assert_eq!(typography.title2, px(16.0));
        assert_eq!(typography.title3, px(15.0));
        assert_eq!(typography.headline, px(15.0));
        assert_eq!(typography.callout, px(14.0));
        assert_eq!(typography.footnote, px(13.0));
        // The single departure from the measured scale: raised from
        // waku's 10.5 for legibility, which puts it above `footnote`.
        // Recorded in `sirio_ui::conformance`'s departures ledger.
        assert_eq!(typography.caption2, px(14.0));
        assert_eq!(typography.ui_size, px(13.0));
        assert_eq!(typography.body_line_height, px(22.0));
        assert_eq!(typography.ui_line_height, px(17.0));
        assert_eq!(Theme::dark().translucent_surface_opacity, 0.85);
        assert_eq!(Theme::surface_opacity(true), 0.85);
        assert_eq!(Theme::surface_opacity(false), 1.0);
    }

    #[test]
    fn with_translucency_fades_structural_surfaces_and_is_reversible() {
        for base in [Theme::dark(), Theme::light()] {
            let opacity = Theme::surface_opacity(true);
            let translucent = base.with_translucency(true);

            assert!(translucent.translucency_enabled);
            assert_eq!(
                translucent.panel_surface,
                softened(base.panel_surface, opacity)
            );
            assert_eq!(translucent.background, translucent.panel_surface);
            assert_eq!(translucent.sidebar, translucent.panel_surface);
            assert_eq!(translucent.chat_surface, translucent.panel_surface);
            assert_eq!(translucent.chrome_tint, translucent.panel_surface);
            assert_eq!(translucent.raised, softened(base.raised, opacity));
            assert_eq!(translucent.composer, translucent.raised);
            assert_eq!(translucent.inset, softened(base.inset, opacity));
            assert_eq!(
                translucent.terminal_surface,
                softened(base.terminal_surface, opacity)
            );

            assert_eq!(
                translucent.frame_surface, base.frame_surface,
                "the frame material is already translucent and is not faded twice"
            );
            assert_eq!(
                translucent.frame_fallback, base.frame_fallback,
                "the opaque fallback stays opaque"
            );
            assert_eq!(
                translucent.panel_border, base.panel_border,
                "borders stay crisp on a translucent panel"
            );
            assert_eq!(translucent.title, base.title, "text is untouched");

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

    #[test]
    fn ui_family_resolution_takes_jetbrains_sans_first_when_installed() {
        let installed: HashSet<String> = ["JetBrains Sans", "DejaVu Sans"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(resolve_ui_family(&installed), "JetBrains Sans");
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

    /// The terminal family prefers the Nerd Font build of JetBrains Mono
    /// (the agent TUIs' glyph set needs a Nerd Font), then the classic
    /// MesloLGS Nerd Font, then plain JetBrains Mono.
    #[test]
    fn terminal_family_resolution_prefers_nerd_fonts_in_order() {
        let installed: HashSet<String> = [
            "MesloLGS Nerd Font Mono",
            "JetBrains Mono",
            "JetBrainsMono Nerd Font",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            resolve_terminal_family(&installed),
            "JetBrainsMono Nerd Font"
        );

        let without_jetbrains_nerd: HashSet<String> = ["MesloLGS Nerd Font Mono", "JetBrains Mono"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            resolve_terminal_family(&without_jetbrains_nerd),
            "MesloLGS Nerd Font Mono"
        );

        let only_plain: HashSet<String> = ["JetBrains Mono", "DejaVu Sans Mono"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(resolve_terminal_family(&only_plain), "JetBrains Mono");
    }

    /// The terminal family falls back to the system's generic monospace
    /// answer — a family fontdb actually loaded.
    #[test]
    fn terminal_family_resolution_falls_back_to_a_family_that_exists() {
        let resolved = resolve_terminal_family(&HashSet::new());
        assert_eq!(resolved, system_monospace_family());
        assert!(!resolved.is_empty());
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

    /// The theme carries one UI-family answer too, real and non-empty
    /// before any GPUI app resolves it.
    #[test]
    fn typography_carries_a_ui_family_token() {
        let family = Theme::dark().typography.ui_family;
        assert!(!family.is_empty());
        assert_eq!(Theme::light().typography.ui_family, family);
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

    /// The compact icon-only action token (P76's cluster buttons; P75's
    /// dead-control rows are its next consumer).
    #[test]
    fn spacing_carries_compact_action() {
        assert_eq!(Spacing::default().compact_action, px(24.0));
    }

    /// The bar height and traffic-light geometry P76 measured/derived —
    /// pinned here so a future edit has to be a deliberate re-derivation,
    /// not an accidental drift.
    #[test]
    fn browser_chrome_matches_the_comet_measured_spec() {
        let chrome = BrowserChrome::default();
        assert_eq!(chrome.bar_height, px(38.0), "comet Theme::TITLEBAR_HEIGHT");
        assert_eq!(chrome.traffic_light_diameter, px(12.0));
        assert_eq!(chrome.traffic_light_gap, px(8.0));
        assert_eq!(
            chrome.traffic_light_inset,
            px(10.0),
            "comet's own non-macOS cluster_buttons_start baseline"
        );
        assert_eq!(
            chrome.macos_traffic_light_cluster_inset,
            px(80.0),
            "AppKit position 12 + 3*14pt buttons + 2*9pt gaps + 8pt separation"
        );
        assert_eq!(
            chrome.cluster_button_gap,
            px(2.0),
            "comet CLUSTER_BUTTONS_WIDTH = 24*3 + 2*2"
        );
    }

    /// The cluster-start derivation must never silently become comet's
    /// macOS 88px (calibrated to Apple's own dot size, not ours) nor its
    /// no-lights 10px (we draw lights, comet's Linux build does not).
    #[test]
    fn browser_chrome_cluster_start_is_derived_between_comets_two_reference_numbers() {
        let start = BrowserChrome::default().cluster_start();
        assert_eq!(start, px(70.0), "10 + 3*12 + 2*8 + 8 = 70");
        assert!(
            start > px(10.0),
            "we draw lights comet's Linux bar does not"
        );
        assert!(
            start < px(88.0),
            "our lights are smaller than macOS's own dot geometry"
        );
    }

    #[test]
    fn theme_carries_browser_chrome() {
        assert_eq!(Theme::dark().browser_chrome, BrowserChrome::default());
    }

    /// The caption-button spec sampled from a real Windows 11 close
    /// button (build 26200), pinned so a future edit has to be a
    /// deliberate re-measurement rather than a drift back to the value
    /// Zed happens to carry.
    #[test]
    fn windows_caption_matches_the_measured_windows_11_spec() {
        let caption = WindowsCaption::default();
        assert_eq!(caption.button_width, px(36.0));
        assert_eq!(caption.glyph_size, px(10.0));
        assert_eq!(
            caption.close_hover,
            rgb(0xC42B1C),
            "sampled under the cursor; Zed's #E81123 is the older Win32/UWP value"
        );
        assert_eq!(
            caption.close_pressed,
            rgb(0xB42A1B),
            "sampled while held -- NOT close_hover at 0.8, which reads visibly duller"
        );
        assert_eq!(caption.close_on, rgb(0xFFFFFF));
    }

    /// The close red is a system constant, so unlike every adaptive token
    /// on this theme it must resolve identically in both appearances --
    /// everything that should follow the theme comes from `icon_button`.
    #[test]
    fn windows_caption_is_the_same_in_both_appearances() {
        assert_eq!(Theme::dark().windows_caption, WindowsCaption::default());
        assert_eq!(Theme::light().windows_caption, WindowsCaption::default());
    }
}

#[cfg(test)]
mod agent_brand_tests {
    use super::*;

    /// Swift `AgentAccentColor.defaultHexByAgentId`, transcribed.
    #[test]
    fn brand_hexes_match_the_reference_table() {
        for (id, hex) in [
            ("claude", 0xD97757_u32),
            ("codex", 0x0A84FF),
            ("opencode", 0xFF9500),
            ("pi", 0x34C759),
            ("omp", 0x9B4DFF),
        ] {
            assert_eq!(
                AgentBrandColor::for_agent_id(id).hex(),
                hex,
                "{id} must wear its own brand hex"
            );
        }
    }

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

    /// Swift's `AgentAccentColor.fallbackHex` — an adapter with no default
    /// yet gets neutral grey, never a catalog agent's colour.
    #[test]
    fn unknown_agents_get_the_neutral_fallback() {
        assert_eq!(
            AgentBrandColor::for_agent_id("some-future-agent"),
            AgentBrandColor::Unknown
        );
        assert_eq!(AgentBrandColor::Unknown.hex(), 0x8E8E93);
    }

    /// The regression this type exists for. `AgentAccentColor::Amber`
    /// resolved to `theme.tab_needs_input`, so a **running** Claude worktree
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
                ("tab_needs_input", theme.tab_needs_input),
                ("tab_done", theme.tab_done),
                ("tab_error", theme.tab_error),
            ];
            for brand in brands {
                let brand_color = brand.color();
                for (name, status) in statuses {
                    assert!(
                        (brand_color.r - status.r).abs() > f32::EPSILON
                            || (brand_color.g - status.g).abs() > f32::EPSILON
                            || (brand_color.b - status.b).abs() > f32::EPSILON,
                        "{brand:?} is byte-identical to {name}: a running \
                         worktree would be indistinguishable from that status"
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

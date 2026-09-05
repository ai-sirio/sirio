//! Sirio's shared color, spacing, and typography tokens.
//!
//! Sirio keeps its chrome quiet through a compact cool-tinted shell hierarchy:
//! a translucent frame surrounds opaque panel surfaces, selected rows, and
//! shell borders. Within that hierarchy, generic hover, pressed, and divider
//! washes remain neutral veils; focus and active chrome are spelled with
//! contrast rather than hue, and semantic hues retain their existing state
//! meanings. Colour is spent on two things only: data (a diff, a git status, an
//! agent's brand) and attention (needs-input, error). The brand coral no longer
//! has a role — see [`ThemeColors::brand_coral`].
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

use gpui::{App, FontWeight, Global, Pixels, Rgba, Size, WindowAppearance, px, rgb, size};
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

pub use base_color::BaseColor;

pub use bezel::theme::appearance::AppearanceMode as ThemeMode;

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

/// All adaptive colors used by Sirio.
///
/// Names say what a value *does* in the design system (`surface_raised`,
/// `input_bg`, `overlay`, …) rather than which component consumes it. The
/// component-named layer this file used to carry alongside it — the
/// `tab_focus_accent`/`filter_field_bg` aliases — has been collapsed into the
/// roles it resolved to.
///
/// Where each value comes from — measured off a reference frame, or chosen by
/// us because no frame could settle it — is in
/// `docs/THEME-PROVENANCE.md`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColors {
    /// Translucent window-frame material. Its RGB value is paired with
    /// [`ThemeColors::bg`] for platforms without translucency.
    pub frame_surface: Rgba,
    /// Opaque fallback behind the app shell and window canvas, and behind the
    /// working columns when translucency is unavailable.
    pub bg: Rgba,
    /// Opaque reading surface inside the shell: panels, the sidebar, the tab
    /// bar, the workspace column, the right panel, settings, and the chat
    /// transcript, which reads directly on the central panel rather than on a
    /// floating card.
    pub surface: Rgba,
    /// Opaque separator between shell panels, and the seam the tab chip's
    /// underline and the sidebar draw. Opaque rather than a veil, which is
    /// what keeps it a separate token from [`ThemeColors::border`].
    pub border_opaque: Rgba,
    /// Terminal surface — the pane surface in dark mode and paper-white in
    /// light mode. Kept under its own name because the terminal renderer also
    /// uses it as the ANSI default background.
    pub terminal_surface: Rgba,
    /// Waiting-for-input status, a modified file, and the rail down a
    /// question card — the one card kind that is waiting on the reader, and
    /// so the one that keeps its colour.
    pub warning: Rgba,
    /// Completed status, and a staged file.
    pub success: Rgba,
    /// Errored status, and a conflicted file.
    pub danger: Rgba,
    /// Shared one-pixel border/divider stroke: a near-white neutral at 7-8%,
    /// so it reads as a seam rather than a line.
    pub border: Rgba,
    /// Hover fill for sidebar rows — a 6% neutral layer, not a colour.
    pub element_hover: Rgba,
    /// Primary text neutral, and the shell's whole "this is active" channel:
    /// row titles selected or not, the tab strip's underline and dirty dot, a
    /// menu's checkmark, a running worktree's badge, focused-field borders,
    /// composer body text, the insertion caret, and inline `code` glyphs. With
    /// colour reserved for data and attention, contrast is the only channel
    /// left to say "this one is active", so active chrome is the full text
    /// neutral rather than a step below it, and never a second blue.
    pub text: Rgba,
    /// Secondary row text, and the focus ring for shell panels — one step
    /// below [`ThemeColors::text`]: a focused pane has to be findable, not
    /// loud. Clears WCAG AA on the panel surface at body size.
    pub text_muted: Rgba,
    /// Raw sampled meta text, reserved for nonessential metadata and disabled
    /// labels. Body-size secondary text uses [`ThemeColors::text_muted`],
    /// which clears WCAG AA on the panel surface.
    pub text_faint: Rgba,
    /// Tree guide stroke, including its source alpha.
    pub tree_guide: Rgba,
    /// Untracked-file status color — the accent blue.
    pub git_untracked: Rgba,
    /// Addition diff accent — the success hue.
    pub diff_add: Rgba,
    /// Addition diff background — translucent success wash.
    pub diff_add_bg: Rgba,
    /// Deletion diff accent — the danger hue.
    pub diff_del: Rgba,
    /// Deletion diff background — translucent danger wash.
    pub diff_del_bg: Rgba,
    /// Clickable file-link color — the accent blue, the one place blue means
    /// "you can click this" rather than "this is a quantity".
    pub file_link: Rgba,
    // ── Role tokens: what a value does, rather than who consumes it ───────
    /// Floating cards, popovers, tooltips, the composer and primary pills: a
    /// step *above* the surface.
    pub surface_raised: Rgba,
    /// Recessed wells — filter fields, code and diff insets: a step *below*
    /// the surface. Code sits *in* the card, the inverse of a raised move.
    pub input_bg: Rgba,
    /// Generic hover wash — 5% neutral. Also the transcript row hover: a step
    /// lighter than [`ThemeColors::element_hover`], because transcript rows
    /// are wider and a 6% wash over that area reads as a block.
    pub overlay: Rgba,
    /// Pressed wash — 9% neutral, so press reads as more than hover.
    pub overlay_strong: Rgba,
    /// Stronger divider, for seams that separate rather than merely delimit,
    /// and the neutral rail down a task, edit or tool card.
    pub border_strong: Rgba,
    /// Keyboard-focus ring on a text field — bezel's `ring`, the translucent
    /// hairline every bezel input, select and control lights up with. A veil
    /// on the surface's own tone (white on dark, black on light), never the
    /// body text colour: that painted an opaque white frame on the dark theme.
    pub ring: Rgba,
    /// Faintest text step — placeholder copy and disabled labels, below
    /// [`ThemeColors::text_faint`].
    pub text_dim: Rgba,
    /// Brand coral. No role paints it any more: the shell's focus rings,
    /// caret, selection and active chrome are neutral, and every colour left in
    /// the UI is a status, a diff, a quantity or an agent's own brand.
    ///
    /// What still reads it is the `Coral` entry of the agent-colour picker,
    /// which needs a real coral to offer. Kept as a token rather than inlined
    /// as a literal there, so the picker keeps drawing from `Theme` — and so
    /// the two invariants this value carries (it clears AA on its own surface,
    /// and it is not any agent's brand) still have something to hold.
    pub brand_coral: Rgba,
    /// Quantity blue: quota meters, and the clone and update progress bars.
    /// Blue means "how much", which is why a progress bar is never painted in
    /// a status hue — a bar filling up is not an alert.
    pub accent: Rgba,
    /// Selected-row fill, and the resting fill of a control the user clicks.
    /// Must stay distinguishable from [`ThemeColors::surface_raised`]; that is
    /// the property this token exists to preserve.
    /// [`ThemeColors::selection`] remains reserved for text-selection under
    /// glyphs.
    pub element_active: Rgba,
    /// Text-selection wash, painted *under* glyphs: the top rung of the veil
    /// ladder, neutral in both appearances. Never used for row chrome — that
    /// is [`ThemeColors::element_active`].
    pub selection: Rgba,
    /// Inline `code` rounded wash, and the band under a diff hunk — the same
    /// wash, because a hunk is code too.
    pub code_wash: Rgba,
    /// Light fill for primary buttons, dark glyph on top.
    pub solid: Rgba,
    /// Glyph on primary buttons.
    pub on_solid: Rgba,
    /// Star/favorite amber.
    pub favorite: Rgba,
    /// Soft danger fill (stop button hover).
    pub danger_muted: Rgba,
}

fn bezel_theme_for(base_color: BaseColor, appearance: Appearance) -> bezel::theme::Theme {
    bezel::theme::Theme::branded(
        &bezel::theme::Brand {
            tint: base_color.tint(),
            ..Default::default()
        },
        match appearance {
            Appearance::Dark => bezel::theme::Appearance::Dark,
            Appearance::Light => bezel::theme::Appearance::Light,
        },
    )
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

    fn for_appearance(appearance: Appearance, base: BaseColor) -> Self {
        // Every neutral, status and diff token below is bezel's. What stays
        // Sirio's is listed in `Group C` of the design doc: the coral, the
        // window-frame material, the terminal surface, and the washes that sit
        // between bezel's rungs.
        //
        // The palette is read here rather than through bezel's `wash`/`ink`
        // helpers because those resolve against a process-global appearance,
        // and this function is called for both appearances in one process.
        // `branded` is the same reason the tint arrives as a parameter: a
        // brand global would put back exactly the problem those helpers have.
        let bezel = bezel_theme_for(base, appearance);
        // Sirio's brand coral. Part measured, part chosen, and the seam between
        // the two is the whole point — see `docs/THEME-PROVENANCE.md`.
        //
        // Measured: the hue, 24.3°, taken from the warm family the reference
        // frames actually render (their inline-code tone, `#E0A882`, agreeing
        // across three independent spans). Neither frame contains a coral to
        // sample directly — both show one idle chat with no logo, caret, focus
        // ring or activity dot, and a search of the whole frame finds zero
        // pixels within 37 units of any coral — so hue is as much as looking
        // can settle.
        //
        // Chosen: saturation 0.70 and lightness 0.60/0.40, against two
        // constraints rather than taste. Each variant clears WCAG AA on the
        // surface it is painted on (6.62:1 dark, 4.61:1 light — the light one
        // is the first lightness step that does), held by
        // `brand_coral_clears_contrast_on_its_own_surface`. And both stay clear of
        // every `AgentBrandColor`, held by
        // `worktree_activity_colours_name_the_agent_and_never_a_status`: a tab
        // shows its coral and its agent's mark side by side, so a coral
        // that lands on a brand makes the mark stop meaning anything. Claude's
        // `#D97757` is the near one at 22 units, which is also why the obvious
        // shortcut — reusing our own Swift's Claude fill for the coral — is
        // the one coral this app cannot have.
        let brand_coral = Self::adaptive(rgb_hex(0xE08B52), rgb_hex(0xAD581F), appearance);
        // The state hues are not a fresh design problem: Sirio already
        // shipped them. These four are the sRGB components of
        // `App/AppTheme.swift`'s `tabNeedsInput`, `tabDone`, `tabError` and
        // `tabFocusAccent`, transcribed digit for digit from our own macOS
        // app, where they mark the same four things on the same tab strip.
        // They are written as the float triples the Swift declares rather than
        // as hex so the two files can be diffed by eye.
        //
        // Nothing here could have come off the reference frames anyway: both
        // show one idle chat session — no error, no progress bar, no starred
        // row, no terminal — so there is no pixel of any of these states to
        // sample. Reusing our own is the strictly better answer than inventing
        // a second vocabulary for a meaning we had already fixed.
        //
        // `accent` is the exception worth naming: in Swift this blue is the
        // focus accent. The Rust brand colour is coral, which freed the blue, and a
        // progress bar is the one place left that wants a cool hue.
        let warning = Rgba::from(bezel.warning);
        let success = Rgba::from(bezel.success);
        let danger = Rgba::from(bezel.danger);
        let accent = Rgba::from(bezel.accent);
        let diff_add = Rgba::from(bezel.diff_add);
        let diff_del = Rgba::from(bezel.diff_del);
        // A starred row is the warning hue, not a fifth colour. Sirio used to
        // turn its own warning up to full chroma to make the star the louder
        // of the two; bezel's warning already is at full chroma, so there is
        // nothing left to turn up and the two are the same value.
        let favorite = warning;
        let frame_fallback = Rgba::from(bezel.bg);
        let frame_surface = match appearance {
            Appearance::Dark => softened(frame_fallback, 0.35),
            Appearance::Light => softened(frame_fallback, 0.30),
        };
        let panel_surface = Rgba::from(bezel.surface);
        let selected_fill = Rgba::from(bezel.element_active);
        // bezel paints body text at full contrast against its page: #E5E5E5
        // on #0D0D0D is 15.4:1, #222222 on #F4F4F4 is 14.5:1. On a surface
        // this dense — a sidebar, a tab strip and a file tree all in view —
        // that reads as glare rather than as emphasis, so Sirio pulls the
        // primary text one step back toward the surface it sits on. The
        // result lands where Sirio's own text was before it adopted bezel
        // (12.2:1 dark, 10.8:1 light), which is the target, not a taste:
        // both are comfortably past WCAG AAA's 7:1, so nothing is spent.
        //
        // Only the primary rung moves. `text_muted` and below are already
        // pulled back, and softening them too would collapse the ladder.
        let text = toward(Rgba::from(bezel.text), panel_surface, TEXT_SOFTENING);
        let text_muted = Rgba::from(bezel.text_muted);
        let text_faint = Rgba::from(bezel.text_faint);
        let text_dim = Rgba::from(bezel.text_dim);
        let raised = Rgba::from(bezel.surface_raised);
        // One step *into* the page, and derived from the measured surface for
        // the same reason `sidebar` is: a well is a relationship to the page
        // it is cut into, so it should move when the page does. The two
        // factors differ because the move is not symmetric — dark has 26 units
        // of headroom below the surface and can take a big step, light has 246
        // and would go grey long before it read as a well. Both were picked to
        // make the well legible at a glance and neither is a measurement;
        // `the_depth_ladder_reads_as_depth` holds the ordering.
        let inset = Rgba::from(bezel.input_bg);
        // A terminal shares the pane surface in dark mode so its empty area
        // cannot become a lighter grey than the pane around it. Light mode
        // keeps the paper-white terminal surface.
        let terminal_surface = Self::adaptive(panel_surface, color(1.0, 1.0, 1.0, 1.0), appearance);
        // Everything from here to `danger_soft` is a veil off the ladder — see
        // [`veil`] for why washes cannot be measured and must come from one
        // rule instead.
        let border = Rgba::from(bezel.border);
        let border_strong = Rgba::from(bezel.border_strong);
        let ring = Rgba::from(bezel.ring);
        // Measured off the seam itself, which is two frame pixels wide — one
        // logical pixel at 2x — and flat at 200/200 in both variants, so these
        // are solid values and not a blend of the surfaces either side.
        let row_hover = Rgba::from(bezel.element_hover);
        // A chat row is most of the width of the pane. The same veil a sidebar
        // row uses would read as a change of surface at that size, so the
        // large-area hover sits one rung lower.
        // bezel has no rung at 0.05 or 0.12; `wash` is its interactive-state
        // helper and takes the alpha directly, so Sirio's faint and mid rungs
        // survive as calls rather than as tokens of their own.
        let overlay = wash(VEIL_FAINT, appearance);
        let overlay_strong = wash(VEIL_MID, appearance);
        // A selection wash sits under its own text, so it has two jobs at
        // once: be visible, and not swallow the glyphs. The top rung of the
        // veil ladder is the strongest wash that still does both in either
        // appearance; `selection_stays_under_its_text` holds the second half.
        let selection = Rgba::from(bezel.selection);
        let code_wash = Rgba::from(bezel.code_wash);
        // An inverted chip — a tooltip, a keycap — is literally the other
        // appearance's page, so it is the same measured pair, swapped. No new
        // number, and it stays right by construction if either is ever
        // re-measured.
        let inverse = Rgba::from(bezel.solid);
        let on_inverse = Rgba::from(bezel.on_solid);
        let danger_soft = Rgba::from(bezel.danger_muted);

        Self {
            frame_surface,
            bg: frame_fallback,
            surface: panel_surface,
            // Collapsed onto `border`: bezel draws every seam as a hairline
            // veil, so the opaque separator Sirio used to carry has no source
            // any more. Kept as a name until phase 3 removes it, so this value
            // change does not also move five call sites.
            border_opaque: border,
            terminal_surface,
            warning,
            success,
            danger,
            border,
            element_hover: row_hover,
            text,
            text_muted,
            text_faint,
            // A guide is an edge, so it scales with the surround like every
            // other hairline rather than holding a fixed alpha.
            tree_guide: hairline(VEIL_MID, appearance),
            git_untracked: text_faint,
            // The band under a diff line is the line's own colour turned down,
            // never a second green or a second red — see [`softened`].
            diff_add,
            diff_add_bg: softened(diff_add, VEIL_MID),
            diff_del,
            diff_del_bg: softened(diff_del, VEIL_MID),
            file_link: accent,
            surface_raised: raised,
            input_bg: inset,
            overlay,
            overlay_strong,
            border_strong,
            ring,
            text_dim,
            brand_coral,
            accent,
            selection,
            element_active: selected_fill,
            code_wash,
            solid: inverse,
            on_solid: on_inverse,
            favorite,
            danger_muted: danger_soft,
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
    /// Same rule as [`Radii::default`]: the measurements do not move (T2),
    /// only their derivation, which is now bezel's four spacing steps.
    fn default() -> Self {
        use bezel::theme::Theme as BezelTheme;
        Self {
            shell_gap: px(BezelTheme::SPACE_XS),                     // 4.0
            shell_outer_inset: px(BezelTheme::SPACE_XS),             // 4.0
            card_corner_radius: px(BezelTheme::SPACE_MD * 0.5),      // 6.0
            card_gap: px(BezelTheme::SPACE_MD * 0.833_333_3),        // 10.0
            card_shadow_radius: px(BezelTheme::SPACE_LG * 1.125),    // 18.0
            card_shadow_y_offset: px(BezelTheme::SPACE_MD * 0.5),    // 6.0
            title_strip_height: px(BezelTheme::SPACE_LG * 3.0),      // 48.0
            traffic_light_inset: px(BezelTheme::SPACE_LG * 0.875),   // 14.0
            title_strip_icon_size: px(BezelTheme::SPACE_LG * 0.875), // 14.0
            titlebar_control_frame: size(
                px(BezelTheme::SPACE_LG * 1.625),
                px(BezelTheme::SPACE_LG * 1.625),
            ), // 26.0 square
            titlebar_control_spacing: px(BezelTheme::SPACE_MD * 0.5), // 6.0
            bottom_bar_height: px(BezelTheme::SPACE_LG * 2.5),       // 40.0
            menu_width: px(BezelTheme::SPACE_LG * 15.0),             // 240.0
            hairline_thickness: px(BezelTheme::SPACE_XS * 0.25),     // 1.0
            compact_action: px(BezelTheme::SPACE_LG * 1.5),          // 24.0
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
    /// The values are unchanged (spec decision T2); what changes is that they
    /// now follow bezel's `Brand::radius` instead of standing alone. Ratios
    /// that do not land on one of bezel's five named corners carry an explicit
    /// multiplier rather than being rounded to the nearest named one —
    /// rounding would move the UI, which T2 forbids.
    fn default() -> Self {
        use bezel::theme::Theme as BezelTheme;
        Self {
            shell_panel: px(BezelTheme::BASE_RADIUS * 0.875), // 7.0
            chip: px(BezelTheme::BASE_RADIUS * 0.5),          // 4.0
            chip_active: px(BezelTheme::BASE_RADIUS * 0.625), // 5.0
            control: px(BezelTheme::BASE_RADIUS * 0.75),      // 6.0
            row_card: px(BezelTheme::BASE_RADIUS * 0.875),    // 7.0
            code_block: px(BezelTheme::BASE_RADIUS),          // 8.0
            toast: px(BezelTheme::BASE_RADIUS * 1.25),        // 10.0
            user_pill: px(BezelTheme::BASE_RADIUS * 1.5),     // 12.0
            composer: px(BezelTheme::BASE_RADIUS * 1.625),    // 13.0
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
// lead everywhere, registered by `bezel::ui::register_fonts` before the first
// window opens, so they are always present. A theme that changes face per
// platform cannot be reviewed as one design, which is why the Apple faces sit
// at the tail as a last resort rather than leading on macOS. The JetBrains
// faces (JetBrains Mono — SIL OFL 1.1; JetBrains Sans — Apache 2.0 — both open
// source) follow as the fontconfig-detected fallback. Each list was verified
// against
// `fc-list : family` on the Linux build machine rather than assumed.

/// The sans-serif families to prefer, in order, when resolving the UI font.
///
/// "Geist" leads on every platform, macOS included: bezel registers it, with
/// real 500/600/700 statics, so it is always present here — the chain behind
/// it only matters if that registration is ever skipped. Apple's SF faces are
/// kept at the tail as a last resort rather than led with, because a theme
/// that changes face per platform cannot be reviewed as one design.
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
/// [`UI_FAMILY_CANDIDATES`]: bezel registers it before the first window
/// opens. Behind it, the family the visual bar is set in (waku's JetBrains
/// Mono), then common good monospaced faces, then whatever the system's
/// generic "monospace" resolves to (fontconfig's alias on Linux, always
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
    pub fn graph_lane(&self, index: usize) -> Rgba {
        let lanes = [
            self.text,
            self.git_untracked,
            self.success,
            self.warning,
            self.danger,
            // Not `favorite`: it is `warning`, which is already lane four.
            // The coral is the one hue in the theme no other lane can collide
            // with, because no role paints it.
            self.brand_coral,
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
        bezel::theme::set_current_appearance(match self.appearance {
            Appearance::Light => bezel::theme::Appearance::Light,
            Appearance::Dark => bezel::theme::Appearance::Dark,
        });
    }

    /// The bezel theme this Sirio theme is derived from — the same
    /// `Theme::branded` call used to build Sirio's adaptive colors.
    pub fn to_bezel_theme(&self) -> bezel::theme::Theme {
        bezel_theme_for(self.base_color, self.appearance)
    }

    /// Installs this theme's branded palette into bezel's registry and keeps
    /// bezel's context-free appearance mirror in sync.
    pub fn install_into_bezel(&self, cx: &mut bezel::gpui::App) {
        bezel::theme::Theme::install_custom(self.to_bezel_theme(), cx);
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
                    let mut next = Theme::for_appearance(
                        ThemeMode::System,
                        preference,
                        current.base_color,
                    );
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
    pub fn for_mode(
        mode: ThemeMode,
        system_appearance: WindowAppearance,
        base: BaseColor,
    ) -> Self {
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
        theme.typography = Typography::for_interface_size(value.clamp(10, 20) as f32);
        cx.set_global(theme);
    }

    /// Returns the surface opacity used when translucency is enabled.
    ///
    /// The fade is 0.45. The earlier steps (0.96 → 0.85 → 0.70) all read as
    /// opaque in practice because the layers *stack*: a terminal pane paints
    /// terminal_surface over the panel's surface over the frame material, so
    /// at 0.70 the composite still covered ~97% of the backdrop — grey, not
    /// glass. At 0.45 over the 0.35/0.30 frame the composite lets roughly a
    /// third of the blurred desktop through in panel areas (a fifth where a
    /// third layer stacks), which finally reads as glass. Legibility holds
    /// because the backdrop is blurred: text sits on an averaged tone rather
    /// than raw desktop pixels.
    pub fn surface_opacity(translucency_enabled: bool) -> f32 {
        if translucency_enabled { 0.45 } else { 1.0 }
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
        // Scale the alpha, do not overwrite it. These were all opaque once, so
        // the two were the same thing; bezel's dark `input_bg` is a 3% white
        // veil, and overwriting turned it into an 85% white fill.
        let fade = |surface: Rgba| Rgba {
            a: surface.a * opacity,
            ..surface
        };
        theme.colors.surface = fade(theme.colors.surface);
        theme.colors.surface_raised = fade(theme.colors.surface_raised);
        theme.colors.input_bg = fade(theme.colors.input_bg);
        theme.colors.terminal_surface = fade(theme.colors.terminal_surface);
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

/// bezel's interactive-state wash at `alpha`, for a stated appearance.
///
/// A mirror of `bezel::wash`, which exists only in the form that resolves
/// against a process-global appearance (`bezel::paint::wash_for` is
/// `pub(crate)`). `ThemeColors::for_appearance` builds both palettes in one
/// process, so it cannot use the global form: dark and light would come out
/// identical. The numbers are bezel's, copied — if bezel changes them this
/// mirror has to follow, which is what `washes_follow_bezels_two_rules` holds.
fn wash(alpha: f32, appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => color(0.92, 0.92, 0.92, alpha),
        Appearance::Light => color(0.10, 0.10, 0.10, alpha * bezel::theme::INK_FILL_SCALE),
    }
}

/// bezel's hairline ink at `alpha`, for a stated appearance. Mirror of
/// `bezel::hairline`, for the same reason [`wash`] is a mirror.
///
/// Edges scale opposite to fills: a 1px line needs *more* ink on a bright
/// surround, which is what [`bezel::theme::INK_HAIRLINE_SCALE`] carries.
fn hairline(alpha: f32, appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => color(1.0, 1.0, 1.0, alpha),
        Appearance::Light => color(
            0.0,
            0.0,
            0.0,
            (alpha * bezel::theme::INK_HAIRLINE_SCALE).min(0.5),
        ),
    }
}

/// The two alphas Sirio still chooses for itself.
///
/// This was a four-rung ladder (0.05 · 0.08 · 0.12 · 0.18), each rung half
/// again the one below, because every wash token in the theme was `veil(rung)`
/// and the ladder was the only free parameter in the lot. bezel now supplies
/// the borders, hovers, selection and code wash, so two rungs have no consumer
/// and the "ladder" no longer describes anything: what is left is the faint
/// wash and the mid wash, quoted in dark-mode terms the way bezel quotes its
/// own.
/// How far the primary text rung is pulled back toward its surface. Chosen so
/// the result lands on the contrast Sirio shipped before adopting bezel; held
/// by `body_text_is_softened_off_bezels_full_contrast`.
const TEXT_SOFTENING: f32 = 0.10;

const VEIL_FAINT: f32 = 0.05;
const VEIL_MID: f32 = 0.12;

/// The same colour at a lower opacity.
///
/// Used for the soft fills that sit *under* text of the same meaning — a
/// deleted diff line under red text, a danger banner under a danger label. The
/// fill is not a second red to choose; it is the one red already chosen,
/// turned down.
fn softened(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

/// Mixes `fraction` of `target` into `color`, opaquely.
///
/// Distinct from [`softened`], which lowers alpha and lets whatever is behind
/// show through: this states one opaque colour as a step from another toward a
/// named second one, so the result does not depend on what it is drawn over.
fn toward(color: Rgba, target: Rgba, fraction: f32) -> Rgba {
    Rgba {
        r: color.r + (target.r - color.r) * fraction,
        g: color.g + (target.g - color.g) * fraction,
        b: color.b + (target.b - color.b) * fraction,
        a: color.a,
    }
}

/// HSL hue in degrees, and HSL lightness in 0..1, of an sRGB colour.
///
/// The inverse of [`hsla`], and only used to state one token as a
/// transformation of another rather than as a fresh number.
#[cfg(test)]
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
#[cfg(test)]
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

    /// Installing a theme pushes its appearance into bezel's mirror.
    ///
    /// bezel's `ink`, `wash` and `hairline` are free functions with no `cx`:
    /// they read a process-wide mirror that defaults to Dark, not the theme
    /// they are painting for. Nothing else in the suite would notice a stale
    /// mirror, because every token this crate builds resolves its own
    /// appearance explicitly — the damage is confined to the bezel primitives
    /// `sirio_ui` renders, and it looks like a light window with dark
    /// hairlines.
    #[test]
    fn installing_a_theme_syncs_bezels_appearance_mirror() {
        // The mirror is process-wide, so two tests touching it concurrently
        // would flake. This is bezel's own guard for exactly that.
        let _guard = bezel::theme::lock_appearance();

        Theme::light().sync_appearance();
        assert_eq!(
            bezel::theme::current_appearance(),
            bezel::theme::Appearance::Light
        );

        Theme::dark().sync_appearance();
        assert_eq!(
            bezel::theme::current_appearance(),
            bezel::theme::Appearance::Dark
        );
    }

    /// The dark palette is bezel's.
    ///
    /// Sirio's dark palette used to be sampled off reference frames and pinned
    /// hex by hex against a provenance document that has since been deleted.
    /// It is now
    /// bezel's, so what is worth pinning is the *link*, not the values: this
    /// test fails the moment a `bezel` bump restyles the app, which is spec
    /// risk R1 and the reason the dependency is pinned `=0.1.3`. The record of
    /// where the values come from now lives in `docs/THEME-PROVENANCE.md`.
    #[test]
    fn dark_palette_comes_from_bezel() {
        assert_palette_comes_from_bezel(Appearance::Dark, bezel::theme::Theme::dark());
    }

    /// The light palette is bezel's, for the same reason.
    #[test]
    fn light_palette_comes_from_bezel() {
        assert_palette_comes_from_bezel(Appearance::Light, bezel::theme::Theme::light());
    }

    /// Every token Sirio takes from bezel, checked against bezel itself.
    ///
    /// Group C — the coral, the window-frame material and the terminal surface
    /// — is deliberately absent: those are Sirio's own and have no bezel
    /// counterpart to compare against.
    fn assert_palette_comes_from_bezel(appearance: Appearance, bezel: bezel::theme::Theme) {
        let sirio = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
        for (name, ours, theirs) in [
            ("bg", sirio.bg, bezel.bg),
            ("surface", sirio.surface, bezel.surface),
            ("surface_raised", sirio.surface_raised, bezel.surface_raised),
            ("input_bg", sirio.input_bg, bezel.input_bg),
            ("element_active", sirio.element_active, bezel.element_active),
            ("element_hover", sirio.element_hover, bezel.element_hover),
            // `text` is deliberately absent: it is bezel's, pulled one step
            // back toward the surface. `body_text_is_softened_off_bezels_full_contrast`
            // is its guard.
            ("text_muted", sirio.text_muted, bezel.text_muted),
            ("text_faint", sirio.text_faint, bezel.text_faint),
            ("text_dim", sirio.text_dim, bezel.text_dim),
            ("border", sirio.border, bezel.border),
            ("border_strong", sirio.border_strong, bezel.border_strong),
            ("ring", sirio.ring, bezel.ring),
            ("selection", sirio.selection, bezel.selection),
            ("code_wash", sirio.code_wash, bezel.code_wash),
            ("solid", sirio.solid, bezel.solid),
            ("on_solid", sirio.on_solid, bezel.on_solid),
            ("accent", sirio.accent, bezel.accent),
            ("success", sirio.success, bezel.success),
            ("warning", sirio.warning, bezel.warning),
            ("danger", sirio.danger, bezel.danger),
            ("danger_muted", sirio.danger_muted, bezel.danger_muted),
            ("diff_add", sirio.diff_add, bezel.diff_add),
            ("diff_del", sirio.diff_del, bezel.diff_del),
        ] {
            assert_eq!(
                ours,
                Rgba::from(theirs),
                "{appearance:?} {name} is not bezel's any more"
            );
        }
    }

    #[test]
    fn neutral_reproduces_the_palette_shipped_before_base_colours() {
        // The upgrade guard. `Tint::NONE` is bezel's shipped grey, so an
        // install that has never touched the new setting must paint exactly
        // what it painted yesterday — every token, both appearances.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let bezel = match appearance {
                Appearance::Dark => bezel::theme::Theme::dark(),
                Appearance::Light => bezel::theme::Theme::light(),
            };
            assert_eq!(
                neutral.bg,
                Rgba::from(bezel.bg),
                "{appearance:?} bg is bezel's untinted page"
            );
            assert_eq!(neutral.surface, Rgba::from(bezel.surface));
            assert_eq!(neutral.border, Rgba::from(bezel.border));
        }
    }

    #[test]
    fn a_tinted_base_moves_the_greys_and_leaves_sirios_own_colours_alone() {
        // Decision B4: the coral is Sirio's identity, anchored by two
        // measured constraints. The dark terminal now follows the pane's
        // tinted surface; the light terminal intentionally remains paper.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let slate = ThemeColors::for_appearance(appearance, BaseColor::Slate);

            assert_ne!(slate.bg, neutral.bg, "{appearance:?} page takes the tint");
            assert_ne!(slate.surface, neutral.surface);
            // The borders stay verbatim, on purpose: bezel 0.1.3 tints only
            // opaque achromatic ink — `Brand::apply`: "Translucent ink is
            // skipped because it paints over whatever is beneath it, which is
            // tinted already" — and both borders are 8–10% veils. They read
            // the page's tint through compositing, which is also exactly what
            // keeps `assert_palette_comes_from_bezel` true under every base.
            assert_eq!(slate.border, neutral.border);

            assert_eq!(
                slate.brand_coral, neutral.brand_coral,
                "{appearance:?} coral is Sirio's, not bezel's to rotate"
            );
            if appearance == Appearance::Light {
                assert_eq!(
                    slate.terminal_surface, neutral.terminal_surface,
                    "light terminal remains paper despite the pane tint"
                );
            }
        }
    }

    #[test]
    fn dark_terminal_surface_matches_the_pane_surface() {
        for base in BaseColor::ALL {
            let theme = Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, base);

            assert_eq!(
                theme.terminal_surface, theme.surface,
                "dark terminal background must match the pane for {base:?}"
            );
        }
    }

    #[test]
    fn the_semantic_hues_hold_under_every_base_colour() {
        // `Brand::apply` keeps danger, warning and success where they are
        // because they mean something. Asserted from Sirio's side too, so a
        // bezel bump that changed the rule fails here rather than shipping a
        // status colour nobody chose.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let neutral = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            for base in BaseColor::ALL {
                let tinted = ThemeColors::for_appearance(appearance, base);
                assert_eq!(tinted.danger, neutral.danger, "{base:?} danger");
                assert_eq!(tinted.warning, neutral.warning, "{base:?} warning");
                assert_eq!(tinted.success, neutral.success, "{base:?} success");
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
            translucent.colors.border,
            theme.colors.border,
            "a non-faded token keeps the tinted value"
        );
    }

    #[test]
    fn the_base_colour_defaults_to_neutral_when_nothing_is_installed() {
        // A pure test of the recovery rule; no gpui context involved.
        assert_eq!(BaseColor::default(), BaseColor::Neutral);
        assert_eq!(Theme::dark().base_color, BaseColor::Neutral);
    }

    #[test]
    fn a_reinstall_keeps_the_base_colour_the_way_it_keeps_translucency() {
        // `install` recovers both from the previously installed theme. This
        // is the pure half of that contract: rebuilding for a new mode from
        // an existing theme's fields must carry the choice across.
        let installed =
            Theme::for_appearance(ThemeMode::Dark, Appearance::Dark, BaseColor::Zinc);
        let next = Theme::for_appearance(
            ThemeMode::Light,
            Appearance::Light,
            installed.base_color,
        );
        assert_eq!(next.base_color, BaseColor::Zinc);
        assert_ne!(
            next.colors.bg,
            ThemeColors::for_appearance(Appearance::Light, BaseColor::Neutral).bg,
            "the light rebuild is still tinted"
        );
    }

    /// The primary text rung is bezel's, softened — not bezel's as-is, and not
    /// a hand-picked hex either.
    ///
    /// bezel paints body text at full contrast against its page. Sirio's
    /// surfaces carry more text per screen (a sidebar, a tab strip and a file
    /// tree at once), where that reads as glare. The target is the contrast
    /// Sirio shipped before adopting bezel, so this pins the *relationship* —
    /// softened toward the surface, still past AAA, still clearly ahead of
    /// `text_muted`.
    #[test]
    fn body_text_is_softened_off_bezels_full_contrast() {
        for (appearance, bezel) in [
            (Appearance::Dark, bezel::theme::Theme::dark()),
            (Appearance::Light, bezel::theme::Theme::light()),
        ] {
            let sirio = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            let full = Rgba::from(bezel.text);

            assert_ne!(
                sirio.text, full,
                "{appearance:?} text must not be bezel's full-contrast rung"
            );
            assert_eq!(
                sirio.text,
                toward(full, sirio.surface, TEXT_SOFTENING),
                "{appearance:?} text must be bezel's, softened toward the surface"
            );

            // Softer than bezel, but not into the muted rung's territory: the
            // ladder still has a visible first step.
            let step = contrast_ratio(sirio.text, sirio.surface);
            assert!(
                step > contrast_ratio(sirio.text_muted, sirio.surface),
                "{appearance:?} text ({step:.1}:1) must stay ahead of text_muted"
            );
            assert!(
                step > 7.0,
                "{appearance:?} text is {step:.1}:1 — softening must not spend WCAG AAA"
            );
            assert!(
                step < contrast_ratio(full, sirio.surface),
                "{appearance:?} text must be softer than bezel's"
            );
        }
    }

    #[test]
    fn radii_are_ratios_of_bezel_base_radius() {
        // T2: the values do not move, but they stop being independent
        // constants. bezel's `Brand::radius` moves the whole ladder together;
        // a literal cannot follow it. `code_block` and `user_pill` are the
        // anchors because they already sit exactly on two named corners.
        use bezel::theme::Theme as BezelTheme;
        let radii = Radii::default();
        assert_eq!(radii.code_block, px(BezelTheme::button_radius()));
        assert_eq!(radii.user_pill, px(BezelTheme::surface_radius()));
    }

    #[test]
    fn washes_follow_bezels_two_rules() {
        // `wash` and `hairline` are hand-copied from bezel because the
        // appearance-taking forms there are `pub(crate)`. This is the guard on
        // that copy: the alpha scaling comes from bezel's own constants, and
        // the two rules stay opposite — a fill is not scaled up on light, an
        // edge is.
        let a = VEIL_MID;
        assert_eq!(
            wash(a, Appearance::Light).a,
            a * bezel::theme::INK_FILL_SCALE
        );
        assert_eq!(
            hairline(a, Appearance::Light).a,
            (a * bezel::theme::INK_HAIRLINE_SCALE).min(0.5)
        );
        assert!(
            hairline(a, Appearance::Light).a > wash(a, Appearance::Light).a,
            "an edge must carry more ink than a fill on a bright surround"
        );
        // Dark quotes the alpha as given, in both rules.
        assert_eq!(wash(a, Appearance::Dark).a, a);
        assert_eq!(hairline(a, Appearance::Dark).a, a);
    }

    #[test]
    fn theme_colours_come_from_bezel() {
        // The swap's defining property: Sirio's neutrals are bezel's, not a
        // copy that happens to agree today. Read through `Deref`, which is what
        // every call site uses. `text_muted` stands in for the text ladder
        // here because the primary rung is softened —
        // `body_text_is_softened_off_bezels_full_contrast` covers that one.
        for (appearance, bezel) in [
            (Appearance::Dark, bezel::theme::Theme::dark()),
            (Appearance::Light, bezel::theme::Theme::light()),
        ] {
            let sirio = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            assert_eq!(sirio.surface, Rgba::from(bezel.surface));
            assert_eq!(sirio.text_muted, Rgba::from(bezel.text_muted));
            assert_eq!(sirio.border, Rgba::from(bezel.border));
        }
    }

    #[test]
    fn git_untracked_is_a_neutral_not_the_accent() {
        // C1: untracked leaves the blue. It is quieter than the chromatic
        // staged / modified / conflict states, so it reads as "not yet tracked"
        // rather than as a status.
        for appearance in [Appearance::Dark, Appearance::Light] {
            let c = ThemeColors::for_appearance(appearance, BaseColor::Neutral);
            assert_eq!(c.git_untracked, c.text_faint);
            assert_ne!(c.git_untracked, c.accent);
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
    fn shell_body_text_meets_wcag_aa_on_its_panel() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            for (role, text) in [("primary", theme.text), ("secondary", theme.text_muted)] {
                let ratio = contrast_ratio(text, theme.surface);
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
    fn composite(over: Rgba, under: Rgba) -> Rgba {
        let a = over.a;
        Rgba {
            r: over.r * a + under.r * (1.0 - a),
            g: over.g * a + under.g * (1.0 - a),
            b: over.b * a + under.b * (1.0 - a),
            a: 1.0,
        }
    }

    /// The mid wash is still half again the faint one.
    ///
    /// Two rungs is what is left of the four-rung ladder; the relationship
    /// between them is the whole of the arbitrariness Sirio still owns.
    #[test]
    fn the_two_remaining_rungs_keep_their_step() {
        let ratio = VEIL_MID / VEIL_FAINT;
        assert!(
            (2.2..=2.6).contains(&ratio),
            "{VEIL_FAINT} -> {VEIL_MID} is a {ratio:.2}x step"
        );
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
                ("border", theme.border),
                ("element_hover", theme.element_hover),
                ("overlay", theme.overlay),
                ("overlay_strong", theme.overlay_strong),
                ("tree_guide", theme.tree_guide),
                ("code_wash", theme.code_wash),
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
            // `danger_muted` is bezel's and is a *lighter* danger rather than
            // a turned-down one, so it is checked by hue below instead.
            let (muted_hue, _) = hue_and_lightness(theme.danger_muted);
            let (danger_hue, _) = hue_and_lightness(theme.danger);
            assert!(
                (muted_hue - danger_hue).abs() < 20.0,
                "{label}: danger_muted left danger's hue ({muted_hue} vs {danger_hue})"
            );
            for (name, fill, parent) in [
                ("diff_del_bg", theme.diff_del_bg, theme.diff_del),
                ("diff_add_bg", theme.diff_add_bg, theme.diff_add),
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
            let page = composite(theme.input_bg, theme.surface);
            let card = composite(theme.input_bg, theme.surface_raised);
            assert_ne!(
                page, theme.surface,
                "{label}: the well vanishes into the page"
            );
            assert_ne!(
                card, theme.surface_raised,
                "{label}: the well vanishes into a card"
            );
            assert_ne!(
                theme.surface, theme.surface_raised,
                "{label}: the page and a raised card are the same plane"
            );
        }
    }

    /// The star is a louder `warning`, not a fifth colour.
    #[test]
    fn favorite_is_the_warning_hue_at_full_chroma() {
        for (label, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
            let (star_hue, star_light) = hue_and_lightness(theme.favorite);
            let (warn_hue, warn_light) = hue_and_lightness(theme.warning);
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
                chroma(theme.favorite) >= chroma(theme.warning),
                "{label}: the star is not the louder of the two"
            );
            // bezel's warning is already full chroma, so "louder" resolves to
            // "the same". What still has to hold is that a starred row is not
            // a colour of its own.
            assert_eq!(theme.favorite, theme.warning);
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
            let ratio = contrast_ratio(theme.on_solid, theme.solid);
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
            let seen = composite(theme.selection, theme.surface);
            let ratio = contrast_ratio(theme.text, seen);
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
            let ratio = contrast_ratio(theme.brand_coral, theme.surface);
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
                    theme.brand_coral,
                    brand.color(),
                    "{label}: the coral is {brand:?}'s brand, so that agent's mark \
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

    #[test]
    fn every_adaptive_token_differs_between_light_and_dark() {
        let light = Theme::light().colors;
        let dark = Theme::dark().colors;
        let tokens = [
            ("frame_surface", light.frame_surface, dark.frame_surface),
            ("bg", light.bg, dark.bg),
            ("surface", light.surface, dark.surface),
            ("border_opaque", light.border_opaque, dark.border_opaque),
            (
                "terminal_surface",
                light.terminal_surface,
                dark.terminal_surface,
            ),
            ("warning", light.warning, dark.warning),
            ("success", light.success, dark.success),
            ("danger", light.danger, dark.danger),
            ("border", light.border, dark.border),
            ("element_hover", light.element_hover, dark.element_hover),
            ("element_active", light.element_active, dark.element_active),
            ("selection", light.selection, dark.selection),
            ("text", light.text, dark.text),
            ("text_muted", light.text_muted, dark.text_muted),
            ("text_faint", light.text_faint, dark.text_faint),
            ("tree_guide", light.tree_guide, dark.tree_guide),
            ("git_untracked", light.git_untracked, dark.git_untracked),
            ("diff_add", light.diff_add, dark.diff_add),
            ("diff_add_bg", light.diff_add_bg, dark.diff_add_bg),
            ("diff_del", light.diff_del, dark.diff_del),
            ("diff_del_bg", light.diff_del_bg, dark.diff_del_bg),
            ("file_link", light.file_link, dark.file_link),
            ("surface_raised", light.surface_raised, dark.surface_raised),
            ("input_bg", light.input_bg, dark.input_bg),
            ("overlay", light.overlay, dark.overlay),
            ("overlay_strong", light.overlay_strong, dark.overlay_strong),
            ("border_strong", light.border_strong, dark.border_strong),
            ("ring", light.ring, dark.ring),
            ("text_dim", light.text_dim, dark.text_dim),
            ("brand_coral", light.brand_coral, dark.brand_coral),
            ("accent", light.accent, dark.accent),
            ("selection", light.selection, dark.selection),
            ("code_wash", light.code_wash, dark.code_wash),
            ("solid", light.solid, dark.solid),
            ("on_solid", light.on_solid, dark.on_solid),
            ("favorite", light.favorite, dark.favorite),
            ("danger_muted", light.danger_muted, dark.danger_muted),
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
        assert_eq!(Theme::dark().translucent_surface_opacity, 0.45);
        assert_eq!(Theme::surface_opacity(true), 0.45);
        assert_eq!(Theme::surface_opacity(false), 1.0);
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
        let base = Theme::dark();
        assert!(
            base.input_bg.a < 0.5,
            "precondition: dark input_bg is a veil, not a fill (got {})",
            base.input_bg.a
        );

        let translucent = base.with_translucency(true);
        assert!(
            translucent.input_bg.a <= base.input_bg.a,
            "fading made the veil more opaque: {} -> {}",
            base.input_bg.a,
            translucent.input_bg.a
        );
    }

    #[test]
    fn with_translucency_at_fades_to_the_given_opacity_and_remembers_it() {
        for base in [Theme::dark(), Theme::light()] {
            let subtle = base.with_translucency_at(true, 0.9);
            assert!(subtle.translucency_enabled);
            assert_eq!(subtle.translucent_surface_opacity, 0.9);
            assert!((subtle.surface.a - base.surface.a * 0.9).abs() < 1e-6);
            assert!((subtle.terminal_surface.a - base.terminal_surface.a * 0.9).abs() < 1e-6);
            // Re-deriving from the faded theme keeps the caller's opacity, so
            // a mode switch that rebuilds the palette cannot fall back to the
            // default fade.
            let again = subtle.with_translucency_at(
                subtle.translucency_enabled,
                subtle.translucent_surface_opacity,
            );
            assert_eq!(again.surface, subtle.surface);
            let opaque = subtle.with_translucency_at(false, 0.9);
            assert!(!opaque.translucency_enabled);
            assert_eq!(opaque.surface, base.surface);
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
            let faded = |s: Rgba| Rgba { a: s.a * opacity, ..s };
            assert_eq!(translucent.surface, faded(base.surface));
            assert_eq!(translucent.surface_raised, faded(base.surface_raised));
            assert_eq!(translucent.input_bg, faded(base.input_bg));
            assert_eq!(translucent.terminal_surface, faded(base.terminal_surface));

            assert_eq!(
                translucent.frame_surface, base.frame_surface,
                "the frame material is already translucent and is not faded twice"
            );
            assert_eq!(translucent.bg, base.bg, "the opaque fallback stays opaque");
            assert_eq!(
                translucent.border_opaque, base.border_opaque,
                "borders stay crisp on a translucent panel"
            );
            assert_eq!(translucent.text, base.text, "text is untouched");

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

    /// B3 makes the bundled face the face everywhere: bezel ships Geist with
    /// real 500/600/700 statics, which the cosmic-text path needs because it
    /// rasterizes a variable font at its default instance only and never
    /// applies `wght` coordinates.
    #[test]
    fn geist_leads_on_every_platform() {
        assert_eq!(UI_FAMILY_CANDIDATES[0], "Geist");
        assert_eq!(CODE_FAMILY_CANDIDATES[0], "Geist Mono");
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

    /// `Theme::for_mode` must resolve for the mode it was asked for, not a
    /// stale or independently-defaulted one — `System` under a light window
    /// appearance is a light theme.
    ///
    /// This used to check the same thing through the COSMIC sub-theme's
    /// `is_dark`, which was the seam that made `Theme` the single source
    /// those tokens flowed through. There is no sub-theme any more; the
    /// appearance is the theme's own field, so this is what is left to
    /// assert.
    #[test]
    fn theme_for_mode_resolves_for_the_appearance_it_was_asked_for() {
        let theme = Theme::for_mode(ThemeMode::Light, WindowAppearance::Light, BaseColor::Neutral);
        assert_eq!(theme.appearance, Appearance::Light);

        let theme = Theme::for_mode(ThemeMode::Dark, WindowAppearance::Dark, BaseColor::Neutral);
        assert_eq!(theme.appearance, Appearance::Dark);
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
                ("warning", theme.warning),
                ("success", theme.success),
                ("danger", theme.danger),
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

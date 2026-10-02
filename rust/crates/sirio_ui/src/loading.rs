//! The one place Bezel loading primitives are called from.
//!
//! OWNERSHIP: this module renders; it owns no business state. Every surface
//! keeps its own task/status state machine and asks here only for a picture of
//! it. Bezel imports live here and nowhere else, so a Bezel API change is a
//! one-file change.
//!
//! A note on the `id` every entry point takes: Bezel 0.1.3 currently discards
//! it (`loaders.rs` binds `let _key`, `popover.rs` takes `_id`), so today it
//! keys nothing and a collision would be symptomless. Pass a unique one anyway
//! — it is the identity these primitives are documented to take, and the day
//! Bezel starts honouring it, duplicates would become state bleeding between
//! elements with no compile error to warn you.

use std::time::Duration;

use bezel::motion;
use bezel::motion::Painter;
use bezel::ui::loaders;
use bezel::ui::popover;
use bezel::ui::widgets::Controls;
use gpui::{
    AnyElement, App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window,
    div, px,
};
use sirio_theme::Theme;

/// The glyph slot in an Activity-derived reasoning header.
pub const THINKING_GLYPH: f32 = 14.0;
/// The generic orb for a full-surface first load or empty state.
pub const GENERIC_ORB: f32 = 44.0;
/// The status bloom that stands in for a sidebar row's status word.
pub const BLOOM_GLYPH: f32 = 14.0;
/// A bloom ring's border as a fraction of the box — Bezel's own figure, from
/// the `Orb::Bloom` arm of `loaders::orb`.
pub const BLOOM_BORDER_RATIO: f32 = 0.05;
/// A settled bloom's ring opacities, outermost first.
///
/// A travelling bloom fades each ring out as it leaves the centre
/// (`orb_bloom_opacity`, `(1 - phase)²`), so the frame where a ring reaches
/// the edge is exactly the frame where it has become invisible. Freezing one
/// "at full" therefore means overriding that curve rather than sampling it:
/// bright at the rim and dimming inward reads as arrived and stopped, which
/// is the opposite of the travelling shape and the point of the distinction.
pub const SETTLED_BLOOM_OPACITIES: [f32; 3] = [1.0, 0.55, 0.25];
const _: () = assert!(SETTLED_BLOOM_OPACITIES.len() == motion::ORB_BLOOM_RINGS);
/// The cell size of the compact refresh spinner.
pub const COMPACT_MINI_CELL: f32 = 2.5;
/// Determinate progress: track thickness and the width the gallery demos.
pub const PROGRESS_TRACK: f32 = 4.0;
pub const PROGRESS_MAX_WIDTH: f32 = 280.0;
/// A subordinate skeleton: three rows, never the sole activity signal. Its
/// geometry belongs to Bezel's `popover::redacted_rows`: 28px rows, 6px gaps,
/// and 4px vertical padding.
pub const SKELETON_ROWS: usize = 3;

/// The label for a settled reasoning header.
///
/// `None` means Sirio has no truthful duration for the run, and the label says
/// so by omission rather than inventing an elapsed time.
pub fn thought_label(elapsed: Option<Duration>) -> String {
    match elapsed {
        Some(elapsed) => format!("Thought for {:.0}s", elapsed.as_secs_f32()),
        None => "Thought".to_string(),
    }
}

/// Clamp a caller's fraction into the unit range. A determinate bar shows a
/// truthful fraction of a known total; a caller that overshoots is pinned, not
/// panicked. `NaN` means the fraction is unknown, so it renders as empty.
pub fn clamp_fraction(fraction: f32) -> f32 {
    if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    }
}

/// Build the Bezel palette expected by its published UI primitives while
/// retaining Sirio's appearance and accent. Bezel's loading APIs use their own
/// theme type, whereas the adapter's public contract intentionally exposes
/// Sirio's theme type to its callers.
///
/// This module used to push Sirio's appearance into Bezel's process-wide
/// paint mirror on every call, because `ink`, `wash` and `hairline` are free
/// functions with no `cx` and would otherwise paint for the previous
/// appearance. `Theme::sync_appearance` now does it wherever the theme global
/// is written, which is the one place that can be sure it happened.
fn bezel_theme(theme: &Theme) -> bezel::theme::Theme {
    let mut bezel_theme = match theme.appearance {
        sirio_theme::Appearance::Light => bezel::theme::Theme::light(),
        sirio_theme::Appearance::Dark => bezel::theme::Theme::dark(),
    };
    bezel_theme.accent = theme.sirio.brand_coral;
    bezel_theme
}

/// The neutral gray the thinking and loading orbs paint in.
fn loading_accent(theme: &Theme) -> Hsla {
    theme.ely.fg_muted
}

/// The view whose render is leasing the shared Bezel animation clock.
fn painter(window: &Window) -> Painter {
    Painter::from(window.current_view())
}

/// An Activity-derived reasoning indicator using Bezel's Cluster orb.
pub fn thinking_indicator(
    id: &'static str,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut bezel_theme = bezel_theme(theme);
    bezel_theme.accent = loading_accent(theme);
    loaders::orb(
        loaders::Orb::Cluster,
        id,
        THINKING_GLYPH,
        &bezel_theme,
        painter(window),
        cx,
    )
    .into_any_element()
}

/// A generic Bezel Cluster orb at the caller's requested size.
pub fn indeterminate(
    id: &'static str,
    size: f32,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut bezel_theme = bezel_theme(theme);
    bezel_theme.accent = loading_accent(theme);
    loaders::orb(
        loaders::Orb::Cluster,
        id,
        size,
        &bezel_theme,
        painter(window),
        cx,
    )
    .into_any_element()
}

/// The Bezel bloom — rings leaving the centre — in the caller's tint.
///
/// Bezel's loaders paint in one colour, the palette's accent, so a tint that
/// is not the accent is applied by handing the primitive a palette whose
/// accent is it. That is the mechanism `bezel_theme` already uses to put
/// Sirio's coral on every loader, one call deeper.
pub fn bloom(
    id: &'static str,
    size: f32,
    tint: Hsla,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut bezel_theme = bezel_theme(theme);
    bezel_theme.accent = tint;
    loaders::orb(
        loaders::Orb::Bloom,
        id,
        size,
        &bezel_theme,
        painter(window),
        cx,
    )
    .into_any_element()
}

/// How much of its slot a settled bloom's mark actually fills — the widest
/// ring a *travelling* bloom still paints at real strength.
///
/// A travelling ring fades as it leaves the centre (`orb_bloom_opacity`,
/// `(1 - phase)²`), so the frame where a ring reaches the rim is the frame
/// where it has become invisible: a running bloom's visible mark is only its
/// innermost rings. Settling one at `ORB_BLOOM_MAX` therefore drew the mark
/// on the rim nothing else ever reaches, more than twice the width of the
/// running bloom it shares a green with — so the same status read as two
/// different sizes depending on which state it settled in, which is the
/// defect this exists to close. Stopping at the ring one stagger step out
/// puts both on the same footprint.
///
/// Derived from Bezel's own curve rather than written down as a number, so a
/// `bezel` bump that retunes the bloom carries this with it instead of
/// silently drifting away from it.
pub fn settled_bloom_span() -> f32 {
    motion::orb_bloom_radius(1.0 / motion::ORB_BLOOM_RINGS as f32)
}

/// A settled bloom's rings as `(diameter, opacity)`, outermost first, in the
/// coordinates of the `slot` the caller reserves for the glyph.
///
/// The proportions are Bezel's: the span a travelling ring interpolates
/// across (`ORB_BLOOM_MIN`..`ORB_BLOOM_MAX`), sampled at the even steps
/// `ORB_BLOOM_RINGS` divides the travel into, with the outermost held at the
/// far end. `orb_bloom_radius` cannot be asked for that far end — it takes
/// `phase.rem_euclid(1.0)`, so a phase of exactly 1 wraps back to the centre
/// and would silently return the *smallest* ring — hence `lerp` against the
/// two constants directly.
///
/// The whole figure is then drawn at [`settled_bloom_span`] of the slot: the
/// same picture as before, at the size the running bloom reads at. The slot
/// itself is unchanged, so no strip or row relayouts around it.
pub fn settled_bloom_rings(slot: f32) -> Vec<(f32, f32)> {
    let mark = slot * settled_bloom_span();
    (0..motion::ORB_BLOOM_RINGS)
        .map(|index| {
            let step = (motion::ORB_BLOOM_RINGS - index) as f32 / motion::ORB_BLOOM_RINGS as f32;
            let diameter = mark
                * motion::lerp(
                    motion::phase::ORB_BLOOM_MIN,
                    motion::phase::ORB_BLOOM_MAX,
                    step,
                );
            (diameter, SETTLED_BLOOM_OPACITIES[index])
        })
        .collect()
}

/// A bloom stopped at its fullest frame: the same concentric rings, no clock
/// and no lease.
///
/// This is the one loader in this module Sirio draws itself, because Bezel
/// has no static orb — `loaders::orb` always takes the shared clock through
/// `pulse_delta`. It is built from Bezel's constants and mirrors the geometry
/// of `loaders::orb`'s `Orb::Bloom` arm (a border, not a fill, centred in the
/// box) so the moving and settled forms stay the same shape.
/// `size` is the *slot*, not the mark: the rings are drawn at
/// [`settled_bloom_span`] of it and centred, so the element a caller lays out
/// keeps the same footprint as the travelling bloom it stands in for.
pub fn settled_bloom(id: &'static str, size: f32, tint: Hsla) -> AnyElement {
    // The stroke belongs to the mark it outlines, not to the slot around it:
    // scaling it off `size` would leave a 6px figure wearing a 14px figure's
    // border and fill itself in.
    let border = px((size * settled_bloom_span() * BLOOM_BORDER_RATIO).max(1.0));
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .relative()
        .size(px(size))
        .children(
            settled_bloom_rings(size)
                .into_iter()
                .map(move |(diameter, opacity)| {
                    div()
                        .absolute()
                        .left(px((size - diameter) / 2.0))
                        .top(px((size - diameter) / 2.0))
                        .size(px(diameter))
                        .rounded_full()
                        .border(border)
                        .border_color(tint.opacity(opacity))
                }),
        )
        .into_any_element()
}

/// The compact Bezel mini gradient spinner for refresh/status slots.
///
/// The spinner sits in a wrapper that carries `id` as its debug selector:
/// Bezel hangs no selector on the key it takes (see the module note), so
/// without the wrapper no test could tell a slot holding a spinner from
/// an empty one. Every caller puts it in a centered flex slot, so a flex
/// wrapper sized by its content is layout-neutral.
pub fn compact(id: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .child(loaders::mini_gradient_spinner(
            id,
            COMPACT_MINI_CELL,
            painter(window),
            cx,
        ))
        .into_any_element()
}

/// A determinate progress bar with the gallery's fixed track and width.
pub fn progress(fraction: f32, theme: &Theme) -> Div {
    let bezel_theme = bezel_theme(theme);
    div()
        .w_full()
        .max_w(px(PROGRESS_MAX_WIDTH))
        .h(px(PROGRESS_TRACK))
        .child(bezel_theme.progress_bar(clamp_fraction(fraction)))
}

/// A subordinate set of redacted rows. Bezel's `pulse_delta` owns the shared
/// clock and its `reduced_motion` behavior; no Sirio timer is created here.
pub fn skeleton_rows(
    id: &'static str,
    count: usize,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let bezel_theme = bezel_theme(theme);
    popover::redacted_rows(id, &bezel_theme, count, painter(window), cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fraction_outside_the_unit_range_is_clamped_not_rejected() {
        assert_eq!(clamp_fraction(-0.5), 0.0);
        assert_eq!(clamp_fraction(1.7), 1.0);
        assert_eq!(clamp_fraction(0.35), 0.35);
        assert_eq!(clamp_fraction(f32::NAN), 0.0);
    }

    /// The defect this pins: a finished (green) or waiting (amber) tab drew a
    /// mark spanning the full 14px slot, while the running tab beside it read
    /// as a ~6px dot -- the same glyph at two sizes depending on which state
    /// it settled in.
    ///
    /// A travelling bloom's rings fade by `(1 - phase)²`, so its widest ring
    /// painted at real strength is the one a single stagger step out. The
    /// settled mark stops exactly there, so both states read the same size.
    #[test]
    fn a_settled_bloom_is_no_wider_than_a_travelling_blooms_bright_extent() {
        let bright_extent =
            motion::orb_bloom_radius(1.0 / motion::ORB_BLOOM_RINGS as f32) * BLOOM_GLYPH;
        let settled = settled_bloom_rings(BLOOM_GLYPH)[0].0;
        assert_eq!(
            settled, bright_extent,
            "the settled outer ring rides the travelling bloom's brightest travelling ring"
        );
        assert!(
            settled < BLOOM_GLYPH * motion::phase::ORB_BLOOM_MAX,
            "and no longer spans the whole slot ({settled} vs {BLOOM_GLYPH})"
        );
    }
}

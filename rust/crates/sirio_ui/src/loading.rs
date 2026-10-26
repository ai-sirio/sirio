// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The one place the loading primitives are called from.
//!
//! OWNERSHIP: this module renders; it owns no business state. Every surface
//! keeps its own task/status state machine and asks here only for a picture of
//! it. The orbs, the mini spinner and the skeleton rows are ports of
//! bezel-ui's, drawn on this crate's own animation clock (`crate::motion`),
//! so a change to how one looks or moves is a one-file change.
//!
//! A note on the `id` the orb and skeleton entry points take: the ports
//! discard it, as bezel's loaders did (`let _key`, `_id`), so today it keys
//! nothing and a collision would be symptomless. Pass a unique one anyway —
//! it is the identity these primitives are documented to take, and the day a
//! loader starts honouring it, duplicates would become state bleeding between
//! elements with no compile error to warn you.

use std::time::Duration;

use bezel::ui::widgets::Controls;
use gpui::{
    AnyElement, App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window,
    div, px,
};
use sirio_theme::Theme;

use crate::motion::{self, Painter};

/// The glyph slot in an Activity-derived reasoning header.
pub const THINKING_GLYPH: f32 = 14.0;
/// The generic orb for a full-surface first load or empty state.
pub const GENERIC_ORB: f32 = 44.0;
/// The status bloom that stands in for a sidebar row's status word.
pub const BLOOM_GLYPH: f32 = 14.0;
/// A bloom ring's border as a fraction of the box, as `orb_bloom` and
/// `settled_bloom` both draw it.
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
/// geometry belongs to `redacted_rows`: 28px rows, 6px gaps, and 4px vertical
/// padding.
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
/// retaining Sirio's appearance and accent. Bezel's progress bar and the
/// skeleton rows take that theme type, whereas the adapter's public contract
/// intentionally exposes Sirio's theme type to its callers.
///
/// This module used to push Sirio's appearance into Bezel's process-wide
/// paint mirror on every call, because `ink`, `wash` and `hairline` are free
/// functions with no `cx` and would otherwise paint for the previous
/// appearance. `Theme::sync_appearance` now does it wherever the theme global
/// is written, which is the one place that can be sure it happened.
fn bezel_theme(theme: &Theme) -> bezel_theme::Theme {
    let mut bezel_theme = match theme.appearance {
        sirio_theme::Appearance::Light => bezel_theme::Theme::light(),
        sirio_theme::Appearance::Dark => bezel_theme::Theme::dark(),
    };
    bezel_theme.accent = theme.sirio.brand_coral;
    bezel_theme
}

/// The neutral gray the thinking and loading orbs paint in.
fn loading_accent(theme: &Theme) -> Hsla {
    theme.ely.fg_muted
}

/// The view whose render is leasing the shared animation clock.
fn painter(window: &Window) -> Painter {
    Painter::from(window.current_view())
}

/// The orb cluster: blobs whose sizes swing so the count you perceive
/// changes, drifting far enough to merge and separate. The one a thinking
/// surface should reach for.
///
/// Everything is circles, because that is the whole vocabulary gpui gives: no
/// rotation transform, no conic gradient, and no blur filter on an element. So
/// the glow is a `BoxShadow` and every position is arithmetic — all of it pure
/// and in [`motion::phase`]. One tint, `accent`.
fn orb_cluster(size_px: f32, accent: Hsla, painter: Painter, cx: &mut App) -> impl IntoElement {
    let delta = motion::pulse_delta(Duration::from_millis(motion::ORB_MS), painter, cx);

    // A circle placed by its centre, since every seat below is a centre.
    let dot = move |cx: f32, cy: f32, size: f32, opacity: f32, glow: f32| {
        div()
            .absolute()
            .left(px(cx - size / 2.0))
            .top(px(cy - size / 2.0))
            .size(px(size))
            .rounded_full()
            .bg(accent.opacity(opacity))
            .shadow(vec![gpui::BoxShadow {
                color: accent.opacity(opacity * 0.55),
                offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(glow),
                spread_radius: px(0.0),
                inset: false,
            }])
    };

    let cells: Vec<gpui::Div> = (0..motion::ORBS)
        .map(|index| {
            // A third of a period apart, so one is always swelling while
            // another shrinks and the silhouette never repeats.
            let phase = motion::staggered_phase(delta, index, 1.0 / motion::ORBS as f32);
            let (seat_x, seat_y) = motion::ORB_SEATS[index];
            let (drift_x, drift_y) = motion::orb_drift(phase);
            dot(
                size_px * (seat_x + drift_x),
                size_px * (seat_y + drift_y),
                size_px * motion::orb_size(phase),
                motion::orb_opacity(phase),
                size_px * motion::orb_glow(phase),
            )
        })
        .collect();

    div().relative().size(px(size_px)).children(cells)
}

/// The bloom: rings leaving the centre and fading before the edge. The only
/// orb that travels outward, which is what makes it read as a signal rather
/// than a wait.
fn orb_bloom(size_px: f32, accent: Hsla, painter: Painter, cx: &mut App) -> impl IntoElement {
    let delta = motion::pulse_delta(Duration::from_millis(motion::ORB_MS), painter, cx);

    let cells: Vec<gpui::Div> = (0..motion::ORB_BLOOM_RINGS)
        .map(|index| {
            let phase = motion::staggered_phase(delta, index, 1.0 / motion::ORB_BLOOM_RINGS as f32);
            let diameter = size_px * motion::orb_bloom_radius(phase);
            let opacity = motion::orb_bloom_opacity(phase);
            // A ring, not a disc: the border is the whole element, so this
            // one is the odd shape out and cannot go through `dot`.
            div()
                .absolute()
                .left(px((size_px - diameter) / 2.0))
                .top(px((size_px - diameter) / 2.0))
                .size(px(diameter))
                .rounded_full()
                .border(px((size_px * BLOOM_BORDER_RATIO).max(1.0)))
                .border_color(accent.opacity(opacity))
        })
        .collect();

    div().relative().size(px(size_px)).children(cells)
}

/// A 2×3 miniature of the gradient spinner sized for a status-dot slot: the
/// brightness SNAKES around the grid's perimeter (every cell of a 2×3 grid is
/// on the ring) instead of sweeping as a vertical wave — a tiny radial chase.
/// ~6×10px footprint at the default 2.5px cells.
fn mini_gradient_spinner(cell_px: f32, painter: Painter, cx: &mut App) -> impl IntoElement {
    const COLS: usize = 2;
    const ROWS: usize = 3;
    /// Clockwise ring position of each `(row, col)` cell, top-left first:
    /// (0,0) → (0,1) → (1,1) → (2,1) → (2,0) → (1,0).
    const RING: [[usize; COLS]; ROWS] = [[0, 1], [5, 2], [4, 3]];
    const RING_LEN: f32 = (COLS * ROWS) as f32;
    let delta = motion::pulse_delta(
        Duration::from_millis(motion::GRADIENT_SPIN_MS),
        painter,
        cx,
    );
    div()
        .flex()
        .flex_col()
        .gap(px(cell_px / 2.0))
        .children((0..ROWS).map(move |row| {
            let tint: gpui::Hsla = gpui::rgb(motion::GSPIN_ROW_TINTS[row]).into();
            div()
                .flex()
                .flex_row()
                .gap(px(cell_px / 2.0))
                .children((0..COLS).map(move |col| {
                    let phase = RING[row][col] as f32 / RING_LEN;
                    div()
                        .size(px(cell_px))
                        .rounded(px(cell_px / 2.0))
                        .bg(tint)
                        .opacity(motion::gspin_opacity(delta + phase, motion::GSPIN_DIM))
                }))
        }))
}

/// Pulsing skeleton rows shown while a list loads (the reference:
/// `h-7 animate-pulse rounded-md bg-white/[0.04]`).
fn redacted_rows(
    _id: &'static str,
    _theme: &bezel_theme::Theme,
    count: usize,
    painter: Painter,
    cx: &mut App,
) -> AnyElement {
    let wash = bezel_theme::ink(0.04);
    let delta = motion::pulse_delta(Duration::from_millis(motion::PULSE_MS), painter, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .py(px(4.0))
        .children((0..count).map(move |i| {
            let phase = motion::staggered_phase(delta, i, 0.08);
            div()
                .h(px(28.0))
                .rounded(px(bezel_theme::Theme::control_radius()))
                .bg(wash)
                .opacity(0.35 + 0.4 * motion::pulse_wave(phase))
        }))
        .into_any_element()
}

/// An Activity-derived reasoning indicator using the Cluster orb.
pub fn thinking_indicator(
    _id: &'static str,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    orb_cluster(THINKING_GLYPH, loading_accent(theme), painter(window), cx).into_any_element()
}

/// A generic Cluster orb at the caller's requested size.
pub fn indeterminate(
    _id: &'static str,
    size: f32,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    orb_cluster(size, loading_accent(theme), painter(window), cx).into_any_element()
}

/// The bloom — rings leaving the centre — in the caller's tint.
///
/// The orbs paint in one colour, the `accent` they are handed, so the tint is
/// passed straight through.
pub fn bloom(
    _id: &'static str,
    size: f32,
    tint: Hsla,
    _theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    orb_bloom(size, tint, painter(window), cx).into_any_element()
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
/// Derived from the bloom's own curve rather than written down as a number,
/// so a retune of the bloom carries this with it instead of silently drifting
/// away from it.
pub fn settled_bloom_span() -> f32 {
    motion::orb_bloom_radius(1.0 / motion::ORB_BLOOM_RINGS as f32)
}

/// A settled bloom's rings as `(diameter, opacity)`, outermost first, in the
/// coordinates of the `slot` the caller reserves for the glyph.
///
/// The proportions are the travelling bloom's: the span a travelling ring
/// interpolates across (`ORB_BLOOM_MIN`..`ORB_BLOOM_MAX`), sampled at the even
/// steps `ORB_BLOOM_RINGS` divides the travel into, with the outermost held at
/// the far end. `orb_bloom_radius` cannot be asked for that far end — it takes
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
/// This is the one loader in this module that never touches the clock, because
/// there is no static orb — `orb_bloom` always takes the shared clock through
/// `pulse_delta`. It is built from the same constants and mirrors the geometry
/// of `orb_bloom` (a border, not a fill, centred in the box) so the moving and
/// settled forms stay the same shape.
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

/// The compact mini gradient spinner for refresh/status slots.
///
/// The spinner sits in a wrapper that carries `id` as its debug selector:
/// the spinner itself hangs no selector on anything (see the module note), so
/// without the wrapper no test could tell a slot holding a spinner from
/// an empty one. Every caller puts it in a centered flex slot, so a flex
/// wrapper sized by its content is layout-neutral.
pub fn compact(id: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .child(mini_gradient_spinner(
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

/// A subordinate set of redacted rows. `motion::pulse_delta` owns the shared
/// clock and its reduced-motion behavior; no timer is created here.
pub fn skeleton_rows(
    id: &'static str,
    count: usize,
    theme: &Theme,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let bezel_theme = bezel_theme(theme);
    redacted_rows(id, &bezel_theme, count, painter(window), cx)
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

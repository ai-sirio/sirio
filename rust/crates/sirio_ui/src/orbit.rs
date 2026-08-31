//! The orbit mark: a dot circling a planet, drawn in the empty centre
//! surfaces where a terminal would otherwise be.
//!
//! Decoration, not a loader. `loading.rs` is where an indicator that means
//! *something is running* comes from; this one means the opposite — nothing is
//! running, the surface is empty — so it deliberately does not reach for
//! Bezel's orbs, whose whole vocabulary is "wait".
//!
//! Everything is positioned arithmetic on plain circular `div`s because gpui
//! at this pin offers no rotation transform, so a ring with a travelling dot
//! cannot be one rotated element. Bezel's own loaders hit the same wall and
//! answered it the same way (see `bezel-ui`'s `loaders.rs` preamble).
//!
//! The animation runs through [`gpui::AnimationExt`], which honours
//! [`gpui::App::reduce_motion`]: with that set the mark renders static at its
//! start phase and schedules no frames at all.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, px,
};

/// The size the empty centre surfaces draw the mark at, replacing the 32px
/// terminal glyph that stood there before.
pub const EMPTY_SURFACE_MARK: f32 = 120.0;

/// One full revolution. Slow on purpose: at a spinner's tempo the mark reads
/// as work in progress, which is the one thing an empty surface is not.
const PERIOD: Duration = Duration::from_secs(8);

/// The mark never redraws faster than this. A decorative element on an idle
/// surface has no claim on a 120Hz display's frame budget.
const MAX_FPS: f32 = 30.0;

/// Diameters, as a fraction of the mark's overall size.
const PLANET_RATIO: f32 = 0.22;
const DOT_RATIO: f32 = 0.09;

/// Alpha multipliers against the caller's colour. The ring sits at the
/// threshold of visible; the dot is the only part meant to draw the eye.
const ORBIT_ALPHA: f32 = 0.22;
const PLANET_ALPHA: f32 = 0.45;
const DOT_ALPHA: f32 = 0.85;

/// The ring's stroke, matching the `border_1` it is drawn with.
const RING_BORDER: f32 = 1.0;

/// The distance from the mark's centre to the middle of the ring's stroke —
/// the line the dot rides.
///
/// The ring is inset by one dot-radius on every side rather than filling the
/// mark, which is what keeps a dot *centred on the line* from overflowing the
/// mark's own box. gpui borders are drawn inward from the edge, so the stroke's
/// middle is another half-border in.
fn orbit_radius(size: f32) -> f32 {
    let dot = size * DOT_RATIO;
    (size - dot) / 2.0 - RING_BORDER / 2.0
}

/// The top-left corner of the orbiting dot, in pixels from the mark's own
/// top-left corner, at phase `t` where 0..1 is one full revolution.
///
/// Pure, so the geometry is checkable without a window — the fake text system
/// behind `TestAppContext` cannot render this crate's elements.
fn dot_offset(t: f32, size: f32) -> (f32, f32) {
    let centre = size / 2.0;
    let dot = size * DOT_RATIO;
    let radius = orbit_radius(size);
    // Screen y grows downward, so subtracting a quarter turn starts the dot at
    // twelve o'clock and a growing angle carries it clockwise.
    let angle = t * TAU - FRAC_PI_2;
    (
        centre + radius * angle.cos() - dot / 2.0,
        centre + radius * angle.sin() - dot / 2.0,
    )
}

/// A dot orbiting a planet, in one colour at the caller's alpha ladder.
///
/// `id` keys the animation and must be unique among the elements alive at the
/// same time; two marks sharing one id would share one phase.
pub fn orbit(id: &'static str, size: f32, color: impl Into<Hsla>) -> impl IntoElement {
    // Sirio's theme tokens are `Rgba`; alpha is applied here in `Hsla`, which
    // is the space gpui's own `opacity` ladder works in.
    let color = color.into();
    let planet = size * PLANET_RATIO;
    let dot = size * DOT_RATIO;
    let planet_offset = (size - planet) / 2.0;

    div()
        .relative()
        .flex_none()
        .w(px(size))
        .h(px(size))
        .child(
            div()
                .absolute()
                .left(px(dot / 2.0))
                .top(px(dot / 2.0))
                .w(px(size - dot))
                .h(px(size - dot))
                .rounded_full()
                .border_1()
                .border_color(color.opacity(ORBIT_ALPHA)),
        )
        .child(
            div()
                .absolute()
                .left(px(planet_offset))
                .top(px(planet_offset))
                .w(px(planet))
                .h(px(planet))
                .rounded_full()
                .bg(color.opacity(PLANET_ALPHA)),
        )
        .child(
            div()
                .id(id)
                .absolute()
                .w(px(dot))
                .h(px(dot))
                .rounded_full()
                .bg(color.opacity(DOT_ALPHA))
                .with_animation(
                    id,
                    Animation::new(PERIOD).repeat().with_max_fps(MAX_FPS),
                    move |dot, t| {
                        let (x, y) = dot_offset(t, size);
                        dot.left(px(x)).top(px(y))
                    },
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: f32 = 100.0;
    /// Half the dot's diameter — the gap between a quarter-turn landmark and
    /// the dot's top-left corner along the axis it is centred on.
    const HALF_DOT: f32 = SIZE * DOT_RATIO / 2.0;

    fn assert_close(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 0.01,
            "{what}: expected {expected}, got {actual}"
        );
    }

    /// The dot's centre, which is the point the ring has to run through.
    fn dot_centre(t: f32, size: f32) -> (f32, f32) {
        let (x, y) = dot_offset(t, size);
        let half_dot = size * DOT_RATIO / 2.0;
        (x + half_dot, y + half_dot)
    }

    #[test]
    fn phase_zero_puts_the_dot_at_twelve_oclock() {
        let (x, y) = dot_offset(0.0, SIZE);

        assert_close(x, SIZE / 2.0 - HALF_DOT, "x is centred");
        assert_close(
            y,
            SIZE / 2.0 - orbit_radius(SIZE) - HALF_DOT,
            "y hangs the dot on the ring, not inside it",
        );
    }

    #[test]
    fn the_dot_rides_the_ring_at_every_phase() {
        // The defect this pins: the dot used to orbit at a radius one
        // half-dot shorter than the ring, so it visibly ran inside the line
        // instead of along it.
        let centre = SIZE / 2.0;

        for step in 0..360 {
            let t = step as f32 / 360.0;
            let (x, y) = dot_centre(t, SIZE);
            let distance = ((x - centre).powi(2) + (y - centre).powi(2)).sqrt();

            assert_close(distance, orbit_radius(SIZE), "the dot sits on the ring");
        }
    }

    #[test]
    fn the_dot_travels_clockwise_through_the_quarter_turns() {
        let centre = SIZE / 2.0;
        let radius = orbit_radius(SIZE);
        let (right_x, right_y) = dot_centre(0.25, SIZE);
        let (bottom_x, bottom_y) = dot_centre(0.5, SIZE);
        let (left_x, left_y) = dot_centre(0.75, SIZE);

        assert_close(right_x, centre + radius, "quarter turn is hard right");
        assert_close(right_y, centre, "quarter turn is centred");
        assert_close(bottom_x, centre, "half turn is centred");
        assert_close(bottom_y, centre + radius, "half turn is at the bottom");
        assert_close(left_x, centre - radius, "three quarter turn is hard left");
        assert_close(left_y, centre, "three quarter turn is centred");
    }

    #[test]
    fn a_full_revolution_returns_to_the_start() {
        let (start_x, start_y) = dot_offset(0.0, SIZE);
        let (end_x, end_y) = dot_offset(1.0, SIZE);

        // `Animation::repeat` restarts at 0 with no easing between the two, so
        // any gap here would show as a visible jump once every period.
        assert_close(end_x, start_x, "x wraps seamlessly");
        assert_close(end_y, start_y, "y wraps seamlessly");
    }

    #[test]
    fn the_dot_stays_inside_the_mark_at_every_phase() {
        let dot = SIZE * DOT_RATIO;

        for step in 0..360 {
            let t = step as f32 / 360.0;
            let (x, y) = dot_offset(t, SIZE);

            assert!(
                x >= -0.01 && x + dot <= SIZE + 0.01,
                "phase {t} leaves the box horizontally: {x}"
            );
            assert!(
                y >= -0.01 && y + dot <= SIZE + 0.01,
                "phase {t} leaves the box vertically: {y}"
            );
        }
    }
}

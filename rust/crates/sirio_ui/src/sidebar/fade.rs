//! The right-edge veil: text runs out under a gradient instead of stopping
//! at an ellipsis.
//!
//! Zed draws the same effect with `GradientFade`, an absolutely positioned
//! div carrying `linear_gradient(90°, row_background → transparent)`, and
//! switches between a normal, a hover and an active colour so the veil
//! always matches the row underneath. That file is GPL-3.0-or-later and is
//! not copied here: only its measurements are, and the gradient is built
//! against gpui's own `linear_gradient`/`linear_color_stop`.

use gpui::{Background, Div, Rgba, div, linear_color_stop, linear_gradient, prelude::*, px};

pub(super) const FADE_WIDTH: f32 = 92.0;
const FADE_STOP: f32 = 0.7;

/// The veil's fill for one background colour. Exposed on its own so a
/// caller whose background moves — a row that highlights on hover or when
/// selected — can hand the matching fill to `group_hover` instead of
/// leaving a patch of the wrong colour floating over the highlight.
pub(super) fn fade_gradient(background: Rgba) -> Background {
    linear_gradient(
        90.,
        linear_color_stop(background, FADE_STOP),
        linear_color_stop(
            Rgba {
                a: 0.0,
                ..background
            },
            0.,
        ),
    )
}

pub(super) fn fade_right(background: Rgba, width: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .w(px(width))
        .bg(fade_gradient(background))
}

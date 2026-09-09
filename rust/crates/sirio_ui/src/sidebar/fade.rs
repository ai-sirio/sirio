//! The right-edge veil: text runs out under a gradient instead of stopping
//! at an ellipsis.

use gpui::{Div, Rgba, div, linear_color_stop, linear_gradient, prelude::*, px};

pub(super) const FADE_WIDTH: f32 = 92.0;
const FADE_STOP: f32 = 0.7;

pub(super) fn fade_right(background: Rgba, width: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .w(px(width))
        .bg(linear_gradient(
            90.,
            linear_color_stop(background, FADE_STOP),
            linear_color_stop(
                Rgba {
                    a: 0.0,
                    ..background
                },
                0.,
            ),
        ))
}

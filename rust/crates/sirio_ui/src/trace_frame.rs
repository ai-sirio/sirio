//! The one loop that keeps a running trace moving, for both places that draw
//! it: the Trace card in the session panel's column and the Trace tab.
//!
//! Everything in either that moves is a function of the turn's own clock and
//! is painted by this loop, so nothing in them runs a gpui animation, which
//! would ask for frames of its own, uncapped and uncounted. The loop asks for
//! one frame at a time, by timer and never by gpui's next-frame callbacks, so
//! it stays at 30 frames a second or fewer, as gpui's own
//! `Animation::with_max_fps` does.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AnyElement, IntoElement, Styled, canvas};

/// How long the animation waits between two frames: 30 a second at most.
pub(crate) const FRAME: Duration = Duration::from_millis(33);
/// What both the card's folded title tick and the frames of a reader who
/// asked for reduced motion wait for.
pub(crate) const TICK: Duration = Duration::from_secs(1);

/// The canvas that keeps a running turn moving: while `running` and the
/// element is painted it schedules the next frame from its own prepaint, one
/// at a time and no sooner than `FRAME` after the last (`TICK` under reduced
/// motion), and counts and traces each. `pending` is what keeps one frame on
/// its way at a time and `counted` what a test reads; `entity` only tells one
/// owner's `trace.frame` rows from another's.
///
/// An element that is not drawn is never prepainted, so a hidden chat or tab
/// asks for nothing, and a turn that ended draws the canvas unarmed.
pub(crate) fn frame_driver(
    pending: Rc<Cell<bool>>,
    counted: Rc<Cell<u32>>,
    running: bool,
    reduced: bool,
    entity: u64,
) -> AnyElement {
    let wait = if reduced { TICK } else { FRAME };
    canvas(
        move |_, window, cx| {
            if !running || pending.replace(true) {
                return;
            }
            counted.set(counted.get().saturating_add(1));
            sirio_perf::event("trace.frame", entity);
            let view = window.current_view();
            let pending = pending.clone();
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(wait).await;
                    pending.set(false);
                    cx.update(move |_, cx| cx.notify(view)).ok();
                })
                .detach();
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
    .into_any_element()
}

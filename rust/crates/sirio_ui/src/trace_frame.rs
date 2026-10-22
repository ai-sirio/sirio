//! The one loop that keeps a running trace moving, for both places that draw
//! it: the Trace card in the session panel's column and the Trace tab.
//!
//! Everything in either that moves is a function of the turn's own clock and
//! is painted by this loop, so nothing in them runs a gpui animation, which
//! would ask for frames of its own, uncapped and uncounted. The loop asks for
//! one frame at a time, by timer and never by gpui's next-frame callbacks, so
//! it stays at 30 frames a second or fewer, as gpui's own
//! `Animation::with_max_fps` does, and only while something in the turn runs:
//! a turn that only waits for the user ticks once a second, for its clock.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AnyElement, IntoElement, Styled, canvas};

/// How long the animation waits between two frames: 30 a second at most.
pub(crate) const FRAME: Duration = Duration::from_millis(33);
/// What the card's folded title tick, a turn that is open but runs nothing
/// and the frames of a reader who asked for reduced motion wait for.
pub(crate) const TICK: Duration = Duration::from_secs(1);

/// How often a drawn turn asks to be painted again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pace {
    /// The turn has ended: nothing is asked for.
    Still,
    /// The turn is open and nothing in it runs: it waits for the user, or
    /// sits between two calls. Once a second keeps its clock, its duration
    /// and the "now" line moving.
    Tick,
    /// A model call streams, a tool runs or a subagent works: 30 a second
    /// at most.
    Frames,
}

impl Pace {
    /// `open` is whether the turn has not ended and `moving` whether some
    /// span is running (`Turn::is_moving`). Reduced motion keeps an open turn
    /// at a tick.
    pub(crate) fn of(open: bool, moving: bool, reduced: bool) -> Self {
        match (open, moving && !reduced) {
            (false, _) => Self::Still,
            (true, false) => Self::Tick,
            (true, true) => Self::Frames,
        }
    }
}

/// The frame a loop has on its way, if one is: the number of the timer that
/// owns it (the loop counts up, so a timer that was replaced knows it was) and
/// how long that timer waits. Shared between the canvas that asks and the
/// timer tasks, which end what they own.
pub(crate) type Pending = Rc<Cell<(u32, Option<Duration>)>>;

/// Whether a frame is on its way.
pub(crate) fn is_pending(pending: &Pending) -> bool {
    pending.get().1.is_some()
}

/// The canvas that keeps an open turn moving: while it is painted it
/// schedules the next frame from its own prepaint, one at a time and no
/// sooner than `pace` says after the last, and counts and traces each.
/// `pending` is what keeps one frame on its way at a time and `counted` what
/// a test reads; `entity` only tells one owner's `trace.frame` rows from
/// another's.
///
/// A turn that begins to run while a tick is on its way is not held for it: the
/// tick's timer is replaced by a frame's. The replaced timer repaints once
/// when it comes due and carries the loop no further, so one timer at a time
/// does.
///
/// An element that is not drawn is never prepainted, so a hidden chat or tab
/// asks for nothing, and a turn that ended draws the canvas unarmed.
pub(crate) fn frame_driver(
    pending: Pending,
    counted: Rc<Cell<u32>>,
    pace: Pace,
    entity: u64,
) -> AnyElement {
    let wait = match pace {
        Pace::Still => None,
        Pace::Tick => Some(TICK),
        Pace::Frames => Some(FRAME),
    };
    canvas(
        move |_, window, cx| {
            let Some(wait) = wait else {
                return;
            };
            let (timer, armed) = pending.get();
            if armed.is_some_and(|armed| armed <= wait) {
                return;
            }
            let timer = timer.wrapping_add(1);
            pending.set((timer, Some(wait)));
            counted.set(counted.get().saturating_add(1));
            sirio_perf::event("trace.frame", entity);
            let view = window.current_view();
            let pending = pending.clone();
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(wait).await;
                    // Only the timer the frame belongs to ends it.
                    if pending.get().0 == timer {
                        pending.set((timer, None));
                    }
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

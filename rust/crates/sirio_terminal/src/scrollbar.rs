//! The terminal's vertical scrollbar: where the thumb sits for a given
//! viewport position, and which row a dragged thumb asks for.
//!
//! Geometry is bezel's (`scroll::thumb`, `scroll::offset_for_thumb`), the same
//! functions the chat transcript and the file view draw their bars with. What
//! is different here is the unit: libghostty counts *rows* from the top of the
//! scrollback (offset 0 is the oldest line), while bezel counts pixels and
//! calls scrolling down *negative*. This module is the one place that
//! translation happens.

use bezel::theme::ink;
use bezel::ui::scroll::{self, MIN_THUMB, ScrollbarDrag};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, DragMoveEvent, Empty, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, Styled, Window, canvas, div, px,
};
use parking_lot::Mutex;
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// bezel's own strip and thumb widths (`scroll.rs`, private there); the chat
/// and file view bars restate them the same way.
const TRACK: f32 = 10.0;
const THUMB: f32 = 6.0;

/// The viewport's position in the scrollback, as libghostty reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScrollMetrics {
    /// Every row the terminal holds: scrollback plus the active screen.
    pub(crate) total: usize,
    /// The first visible row, counted from the top of the scrollback.
    pub(crate) offset: usize,
    /// How many rows the viewport shows.
    pub(crate) len: usize,
}

impl ScrollMetrics {
    /// Rows above the bottom of the scrollback the viewport can still move.
    fn scrollable_rows(self) -> usize {
        self.total.saturating_sub(self.len)
    }
}

/// Pixels of track per row of viewport.
///
/// The scale is the track over the rows *shown*, not the pane's height over a
/// measured cell: a pane rarely holds a whole number of rows, and this way the
/// thumb is exactly `len / total` of the track however the remainder falls.
/// `None` before the emulator has been sized.
fn pixels_per_row(metrics: ScrollMetrics, track: Pixels) -> Option<Pixels> {
    (metrics.len > 0).then(|| track / metrics.len as f32)
}

/// Where the thumb sits in a track of `track` pixels, measured from its top,
/// or `None` when there is nothing to show.
pub(crate) fn thumb_range(metrics: ScrollMetrics, track: Pixels) -> Option<Range<Pixels>> {
    let per_row = pixels_per_row(metrics, track)?;
    let scrollable = metrics.scrollable_rows();
    let max_offset = per_row * scrollable as f32;
    // bezel counts scrolling down as negative; libghostty counts from the top.
    let offset = -(per_row * metrics.offset.min(scrollable) as f32);
    scroll::thumb(track, max_offset, offset, MIN_THUMB)
}

/// The row a viewport should start at when the thumb's top is dragged to
/// `top` pixels below the track's top. Never past either end of the
/// scrollback; when there is no thumb to drag, the viewport stays where it is.
pub(crate) fn row_for_thumb_top(top: Pixels, metrics: ScrollMetrics, track: Pixels) -> usize {
    let (Some(range), Some(per_row)) =
        (thumb_range(metrics, track), pixels_per_row(metrics, track))
    else {
        return metrics.offset;
    };
    let scrollable = metrics.scrollable_rows();
    let max_offset = per_row * scrollable as f32;
    let travelled = -scroll::offset_for_thumb(top, track, max_offset, range.end - range.start);
    ((travelled / per_row).round() as usize).min(scrollable)
}

/// State a thumb drag carries between events, shared by the bar (which sets
/// it) and the pane's own mouse handlers (which must leave the gesture alone).
#[derive(Clone, Default)]
pub(crate) struct ScrollbarGesture {
    /// Where in the thumb the pointer took hold, so the thumb does not jump
    /// its middle to the pointer on the first move.
    grab: Rc<Cell<Option<Pixels>>>,
    /// Set by a press on the bar and consumed by the pane's next release. The
    /// press never reaches the pane (the bar stops it), but the release that
    /// ends the drag can land anywhere in it -- and must not be reported to a
    /// guest that never saw the press.
    pressed: Rc<Cell<bool>>,
}

impl ScrollbarGesture {
    /// Whether a press began on the bar since the last release the pane saw;
    /// clears the mark, because a release ends the gesture.
    pub(crate) fn take_pressed(&self) -> bool {
        self.pressed.replace(false)
    }

    /// A press anywhere else means any earlier bar gesture is over, even if
    /// its release happened outside the window and was never delivered.
    pub(crate) fn forget_press(&self) {
        self.pressed.set(false);
    }
}

fn same_height(a: Option<Pixels>, b: Option<Pixels>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= px(0.5),
        (None, None) => true,
        _ => false,
    }
}

/// The bar: an overlay strip along the pane's right edge, showing nothing at
/// all while there is no history to scroll. Overlay rather than a gutter, so
/// its arrival never changes the column count the guest sees.
///
/// `bounds` is the pane's last-painted box, the same cell
/// `TerminalElement::prepaint` writes every frame: a render can only see the
/// frame before it, so a zero-size canvas at the end of the bar asks for one
/// more frame when the fresh layout disagrees, and the bar is right the frame
/// after the pane is resized.
pub(crate) fn bar(
    entity_id: u64,
    metrics: ScrollMetrics,
    bounds: &Arc<Mutex<Option<Bounds<Pixels>>>>,
    gesture: &ScrollbarGesture,
    scroll_to: impl Fn(usize, &mut App) + 'static,
) -> AnyElement {
    if metrics.total <= metrics.len {
        return Empty.into_any_element();
    }
    let seen = bounds.lock().map(|bounds| bounds.size.height);
    let watched = bounds.clone();
    let watch = canvas(
        move |_, window, _| {
            let fresh = watched.lock().map(|bounds| bounds.size.height);
            if !same_height(fresh, seen) {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();
    let Some(track) = seen else {
        return watch.into_any_element();
    };
    let Some(range) = thumb_range(metrics, track) else {
        return watch.into_any_element();
    };
    let size = range.end - range.start;
    let dragging = gesture.grab.get().is_some();

    let id = SharedString::from(format!("terminal-scrollbar-{entity_id}"));
    let drag_id = id.clone();
    let drag_gesture = gesture.clone();
    let release_gesture = gesture.clone();
    let press_gesture = gesture.clone();
    let released = move |_: &MouseUpEvent, _: &mut Window, _: &mut App| {
        release_gesture.grab.set(None);
    };
    let pressed = move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
        press_gesture.pressed.set(true);
        // The pane's own press handler would focus, anchor a selection and
        // report the press to a guest that asked for mouse reports.
        cx.stop_propagation();
    };
    div()
        .id(SharedString::from(format!("{id}-track")))
        .debug_selector(|| "terminal-scrollbar-track".to_owned())
        .absolute()
        .top_0()
        .right_0()
        .h(track)
        .w(px(TRACK))
        .flex()
        .justify_center()
        .on_mouse_down(MouseButton::Left, pressed)
        .on_drag_move(move |event: &DragMoveEvent<ScrollbarDrag>, _, cx| {
            // Every terminal in the window listens for drags of this type;
            // only the one whose thumb was grabbed answers.
            if event.drag(cx).0 != drag_id {
                return;
            }
            let pointer = event.event.position.y - event.bounds.top();
            let grab = drag_gesture.grab.get().unwrap_or_else(|| {
                let grab = (pointer - range.start).clamp(px(0.0), size);
                drag_gesture.grab.set(Some(grab));
                grab
            });
            scroll_to(row_for_thumb_top(pointer - grab, metrics, track), cx);
        })
        // Both, because a release can land anywhere on screen; a grab left
        // set would make the next press continue the last gesture.
        .on_mouse_up(MouseButton::Left, released.clone())
        .on_mouse_up_out(MouseButton::Left, released)
        .child(
            div()
                .id(SharedString::from(format!("{id}-thumb")))
                .debug_selector(|| "terminal-scrollbar-thumb".to_owned())
                .absolute()
                .top(range.start)
                .h(size)
                .w(px(THUMB))
                .rounded_full()
                .bg(if dragging { ink(0.38) } else { ink(0.2) })
                .hover(|style| style.bg(ink(0.32)))
                .on_drag(ScrollbarDrag(id.clone()), |_, _, _, cx| cx.new(|_| Empty)),
        )
        .child(watch)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    fn metrics(total: usize, offset: usize, len: usize) -> ScrollMetrics {
        ScrollMetrics { total, offset, len }
    }

    fn close(actual: Pixels, expected: f32) {
        let actual = f32::from(actual);
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    fn bounds_of(range: Option<Range<Pixels>>) -> (f32, f32) {
        let range = range.expect("a thumb was expected");
        (f32::from(range.start), f32::from(range.end))
    }

    // ---- the ways showing the thumb can be wrong -------------------------

    #[test]
    fn no_history_means_no_thumb() {
        // A fresh shell, or an alternate-screen app (vim, a full-screen TUI):
        // the viewport is the whole buffer.
        assert_eq!(thumb_range(metrics(24, 0, 24), px(480.0)), None);
    }

    #[test]
    fn a_total_smaller_than_the_viewport_means_no_thumb() {
        // Metrics are published from another thread; a torn read must not
        // underflow into a huge scrollable range.
        assert_eq!(thumb_range(metrics(10, 0, 24), px(480.0)), None);
    }

    #[test]
    fn a_zero_row_viewport_means_no_thumb_and_no_division_by_zero() {
        // The first frame, before the emulator has been sized.
        assert_eq!(thumb_range(metrics(100, 0, 0), px(480.0)), None);
        assert_eq!(
            row_for_thumb_top(px(10.0), metrics(100, 40, 0), px(480.0)),
            40
        );
    }

    #[test]
    fn a_zero_height_track_means_no_thumb() {
        assert_eq!(thumb_range(metrics(100, 0, 24), px(0.0)), None);
    }

    #[test]
    fn a_track_too_short_to_hold_a_thumb_means_no_thumb() {
        // bezel refuses a bar that would be all thumb and no travel.
        assert_eq!(thumb_range(metrics(100, 0, 24), px(20.0)), None);
    }

    #[test]
    fn the_thumb_is_the_viewport_s_share_of_the_track() {
        // 24 of 240 rows visible: a tenth of a 480px track.
        let (start, end) = bounds_of(thumb_range(metrics(240, 0, 24), px(480.0)));
        close(px(start), 0.0);
        close(px(end - start), 48.0);
    }

    #[test]
    fn the_share_is_exact_when_the_pane_is_not_a_whole_number_of_rows() {
        // A 500px pane showing 24 rows (20.83px per row of track): half the
        // buffer is visible, so half the track is thumb.
        let (start, end) = bounds_of(thumb_range(metrics(48, 0, 24), px(500.0)));
        close(px(start), 0.0);
        close(px(end - start), 250.0);
    }

    #[test]
    fn a_very_long_scrollback_keeps_the_thumb_grabbable() {
        let (start, end) = bounds_of(thumb_range(metrics(10_024, 0, 24), px(480.0)));
        close(px(start), 0.0);
        close(px(end - start), 25.0);
    }

    #[test]
    fn at_the_top_the_thumb_touches_the_top_of_the_track() {
        let (start, _) = bounds_of(thumb_range(metrics(240, 0, 24), px(480.0)));
        close(px(start), 0.0);
    }

    #[test]
    fn at_the_bottom_the_thumb_touches_the_bottom_of_the_track() {
        // Offset 216 is the last position a 24 row viewport can take in 240.
        let (_, end) = bounds_of(thumb_range(metrics(240, 216, 24), px(480.0)));
        close(px(end), 480.0);
    }

    #[test]
    fn halfway_through_the_history_the_thumb_is_halfway_along_its_travel() {
        // Offset 108 of 216 scrollable rows; travel is 480 - 48 = 432.
        let (start, _) = bounds_of(thumb_range(metrics(240, 108, 24), px(480.0)));
        close(px(start), 216.0);
    }

    #[test]
    fn a_stale_offset_past_the_bottom_pins_the_thumb_to_the_bottom() {
        // Published a moment before a clear shrank `total`.
        let (_, end) = bounds_of(thumb_range(metrics(240, 500, 24), px(480.0)));
        close(px(end), 480.0);
    }

    // ---- the ways a drag can pick the wrong row --------------------------

    #[test]
    fn dragging_to_the_top_of_the_track_asks_for_the_oldest_row() {
        assert_eq!(
            row_for_thumb_top(px(0.0), metrics(240, 100, 24), px(480.0)),
            0
        );
    }

    #[test]
    fn dragging_to_the_end_of_the_travel_asks_for_the_last_scrollable_row() {
        // Travel is 480 - 48 = 432.
        assert_eq!(
            row_for_thumb_top(px(432.0), metrics(240, 0, 24), px(480.0)),
            216
        );
    }

    #[test]
    fn dragging_past_either_end_of_the_track_stops_at_that_end() {
        let m = metrics(240, 100, 24);
        assert_eq!(row_for_thumb_top(px(-50.0), m, px(480.0)), 0);
        assert_eq!(row_for_thumb_top(px(900.0), m, px(480.0)), 216);
    }

    #[test]
    fn dragging_halfway_asks_for_the_middle_row() {
        assert_eq!(
            row_for_thumb_top(px(216.0), metrics(240, 0, 24), px(480.0)),
            108
        );
    }

    #[test]
    fn a_drag_lands_on_a_whole_row_not_a_truncated_one() {
        // 216 rows over 432px of travel is 2px per row. 11.4px is row 5.7:
        // rounds to 6, and truncating would say 5.
        assert_eq!(
            row_for_thumb_top(px(11.4), metrics(240, 0, 24), px(480.0)),
            6
        );
    }

    #[test]
    fn with_no_thumb_to_drag_the_viewport_stays_put() {
        // Not row 0: a stray drag event on a bar that just vanished must not
        // throw the viewport to the top.
        assert_eq!(
            row_for_thumb_top(px(200.0), metrics(24, 0, 24), px(480.0)),
            0
        );
        assert_eq!(
            row_for_thumb_top(px(200.0), metrics(100, 60, 24), px(20.0)),
            60
        );
    }

    #[test]
    fn a_thumb_placed_by_thumb_range_is_dragged_back_to_the_same_row() {
        // The two functions are inverses; a sign or scale slip in either one
        // shows up as a row that comes back different, at some offset, for
        // some scrollback length -- including the one where the thumb is
        // clamped to its minimum size and the travel changes.
        for (total, len, track) in [
            (240, 24, 480.0),
            (48, 24, 500.0),
            (10_024, 24, 480.0),
            (31, 30, 900.0),
        ] {
            for offset in (0..=total - len).step_by(((total - len) / 40).max(1)) {
                let m = metrics(total, offset, len);
                let range = thumb_range(m, px(track)).expect("scrollable");
                assert_eq!(
                    row_for_thumb_top(range.start, m, px(track)),
                    offset,
                    "total {total}, len {len}, track {track}, offset {offset}"
                );
            }
        }
    }
}

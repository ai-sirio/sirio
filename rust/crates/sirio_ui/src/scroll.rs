// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The maths Sirio's own scrollbars share, the drag payload they pass, and the
//! state that keeps a view pinned to the end of content that grows under it.
//!
//! The bars themselves are drawn elsewhere (the terminal's, the transcript's
//! and the horizontal one); what they have in common is here: where a thumb
//! sits in its track, which scroll offset a dragged thumb asks for, and the
//! payload that tells one bar's drag from another's.
//!
//! The geometry is transcribed from zed's own scrollbar (`thumb_ranges` in
//! `crates/ui/src/components/scrollbar.rs`). Two of gpui's conventions are easy
//! to get backwards and both are load-bearing here: `max_offset` is the
//! *overflow* (content minus viewport, not content), and `offset` is
//! **negative** as you scroll down.

use std::{cell::Cell, ops::Range, rc::Rc};

use gpui::{Pixels, ScrollHandle, SharedString, canvas, point, prelude::*, px};

/// Shortest a thumb may get, however long the document — below this it stops
/// being something a pointer can catch.
pub const MIN_THUMB: Pixels = px(25.0);

/// Where the thumb sits in a track of `viewport` length, as a range from the
/// track's start — or `None` when there is nothing to scroll.
///
/// `None` also covers a viewport of zero (the frame before layout has run) and
/// a thumb that would not fit, which is zed's third guard: with a viewport
/// shorter than [`MIN_THUMB`] a bar would be all thumb and no travel.
pub fn thumb(
    viewport: Pixels,
    max_offset: Pixels,
    offset: Pixels,
    min: Pixels,
) -> Option<Range<Pixels>> {
    if viewport <= px(0.0) || max_offset <= px(0.0) {
        return None;
    }
    let content = viewport + max_offset;
    let size = min.max(viewport * (viewport / content));
    if size > viewport {
        return None;
    }
    // Negative going down, and never past either end — a wheel can overshoot.
    let travelled = offset.clamp(-max_offset, px(0.0)).abs();
    let start = (travelled / max_offset) * (viewport - size);
    Some(start..start + size)
}

/// The inverse: the scroll offset that puts the thumb's top at `top`.
///
/// Negative, because that is the direction gpui counts in, and clamped to the
/// scrollable range so a drag past either end simply stops.
pub fn offset_for_thumb(top: Pixels, viewport: Pixels, max_offset: Pixels, size: Pixels) -> Pixels {
    let travel = viewport - size;
    if travel <= px(0.0) || max_offset <= px(0.0) {
        return px(0.0);
    }
    -(max_offset * (top / travel).clamp(0.0, 1.0))
}

/// The drag payload. Carries the bar's id because, unlike a split, an app has
/// several of these on screen at once and `on_drag_move` filters by type alone
/// — without the id every bar in the window would answer one thumb's gesture.
#[derive(Clone)]
pub struct ScrollbarDrag(pub SharedString);

// ---------------------------------------------------------------------------
// Follow — a view pinned to the bottom of content that grows under it
// ---------------------------------------------------------------------------

/// How close to the bottom still counts as following. A wheel lands on
/// fractional offsets and a re-layout can move the end by a hair; without slack
/// a view would unpin itself for a rounding error nobody asked for.
const FOLLOW_SLACK: Pixels = px(4.0);

/// Whether `offset` is at the end of the scrollable range, within `slack`.
///
/// Both of gpui's conventions bite here, so: `max_offset` is the *overflow* and
/// `offset` is **negative** going down, which makes the distance still to go
/// `max_offset - |offset|`. Content that fits is always "at the bottom" — there
/// is nowhere else to be, and answering `false` would unpin an empty log.
fn at_bottom(max_offset: Pixels, offset: Pixels, slack: Pixels) -> bool {
    if max_offset <= px(0.0) {
        return true;
    }
    let travelled = offset.clamp(-max_offset, px(0.0)).abs();
    max_offset - travelled <= slack
}

/// Whether a [`follow`] view is still pinned, and the overflow it last saw.
///
/// An `Rc` cell the view holds one field of and the element clones: it mutates
/// through `&self`, so the element carries the whole behaviour without the view
/// wiring a listener. Starts pinned — a transcript or a log opens on its newest
/// line.
#[derive(Clone)]
pub struct FollowState(Rc<Cell<(bool, Pixels)>>);

impl Default for FollowState {
    fn default() -> Self {
        Self(Rc::new(Cell::new((true, px(0.0)))))
    }
}

impl FollowState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Keep `handle` pinned to the bottom of its content while the user leaves it
/// there, and get out of the way the moment they scroll up.
///
/// Drop it in over the same container the handle tracks, as an absolute sibling
/// of the scrolling `div`.
///
/// **Telling appended content from a user scroll is the whole problem**, and
/// neither is an event this can subscribe to — both surface as the same handle
/// reading differently than last frame. The overflow is what separates them: if
/// it changed, the content grew and the pin is left as the user last set it; if
/// it did not, the offset moved because the *user* moved it, and being at the
/// end is what re-pins. So scrolling up releases, and scrolling back down
/// re-attaches, with no gesture to hook.
///
/// The correction lands a frame late — the scrolling div was laid out with the
/// old offset before this runs — which is why it asks for that frame. At a
/// streaming cadence it is invisible, and it converges rather than spinning:
/// once pinned and at the end, nothing is requested.
pub fn follow(handle: &ScrollHandle, state: &FollowState) -> gpui::AnyElement {
    let handle = handle.clone();
    let state = state.clone();
    canvas(
        move |_, window, _| {
            let max_offset = handle.max_offset().y;
            let offset = handle.offset().y;
            let (was_pinned, last_max) = state.0.get();

            let pinned = if (max_offset - last_max).abs() > px(0.5) {
                was_pinned
            } else {
                at_bottom(max_offset, offset, FOLLOW_SLACK)
            };

            if pinned && (offset + max_offset).abs() > px(0.5) {
                handle.set_offset(point(handle.offset().x, -max_offset));
                window.request_animation_frame();
            }
            state.0.set((pinned, max_offset));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
    .into_any_element()
}

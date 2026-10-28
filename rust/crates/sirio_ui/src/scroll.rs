// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The maths Sirio's own scrollbars share, the drag payload they pass, the two
//! generic bars over a `ScrollHandle`, and the state that keeps a view pinned
//! to the end of content that grows under it.
//!
//! The bars the app puts over a tracked surface are built here: [`bar`], Ely's
//! vertical bar, and [`horizontal_bar`], Sirio's own horizontal one, each with
//! the canvas that stands in for it until the content first overflows. The
//! terminal's and the transcript's are drawn elsewhere; what all of them have
//! in common is here: where a thumb sits in its track, which scroll offset a
//! dragged thumb asks for, and the payload that tells one bar's drag from
//! another's. Beside Ely's bars, which draw from the geometry the previous
//! frame left, sits [`request_frame_when_geometry_moves`], the canvas that asks
//! for the frame that redraws them when this frame's layout disagrees.
//!
//! The geometry is transcribed from zed's own scrollbar (`thumb_ranges` in
//! `crates/ui/src/components/scrollbar.rs`). Two of gpui's conventions are easy
//! to get backwards and both are load-bearing here: `max_offset` is the
//! *overflow* (content minus viewport, not content), and `offset` is
//! **negative** as you scroll down.

use std::{cell::Cell, ops::Range, rc::Rc};

use gpui::{
    AnyElement, Axis, Entity, Pixels, ScrollHandle, SharedString, canvas, div, point, prelude::*,
    px,
};

use crate::horizontal_scroll::{self, HorizontalBarState};

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

/// A scroll box's length along `axis` and how far its content overflows, as
/// the handle last reported them.
fn geometry(handle: &ScrollHandle, axis: Axis) -> (Pixels, Pixels) {
    let (bounds, max) = (handle.bounds().size, handle.max_offset());
    match axis {
        Axis::Vertical => (bounds.height, max.y),
        Axis::Horizontal => (bounds.width, max.x),
    }
}

/// Asks for one more frame when a scroll box, once this frame has laid it
/// out, is not the size, or does not overflow by the amount, the bar built
/// beside it was drawn from.
///
/// A bar reads its handle while it is being built, which is before this
/// frame's layout has run: it can only draw from the geometry the *previous*
/// frame left. The frame that first lays content out taller than its box, or a
/// resize, would end with a bar drawn for the old geometry and nothing asking
/// for the frame that fixes it. This canvas takes the geometry the bar was
/// built from, prepaints after the scroll box has, compares, and asks for one
/// more frame when they disagree by more than half a pixel. Self-limiting: once
/// the bar's geometry is the layout's, nothing is requested.
///
/// Lay it over the box beside its bar, as an absolute child of the box's
/// positioned parent. `axis` is the one the bar runs along.
pub(crate) fn request_frame_when_geometry_moves(
    handle: &ScrollHandle,
    axis: Axis,
) -> gpui::AnyElement {
    let drawn = geometry(handle, axis);
    let watched = handle.clone();
    canvas(
        move |_, window, _| {
            let live = geometry(&watched, axis);
            if (live.0 - drawn.0).abs() > px(0.5) || (live.1 - drawn.1).abs() > px(0.5) {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
    .into_any_element()
}

/// Ely's vertical bar over one tracked surface, under a selector the
/// drawn-frame tests can find. Ely's track carries no selector of its own, so
/// the tag goes on a full-surface overlay that exists only while there is
/// overflow to show, and that registers no hitbox, so the pointer still
/// reaches the rows beneath it. `id` names the overlay and, with `-track`,
/// the bar's own element.
///
/// While there is nothing to show, a canvas watches the handle instead:
/// the bar can only draw from the geometry the *previous* frame left, so
/// the frame that first lays the content out taller than its viewport
/// would otherwise end with no bar and nothing asking for the frame that
/// paints it (Preview has no caret blink to repaint on). The canvas
/// prepaints after its sibling has, sees this frame's overflow, and asks
/// for one more frame. Self-limiting: once the bar is up it is not here.
/// While the bar is up, [`request_frame_when_geometry_moves`] does the same
/// for a viewport that resizes or content that grows or shrinks under it.
pub(crate) fn bar(id: impl Into<SharedString>, handle: &ScrollHandle) -> AnyElement {
    let id: SharedString = id.into();
    let has_overflow = |handle: &ScrollHandle| {
        thumb(
            handle.bounds().size.height,
            handle.max_offset().y,
            handle.offset().y,
            MIN_THUMB,
        )
        .is_some()
    };
    if !has_overflow(handle) {
        let watched = handle.clone();
        return canvas(
            move |_, window, _| {
                if has_overflow(&watched) {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element();
    }
    let selector = id.clone();
    div()
        .debug_selector(move || selector.to_string())
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .child(ely_gpui_component::layout::Scrollbar::new(
            SharedString::from(format!("{id}-track")),
            handle,
            Axis::Vertical,
        ))
        .child(request_frame_when_geometry_moves(handle, Axis::Vertical))
        .into_any_element()
}

/// The horizontal counterpart of [`bar`], for a surface wider than
/// its viewport. The bar overlays the surface's own box, so it spans
/// exactly the viewport it reports on and never reflows what is beneath
/// it; a drag moves only X, leaving the vertical offset where it was.
/// While there is nothing to show, the same canvas watch asks for the
/// frame that paints the bar once the content first lays out wider.
pub(crate) fn horizontal_bar<V: 'static>(
    id: &'static str,
    handle: &ScrollHandle,
    state: &HorizontalBarState,
    entity: Entity<V>,
) -> AnyElement {
    let has_overflow = |handle: &ScrollHandle| {
        thumb(
            handle.bounds().size.width,
            handle.max_offset().x,
            handle.offset().x,
            MIN_THUMB,
        )
        .is_some()
    };
    if !has_overflow(handle) {
        let watched = handle.clone();
        return canvas(
            move |_, window, _| {
                if has_overflow(&watched) {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element();
    }
    let drag_handle = handle.clone();
    horizontal_scroll::bar(
        id,
        handle.bounds().size.width,
        handle.max_offset().x,
        handle.offset().x,
        state,
        move |x, cx| {
            drag_handle.set_offset(point(x, drag_handle.offset().y));
            entity.update(cx, |_, cx| cx.notify());
        },
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, IntoElement, Render, TestAppContext, VisualTestContext, Window, div, size};

    /// A scroll box of `rows` 20 px rows with the helper laid over it, the way a bar's host
    /// lays its bar.
    struct Host {
        handle: ScrollHandle,
        rows: usize,
        axis: Axis,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let scroll = div().id("scroll").size_full().track_scroll(&self.handle);
            let scroll = match self.axis {
                Axis::Vertical => scroll.overflow_y_scroll(),
                Axis::Horizontal => scroll.overflow_x_scroll(),
            };
            let body = match self.axis {
                Axis::Vertical => div().children((0..self.rows).map(|_| div().h(px(20.0)))),
                Axis::Horizontal => div().w(px(20.0 * self.rows as f32)).h(px(20.0)).flex_none(),
            };
            div()
                .size_full()
                .relative()
                .child(scroll.child(body))
                .child(request_frame_when_geometry_moves(&self.handle, self.axis))
        }
    }

    fn mounted(cx: &mut TestAppContext, rows: usize, axis: Axis) -> (gpui::Entity<Host>, &mut VisualTestContext) {
        let (host, cx) = cx.add_window_view(move |_, _| Host {
            handle: ScrollHandle::new(),
            rows,
            axis,
        });
        cx.simulate_resize(size(px(100.0), px(100.0)));
        (host, cx)
    }

    /// Frames asked for since the last count, taken after three more are drawn. A test has no
    /// frame loop: the requests wait until `simulate_next_frame` collects them.
    fn asked_over_three_frames(cx: &mut VisualTestContext) -> usize {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| window.simulate_next_frame(cx))
    }

    #[gpui::test]
    fn a_box_that_first_overflows_asks_for_the_frame_that_draws_its_bar(cx: &mut TestAppContext) {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let (_host, cx) = mounted(cx, 40, axis);
            assert!(
                asked_over_three_frames(cx) > 0,
                "{axis:?}: the first frame lays the content out past its box; the bar was built from a handle that knew nothing"
            );
            assert_eq!(
                asked_over_three_frames(cx),
                0,
                "{axis:?}: once the bar and the layout agree, an idle box asks for nothing"
            );
        }
    }

    #[gpui::test]
    fn content_that_grows_or_shrinks_under_a_bar_asks_for_one_more_frame(cx: &mut TestAppContext) {
        let (host, cx) = mounted(cx, 40, Axis::Vertical);
        asked_over_three_frames(cx);
        assert_eq!(asked_over_three_frames(cx), 0);
        for rows in [90, 3] {
            host.update(cx, |host, cx| {
                host.rows = rows;
                cx.notify();
            });
            assert!(
                asked_over_three_frames(cx) > 0,
                "{rows} rows: the bar drawn beside them was sized for the old content"
            );
            assert_eq!(asked_over_three_frames(cx), 0, "{rows} rows: and then it settles");
        }
    }

    #[gpui::test]
    fn a_box_resized_along_its_bars_axis_asks_for_one_more_frame_and_across_it_for_none(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = mounted(cx, 40, Axis::Horizontal);
        let overflow = |host: &gpui::Entity<Host>, cx: &mut VisualTestContext| {
            host.read_with(cx, |host, _| host.handle.max_offset().x)
        };
        asked_over_three_frames(cx);
        assert_eq!(asked_over_three_frames(cx), 0, "the bar is up and the box idle");
        assert!(
            (overflow(&host, cx) - px(700.0)).abs() < px(0.5),
            "the premise: 800 px of content in a 100 px box overflow by 700, not {:?}",
            overflow(&host, cx)
        );

        // 40 px narrower with 40 px less content, in the one frame the resize draws (the content
        // is changed without a notify): the length moved and the overflow did not, so only the
        // comparison of the length can see it.
        host.update(cx, |host, _| host.rows = 38);
        cx.simulate_resize(size(px(60.0), px(100.0)));
        assert!(
            asked_over_three_frames(cx) > 0,
            "the box is 40 px narrower and its bar was sized for the old width"
        );
        assert_eq!(asked_over_three_frames(cx), 0, "and then it settles");
        assert!(
            (overflow(&host, cx) - px(700.0)).abs() < px(0.5),
            "the premise held: 760 px of content in a 60 px box still overflow by 700, not {:?}",
            overflow(&host, cx)
        );

        // Shorter only: a horizontal bar reads the width, so the height is not its business.
        cx.simulate_resize(size(px(60.0), px(70.0)));
        assert_eq!(
            asked_over_three_frames(cx),
            0,
            "a horizontal bar asks for nothing when only the box's height changes"
        );
    }
}

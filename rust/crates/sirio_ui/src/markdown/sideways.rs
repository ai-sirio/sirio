//! A block that scrolls sideways, with a bar that shows it does.
//!
//! The port of bezel's `Sideways` (`bezel_markdown/src/render.rs`). A container that
//! scrolls without drawing a bar hides whatever is past its edge, and Ely draws none, so
//! fenced code and (through its `table_frame`) tables are framed here.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Div, DragMoveEvent, ElementId, EmptyView, EntityId, Hsla,
    IntoElement, MouseButton, Pixels, RenderOnce, ScrollHandle, Stateful, Window, canvas, div,
    point, prelude::*, px,
};

/// The bar's room at the bottom of a scroll viewport, the thumb's thickness and the least
/// length it is drawn with.
const BAR_TRACK: f32 = 10.0;
const BAR_THUMB: f32 = 6.0;
const BAR_INSET: f32 = 2.0;
const BAR_MIN_THUMB: f32 = 24.0;

/// The thumb at rest, under the pointer, and held.
pub(super) fn bar_ink() -> [Hsla; 3] {
    [
        bezel_theme::ink(0.2),
        bezel_theme::ink(0.32),
        bezel_theme::ink(0.38),
    ]
}

/// A thumb press: which bar it belongs to (every bar in a window hears every drag of this
/// type), where the thumb started and where the pointer was when it was pressed. The
/// pointer's place is taken at the press, not at the first move: a drag is only recognised
/// after the pointer has already travelled.
struct BarDrag {
    owner: EntityId,
    start: Pixels,
    anchor: Rc<Cell<Option<Pixels>>>,
}

/// What the previous frame's layout says about a sideways scroll: the thumb's length, how
/// far it can travel, the content's overflow and the thumb's start. `None` while nothing
/// overflows, which is also when no bar is drawn.
fn bar_geometry(handle: &ScrollHandle) -> Option<(Pixels, Pixels, Pixels, Pixels)> {
    let view = handle.bounds().size.width;
    let reach = handle.max_offset().x;
    if reach <= Pixels::ZERO || view <= Pixels::ZERO {
        return None;
    }
    let length = (view * (view / (view + reach))).max(px(BAR_MIN_THUMB));
    let travel = view - length;
    if travel <= Pixels::ZERO {
        return None;
    }
    let start = travel * (-handle.offset().x / reach).clamp(0.0, 1.0);
    Some((length, travel, reach, start))
}

/// A horizontally scrolling block with a bar. `scroller` is the scrolling container; it is
/// tracked here, and the bar is an overlay beside it, not inside it, because a child would
/// scroll away with the content.
///
/// A component rather than a function so its state is keyed under the element ids above
/// it: two documents on one page would otherwise share one scroll offset.
#[derive(IntoElement)]
pub(super) struct Sideways {
    /// Names this block's scroll state and its bar.
    id: ElementId,
    scroller: Stateful<Div>,
    ink: [Hsla; 3],
    /// This block's place among the body's, for the test selectors and for the report of
    /// where the frame sits.
    ordinal: usize,
    on_bounds: Option<Rc<dyn Fn(usize, Bounds<Pixels>)>>,
}

impl Sideways {
    pub(super) fn new(id: ElementId, scroller: Stateful<Div>, ordinal: usize) -> Self {
        Self {
            id,
            scroller,
            ink: bar_ink(),
            ordinal,
            on_bounds: None,
        }
    }

    /// Told where the frame sits each frame, in window pixels.
    pub(super) fn on_bounds(mut self, report: Rc<dyn Fn(usize, Bounds<Pixels>)>) -> Self {
        self.on_bounds = Some(report);
        self
    }
}

impl RenderOnce for Sideways {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ordinal = self.ordinal;
        let key = |what: &str| ElementId::from((self.id.clone(), what.to_owned()));
        let state = window.use_keyed_state(key("scroll"), cx, |_, _| ScrollHandle::new());
        let owner = state.entity_id();
        let handle = state.read(cx).clone();
        // Outside the frame's elements: the press and the drag it starts can be built by
        // different frames, and each would hold its own cell.
        let grab = window
            .use_keyed_state(key("grab"), cx, |_, _| Rc::new(Cell::new(None)))
            .read(cx)
            .clone();
        let bar = sideways_bar(key("thumb"), ordinal, &handle, owner, grab, self.ink);
        let report = self.on_bounds.clone().map(|report| {
            canvas(move |bounds, _, _| report(ordinal, bounds), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full()
        });
        div()
            .relative()
            .w_full()
            .debug_selector(move || format!("markdown-frame-{ordinal}"))
            .child(self.scroller.track_scroll(&handle))
            .children(report)
            .child(bar)
    }
}

/// The overlay along the bottom of the scroller, sized from the geometry the last frame
/// left. When that shows nothing to scroll it returns a canvas that asks for one more frame
/// if this frame's layout does: the first frame that lays the content out wider than its
/// viewport would otherwise end with no bar and nothing to paint one. Self-limiting, since
/// once the bar is up the canvas is gone.
fn sideways_bar(
    id: ElementId,
    ordinal: usize,
    handle: &ScrollHandle,
    owner: EntityId,
    grab: Rc<Cell<Option<Pixels>>>,
    ink: [Hsla; 3],
) -> AnyElement {
    let Some((length, travel, reach, start)) = bar_geometry(handle) else {
        let watched = handle.clone();
        return canvas(
            move |_, window, _| {
                if bar_geometry(&watched).is_some() {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element();
    };
    let [rest, hover, held] = ink;
    let drag_handle = handle.clone();
    let press = grab.clone();
    let thumb = div()
        .id(id)
        .debug_selector(move || format!("markdown-bar-{ordinal}"))
        .absolute()
        .left(start)
        .bottom(px(BAR_INSET))
        .h(px(BAR_THUMB))
        .w(length)
        .rounded_full()
        .bg(rest)
        .hover(|style| style.bg(hover))
        .active(|style| style.bg(held))
        // The press is the bar's: a body's host starts a text selection from any left
        // press on it. The drag below is registered on this same element and still starts.
        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
            press.set(Some(event.position.x));
            cx.stop_propagation();
        })
        .on_drag(
            BarDrag {
                owner,
                start,
                anchor: grab,
            },
            |_, _, _, cx| cx.new(|_| EmptyView),
        );
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(BAR_TRACK))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag_move(move |event: &DragMoveEvent<BarDrag>, window, cx| {
            let drag = event.drag(cx);
            if drag.owner != owner {
                return;
            }
            let along = event.event.position.x;
            let anchor = drag.anchor.get().unwrap_or(along);
            let ratio = ((drag.start + along - anchor) / travel).clamp(0.0, 1.0);
            let current = drag_handle.offset();
            drag_handle.set_offset(point(-(reach * ratio), current.y));
            window.refresh();
        })
        .child(thumb)
        .into_any_element()
}

/// A scroll container for `content`, wider than the body it sits in when it needs to be.
pub(super) fn scroller(id: ElementId) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .overflow_x_scroll()
        // Without it a scroll down the page turns sideways the moment the pointer crosses
        // the block: gpui remaps input to whichever axis a container can scroll.
        .restrict_scroll_to_axis()
}

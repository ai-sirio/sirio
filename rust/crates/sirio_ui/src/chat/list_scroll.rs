//! bezel's scrollbar, for the transcript's virtualized list.
//!
//! `bezel::ui::scroll::scrollbar` reads a gpui `ScrollHandle`, and the
//! transcript is a `gpui::list` — it has no handle, but it answers the same
//! three numbers (viewport, overflow, offset) through its own
//! `*_for_scrollbar` hooks. This is that bar with the hooks in place of the
//! handle: the geometry (`scroll::thumb`, `scroll::offset_for_thumb`,
//! `MIN_THUMB`) and the drag payload (`ScrollbarDrag`) are bezel's own, so
//! this bar and the thought bodies' agree to the pixel, and the paint is
//! [`ink`] for the same reason bezel's is — a scrollbar is a neutral overlay,
//! not a toned surface.
//!
//! The list freezes its content height for the length of a thumb drag
//! (`scrollbar_drag_started` / `_ended`), so rows measured mid-drag do not
//! pull the thumb out from under the pointer.

use std::{cell::Cell, rc::Rc};

use bezel::motion::Painter;
use bezel::theme::ink;
use bezel::ui::scroll::{self, MIN_THUMB, ScrollbarDrag};
use gpui::{
    AnyElement, App, Context, DragMoveEvent, Empty, FollowMode, ListState, MouseButton, Pixels,
    SharedString, Window, canvas, div, point, prelude::*, px,
};

use super::Chat;

/// The jump-to-latest disc's diameter.
const DISC: f32 = 28.0;

impl Chat {
    /// The "jump to latest" affordance bezel's `FollowState::following`
    /// exists to drive, for a list: nothing while the transcript follows
    /// its tail, a disc floating over the list's bottom edge once the reader
    /// scrolls away. Laid inside the transcript container (which is
    /// `relative`), so it is centred on the prose column rather than the
    /// pane.
    ///
    /// A click asks the list to follow again — `set_follow_mode(Tail)` both
    /// scrolls to the end and re-pins, the same contract the scroll handler
    /// in `Chat::new` restores when a wheel lands back on the tail.
    pub(crate) fn render_jump_to_latest(
        &self,
        bezel_theme: &bezel::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.list_state.is_following_tail() {
            return Empty.into_any_element();
        }
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(8.0))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("chat-jump-latest")
                    .debug_selector(|| "chat-jump-latest".into())
                    .size(px(DISC))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(bezel_theme.bg)
                    .border_1()
                    .border_color(bezel_theme.border)
                    .cursor_pointer()
                    .hover(|s| s.border_color(ink(0.4)))
                    .child(
                        bezel::ui::icons::icon(bezel::ui::icons::ARROW_DOWN)
                            .size(px(14.0))
                            .text_color(bezel_theme.text),
                    )
                    .on_click(cx.listener(|chat, _, _, cx| {
                        chat.list_state.set_follow_mode(FollowMode::Tail);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

/// bezel's own strip and thumb widths (`scroll.rs`, private there).
const TRACK: f32 = 10.0;
const THUMB: f32 = 6.0;

/// Where in the thumb a drag was grabbed — bezel's `ScrollbarState`, whose
/// grab cell is private, re-stated for a list.
#[derive(Clone)]
pub(crate) struct ListScrollbarState {
    grab: Rc<Cell<Option<Pixels>>>,
    /// A drag runs in event-dispatch context, where the window cannot
    /// resolve which view is asking — so the bar carries its own.
    painter: Painter,
}

impl ListScrollbarState {
    pub(crate) fn new(painter: Painter) -> Self {
        Self {
            grab: Rc::new(Cell::new(None)),
            painter,
        }
    }

    /// Whether a thumb drag is in flight.
    pub(crate) fn dragging(&self) -> bool {
        self.grab.get().is_some()
    }

    /// One drag move of the thumb: filter the gesture to this bar's track,
    /// then translate the pointer into a list offset.
    fn drag(
        &self,
        track_id: &SharedString,
        list: &ListState,
        event: &DragMoveEvent<ScrollbarDrag>,
        cx: &mut App,
    ) {
        if event.drag(cx).0 != *track_id {
            return;
        }
        let viewport = list.viewport_bounds().size.height;
        let max_offset = list.max_offset_for_scrollbar().y;
        let offset = list.scroll_px_offset_for_scrollbar().y;
        let Some(range) = scroll::thumb(viewport, max_offset, offset, MIN_THUMB) else {
            return;
        };
        let size = range.end - range.start;
        let pointer = event.event.position.y - event.bounds.top();
        // First move of this drag: the thumb is still where the press
        // landed on it, so the grab is simply the difference. Held for the
        // rest of the gesture, and the list's content height with it.
        let grab = self.grab.get().unwrap_or_else(|| {
            let grab = (pointer - range.start).clamp(px(0.0), size);
            self.grab.set(Some(grab));
            list.scrollbar_drag_started();
            grab
        });
        let offset = scroll::offset_for_thumb(pointer - grab, viewport, max_offset, size);
        list.set_offset_from_scrollbar(point(px(0.0), offset));
        self.painter.notify(cx);
    }
}

/// The bar: an overlay strip along the right edge of whatever it is laid
/// over, `top` below that container's top edge and exactly as tall as the
/// list's viewport — the track *is* the viewport, in the coordinates
/// `scroll::thumb` answers in. Nothing at all when the content fits.
///
/// Its geometry comes from the list as the *last* frame left it, which is
/// all a render pass can see; the canvas at the end asks for one more frame
/// when the list's fresh layout disagrees, so the bar is right on the frame
/// after it first appears rather than whenever something else repaints.
pub(crate) fn list_scrollbar(
    id: impl Into<SharedString>,
    top: Pixels,
    list: &ListState,
    state: &ListScrollbarState,
) -> AnyElement {
    let id = id.into();
    let viewport = list.viewport_bounds().size.height;
    let max_offset = list.max_offset_for_scrollbar().y;
    let offset = list.scroll_px_offset_for_scrollbar().y;
    let Some(range) = scroll::thumb(viewport, max_offset, offset, MIN_THUMB) else {
        return Empty.into_any_element();
    };
    let size = range.end - range.start;
    let dragging = state.dragging();

    let track_id = id.clone();
    let drag_list = list.clone();
    let drag_state = state.clone();
    let release_state = state.clone();
    let release_list = list.clone();
    let released = move |_: &gpui::MouseUpEvent, _: &mut Window, _: &mut App| {
        release_state.grab.set(None);
        release_list.scrollbar_drag_ended();
    };
    let fresh_list = list.clone();

    let track_selector = format!("{id}-track");
    let thumb_selector = format!("{id}-thumb");
    div()
        .id(SharedString::from(format!("{id}-track")))
        .debug_selector(move || track_selector.clone())
        .absolute()
        .top(top)
        .right_0()
        .h(viewport)
        .w(px(TRACK))
        .flex()
        .justify_center()
        .on_drag_move(move |event, _, cx| {
            drag_state.drag(&track_id, &drag_list, event, cx);
        })
        // Both, because a release can land anywhere on screen; a grab left
        // set would make the next press continue the last gesture.
        .on_mouse_up(MouseButton::Left, released.clone())
        .on_mouse_up_out(MouseButton::Left, released)
        .child(
            div()
                .id(SharedString::from(format!("{id}-thumb")))
                .debug_selector(move || thumb_selector.clone())
                .absolute()
                .top(range.start)
                .h(size)
                .w(px(THUMB))
                .rounded_full()
                .bg(if dragging { ink(0.38) } else { ink(0.2) })
                .hover(|s| s.bg(ink(0.32)))
                .on_drag(ScrollbarDrag(id.clone()), |_, _, _, cx| cx.new(|_| Empty)),
        )
        .child(
            canvas(
                move |_, window, _| {
                    let fresh = fresh_list.viewport_bounds().size.height;
                    if (fresh - viewport).abs() > px(0.5) {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        )
        .into_any_element()
}

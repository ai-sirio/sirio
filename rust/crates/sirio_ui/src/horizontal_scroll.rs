use bezel::theme::ink;
use bezel::ui::scroll::{self, ScrollbarDrag};
use gpui::{
    AnyElement, App, Empty, MouseButton, Pixels, SharedString, Window, div, prelude::*, px,
};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(crate) struct HorizontalBarState {
    grab: Rc<Cell<Option<Pixels>>>,
}

fn horizontal_range(viewport: Pixels, overflow: Pixels, offset: Pixels) -> Option<Range<Pixels>> {
    scroll::thumb(viewport, overflow, offset, scroll::MIN_THUMB)
}

pub(crate) fn bar(
    id: &'static str,
    viewport: Pixels,
    overflow: Pixels,
    offset: Pixels,
    state: &HorizontalBarState,
    on_offset: impl Fn(Pixels, &mut App) + Clone + 'static,
) -> AnyElement {
    let Some(range) = horizontal_range(viewport, overflow, offset) else {
        return Empty.into_any_element();
    };
    let thumb_width = range.end - range.start;
    let track_id = SharedString::from(id);
    let drag_state = state.clone();
    let release_state = state.clone();
    let released = move |_: &gpui::MouseUpEvent, _: &mut Window, _: &mut App| {
        release_state.grab.set(None);
    };
    let drag_offset = on_offset.clone();

    div()
        .id(SharedString::from(format!("{id}-track")))
        .debug_selector(move || format!("{id}-track"))
        .absolute()
        .h(px(10.0))
        .left_0()
        .right_0()
        .bottom_0()
        .on_drag_move(move |event: &gpui::DragMoveEvent<ScrollbarDrag>, _, cx| {
            if event.drag(cx).0 != track_id {
                return;
            }
            let pointer = event.event.position.x - event.bounds.left();
            let grab = drag_state.grab.get().unwrap_or_else(|| {
                let grab = (pointer - range.start).clamp(px(0.), thumb_width);
                drag_state.grab.set(Some(grab));
                grab
            });
            drag_offset(
                scroll::offset_for_thumb(pointer - grab, viewport, overflow, thumb_width),
                cx,
            );
        })
        .on_mouse_up(MouseButton::Left, released.clone())
        .on_mouse_up_out(MouseButton::Left, released)
        .child(
            div()
                .id(SharedString::from(format!("{id}-thumb")))
                .debug_selector(move || format!("{id}-thumb"))
                .absolute()
                .left(range.start)
                .w(thumb_width)
                .h_full()
                .rounded_full()
                .bg(ink(0.2))
                .hover(|style| style.bg(ink(0.32)))
                .on_drag(ScrollbarDrag(id.into()), |_, _, _, cx| {
                    cx.new(|_| gpui::Empty)
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Context, IntoElement, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext,
        size,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn thumb_geometry_is_horizontal_and_clamped() {
        let range = horizontal_range(px(100.), px(100.), px(-50.)).unwrap();
        assert_eq!(
            bezel::ui::scroll::offset_for_thumb(
                range.start,
                px(100.),
                px(100.),
                range.end - range.start,
            ),
            px(-50.)
        );
        assert_eq!(
            bezel::ui::scroll::offset_for_thumb(
                px(999.),
                px(100.),
                px(100.),
                range.end - range.start,
            ),
            px(-100.)
        );
        assert!(horizontal_range(px(100.), px(0.), px(0.)).is_none());
    }

    struct BarHarness {
        overflow: Pixels,
        state: HorizontalBarState,
        observed: Rc<Cell<Option<Pixels>>>,
    }

    impl Render for BarHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let observed = self.observed.clone();
            div()
                .id("horizontal-test-root")
                .size_full()
                .relative()
                .child(bar(
                    "horizontal-test",
                    px(200.),
                    self.overflow,
                    px(0.),
                    &self.state,
                    move |offset, _| observed.set(Some(offset)),
                ))
        }
    }

    #[gpui::test]
    fn drawn_bar_hides_without_overflow_and_drag_clamps_to_end(cx: &mut TestAppContext) {
        let observed = Rc::new(Cell::new(None));
        let observed_for_window = observed.clone();
        let window = cx.add_window(move |_, _| BarHarness {
            overflow: px(200.),
            state: HorizontalBarState::default(),
            observed: observed_for_window,
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(200.), px(100.)));
        cx.run_until_parked();

        let track = cx
            .debug_bounds("horizontal-test-track")
            .expect("overflow draws a horizontal track");
        let thumb = cx
            .debug_bounds("horizontal-test-thumb")
            .expect("overflow draws a horizontal thumb");
        assert!(thumb.size.width < track.size.width);

        cx.simulate_mouse_move(thumb.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(thumb.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(thumb.center().x + px(8.), thumb.center().y),
            MouseButton::Left,
            Modifiers::none(),
        );
        let end = gpui::point(track.right() + px(50.), thumb.center().y);
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(observed.get(), Some(px(-200.)));

        let window = cx.add_window(|_, _| BarHarness {
            overflow: px(0.),
            state: HorizontalBarState::default(),
            observed: Rc::new(Cell::new(None)),
        });
        let mut no_overflow = VisualTestContext::from_window(window.into(), &cx.cx);
        no_overflow.run_until_parked();
        assert!(no_overflow.debug_bounds("horizontal-test-track").is_none());
    }
}

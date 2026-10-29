//! Views the example probes draw, kept here because they reach `pub(crate)` pieces an example
//! cannot: the transcript's list scrollbar and the horizontal bar.
//!
//! This is not application code. Nothing in the app links it, and it exists so that
//! `Scripts/Tests/test-scroll-drag-probe.sh` can drive the three scrollbar implementations the
//! app uses in one window, before and after a PR changes what draws them.

use gpui::{
    AnyElement, AnyView, App, Context, ListAlignment, ListState, Render, ScrollHandle, Window,
    canvas, div, list, point, prelude::*, px,
};
use sirio_theme::Theme;

use crate::chat::list_scroll::{ListScrollbarState, list_scrollbar};
use crate::file_view;
use crate::horizontal_scroll::{self, HorizontalBarState};
use crate::motion::Painter;

/// Width of each of the three panes. The window is `3 * PANE_WIDTH` wide and the panes fill its
/// height, side by side from the left edge with nothing between them. The script reads this
/// layout: pane `n` (1-based) starts at `x = (n - 1) * PANE_WIDTH`.
pub const PANE_WIDTH: f32 = 300.0;
/// What the horizontal pane's body is wide: eight panes, so the thumb has room to travel.
const WIDE_BODY: f32 = 8.0 * PANE_WIDTH;
const ROW_HEIGHT: f32 = 20.0;
/// Rows of the two vertical panes: 4000 px of content under a 600 px viewport.
const ROWS: usize = 200;
/// Rows of the horizontal pane. Few enough that the bottom strip the bar lies over is empty, so
/// the script's scan of that row sees the thumb and nothing else.
const WIDE_ROWS: usize = 20;
/// Kept clear of text on the bar's side, so the scan down a vertical bar's centre line sees the
/// thumb and nothing else.
const BAR_CLEARANCE: f32 = 16.0;

/// Three panes side by side, each over long fixture text and each with its own scrollbar:
///
/// 1. a `ScrollHandle` body with [`file_view::scrollbar`], the file view's and the CI log's bar;
/// 2. a `ListState` body with [`list_scrollbar`], the chat transcript's bar;
/// 3. a wide body with [`horizontal_scroll::bar`], the diff and Preview's bar along the bottom.
///
/// Each bar sits under the debug selector `probe-bar-<n>`. Later tasks change what draws the
/// bars, never this signature.
pub fn scroll_drag_view(_window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(ScrollDragProbe::new).into()
}

struct ScrollDragProbe {
    handle: ScrollHandle,
    list: ListState,
    list_bar: ListScrollbarState,
    wide_handle: ScrollHandle,
    wide_bar: HorizontalBarState,
}

impl ScrollDragProbe {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            handle: ScrollHandle::new(),
            list: ListState::new(ROWS, ListAlignment::Top, px(2048.0)),
            list_bar: ListScrollbarState::new(Painter::of(cx)),
            wide_handle: ScrollHandle::new(),
            wide_bar: HorizontalBarState::default(),
        }
    }
}

fn row(text: String) -> AnyElement {
    div()
        .h(px(ROW_HEIGHT))
        .flex_none()
        .whitespace_nowrap()
        .overflow_hidden()
        .child(text)
        .into_any_element()
}

fn pane() -> gpui::Div {
    div().relative().w(px(PANE_WIDTH)).h_full().flex_none()
}

/// A bar's overlay, carrying the selector `probe-bar-<n>` over the whole pane. The bars each
/// lay themselves along an edge of the nearest positioned ancestor, which this is.
fn bar_overlay(number: usize, bar: AnyElement) -> AnyElement {
    let selector = format!("probe-bar-{number}");
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .debug_selector(move || selector.clone())
        .child(bar)
        .into_any_element()
}

impl Render for ScrollDragProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);

        let plain = pane().child(
            div()
                .id("probe-scroll-1")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.handle)
                .pr(px(BAR_CLEARANCE))
                .children((0..ROWS).map(|ix| row(format!("pane 1 row {ix:04} scroll handle body")))),
        );
        // `file_view::scrollbar` tags its own full-pane overlay with the id it is given; that is
        // not the selector the probe promises, which every bar gets the same way, from the probe.
        let plain = plain.child(bar_overlay(
            1,
            file_view::scrollbar("probe-file-bar", &self.handle),
        ));

        let listed = pane()
            .child(
                div().size_full().pr(px(BAR_CLEARANCE)).child(
                    list(self.list.clone(), move |ix, _, _| {
                        row(format!("pane 2 row {ix:04} list state body"))
                    })
                    .size_full(),
                ),
            )
            .child(bar_overlay(
                2,
                list_scrollbar("probe-bar-2", px(0.0), &self.list, &self.list_bar),
            ));

        let wide = pane()
            .child(
                div()
                    .id("probe-scroll-3")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.wide_handle)
                    .child(
                        div()
                            .w(px(WIDE_BODY))
                            .flex_none()
                            .children((0..WIDE_ROWS).map(|ix| {
                                row(format!("pane 3 row {ix:02} wide body {}", "-".repeat(380)))
                            })),
                    ),
            )
            .child(bar_overlay(3, self.wide_bar_element(cx)));

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(theme.ely.bg)
            .text_color(theme.ely.fg)
            .text_size(px(13.0))
            .child(plain)
            .child(listed)
            .child(wide)
    }
}

impl ScrollDragProbe {
    /// `horizontal_scroll::bar` over the wide pane's handle. The handle reports no viewport on
    /// the first frame, so while the bar has nothing to draw a canvas asks for the frame that
    /// does, as `file_view::horizontal_scrollbar` does for the surfaces that use it.
    fn wide_bar_element(&self, cx: &mut Context<Self>) -> AnyElement {
        let handle = &self.wide_handle;
        let overflowing = |handle: &ScrollHandle| {
            handle.bounds().size.width > px(0.0) && handle.max_offset().x > px(0.0)
        };
        if !overflowing(handle) {
            let watched = handle.clone();
            return canvas(
                move |_, window, _| {
                    if overflowing(&watched) {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .into_any_element();
        }
        let entity = cx.entity();
        let drag_handle = handle.clone();
        horizontal_scroll::bar(
            "probe-bar-3",
            handle.bounds().size.width,
            handle.max_offset().x,
            handle.offset().x,
            &self.wide_bar,
            move |x, cx| {
                drag_handle.set_offset(point(x, drag_handle.offset().y));
                entity.update(cx, |_, cx| cx.notify());
            },
        )
    }
}

//! Shared blinking text-caret state and rendering helpers.
//!
//! The Rust port renders every editable surface with custom `StyledText`
//! elements (gpui draws no caret on its own — the Swift app got one for free
//! from UITextView/NSTextView). This module is the one place that knows how
//! a caret blinks: each editable surface owns a [`Blink`], wakes it when the
//! user interacts, asks it whether the bar is currently visible while
//! rendering, and calls [`schedule`] once per render so exactly one toggle
//! timer is ever outstanding per surface.

use std::time::Duration;

use gpui::{Context, Div, IntoElement, ParentElement, Pixels, Rgba, Styled, div, px};

/// Blink cadence. 530ms matches the conventional text-caret cycle closely
/// enough that nobody perceives the difference from a native field.
pub const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// The caret bar's width, in pixels.
pub const BAR_WIDTH: Pixels = px(2.0);

/// Per-surface blink state: whether the bar would be painted this frame,
/// and whether a toggle timer is already in flight.
pub struct Blink {
    visible: bool,
    pending: bool,
}

impl Default for Blink {
    fn default() -> Self {
        Self::new()
    }
}

impl Blink {
    pub fn new() -> Self {
        Self {
            visible: true,
            pending: false,
        }
    }

    /// Show the bar immediately — call whenever the user edits or moves the
    /// caret, or gains focus, so the bar never blinks off mid-interaction.
    pub fn wake(&mut self) {
        self.visible = true;
    }

    /// Whether the bar should be painted this frame.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Consume one timer tick: flip visibility and allow the next schedule.
    pub fn flip(&mut self) {
        self.visible = !self.visible;
        self.pending = false;
    }
}

/// Arms this frame's blink-toggle timer for `entity`, if one isn't already
/// outstanding. Call once per render while the surface may show a caret:
///
/// * unfocused → the bar is forced visible (irrelevant, nothing paints it)
///   and any pending timer is forgotten;
/// * focused → at most one timer exists; when it fires it invokes `on_flip`,
///   which must call `Blink::flip` on that surface's state and notify.
pub fn schedule<T: 'static>(
    blink: &mut Blink,
    focused: bool,
    on_flip: fn(&mut T, &mut Context<T>),
    cx: &mut Context<T>,
) {
    if !focused {
        blink.visible = true;
        blink.pending = false;
        return;
    }
    if blink.pending {
        return;
    }
    blink.pending = true;
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(BLINK_INTERVAL).await;
        let _ = this.update(cx, on_flip);
    })
    .detach();
}

/// The inline caret bar for single-line / flex-row fields: a fixed-width
/// rounded quad whose background appears only while `visible`. Always
/// occupying layout so surrounding text doesn't shift as it blinks, and
/// never shrinking: in a row whose value overflows, flex shrink would take
/// its share out of the bar too and leave a sliver.
pub fn bar(height: Pixels, color: Rgba, visible: bool) -> gpui::AnyElement {
    let bar = gpui::div()
        .w(BAR_WIDTH)
        .h(height)
        .flex_shrink_0()
        .rounded(px(1.0));
    if visible {
        bar.bg(color).into_any_element()
    } else {
        bar.into_any_element()
    }
}

/// The value of a single-line, append-only field — the kind [`bar`] ends.
///
/// Once the value outgrows the field it is clipped from the *start*, so the
/// tail the user is typing into stays in view with the bar flush against
/// its last character, the way a native text field scrolls. The earlier
/// `text_ellipsis()` idiom truncated the *end* — precisely the characters
/// being typed — and gpui's truncation sums per-glyph advances, so the bar
/// drifted a glyph away from the last visible character.
///
/// The parent must be `overflow_hidden` (#212) and lay this out as a flex
/// item: the wrapper shrinks (`min_w_0`) but never grows, so the bar that
/// follows it never gets pushed to the far edge of an empty field; the run
/// inside refuses to shrink and is packed against the end, which is where
/// Taffy puts the overflow of a `justify_end` row — at the start, out of
/// view.
pub fn field_value(text: impl IntoElement) -> Div {
    div()
        .min_w_0()
        .flex()
        .justify_end()
        .overflow_hidden()
        .child(div().whitespace_nowrap().flex_shrink_0().child(text))
}

/// The placeholder of an empty single-line field — the hint that stands in
/// for a value until one is typed.
///
/// Laid out in the same slot as [`field_value`] (a shrinking, never growing
/// flex item, so the bar still follows its last character), but clipped at
/// the *end*, with an ellipsis. Nobody is typing into a placeholder, so the
/// tail is not where the eye is; the start carries the meaning — "base
/// branch (optional, …)", "location (optional, …)" — and a hint read from
/// its middle ("branch (optional, defaults to HEAD)") describes the wrong
/// thing. Routing a placeholder through [`field_value`] did exactly that:
/// its start-clipping is right for a value and wrong for a hint.
pub fn field_placeholder(text: impl IntoElement) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}

#[cfg(test)]
mod tests {
    use gpui::{
        Bounds, Context, InteractiveElement, Render, Styled, TestAppContext, VisualTestContext,
        Window, black, size,
    };

    use super::*;

    /// A field the value has outgrown, laid out the way every single-line
    /// field lays out [`field_value`]: clipped container, the value, the bar.
    struct OverflowingField {
        value: String,
    }

    impl Render for OverflowingField {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .debug_selector(|| "field".to_owned())
                    .w(px(120.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        field_value(
                            div()
                                .debug_selector(|| "run".to_owned())
                                .child(self.value.clone()),
                        )
                        .debug_selector(|| "value".to_owned()),
                    )
                    .child(div().debug_selector(|| "bar".to_owned()).child(bar(
                        px(16.0),
                        black().into(),
                        true,
                    ))),
            )
        }
    }

    fn field(cx: &mut TestAppContext, value: &str) -> VisualTestContext {
        let value = value.to_owned();
        let window = cx.open_window(size(px(400.0), px(100.0)), move |_, _| OverflowingField {
            value,
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, _| window.refresh());
        cx.cx.run_until_parked();
        cx
    }

    fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("`{selector}` is drawn"))
    }

    /// The value is clipped from the *start*: its tail — the characters the
    /// user is typing — stays inside the field with the bar flush against
    /// it. `text_ellipsis()` did the opposite, truncating the end and
    /// leaving the bar a glyph's width away from the last visible one.
    #[gpui::test]
    async fn an_overflowing_value_keeps_its_tail_in_view_with_the_bar_flush(
        cx: &mut TestAppContext,
    ) {
        let mut cx = field(cx, &"x".repeat(80));
        let field = bounds(&mut cx, "field");
        let value = bounds(&mut cx, "value");
        let run = bounds(&mut cx, "run");
        let bar = bounds(&mut cx, "bar");

        assert!(
            run.size.width > field.size.width,
            "the whole value is laid out, not truncated to fit: run={run:?} field={field:?}"
        );
        assert!(
            run.origin.x < field.origin.x,
            "the overflow is at the start, out of view: run={run:?} field={field:?}"
        );
        assert_eq!(
            run.origin.x + run.size.width,
            value.origin.x + value.size.width,
            "the tail of the value is the visible end: run={run:?} value={value:?}"
        );
        assert_eq!(
            bar.origin.x,
            value.origin.x + value.size.width,
            "the bar is flush against the visible tail: bar={bar:?} value={value:?}"
        );
        assert_eq!(
            bar.size.width, BAR_WIDTH,
            "flex shrink never takes its share out of the bar: bar={bar:?}"
        );
        assert!(
            bar.origin.x + bar.size.width <= field.origin.x + field.size.width,
            "the bar stays inside the field: bar={bar:?} field={field:?}"
        );
    }

    /// A short value is not pushed to the far edge of the field: the wrapper
    /// shrinks but never grows, so the bar follows the last character.
    #[gpui::test]
    async fn a_short_value_keeps_the_bar_at_its_end_not_the_fields(cx: &mut TestAppContext) {
        let mut cx = field(cx, "ab");
        let field = bounds(&mut cx, "field");
        let run = bounds(&mut cx, "run");
        let bar = bounds(&mut cx, "bar");

        assert_eq!(
            run.origin.x, field.origin.x,
            "the value starts at the field's start"
        );
        assert_eq!(
            bar.origin.x,
            run.origin.x + run.size.width,
            "the bar touches the last character: bar={bar:?} run={run:?}"
        );
        assert!(
            bar.origin.x + bar.size.width < field.origin.x + field.size.width / 2.0,
            "the bar is not pushed to the far edge of the field: bar={bar:?} field={field:?}"
        );
    }

    /// An empty field whose placeholder is longer than the field, laid out
    /// the way every single-line field lays out [`field_placeholder`]:
    /// clipped container, the hint, the bar.
    struct OverflowingPlaceholder {
        placeholder: String,
    }

    impl Render for OverflowingPlaceholder {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .debug_selector(|| "field".to_owned())
                    .w(px(120.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        field_placeholder(
                            div()
                                .debug_selector(|| "run".to_owned())
                                .child(self.placeholder.clone()),
                        )
                        .debug_selector(|| "placeholder".to_owned()),
                    )
                    .child(div().debug_selector(|| "bar".to_owned()).child(bar(
                        px(16.0),
                        black().into(),
                        true,
                    ))),
            )
        }
    }

    fn placeholder_field(cx: &mut TestAppContext, placeholder: &str) -> VisualTestContext {
        let placeholder = placeholder.to_owned();
        let window = cx.open_window(size(px(400.0), px(100.0)), move |_, _| {
            OverflowingPlaceholder { placeholder }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, _| window.refresh());
        cx.cx.run_until_parked();
        cx
    }

    /// A placeholder is clipped from the *end*: its start — the words that
    /// say what the field is for — stays in view. Routed through
    /// [`field_value`] it was clipped from the start instead, and "location
    /// (optional, defaults next to project)" read as "ı (optional, defaults
    /// next to project)".
    #[gpui::test]
    async fn an_overflowing_placeholder_keeps_its_start_in_view(cx: &mut TestAppContext) {
        let mut cx = placeholder_field(cx, "location (optional, defaults next to project)");
        let field = bounds(&mut cx, "field");
        let run = bounds(&mut cx, "run");
        let bar = bounds(&mut cx, "bar");

        assert!(
            run.origin.x >= field.origin.x,
            "the start of the hint is in view: run={run:?} field={field:?}"
        );
        assert!(
            run.origin.x + run.size.width <= field.origin.x + field.size.width,
            "the hint is truncated to the field, not laid out past it: run={run:?} field={field:?}"
        );
        assert!(
            bar.origin.x + bar.size.width <= field.origin.x + field.size.width,
            "the bar stays inside the field: bar={bar:?} field={field:?}"
        );
    }

    /// A short placeholder behaves like a short value: it starts at the
    /// field's start and the bar follows its last character rather than
    /// being pushed to the far edge.
    #[gpui::test]
    async fn a_short_placeholder_keeps_the_bar_at_its_end_not_the_fields(cx: &mut TestAppContext) {
        let mut cx = placeholder_field(cx, "ab");
        let field = bounds(&mut cx, "field");
        let run = bounds(&mut cx, "run");
        let bar = bounds(&mut cx, "bar");

        assert_eq!(
            run.origin.x, field.origin.x,
            "the hint starts at the field's start"
        );
        assert_eq!(
            bar.origin.x,
            run.origin.x + run.size.width,
            "the bar touches the last character: bar={bar:?} run={run:?}"
        );
        assert!(
            bar.origin.x + bar.size.width < field.origin.x + field.size.width / 2.0,
            "the bar is not pushed to the far edge of the field: bar={bar:?} field={field:?}"
        );
    }
}

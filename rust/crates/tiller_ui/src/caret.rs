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

use gpui::{px, Context, Pixels, Rgba, IntoElement, Styled};

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
/// occupying layout so surrounding text doesn't shift as it blinks.
pub fn bar(height: Pixels, color: Rgba, visible: bool) -> gpui::AnyElement {
    let bar = gpui::div().w(BAR_WIDTH).h(height).rounded(px(1.0));
    if visible {
        bar.bg(color).into_any_element()
    } else {
        bar.into_any_element()
    }
}

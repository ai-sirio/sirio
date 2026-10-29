// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The reasoning entry as the gallery's Activity page draws it: an orb while
//! it streams and a disclosure chevron once it settles, `Thinking` /
//! `Thought for Ns`, and a body pinned to its newest line —
//! `crabtalk/bezel` tag `v0.1.4`, `apps/gallery/src/patterns/agent.rs`
//! (`Activity`). Whether the body is open is [`Takeover`]: it follows
//! the run until the reader presses the header, then obeys the reader.

#[cfg(test)]
use std::time::Duration;

use gpui::{AnyElement, ScrollHandle, div, linear_color_stop, linear_gradient, prelude::*, px};
use sirio_theme::Theme;

use crate::scroll::FollowState;

#[cfg(test)]
use crate::loading;

use super::{Chat, Entry, TranscriptInteraction, tool_calls::tool_output_consumes_scroll};

/// The header's word: `Thinking` while the thought streams, then
/// `Thought for Ns` when this process measured it, or a bare `Thought` for a
/// thought restored from a database that never knew.
#[cfg(test)]
pub(crate) fn thought_header_label(streaming: bool, duration_ms: Option<u64>) -> String {
    if streaming {
        "Thinking".to_string()
    } else {
        loading::thought_label(duration_ms.map(Duration::from_millis))
    }
}

/// A flag that follows something else until the user takes it over.
///
/// The rule behind a section that opens itself while work streams in and
/// collapses when it stops: auto-follow is right until the first press, and
/// wrong immediately after — whatever the flag does next, the person who
/// clicked has to win.
///
/// It is an `Option<bool>` rather than the two flags it reads as (*touched*,
/// plus the value): "untouched, and here is the manual value" is a state that
/// cannot mean anything, and this way it cannot be written.
///
/// Paint `get(running)`; on the header's click, `toggle(running)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Takeover(Option<bool>);

impl Takeover {
    /// What to show: `auto` until the first [`Self::toggle`], the user's own
    /// choice from then on.
    pub(crate) fn get(self, auto: bool) -> bool {
        self.0.unwrap_or(auto)
    }

    /// Flip what is currently on screen — which while nobody has touched it is
    /// `auto`, *not* the stored value — and take over from here.
    pub(crate) fn toggle(&mut self, auto: bool) {
        self.0 = Some(!self.get(auto));
    }
}

/// Per-entry scroll state for an open body: the handle the well tracks and
/// the follow pin. Not persisted; created the first time a thought's body is
/// drawn.
pub(crate) struct ThoughtScroll {
    pub scroll: ScrollHandle,
    pub follow: FollowState,
}

impl ThoughtScroll {
    pub(crate) fn new() -> Self {
        Self {
            scroll: ScrollHandle::new(),
            follow: FollowState::new(),
        }
    }

    fn follow_element(&self) -> AnyElement {
        let follow = crate::scroll::follow(&self.scroll, &self.follow);
        if !sirio_perf::enabled() {
            return follow;
        }
        // Observe `scroll::follow`'s actual prepaint correction, not a
        // prediction made from the previous layout. It requests an animation
        // frame in the same branch that changes this offset. Do not add a
        // callback or a notification of our own to observe it.
        let before = std::rc::Rc::new(std::cell::Cell::new(None));
        let capture = before.clone();
        let before_scroll = self.scroll.clone();
        let after_scroll = self.scroll.clone();
        div()
            .absolute()
            .size_full()
            .child(
                gpui::canvas(
                    move |_, _, _| capture.set(Some(before_scroll.offset())),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
            )
            .child(follow)
            .child(
                gpui::canvas(
                    move |_, window, _| {
                        if before
                            .get()
                            .is_some_and(|offset| offset != after_scroll.offset())
                        {
                            sirio_perf::event(
                                "request_frame.Chat.thought_follow",
                                window.current_view().as_u64(),
                            );
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_0(),
            )
            .into_any_element()
    }
}

/// The gallery's `LiveActivity.svelte` cap and mask.
const BOX_MAX: f32 = 160.0;
const FADE: f32 = 20.0;

impl Chat {
    /// The gallery's `Activity::reasoning`: a border line down the left, a
    /// well capped at 160 px that scrolls past that, the 20 px fade painted
    /// over its top edge, the follow pin and the scrollbar laid over the
    /// same container.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_thought_body(
        entry_index: usize,
        text: &str,
        source_start: usize,
        interaction: &TranscriptInteraction,
        scroll: &ThoughtScroll,
        theme: &Theme,
        bezel_theme: &bezel_theme::Theme,
    ) -> AnyElement {
        let _perf = sirio_perf::span("Chat.render_thought_body", entry_index as u64);
        let typography = theme.typography;
        div()
            .id(("thought-body", entry_index))
            .debug_selector(move || format!("thought-body-{entry_index}"))
            .ml(px(10.0))
            .pl(px(12.0))
            .border_l_1()
            .border_color(bezel_theme.border)
            .child(
                div()
                    .relative()
                    .max_h(px(BOX_MAX))
                    .child(
                        div()
                            .id(("thought-well", entry_index))
                            .debug_selector(move || format!("thought-well-{entry_index}"))
                            .max_h(px(BOX_MAX))
                            .overflow_y_scroll()
                            .track_scroll(&scroll.scroll)
                            .on_scroll_wheel({
                                let handle = scroll.scroll.clone();
                                move |event, _, cx| {
                                    let delta_y = event.delta.pixel_delta(px(20.0)).y;
                                    if tool_output_consumes_scroll(
                                        handle.offset().y,
                                        handle.max_offset().y,
                                        delta_y,
                                    ) {
                                        cx.stop_propagation();
                                    }
                                }
                            })
                            .pr(px(14.0))
                            .text_size(typography.callout)
                            .line_height(px(19.0))
                            .text_color(theme.ely.fg_muted.opacity(0.7))
                            .child(Self::render_plain_text(
                                text.to_string(),
                                theme,
                                format!("thought-entry-{entry_index}"),
                                source_start,
                                Some(interaction),
                            )),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("thought-fade-{entry_index}"))
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(FADE))
                            .bg(linear_gradient(
                                180.0,
                                linear_color_stop(bezel_theme.bg, 0.0),
                                linear_color_stop(bezel_theme.bg.opacity(0.0), 1.0),
                            )),
                    )
                    .child(scroll.follow_element())
                    .child(crate::file_view::scrollbar(
                        format!("thought-bar-{entry_index}"),
                        &scroll.scroll,
                    )),
            )
            .into_any_element()
    }

    /// A thought streams while the turn does and nothing has settled it.
    pub(crate) fn thought_is_streaming(&self, index: usize) -> bool {
        self.streaming
            && matches!(
                self.entries.get(index),
                Some(Entry::Thought {
                    started: Some(_),
                    duration_ms: None,
                    ..
                })
            )
    }

    /// Settles the thought at the tail (notices aside), if one is still open:
    /// stores its elapsed time and remeasures the row (the header's word and
    /// the body's auto-open both change). Called before any entry other than
    /// a thought or a notice is pushed, and when the turn ends.
    pub(crate) fn settle_open_thought(&mut self) {
        let index = self.streamed_entry();
        if let Some(index) = index
            && let Some(Entry::Thought {
                started: Some(started),
                duration_ms,
                ..
            }) = self.entries.get_mut(index)
            && duration_ms.is_none()
        {
            *duration_ms = Some(started.elapsed().as_millis() as u64);
            self.remeasure_entry(index);
        }
    }

    /// The reader presses the header: flip what is on screen and hold it
    /// (`Takeover::toggle` against the current auto state).
    pub(crate) fn toggle_thought(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        let auto = self.thought_is_streaming(index);
        if let Some(Entry::Thought { open, .. }) = self.entries.get_mut(index) {
            open.toggle(auto);
            self.remeasure_entry(index);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_follows_the_state() {
        assert_eq!(thought_header_label(true, None), "Thinking");
        assert_eq!(thought_header_label(true, Some(4_000)), "Thinking");
        assert_eq!(thought_header_label(false, Some(4_000)), "Thought for 4s");
        assert_eq!(thought_header_label(false, Some(450)), "Thought for 0s");
        assert_eq!(thought_header_label(false, None), "Thought");
    }
}

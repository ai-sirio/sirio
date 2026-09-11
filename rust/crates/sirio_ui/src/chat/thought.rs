//! The reasoning entry as the gallery's Activity page draws it: an orb while
//! it streams and a disclosure chevron once it settles, `Thinking` /
//! `Thought for Ns`, and a body pinned to its newest line —
//! `crabtalk/bezel` tag `v0.1.4`, `apps/gallery/src/patterns/agent.rs`
//! (`Activity`). Whether the body is open is `widgets::Takeover`: it follows
//! the run until the reader presses the header, then obeys the reader.

use std::time::Duration;

use bezel::motion::Painter;
use bezel::ui::scroll;
use bezel::ui::scroll::{FollowState, ScrollbarState};
use bezel::ui::widgets::Layout as _;
use gpui::{
    AnyElement, App, Entity, ScrollHandle, Window, div, linear_color_stop, linear_gradient,
    prelude::*, px,
};
use sirio_theme::Theme;

use crate::loading;

use super::{Chat, Entry, TranscriptInteraction};

/// The header's word: `Thinking` while the thought streams, then
/// `Thought for Ns` when this process measured it, or a bare `Thought` for a
/// thought restored from a database that never knew.
pub(crate) fn thought_header_label(streaming: bool, duration_ms: Option<u64>) -> String {
    if streaming {
        "Thinking".to_string()
    } else {
        loading::thought_label(duration_ms.map(Duration::from_millis))
    }
}

/// Per-entry scroll state for an open body: the handle the well tracks, the
/// follow pin, and the scrollbar's own state. Not persisted; created the
/// first time a thought's body is drawn.
pub(crate) struct ThoughtScroll {
    pub scroll: ScrollHandle,
    pub follow: FollowState,
    pub bar: ScrollbarState,
}

impl ThoughtScroll {
    pub(crate) fn new(painter: Painter) -> Self {
        Self {
            scroll: ScrollHandle::new(),
            follow: FollowState::new(),
            bar: ScrollbarState::new(painter),
        }
    }

    fn follow_element(&self) -> AnyElement {
        let follow = scroll::follow(&self.scroll, &self.follow);
        if !sirio_perf::enabled() {
            return follow;
        }
        // Observe Bezel's actual prepaint correction, not a prediction made
        // from the previous layout. Bezel 0.1.4 requests an animation frame
        // in the same branch that changes this offset. Do not add a callback
        // or a notification of our own to observe it.
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

impl Chat {
    /// The gallery's `Activity::header`: a 14 px glyph slot — the orb
    /// cluster while the thought streams, the disclosure chevron once it
    /// settles — and the state's word. Clickable when it belongs to an entry;
    /// the generating spinner (#239) draws the same row without a click.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_thought_header(
        entry_index: usize,
        streaming: bool,
        open: bool,
        duration_ms: Option<u64>,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        window: &mut Window,
        cx: &mut App,
        entity: Option<Entity<Chat>>,
    ) -> AnyElement {
        // Built outside the row: gpui reads an svg's colour off its own
        // element, so a chevron tinted by its parent paints nothing.
        let glyph: AnyElement = if streaming {
            loading::thinking_indicator("thought-orb", theme, window, cx)
        } else {
            bezel_theme.disclosure(open).into_any_element()
        };
        let label = thought_header_label(streaming, duration_ms);
        let mut row = div()
            .id(("thought-toggle", entry_index))
            .debug_selector(move || format!("thought-toggle-{entry_index}"))
            .self_start()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(4.0))
            .py(px(5.0))
            .rounded(px(bezel::theme::Theme::control_radius()))
            .cursor_pointer()
            .hover(|s| s.bg(bezel::theme::ink(0.03)))
            .child(
                div()
                    .flex()
                    .w(px(loading::THINKING_GLYPH))
                    .justify_center()
                    .child(glyph),
            )
            .child(
                div()
                    .text_size(theme.typography.callout)
                    .line_height(px(19.0))
                    .text_color(theme.text_muted)
                    .child(label),
            )
            .child(div().size_0().debug_selector(move || {
                if streaming {
                    format!("thought-streaming-{entry_index}")
                } else {
                    format!("thought-settled-{entry_index}")
                }
            }))
            .when(duration_ms.is_some() && !streaming, |row| {
                row.child(
                    div()
                        .size_0()
                        .debug_selector(move || format!("thought-took-{entry_index}")),
                )
            });
        if let Some(entity) = entity {
            row = row.on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| chat.toggle_thought(entry_index, cx));
            });
        }
        row.into_any_element()
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
        bezel_theme: &bezel::theme::Theme,
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
                            .pr(px(14.0))
                            .text_size(typography.callout)
                            .line_height(px(19.0))
                            .text_color(theme.text_muted.opacity(0.7))
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
                    .child(scroll::scrollbar(
                        format!("thought-bar-{entry_index}"),
                        &scroll.scroll,
                        &scroll.bar,
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

    /// Settles the thought at the tail, if one is still open: stores its
    /// elapsed time and remeasures the row (the header's word and the body's
    /// auto-open both change). Called before any non-thought entry is pushed
    /// and when the turn ends.
    pub(crate) fn settle_open_thought(&mut self) {
        let index = self.entries.len().checked_sub(1);
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

    #[test]
    fn a_fresh_scroll_state_follows() {
        let state = FollowState::new();
        assert!(
            state.following(),
            "a new follow pin starts pinned to the newest line"
        );
    }
}

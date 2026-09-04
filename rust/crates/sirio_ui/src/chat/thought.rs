//! The reasoning entry as the gallery's Activity page draws it: an orb while
//! it streams and a disclosure chevron once it settles, `Thinking` /
//! `Thought for Ns`, and a body pinned to its newest line —
//! `crabtalk/bezel` tag `v0.1.4`, `apps/gallery/src/patterns/agent.rs`
//! (`Activity`). Whether the body is open is `widgets::Takeover`: it follows
//! the run until the reader presses the header, then obeys the reader.

use std::time::Duration;

use bezel::motion::Painter;
use bezel::ui::scroll::{FollowState, ScrollbarState};
use gpui::ScrollHandle;

use crate::loading;

use super::Entry;

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
}

impl super::Chat {
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
                && duration_ms.is_none() {
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

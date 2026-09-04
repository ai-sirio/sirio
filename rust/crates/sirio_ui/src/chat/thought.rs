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

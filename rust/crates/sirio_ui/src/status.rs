//! The status an agent surface reports, and the one colour each status is
//! painted in. Shared by the sidebar (worktree cards, pills, sessions) —
//! it used to live in `right_panel/`, which drew it for the Activity view.

use sirio_theme::Theme;

/// Status shown alongside a sidebar session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStatus {
    /// The surface is waiting for input.
    Idle,
    /// The surface is currently running.
    Running,
    /// The agent is blocked on a user decision or answer.
    NeedsInput,
    /// The surface completed successfully.
    Done,
    /// The surface reported an error.
    Error,
}

/// The colour an [`ActivityStatus`] is painted in, wherever it is drawn.
///
/// Sidebar session rows and the tab strip's status glyphs show the same five
/// states, and used to carry two copies of this table — one here and one in the
/// app crate. Two copies of a colour table is one edit away from two different
/// colours for the same state, so there is now one.
///
/// Only the states that want something from the reader are coloured. Running is
/// the ordinary case and reads as the bright text neutral; idle is the same
/// neutral turned down. Amber, green and red are kept for "answer me", "this
/// finished" and "this broke".
pub fn status_color(status: ActivityStatus, theme: Theme) -> gpui::Rgba {
    match status {
        ActivityStatus::Idle => theme.text_faint,
        ActivityStatus::Running => theme.text,
        ActivityStatus::NeedsInput => theme.warning,
        ActivityStatus::Done => theme.success,
        ActivityStatus::Error => theme.danger,
    }
}

//! What the Trace card and the Trace tab draw the same way: the colour of a
//! span's state and of a turn's, and a count of tokens in few characters.

use gpui::Hsla;
use sirio_theme::Theme;

use crate::chat::trace::{SpanState, TurnState};

/// The colour of a state's dot or tag.
pub(crate) fn state_color(theme: &Theme, state: SpanState) -> Hsla {
    match state {
        SpanState::Running => theme.ely.info,
        SpanState::Waiting => theme.ely.warning,
        SpanState::Done => theme.ely.success,
        SpanState::Failed => theme.ely.danger,
        SpanState::Interrupted => theme.ely.fg_subtle,
    }
}

/// The colour of a turn's tag and status line: a turn is blue while it runs,
/// amber while it waits, green or red when it ends, and a quiet grey when it
/// was interrupted.
pub(crate) fn turn_color(theme: &Theme, state: TurnState) -> Hsla {
    match state {
        TurnState::Live => theme.ely.info,
        TurnState::Waiting => theme.ely.warning,
        TurnState::Done => theme.ely.success,
        TurnState::Failed => theme.ely.danger,
        TurnState::Interrupted => theme.ely.fg_subtle,
    }
}

/// Below a thousand as they are, then `x.yk` with one decimal.
pub(crate) fn compact_count(count: u64) -> String {
    if count < 1000 {
        count.to_string()
    } else {
        format!("{:.1}k", count as f64 / 1000.0)
    }
}

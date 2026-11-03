//! ↑ and ↓ in the composer: the prompts this chat has sent, newest first, and
//! the step from one to the next. Pure, so the rules are proven without a
//! window; `Chat` owns the list, the caret test and the field.

use serde::{Deserialize, Serialize};

/// How many prompts a chat remembers.
pub(crate) const HISTORY_CAP: usize = 50;

/// One prompt as the user composed it: the draft's raw text, `@path ` tokens
/// included, and the accepted mention paths that appear in it. Recalling it
/// puts both back, so sending it again lifts those paths into mentions
/// instead of leaving them as plain text. Images are never kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptRecord {
    pub text: String,
    #[serde(default)]
    pub mentions: Vec<String>,
}

/// Which way a key steps through the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// ↑: one prompt further back.
    Older,
    /// ↓: one prompt closer to now.
    Newer,
}

/// What a key does, given what the composer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Show the prompt at this position (0 is the newest).
    Show(usize),
    /// ↓ past the newest: the composer goes back to the empty draft it
    /// started from.
    Restore,
    /// An end of the history: the key is taken and nothing moves.
    Stay,
    /// Not the history's key here: it is left to the field.
    Pass,
}

/// Records `record` as the newest prompt. A prompt with no text is not kept,
/// nor one identical to the newest; the oldest goes past [`HISTORY_CAP`].
pub(crate) fn remember(history: &mut Vec<PromptRecord>, record: PromptRecord) {
    if record.text.trim().is_empty() || history.first() == Some(&record) {
        return;
    }
    history.insert(0, record);
    history.truncate(HISTORY_CAP);
}

/// The step a key takes. `shown` is the position the composer displays
/// unedited, `None` for a fresh composer.
pub(crate) fn step(shown: Option<usize>, len: usize, direction: Direction) -> Step {
    if len == 0 {
        return Step::Pass;
    }
    match (shown, direction) {
        (None, Direction::Older) => Step::Show(0),
        (None, Direction::Newer) => Step::Pass,
        (Some(at), _) if at >= len => Step::Pass,
        (Some(at), Direction::Older) if at + 1 < len => Step::Show(at + 1),
        (Some(_), Direction::Older) => Step::Stay,
        (Some(0), Direction::Newer) => Step::Restore,
        (Some(at), Direction::Newer) => Step::Show(at - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(text: &str) -> PromptRecord {
        PromptRecord {
            text: text.to_string(),
            mentions: Vec::new(),
        }
    }

    fn texts(history: &[PromptRecord]) -> Vec<&str> {
        history.iter().map(|record| record.text.as_str()).collect()
    }

    #[test]
    fn the_newest_prompt_is_first() {
        let mut history = Vec::new();
        remember(&mut history, record("one"));
        remember(&mut history, record("two"));
        assert_eq!(texts(&history), ["two", "one"]);
    }

    #[test]
    fn a_prompt_identical_to_the_newest_is_not_kept_twice() {
        let mut history = Vec::new();
        remember(&mut history, record("one"));
        remember(&mut history, record("one"));
        assert_eq!(texts(&history), ["one"]);
    }

    #[test]
    fn an_older_duplicate_is_kept() {
        let mut history = Vec::new();
        for text in ["one", "two", "one"] {
            remember(&mut history, record(text));
        }
        assert_eq!(texts(&history), ["one", "two", "one"]);
    }

    #[test]
    fn the_same_text_with_other_mentions_is_another_prompt() {
        let mut history = Vec::new();
        remember(&mut history, record("see @a.rs "));
        remember(
            &mut history,
            PromptRecord {
                text: "see @a.rs ".into(),
                mentions: vec!["a.rs".into()],
            },
        );
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn a_blank_prompt_is_not_kept() {
        let mut history = Vec::new();
        remember(&mut history, record(""));
        remember(&mut history, record("  \n "));
        assert!(history.is_empty());
    }

    #[test]
    fn the_oldest_goes_past_the_cap() {
        let mut history = Vec::new();
        for n in 0..HISTORY_CAP + 3 {
            remember(&mut history, record(&format!("p{n}")));
        }
        assert_eq!(history.len(), HISTORY_CAP);
        assert_eq!(history[0].text, format!("p{}", HISTORY_CAP + 2));
        assert_eq!(history[HISTORY_CAP - 1].text, "p3");
    }

    #[test]
    fn up_on_a_fresh_composer_shows_the_newest() {
        assert_eq!(step(None, 3, Direction::Older), Step::Show(0));
    }

    #[test]
    fn up_goes_one_further_back_each_time() {
        assert_eq!(step(Some(0), 3, Direction::Older), Step::Show(1));
        assert_eq!(step(Some(1), 3, Direction::Older), Step::Show(2));
    }

    #[test]
    fn up_at_the_oldest_stays() {
        assert_eq!(step(Some(2), 3, Direction::Older), Step::Stay);
        assert_eq!(step(Some(0), 1, Direction::Older), Step::Stay);
    }

    #[test]
    fn down_goes_one_closer_to_now() {
        assert_eq!(step(Some(2), 3, Direction::Newer), Step::Show(1));
        assert_eq!(step(Some(1), 3, Direction::Newer), Step::Show(0));
    }

    #[test]
    fn down_past_the_newest_restores_the_draft() {
        assert_eq!(step(Some(0), 3, Direction::Newer), Step::Restore);
    }

    #[test]
    fn down_on_a_fresh_composer_is_the_fields() {
        assert_eq!(step(None, 3, Direction::Newer), Step::Pass);
    }

    #[test]
    fn an_empty_history_leaves_both_keys_to_the_field() {
        assert_eq!(step(None, 0, Direction::Older), Step::Pass);
        assert_eq!(step(None, 0, Direction::Newer), Step::Pass);
    }

    #[test]
    fn a_position_past_the_end_of_a_shorter_history_is_not_followed() {
        // The list shrank under the position (a restore replaced it): the
        // composer shows nothing of it, so the key is the field's.
        assert_eq!(step(Some(5), 3, Direction::Older), Step::Pass);
        assert_eq!(step(Some(5), 3, Direction::Newer), Step::Pass);
    }
}

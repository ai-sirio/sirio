//! The line above the composer: the CLI's retry of a request, and the
//! notifications it raises. Pure state and text, so every rule is testable
//! without a window; `mod.rs` owns the timers and the drawing.
//!
//! Two sources, two lifetimes. The retry line belongs to the turn: it clears
//! on the first content, the turn's end and the other events the chat names.
//! A notification belongs to the session: it is kept for its own timeout and
//! outlives the turn. Every entry carries a number from one counter that never
//! resets, and a timer clears only the entry that still holds its number.

use serde_json::json;

/// How long a notification is kept when the frame names no timeout.
pub(super) const DEFAULT_NOTIFICATION_MS: u64 = 10_000;

/// A retry line the CLI never follows with content or an end must still go:
/// this long after its own delay, it clears itself.
pub(super) const RETRY_SAFETY_MS: u64 = 30_000;

/// How a line is coloured.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Tone {
    Plain,
    Warning,
    Error,
}

impl Tone {
    /// The frame's `color` when it is `error` or `warning`; anything else is plain.
    fn from_color(color: Option<&str>) -> Self {
        match color {
            Some("error") => Self::Error,
            Some("warning") => Self::Warning,
            _ => Self::Plain,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// The text of one retry: `Retrying 2 of 10 in 2 s: overloaded`. The seconds
/// are the delay rounded up, and at least one.
pub(super) fn retry_text(
    attempt: u32,
    max_retries: u32,
    retry_delay_ms: u64,
    error: Option<&str>,
    error_status: Option<u16>,
    no_response: bool,
) -> String {
    let seconds = retry_delay_ms.div_ceil(1000).max(1);
    let cause = if no_response {
        "no response".to_string()
    } else {
        match (error, error_status) {
            (Some("overloaded"), _) => "overloaded".to_string(),
            (Some("server_error"), _) => "server error".to_string(),
            (Some("rate_limit"), _) => "rate limited".to_string(),
            (_, Some(status)) => format!("error {status}"),
            (_, None) => "connection lost".to_string(),
        }
    };
    format!("Retrying {attempt} of {max_retries} in {seconds} s: {cause}")
}

#[derive(Clone, Debug)]
struct RetryLine {
    text: String,
    number: u64,
}

#[derive(Clone, Debug)]
struct Held {
    key: String,
    text: String,
    priority: Option<String>,
    tone: Tone,
    number: u64,
}

impl Held {
    /// `immediate` first; an unknown or missing priority ranks as `medium`.
    fn rank(&self) -> u8 {
        match self.priority.as_deref() {
            Some("immediate") => 0,
            Some("high") => 1,
            Some("low") => 3,
            _ => 2,
        }
    }

    /// The priority as `surface.chat.read` names it.
    fn priority_name(&self) -> &'static str {
        match self.rank() {
            0 => "immediate",
            1 => "high",
            3 => "low",
            _ => "medium",
        }
    }
}

/// What the line shows right now.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Shown {
    pub(super) text: String,
    pub(super) tone: Tone,
}

#[derive(Debug, Default)]
pub(super) struct StatusLine {
    retry: Option<RetryLine>,
    notifications: Vec<Held>,
    /// Never reset for the life of the chat.
    counter: u64,
}

impl StatusLine {
    fn next_number(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// Shows a retry line and returns the number its safety expiry carries.
    pub(super) fn set_retry(&mut self, text: String) -> u64 {
        let number = self.next_number();
        self.retry = Some(RetryLine { text, number });
        number
    }

    /// Clears the retry line, whichever one it is. Returns whether one was showing.
    pub(super) fn clear_retry(&mut self) -> bool {
        self.retry.take().is_some()
    }

    /// The safety expiry of the retry line numbered `number`: a later retry's
    /// line is not touched.
    pub(super) fn expire_retry(&mut self, number: u64) -> bool {
        if self.retry.as_ref().is_some_and(|line| line.number == number) {
            self.retry = None;
            true
        } else {
            false
        }
    }

    /// Holds a notification, replacing the one with the same key, and returns
    /// the number its timer carries.
    pub(super) fn hold(
        &mut self,
        key: String,
        text: String,
        priority: Option<String>,
        color: Option<&str>,
    ) -> u64 {
        let number = self.next_number();
        self.notifications.retain(|held| held.key != key);
        self.notifications.push(Held {
            key,
            text,
            priority,
            tone: Tone::from_color(color),
            number,
        });
        number
    }

    /// A notification's timer: removes the entry only if `key` still holds `number`.
    pub(super) fn expire_notification(&mut self, key: &str, number: u64) -> bool {
        let before = self.notifications.len();
        self.notifications
            .retain(|held| !(held.key == key && held.number == number));
        self.notifications.len() != before
    }

    /// A new process: nothing of the last one's is still true.
    pub(super) fn clear_all(&mut self) {
        self.retry = None;
        self.notifications.clear();
    }

    /// The top notification: highest rank, newest first among equals.
    fn top(&self) -> Option<&Held> {
        self.notifications
            .iter()
            .min_by_key(|held| (held.rank(), std::cmp::Reverse(held.number)))
    }

    /// The retry line if there is one, otherwise the top notification.
    pub(super) fn shown(&self) -> Option<Shown> {
        if let Some(retry) = &self.retry {
            return Some(Shown {
                text: retry.text.clone(),
                tone: Tone::Plain,
            });
        }
        self.top().map(|held| Shown {
            text: held.text.clone(),
            tone: held.tone,
        })
    }

    /// `surface.chat.read`'s `statusLine`: `""` when nothing shows, else a JSON object.
    pub(super) fn snapshot(&self) -> String {
        let held = self.notifications.len();
        if let Some(retry) = &self.retry {
            return json!({
                "source": "retry",
                "text": retry.text,
                "tone": Tone::Plain.as_str(),
                "held": held,
            })
            .to_string();
        }
        match self.top() {
            Some(top) => json!({
                "source": "notification",
                "key": top.key,
                "text": top.text,
                "priority": top.priority_name(),
                "tone": top.tone.as_str(),
                "held": held,
            })
            .to_string(),
            None => String::new(),
        }
    }
}

// Failure list for the state above, written before its tests. It is wrong if:
// - the retry line outlives the answer, or never clears when nothing follows;
// - a notification dies with the turn (nothing but its timer or a new process clears it);
// - one timer clears another's line (an old retry's expiry the new retry; a replaced
//   notification's timer its replacement);
// - a same-key notification stacks instead of replacing;
// - priority order is ignored, or an unknown priority outranks a known one;
// - an unknown colour leaks into `tone`;
// - a cause word or the rounding of the seconds is wrong.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retry_text_names_the_attempt_the_rounded_wait_and_the_cause() {
        let cases = [
            ((1, 10, 617, Some("overloaded"), Some(529), false), "Retrying 1 of 10 in 1 s: overloaded"),
            ((2, 10, 1101, Some("overloaded"), Some(529), false), "Retrying 2 of 10 in 2 s: overloaded"),
            ((3, 10, 3399, Some("overloaded"), Some(529), false), "Retrying 3 of 10 in 4 s: overloaded"),
            ((1, 10, 617, Some("server_error"), Some(500), false), "Retrying 1 of 10 in 1 s: server error"),
            ((1, 10, 617, Some("rate_limit"), Some(429), false), "Retrying 1 of 10 in 1 s: rate limited"),
            ((1, 10, 617, Some("unknown"), None, false), "Retrying 1 of 10 in 1 s: connection lost"),
            ((1, 10, 617, Some("unknown"), Some(400), false), "Retrying 1 of 10 in 1 s: error 400"),
            ((1, 10, 617, Some("unknown"), None, true), "Retrying 1 of 10 in 1 s: no response"),
            ((1, 10, 0, Some("overloaded"), Some(529), false), "Retrying 1 of 10 in 1 s: overloaded"),
        ];
        for ((attempt, max, delay, error, status, none), want) in cases {
            assert_eq!(retry_text(attempt, max, delay, error, status, none), want);
        }
    }

    #[test]
    fn an_earlier_retrys_expiry_leaves_the_later_line_alone() {
        let mut line = StatusLine::default();
        let first = line.set_retry("one".into());
        let second = line.set_retry("two".into());
        assert!(!line.expire_retry(first));
        assert_eq!(line.shown().map(|shown| shown.text), Some("two".into()));
        assert!(line.expire_retry(second));
        assert_eq!(line.snapshot(), "");
    }

    #[test]
    fn a_replaced_notifications_timer_leaves_its_replacement_alone() {
        let mut line = StatusLine::default();
        let old = line.hold("k".into(), "old".into(), Some("low".into()), None);
        let new = line.hold("k".into(), "new".into(), Some("low".into()), None);
        assert_eq!(line.notifications.len(), 1, "same key replaces");
        assert!(!line.expire_notification("k", old));
        assert_eq!(line.shown().map(|shown| shown.text), Some("new".into()));
        assert!(line.expire_notification("k", new));
        assert!(line.shown().is_none());
    }

    #[test]
    fn priority_orders_the_line_and_newest_wins_among_equals() {
        let mut line = StatusLine::default();
        line.hold("low".into(), "low".into(), Some("low".into()), None);
        line.hold("odd".into(), "odd".into(), Some("urgent".into()), None);
        assert_eq!(line.shown().map(|shown| shown.text), Some("odd".into()), "unknown ranks as medium, above low");
        line.hold("high".into(), "high".into(), Some("high".into()), None);
        line.hold("a".into(), "a".into(), Some("immediate".into()), None);
        line.hold("b".into(), "b".into(), Some("immediate".into()), None);
        assert_eq!(line.shown().map(|shown| shown.text), Some("b".into()));
        let json: serde_json::Value = serde_json::from_str(&line.snapshot()).unwrap();
        assert_eq!(json["held"], 5);
        assert_eq!(json["priority"], "immediate");
        assert_eq!(json["source"], "notification");
    }

    #[test]
    fn only_error_and_warning_colours_become_a_tone() {
        let mut line = StatusLine::default();
        line.hold("a".into(), "a".into(), None, Some("red"));
        assert_eq!(line.shown().map(|shown| shown.tone), Some(Tone::Plain));
        line.hold("b".into(), "b".into(), None, Some("warning"));
        assert_eq!(line.shown().map(|shown| shown.tone), Some(Tone::Warning));
        line.hold("c".into(), "c".into(), None, Some("error"));
        assert_eq!(line.shown().map(|shown| shown.tone), Some(Tone::Error));
    }

    #[test]
    fn a_retry_line_shows_over_the_notifications_and_clears_alone() {
        let mut line = StatusLine::default();
        line.hold("k".into(), "note".into(), Some("immediate".into()), None);
        line.set_retry("Retrying".into());
        let json: serde_json::Value = serde_json::from_str(&line.snapshot()).unwrap();
        assert_eq!(json["source"], "retry");
        assert_eq!(json["held"], 1);
        assert!(line.clear_retry());
        assert_eq!(line.shown().map(|shown| shown.text), Some("note".into()));
        line.clear_all();
        assert_eq!(line.snapshot(), "");
    }
}

//! When the worktree holds an error the user has not seen (spec §4.4).
//!
//! An error is known by its identity (`ErrorKey`: server, worktree-relative
//! file, message), never by its line or by when it arrived, so no ordering of
//! publishes, clears, restarts or a relaunch's refill can make an old error
//! look new (with no look between a clear and its republish). The alarm holds
//! the `seen` counts and changes them in one place, `look`: seen becomes the
//! current errors, plus the old seen entries of every (server, file) that no
//! live server has reported since it started, so a look in the middle of a
//! refill never forgets the files not reported yet. The drawer is called for
//! when some identity's current count is above its seen count.
//!
//! Two accepted costs (spec §4.4):
//! - an error the user saw, fixed while they were not looking, that returns
//!   identically (an agent reverting its own fix) does not call for the
//!   drawer; `a_clear_then_republish_does_not` is that case, and Problems'
//!   title still counts it;
//! - the converse, rarer one: a look or close that lands between a server
//!   clearing a file and republishing it records the file as fixed, so the
//!   identical republish calls for the drawer once;
//!   `a_close_between_a_clear_and_its_republish_calls_once` is that case.
//!
//! Ways this can fail, written before the code:
//! - the first error of a worktree with an empty seen set does not call for
//!   the drawer;
//! - the same errors republished call for it;
//! - a message the user has not seen, or a second copy of a seen one, does
//!   not;
//! - after a relaunch the refill arrives in any order, with dips (a server
//!   clears a file and republishes it), and some prefix of it calls for the
//!   drawer;
//! - a server that exits and comes back, or a file cleared and republished
//!   with no look between them, reads as new errors;
//! - a look in the middle of a refill forgets the seen entries of the files
//!   no server has reported yet, so their return calls for the drawer;
//! - a look after a real fix keeps the fixed key, so the error that returns
//!   later is judged against a count the user saw before the fix;
//! - the persisted text does not read back to the same set, or malformed text
//!   (a raw newline in a string, a wrong arity, a negative count, trailing
//!   text) panics or yields a half-read set instead of an empty one;
//! - accepted (spec §4.4): an error the user saw, fixed while they were not
//!   looking, that returns identically does not call for the drawer
//!   (`a_clear_then_republish_does_not`);
//! - accepted (spec §4.4): a look or close between a clear and its republish
//!   records the file as fixed, so the identical republish calls for the
//!   drawer once (`a_close_between_a_clear_and_its_republish_calls_once`).

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{ErrorCounts, ServerKey};

/// The errors the user has seen, by identity, and the one question the
/// drawer asks of them.
#[derive(Debug, Clone, Default)]
pub struct ErrorAlarm {
    seen: ErrorCounts,
}

impl ErrorAlarm {
    /// An alarm that has seen `seen`: the persisted set, or empty for a
    /// worktree the user has never looked at.
    pub fn new(seen: ErrorCounts) -> Self {
        Self { seen }
    }

    pub fn seen(&self) -> &ErrorCounts {
        &self.seen
    }

    /// The user is looking at Problems, or has closed the drawer. The seen
    /// set becomes `current`, plus the old entries of every (server, path)
    /// that is not in `reported`: nobody has spoken for those files since the
    /// server started, so their absence in `current` is not a fix.
    pub fn look(&mut self, current: &ErrorCounts, reported: &BTreeSet<(ServerKey, PathBuf)>) {
        let mut next = current.clone();
        for (identity, count) in &self.seen {
            let (server, path, _) = identity;
            if !reported.contains(&(server.clone(), path.clone())) {
                next.entry(identity.clone()).or_insert(*count);
            }
        }
        self.seen = next;
    }

    /// Whether some error identity is listed more times than the user saw it.
    pub fn calls_for_drawer(&self, current: &ErrorCounts) -> bool {
        current
            .iter()
            .any(|(identity, count)| *count > self.seen.get(identity).copied().unwrap_or(0))
    }

    /// The seen set as persisted: an array of
    /// `[project root, language, path, message, count]`.
    pub fn to_json(&self) -> String {
        let rows: Vec<Row> = self
            .seen
            .iter()
            .map(|((server, path, message), count)| {
                (
                    server.0.to_string_lossy().into_owned(),
                    server.1.clone(),
                    path.to_string_lossy().into_owned(),
                    message.clone(),
                    *count,
                )
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Reads what `to_json` wrote. Malformed text is an empty set, never a
    /// half-read one: an unseen error then opens the drawer once, which is the
    /// safe side.
    pub fn from_json(text: &str) -> Self {
        let rows: Vec<Row> = serde_json::from_str(text).unwrap_or_default();
        Self::new(
            rows.into_iter()
                .map(|(root, language, path, message, count)| {
                    (
                        ((PathBuf::from(root), language), PathBuf::from(path), message),
                        count,
                    )
                })
                .collect(),
        )
    }
}

/// One persisted entry: project root, language, path, message, count.
type Row = (String, String, String, String, usize);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawer::{ErrorKey, ProblemStore, StoredProblem};
    use sirio_lsp::Severity;
    use std::path::Path;

    fn server(language: &str) -> ServerKey {
        (PathBuf::from("/project"), language.to_owned())
    }

    fn key(language: &str, path: &str, message: &str) -> ErrorKey {
        (server(language), PathBuf::from(path), message.to_owned())
    }

    fn counts(entries: &[(ErrorKey, usize)]) -> ErrorCounts {
        entries.iter().cloned().collect()
    }

    fn reported(pairs: &[(&str, &str)]) -> BTreeSet<(ServerKey, PathBuf)> {
        pairs
            .iter()
            .map(|(language, path)| (server(language), PathBuf::from(path)))
            .collect()
    }

    fn errors(messages: &[&str]) -> Vec<StoredProblem> {
        messages
            .iter()
            .enumerate()
            .map(|(line, message)| StoredProblem {
                line: line as u32,
                column: 0,
                severity: Severity::Error,
                message: (*message).to_owned(),
                source: None,
            })
            .collect()
    }

    /// An alarm that has looked at exactly `seen`, with every pair reported.
    fn alarm_that_saw(seen: &[(ErrorKey, usize)]) -> ErrorAlarm {
        let seen = counts(seen);
        let all: BTreeSet<_> = seen
            .keys()
            .map(|(server, path, _)| (server.clone(), path.clone()))
            .collect();
        let mut alarm = ErrorAlarm::new(ErrorCounts::new());
        alarm.look(&seen, &all);
        alarm
    }

    #[test]
    fn the_first_error_calls_for_the_drawer() {
        let alarm = ErrorAlarm::new(ErrorCounts::new());
        assert!(alarm.calls_for_drawer(&counts(&[(key("rust", "a.rs", "boom"), 1)])));
        assert!(!alarm.calls_for_drawer(&ErrorCounts::new()));
    }

    #[test]
    fn the_same_errors_republished_do_not() {
        let seen = [(key("rust", "a.rs", "boom"), 3), (key("rust", "b.rs", "x"), 1)];
        let alarm = alarm_that_saw(&seen);
        assert!(!alarm.calls_for_drawer(&counts(&seen)));
    }

    #[test]
    fn a_new_message_does() {
        let alarm = alarm_that_saw(&[(key("rust", "a.rs", "boom"), 1)]);
        let now = counts(&[
            (key("rust", "a.rs", "boom"), 1),
            (key("rust", "a.rs", "other"), 1),
        ]);
        assert!(alarm.calls_for_drawer(&now));
        // The same message from another server or in another file is new too.
        assert!(alarm.calls_for_drawer(&counts(&[(key("clippy", "a.rs", "boom"), 1)])));
        assert!(alarm.calls_for_drawer(&counts(&[(key("rust", "b.rs", "boom"), 1)])));
    }

    #[test]
    fn a_second_copy_of_a_seen_message_does() {
        let alarm = alarm_that_saw(&[(key("rust", "a.rs", "boom"), 1)]);
        assert!(alarm.calls_for_drawer(&counts(&[(key("rust", "a.rs", "boom"), 2)])));
    }

    #[test]
    fn a_relaunch_refill_in_any_order_with_dips_does_not() {
        // The persisted set, then the store refilling from zero.
        let seen = [
            (key("rust", "a.rs", "e1"), 1),
            (key("rust", "b.rs", "e2"), 2),
            (key("rust", "c.rs", "e3"), 1),
        ];
        let mut alarm = ErrorAlarm::new(counts(&seen));
        let c_only = counts(&[(key("rust", "c.rs", "e3"), 1)]);
        let steps: [&[(ErrorKey, usize)]; 6] = [
            &[],
            &[(key("rust", "c.rs", "e3"), 1)],
            &[(key("rust", "c.rs", "e3"), 1), (key("rust", "b.rs", "e2"), 1)],
            &[(key("rust", "b.rs", "e2"), 1)],
            &[(key("rust", "b.rs", "e2"), 2), (key("rust", "a.rs", "e1"), 1)],
            &seen,
        ];
        for (n, step) in steps.into_iter().enumerate() {
            assert!(!alarm.calls_for_drawer(&counts(step)), "{step:?}");
            if n == 1 {
                // The drawer shows Problems while only c.rs has been reported.
                alarm.look(&c_only, &reported(&[("rust", "c.rs")]));
                assert_eq!(alarm.seen(), &counts(&seen), "a.rs and b.rs survive");
            }
        }
    }

    #[test]
    fn a_server_gone_and_republished_does_not() {
        let mut store = ProblemStore::default();
        let rust = server("rust");
        store.publish(&rust, Path::new("a.rs"), errors(&["boom", "boom", "bang"]));
        store.publish(&rust, Path::new("b.rs"), errors(&["crash"]));
        let mut alarm = ErrorAlarm::new(ErrorCounts::new());
        alarm.look(&store.error_counts(), &store.reported());
        let seen = alarm.seen().clone();
        store.server_gone(&rust);
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
        store.publish(&rust, Path::new("a.rs"), errors(&["bang", "boom", "boom"]));
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
        // The drawer shows Problems while only a.rs is back.
        alarm.look(&store.error_counts(), &store.reported());
        assert_eq!(alarm.seen(), &seen, "b.rs survives the look");
        store.publish(&rust, Path::new("b.rs"), errors(&["crash"]));
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
    }

    #[test]
    fn a_close_between_a_clear_and_its_republish_calls_once() {
        // The accepted cost: the look records the cleared file as fixed.
        let mut store = ProblemStore::default();
        let rust = server("rust");
        store.publish(&rust, Path::new("a.rs"), errors(&["boom"]));
        let mut alarm = ErrorAlarm::new(ErrorCounts::new());
        alarm.look(&store.error_counts(), &store.reported());
        store.publish(&rust, Path::new("a.rs"), Vec::new());
        alarm.look(&store.error_counts(), &store.reported());
        store.publish(&rust, Path::new("a.rs"), errors(&["boom"]));
        assert!(alarm.calls_for_drawer(&store.error_counts()));
        alarm.look(&store.error_counts(), &store.reported());
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
    }

    #[test]
    fn a_clear_then_republish_does_not() {
        // Also the accepted cost: a seen error fixed while not looking that
        // returns identically is not new.
        let mut store = ProblemStore::default();
        let rust = server("rust");
        store.publish(&rust, Path::new("a.rs"), errors(&["boom"]));
        let mut alarm = ErrorAlarm::new(ErrorCounts::new());
        alarm.look(&store.error_counts(), &store.reported());
        store.publish(&rust, Path::new("a.rs"), Vec::new());
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
        store.publish(&rust, Path::new("a.rs"), errors(&["boom"]));
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
    }

    #[test]
    fn a_look_mid_refill_keeps_unreported_pairs_and_their_return_does_not() {
        let seen = [(key("rust", "a.rs", "e1"), 1), (key("ts", "b.ts", "e2"), 1)];
        let mut alarm = ErrorAlarm::new(counts(&seen));
        // Only the rust server has reported so far, and the user looks.
        let current = counts(&[(key("rust", "a.rs", "e1"), 1)]);
        alarm.look(&current, &reported(&[("rust", "a.rs")]));
        assert_eq!(alarm.seen(), &counts(&seen), "the ts entry survives the look");
        // The ts server now reports what it reported before.
        assert!(!alarm.calls_for_drawer(&counts(&seen)));
    }

    #[test]
    fn a_look_after_a_fix_drops_the_key_and_its_return_does() {
        let mut alarm = alarm_that_saw(&[(key("rust", "a.rs", "boom"), 1)]);
        // The error was fixed; the server has reported the file since.
        alarm.look(&ErrorCounts::new(), &reported(&[("rust", "a.rs")]));
        assert!(alarm.seen().is_empty());
        assert!(alarm.calls_for_drawer(&counts(&[(key("rust", "a.rs", "boom"), 1)])));
    }

    #[test]
    fn json_round_trips_and_malformed_text_is_empty() {
        let seen = counts(&[
            (key("rust", "src/a.rs", "plain"), 1),
            (key("rust", "dir/b \"q\".rs", "quote \" back\\ nl\n tab\t \u{1} \u{1f600} \u{e8}\u{6f22}"), 2),
            (key("ts", "c.ts", ""), 7),
        ]);
        let text = ErrorAlarm::new(seen.clone()).to_json();
        assert_eq!(ErrorAlarm::from_json(&text).seen(), &seen);
        let raw_newline = "[[\"a\",\"b\",\"c\",\"d\ne\",1]]";
        let bad_texts = [
            "",
            "{",
            "[",
            "[[1]]",
            "[[\"a\",\"b\",\"c\",\"d\"]]",
            "null",
            "[] x",
            "[[\"a\",\"b\",\"c\",\"d\",-1]]",
            "[[\"a\",\"b\",\"c\",\"d\",1,2]]",
            raw_newline,
        ];
        for bad in bad_texts {
            assert!(ErrorAlarm::from_json(bad).seen().is_empty(), "{bad:?}");
        }
        assert!(ErrorAlarm::from_json("[]").seen().is_empty());
    }
}

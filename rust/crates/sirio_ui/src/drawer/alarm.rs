//! When the worktree holds an error the user has not seen (spec §4.4).
//!
//! An error is known by its identity (`ErrorKey`: server, worktree-relative
//! file, message), never by its line or by when it arrived, so no ordering of
//! publishes, clears, restarts or a relaunch's refill can make an old error
//! look new. The alarm holds the `seen` counts and changes them in one place,
//! `look`: seen becomes the current errors, plus the old seen entries of every
//! (server, file) that no live server has reported since it started, so a look
//! in the middle of a refill keeps what the user saw before it. The drawer is
//! called for when some identity's current count is above its seen count.
//!
//! Accepted cost (spec §4.4): an error the user saw, fixed while they were
//! not looking, that returns identically (an agent reverting its own fix)
//! does not call for the drawer. `a_clear_then_republish_does_not` is that
//! case. Problems' title still counts it.
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
//! - a server that exits and comes back, or a file cleared and republished,
//!   reads as new errors;
//! - a look in the middle of a refill forgets the seen entries of the files
//!   no server has reported yet, so their return calls for the drawer;
//! - a look after a real fix keeps the fixed key, so the error that returns
//!   later is judged against a count the user saw before the fix;
//! - the persisted text does not read back to the same set, or malformed text
//!   panics or yields a half-read set instead of an empty one.

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
        let mut out = String::from("[");
        for (n, ((server, path, message), count)) in self.seen.iter().enumerate() {
            if n > 0 {
                out.push(',');
            }
            out.push('[');
            for text in [
                server.0.to_string_lossy().as_ref(),
                &server.1,
                path.to_string_lossy().as_ref(),
                message,
            ] {
                write_string(&mut out, text);
                out.push(',');
            }
            out.push_str(&count.to_string());
            out.push(']');
        }
        out.push(']');
        out
    }

    /// Reads what `to_json` wrote. Malformed text is an empty set, never a
    /// half-read one: an unseen error then opens the drawer once, which is the
    /// safe side.
    pub fn from_json(text: &str) -> Self {
        Self::new(read_set(text).unwrap_or_default())
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    rest: &'a str,
}

impl<'a> Reader<'a> {
    fn eat(&mut self, c: char) -> Option<()> {
        self.rest = self.rest.strip_prefix(c)?;
        Some(())
    }

    fn string(&mut self) -> Option<String> {
        self.eat('"')?;
        let mut out = String::new();
        let mut chars = self.rest.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => {
                    self.rest = &self.rest[at + 1..];
                    return Some(out);
                }
                '\\' => match chars.next()?.1 {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let mut unit = hex4(&mut chars)?;
                        if (0xD800..0xDC00).contains(&unit) {
                            if chars.next()?.1 != '\\' || chars.next()?.1 != 'u' {
                                return None;
                            }
                            let low = hex4(&mut chars)?;
                            if !(0xDC00..0xE000).contains(&low) {
                                return None;
                            }
                            unit = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                        }
                        out.push(char::from_u32(unit)?);
                    }
                    _ => return None,
                },
                c => out.push(c),
            }
        }
        None
    }

    fn count(&mut self) -> Option<usize> {
        let digits = self.rest.len() - self.rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (number, rest) = self.rest.split_at(digits);
        let count = number.parse().ok()?;
        self.rest = rest;
        Some(count)
    }
}

fn hex4(chars: &mut std::str::CharIndices<'_>) -> Option<u32> {
    let mut unit = 0;
    for _ in 0..4 {
        unit = unit * 16 + chars.next()?.1.to_digit(16)?;
    }
    Some(unit)
}

fn read_set(text: &str) -> Option<ErrorCounts> {
    let mut reader = Reader { rest: text.trim() };
    let mut seen = ErrorCounts::new();
    reader.eat('[')?;
    if reader.eat(']').is_some() {
        return reader.rest.is_empty().then_some(seen);
    }
    loop {
        reader.eat('[')?;
        let root = reader.string()?;
        reader.eat(',')?;
        let language = reader.string()?;
        reader.eat(',')?;
        let path = reader.string()?;
        reader.eat(',')?;
        let message = reader.string()?;
        reader.eat(',')?;
        let count = reader.count()?;
        reader.eat(']')?;
        seen.insert(((PathBuf::from(root), language), PathBuf::from(path), message), count);
        if reader.eat(']').is_some() {
            return reader.rest.is_empty().then_some(seen);
        }
        reader.eat(',')?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawer::{ErrorKey, ProblemStore, ServerKey, StoredProblem};
    use sirio_lsp::Severity;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

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
        let alarm = ErrorAlarm::new(counts(&seen));
        let steps: [&[(ErrorKey, usize)]; 6] = [
            &[],
            &[(key("rust", "c.rs", "e3"), 1)],
            &[(key("rust", "c.rs", "e3"), 1), (key("rust", "b.rs", "e2"), 1)],
            &[(key("rust", "b.rs", "e2"), 1)],
            &[(key("rust", "b.rs", "e2"), 2), (key("rust", "a.rs", "e1"), 1)],
            &seen,
        ];
        for step in steps {
            assert!(!alarm.calls_for_drawer(&counts(step)), "{step:?}");
        }
    }

    #[test]
    fn a_server_gone_and_republished_does_not() {
        let mut store = ProblemStore::default();
        let rust = server("rust");
        store.publish(&rust, Path::new("a.rs"), errors(&["boom", "boom", "bang"]));
        let mut alarm = ErrorAlarm::new(ErrorCounts::new());
        alarm.look(&store.error_counts(), &store.reported());
        store.server_gone(&rust);
        assert!(!alarm.calls_for_drawer(&store.error_counts()));
        store.publish(&rust, Path::new("a.rs"), errors(&["bang", "boom", "boom"]));
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
        for bad in ["", "{", "[", "[[1]]", "[[\"a\",\"b\",\"c\",\"d\"]]", "null", "[] x", "[[\"a\",\"b\",\"c\",\"d\",-1]]"] {
            assert!(ErrorAlarm::from_json(bad).seen().is_empty(), "{bad:?}");
        }
        assert!(ErrorAlarm::from_json("[]").seen().is_empty());
    }
}

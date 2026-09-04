//! The transcript as the gallery's Transcript page draws it: a turn is a
//! question and everything up to the next one; **the answer is the prose
//! after the last tool call, everything before it is interim** — that
//! sentence is the whole rule (`apps/gallery/src/patterns/transcript.rs`,
//! `split`). The interim half folds behind `Worked · N steps`, run by
//! `widgets::Takeover`: open while the turn streams, folded once it ends,
//! the reader's press holding from then on. A day heading marks where the
//! calendar day changes between turns.

use std::collections::HashMap;

use bezel::ui::widgets::Takeover;
use chrono::{DateTime, Local};

use super::{Entry, TurnSegment};

/// One turn's zones: entries before `answer_from` are interim work, prose
/// at or after it is the answer. `steps` is how much happened — each tool
/// call is one, a subagent task is one plus its nested calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkSplit {
    pub answer_from: usize,
    pub steps: usize,
}

/// `rposition` of the last tool in the turn — `Transcript.svelte`'s whole
/// `splitTurn`. A turn without tools is all answer.
pub(crate) fn split_work(entries: &[Entry], turn: &TurnSegment) -> WorkSplit {
    let range = turn.start..=turn.end;
    let slice = &entries[range.clone()];
    let is_tool =
        |entry: &Entry| matches!(entry, Entry::ToolCall { .. } | Entry::SubagentTask { .. });
    let answer_from = slice
        .iter()
        .rposition(is_tool)
        .map_or(turn.start, |i| turn.start + i + 1);
    let steps = slice[..answer_from - turn.start]
        .iter()
        .map(|entry| match entry {
            Entry::ToolCall { .. } => 1,
            Entry::SubagentTask { tool_calls, .. } => 1 + tool_calls.len(),
            _ => 0,
        })
        .sum();
    WorkSplit { answer_from, steps }
}

/// The Work header's word (the old `tool_group_label`).
pub(crate) fn work_label(steps: usize) -> String {
    if steps == 1 {
        "Worked · 1 step".to_string()
    } else {
        format!("Worked · {steps} steps")
    }
}

/// Sirio's words for a day — bezel carries no clock. `Today`, `Yesterday`,
/// otherwise `Mon 2 Sep`.
pub(crate) fn day_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let day = at.date_naive();
    let today = now.date_naive();
    if day == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(day) {
        "Yesterday".to_string()
    } else {
        at.format("%a %-d %b").to_string()
    }
}

/// What the list draws at one index once the Work zones are applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkRole {
    /// Not part of a zone: the answer, footers, permissions, plans, errors,
    /// and the members of a turn that has no tool calls.
    Outside,
    /// The turn's first entry: draws the heading, the bubble and — when the
    /// turn has steps — the Work header.
    Header {
        turn: usize,
        steps: usize,
        open: bool,
    },
    /// An interim entry of a turn with steps: an empty row while the zone is
    /// folded, its own content inside the zone's frame while open.
    Member { turn: usize, open: bool },
}

/// One role per entry, resolved once per frame. `streaming_turn` is the
/// start index of the turn still being answered, if any: its zone's `auto`
/// is `true`, every other turn's is `false`.
pub(crate) fn work_roles(
    entries: &[Entry],
    work_open: &HashMap<usize, Takeover>,
    streaming_turn: Option<usize>,
) -> Vec<WorkRole> {
    let mut roles = vec![WorkRole::Outside; entries.len()];
    for turn in super::segment_turns(entries) {
        let split = split_work(entries, &turn);
        // A turn without tools draws no header, so it has no open state to
        // report: `auto` only applies where a zone exists.
        let auto = streaming_turn == Some(turn.start) && split.steps > 0;
        let open = work_open
            .get(&turn.start)
            .copied()
            .unwrap_or_default()
            .get(auto);
        roles[turn.start] = WorkRole::Header {
            turn: turn.start,
            steps: split.steps,
            open,
        };
        if split.steps == 0 {
            continue;
        }
        for role in &mut roles[turn.start + 1..split.answer_from] {
            *role = WorkRole::Member {
                turn: turn.start,
                open,
            };
        }
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{parse_chat_markdown, segment_turns};
    use chrono::TimeZone;

    fn user(text: &str) -> Entry {
        Entry::User {
            text: text.into(),
            at: None,
        }
    }
    fn prose(text: &str) -> Entry {
        Entry::Assistant {
            text: text.into(),
            document: parse_chat_markdown(text),
        }
    }
    fn tool(id: &str) -> Entry {
        crate::chat::tests::test_tool_call(id)
    }
    fn thought() -> Entry {
        Entry::Thought {
            text: "…".into(),
            open: Default::default(),
            started: None,
            duration_ms: None,
        }
    }

    #[test]
    fn the_last_tool_call_decides_where_the_answer_starts() {
        let entries = vec![
            user("q"),
            thought(),
            prose("interim"),
            tool("a"),
            tool("b"),
            prose("answer"),
            Entry::TurnFooter("12:00".into()),
        ];
        let turns = segment_turns(&entries);
        assert_eq!(
            split_work(&entries, &turns[0]),
            WorkSplit {
                answer_from: 5,
                steps: 2
            }
        );
    }

    #[test]
    fn a_turn_without_tools_is_all_answer() {
        let entries = vec![
            user("q"),
            thought(),
            prose("answer"),
            Entry::TurnFooter("12:00".into()),
        ];
        let turns = segment_turns(&entries);
        assert_eq!(
            split_work(&entries, &turns[0]),
            WorkSplit {
                answer_from: 0,
                steps: 0
            }
        );
    }

    #[test]
    fn a_subagent_task_counts_as_a_tool_with_its_members() {
        let nested = |id: &str| crate::chat::SubagentToolCall {
            id: id.into(),
            title: format!("{id} title"),
            status: "Completed".into(),
            kind: "Edit".into(),
            content: vec![],
            locations: vec![],
            expanded: false,
            duration_ms: None,
        };
        let task = Entry::SubagentTask {
            id: "t".into(),
            title: "t title".into(),
            status: "InProgress".into(),
            tool_calls: vec![nested("n1"), nested("n2")],
            expanded: false,
        };
        let entries = vec![user("q"), task, prose("answer")];
        let turns = segment_turns(&entries);
        let split = split_work(&entries, &turns[0]);
        assert_eq!(split.answer_from, 2);
        assert_eq!(split.steps, 3, "the task and its two members");
    }

    #[test]
    fn work_labels_count_steps() {
        assert_eq!(work_label(1), "Worked · 1 step");
        assert_eq!(work_label(4), "Worked · 4 steps");
    }

    #[test]
    fn day_labels_are_sirios_words() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        assert_eq!(day_label(now, now), "Today");
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 3, 1, 0, 0).unwrap(), now),
            "Yesterday"
        );
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(), now),
            "Tue 1 Sep"
        );
    }

    #[test]
    fn roles_follow_the_split_and_the_takeover() {
        let entries = vec![
            user("q"),
            thought(),
            tool("a"),
            prose("answer"),
            Entry::TurnFooter("t".into()),
            user("q2"),
            prose("plain"),
        ];
        let none = HashMap::new();
        let roles = work_roles(&entries, &none, Some(5));
        assert_eq!(
            roles[0],
            WorkRole::Header {
                turn: 0,
                steps: 1,
                open: false
            }
        );
        assert_eq!(
            roles[1],
            WorkRole::Member {
                turn: 0,
                open: false
            }
        );
        assert_eq!(
            roles[2],
            WorkRole::Member {
                turn: 0,
                open: false
            }
        );
        assert_eq!(
            roles[3],
            WorkRole::Outside,
            "the answer is outside the zone"
        );
        assert_eq!(roles[4], WorkRole::Outside);
        assert_eq!(
            roles[5],
            WorkRole::Header {
                turn: 5,
                steps: 0,
                open: false
            },
            "no tools: a header row with no header"
        );
        assert_eq!(roles[6], WorkRole::Outside);

        let streaming = work_roles(&entries, &none, Some(0));
        assert_eq!(
            streaming[1],
            WorkRole::Member {
                turn: 0,
                open: true
            },
            "the streaming turn's zone is open by itself"
        );

        let mut pressed = HashMap::new();
        let mut t = Takeover::default();
        t.toggle(false);
        pressed.insert(0, t);
        let held = work_roles(&entries, &pressed, None);
        assert_eq!(
            held[1],
            WorkRole::Member {
                turn: 0,
                open: true
            },
            "a press holds"
        );
    }
}

//! The question dock: the one place an open agent question is answered,
//! drawn above the composer. This file holds the pure projection of the
//! transcript onto what the dock draws and the selection arithmetic the
//! keyboard uses; the design is
//! `docs/superpowers/specs/2026-09-24-question-dock-design.md`.

use super::{AnswerOption, AnswerTextInput, Entry, PlanApproval};

/// What the dock draws for the open question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QuestionView {
    /// Handle the answer goes back on.
    pub(super) request_id: u64,
    /// The transcript entry the question is recorded in.
    pub(super) entry_index: usize,
    /// The small line above the question: its header, or what kind of
    /// question it is.
    pub(super) caption: String,
    /// The question itself, whole. Empty when the caption says it all.
    pub(super) body: String,
    /// The answers, in the order they are numbered.
    pub(super) rows: Vec<DockRow>,
}

/// One numbered row of the dock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DockRow {
    /// An answer the agent offered.
    Option(AnswerOption),
    /// A typed answer, where the transport can take one.
    FreeText(AnswerTextInput),
    /// The way out of a request that offered nothing to choose.
    Dismiss,
}

/// Whether an entry is a question still waiting on the user. The one
/// predicate `Chat::pending_question` and [`question_view`] share, so the
/// dock and the disabled composer cannot disagree about whether a question
/// is open.
pub(super) fn is_open(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Permission {
            resolved: None,
            expired: false,
            ..
        } | Entry::Plan {
            approval: Some(PlanApproval {
                resolved: None,
                expired: false,
                ..
            }),
            ..
        }
    )
}

/// The first open question in the transcript, as the dock draws it.
pub(super) fn question_view(entries: &[Entry]) -> Option<QuestionView> {
    let (entry_index, entry) = entries.iter().enumerate().find(|(_, entry)| is_open(entry))?;
    match entry {
        Entry::Permission {
            request_id,
            title,
            prompt,
            options,
            text_input,
            is_question,
            ..
        } => {
            let (caption, body) = match (*is_question, prompt.is_empty()) {
                (true, false) => (title.clone(), prompt.clone()),
                // Pi's question repeats its header, so the prompt was
                // blanked at parse time and the title is the question.
                (true, true) => ("Question".to_string(), title.clone()),
                (false, _) => ("Permission requested".to_string(), title.clone()),
            };
            let mut rows = options
                .iter()
                .cloned()
                .map(DockRow::Option)
                .collect::<Vec<_>>();
            if let Some(input) = text_input {
                rows.push(DockRow::FreeText(input.clone()));
            }
            if rows.is_empty() {
                rows.push(DockRow::Dismiss);
            }
            Some(QuestionView {
                request_id: *request_id,
                entry_index,
                caption,
                body,
                rows,
            })
        }
        Entry::Plan {
            approval: Some(approval),
            ..
        } => Some(QuestionView {
            request_id: approval.request_id,
            entry_index,
            caption: "Plan approval".to_string(),
            body: approval.title.clone(),
            rows: approval.options.iter().cloned().map(DockRow::Option).collect(),
        }),
        _ => None,
    }
}

/// The selected row, kept inside the rows actually drawn.
pub(super) fn clamp_selection(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

/// One step up or down from `selected`. It stops at either end rather than
/// wrapping: Down from the last of three landing on the first reads as a
/// jump, not a step.
pub(super) fn step_selection(selected: usize, row_count: usize, down: bool) -> usize {
    let selected = clamp_selection(selected, row_count);
    if down {
        clamp_selection(selected + 1, row_count)
    } else {
        selected.saturating_sub(1)
    }
}

/// The row a digit key picks: `1` is the first row. `0`, a digit past the
/// last row, and any key that is not a single digit pick nothing.
pub(super) fn row_for_digit(key: &str, row_count: usize) -> Option<usize> {
    let digit = key
        .parse::<usize>()
        .ok()
        .filter(|digit| (1..=9).contains(digit))?;
    (digit <= row_count).then(|| digit - 1)
}

/// The number drawn on row `index`, for the nine rows a digit reaches.
pub(super) fn badge_label(index: usize) -> Option<String> {
    (index < 9).then(|| (index + 1).to_string())
}

/// Options with the given labels; each id is its label, lower-cased.
#[cfg(test)]
pub(super) fn answer_options(labels: &[&str]) -> Vec<AnswerOption> {
    labels
        .iter()
        .map(|label| AnswerOption {
            id: label.to_lowercase(),
            label: (*label).to_string(),
            description: None,
            is_rejection: false,
        })
        .collect()
}

/// A plain permission, still open.
#[cfg(test)]
pub(super) fn open_permission(request_id: u64, title: &str, labels: &[&str]) -> Entry {
    Entry::Permission {
        request_id,
        title: title.to_string(),
        prompt: String::new(),
        options: answer_options(labels),
        text_input: None,
        is_question: false,
        resolved: None,
        expired: false,
        dismissed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::PlanEntryRow;
    use super::*;

    fn question(
        request_id: u64,
        title: &str,
        prompt: &str,
        labels: &[&str],
        text_input: Option<AnswerTextInput>,
    ) -> Entry {
        Entry::Permission {
            request_id,
            title: title.to_string(),
            prompt: prompt.to_string(),
            options: answer_options(labels),
            text_input,
            is_question: true,
            resolved: None,
            expired: false,
            dismissed: false,
        }
    }

    #[test]
    fn the_first_open_question_is_the_one_drawn() {
        let mut answered = open_permission(1, "first", &["Allow"]);
        if let Entry::Permission { resolved, .. } = &mut answered {
            *resolved = Some("Allow".into());
        }
        let entries = vec![
            answered,
            open_permission(2, "second", &["Allow"]),
            open_permission(3, "third", &["Allow"]),
        ];
        let view = question_view(&entries).expect("an open question is drawn");
        assert_eq!((view.request_id, view.entry_index), (2, 1));
    }

    #[test]
    fn nothing_is_drawn_when_no_question_is_open() {
        let mut expired = open_permission(1, "expired", &["Allow"]);
        if let Entry::Permission { expired: flag, .. } = &mut expired {
            *flag = true;
        }
        let mut dismissed = open_permission(2, "dismissed", &[]);
        if let Entry::Permission {
            expired, dismissed, ..
        } = &mut dismissed
        {
            *expired = true;
            *dismissed = true;
        }
        let entries = [expired, dismissed, Entry::TurnFooter("12:00".into())];
        assert!(entries.iter().all(|entry| !is_open(entry)));
        assert_eq!(question_view(&entries), None);
    }

    #[test]
    fn a_question_with_a_prompt_is_captioned_by_its_header() {
        let view = question_view(&[question(
            1,
            "Approach",
            "Which layout should the panel use?",
            &["Docked"],
            None,
        )])
        .unwrap();
        assert_eq!(view.caption, "Approach");
        assert_eq!(view.body, "Which layout should the panel use?");
    }

    #[test]
    fn a_question_that_repeats_its_header_is_captioned_question() {
        // Pi: the prompt was blanked because it repeated the header, so the
        // title is the question.
        let view = question_view(&[question(
            1,
            "Which color should the button be?",
            "",
            &["Blue"],
            None,
        )])
        .unwrap();
        assert_eq!(view.caption, "Question");
        assert_eq!(view.body, "Which color should the button be?");
    }

    #[test]
    fn a_plain_permission_is_captioned_permission_requested() {
        let view = question_view(&[open_permission(
            1,
            "/repo/src/main.rs",
            &["Allow", "Reject"],
        )])
        .unwrap();
        assert_eq!(view.caption, "Permission requested");
        assert_eq!(view.body, "/repo/src/main.rs");
        assert_eq!(
            view.rows,
            answer_options(&["Allow", "Reject"])
                .into_iter()
                .map(DockRow::Option)
                .collect::<Vec<_>>(),
            "a plain permission never offers free text"
        );
    }

    #[test]
    fn a_plan_approval_is_drawn_with_its_options() {
        let plan = Entry::Plan {
            entries: vec![PlanEntryRow {
                content: "Read the design".into(),
                status: "pending".into(),
            }],
            approval: Some(PlanApproval {
                request_id: 4,
                title: "Ready to code?".into(),
                options: answer_options(&["Approve", "Keep planning"]),
                resolved: None,
                expired: false,
            }),
        };
        assert!(is_open(&plan));
        let view = question_view(&[plan]).unwrap();
        assert_eq!(
            (view.request_id, view.caption.as_str(), view.body.as_str()),
            (4, "Plan approval", "Ready to code?")
        );
        assert_eq!(view.rows.len(), 2);
    }

    #[test]
    fn free_text_is_offered_exactly_when_the_question_declares_it() {
        let without = question_view(&[question(1, "Pick", "Which one?", &["A", "B"], None)])
            .unwrap();
        assert!(
            !without
                .rows
                .iter()
                .any(|row| matches!(row, DockRow::FreeText(_))),
            "a question that declared no field gets none"
        );
        let input = AnswerTextInput {
            placeholder: None,
            prefill: None,
        };
        let with = question_view(&[question(
            1,
            "Pick",
            "Which one?",
            &["A", "B"],
            Some(input.clone()),
        )])
        .unwrap();
        assert_eq!(with.rows.len(), 3);
        assert_eq!(
            with.rows.last(),
            Some(&DockRow::FreeText(input)),
            "the typed answer is the last row"
        );
    }

    #[test]
    fn a_request_with_nothing_to_choose_offers_dismiss() {
        let view = question_view(&[open_permission(1, "mystery", &[])]).unwrap();
        assert_eq!(view.rows, vec![DockRow::Dismiss]);
    }

    #[test]
    fn the_selection_stays_on_the_rows_and_stops_at_the_ends() {
        assert_eq!(clamp_selection(7, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
        assert_eq!(step_selection(0, 3, true), 1);
        assert_eq!(step_selection(2, 3, true), 2, "down from the last row stays");
        assert_eq!(step_selection(0, 3, false), 0, "up from the first row stays");
        assert_eq!(
            step_selection(9, 3, false),
            1,
            "a stale index is clamped before it moves"
        );
    }

    #[test]
    fn digits_pick_rows_counting_from_one() {
        assert_eq!(row_for_digit("1", 3), Some(0));
        assert_eq!(row_for_digit("3", 3), Some(2));
        assert_eq!(row_for_digit("4", 3), None);
        assert_eq!(row_for_digit("0", 3), None);
        assert_eq!(row_for_digit("a", 3), None);
        assert_eq!(row_for_digit("enter", 3), None);
    }

    #[test]
    fn badges_number_the_nine_rows_a_digit_reaches() {
        assert_eq!(badge_label(0).as_deref(), Some("1"));
        assert_eq!(badge_label(8).as_deref(), Some("9"));
        assert_eq!(badge_label(9), None);
    }

    #[test]
    fn the_free_text_placeholder_names_its_place() {
        let declared = AnswerTextInput {
            placeholder: Some("Type a color".into()),
            prefill: None,
        };
        assert_eq!(declared.placeholder(true), "Type a color");
        let bare = AnswerTextInput {
            placeholder: None,
            prefill: None,
        };
        assert_eq!(bare.placeholder(true), "Type something else…");
        assert_eq!(bare.placeholder(false), "Type an answer");
    }
}

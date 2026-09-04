//! Tool calls as the gallery draws them: one `step_row` per call, a bordered
//! box per run of consecutive calls, and a `Verb · N` fold for consecutive
//! calls of the same verb — `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `ToolCalls` section).
//!
//! The library never learns what a tool is: `step_row` takes strings, and the
//! grouping is `slice::chunk_by`. What Sirio adds is the mapping from the
//! protocol's `kind` to an icon and a verb, and a clock for the duration.

use bezel::ui::icons;

/// The icon for a protocol tool kind (`Read`, `Edit`, `Execute`, …).
pub(crate) fn tool_icon(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "read" => icons::BOOK,
        "edit" => icons::PEN,
        "execute" => icons::TERMINAL,
        "search" => icons::MAGNIFER,
        "fetch" => icons::DOWNLOAD,
        "think" => icons::CPU,
        "delete" => icons::TRASH_BIN_MINIMALISTIC,
        "move" => icons::ARROW_RIGHT,
        _ => icons::WIDGET,
    }
}

/// The verb a row leads with: the kind word, capitalised; `Tool` when the
/// protocol gave none (an empty kind, or the generic `tool` a pre-#168
/// database restores).
pub(crate) fn tool_verb(kind: &str) -> String {
    let kind = kind.trim();
    if kind.is_empty() || kind.eq_ignore_ascii_case("tool") {
        return "Tool".to_string();
    }
    let mut chars = kind.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    format!("{first}{}", chars.as_str())
}

/// `412ms` under a second, `1.4s` over it — a figure you read at a glance
/// rather than count digits in (gallery `took`).
pub(crate) fn took(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f32 / 1000.0)
    }
}

/// Whether a status reads as a failure — tinted red on the row.
pub(crate) fn is_failed_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "failed" | "cancelled" | "canceled"
    )
}

/// Consecutive same-verb runs as `(start, len)` — the gallery's grouping,
/// straight out of std.
pub(crate) fn verb_folds(kinds: &[&str]) -> Vec<(usize, usize)> {
    let mut folds = Vec::new();
    let mut start = 0;
    for run in kinds.chunk_by(|a, b| a.eq_ignore_ascii_case(b)) {
        folds.push((start, run.len()));
        start += run.len();
    }
    folds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_kind_and_fall_back_to_widget() {
        assert_eq!(tool_icon("Read"), icons::BOOK);
        assert_eq!(tool_icon("edit"), icons::PEN);
        assert_eq!(tool_icon("Execute"), icons::TERMINAL);
        assert_eq!(tool_icon("Search"), icons::MAGNIFER);
        assert_eq!(tool_icon("Fetch"), icons::DOWNLOAD);
        assert_eq!(tool_icon("Think"), icons::CPU);
        assert_eq!(tool_icon("Delete"), icons::TRASH_BIN_MINIMALISTIC);
        assert_eq!(tool_icon("Move"), icons::ARROW_RIGHT);
        assert_eq!(tool_icon("Other"), icons::WIDGET);
        assert_eq!(tool_icon(""), icons::WIDGET);
    }

    #[test]
    fn verbs_are_the_kind_word_or_tool() {
        assert_eq!(tool_verb("Read"), "Read");
        assert_eq!(tool_verb("execute"), "Execute");
        assert_eq!(tool_verb("tool"), "Tool");
        assert_eq!(tool_verb(""), "Tool");
    }

    #[test]
    fn took_reads_at_a_glance() {
        assert_eq!(took(0), "0ms");
        assert_eq!(took(412), "412ms");
        assert_eq!(took(999), "999ms");
        assert_eq!(took(1000), "1.0s");
        assert_eq!(took(1412), "1.4s");
        assert_eq!(took(61_000), "61.0s");
    }

    #[test]
    fn failed_statuses_are_failed_and_cancelled() {
        assert!(is_failed_status("failed"));
        assert!(is_failed_status("Cancelled"));
        assert!(is_failed_status("canceled"));
        assert!(!is_failed_status("completed"));
        assert!(!is_failed_status("in_progress"));
    }

    #[test]
    fn verb_folds_are_consecutive_same_kind_runs() {
        assert_eq!(verb_folds(&[]), vec![]);
        assert_eq!(verb_folds(&["Read"]), vec![(0, 1)]);
        assert_eq!(
            verb_folds(&["Read", "Read", "read", "Execute", "Read"]),
            vec![(0, 3), (3, 1), (4, 1)]
        );
    }
}

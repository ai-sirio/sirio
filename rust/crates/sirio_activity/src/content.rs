//! Layer C — content signal: matches recent pane buffer content
//! (ANSI-stripped tail text) against per-agent rules, as a content-based
//! counterpart to Layer B's title-based detection. Ported from
//! `TillerCore/ScreenManifest.swift`.
//!
//! Returns `None` when no rule matches; `None` must never force a
//! transition (same contract as [`crate::title::detect_status_from_title`]).
//!
//! Only `claude`'s rule is based on a confirmed CLI convention (its
//! numbered permission-prompt menu and "(esc to interrupt)" streaming
//! indicator). The other four adapters share a conservative generic
//! confirmation-prompt matcher — best-effort until verified against live
//! output.

use crate::ansi::strip_ansi;
use crate::status::AgentStatus;

/// Detects a status opinion from live pane tail text.
pub fn detect_content_status(tail_text: &str, agent_id: &str) -> Option<AgentStatus> {
    let _perf = sirio_perf::span("activity.detect_content_status", tail_text.len() as u64);
    let tail_text = strip_ansi(tail_text);
    if tail_text.is_empty() {
        return None;
    }
    match agent_id {
        "claude" => detect_claude(&tail_text),
        "codex" | "opencode" | "pi" | "omp" => detect_generic_prompt(&tail_text),
        _ => None,
    }
}

fn detect_claude(text: &str) -> Option<AgentStatus> {
    if text.contains("Do you want to proceed?") {
        return Some(AgentStatus::NeedsInput);
    }
    if text.contains("esc to interrupt") {
        return Some(AgentStatus::Running);
    }
    None
}

/// Best-effort: matches common CLI confirmation phrasing. A wrong match is
/// worse than no match (`None` is always safe), so this stays narrow.
fn detect_generic_prompt(text: &str) -> Option<AgentStatus> {
    let lower = text.to_lowercase();
    if contains_y_n_prompt(&lower) {
        return Some(AgentStatus::NeedsInput);
    }
    const CONFIRM_PHRASES: [&str; 4] = ["proceed?", "continue?", "allow?", "confirm?"];
    if CONFIRM_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        return Some(AgentStatus::NeedsInput);
    }
    None
}

/// Matches the Swift regex `\(y/n\)|\[y/n\]|\by/n\b`: literal "(y/n)" or
/// "[y/n]", or "y/n" with a regex `\b` word boundary on both sides (regex
/// word characters are letters, digits and underscore — no hyphen).
fn contains_y_n_prompt(lower: &str) -> bool {
    if lower.contains("(y/n)") || lower.contains("[y/n]") {
        return true;
    }
    let mut search_from = 0;
    while let Some(offset) = lower[search_from..].find("y/n") {
        let position = search_from + offset;
        let before_ok = match lower[..position].chars().next_back() {
            None => true,
            Some(c) => !is_regex_word_char(c),
        };
        let after = position + 3;
        let after_ok = match lower[after..].chars().next() {
            None => true,
            Some(c) => !is_regex_word_char(c),
        };
        if before_ok && after_ok {
            return true;
        }
        search_from = position + 1;
    }
    false
}

/// Regex `\w`: letters, digits, underscore.
fn is_regex_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_permission_prompt_is_needs_input() {
        let text = "Some tool output\nDo you want to proceed?\n1. Yes\n2. No";
        assert_eq!(
            detect_content_status(text, "claude"),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn claude_streaming_indicator_is_running() {
        assert_eq!(
            detect_content_status("Thinking about the fix... (esc to interrupt)", "claude"),
            Some(AgentStatus::Running)
        );
    }

    #[test]
    fn claude_unrelated_text_is_nil() {
        assert_eq!(detect_content_status("$ ls\nREADME.md\n", "claude"), None);
    }

    #[test]
    fn generic_yes_no_prompts_are_needs_input() {
        assert_eq!(
            detect_content_status("Run this command? (y/n)", "codex"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_content_status("Allow? [y/n]", "opencode"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_content_status("Continue? y/n", "omp"),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn generic_confirm_words_are_needs_input() {
        assert_eq!(
            detect_content_status("About to delete files. Proceed?", "pi"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_content_status("really allow?", "codex"),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn y_n_without_boundaries_does_not_match() {
        // "say/nothing" contains "y/n" but with a word char on both sides.
        assert_eq!(detect_content_status("say/nothing", "codex"), None);
    }

    #[test]
    fn unknown_agent_and_empty_text_are_nil() {
        assert_eq!(
            detect_content_status("Do you want to proceed?", "unknown"),
            None
        );
        assert_eq!(detect_content_status("", "claude"), None);
    }
}

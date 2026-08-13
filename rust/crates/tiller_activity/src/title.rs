//! Layer B — OSC terminal title conventions: identity assignment and status
//! detection, ported from `TillerCore/AgentTitleIdentity.swift`,
//! `AgentTitleStatus.swift`, `AgentNameBoundaryMatch.swift` and
//! `AgentSignalMerger.swift`.
//!
//! Every CLI has its own title format, captured empirically, not guessed:
//! Claude idles as `✳ …` and works as `. …` or a braille spinner; Pi titles
//! `π - <cwd>`; its omp fork titles `π: <cwd>` — the colon is the only
//! distinguishing mark; Codex 0.144+ also writes a braille "dots" spinner
//! into its title while working.
//!
//! **A bare braille spinner is therefore ambiguous between Claude and Codex
//! and must NOT be used to assign identity** — both are native binaries
//! that Layer D's foreground-process scan recognizes instead.

use std::time::{Duration, Instant};

use crate::ansi::strip_ansi;
use crate::status::AgentStatus;

/// How long a Layer-A (hook) push stays authoritative over Layer-B (title)
/// signals. Mirrors `AgentSignalMerger.debounceInterval` (1.5 s).
pub const TITLE_DEBOUNCE: Duration = Duration::from_millis(1500);

/// The braille "dots" block, where both Claude's and Codex's spinners live.
const BRAILLE_SPINNER_RANGE: std::ops::RangeInclusive<char> = '\u{2800}'..='\u{28FF}';

/// Claude's idle glyph.
const CLAUDE_IDLE_GLYPH: &str = "\u{2733}"; // ✳

/// True if `title` contains any braille block character (a spinner frame).
pub fn contains_braille_spinner(title: &str) -> bool {
    strip_ansi(title)
        .chars()
        .any(|c| BRAILLE_SPINNER_RANGE.contains(&c))
}

/// Word-boundary-safe substring containment, shared by the status keyword
/// matcher and the identity name matcher so both agree on what counts as a
/// word boundary. Ported from `AgentNameBoundaryMatch.containsWord`:
/// `word` must appear with a non-word character (or the string edge) on
/// both sides — so "ready" doesn't match inside "already", and "codex"
/// doesn't match inside "opencodex" or "codex-notes" (hyphen counts as a
/// word character here). Only the FIRST occurrence is considered, exactly
/// like the Swift original.
fn contains_word(word: &str, lower: &str) -> bool {
    let Some(position) = lower.find(word) else {
        return false;
    };
    let before_ok = match lower[..position].chars().next_back() {
        None => true,
        Some(c) => !is_word_char(c),
    };
    let after = position + word.len();
    let after_ok = match lower[after..].chars().next() {
        None => true,
        Some(c) => !is_word_char(c),
    };
    before_ok && after_ok
}

/// What counts as a word character for boundary checks: letters, numbers,
/// hyphen and underscore (`AgentNameBoundaryMatch.isWordChar`).
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c.is_numeric() || c == '-' || c == '_'
}

/// Determines which catalog agent (if any) a terminal title belongs to,
/// with no prior registration required — the identity-detection half of
/// Layer B, used to recognize an agent the user launched by hand.
///
/// Deliberately more conservative than [`detect_status_from_title`], which
/// assumes the identity is already known and is safe to be permissive.
/// Assigning IDENTITY from an arbitrary, unregistered title must avoid
/// mistaking a plain shell prompt showing a branch/cwd name like
/// "pi-notes" for a running Pi agent.
pub fn identify_agent_from_title(title: &str) -> Option<&'static str> {
    let title = strip_ansi(title);
    if title.is_empty() {
        return None;
    }

    // 1. Claude's own glyph prefixes are unambiguous — check first.
    if title
        .strip_prefix(CLAUDE_IDLE_GLYPH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        || title.starts_with(". ")
    {
        return Some("claude");
    }

    // 2. Pi and its omp fork brand titles with the π glyph; the separator
    //    distinguishes them: pi = "π - <cwd>", omp = "π: <cwd>" (captured
    //    live from both CLIs). Checked before the braille fallback so a
    //    working "⠋ π: …" title isn't misread as Claude's spinner.
    if title.contains("π:") {
        return Some("omp");
    }
    if title.contains('π') {
        return Some("pi");
    }

    // 3. Codex/OpenCode/omp announce their own name as literal text.
    //    Word-boundary match (not substring) so a bare cwd/branch title
    //    like "opencode-experiment" or "~/codex-notes" doesn't identify.
    let lower = title.to_lowercase();
    for id in ["codex", "opencode", "omp"] {
        if contains_word(id, &lower) {
            return Some(id);
        }
    }

    // 4. A braille spinner plus the literal word "pi" is Pi. The spinner
    //    ALONE identifies nothing: Claude and Codex 0.144+ both write the
    //    same "dots" cycle (⠋⠙⠹…) into their working titles, so it carries
    //    no identity.
    if contains_braille_spinner(&title) && contains_word("pi", &lower) {
        return Some("pi");
    }

    None
}

/// Derives an `AgentStatus` from a terminal's OSC title text using each
/// CLI's own title conventions (Layer B: a fallback/gap-filler used when a
/// lifecycle hook hasn't fired yet, or doesn't exist for that agent).
///
/// Returns `None` when the title carries no recognizable status opinion;
/// `None` must never force a transition.
pub fn detect_status_from_title(title: &str, agent_id: &str) -> Option<AgentStatus> {
    let title = strip_ansi(title);
    if title.is_empty() {
        return None;
    }
    match agent_id {
        "claude" => detect_claude(&title),
        "pi" | "omp" => detect_pi_family(&title, agent_id),
        _ => detect_generic(&title, agent_id),
    }
}

/// Claude Code sets its own title to "✳ <task>" when idle and either
/// ". <task>" or a braille spinner frame when actively thinking.
fn detect_claude(title: &str) -> Option<AgentStatus> {
    if title
        .strip_prefix(CLAUDE_IDLE_GLYPH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    {
        return Some(AgentStatus::NeedsInput);
    }
    if title.starts_with(". ") || contains_braille_spinner(title) {
        return Some(AgentStatus::Running);
    }
    None
}

/// Pi (and its omp fork) sets a braille spinner while working; the same
/// title minus the spinner, still recognizable as the agent's (its π glyph
/// or literal name), means idle. omp must follow pi conventions too — a
/// title like "π: <cwd>" has no ASCII "omp" anywhere, and routing it
/// through the generic matcher (which needs the literal "omp") would
/// instantly clear title-owned omp panes.
fn detect_pi_family(title: &str, agent_id: &str) -> Option<AgentStatus> {
    if contains_braille_spinner(title) {
        return Some(AgentStatus::Running);
    }
    if title.contains('π') || title.to_lowercase().contains(agent_id) {
        return Some(AgentStatus::NeedsInput);
    }
    None
}

/// Codex/OpenCode have no bespoke glyph convention — match generically on
/// the agent's own name token plus a status keyword (word-boundary).
fn detect_generic(title: &str, agent_id: &str) -> Option<AgentStatus> {
    const WORKING: [&str; 3] = ["working", "thinking", "running"];
    const IDLE: [&str; 3] = ["ready", "idle", "done"];
    const WAITING: [&str; 3] = ["permission", "action required", "waiting"];

    let lower = title.to_lowercase();
    if !contains_word(agent_id, &lower) {
        return None;
    }
    if WAITING.iter().any(|keyword| contains_word(keyword, &lower)) {
        return Some(AgentStatus::NeedsInput);
    }
    if IDLE.iter().any(|keyword| contains_word(keyword, &lower)) {
        return Some(AgentStatus::NeedsInput);
    }
    if WORKING.iter().any(|keyword| contains_word(keyword, &lower)) {
        return Some(AgentStatus::Running);
    }
    None
}

/// Combines Layer A (explicit hook pushes) and Layer B (title-derived
/// status) into one decision per pane: a title-derived signal is dropped if
/// a hook signal for the same pane landed very recently, so a lagging title
/// glyph can't stomp a fresher explicit hook event. Deliberately simple —
/// last-write-wins otherwise.
pub fn should_apply_title_signal(
    last_hook_update_at: Option<Instant>,
    now: Instant,
    debounce: Duration,
) -> bool {
    match last_hook_update_at {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= debounce,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn spinner() -> String {
        "\u{280B}".to_string() // ⠋
    }

    // ------------------------------------------------------------------
    // Identity
    // ------------------------------------------------------------------

    #[test]
    fn claude_glyphs_identify_claude() {
        assert_eq!(identify_agent_from_title("✳ Fix login bug"), Some("claude"));
        assert_eq!(identify_agent_from_title("✳"), Some("claude"));
        assert_eq!(identify_agent_from_title(". Fix login bug"), Some("claude"));
    }

    #[test]
    fn bare_braille_spinner_does_not_identify() {
        // Codex 0.144+ writes the same "dots" braille cycle (⠋⠙⠹…) into its
        // terminal title while working, so a bare spinner no longer implies
        // Claude. Both are native binaries caught by Layer D instead.
        assert_eq!(
            identify_agent_from_title(&format!("{} Fix login bug", spinner())),
            None
        );
    }

    #[test]
    fn name_tokens_identify_codex_opencode_omp() {
        assert_eq!(identify_agent_from_title("codex - thinking"), Some("codex"));
        assert_eq!(
            identify_agent_from_title("opencode running"),
            Some("opencode")
        );
        assert_eq!(identify_agent_from_title("omp working"), Some("omp"));
    }

    #[test]
    fn pi_glyph_titles_identify_pi_and_omp() {
        // pi = "π - <cwd>", omp = "π: <cwd>" — the colon is the only mark.
        assert_eq!(identify_agent_from_title("π - tiller"), Some("pi"));
        assert_eq!(identify_agent_from_title("π"), Some("pi"));
        assert_eq!(identify_agent_from_title("π: tiller"), Some("omp"));
        // Working titles keep the fork distinction.
        assert_eq!(
            identify_agent_from_title(&format!("{} π - tiller", spinner())),
            Some("pi")
        );
        assert_eq!(
            identify_agent_from_title(&format!("{} π: tiller", spinner())),
            Some("omp")
        );
    }

    #[test]
    fn pi_spinner_with_literal_name_identifies_pi() {
        assert_eq!(
            identify_agent_from_title(&format!("{} Pi", spinner())),
            Some("pi")
        );
    }

    #[test]
    fn bare_name_without_spinner_or_glyph_does_not_identify() {
        // No spinner present — bare "pi" text alone is too ambiguous with a
        // branch/cwd name (e.g. "pi-notes") to safely claim identity.
        assert_eq!(identify_agent_from_title("Pi"), None);
    }

    #[test]
    fn plain_shell_prompts_do_not_identify() {
        assert_eq!(identify_agent_from_title("zsh"), None);
        assert_eq!(identify_agent_from_title(""), None);
    }

    #[test]
    fn branch_name_substrings_do_not_falsely_identify() {
        assert_eq!(identify_agent_from_title("opencode-experiment"), None);
        assert_eq!(identify_agent_from_title("~/codex-notes"), None);
        assert_eq!(identify_agent_from_title("pi-notes"), None);
    }

    // ------------------------------------------------------------------
    // Status detection
    // ------------------------------------------------------------------

    #[test]
    fn claude_idle_prefix_is_needs_input() {
        assert_eq!(
            detect_status_from_title("✳ Fix login bug", "claude"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_status_from_title("✳", "claude"),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn claude_working_prefix_and_spinner_are_running() {
        assert_eq!(
            detect_status_from_title(". Fix login bug", "claude"),
            Some(AgentStatus::Running)
        );
        assert_eq!(
            detect_status_from_title(&format!("{} Fix login bug", spinner()), "claude"),
            Some(AgentStatus::Running)
        );
    }

    #[test]
    fn claude_unrecognized_title_is_nil() {
        assert_eq!(detect_status_from_title("zsh", "claude"), None);
    }

    #[test]
    fn pi_spinner_is_running_and_bare_name_is_needs_input() {
        assert_eq!(
            detect_status_from_title(&format!("{} Pi", spinner()), "pi"),
            Some(AgentStatus::Running)
        );
        assert_eq!(
            detect_status_from_title("Pi", "pi"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_status_from_title("π - tiller", "pi"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(detect_status_from_title("zsh", "pi"), None);
    }

    #[test]
    fn omp_follows_pi_family_conventions() {
        assert_eq!(
            detect_status_from_title("π: tiller", "omp"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_status_from_title(&format!("{} π: tiller", spinner()), "omp"),
            Some(AgentStatus::Running)
        );
        assert_eq!(detect_status_from_title("zsh", "omp"), None);
    }

    #[test]
    fn generic_keywords_map_to_statuses() {
        assert_eq!(
            detect_status_from_title("codex - thinking", "codex"),
            Some(AgentStatus::Running)
        );
        assert_eq!(
            detect_status_from_title("opencode running", "opencode"),
            Some(AgentStatus::Running)
        );
        assert_eq!(
            detect_status_from_title("codex ready", "codex"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_status_from_title("opencode done", "opencode"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            detect_status_from_title("codex - action required", "codex"),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(detect_status_from_title("zsh", "codex"), None);
    }

    #[test]
    fn generic_avoids_substring_false_positives() {
        // "~/codex-ready" must not fire "ready" — it's a path fragment.
        assert_eq!(detect_status_from_title("~/codex-ready", "codex"), None);
        // "reworking" contains "working" as a substring but isn't the keyword.
        assert_eq!(
            detect_status_from_title("codex reworking diff", "codex"),
            None
        );
        // "already" contains "ready" as a substring and nothing else matches.
        assert_eq!(detect_status_from_title("codex already", "codex"), None);
    }

    #[test]
    fn empty_title_is_nil() {
        assert_eq!(detect_status_from_title("", "claude"), None);
    }

    // ------------------------------------------------------------------
    // Signal merger (debounce)
    // ------------------------------------------------------------------

    #[test]
    fn no_prior_hook_always_applies_title_signal() {
        assert!(should_apply_title_signal(
            None,
            Instant::now(),
            TITLE_DEBOUNCE
        ));
    }

    #[test]
    fn title_signal_dropped_within_debounce_window() {
        let now = Instant::now();
        let recent_hook = now - Duration::from_millis(500);
        assert!(!should_apply_title_signal(
            Some(recent_hook),
            now,
            TITLE_DEBOUNCE
        ));
    }

    #[test]
    fn title_signal_applies_after_debounce_window() {
        let now = Instant::now();
        let stale_hook = now - Duration::from_secs(2);
        assert!(should_apply_title_signal(
            Some(stale_hook),
            now,
            TITLE_DEBOUNCE
        ));
    }

    #[test]
    fn exactly_at_debounce_boundary_applies() {
        let now = Instant::now();
        let hook_at_boundary = now - TITLE_DEBOUNCE;
        assert!(should_apply_title_signal(
            Some(hook_at_boundary),
            now,
            TITLE_DEBOUNCE
        ));
    }

    #[test]
    fn custom_debounce_interval_is_respected() {
        let now = Instant::now();
        let hook = now - Duration::from_secs(3);
        assert!(!should_apply_title_signal(
            Some(hook),
            now,
            Duration::from_secs(5)
        ));
        assert!(should_apply_title_signal(
            Some(hook),
            now,
            Duration::from_secs(1)
        ));
    }
}

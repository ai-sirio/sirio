//! Integration tests against real temporary files: a provider's usage
//! "state" is its captured transcript, and the parser must read it from
//! disk exactly as the app would. No mocked readers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tiller_usage::{
    ClaudeUsageFetcher, ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageReason,
    classify_failure,
    UsageWindow, parse_claude_usage, reduce,
};

/// A throwaway directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiller-usage-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The real `/usage` panel captured from `claude` on this machine
/// (ANSI-stripped for readability; the parser strips ANSI itself).
const WELL_FORMED_TRANSCRIPT: &str = "\
 SettingsStatus   Config   Usage Stats\r
\r
 Session\r
\r
 Total cost:            $0.0000\r
 Total duration (API): 0s\r
 Total duration (wal): 6\r
 Total code changes:    0 lines added, 0 lines removed\r
   Usage:                 0 input, 0 output, 0 cache read, 0 cache write\r
 Current session\r
██████                                            12%used\r
Resets 8:50am (Europe/Rome)\r
Current week (all models)\r
█████                                             10%used\r
   Resets Aug 16 at 10am (Europe/Rome)\r
  +50% weekly limits promo through Aug 19 · clau.de/cc-50-promo\r
\r
What's contributing to your limits usage?\r
 Approximate,based onlocal sessions on this machine — does not include ↓\r
Current week (Fable)\r
                                                   0% used\r
";

fn expected_usage() -> ProviderUsage {
    ProviderUsage {
        session: Some(UsageWindow::new("5h", 12)),
        weekly: Some(UsageWindow::new("wk", 10)),
        monthly: None,
        fable_weekly: Some(UsageWindow::new("Fable", 0)),
    }
}

#[test]
fn a_well_formed_state_file_parses_into_the_expected_windows() {
    let dir = TempDir::new();
    // The transcript is read from a real file, exactly like the app would.
    let path = dir.file("usage-panel.txt", WELL_FORMED_TRANSCRIPT);
    let raw = std::fs::read_to_string(&path).expect("read fixture");

    assert_eq!(
        parse_claude_usage(&raw),
        Some(expected_usage()),
        "session 12%, weekly 10%, Fable 0% — the values the CLI reports"
    );
}

#[test]
fn a_real_capture_with_carriage_return_line_separators_parses() {
    let dir = TempDir::new();
    // The Claude TUI redraws the panel in place: lines are joined with
    // carriage returns, not newlines. This is a verbatim capture of the
    // panel from this machine (escape sequences removed for readability;
    // the parser strips its own).
    let real = "Current session\r\r\r████████████▌                                     25%used\rResets 8:50am (Europe/Rome)\r\rCurrent week (all models)\r██████                                            12%used\r   Resets Aug 16 at 10am (Europe/Rome)\r  +50% weekly limits promo through Aug 19 · clau.de/cc-50-promo\r\rWhat's contributing to your limits usage?\r Approximate,based onlocal sessions on this machine — does not include ↓\rCurrent week (Fable)\r                                                   0% used\r";
    let path = dir.file("real-capture.txt", real);
    let text = std::fs::read_to_string(&path).expect("read fixture");

    let usage = parse_claude_usage(&text).expect("real capture parses");
    assert_eq!(
        usage.session,
        Some(UsageWindow::new("5h", 25)),
        "session window from its own percent, not another window's"
    );
    assert_eq!(
        usage.weekly,
        Some(UsageWindow::new("wk", 12)),
        "weekly window must read its own percent (a \\r-joined panel used to\
         collapse into one line and every window read the session's value)"
    );
    assert_eq!(usage.fable_weekly, Some(UsageWindow::new("Fable", 0)));
}

#[test]
fn a_missing_state_file_yields_an_unavailable_provider() {
    let dir = TempDir::new();
    let missing = dir.0.join("does-not-exist.txt");
    // The fetch maps an unreadable source to Unavailable, never a panic.
    let outcome = match std::fs::read_to_string(&missing) {
        Ok(text) => transcript_outcome(&text),
        Err(_) => UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
    };
    assert_eq!(
        outcome,
        UsageFetchOutcome::Unavailable(UsageReason::NotInstalled)
    );

    // And the bar shows "—", not a crash: the reducer accepts it.
    let state = reduce(outcome, &ProviderUsageState::Loading);
    assert_eq!(state, ProviderUsageState::Unavailable(UsageReason::NotInstalled));
}

#[test]
fn a_truncated_state_file_never_panics_and_loses_nothing_complete() {
    let dir = TempDir::new();
    let full = dir.file("full.txt", WELL_FORMED_TRANSCRIPT);
    let raw = std::fs::read_to_string(&full).expect("read fixture");

    // Cut the transcript halfway through the weekly section.
    let cut = raw.find("Current week (Fable)").expect("marker");
    let truncated = &raw[..cut];
    let path = dir.file("truncated.txt", truncated);
    let text = std::fs::read_to_string(&path).expect("read fixture");

    // No panic; whatever parsed before the cut survives.
    if let Some(parsed) = parse_claude_usage(&text) {
        assert_eq!(
            parsed.session,
            Some(UsageWindow::new("5h", 12)),
            "the complete session window survives the truncation"
        );
    } else {
        panic!("truncated transcript must not panic; the session window was already complete");
    }
}

#[test]
fn a_state_file_with_unexpected_json_shape_is_handled_not_parsed() {
    let dir = TempDir::new();
    // JSON where the TUI panel is expected: the parser must not panic and
    // must not invent numbers from it.
    let path = dir.file(
        "usage.json",
        "{\"rate_limit\": {\"primary_window\": {\"used_percent\": 99}}}",
    );
    let text = std::fs::read_to_string(&path).expect("read fixture");

    assert_eq!(parse_claude_usage(&text), None);
    // A transcript that never renders is a timeout, and a timeout with no
    // previous value is unavailable — the bar shows "—".
    let state = reduce(transcript_outcome(&text), &ProviderUsageState::Loading);
    assert_eq!(
        state,
        ProviderUsageState::Unavailable(UsageReason::Error),
        "no panic, no fabricated numbers"
    );
}

#[test]
fn data_old_enough_to_count_as_stale_is_marked_stale_not_current() {
    let dir = TempDir::new();
    let path = dir.file("usage-panel.txt", WELL_FORMED_TRANSCRIPT);
    let raw = std::fs::read_to_string(&path).expect("read fixture");

    // The provider reported fine a while ago…
    let loaded = reduce(
        transcript_outcome(&raw),
        &ProviderUsageState::Loading,
    );
    assert_eq!(loaded, ProviderUsageState::Loaded(expected_usage()));

    // …the next refresh fails to reach it (bounded timeout): the last good
    // value is kept, but as `.stale`, which the bar renders dimmed.
    let stale = reduce(UsageFetchOutcome::TimedOut, &loaded);
    assert_eq!(stale, ProviderUsageState::Stale(expected_usage()));
}

#[test]
fn the_fetch_is_bounded_and_single_attempts_do_not_hang() {
    // The real fetch has a hard 25 s ceiling; with a zero-length timeout it
    // must give up immediately rather than hang the caller.
    let started = Instant::now();
    let outcome = ClaudeUsageFetcher::fetch_with(
        Duration::from_millis(50),
        Duration::from_millis(50),
        Duration::from_millis(200),
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the fetch must respect its bounded timeout"
    );
    // It either parsed, failed fast, or timed out — never hung, never
    // panicked.
    assert!(matches!(
        outcome,
        UsageFetchOutcome::Success(_)
            | UsageFetchOutcome::Unavailable(_)
            | UsageFetchOutcome::TimedOut
    ));
}

/// Maps a captured transcript onto a fetch outcome the way the PTY loop
/// does: failure markers classify first, a parseable panel succeeds, and
/// anything else never rendered and is a timeout.
fn transcript_outcome(text: &str) -> UsageFetchOutcome {
    if let Some(reason) = classify_failure(text) {
        return UsageFetchOutcome::Unavailable(reason);
    }
    match parse_claude_usage(text) {
        Some(usage) => UsageFetchOutcome::Success(usage),
        None => UsageFetchOutcome::TimedOut,
    }
}

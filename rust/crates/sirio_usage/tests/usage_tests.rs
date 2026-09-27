//! Integration tests against real temporary files: a provider's usage
//! "state" is its captured transcript, and the parser must read it from
//! disk exactly as the app would. No mocked readers.

// `Path` is only used by the gated-off (Windows: stubbed) login-shell
// fetch tests — see the `#[cfg(unix)]` block below — so it is imported
// conditionally to keep the Windows build warning-free.
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sirio_usage::{
    ClaudeUsageFetcher, ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageReason,
    UsageWindow, classify_failure, parse_claude_usage, reduce,
};

/// A throwaway directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sirio-usage-test-{}-{unique}", std::process::id()));
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

/// The same usage with every window's reset time cleared. The panel's
/// `Resets …` lines are anchored to the wall clock at parse time, so the
/// percent-and-label comparisons below strip them and the reset times are
/// asserted separately as "read" rather than as a value.
fn without_resets(mut usage: ProviderUsage) -> ProviderUsage {
    for window in [
        &mut usage.session,
        &mut usage.weekly,
        &mut usage.monthly,
        &mut usage.fable_weekly,
    ]
    .into_iter()
    .flatten()
    {
        window.resets_at = None;
    }
    usage
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
    if let Some(parsed) = parse_claude_usage(&text).map(without_resets) {
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
    // previous value remains distinguishable from a provider error.
    let state = reduce(transcript_outcome(&text), &ProviderUsageState::Loading);
    assert_eq!(
        state,
        ProviderUsageState::Unavailable(UsageReason::TimedOut),
        "no panic, no fabricated numbers, and no collapsed timeout reason"
    );
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

// ---------------------------------------------------------------------------
// F-SET-11 — the three Claude `Unavailable` reasons reached through the
// *real* PTY/login-shell fetch path, not the pure `transcript_outcome`
// stand-in above. `fetch_with_env` skips dotfile re-sourcing
// (`SIRIO_USAGE_NO_DOTFILES`) so a `PATH` override actually decides what
// `claude` resolves to inside the spawned shell.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The three `fetch_with_env` tests below (and their two helpers) are
// `#[cfg(unix)]` as a block. This is NOT the fixture-convenience case (a
// chmod that merely builds a fixture around a portable subject): all three
// drive `ClaudeUsageFetcher::fetch_with_env` through a real login shell and
// a real PTY, and on Windows that function is an honest stub returning
// `Unavailable(Error)` — its doc comment names ConPTY (`CreatePseudoConsole`)
// as future work. Porting the fixture mechanically (a `.cmd` shim instead of
// a chmod'ed shell script) would produce tests that compile and lie: the
// `NotInstalled` and `LoggedOut` cases would fail against the stub and
// `Error` would pass by accident. Gating here suppresses coverage of an
// admitted gap, not of a portable behaviour — the gate is the TODO list for
// the ConPTY port. When that lands, this block comes back with it.
// ---------------------------------------------------------------------------

/// A `PATH` containing only `dir`, so the fake (or absent) `claude` in it is
/// all the spawned shell can find — the real `claude` on this machine's
/// normal `PATH` never enters the picture.
#[cfg(unix)]
fn isolated_path(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

#[cfg(unix)]
#[test]
fn not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path() {
    let dir = TempDir::new(); // no `claude` binary written into it
    let path = isolated_path(&dir.0);
    let outcome = ClaudeUsageFetcher::fetch_with_env(
        Duration::from_millis(0),
        Duration::from_millis(50),
        Duration::from_secs(5),
        &[
            ("SIRIO_USAGE_NO_DOTFILES", "1"),
            ("PATH", &path),
            // The shell's own "command not found" text is locale-dependent
            // ("comando non trovato" under an Italian locale, observed on
            // this machine); pin the child to the C locale so the English
            // marker `classify_failure` looks for is what it actually
            // prints, independent of the host's `$LANG`.
            ("LC_ALL", "C"),
            ("LANG", "C"),
        ],
    );
    assert_eq!(
        outcome,
        UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
        "a login shell whose dotfiles could reintroduce the real `claude` is \
         skipped, so the shell's own \"command not found\" is what's seen"
    );
}

/// Writes an executable `claude` shell script into `dir` and returns its
/// path.
#[cfg(unix)]
fn fake_claude(dir: &Path, script_body: &str) -> PathBuf {
    let path = dir.join("claude");
    std::fs::write(&path, format!("#!/bin/sh\n{script_body}\n")).expect("write fake claude");
    let mut perms = std::fs::metadata(&path)
        .expect("stat fake claude")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&path, perms).expect("chmod fake claude");
    path
}

#[cfg(unix)]
#[test]
fn logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path() {
    let dir = TempDir::new();
    fake_claude(&dir.0, "echo 'Please run /login to continue'");
    let path = isolated_path(&dir.0);
    let outcome = ClaudeUsageFetcher::fetch_with_env(
        Duration::from_millis(0),
        Duration::from_millis(50),
        Duration::from_secs(5),
        &[("SIRIO_USAGE_NO_DOTFILES", "1"), ("PATH", &path)],
    );
    assert_eq!(
        outcome,
        UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
        "the fake claude's own login prompt is what the fetch sees, not \
         whatever a dotfile-restored real claude would have printed"
    );
}

#[cfg(unix)]
#[test]
fn error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path() {
    let dir = TempDir::new();
    fake_claude(&dir.0, "echo 'failed to load usage'");
    let path = isolated_path(&dir.0);
    let outcome = ClaudeUsageFetcher::fetch_with_env(
        Duration::from_millis(0),
        Duration::from_millis(50),
        Duration::from_secs(5),
        &[("SIRIO_USAGE_NO_DOTFILES", "1"), ("PATH", &path)],
    );
    assert_eq!(
        outcome,
        UsageFetchOutcome::Unavailable(UsageReason::Error),
        "the fake claude's own error marker is what the fetch sees"
    );
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

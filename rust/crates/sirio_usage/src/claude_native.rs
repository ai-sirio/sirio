//! The Claude status-bar fetch over the CLI's own API, replacing the
//! hidden PTY that drives `/usage` and scrapes the panel.
//!
//! The probe spawns `claude` with [`LaunchLine::usage_probe`], writes
//! `initialize` then `get_usage`, and maps the structured answer onto the
//! bar's windows — no terminal, no keystrokes, no panel parsing. Anything
//! this probe cannot answer (no binary, too old a CLI, a refused request)
//! falls back to the PTY scraper in [`super::claude`], which stays as the
//! path for those CLIs.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;
use sirio_claude::{
    Catalog, ClaudeVersion, ControlEnvelope, ControlRequest, LaunchLine, PlanUsage, RateLimitWindow,
};

use super::claude::PROBE_ENV;
use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};

/// Maps a `get_usage` response payload onto the bar's windows: `five_hour`
/// → the `5h` session window, `seven_day` → `wk`, the `model_scoped` entry
/// whose `display_name` is `Fable` → `fable_weekly`. Claude publishes no
/// monthly window. `None` when there are no plan windows to report (an
/// API-key session) or the payload is not a usage answer at all.
#[must_use]
pub fn map_plan_usage(payload: &Value) -> Option<ProviderUsage> {
    let plan = PlanUsage::parse(payload)?;
    if !plan.rate_limits_available {
        return None;
    }
    Some(ProviderUsage {
        session: plan
            .five_hour
            .as_ref()
            .and_then(|window| read_window("5h", window)),
        weekly: plan
            .seven_day
            .as_ref()
            .and_then(|window| read_window("wk", window)),
        monthly: None,
        fable_weekly: plan
            .fable
            .as_ref()
            .and_then(|window| read_window("Fable", window)),
    })
}

/// One window, or `None` when the server left its utilization blank —
/// unknown, never rendered as 0%.
fn read_window(label: &str, window: &RateLimitWindow) -> Option<UsageWindow> {
    let mut usage = UsageWindow::from_percent(label, window.utilization?);
    usage.resets_at = window.resets_at.as_deref().and_then(parse_resets_at);
    Some(usage)
}

/// The server's ISO 8601 reset time as a countdown anchor. `None` when it
/// is missing or unreadable — the window then reports no reset rather than
/// a fabricated one.
fn parse_resets_at(raw: &str) -> Option<SystemTime> {
    let moment = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    let secs = u64::try_from(moment.timestamp()).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

/// The structured usage probe: `initialize`, then `get_usage`, over stdio.
/// Blocking — call it off the render thread, like the scraper.
pub struct NativeUsageFetcher;

impl NativeUsageFetcher {
    /// How long the whole probe may take before giving up, against the
    /// scraper's 25 s: one local exec with two questions needs no TUI
    /// settle loop.
    pub const TIMEOUT: Duration = Duration::from_secs(15);

    pub fn fetch() -> UsageFetchOutcome {
        Self::fetch_with_program(Path::new("claude"))
    }

    /// The probe against one binary. A binary that cannot even be spawned
    /// is `NotInstalled`, never a guess — the caller falls back to the
    /// scraper on exactly this outcome.
    pub fn fetch_with_program(program: &Path) -> UsageFetchOutcome {
        Self::fetch_with_program_timeout(program, Self::TIMEOUT)
    }

    pub(crate) fn fetch_with_program_timeout(
        program: &Path,
        timeout: Duration,
    ) -> UsageFetchOutcome {
        let mut line = LaunchLine::usage_probe();
        // A permission question on a terminal prompt would hang the probe
        // with nobody watching; on stdio every question arrives on the
        // pipe this loop already reads.
        line.args.push("--permission-prompt-tool".to_string());
        line.args.push("stdio".to_string());
        let mut child = match Command::new(program)
            .args(&line.args)
            .envs(line.env)
            .envs(PROBE_ENV.iter().copied())
            .current_dir(crate::probe_working_directory())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
        };

        let init_id = "sirio-usage-initialize";
        let usage_id = "sirio-usage-get-usage";
        // Held open for the whole probe: the plan is to read until both
        // answers arrive, then kill the child — never to hang up early.
        let mut stdin = child.stdin.take();
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return UsageFetchOutcome::Unavailable(UsageReason::Error);
        };
        let (lines_tx, lines_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut lines = std::io::BufReader::new(stdout).lines();
            while let Some(Ok(line)) = lines.next() {
                if lines_tx.send(line).is_err() {
                    break;
                }
            }
        });
        for request in [
            ControlRequest::initialize(init_id),
            ControlRequest::get_usage(usage_id),
        ] {
            let Ok(mut wire) = serde_json::to_string(&request) else {
                break;
            };
            wire.push('\n');
            if let Some(stdin) = stdin.as_mut()
                && std::io::Write::write_all(stdin, wire.as_bytes()).is_err()
            {
                break;
            }
        }

        let deadline = Instant::now() + timeout;
        let mut init_payload: Option<Value> = None;
        let mut usage_payload: Option<Value> = None;
        let mut timed_out = false;
        while init_payload.is_none() || usage_payload.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                timed_out = true;
                break;
            }
            match lines_rx.recv_timeout(remaining) {
                Ok(line) => {
                    let Some(envelope) = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|value| ControlEnvelope::parse(&value))
                    else {
                        continue;
                    };
                    if envelope.request_id == init_id && init_payload.is_none() {
                        init_payload = envelope.payload;
                    } else if envelope.request_id == usage_id && usage_payload.is_none() {
                        usage_payload = envelope.payload;
                    }
                }
                // The child exited (or its output broke) before answering.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    timed_out = true;
                    break;
                }
            }
        }
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();

        if init_payload
            .as_ref()
            .map(Catalog::from_initialize)
            .is_some_and(|catalog| catalog.is_logged_out())
        {
            return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut);
        }
        match usage_payload {
            Some(payload) => match map_plan_usage(&payload) {
                Some(usage) => UsageFetchOutcome::Success(usage),
                None if PlanUsage::parse(&payload)
                    .is_some_and(|plan| !plan.rate_limits_available) =>
                {
                    UsageFetchOutcome::Unavailable(UsageReason::ApiKey)
                }
                None => UsageFetchOutcome::Unavailable(UsageReason::Error),
            },
            None if timed_out => UsageFetchOutcome::TimedOut,
            None => UsageFetchOutcome::Unavailable(UsageReason::Error),
        }
    }
}

/// How long `claude --version` gets. A local exec that prints one line; a
/// CLI that cannot manage it in this window is one whose version is
/// unknown, which takes the scraper either way.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The `claude` binary the native probe may drive, or `None` when the PTY
/// scraper must run instead: nothing named `claude` resolves on PATH, or it
/// answers `--version` below the native protocol floor (or not at all) —
/// an unreadable version is never assumed new enough. `envs` overrides
/// (notably a hermetic `PATH`) win over the inherited environment, so the
/// binary checked here is the one the probe would actually spawn.
pub(crate) fn native_program(envs: &[(&str, &str)]) -> Option<PathBuf> {
    let path_var = envs
        .iter()
        .find(|(key, _)| *key == "PATH")
        .map(|(_, value)| std::ffi::OsString::from(value))
        .or_else(|| std::env::var_os("PATH"))?;
    let found = std::env::split_paths(&path_var)
        .flat_map(|dir| candidates().into_iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())?;
    probe_version(&found, VERSION_PROBE_TIMEOUT).filter(|version| version.meets_floor())?;
    Some(found)
}

#[cfg(windows)]
fn candidates() -> [&'static str; 4] {
    ["claude.exe", "claude.cmd", "claude.bat", "claude"]
}

#[cfg(not(windows))]
fn candidates() -> [&'static str; 1] {
    ["claude"]
}

/// Runs `program --version`, bounded. `None` when it cannot be spawned,
/// hangs, or prints nothing a version can be read from.
fn probe_version(program: &Path, timeout: Duration) -> Option<ClaudeVersion> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    ClaudeVersion::parse(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GET_USAGE: &str = include_str!("../tests/fixtures/get_usage.json");

    #[test]
    fn the_windows_map_onto_the_bars_own_labels() {
        let payload: serde_json::Value = serde_json::from_str(GET_USAGE).expect("valid JSON");
        let usage = map_plan_usage(&payload).expect("a provider usage");
        let session = usage.session.expect("the five-hour window");
        assert_eq!(session.label, "5h");
        assert_eq!(session.used_percent, 42);
        assert!(session.resets_at.is_some());
        assert_eq!(usage.weekly.expect("the weekly window").label, "wk");
        // The per-model bucket the bar shows separately, matched by the
        // server's own display name rather than by position.
        assert_eq!(
            usage.fable_weekly.expect("the Fable window").used_percent,
            12
        );
        assert_eq!(usage.monthly, None, "Claude publishes no monthly window");
    }

    #[test]
    fn an_api_key_session_has_no_plan_windows_to_report() {
        let payload = serde_json::json!({
            "session": {"total_cost_usd": 1.0},
            "rate_limits_available": false,
            "rate_limits": null
        });
        assert_eq!(map_plan_usage(&payload), None);
    }

    #[test]
    fn a_window_the_server_did_not_fill_is_absent_rather_than_zero() {
        let payload = serde_json::json!({
            "rate_limits_available": true,
            "rate_limits": {"five_hour": {"utilization": null, "resets_at": null}}
        });
        let usage = map_plan_usage(&payload).expect("a provider usage");
        assert_eq!(usage.session, None, "a null utilization is unknown, not 0%");
    }

    #[test]
    fn a_percentage_outside_the_range_is_clamped_not_rendered_raw() {
        let payload = serde_json::json!({
            "rate_limits_available": true,
            "rate_limits": {"five_hour": {"utilization": 140.0, "resets_at": null}}
        });
        let usage = map_plan_usage(&payload).expect("a provider usage");
        assert_eq!(usage.session.expect("the window").used_percent, 100);
    }

    #[test]
    fn the_probe_is_unavailable_rather_than_wrong_when_it_cannot_run() {
        assert_eq!(
            NativeUsageFetcher::fetch_with_program(std::path::Path::new("/definitely/missing")),
            UsageFetchOutcome::Unavailable(UsageReason::NotInstalled)
        );
    }
}

//! The Claude provider: the pure `/usage` panel parser and the bounded PTY
//! fetch, ported from `TillerTerminal/ClaudeUsageFetcher.swift` and
//! `TillerCore/ProviderUsage.swift` (parser).
//!
//! Claude's usage state lives only inside its interactive TUI — there is no
//! local usage file — so the fetcher drives a hidden `claude` PTY through
//! the user's login shell, sends `/usage`, and parses the rendered panel,
//! exactly like the Swift app (which itself ports Orca's `claude-pty.ts`).

use std::io;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};

/// One usage window of the Claude panel.
const SESSION_LABEL: &str = "5h";
const WEEKLY_LABEL: &str = "wk";
const FABLE_LABEL: &str = "Fable";

/// Upper bound on the PTY output we retain while polling. The `/usage`
/// panel is a few kilobytes; a runaway TUI must not grow the buffer forever.
const MAX_BUFFER_BYTES: usize = 512 * 1024;

// ---------------------------------------------------------------------------
// Pure parser (ported from `parseClaudeUsage`)
// ---------------------------------------------------------------------------

/// Parses the `claude /usage` TUI text into usage windows. Returns `None`
/// when no window could be read (e.g. the TUI has not rendered yet).
pub fn parse_claude_usage(raw: &str) -> Option<ProviderUsage> {
    // The TUI redraws the panel in place: lines are separated by carriage
    // returns, not newlines (the Swift splits on both — this port must
    // too, or the whole panel collapses into one line and every window
    // reads the session's percent).
    let lines: Vec<String> = strip_ansi(raw)
        .split(['\n', '\r'])
        .map(str::to_string)
        .collect();

    let session = first_percent(&lines, is_session_label)
        .map(|percent| UsageWindow::new(SESSION_LABEL, percent));
    let weekly = first_percent(&lines, |line| is_weekly_label(line) && !contains_fable(line))
        .map(|percent| UsageWindow::new(WEEKLY_LABEL, percent));
    let fable = first_percent(&lines, is_fable_label)
        .map(|percent| UsageWindow::new(FABLE_LABEL, percent));

    let usage = ProviderUsage {
        session,
        weekly,
        monthly: None,
        fable_weekly: fable,
    };
    usage.has_any().then_some(usage)
}

/// Removes CSI and OSC escape sequences, mirroring Swift's `stripANSI`:
/// `ESC [ ... final-byte` and `ESC ] ... BEL`.
fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            let rest = &bytes[i + 1..];
            if rest.first() == Some(&b'[') {
                // CSI: consume until a final byte in @-~ (0x40..=0x7e).
                let mut j = 1;
                while j < rest.len() && !(0x40..=0x7e).contains(&rest[j]) {
                    j += 1;
                }
                i += 1 + j + 1; // ESC + '[' + params + final byte
                continue;
            }
            if rest.first() == Some(&b']') {
                // OSC: consume until BEL (0x07) or end.
                let mut j = 1;
                while j < rest.len() && rest[j] != 0x07 {
                    j += 1;
                }
                i += 1 + j + usize::from(rest[j] == 0x07);
                continue;
            }
        }
        // SAFETY: we only skip full ASCII escape sequences; the rest of the
        // input is UTF-8 and `out` is extended one byte at a time only for
        // ASCII bytes, or a full char otherwise.
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn utf8_char_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// Lowercase, whitespace-stripped line for pattern matching.
fn normalized(line: &str) -> String {
    line.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect()
}

fn is_session_label(line: &str) -> bool {
    let n = normalized(line);
    n.contains("currentsession") || n.contains("sessionlimit") || n.contains("sessionusage")
}

fn is_weekly_label(line: &str) -> bool {
    let n = normalized(line);
    n.contains("currentweek")
        || n.contains("weeklylimit")
        || n.contains("weeklyusage")
        || n.contains("weeklyratelimit")
        || n.contains("7day")
}

/// `\bfable\b` on the original line.
fn contains_fable(line: &str) -> bool {
    line.split(|c: char| !c.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("fable"))
}

fn is_fable_label(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("fable") || (is_weekly_label(line) && contains_fable(line))
}

fn is_section_label(line: &str) -> bool {
    is_session_label(line) || is_weekly_label(line) || is_fable_label(line)
}

/// Finds the label line, then returns the first percent token on that line
/// or the next few lines, stopping at the next (different) section heading.
fn first_percent(lines: &[String], is_label: impl Fn(&str) -> bool) -> Option<u8> {
    for (i, line) in lines.iter().enumerate() {
        if !is_label(line) {
            continue;
        }
        for j in i..(i + 4).min(lines.len()) {
            if j > i && is_section_label(&lines[j]) && !is_label(&lines[j]) {
                break;
            }
            if let Some(percent) = percent_token(&lines[j]) {
                return Some(percent);
            }
        }
    }
    None
}

/// `"12% used"` → 12, `"84% left"` → 16 (remaining inverted), bare `"62%"` → 62.
fn percent_token(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start <= 3 {
                let mut j = i;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'%') {
                    let n: u8 = line[start..i].parse().ok()?;
                    let used = if line.to_lowercase().contains("left") {
                        100u8.saturating_sub(n)
                    } else {
                        n
                    };
                    return Some(used.min(100));
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Failure-marker classification
// ---------------------------------------------------------------------------

/// Maps recognisable TUI failure text onto a [`UsageReason`]; `None` when
/// the output carries none of the failure markers.
pub fn classify_failure(text: &str) -> Option<UsageReason> {
    let lower = text.to_lowercase();
    if lower.contains("command not found") {
        Some(UsageReason::NotInstalled)
    } else if lower.contains("failed to load usage") {
        Some(UsageReason::Error)
    } else if lower.contains("please run /login")
        || lower.contains("not logged in")
        || lower.contains("invalid api key")
    {
        Some(UsageReason::LoggedOut)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The bounded PTY fetch
// ---------------------------------------------------------------------------

/// Drives a hidden `claude` PTY and parses the `/usage` panel, ported from
/// `ClaudeUsageFetcher.fetch`. **Blocking** — call it off the render thread
/// (the status bar runs it on the background executor).
pub struct ClaudeUsageFetcher;

impl ClaudeUsageFetcher {
    /// How long the whole fetch may take before giving up. 60 s, not the
    /// Swift default of 25 s: under load the Claude TUI takes 20 s+ just
    /// to render its prompt, and a fetch that aborts before the panel
    /// appears would report the provider unavailable every cycle. Still
    /// bounded — the fetch runs off the render thread and kills the child
    /// on exit.
    pub const TIMEOUT: Duration = Duration::from_secs(60);
    /// How long the TUI gets to settle before `/usage` is sent.
    pub const SETTLE: Duration = Duration::from_secs(2);
    /// How often the output buffer is polled.
    pub const POLL: Duration = Duration::from_millis(300);

    pub fn fetch() -> UsageFetchOutcome {
        Self::fetch_with(Self::SETTLE, Self::POLL, Self::TIMEOUT)
    }

    /// Internal overload with explicit timing, for tests.
    pub fn fetch_with(settle: Duration, poll: Duration, timeout: Duration) -> UsageFetchOutcome {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut pty = match Pty::spawn(&shell, &["-lc", "claude"]) {
            Ok(pty) => pty,
            Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
        };

        let buffer = pty.buffer();
        let deadline = Instant::now() + timeout;

        // Phase 1 — wait for the TUI prompt to actually render before
        // sending `/usage`. A fixed settle is fragile: under load the TUI
        // takes 20 s+ to boot, and keystrokes typed into the boot buffer
        // get interpreted as something else (a `/usage` typed early opened
        // the session-resume palette instead of the usage panel). The
        // input-line glyph `\u{276f}` (❯) and the status line beside it
        // only appear once the prompt is live. The one-time welcome screen
        // renders the same input line, so dismiss it (Enter) and keep
        // waiting.
        let mut welcome_dismissed = false;
        let mut prompt_seen = false;
        while Instant::now() < deadline && !prompt_seen {
            let text = buffer.lock().expect("usage buffer lock").clone();
            if let Some(reason) = classify_failure(&text) {
                return UsageFetchOutcome::Unavailable(reason);
            }
            if let Some(usage) = parse_claude_usage(&text) {
                return UsageFetchOutcome::Success(usage);
            }
            let lower = text.to_lowercase();
            if !welcome_dismissed
                && (lower.contains("welcome back")
                    || lower.contains("what's new")
                    || lower.contains("getting started")
                    || lower.contains("tips for getting"))
            {
                pty.write(b"\r");
                welcome_dismissed = true;
            }
            if text.contains('\u{276f}') || lower.contains("manual mode") {
                prompt_seen = true;
            }
            if !prompt_seen {
                std::thread::sleep(poll);
            }
        }

        // Phase 2 — send `/usage` now that input is live, then poll for
        // the panel. The welcome screen's sheet can swallow the first
        // keystrokes; when it shows up mid-flight, dismiss it and re-send
        // once.
        std::thread::sleep(settle);
        pty.write(b"/usage\r");
        let mut palette_confirmed = false;
        let mut usage_resent = false;
        // If the panel has not rendered after this long, something opened
        // instead (a palette, a session-resume prompt, a swallowed input):
        // Escape to close it and re-send.
        let resend_every = Duration::from_secs(8);
        let mut last_sent = Instant::now();
        while Instant::now() < deadline {
            let text = buffer.lock().expect("usage buffer lock").clone();
            if let Some(reason) = classify_failure(&text) {
                return UsageFetchOutcome::Unavailable(reason);
            }
            let lower = text.to_lowercase();
            if !welcome_dismissed
                && (lower.contains("welcome back")
                    || lower.contains("what's new")
                    || lower.contains("getting started")
                    || lower.contains("tips for getting"))
            {
                pty.write(b"\r");
                welcome_dismissed = true;
            }
            if !usage_resent && welcome_dismissed {
                // The welcome screen's sheet swallowed the first `/usage`;
                // re-send it once, right after dismissal. (When phase 1
                // already dismissed the welcome, this block is skipped —
                // no double send.)
                pty.write(b"/usage\r");
                usage_resent = true;
                last_sent = Instant::now();
            }
            if last_sent.elapsed() >= resend_every {
                // Nothing rendered since the last send: close whatever is
                // in the way (Escape) and ask again. Escape at the prompt
                // is a harmless no-op.
                pty.write(b"\x1b");
                pty.write(b"/usage\r");
                last_sent = Instant::now();
            }
            // A command-palette prompt ("Show plan usage limits") needs one
            // Enter to reveal the panel.
            if !palette_confirmed && (lower.contains("show plan") || lower.contains("usage limits"))
            {
                pty.write(b"\r");
                palette_confirmed = true;
            }
            if let Some(usage) = parse_claude_usage(&text) {
                return UsageFetchOutcome::Success(usage);
            }
            std::thread::sleep(poll);
        }

        // Kill the child so the reader thread can exit.
        drop(pty);
        UsageFetchOutcome::TimedOut
    }
}

/// A minimal PTY: `posix_openpt` + a login-shell child on the slave side.
/// macOS-only, mirroring the Swift app's `PtyProcess`.
struct Pty {
    master: RawFd,
    child: Child,
    buffer: Arc<Mutex<String>>,
}

impl Pty {
    fn spawn(program: &str, args: &[&str]) -> io::Result<Pty> {
        // SAFETY: standard PTY opening sequence; all error paths check the
        // return values before proceeding.
        let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        if master < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::grantpt(master) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::unlockpt(master) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let slave_path = unsafe {
            let name = libc::ptsname(master);
            if name.is_null() {
                return Err(io::Error::last_os_error());
            }
            PathBuf::from(std::ffi::CStr::from_ptr(name).to_string_lossy().into_owned())
        };
        // SAFETY: slave path comes from ptsname; O_NOCTTY because the child
        // attaches it explicitly via TIOCSCTTY after setsid.
        let slave = unsafe { libc::open(slave_path.to_str().unwrap_or("").as_ptr() as *const _, libc::O_RDWR | libc::O_NOCTTY) };
        if slave < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the slave fd is dup'd for stdout/stderr; ownership of the
        // original passes to the stdin Stdio.
        let slave_stdout = unsafe { libc::dup(slave) };
        let slave_stderr = unsafe { libc::dup(slave) };
        if slave_stdout < 0 || slave_stderr < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut command = Command::new(program);
        command
            .args(args)
            .env("TERM", "xterm-256color")
            // SAFETY: these fds are owned by this function and passed to the
            // child as its standard streams.
            .stdin(unsafe { Stdio::from_raw_fd(slave) })
            .stdout(unsafe { Stdio::from_raw_fd(slave_stdout) })
            .stderr(unsafe { Stdio::from_raw_fd(slave_stderr) });
        unsafe {
            command.pre_exec(move || {
                // Become a session leader with the PTY slave as controlling
                // terminal, like any interactive shell.
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(slave, libc::TIOCSCTTY.into(), 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;

        let buffer = Arc::new(Mutex::new(String::new()));
        let reader_buffer = buffer.clone();
        // SAFETY: `master` stays open for the lifetime of `Pty`; the reader
        // thread exits on EOF (child killed) and is joined via the child's
        // reaping in `Drop`.
        let reader_master = master;
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                let n = unsafe { libc::read(reader_master, chunk.as_mut_ptr().cast(), chunk.len()) };
                if n <= 0 {
                    break;
                }
                let mut buffer = reader_buffer.lock().expect("usage buffer lock");
                if buffer.len() < MAX_BUFFER_BYTES {
                    let take = (n as usize).min(MAX_BUFFER_BYTES - buffer.len());
                    buffer.push_str(&String::from_utf8_lossy(&chunk[..take]));
                }
            }
        });

        Ok(Pty {
            master,
            child,
            buffer,
        })
    }

    fn write(&self, bytes: &[u8]) {
        // SAFETY: master is a valid open fd for the lifetime of `self`.
        unsafe {
            libc::write(
                self.master,
                bytes.as_ptr().cast(),
                bytes.len(),
            );
        }
    }

    fn buffer(&self) -> Arc<Mutex<String>> {
        self.buffer.clone()
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // SAFETY: closing our own fd.
        unsafe {
            libc::close(self.master);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_and_osc() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("a\x1b]0;title\x07b"), "ab");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn percent_token_handles_used_left_and_bare() {
        assert_eq!(percent_token("12%used"), Some(12));
        assert_eq!(percent_token("██████ 12% used"), Some(12));
        assert_eq!(percent_token("84% left"), Some(16));
        assert_eq!(percent_token("62%"), Some(62));
        assert_eq!(percent_token("no percent here"), None);
        assert_eq!(percent_token("9999%"), None); // more than 3 digits
    }

    #[test]
    fn label_matching_follows_the_swift_patterns() {
        assert!(is_session_label("Current session"));
        assert!(is_session_label("Session limit"));
        assert!(is_weekly_label("Current week (all models)"));
        assert!(is_weekly_label("Weekly usage"));
        assert!(is_weekly_label("7 day"));
        assert!(is_fable_label("Current week (Fable)"));
        assert!(is_fable_label("Fable"));
        // The Fable line still matches the weekly pattern — the extraction
        // filter (`is_weekly_label && !contains_fable`) is what excludes it.
        assert!(is_weekly_label("Current week (Fable)"));
        assert!(contains_fable("Current week (Fable)"));
        assert!(!contains_fable("Current week (all models)"));
    }

    #[test]
    fn failure_markers_classify() {
        assert_eq!(
            classify_failure("zsh: command not found: claude"),
            Some(UsageReason::NotInstalled)
        );
        assert_eq!(
            classify_failure("failed to load usage"),
            Some(UsageReason::Error)
        );
        assert_eq!(
            classify_failure("Please run /login to continue"),
            Some(UsageReason::LoggedOut)
        );
        assert_eq!(classify_failure("some other output"), None);
    }
}

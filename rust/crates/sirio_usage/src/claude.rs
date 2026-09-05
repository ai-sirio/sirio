//! The Claude provider: the pure `/usage` panel parser and the bounded PTY
//! fetch, ported from `SirioTerminal/ClaudeUsageFetcher.swift` and
//! `TillerCore/ProviderUsage.swift` (parser).
//!
//! Claude's usage state lives only inside its interactive TUI — there is no
//! local usage file — so the fetcher drives a hidden `claude` PTY (through
//! the user's login shell on Unix, directly under ConPTY on Windows), sends
//! `/usage`, and parses the rendered panel, exactly like the Swift app
//! (which itself ports Orca's `claude-pty.ts`).

use std::io;
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, TimeDelta, TimeZone};

use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};
use crate::user_home_dir;

/// The Claude config directory: `$CLAUDE_CONFIG_DIR`, else `~/.claude` —
/// the same precedence `claude` itself uses.
pub fn claude_config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    user_home_dir()
        .map(|home| home.join(".claude"))
        // No home → an empty config dir: credential reads fail and report
        // "not signed in" instead of killing the process.
        .unwrap_or_default()
}

/// Whether a Claude credentials file (`<config dir>/.credentials.json`)
/// carries a usable credential: an OAuth account (`claudeAiOauth` with an
/// access token) or an API key (`hashedToken`). **Presence, not
/// validity** — the usage fetch answers whether the credential still
/// works; this answers only "has the user signed in".
pub fn claude_has_credentials_at(credentials_file: &Path) -> bool {
    let Ok(data) = std::fs::read(credentials_file) else {
        return false;
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&data) else {
        return false;
    };
    let oauth_token = json
        .get("claudeAiOauth")
        .and_then(|oauth| oauth.get("accessToken"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|token| !token.is_empty());
    if oauth_token {
        return true;
    }
    json.get("hashedToken")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|token| !token.is_empty())
}

/// One usage window of the Claude panel.
const SESSION_LABEL: &str = "5h";
const WEEKLY_LABEL: &str = "wk";
const FABLE_LABEL: &str = "Fable";

/// Upper bound on the PTY output we retain while polling. The `/usage`
/// panel is a few kilobytes; a runaway TUI must not grow the buffer forever.
#[cfg(unix)]
const MAX_BUFFER_BYTES: usize = 512 * 1024;

/// Appends one chunk of PTY output to the shared buffer, honouring
/// [`MAX_BUFFER_BYTES`]: past the cap, further output is dropped rather than
/// retained. (Windows replays its output into a bounded screen grid
/// instead — see [`crate::screen::Screen`].)
#[cfg(unix)]
fn append_capped(buffer: &Mutex<String>, chunk: &[u8]) {
    let mut buffer = buffer.lock().expect("usage buffer lock");
    if buffer.len() < MAX_BUFFER_BYTES {
        let take = chunk.len().min(MAX_BUFFER_BYTES - buffer.len());
        buffer.push_str(&String::from_utf8_lossy(&chunk[..take]));
    }
}

// ---------------------------------------------------------------------------
// Pure parser (ported from `parseClaudeUsage`)
// ---------------------------------------------------------------------------

/// Parses the `claude /usage` TUI text into usage windows. Returns `None`
/// when no window could be read (e.g. the TUI has not rendered yet).
pub fn parse_claude_usage(raw: &str) -> Option<ProviderUsage> {
    parse_claude_usage_at(raw, Local::now())
}

/// [`parse_claude_usage`] with the clock injected. The panel prints reset
/// times without a day (`Resets 8:50am`) or without a year (`Resets Aug 16
/// at 10am`); `now` anchors them to the next such moment so each window's
/// `resets_at` can be counted down from.
pub fn parse_claude_usage_at(raw: &str, now: DateTime<Local>) -> Option<ProviderUsage> {
    // The TUI redraws the panel in place: lines are separated by carriage
    // returns, not newlines (the Swift splits on both — this port must
    // too, or the whole panel collapses into one line and every window
    // reads the session's percent).
    let lines: Vec<String> = strip_ansi(raw)
        .split(['\n', '\r'])
        .map(str::to_string)
        .collect();

    let session = first_window(&lines, is_session_label, now)
        .map(|(percent, resets_at)| window(SESSION_LABEL, percent, resets_at));
    let weekly = first_window(
        &lines,
        |line| is_weekly_label(line) && !contains_fable(line),
        now,
    )
    .map(|(percent, resets_at)| window(WEEKLY_LABEL, percent, resets_at));
    let fable = first_window(&lines, is_fable_label, now)
        .map(|(percent, resets_at)| window(FABLE_LABEL, percent, resets_at));

    let usage = ProviderUsage {
        session,
        weekly,
        monthly: None,
        fable_weekly: fable,
    };
    usage.has_any().then_some(usage)
}

fn window(label: &str, percent: u8, resets_at: Option<DateTime<Local>>) -> UsageWindow {
    UsageWindow {
        resets_at: resets_at.map(SystemTime::from),
        ..UsageWindow::new(label, percent)
    }
}

/// Removes CSI and OSC escape sequences, mirroring Swift's `stripANSI`:
/// `ESC [ ... final-byte` and `ESC ] ... BEL` (or `ESC \`). Safe on a
/// buffer cut mid-sequence, since the parser runs between PTY reads.
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
                // OSC: consume through BEL (0x07) or ST (ESC \). A buffer
                // that ends before either — the parser runs on the live
                // buffer between PTY reads — is consumed to its end.
                let mut consumed = rest.len();
                let mut j = 1;
                while j < rest.len() {
                    if rest[j] == 0x07 {
                        consumed = j + 1;
                        break;
                    }
                    if rest[j] == 0x1b && rest.get(j + 1) == Some(&b'\\') {
                        consumed = j + 2;
                        break;
                    }
                    j += 1;
                }
                i += 1 + consumed;
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
    line.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
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
/// or the next few lines, stopping at the next (different) section heading,
/// together with the section's own `Resets …` line when one follows the
/// percent before the next heading. A window never borrows another's reset.
fn first_window(
    lines: &[String],
    is_label: impl Fn(&str) -> bool,
    now: DateTime<Local>,
) -> Option<(u8, Option<DateTime<Local>>)> {
    for (i, line) in lines.iter().enumerate() {
        if !is_label(line) {
            continue;
        }
        for (offset, candidate) in lines.iter().skip(i).take(4).enumerate() {
            if offset > 0 && is_section_label(candidate) && !is_label(candidate) {
                break;
            }
            if let Some(percent) = percent_token(candidate) {
                let resets_at = lines
                    .iter()
                    .skip(i + offset + 1)
                    .take(3)
                    .take_while(|line| !is_section_label(line))
                    .find_map(|line| parse_reset_line(line, now));
                return Some((percent, resets_at));
            }
        }
    }
    None
}

/// Parses the panel's `Resets …` line into the next such moment in local
/// time, or `None` for any other line. Two forms are printed, both without
/// a year and with an optional `(Zone/Name)` note that is dropped — the
/// panel already renders in the machine's own zone:
///
/// - `Resets 8:50am (Europe/Rome)` — clock only: today, or tomorrow once
///   that time has passed.
/// - `Resets Aug 16 at 10am (Europe/Rome)` — month and day: this year, or
///   next year when the date is far enough behind `now` to have wrapped.
fn parse_reset_line(line: &str, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let trimmed = line.trim();
    let rest = trimmed
        .get(..6)
        .filter(|head| head.eq_ignore_ascii_case("resets"))
        .map(|_| trimmed[6..].trim_start())?;
    let rest = rest.split('(').next().unwrap_or(rest).trim();

    let mut tokens = rest.split_whitespace().peekable();
    let mut date = None;
    if let Some(month) = tokens.peek().and_then(|token| month_number(token)) {
        tokens.next();
        let day: u32 = tokens.next()?.trim_end_matches(',').parse().ok()?;
        date = Some((month, day));
        if tokens
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("at"))
        {
            tokens.next();
        }
    }
    let mut clock = tokens.next()?.to_ascii_lowercase();
    if let Some(meridiem) = tokens
        .peek()
        .map(|token| token.to_ascii_lowercase())
        .filter(|token| token == "am" || token == "pm")
    {
        clock.push_str(&meridiem);
    }
    let (hour, minute) = parse_clock(&clock)?;

    match date {
        Some((month, day)) => {
            let this_year = local_at(now.year(), month, day, hour, minute)?;
            // Dec → Jan: a reset that reads as a month or more in the past
            // is next year's; anything closer is left alone so a stale
            // panel counts down to "now" instead of jumping a year ahead.
            if this_year + TimeDelta::days(30) < now {
                local_at(now.year() + 1, month, day, hour, minute)
            } else {
                Some(this_year)
            }
        }
        None => {
            let today = now.date_naive();
            let candidate = local_at(today.year(), today.month(), today.day(), hour, minute)?;
            if candidate > now {
                Some(candidate)
            } else {
                let tomorrow = today.succ_opt()?;
                local_at(
                    tomorrow.year(),
                    tomorrow.month(),
                    tomorrow.day(),
                    hour,
                    minute,
                )
            }
        }
    }
}

fn local_at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<DateTime<Local>> {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .earliest()
}

/// `"8:50am"` → (8, 50), `"10am"` → (10, 0), `"12am"` → (0, 0),
/// `"12pm"` → (12, 0); a 24-hour `"14:05"` is accepted as-is.
fn parse_clock(clock: &str) -> Option<(u32, u32)> {
    let (digits, meridiem) = match clock.strip_suffix("am") {
        Some(digits) => (digits, Some(false)),
        None => match clock.strip_suffix("pm") {
            Some(digits) => (digits, Some(true)),
            None => (clock, None),
        },
    };
    let (hour, minute) = match digits.split_once(':') {
        Some((hour, minute)) => (hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?),
        None => (digits.parse::<u32>().ok()?, 0),
    };
    if minute > 59 {
        return None;
    }
    let hour = match meridiem {
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some(false) => hour % 12,
        Some(true) => hour % 12 + 12,
        None if hour > 23 => return None,
        None => hour,
    };
    Some((hour, minute))
}

fn month_number(token: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = token.trim_end_matches(',').to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| lower.starts_with(month))
        .map(|index| index as u32 + 1)
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
    } else if lower.contains("trust this folder") {
        // The workspace-trust dialog, not the prompt — its `❯` marks the
        // highlighted "No, exit", so the drive loop must never send Enter
        // into it. `PROBE_ENV` keeps it from appearing; this is the guard
        // for the day that flag stops being honoured.
        Some(UsageReason::Error)
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
    /// How long the whole fetch may take before giving up. This is a hard
    /// upper bound for the provider, including PTY startup and the `/usage`
    /// round trip.
    pub const TIMEOUT: Duration = Duration::from_secs(25);
    /// How long the TUI gets to settle before `/usage` is sent.
    pub const SETTLE: Duration = Duration::from_secs(2);
    /// How often the output buffer is polled.
    pub const POLL: Duration = Duration::from_millis(300);

    pub fn fetch() -> UsageFetchOutcome {
        Self::fetch_with(Self::SETTLE, Self::POLL, Self::timeout())
    }

    /// `Self::TIMEOUT`, unless overridden by `SIRIO_USAGE_CLAUDE_TIMEOUT_MS`
    /// (F-USE-03). This exists purely as a live-driving instrument: a real
    /// 25s hang is too risky to exercise against this project's harness's
    /// own 180s silence kill, so this lets an operator shrink the bound
    /// (below `SETTLE`'s fixed 2s pre-`/usage` sleep, which is unconditional
    /// and not itself clamped to the deadline) and force a genuine
    /// `UsageFetchOutcome::TimedOut` through the real production code path —
    /// a real PTY spawn of the real `claude` binary, not a stand-in outcome
    /// injected around this function. Unset in every normal run, so default
    /// behaviour is exactly `Self::TIMEOUT`.
    fn timeout() -> Duration {
        std::env::var("SIRIO_USAGE_CLAUDE_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Self::TIMEOUT)
    }

    /// Internal overload with explicit timing, for tests.
    pub fn fetch_with(settle: Duration, poll: Duration, timeout: Duration) -> UsageFetchOutcome {
        Self::fetch_with_env(settle, poll, timeout, &[])
    }

    /// Like [`Self::fetch_with`], but with additional environment variables
    /// set on the spawned shell (F-SET-11).
    ///
    /// The production path launches `claude` through a **login** shell
    /// (`-lc`), matching a real terminal. That shell re-sources the user's
    /// login/interactive dotfiles (`.bash_profile`, `.zprofile`, `.zshrc`,
    /// …), which is exactly what a synthetic test cannot control: a `PATH`
    /// or `CLAUDE_CONFIG_DIR` override this call sets on `envs` can be
    /// silently reset by those dotfiles before `claude` ever runs, which
    /// used to make the `NotInstalled`/`LoggedOut`/`Error` unavailable
    /// states unreachable from any test. When `envs` carries
    /// `SIRIO_USAGE_NO_DOTFILES`, the shell is launched without sourcing
    /// login/interactive dotfiles instead, so overrides in `envs` are the
    /// only thing deciding what `claude` resolves to. Production `fetch()`
    /// never sets that key, so real users still get the login shell.
    ///
    /// Unix drives a real PTY (`posix_openpt`/`ptsname`/`TIOCSCTTY`) under a
    /// login shell, exactly like the Swift app's `PtyProcess`. Windows has no
    /// POSIX PTY, no login shell and no dotfiles to skip: it runs `claude`
    /// itself under ConPTY (`CreatePseudoConsole`, through `portable-pty`),
    /// resolved on the probe's own `PATH` — so a `PATH` in `envs` decides
    /// what `claude` resolves to there without any dotfile flag. Only
    /// [`probe_launch`] and [`Pty`] differ per platform; the drive loop
    /// below is shared.
    pub fn fetch_with_env(
        settle: Duration,
        poll: Duration,
        timeout: Duration,
        envs: &[(&str, &str)],
    ) -> UsageFetchOutcome {
        let skip_dotfiles = envs
            .iter()
            .any(|(key, _)| *key == "SIRIO_USAGE_NO_DOTFILES");
        let (program, args) = probe_launch(skip_dotfiles);
        let mut pty = match Pty::spawn_with_env(&program, &args, envs) {
            Ok(pty) => pty,
            Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
        };

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
            let text = pty.text();
            if let Some(reason) = classify_failure(&text) {
                return UsageFetchOutcome::Unavailable(reason);
            }
            if let Some(usage) = parse_claude_usage(&text) {
                return UsageFetchOutcome::Success(settled_usage(usage, &pty, poll, deadline));
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
            let text = pty.text();
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
                return UsageFetchOutcome::Success(settled_usage(usage, &pty, poll, deadline));
            }
            std::thread::sleep(poll);
        }

        // Kill the child so the reader thread can exit.
        drop(pty);
        UsageFetchOutcome::TimedOut
    }
}

/// How long [`settled_usage`] keeps waiting for the panel to finish
/// painting after it first parsed, at most.
const SETTLE_CAP: Duration = Duration::from_secs(2);

/// The panel as read once output stops arriving. The first parse can land
/// on a half-painted panel — ConPTY paints it in pieces, and the Fable
/// window and every `Resets` line arrive after the first percents
/// (confirmed live: an immediate return read session and weekly percents
/// with no reset times and no Fable window). So once `first` parsed, keep
/// polling until one poll interval passes with the text unchanged, bounded
/// by [`SETTLE_CAP`] and the fetch deadline, then parse the final text;
/// `first` stands if that somehow cannot be re-read.
fn settled_usage(
    first: ProviderUsage,
    pty: &Pty,
    poll: Duration,
    deadline: Instant,
) -> ProviderUsage {
    let cap = deadline.min(Instant::now() + SETTLE_CAP);
    let mut seen = pty.text();
    while Instant::now() < cap {
        std::thread::sleep(poll);
        let now = pty.text();
        if now == seen {
            break;
        }
        seen = now;
    }
    parse_claude_usage(&seen).unwrap_or(first)
}

/// The program and arguments the probe PTY runs. Unix launches `claude`
/// through the user's login shell (`-lc`), matching a real terminal; with
/// `skip_dotfiles` the shell is started without sourcing its login and
/// interactive dotfiles, so environment overrides survive (see
/// [`ClaudeUsageFetcher::fetch_with_env`]).
#[cfg(unix)]
fn probe_launch(skip_dotfiles: bool) -> (String, Vec<&'static str>) {
    let shell = login_shell();
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut args: Vec<&str> = if skip_dotfiles {
        if shell_name == "zsh" {
            // `-f`: skip `.zshenv`/`.zshrc`/`.zprofile`/`.zlogin`.
            vec!["-f", "-c"]
        } else {
            // bash (and `sh` symlinked to it): skip both the login
            // profile scripts and the interactive rc file.
            vec!["--noprofile", "--norc", "-c"]
        }
    } else {
        vec!["-lc"]
    };
    args.push("claude");
    (shell, args)
}

/// Windows has no login shell to go through and no dotfiles to skip:
/// `claude` runs directly, resolved by `portable-pty` on the probe's own
/// `PATH` with `PATHEXT` (so the native `claude.exe` is found the way a
/// console would find it).
#[cfg(windows)]
fn probe_launch(_skip_dotfiles: bool) -> (String, Vec<&'static str>) {
    ("claude".to_string(), Vec::new())
}

#[cfg(unix)]
fn login_shell() -> String {
    if let Some(shell) = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .map(std::path::PathBuf::from)
        .filter(|shell| shell.is_file())
    {
        return shell.to_string_lossy().into_owned();
    }
    ["/bin/bash", "/bin/sh", "/usr/bin/bash"]
        .into_iter()
        .find(|shell| std::path::Path::new(shell).is_file())
        .unwrap_or("/bin/sh")
        .to_string()
}

/// A minimal PTY: `posix_openpt` + a login-shell child on the slave side.
/// Unix-only (Linux and macOS both take this path), mirroring the Swift app's
/// `PtyProcess`. See the `fetch_with_env` doc comment for the Windows counterpart.
#[cfg(unix)]
struct Pty {
    master: RawFd,
    child: Child,
    buffer: Arc<Mutex<String>>,
}

#[cfg(unix)]
impl Pty {
    fn spawn_with_env(program: &str, args: &[&str], envs: &[(&str, &str)]) -> io::Result<Pty> {
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
            PathBuf::from(
                std::ffi::CStr::from_ptr(name)
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        // SAFETY: slave path comes from ptsname; O_NOCTTY because the child
        // attaches it explicitly via TIOCSCTTY after setsid.
        let slave = unsafe {
            libc::open(
                slave_path.to_str().unwrap_or("").as_ptr() as *const _,
                libc::O_RDWR | libc::O_NOCTTY,
            )
        };
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

        let mut command = probe_command(program, args, envs);
        command
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
                // TIOCSCTTY's libc-crate type differs by platform: Linux
                // glibc types it as the `ioctl` request's native c_ulong,
                // but macOS/BSD libc types it as a narrower constant (still
                // widened correctly at the ioctl(2) ABI level). `as _`
                // makes this call correct on both without a cfg split.
                if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) < 0 {
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
                let n =
                    unsafe { libc::read(reader_master, chunk.as_mut_ptr().cast(), chunk.len()) };
                if n <= 0 {
                    break;
                }
                append_capped(&reader_buffer, &chunk[..n as usize]);
            }
        });

        Ok(Pty {
            master,
            child,
            buffer,
        })
    }

    fn write(&mut self, bytes: &[u8]) {
        // SAFETY: master is a valid open fd for the lifetime of `self`.
        unsafe {
            libc::write(self.master, bytes.as_ptr().cast(), bytes.len());
        }
    }

    /// Everything the child has written so far — the raw stream, which is
    /// what the TUI itself wrote on a Unix pty.
    fn text(&self) -> String {
        self.buffer.lock().expect("usage buffer lock").clone()
    }
}

/// Windows twin of the Unix [`Pty`]: a ConPTY (`CreatePseudoConsole`)
/// opened through `portable-pty`, the wrapper the terminal pane already
/// builds on, rather than a second hand-rolled ConPTY client. Same shape
/// for the drive loop — a shared output buffer, `write`, kill on drop.
#[cfg(windows)]
struct Pty {
    /// Kept alive for the whole probe: dropping the master closes the
    /// pseudo console, which is what finally unblocks the reader thread
    /// once the child is gone.
    _master: Box<dyn portable_pty::MasterPty + Send>,
    /// Shared with the reader thread, which answers conhost's cursor
    /// queries on the drive loop's behalf (see [`cursor_position_requests`]).
    writer: Arc<Mutex<Box<dyn io::Write + Send>>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// conhost's output is a screen diff, not a text stream; the reader
    /// thread replays it here and [`Self::text`] reads the screen back.
    screen: Arc<Mutex<crate::screen::Screen>>,
}

/// ConPTY's cursor position request, `CSI 6 n` (DSR). conhost emits it as
/// its very first output and **blocks the child's output until the host
/// terminal answers** — confirmed live: without a reply, ten seconds of
/// `cmd /c echo` produced exactly `"\x1b[6n"` and nothing else. A real
/// terminal emulator answers it as a matter of course; this probe has no
/// emulator, so its reader thread answers instead. The Unix probe never
/// needed this: there is no conhost between it and `claude`.
#[cfg(windows)]
const CURSOR_POSITION_REQUEST: &[u8] = b"\x1b[6n";

/// The reply to [`CURSOR_POSITION_REQUEST`]: cursor at row 1, column 1
/// (`CSI 1 ; 1 R`). Any well-formed position unblocks conhost; the probe
/// never renders, so the true position is meaningless.
#[cfg(windows)]
const CURSOR_POSITION_REPLY: &[u8] = b"\x1b[1;1R";

/// Counts the cursor position requests in `chunk`, carrying the tail of
/// the previous chunk in `carry` so a request split across two reads is
/// still seen exactly once.
#[cfg(windows)]
fn cursor_position_requests(carry: &mut Vec<u8>, chunk: &[u8]) -> usize {
    carry.extend_from_slice(chunk);
    let count = carry
        .windows(CURSOR_POSITION_REQUEST.len())
        .filter(|window| *window == CURSOR_POSITION_REQUEST)
        .count();
    let keep = carry.len().min(CURSOR_POSITION_REQUEST.len() - 1);
    carry.drain(..carry.len() - keep);
    count
}

/// The pseudo console's size. The Unix probe never sets one (the TUI copes
/// with an unsized pty); ConPTY needs a real geometry, and a comfortably
/// wide one keeps the `/usage` panel's lines from wrapping into shapes the
/// parser has never seen.
#[cfg(windows)]
const CONPTY_ROWS: u16 = 40;
#[cfg(windows)]
const CONPTY_COLS: u16 = 120;

#[cfg(windows)]
impl Pty {
    fn spawn_with_env(program: &str, args: &[&str], envs: &[(&str, &str)]) -> io::Result<Pty> {
        use portable_pty::{PtySize, native_pty_system};

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: CONPTY_ROWS,
                cols: CONPTY_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io::Error::other)?;
        // A missing `claude` fails right here (`CreateProcessW` finds no
        // program), which the caller reports as `NotInstalled` — the same
        // outcome the Unix login shell's "command not found" yields.
        let child = pair
            .slave
            .spawn_command(probe_command(program, args, envs))
            .map_err(io::Error::other)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
        let writer = Arc::new(Mutex::new(
            pair.master.take_writer().map_err(io::Error::other)?,
        ));

        let screen = Arc::new(Mutex::new(crate::screen::Screen::new(
            usize::from(CONPTY_ROWS),
            usize::from(CONPTY_COLS),
        )));
        let reader_screen = screen.clone();
        let reply_writer = writer.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            let mut carry = Vec::new();
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        reader_screen
                            .lock()
                            .expect("usage screen lock")
                            .feed(&chunk[..n]);
                        for _ in 0..cursor_position_requests(&mut carry, &chunk[..n]) {
                            write_best_effort(&reply_writer, CURSOR_POSITION_REPLY);
                        }
                    }
                }
            }
        });

        Ok(Pty {
            _master: pair.master,
            writer,
            child,
            screen,
        })
    }

    fn write(&mut self, bytes: &[u8]) {
        write_best_effort(&self.writer, bytes);
    }

    /// The screen as conhost currently paints it (scrolled-off rows first),
    /// which is the text-stream shape the parser expects.
    fn text(&self) -> String {
        self.screen.lock().expect("usage screen lock").text()
    }
}

/// Best effort, like the Unix `libc::write`: a closed pty means the child
/// is gone, which the drive loop's timeout already covers.
#[cfg(windows)]
fn write_best_effort(writer: &Mutex<Box<dyn io::Write + Send>>, bytes: &[u8]) {
    let mut writer = writer.lock().expect("usage pty writer lock");
    let _ = writer.write_all(bytes);
    let _ = writer.flush();
}

/// The ConPTY twin of the Unix `probe_command`: same working directory,
/// same `TERM`, same `envs` overrides — as a `portable-pty` builder, which
/// is what resolves `program` on its own `PATH`/`PATHEXT` at spawn time.
#[cfg(windows)]
fn probe_command(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> portable_pty::CommandBuilder {
    let mut command = portable_pty::CommandBuilder::new(program);
    command.args(args);
    command.cwd(crate::probe_working_directory());
    for (key, value) in PROBE_ENV.iter().chain(envs) {
        command.env(key, value);
    }
    command
}

#[cfg(windows)]
impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // `_master` drops after this body, closing the pseudo console and
        // releasing the reader thread.
    }
}

/// Environment every probe launch carries, on both platforms.
///
/// `CLAUDE_CODE_SANDBOXED=1` is the flag `claude` reads to know it is
/// running somewhere already contained; its workspace-trust check returns
/// early on it. Without it, a `claude` started in the probe directory that
/// no human has trusted opens the "Quick safety check … ❯ No, exit / Yes,
/// I trust this folder" dialog instead of its prompt — confirmed live on
/// Windows, where `%TEMP%` had never been trusted: the dialog's `❯` reads
/// as the prompt, the drive loop's Enter selects the highlighted "No,
/// exit", and the fetch times out. The flag is the right answer rather
/// than accepting the dialog on the user's behalf: accepting would persist
/// trust for the probe directory in the user's own `~/.claude.json`, a
/// config change nobody approved, while the flag changes nothing on disk
/// and the probe never asks `claude` to touch the directory anyway.
const PROBE_ENV: &[(&str, &str)] = &[("TERM", "xterm-256color"), ("CLAUDE_CODE_SANDBOXED", "1")];

#[cfg(unix)]
fn probe_command(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(crate::probe_working_directory())
        .envs(PROBE_ENV.iter().copied())
        .envs(envs.iter().copied());
    command
}

#[cfg(unix)]
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
    use chrono::{DateTime, Local, TimeZone};
    #[cfg(unix)]
    use std::path::Path;

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .expect("unambiguous local time")
    }

    #[test]
    fn reset_line_without_a_date_is_the_next_occurrence_of_that_time() {
        // Before 8:50 → today at 8:50.
        assert_eq!(
            parse_reset_line("Resets 8:50am (Europe/Rome)", local(2026, 8, 10, 7, 10)),
            Some(local(2026, 8, 10, 8, 50))
        );
        // At or after 8:50 → tomorrow at 8:50, never a time already gone.
        assert_eq!(
            parse_reset_line("Resets 8:50am (Europe/Rome)", local(2026, 8, 10, 9, 0)),
            Some(local(2026, 8, 11, 8, 50))
        );
        // 12-hour clock edges.
        assert_eq!(
            parse_reset_line("Resets 12pm", local(2026, 8, 10, 7, 0)),
            Some(local(2026, 8, 10, 12, 0))
        );
        assert_eq!(
            parse_reset_line("Resets 12:05am", local(2026, 8, 10, 7, 0)),
            Some(local(2026, 8, 11, 0, 5))
        );
    }

    #[test]
    fn reset_line_with_a_date_reads_month_day_and_time() {
        assert_eq!(
            parse_reset_line(
                "   Resets Aug 16 at 10am (Europe/Rome)",
                local(2026, 8, 10, 7, 10)
            ),
            Some(local(2026, 8, 16, 10, 0))
        );
        // A month before the current one belongs to next year (Dec → Jan).
        assert_eq!(
            parse_reset_line("Resets Jan 2 at 3:30pm", local(2026, 12, 30, 7, 10)),
            Some(local(2027, 1, 2, 15, 30))
        );
    }

    #[test]
    fn lines_that_are_not_a_reset_are_ignored() {
        let now = local(2026, 8, 10, 7, 10);
        assert_eq!(parse_reset_line("Current session", now), None);
        assert_eq!(parse_reset_line("Resets soon", now), None);
        assert_eq!(parse_reset_line("", now), None);
    }

    #[test]
    fn panel_windows_carry_their_own_reset_times() {
        let now = local(2026, 8, 10, 7, 10);
        let panel = "Current session\r\r\r████████████▌                                     25%used\rResets 8:50am (Europe/Rome)\r\rCurrent week (all models)\r██████                                            12%used\r   Resets Aug 16 at 10am (Europe/Rome)\r  +50% weekly limits promo through Aug 19 · clau.de/cc-50-promo\r\rWhat's contributing to your limits usage?\r Approximate,based onlocal sessions on this machine — does not include ↓\rCurrent week (Fable)\r                                                   0% used\r";
        let usage = parse_claude_usage_at(panel, now).expect("panel parses");
        let session = usage.session.expect("session window");
        assert_eq!(session.used_percent, 25);
        assert_eq!(
            session.resets_at,
            Some(local(2026, 8, 10, 8, 50).into()),
            "the session's own Resets line, not the weekly one"
        );
        let weekly = usage.weekly.expect("weekly window");
        assert_eq!(weekly.used_percent, 12);
        assert_eq!(weekly.resets_at, Some(local(2026, 8, 16, 10, 0).into()));
        let fable = usage.fable_weekly.expect("fable window");
        assert_eq!(fable.used_percent, 0);
        assert_eq!(
            fable.resets_at, None,
            "a window without a Resets line reports no reset rather than borrowing one"
        );
    }

    #[cfg(unix)]
    #[test]
    fn usage_probe_command_has_a_safe_working_directory() {
        let command = probe_command("/bin/sh", &["-c", "true"], &[]);
        let cwd = command
            .get_current_dir()
            .expect("usage probes must set a working directory");

        assert_ne!(cwd, Path::new("/"));
        if let Some(home) = user_home_dir() {
            assert_ne!(cwd, home.as_path());
        }
        let sandboxed = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("CLAUDE_CODE_SANDBOXED"))
            .and_then(|(_, value)| value);
        assert_eq!(
            sandboxed,
            Some(std::ffi::OsStr::new("1")),
            "the probe must skip claude's workspace-trust dialog"
        );
    }

    /// Windows twin of the test above: the ConPTY probe's `CommandBuilder`
    /// carries the same safe, absolute working directory, and the `envs`
    /// overrides land on the builder (they are what tests use to point
    /// `claude` resolution somewhere hermetic).
    #[cfg(windows)]
    #[test]
    fn usage_probe_command_has_a_safe_working_directory() {
        let command = probe_command("cmd", &["/c", "exit"], &[("SIRIO_PROBE_MARK", "1")]);
        let cwd = PathBuf::from(
            command
                .get_cwd()
                .expect("usage probes must set a working directory"),
        );

        assert!(cwd.is_absolute(), "{}", cwd.display());
        if let Some(home) = user_home_dir() {
            assert_ne!(cwd, home);
        }
        assert_eq!(
            command.get_env("SIRIO_PROBE_MARK"),
            Some(std::ffi::OsStr::new("1"))
        );
        assert_eq!(
            command.get_env("TERM"),
            Some(std::ffi::OsStr::new("xterm-256color"))
        );
        assert_eq!(
            command.get_env("CLAUDE_CODE_SANDBOXED"),
            Some(std::ffi::OsStr::new("1")),
            "the probe must skip claude's workspace-trust dialog"
        );
    }

    /// conhost's `CSI 6 n` is counted once whether it arrives whole, split
    /// across two reads, or twice in one read; a carry that holds a
    /// complete request already counted is not counted again.
    #[cfg(windows)]
    #[test]
    fn cursor_position_requests_are_counted_across_chunk_boundaries() {
        let mut carry = Vec::new();
        assert_eq!(cursor_position_requests(&mut carry, b"hello \x1b[6n"), 1);
        assert_eq!(cursor_position_requests(&mut carry, b"plain"), 0);
        assert_eq!(cursor_position_requests(&mut carry, b"\x1b["), 0);
        assert_eq!(cursor_position_requests(&mut carry, b"6n"), 1);
        assert_eq!(cursor_position_requests(&mut carry, b"\x1b[6n\x1b[6n"), 2);
        assert_eq!(cursor_position_requests(&mut carry, b""), 0);
        assert_eq!(cursor_position_requests(&mut carry, b"\x1b[6"), 0);
        assert_eq!(cursor_position_requests(&mut carry, b"m"), 0);
    }

    /// The Windows `Pty` is real: a child spawned under ConPTY has its
    /// output captured into the shared buffer the fetch loop polls — which
    /// includes answering conhost's opening cursor query, without which
    /// nothing past `"\x1b[6n"` ever arrives. `cmd` is the one program
    /// every Windows box has.
    #[cfg(windows)]
    #[test]
    fn conpty_probe_captures_the_child_output() {
        let pty = Pty::spawn_with_env("cmd", &["/c", "echo sirio-conpty-marker"], &[])
            .expect("ConPTY spawn of cmd");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if pty.text().contains("sirio-conpty-marker") {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("ConPTY output never reached the screen: {:?}", pty.text());
    }

    /// Live instrument, not a gate: drives the production `fetch()` against
    /// whatever `claude` this machine has and prints the outcome, so a
    /// Windows change to the probe can be checked end to end without the
    /// app (`cargo test -p sirio_usage live_fetch -- --ignored --nocapture`).
    /// Ignored because it needs a signed-in `claude` and ~10 s of wall time.
    #[cfg(windows)]
    #[test]
    #[ignore = "drives the real installed claude; run by hand"]
    fn live_fetch_against_the_installed_claude() {
        let started = Instant::now();
        let outcome = ClaudeUsageFetcher::fetch();
        println!(
            "live claude fetch after {:?}: {outcome:?}",
            started.elapsed()
        );
        let UsageFetchOutcome::Success(usage) = outcome else {
            panic!("expected real usage numbers from the installed claude, got {outcome:?}");
        };
        // The panel always carries these two windows with their reset
        // times; reading them proves the fetch waited for the whole panel,
        // not the first half-painted frame.
        let session = usage.session.expect("session window");
        let weekly = usage.weekly.expect("weekly window");
        assert!(session.resets_at.is_some(), "session reset time");
        assert!(weekly.resets_at.is_some(), "weekly reset time");
    }

    /// With `claude` resolvable nowhere on the probe's `PATH`, the fetch
    /// reports `NotInstalled` — the same outcome the Unix login-shell path
    /// gives — instead of the old `Unsupported` stub (#199), and it does so
    /// from a failed spawn, well inside the timeout.
    #[cfg(windows)]
    #[test]
    fn fetch_reports_not_installed_when_claude_is_off_the_probe_path() {
        let empty =
            std::env::temp_dir().join(format!("sirio-usage-empty-path-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("empty PATH dir");
        let path = empty.to_string_lossy().into_owned();

        let started = Instant::now();
        let outcome = ClaudeUsageFetcher::fetch_with_env(
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_secs(5),
            &[("PATH", path.as_str())],
        );
        let _ = std::fs::remove_dir_all(&empty);

        assert_eq!(
            outcome,
            UsageFetchOutcome::Unavailable(UsageReason::NotInstalled)
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a failed spawn must not wait out the timeout"
        );
    }

    #[test]
    fn strip_ansi_removes_csi_and_osc() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("a\x1b]0;title\x07b"), "ab");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    /// The parser runs on the live buffer between PTY reads, so a chunk
    /// can end in the middle of a sequence. An OSC with no terminator yet
    /// is dropped, not indexed past the end (the live ConPTY probe panicked
    /// on exactly this: `index out of bounds: the len is 272 but the index
    /// is 272`); an OSC closed by `ESC \` (ST) ends there instead of
    /// swallowing everything up to the next BEL.
    #[test]
    fn strip_ansi_survives_a_truncated_osc_and_honours_st() {
        assert_eq!(strip_ansi("a\x1b]0;title"), "a");
        assert_eq!(strip_ansi("a\x1b]0;title\x1b\\b"), "ab");
        assert_eq!(strip_ansi("a\x1b["), "a");
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
        // The workspace-trust dialog is a failure to reach the prompt, never
        // a prompt: pressing Enter into it selects "No, exit".
        assert_eq!(
            classify_failure("Quick safety check … ❯ No, exit  Yes, I trust this folder"),
            Some(UsageReason::Error)
        );
        assert_eq!(classify_failure("some other output"), None);
    }

    /// F-USE-03: `SIRIO_USAGE_CLAUDE_TIMEOUT_MS` overrides `TIMEOUT` when
    /// set and parseable, so a live drive can shrink the bound below
    /// `SETTLE` and force a genuine `TimedOut` in seconds instead of
    /// risking a real 25s hang against this harness's own silence kill.
    /// Unset (or unparseable), it falls back to the unmodified default.
    #[test]
    fn timeout_override_reads_the_env_var_and_falls_back_to_the_default() {
        // SAFETY: single-threaded within this test; the var name is unique
        // to this test and touched nowhere else in the crate.
        unsafe {
            std::env::remove_var("SIRIO_USAGE_CLAUDE_TIMEOUT_MS");
        }
        assert_eq!(ClaudeUsageFetcher::timeout(), ClaudeUsageFetcher::TIMEOUT);

        unsafe {
            std::env::set_var("SIRIO_USAGE_CLAUDE_TIMEOUT_MS", "1500");
        }
        assert_eq!(ClaudeUsageFetcher::timeout(), Duration::from_millis(1500));

        unsafe {
            std::env::set_var("SIRIO_USAGE_CLAUDE_TIMEOUT_MS", "not-a-number");
        }
        assert_eq!(
            ClaudeUsageFetcher::timeout(),
            ClaudeUsageFetcher::TIMEOUT,
            "an unparseable override must fall back, not panic"
        );

        unsafe {
            std::env::remove_var("SIRIO_USAGE_CLAUDE_TIMEOUT_MS");
        }
    }
}

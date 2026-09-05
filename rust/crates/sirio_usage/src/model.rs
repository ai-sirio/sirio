//! The usage value types and the fetch-outcome reducer, ported from
//! `TillerCore/ProviderUsage.swift`.

use std::time::SystemTime;

/// One usage window ("5h", "wk", "Fable", "mo") with its used percentage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageWindow {
    /// The window's label as shown in the bar, e.g. "5h" or "wk".
    pub label: String,
    /// Percentage used, clamped to 0..=100 by construction.
    pub used_percent: u8,
    /// When the window resets, when the source reports it.
    pub resets_at: Option<SystemTime>,
}

impl UsageWindow {
    pub fn new(label: impl Into<String>, used_percent: u8) -> Self {
        Self {
            label: label.into(),
            used_percent: used_percent.min(100),
            resets_at: None,
        }
    }

    /// Constructs a window from an outside numeric percentage, clamping it
    /// to the displayable range and treating non-finite values as unknown
    /// zero rather than allowing them to escape as a bogus percentage.
    pub fn from_percent(label: impl Into<String>, percent: f64) -> Self {
        let used_percent = if percent.is_finite() {
            percent.clamp(0.0, 100.0).floor() as u8
        } else {
            0
        };
        Self {
            label: label.into(),
            used_percent,
            resets_at: None,
        }
    }
}

/// A provider's usage state across its windows.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProviderUsage {
    pub session: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    pub monthly: Option<UsageWindow>,
    pub fable_weekly: Option<UsageWindow>,
}

impl ProviderUsage {
    /// Whether any window was read.
    pub fn has_any(&self) -> bool {
        self.session.is_some()
            || self.weekly.is_some()
            || self.monthly.is_some()
            || self.fable_weekly.is_some()
    }
}

/// Why a provider could not report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageReason {
    /// The CLI is not installed (or not on PATH).
    NotInstalled,
    /// The provider needs credentials that are missing or invalid, with no
    /// more specific reason known (e.g. no auth file at all).
    LoggedOut,
    /// Codex is authenticated with an API key, for which no ChatGPT usage
    /// window is available.
    ApiKey,
    /// A Codex OAuth refresh token was rejected because the server saw it
    /// used more than once (`refresh_token_reused`) — the strongest signal
    /// of a compromised or duplicated token store.
    TokenReused,
    /// A Codex OAuth refresh token was rejected as invalidated
    /// (`refresh_token_invalidated`) — e.g. the user signed out elsewhere.
    TokenRevoked,
    /// A Codex OAuth refresh token was rejected as expired, or the refresh
    /// call failed for a reason `classify_token_refresh_failure` maps to
    /// "expired" (F-CORE-USG-06).
    TokenExpired,
    /// The bounded usage fetch exceeded its time budget before any value was
    /// available to keep as stale data.
    TimedOut,
    /// This platform has no implementation of this provider's fetch, so
    /// nothing was attempted (#199).
    ///
    /// Distinct from [`Self::Error`] on purpose. `Error` means the provider
    /// exists and could not be read, which invites the user to go and find
    /// the fault; this means there was never anything to run here, which is
    /// not a fault and not actionable. Windows reported the Claude fetch as
    /// `Error` for want of this variant, so the status bar said "Claude
    /// error" beside a settings surface saying "Signed in". No provider
    /// returns it today — the Windows Claude fetch runs under ConPTY since
    /// — but the vocabulary stays for the next platform gap.
    Unsupported,
    /// The provider exists but could not be read.
    Error,
}

/// The outcome of one fetch attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageFetchOutcome {
    Success(ProviderUsage),
    Unavailable(UsageReason),
    /// The fetch exceeded its bounded timeout.
    TimedOut,
}

/// The view state of one provider in the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderUsageState {
    Loading,
    Loaded(ProviderUsage),
    /// The last good value, kept because the most recent fetch timed out.
    /// The bar renders it visibly dimmed: stale data must look stale.
    Stale(ProviderUsage),
    Unavailable(UsageReason),
}

/// Maps a fetch outcome onto the next view state, mirroring Swift's
/// `UsageStateReducer`. A timeout keeps the last good value as `.stale`
/// (dimmed) rather than blanking the bar.
pub fn reduce(outcome: UsageFetchOutcome, previous: &ProviderUsageState) -> ProviderUsageState {
    match outcome {
        UsageFetchOutcome::Success(usage) => ProviderUsageState::Loaded(usage),
        UsageFetchOutcome::Unavailable(reason) => ProviderUsageState::Unavailable(reason),
        UsageFetchOutcome::TimedOut => match last_usage(previous) {
            Some(usage) => ProviderUsageState::Stale(usage),
            None => ProviderUsageState::Unavailable(UsageReason::TimedOut),
        },
    }
}

/// The time left until `resets_at`, in the bar's two-unit form: `5d 23h`
/// once a day or more remains, `1h 40m` below a day, `12m` below an hour,
/// and `now` below a minute or once the reset has passed (a reset in the
/// past is never rendered as a negative duration — the next fetch will
/// replace it).
pub fn reset_countdown(resets_at: SystemTime, now: SystemTime) -> String {
    let remaining = resets_at.duration_since(now).unwrap_or_default().as_secs();
    let days = remaining / 86_400;
    let hours = (remaining % 86_400) / 3_600;
    let minutes = (remaining % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "now".to_string()
    }
}

fn last_usage(state: &ProviderUsageState) -> Option<ProviderUsage> {
    match state {
        ProviderUsageState::Loaded(usage) | ProviderUsageState::Stale(usage) => Some(usage.clone()),
        ProviderUsageState::Loading | ProviderUsageState::Unavailable(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage() -> ProviderUsage {
        ProviderUsage {
            session: Some(UsageWindow::new("5h", 12)),
            weekly: Some(UsageWindow::new("wk", 10)),
            monthly: None,
            fable_weekly: Some(UsageWindow::new("Fable", 0)),
        }
    }

    #[test]
    fn reset_countdown_picks_the_two_largest_units() {
        use std::time::{Duration, UNIX_EPOCH};
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let at = |secs: u64| now + Duration::from_secs(secs);
        // Days and hours once a day is reached; minutes drop out.
        assert_eq!(
            reset_countdown(at(5 * 86_400 + 23 * 3_600 + 59 * 60), now),
            "5d 23h"
        );
        assert_eq!(reset_countdown(at(6 * 86_400 + 18 * 3_600), now), "6d 18h");
        // Hours and minutes below a day.
        assert_eq!(reset_countdown(at(3_600 + 40 * 60), now), "1h 40m");
        assert_eq!(reset_countdown(at(2 * 3_600), now), "2h 0m");
        // Minutes alone below an hour; sub-minute reads as "now".
        assert_eq!(reset_countdown(at(12 * 60 + 30), now), "12m");
        assert_eq!(reset_countdown(at(59), now), "now");
        // A reset already in the past is "now", never a negative duration.
        assert_eq!(
            reset_countdown(now - Duration::from_secs(3_600), now),
            "now"
        );
    }

    #[test]
    fn percent_is_clamped_on_construction() {
        assert_eq!(UsageWindow::new("wk", 250).used_percent, 100);
        assert_eq!(UsageWindow::new("wk", 0).used_percent, 0);
        assert_eq!(UsageWindow::from_percent("wk", -5.0).used_percent, 0);
        assert_eq!(UsageWindow::from_percent("wk", 105.0).used_percent, 100);
    }

    #[test]
    fn success_replaces_the_previous_state() {
        let state = reduce(
            UsageFetchOutcome::Success(usage()),
            &ProviderUsageState::Loading,
        );
        assert_eq!(state, ProviderUsageState::Loaded(usage()));
        let stale = reduce(UsageFetchOutcome::TimedOut, &state);
        assert_eq!(stale, ProviderUsageState::Stale(usage()));
        // A later success clears the stale flag.
        let reloaded = reduce(UsageFetchOutcome::Success(usage()), &stale);
        assert_eq!(reloaded, ProviderUsageState::Loaded(usage()));
    }

    #[test]
    fn unavailable_replaces_everything() {
        let loaded = reduce(
            UsageFetchOutcome::Success(usage()),
            &ProviderUsageState::Loading,
        );
        let unavailable = reduce(
            UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
            &loaded,
        );
        assert_eq!(
            unavailable,
            ProviderUsageState::Unavailable(UsageReason::NotInstalled)
        );
        // A timed-out fetch with no previous good value is unavailable too.
        let timed = reduce(UsageFetchOutcome::TimedOut, &ProviderUsageState::Loading);
        assert_eq!(
            timed,
            ProviderUsageState::Unavailable(UsageReason::TimedOut),
            "a timeout must remain distinguishable from a provider error"
        );
    }
}

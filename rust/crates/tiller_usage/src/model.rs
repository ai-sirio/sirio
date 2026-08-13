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
    /// The provider needs credentials that are missing or invalid.
    LoggedOut,
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
            None => ProviderUsageState::Unavailable(UsageReason::Error),
        },
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
        assert_eq!(timed, ProviderUsageState::Unavailable(UsageReason::Error));
    }
}

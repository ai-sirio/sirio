//! One rate-limit pause per forge host (spec §6, §9).
//!
//! A forge that answers "rate limited" answers it to every reader of the same
//! host, so the pause lives in a GPUI global keyed by host: the right panel's
//! list and the chat's session panel stop asking together and resume
//! together. The rules are the list's own: a reset in the future is kept, an
//! absent or passed one pauses a minute, and nothing pauses longer than an
//! hour. `ChangeRequestTab` keeps a pause of its own (it reads a different
//! set of documents) and does not use this one.

use std::collections::HashMap;
use std::sync::Mutex;

use gpui::{App, Global};
use sirio_forge::ForgeError;

/// How long a rate limit that names no reset pauses the readers: one refresh.
pub const DEFAULT_PAUSE_SECS: i64 = 60;
/// No pause outlasts this, whatever reset the forge names.
pub const MAX_PAUSE_SECS: i64 = 3600;

#[derive(Default)]
struct Pauses(Mutex<HashMap<String, i64>>);

impl Global for Pauses {}

/// The Unix second until which no reader may ask `host`, or `None` when there
/// is no pause or it has passed (a passed pause is forgotten).
pub fn paused_until(host: &str, now: i64, cx: &App) -> Option<i64> {
    let pauses = cx.try_global::<Pauses>()?;
    let mut pauses = pauses.0.lock().unwrap();
    match pauses.get(host).copied() {
        Some(until) if now < until => Some(until),
        Some(_) => {
            pauses.remove(host);
            None
        }
        None => None,
    }
}

/// Pauses the host a `RateLimited` error names, until its reset (capped), and
/// rewrites a named reset to the deadline actually used, so "retrying at
/// HH:MM" is a promise. Returns the deadline; `None` for any other error.
pub fn note_rate_limited(error: &mut ForgeError, now: i64, cx: &mut App) -> Option<i64> {
    let ForgeError::RateLimited { host, reset_at } = error else {
        return None;
    };
    let until = pause_for(*reset_at, now);
    if reset_at.is_some() {
        *reset_at = Some(until);
    }
    if !cx.has_global::<Pauses>() {
        cx.set_global(Pauses::default());
    }
    let pauses = cx.global::<Pauses>();
    let mut pauses = pauses.0.lock().unwrap();
    let slot = pauses.entry(host.clone()).or_insert(until);
    *slot = (*slot).max(until);
    Some(*slot)
}

/// Moves every pause `seconds` earlier: GPUI's simulated clock does not move
/// the wall clock the deadlines are read against.
#[cfg(test)]
pub(crate) fn rewind(seconds: i64, cx: &App) {
    if let Some(pauses) = cx.try_global::<Pauses>() {
        for until in pauses.0.lock().unwrap().values_mut() {
            *until -= seconds;
        }
    }
}

/// The deadline for a reset: a reset in the future is kept, any other
/// pauses a minute, and none outlasts an hour.
pub(crate) fn pause_for(reset_at: Option<i64>, now: i64) -> i64 {
    reset_at
        .filter(|at| *at > now)
        .unwrap_or(now + DEFAULT_PAUSE_SECS)
        .min(now + MAX_PAUSE_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn a_reset_in_the_future_is_kept() {
        assert_eq!(pause_for(Some(NOW + 300), NOW), NOW + 300);
    }

    #[test]
    fn no_reset_pauses_a_minute() {
        assert_eq!(pause_for(None, NOW), NOW + 60);
    }

    #[test]
    fn a_reset_in_the_past_pauses_a_minute() {
        assert_eq!(pause_for(Some(NOW - 5), NOW), NOW + 60);
        assert_eq!(pause_for(Some(NOW), NOW), NOW + 60);
    }

    #[test]
    fn a_far_reset_is_capped_at_an_hour() {
        assert_eq!(pause_for(Some(NOW + 86_400), NOW), NOW + 3600);
    }

    fn limited(host: &str, reset_at: Option<i64>) -> ForgeError {
        ForgeError::RateLimited { host: host.to_string(), reset_at }
    }

    #[gpui::test]
    fn a_pause_noted_by_one_reader_holds_the_other(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut error = limited("ghe.test", Some(NOW + 300));
            assert_eq!(note_rate_limited(&mut error, NOW, cx), Some(NOW + 300));
            assert_eq!(paused_until("ghe.test", NOW, cx), Some(NOW + 300));
            assert_eq!(paused_until("gitlab.test", NOW, cx), None);
        });
    }

    #[gpui::test]
    fn a_passed_pause_is_forgotten(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut error = limited("ghe.test", None);
            note_rate_limited(&mut error, NOW, cx);
            assert_eq!(paused_until("ghe.test", NOW + 59, cx), Some(NOW + 60));
            assert_eq!(paused_until("ghe.test", NOW + 60, cx), None);
            assert_eq!(paused_until("ghe.test", NOW, cx), None, "it was forgotten, not just passed");
        });
    }

    #[gpui::test]
    fn the_error_shows_the_deadline_actually_used(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut far = limited("ghe.test", Some(NOW + 86_400));
            note_rate_limited(&mut far, NOW, cx);
            assert!(matches!(far, ForgeError::RateLimited { reset_at: Some(at), .. } if at == NOW + 3600));
            let mut none = limited("gitlab.test", None);
            note_rate_limited(&mut none, NOW, cx);
            assert!(matches!(none, ForgeError::RateLimited { reset_at: None, .. }));
            let mut other = ForgeError::NotFound { host: "ghe.test".into() };
            assert_eq!(note_rate_limited(&mut other, NOW, cx), None);
        });
    }
}

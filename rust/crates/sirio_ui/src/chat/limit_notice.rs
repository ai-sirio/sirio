//! The sentence of a rate-limit row: which window, how full, what was
//! charged and when it resets, in the plan's own words.
//!
//! Every token the CLI sent (an unknown window, status or scope) goes through
//! `visible_text`, so a hidden character in one shows as `⟨U+XXXX⟩`.

use chrono::{DateTime, TimeZone};
use std::fmt::Display;

use super::visible_text::visible_text;

/// The word a known `rateLimitType` reads as, in lower case.
fn window_word(window: &str) -> Option<&'static str> {
    Some(match window {
        "five_hour" => "5-hour",
        "seven_day" | "seven_day_overage_included" => "weekly",
        "seven_day_opus" => "weekly Opus",
        "seven_day_sonnet" => "weekly Sonnet",
        "overage" => "extra",
        _ => return None,
    })
}

/// What a `limitScope` says was charged; any other value is shown as sent.
fn scope_words(scope: &str) -> String {
    match scope {
        "service" => "your individual spend limit".to_string(),
        "group_pool" => "your team's pooled budget".to_string(),
        "channel" => "your channel's monthly spend limit".to_string(),
        other => visible_text(other, false),
    }
}

/// The window as it reads mid-sentence: a known word, or the token as sent.
fn window_inline(window: &str) -> String {
    match window_word(window) {
        Some(word) => word.to_string(),
        None => visible_text(window, false),
    }
}

/// The window as it starts a sentence: a known word takes an upper-case
/// first letter, an unknown token stays exactly as sent.
fn window_leading(window: &str) -> String {
    match window_word(window) {
        Some(word) => {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        None => visible_text(window, false),
    }
}

/// `at` in the reader's local time, as little as says it: the time of day
/// when it is today, the weekday and time within the next six days, and the
/// date and time otherwise (a past date too).
pub(super) fn reset_time<Tz: TimeZone>(at: &DateTime<Tz>, now: &DateTime<Tz>) -> String
where
    Tz::Offset: Display,
{
    let days = at.date_naive().signed_duration_since(now.date_naive()).num_days();
    let format = match days {
        0 => "%H:%M",
        1..=6 => "%a %H:%M",
        _ => "%-d %b %H:%M",
    };
    at.format(format).to_string()
}

/// The sentence of one rate-limit notice. A part whose value is missing is
/// left out.
pub(super) fn rate_limit_sentence<Tz: TimeZone>(
    status: &str,
    window: Option<&str>,
    used_percent: Option<u8>,
    resets_at: Option<DateTime<Tz>>,
    scope: Option<&str>,
    now: &DateTime<Tz>,
) -> String
where
    Tz::Offset: Display,
{
    let mut text = match status {
        "allowed_warning" => {
            let mut text = match window {
                Some(window) => format!("Approaching the {} usage limit", window_inline(window)),
                None => "Approaching the usage limit".to_string(),
            };
            if let Some(percent) = used_percent {
                // A warning is not a refusal: it never reads 100%.
                text.push_str(&format!(": {}% used", percent.min(99)));
            }
            text
        }
        "rejected" => {
            let mut text = match window {
                Some(window) => format!("{} usage limit reached", window_leading(window)),
                None => "Usage limit reached".to_string(),
            };
            if let Some(scope) = scope {
                text.push_str(&format!(" ({})", scope_words(scope)));
            }
            text
        }
        other => {
            let status = visible_text(other, false);
            match window {
                Some(window) => format!("{} usage limit: {status}", window_leading(window)),
                None => format!("Usage limit: {status}"),
            }
        }
    };
    if let Some(at) = resets_at {
        text.push_str(&format!(", resets at {}", reset_time(&at, now)));
    }
    text.push('.');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, day, hour, minute, 0)
            .unwrap()
    }

    fn sentence(
        status: &str,
        window: Option<&str>,
        percent: Option<u8>,
        reset: Option<DateTime<FixedOffset>>,
        scope: Option<&str>,
    ) -> String {
        rate_limit_sentence(status, window, percent, reset, scope, &at(18, 23, 30))
    }

    // Failure list, written before the code: a refusal worded for a warning;
    // an unknown window or status dropped or tidied instead of shown as sent;
    // a missing window, percent, reset or scope leaving a dangling comma,
    // colon or empty parentheses; 99.9% rounded up to 100; a reset one
    // calendar day away read as "today" because fewer than 24 hours remain
    // (23:30 to 00:15); a seventh day or a past date read as a weekday.
    #[test]
    fn rate_limit_sentence_cases() {
        let soon = Some(at(18, 23, 59));
        assert_eq!(
            sentence("allowed_warning", Some("five_hour"), Some(92), soon, None),
            "Approaching the 5-hour usage limit: 92% used, resets at 23:59."
        );
        assert_eq!(
            sentence("allowed_warning", Some("seven_day_opus"), None, None, None),
            "Approaching the weekly Opus usage limit."
        );
        assert_eq!(
            sentence("allowed_warning", None, None, None, None),
            "Approaching the usage limit."
        );
        assert_eq!(
            sentence("allowed_warning", Some("fable_hour"), Some(50), None, None),
            "Approaching the fable_hour usage limit: 50% used."
        );
        assert_eq!(
            sentence(
                "allowed_warning",
                Some("five_hour"),
                Some(sirio_chat::whole_percent(99.9)),
                None,
                None
            ),
            "Approaching the 5-hour usage limit: 99% used."
        );
        assert_eq!(
            sentence("allowed_warning", Some("five_hour"), Some(100), None, None),
            "Approaching the 5-hour usage limit: 99% used."
        );
        assert_eq!(
            sentence("rejected", Some("five_hour"), None, None, None),
            "5-hour usage limit reached."
        );
        assert_eq!(
            sentence("rejected", Some("seven_day_sonnet"), None, None, None),
            "Weekly Sonnet usage limit reached."
        );
        assert_eq!(
            sentence("rejected", Some("overage"), Some(100), None, Some("group_pool")),
            "Extra usage limit reached (your team's pooled budget)."
        );
        assert_eq!(sentence("rejected", None, None, None, None), "Usage limit reached.");
        assert_eq!(
            sentence("rejected", Some("overage"), None, None, Some("service")),
            "Extra usage limit reached (your individual spend limit)."
        );
        assert_eq!(
            sentence("rejected", Some("overage"), None, None, Some("channel")),
            "Extra usage limit reached (your channel's monthly spend limit)."
        );
        assert_eq!(
            sentence("rejected", Some("overage"), None, None, Some("mystery")),
            "Extra usage limit reached (mystery)."
        );
        assert_eq!(
            sentence("queued", Some("seven_day"), None, soon, None),
            "Weekly usage limit: queued, resets at 23:59."
        );
        assert_eq!(sentence("queued", None, None, None, None), "Usage limit: queued.");
        assert_eq!(
            sentence("queued", Some("fable_hour"), None, None, None),
            "fable_hour usage limit: queued."
        );
    }

    // Failure list: a hidden character in a CLI token reaching the row live.
    #[test]
    fn a_hidden_character_in_a_token_is_shown_as_text() {
        let hostile = format!("x{}y", char::from_u32(0x202E).unwrap());
        let text = sentence("rejected", Some(&hostile), None, None, Some(&hostile));
        assert!(text.contains("x⟨U+202E⟩y"), "the override must read as text");
        assert!(!text.contains(char::from_u32(0x202E).unwrap()), "no live override");
    }

    // Failure list: the time of day for a reset on another calendar day; a
    // weekday for one a week out; a plain time for a past date.
    #[test]
    fn reset_time_formats() {
        let now = at(18, 23, 30);
        // 2026-11-18 is a Wednesday.
        assert_eq!(reset_time(&at(18, 23, 59), &now), "23:59");
        assert_eq!(reset_time(&at(18, 1, 0), &now), "01:00");
        assert_eq!(reset_time(&at(19, 0, 15), &now), "Thu 00:15");
        assert_eq!(reset_time(&at(24, 8, 5), &now), "Tue 08:05");
        assert_eq!(reset_time(&at(25, 8, 5), &now), "25 Nov 08:05");
        assert_eq!(reset_time(&at(17, 8, 5), &now), "17 Nov 08:05");
    }
}

//! The `get_usage` response: plan rate-limit windows.
//!
//! Later tasks append `RewindOutcome` and `ContextUsageReport` to this same
//! file; each parses defensively and independently, so a field one of them
//! stops reporting degrades that one number, never the session.

use serde_json::Value;

/// One rate-limit window: how full it is and when it resets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RateLimitWindow {
    /// Percent used on the 0–100 scale. `None` when the server left it
    /// blank — unknown, never read as 0%.
    pub utilization: Option<f64>,
    /// The reset time as the server sent it (ISO 8601), unparsed: this
    /// crate stays free of a date library, and the caller parses it.
    pub resets_at: Option<String>,
}

/// The plan usage a `get_usage` response reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanUsage {
    /// False for API-key sessions, which have no plan windows at all.
    pub rate_limits_available: bool,
    /// The five-hour window the bar shows as `5h`.
    pub five_hour: Option<RateLimitWindow>,
    /// The seven-day window the bar shows as `wk`.
    pub seven_day: Option<RateLimitWindow>,
    /// The model-scoped window for Fable, matched by the server's own
    /// `display_name` rather than by position.
    pub fable: Option<RateLimitWindow>,
}

impl PlanUsage {
    /// Reads the `get_usage` response payload. `None` only when the payload
    /// is not an object at all; a missing or `false`
    /// `rate_limits_available` parses to a usage the caller reports as
    /// having no windows, which is distinct from a malformed answer.
    #[must_use]
    pub fn parse(payload: &Value) -> Option<Self> {
        let body = payload.as_object()?;
        let limits = body.get("rate_limits");
        Some(Self {
            rate_limits_available: body
                .get("rate_limits_available")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            five_hour: limits
                .and_then(|limits| limits.get("five_hour"))
                .and_then(read_window),
            seven_day: limits
                .and_then(|limits| limits.get("seven_day"))
                .and_then(read_window),
            fable: limits
                .and_then(|limits| limits.get("model_scoped"))
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries.iter().find_map(|entry| {
                        (entry.get("display_name").and_then(Value::as_str) == Some("Fable"))
                            .then(|| read_window(entry))
                            .flatten()
                    })
                }),
        })
    }
}

/// Reads one window object. `None` when it is not an object; a window that
/// is an object but carries no numbers still parses, with every field
/// absent rather than zeroed.
fn read_window(value: &Value) -> Option<RateLimitWindow> {
    let window = value.as_object()?;
    Some(RateLimitWindow {
        utilization: window.get("utilization").and_then(Value::as_f64),
        resets_at: window
            .get("resets_at")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// What a rewind did, or would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RewindOutcome {
    /// Whether the CLI can restore to this message at all.
    pub can_rewind: bool,
    /// Why not, in the CLI's own words.
    pub error: Option<String>,
    /// The files touched.
    pub files_changed: Vec<String>,
    pub insertions: u64,
    pub deletions: u64,
    /// Tracked files the CLI refused to touch because a symlink or another
    /// non-regular file now sits where one used to be. Only a real rewind
    /// reports this; a preview never does.
    pub skipped_links: u64,
}

impl RewindOutcome {
    /// Reads the response. `None` when it carries no `canRewind` at all —
    /// an answer this build cannot read must not be shown as a success.
    #[must_use]
    pub fn parse(payload: &Value) -> Option<Self> {
        let can_rewind = payload.get("canRewind")?.as_bool()?;
        Some(Self {
            can_rewind,
            error: payload
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_string),
            files_changed: payload
                .get("filesChanged")
                .and_then(|files| files.as_array())
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|file| file.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            insertions: payload
                .get("insertions")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            deletions: payload
                .get("deletions")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            skipped_links: payload
                .get("skippedLinks")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
    }
}

/// What the CLI reported about how full the context window is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextUsageReport {
    /// Tokens currently in the context, as counted by the CLI.
    pub total_tokens: u64,
    /// The window those tokens are measured against. Never zero in a value
    /// that exists: a zero denominator renders as a number nobody computed.
    pub raw_max_tokens: u64,
    /// The model the CLI measured against, when it named one.
    pub model: Option<String>,
}

impl ContextUsageReport {
    /// Reads the `get_context_usage` response. `None` when it carries no
    /// window to measure against — the meter then says unknown rather than
    /// zero.
    #[must_use]
    pub fn parse(payload: &Value) -> Option<Self> {
        let raw_max_tokens = payload.get("rawMaxTokens").and_then(Value::as_u64)?;
        if raw_max_tokens == 0 {
            return None;
        }
        Some(Self {
            total_tokens: payload
                .get("totalTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            raw_max_tokens,
            model: payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

#[cfg(test)]
mod rewind_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_dry_run_reports_what_it_would_change() {
        let outcome = RewindOutcome::parse(&json!({
            "canRewind": true,
            "filesChanged": ["/repo/a.rs", "/repo/b.rs"],
            "insertions": 12,
            "deletions": 4
        }))
        .expect("a rewind outcome");
        assert!(outcome.can_rewind);
        assert_eq!(outcome.files_changed.len(), 2);
        assert_eq!(outcome.insertions, 12);
        assert_eq!(outcome.deletions, 4);
        assert_eq!(outcome.skipped_links, 0);
        assert!(outcome.error.is_none());
    }

    #[test]
    fn a_refusal_carries_the_clis_own_sentence_and_offers_no_button() {
        let outcome = RewindOutcome::parse(&json!({
            "canRewind": false,
            "error": "File checkpointing is not enabled for this session"
        }))
        .expect("a rewind outcome");
        assert!(!outcome.can_rewind);
        assert_eq!(
            outcome.error.as_deref(),
            Some("File checkpointing is not enabled for this session")
        );
    }

    #[test]
    fn links_a_real_rewind_refused_to_touch_are_counted() {
        // A symlink where a tracked file used to be is a file the CLI will
        // not restore. Saying "12 files restored" while two were skipped
        // would be the transcript lying about the filesystem.
        let outcome = RewindOutcome::parse(&json!({
            "canRewind": true, "filesChanged": ["/repo/a.rs"], "skippedLinks": 2
        }))
        .expect("a rewind outcome");
        assert_eq!(outcome.skipped_links, 2);
    }

    #[test]
    fn a_shape_this_build_cannot_read_is_none_rather_than_an_empty_success() {
        assert_eq!(RewindOutcome::parse(&json!({})), None);
        assert_eq!(RewindOutcome::parse(&json!("nope")), None);
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_summary_answers_with_the_window_it_measured_against() {
        let report = ContextUsageReport::parse(&json!({
            "totalTokens": 48_000, "rawMaxTokens": 200_000, "maxTokens": 180_000,
            "percentage": 24, "model": "claude-fable-5-1", "categories": []
        }))
        .expect("a context report");
        assert_eq!(report.total_tokens, 48_000);
        // The raw window, not the autocompact one: the meter shows how full
        // the context is, not how close compaction is.
        assert_eq!(report.raw_max_tokens, 200_000);
    }

    #[test]
    fn a_report_without_a_window_is_none_rather_than_a_zero() {
        // A zero denominator renders as "0%" — a number nobody computed.
        assert_eq!(ContextUsageReport::parse(&json!({"totalTokens": 10})), None);
        assert_eq!(
            ContextUsageReport::parse(&json!({"totalTokens": 10, "rawMaxTokens": 0})),
            None
        );
    }
}

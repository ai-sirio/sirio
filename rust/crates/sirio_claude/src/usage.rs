//! The `get_usage` response: plan rate-limit windows.
//!
//! Later tasks append `RewindOutcome` and `ContextUsageReport` to this same
//! file; each parses defensively and independently, so a field one of them
//! stops reporting degrades that one number, never the session.

use serde_json::Value;

/// A percent as the whole number a reader sees: `value` is already on the
/// 0–100 scale, rounded down. The epsilon keeps IEEE products whole:
/// `0.57 * 100.0` is `56.99999999999999` and must read 57. A value outside
/// 0–100 is clamped into it, so 100% is possible here; NaN reads 0. A
/// warning's own sentence caps itself under 100.
#[must_use]
pub fn whole_percent(value: f64) -> u8 {
    // Truncation is the point: the value is clamped to 0..=100 first.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = (value + 1e-9).floor().clamp(0.0, 100.0) as u8;
    whole
}

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

/// The `usage_report` a local `/usage` carries: this chat's totals, the plan's
/// limit rows and the extra-usage line, each read apart so one the CLI changed
/// costs that part and not the card.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageReport {
    /// This chat's cost, time, edits and tokens. `None` without `session`.
    pub totals: Option<SessionTotals>,
    /// The plan's limit rows, or that they cannot be read.
    pub limits: PlanLimits,
    /// The extra usage (overage) line, from `rate_limits.extra_usage`.
    pub extra: Option<ExtraUsage>,
}

/// The plan's limit rows.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanLimits {
    /// `rate_limits` or `limits` is absent, null or the wrong type: the CLI
    /// could not say. Distinct from no rows, which it said.
    Unavailable,
    /// The rows, in the CLI's order; empty when the plan has none.
    Rows(Vec<LimitRow>),
}

/// One row of the plan's limits.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitRow {
    pub kind: String,
    pub group: String,
    /// On the 0–100 scale. `None` when the CLI left it out: unknown, never 0.
    pub percent: Option<f64>,
    /// The reset as the CLI sent it (ISO 8601), unparsed.
    pub resets_at: Option<String>,
    /// `scope.model.display_name`, else `scope.surface.display_name`.
    pub scope_name: Option<String>,
    pub severity: String,
    pub is_active: bool,
}

/// This chat's totals, from `session` (snake_case keys).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionTotals {
    pub cost_usd: Option<f64>,
    pub api_ms: Option<u64>,
    pub wall_ms: Option<u64>,
    pub lines_added: Option<u64>,
    pub lines_removed: Option<u64>,
    /// One entry per model, in the CLI's order.
    pub models: Vec<(String, ModelTokens)>,
}

/// One model's token counts, from a `model_usage` entry (camelCase keys). A
/// count the CLI left out is `None`: the card leaves it out, never shows 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelTokens {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

/// The extra usage (overage) state, from `rate_limits.extra_usage`. Amounts
/// are in minor units.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraUsage {
    pub enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used: Option<f64>,
    pub currency: Option<String>,
}

impl UsageReport {
    /// Reads the raw `usage_report`. `None` unless it is an object that has
    /// `session` or `rate_limits`.
    #[must_use]
    pub fn parse(report: &Value) -> Option<Self> {
        let report = report.as_object()?;
        if !report.contains_key("session") && !report.contains_key("rate_limits") {
            return None;
        }
        let rate_limits = report.get("rate_limits").and_then(Value::as_object);
        Some(Self {
            totals: report.get("session").and_then(read_totals),
            limits: rate_limits
                .and_then(|limits| limits.get("limits"))
                .and_then(Value::as_array)
                .map_or(PlanLimits::Unavailable, |rows| {
                    PlanLimits::Rows(rows.iter().filter_map(read_row).collect())
                }),
            extra: rate_limits
                .and_then(|limits| limits.get("extra_usage"))
                .and_then(read_extra),
        })
    }
}

/// The text of `key`, or `None` for anything that is not a string.
fn text_of(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_string)
}

/// One limit row; `None` when it is not an object.
fn read_row(value: &Value) -> Option<LimitRow> {
    let row = value.as_object()?;
    let scope = row.get("scope");
    let display_name = |part: &str| {
        scope
            .and_then(|scope| scope.get(part))
            .and_then(|part| part.get("display_name"))
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    };
    Some(LimitRow {
        kind: text_of(row, "kind").unwrap_or_default(),
        group: text_of(row, "group").unwrap_or_default(),
        percent: row.get("percent").and_then(Value::as_f64),
        resets_at: text_of(row, "resets_at"),
        scope_name: display_name("model").or_else(|| display_name("surface")),
        severity: text_of(row, "severity").unwrap_or_default(),
        is_active: row.get("is_active").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// `session`, or `None` when it is not an object.
fn read_totals(value: &Value) -> Option<SessionTotals> {
    let session = value.as_object()?;
    let count = |key: &str| session.get(key).and_then(Value::as_u64);
    Some(SessionTotals {
        cost_usd: session.get("total_cost_usd").and_then(Value::as_f64),
        api_ms: count("total_api_duration_ms"),
        wall_ms: count("total_duration_ms"),
        lines_added: count("total_lines_added"),
        lines_removed: count("total_lines_removed"),
        models: session
            .get("model_usage")
            .and_then(Value::as_object)
            .map(|models| {
                models
                    .iter()
                    .filter_map(|(name, usage)| {
                        let usage = usage.as_object()?;
                        let tokens = |key: &str| usage.get(key).and_then(Value::as_u64);
                        Some((
                            name.clone(),
                            ModelTokens {
                                input: tokens("inputTokens"),
                                output: tokens("outputTokens"),
                                cache_read: tokens("cacheReadInputTokens"),
                                cache_write: tokens("cacheCreationInputTokens"),
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// `rate_limits.extra_usage`, or `None` when it is not an object.
fn read_extra(value: &Value) -> Option<ExtraUsage> {
    let extra = value.as_object()?;
    Some(ExtraUsage {
        enabled: extra.get("is_enabled").and_then(Value::as_bool).unwrap_or(false),
        monthly_limit: extra.get("monthly_limit").and_then(Value::as_f64),
        used: extra.get("used_credits").and_then(Value::as_f64),
        currency: text_of(extra, "currency"),
    })
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

#[cfg(test)]
mod usage_report_tests {
    use super::*;
    use serde_json::json;

    fn rows(report: &UsageReport) -> &[LimitRow] {
        match &report.limits {
            PlanLimits::Rows(rows) => rows,
            PlanLimits::Unavailable => panic!("expected rows"),
        }
    }

    #[test]
    fn usage_report_parse_cases() {
        // Guard: holds with a parser that reads nothing (it returns `None` for
        // everything); the cases below are the ones that need the reading.
        // A report that is not an object, or names neither part, is not one.
        for not_a_report in [json!(null), json!(7), json!("x"), json!([]), json!({}), json!({"x": 1})] {
            assert_eq!(UsageReport::parse(&not_a_report), None);
        }

        // `rate_limits` that is not an object is unavailable, extra and all;
        // `session` is still read.
        for bad in [json!(null), json!(7), json!("x"), json!([1])] {
            let report = UsageReport::parse(&json!({
                "session": {"total_cost_usd": 2.5}, "rate_limits": bad
            }))
            .expect("session is present");
            assert_eq!(report.limits, PlanLimits::Unavailable);
            assert_eq!(report.extra, None);
            assert_eq!(report.totals.expect("totals").cost_usd, Some(2.5));
        }

        // `rate_limits: null` beside an `extra_usage` that sits at the report's
        // top level: the extra line lives under `rate_limits`, so there is none.
        let report = UsageReport::parse(&json!({
            "rate_limits": null, "extra_usage": {"is_enabled": true, "used_credits": 5}
        }))
        .expect("rate_limits is present");
        assert_eq!(report.limits, PlanLimits::Unavailable);
        assert_eq!(report.extra, None);

        // `limits` absent or of the wrong type is unavailable while the extra
        // line, which does not depend on the rows, is still read.
        for limits in [None, Some(json!(null)), Some(json!("x")), Some(json!({}))] {
            let mut rate_limits = json!({"extra_usage": {"is_enabled": false}});
            if let Some(limits) = limits {
                rate_limits["limits"] = limits;
            }
            let report = UsageReport::parse(&json!({ "rate_limits": rate_limits })).expect("a report");
            assert_eq!(report.limits, PlanLimits::Unavailable);
            assert_eq!(report.totals, None);
            assert!(!report.extra.expect("extra").enabled);
        }

        // An empty list is a statement ("none reported"), not unavailable.
        let empty = UsageReport::parse(&json!({"rate_limits": {"limits": []}})).expect("a report");
        assert_eq!(empty.limits, PlanLimits::Rows(vec![]));

        // A row that is not an object is skipped; the rest keep their order.
        let report = UsageReport::parse(&json!({"rate_limits": {"limits": [
            {"kind": "a", "group": "g", "percent": 1.5, "severity": "warning", "is_active": true,
             "resets_at": "2026-11-21T10:00:00Z"},
            7, "x", null,
            {"kind": "b", "group": "g"}
        ]}}))
        .expect("a report");
        let kinds: Vec<&str> = rows(&report).iter().map(|row| row.kind.as_str()).collect();
        assert_eq!(kinds, ["a", "b"]);
        assert_eq!(rows(&report)[0].percent, Some(1.5));
        assert_eq!(rows(&report)[0].resets_at.as_deref(), Some("2026-11-21T10:00:00Z"));
        assert!(rows(&report)[0].is_active);
        let only_junk = UsageReport::parse(&json!({"rate_limits": {"limits": [7, "x"]}})).expect("a report");
        assert_eq!(only_junk.limits, PlanLimits::Rows(vec![]));

        // A wrong type inside a row costs that field only, and never reads as 0.
        let report = UsageReport::parse(&json!({"rate_limits": {"limits": [{
            "kind": 3, "group": null, "percent": "50", "resets_at": 17, "severity": [],
            "is_active": "yes", "scope": 4
        }]}}))
        .expect("a report");
        assert_eq!(
            rows(&report)[0],
            LimitRow {
                kind: String::new(),
                group: String::new(),
                percent: None,
                resets_at: None,
                scope_name: None,
                severity: String::new(),
                is_active: false,
            }
        );

        // The scope name is the model's, else the surface's.
        let report = UsageReport::parse(&json!({"rate_limits": {"limits": [
            {"scope": {"model": {"display_name": "M"}, "surface": {"display_name": "S"}}},
            {"scope": {"model": {"display_name": 1}, "surface": {"display_name": "S"}}},
            {"scope": {"surface": {"display_name": "S"}}},
            {"scope": {"model": {}}}
        ]}}))
        .expect("a report");
        let names: Vec<Option<&str>> = rows(&report).iter().map(|row| row.scope_name.as_deref()).collect();
        assert_eq!(names, [Some("M"), Some("S"), Some("S"), None]);

        // An empty name is no name: the fallback still applies.
        let report = UsageReport::parse(&json!({"rate_limits": {"limits": [
            {"scope": {"model": {"display_name": ""}, "surface": {"display_name": "Web"}}},
            {"scope": {"model": {"display_name": ""}, "surface": {"display_name": ""}}}
        ]}}))
        .expect("a report");
        let names: Vec<Option<&str>> = rows(&report).iter().map(|row| row.scope_name.as_deref()).collect();
        assert_eq!(names, [Some("Web"), None]);

        // A surface that is not an object, or has no string name, gives no name.
        let report = UsageReport::parse(&json!({"rate_limits": {"limits": [
            {"kind": "k1", "percent": 10, "scope": {"surface": 4}},
            {"kind": "k2", "percent": 20, "scope": {"surface": {"display_name": 4}}}
        ]}}))
        .expect("a report");
        let names: Vec<Option<&str>> = rows(&report).iter().map(|row| row.scope_name.as_deref()).collect();
        assert_eq!(names, [None, None]);
        // The siblings of the unreadable scope still read.
        let siblings: Vec<(&str, Option<f64>)> =
            rows(&report).iter().map(|row| (row.kind.as_str(), row.percent)).collect();
        assert_eq!(siblings, [("k1", Some(10.0)), ("k2", Some(20.0))]);

        // `session` that is not an object leaves no totals.
        let report = UsageReport::parse(&json!({"session": 5, "rate_limits": null})).expect("a report");
        assert_eq!(report.totals, None);

        // A wrong type inside `session` costs that field only; models keep
        // the object's order, a non-object entry is skipped, a bad count is None.
        let report = UsageReport::parse(&json!({"session": {
            "total_cost_usd": "1", "total_api_duration_ms": -5, "total_duration_ms": 1.5,
            "total_lines_added": 7, "total_lines_removed": null,
            "model_usage": {
                "zeta": {"inputTokens": 10, "outputTokens": "5", "cacheReadInputTokens": 3},
                "junk": 4,
                "alpha": {"cacheCreationInputTokens": 2}
            }
        }}))
        .expect("a report");
        let totals = report.totals.expect("totals");
        assert_eq!(totals.cost_usd, None);
        assert_eq!(totals.api_ms, None);
        assert_eq!(totals.wall_ms, None);
        assert_eq!(totals.lines_added, Some(7));
        assert_eq!(totals.lines_removed, None);
        assert_eq!(
            totals.models,
            vec![
                (
                    "zeta".to_string(),
                    ModelTokens { input: Some(10), output: None, cache_read: Some(3), cache_write: None }
                ),
                (
                    "alpha".to_string(),
                    ModelTokens { input: None, output: None, cache_read: None, cache_write: Some(2) }
                ),
            ]
        );
        // Each wrongly typed count is `None` while its siblings still read.
        let report = UsageReport::parse(&json!({"session": {
            "total_lines_added": "7", "total_lines_removed": 3,
            "model_usage": {
                "a": {"inputTokens": "1", "outputTokens": 2, "cacheReadInputTokens": 3,
                      "cacheCreationInputTokens": 4},
                "b": {"inputTokens": 1, "outputTokens": 2, "cacheReadInputTokens": "3",
                      "cacheCreationInputTokens": 4},
                "c": {"inputTokens": 1, "outputTokens": 2, "cacheReadInputTokens": 3,
                      "cacheCreationInputTokens": "4"}
            }
        }}))
        .expect("a report");
        let totals = report.totals.expect("totals");
        assert_eq!((totals.lines_added, totals.lines_removed), (None, Some(3)));
        let counts: Vec<[Option<u64>; 4]> = totals
            .models
            .iter()
            .map(|(_, t)| [t.input, t.output, t.cache_read, t.cache_write])
            .collect();
        assert_eq!(
            counts,
            [
                [None, Some(2), Some(3), Some(4)],
                [Some(1), Some(2), None, Some(4)],
                [Some(1), Some(2), Some(3), None],
            ]
        );
        let report = UsageReport::parse(&json!({"session": {"model_usage": [1]}})).expect("a report");
        assert_eq!(report.totals.expect("totals").models, vec![]);

        // `extra_usage`: not an object is none; wrong types cost their field.
        let report = UsageReport::parse(&json!({"rate_limits": {"extra_usage": 3}})).expect("a report");
        assert_eq!(report.extra, None);
        let report = UsageReport::parse(&json!({"rate_limits": {"extra_usage": {
            "is_enabled": "true", "monthly_limit": "50", "used_credits": 1234, "currency": 9
        }}}))
        .expect("a report");
        assert_eq!(
            report.extra,
            Some(ExtraUsage { enabled: false, monthly_limit: None, used: Some(1234.0), currency: None })
        );
        let report = UsageReport::parse(&json!({"rate_limits": {"extra_usage": {
            "is_enabled": true, "monthly_limit": 5000, "used_credits": "12", "currency": "USD"
        }}}))
        .expect("a report");
        assert_eq!(
            report.extra,
            Some(ExtraUsage {
                enabled: true,
                monthly_limit: Some(5000.0),
                used: None,
                currency: Some("USD".to_string())
            })
        );
    }
}

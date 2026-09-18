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

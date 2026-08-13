use serde_json::Value;

use crate::model::{ProviderUsage, UsageWindow};

/// Parses the one field Ollama Cloud currently exposes for this best-effort
/// indicator. The result is intentionally marked unverified by its label.
pub fn parse_ollama_cloud_usage(body: &str) -> Option<ProviderUsage> {
    let json: Value = serde_json::from_str(body).ok()?;
    let percent = json
        .get("usage_percent")
        .or_else(|| json.get("usagePercent"))
        .and_then(Value::as_f64)?;
    let window = UsageWindow::from_percent("session (unverified)", percent);
    Some(ProviderUsage {
        session: Some(window),
        weekly: None,
        monthly: None,
        fable_weekly: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_expected_usage_percent_field() {
        let usage = parse_ollama_cloud_usage(r#"{"usage_percent": 37.5}"#).unwrap();
        assert_eq!(usage.session.unwrap().used_percent, 37);
        assert_eq!(parse_ollama_cloud_usage(r#"{"usage": 37}"#), None);
        assert_eq!(parse_ollama_cloud_usage(r#"{"usage_percent": "37"}"#), None);
        assert_eq!(parse_ollama_cloud_usage("not json"), None);
    }
}

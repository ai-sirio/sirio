//! The Ollama Cloud provider: a locally stored session cookie and one HTTPS
//! page on ollama.com, ported from `App/OllamaCloudUsageFetcher.swift` and
//! `TillerCore/OllamaCloudUsageParser.swift` (F-SET-13).
//!
//! Like the Swift original, this is **best-effort**: the real page
//! structure was unverified when the parser was written (no public API, no
//! logged-in account to observe), so any unrecognized body is the expected
//! degradation path — `Unavailable(Error)`, never a fabricated number.
//!
//! The cookie lives where OpenCode Go's does: the `com.tiller.usage`
//! Keychain on macOS, the app's own [`crate::CredentialStore`] here,
//! written by the settings surface. One deliberate Swift asymmetry is
//! preserved: OpenCode Go normalizes a bare token to `auth=<token>`, but
//! the Swift Ollama fetcher sends the stored string **raw** as the
//! `Cookie` header — so this port does too.

use std::time::Duration;

use serde_json::Value;

use crate::http::{HttpError, get};
use crate::model::{ProviderUsage, UsageFetchOutcome, UsageReason, UsageWindow};

/// How long a fetch may take before giving up (the Swift app's bound).
pub const TIMEOUT: Duration = Duration::from_secs(12);

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

/// Extracts usage from the settings *page body* the fetch path actually
/// receives — the Swift `OllamaCloudUsageParser.extractUsage` semantics:
/// the first `usagePercent` followed by optional whitespace, `:`, optional
/// whitespace and digits, anywhere in the text. Note this deliberately does
/// **not** match quoted JSON keys (`"usagePercent": 37` puts a quote where
/// the Swift regex demands whitespace-or-colon) — for the JSON shape, see
/// [`parse_ollama_cloud_usage`], which predates this fetcher and remains
/// the best-effort JSON reading (F-CORE-USG-03).
pub fn extract_ollama_cloud_usage(body: &str) -> Option<ProviderUsage> {
    let percent = integer_after_key(body, "usagePercent")?;
    // The Swift `UsageWindow(label:usedPercent:)` and this crate's
    // `UsageWindow::new` both clamp to 100.
    let session = UsageWindow::new("usage", percent.min(100) as u8);
    Some(ProviderUsage {
        session: Some(session),
        weekly: None,
        monthly: None,
        fable_weekly: None,
    })
}

/// The first `key` `\s*` `:` `\s*` `(\d+)` occurrence, Swift-regex parity.
fn integer_after_key(body: &str, key: &str) -> Option<u64> {
    let bytes = body.as_bytes();
    let key_bytes = key.as_bytes();
    let mut i = 0;
    while i + key_bytes.len() <= bytes.len() {
        if &bytes[i..i + key_bytes.len()] == key_bytes {
            let mut j = i + key_bytes.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b':') {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > start
                    && let Ok(value) = body[start..j].parse()
                {
                    return Some(value);
                }
            }
        }
        i += 1;
    }
    None
}

/// The Ollama Cloud session cookie from this platform's local store —
/// **presence, not validity**. macOS reads the Keychain item the Swift app
/// writes; other platforms read the app's own credential file
/// ([`crate::CredentialStore`]), the store the settings surface saves into
/// (F-SET-13).
pub(crate) fn ollama_cloud_local_cookie() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        crate::opencode_go::keychain_cookie(
            crate::opencode_go::KEYCHAIN_SERVICE,
            OllamaCloudUsageFetcher::COOKIE_KEY,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::CredentialStore::from_env()
            .ok()?
            .get(OllamaCloudUsageFetcher::COOKIE_KEY)
            .map(|cookie| cookie.trim().to_string())
            .filter(|cookie| !cookie.is_empty())
    }
}

/// The Ollama Cloud usage fetcher. **Blocking** — bounded by [`TIMEOUT`];
/// call it off the render thread.
pub struct OllamaCloudUsageFetcher;

impl OllamaCloudUsageFetcher {
    /// The credential key for the session cookie — the Swift
    /// `OllamaCloudUsageFetcher.cookieKey`, shared by the Keychain item and
    /// the on-disk store so the two platforms name the credential
    /// identically.
    pub const COOKIE_KEY: &'static str = "ollama-cloud-cookie";

    /// Fetches usage from `https://ollama.com/settings`. A missing cookie →
    /// logged out; a page that refuses or cannot be parsed → error; a fetch
    /// that outlives its budget → `TimedOut`.
    pub fn fetch() -> UsageFetchOutcome {
        let Some(cookie) = ollama_cloud_local_cookie() else {
            return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut);
        };
        let page = match get(
            "https://ollama.com/settings",
            &[("Cookie", &cookie)],
            TIMEOUT.as_secs(),
        ) {
            Ok(response) if (200..300).contains(&response.code) => response.body,
            Ok(_) => return UsageFetchOutcome::Unavailable(UsageReason::Error),
            Err(HttpError::TimedOut) => return UsageFetchOutcome::TimedOut,
            Err(HttpError::Network(error)) => {
                eprintln!("[ollama-cloud-usage] settings request failed: {error}");
                return UsageFetchOutcome::Unavailable(UsageReason::Error);
            }
        };
        match extract_ollama_cloud_usage(&page) {
            Some(usage) => UsageFetchOutcome::Success(usage),
            None => UsageFetchOutcome::Unavailable(UsageReason::Error),
        }
    }
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

    /// F-SET-13: the fetch-path extractor carries the Swift regex's exact
    /// semantics — unquoted `usagePercent : NN` anywhere in the page text,
    /// first match wins, and quoted JSON keys do *not* match (the quote
    /// sits where the Swift pattern demands whitespace-then-colon).
    #[test]
    fn extracts_the_first_unquoted_usage_percent_from_page_text() {
        let page = "<script>window.state={usagePercent: 42, other: 1};\
                    usagePercent:99</script>";
        let usage = extract_ollama_cloud_usage(page).expect("parses");
        let session = usage.session.expect("session window");
        assert_eq!(session.used_percent, 42, "the first match wins");
        assert_eq!(session.label, "usage", "the Swift parser's label");
        assert_eq!(usage.weekly, None, "the page exposes one window only");

        assert_eq!(
            extract_ollama_cloud_usage(r#"{"usagePercent": 37}"#),
            None,
            "a quoted JSON key does not match — Swift regex parity"
        );
        assert_eq!(extract_ollama_cloud_usage("<html>login</html>"), None);
        assert_eq!(extract_ollama_cloud_usage("usagePercent = 12"), None);
    }

    #[test]
    fn page_percents_above_100_are_clamped() {
        let usage = extract_ollama_cloud_usage("usagePercent: 400").expect("parses");
        assert_eq!(usage.session.unwrap().used_percent, 100);
    }
}

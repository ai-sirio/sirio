//! P99 exercise of `F-CORE-AUTH-01`: account identity parsing reports
//! Claude logged-in/email/organization fields from JSON and uses the first
//! nonempty Codex credential line as identity — including the empty-field
//! inputs the VERIFY singles out.

use sirio_usage::{AgentAccountIdentity, parse_codex_identity};

fn identity(raw: &str) -> Option<AgentAccountIdentity> {
    AgentAccountIdentity::parse_claude_json(raw)
}

#[test]
fn claude_identity_parses_representative_json_shapes() {
    // The account wrapper with the camel-case organization key.
    assert_eq!(
        identity(r#"{"account":{"email":"dev@example.com","organizationName":"Acme Corp"}}"#),
        Some(AgentAccountIdentity {
            logged_in: true,
            email: "dev@example.com".into(),
            organization: Some("Acme Corp".into()),
        })
    );
    // Top-level fields with the alternate email/organization spellings.
    assert_eq!(
        identity(r#"{"email_address":"top@example.com","organization_name":"Beta"}"#),
        Some(AgentAccountIdentity {
            logged_in: true,
            email: "top@example.com".into(),
            organization: Some("Beta".into()),
        })
    );
    // The plain `organization` key, and whitespace-padded email trimmed.
    assert_eq!(
        identity(r#"{"account":{"email":"  padded@example.com  ","organization":"Gamma"}}"#),
        Some(AgentAccountIdentity {
            logged_in: true,
            email: "padded@example.com".into(),
            organization: Some("Gamma".into()),
        })
    );
}

#[test]
fn empty_fields_never_invent_identity() {
    // An empty email is no identity at all — not a logged-in claim.
    assert_eq!(
        identity(r#"{"account":{"email":"","organizationName":"Acme"}}"#),
        None
    );
    // Whitespace is as empty as empty.
    assert_eq!(identity(r#"{"account":{"email":"   "}}"#), None);
    // An empty organization is an email-only identity, not an empty string.
    assert_eq!(
        identity(r#"{"account":{"email":"dev@example.com","organizationName":""}}"#),
        Some(AgentAccountIdentity {
            logged_in: true,
            email: "dev@example.com".into(),
            organization: None,
        })
    );
    // An empty organization key falls through to a nonempty alternate.
    assert_eq!(
        identity(
            r#"{"account":{"email":"dev@example.com","organization":"","organization_name":"Real Org"}}"#
        ),
        Some(AgentAccountIdentity {
            logged_in: true,
            email: "dev@example.com".into(),
            organization: Some("Real Org".into()),
        })
    );
    // Malformed input is no claim in either direction.
    assert_eq!(identity("{ not json"), None);
    assert_eq!(identity(""), None);
    assert_eq!(identity("{}"), None);
}

#[test]
fn codex_identity_is_the_first_nonempty_line() {
    assert_eq!(
        parse_codex_identity("Logged in using ChatGPT - u@example.com\nplan: plus\n"),
        Some("Logged in using ChatGPT - u@example.com".into())
    );
    // Blank and whitespace-only lines are skipped, and the surviving line
    // is trimmed.
    assert_eq!(
        parse_codex_identity("\n\n   \n  second line is the identity  \nlater"),
        Some("second line is the identity".into())
    );
    // All-empty input yields no identity.
    assert_eq!(parse_codex_identity(""), None);
    assert_eq!(parse_codex_identity("   \n\t\n"), None);
}

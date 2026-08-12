//! Shell quoting helpers shared by the adapters, ported from
//! `TillerAgents/ShellQuote.swift` and `TillerCore/ShellQuote.swift`.
//!
//! Both helpers exist because generated commands are executed by a shell
//! (single-quote quoting) and parsed as config by the agent CLIs (JSON /
//! TOML string literals) — two different escaping worlds that must not be
//! conflated.

/// Single-quotes a string for shell consumption, escaping embedded single
/// quotes as `'\''`. Ported byte-for-byte from `TillerCore.shellQuote`.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Builds a JSON string literal — also a valid TOML basic string literal —
/// for embedding arbitrary text in generated code and config.
///
/// THE TRAP: `\/` is not a valid TOML escape sequence. A JSON encoder that
/// applies slash-escaping by default (Foundation's `JSONEncoder` does; so do
/// several JSON crates) would produce `\/` for every slash in a path, and a
/// TOML parser — e.g. Codex's `-c key=value` override, which is parsed as
/// TOML — rejects the whole value. The adapter fails silently at config
/// load, before it ever reaches its UI. The literal is therefore built with
/// slashes left unescaped: `serde_json` emits valid JSON and only escapes
/// what JSON requires (quotes, backslashes, control characters), which is
/// exactly the `.withoutEscapingSlashes` behavior of the Swift original.
pub fn json_string_literal(value: &str) -> String {
    // Serializing a `&str` is infallible (the only error case is
    // non-finite floats, which cannot occur here), mirroring the Swift
    // original's `try!`.
    serde_json::to_string(value).expect("serializing a string literal cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_in_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        // O'Brien → 'O'\''Brien' — the classic shell quoting idiom.
        assert_eq!(shell_quote("O'Brien"), "'O'\\''Brien'");
        assert_eq!(shell_quote("a'b'c"), "'a'\\''b'\\''c'");
    }

    #[test]
    fn shell_quote_leaves_spaces_and_double_quotes_alone() {
        assert_eq!(
            shell_quote("/Users/John Smith/bin/tillerctl"),
            "'/Users/John Smith/bin/tillerctl'"
        );
        assert_eq!(shell_quote("say \"hi\""), "'say \"hi\"'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn json_literal_never_escapes_slashes() {
        // The Codex trap: `\/` is not a TOML escape. A path with slashes must
        // survive the round trip byte-for-byte.
        let path = "/usr/local/bin/tillerctl";
        let literal = json_string_literal(path);

        assert!(
            !literal.contains("\\/"),
            "slash escaping would break TOML: {literal}"
        );
        assert!(literal.contains("/usr/local/bin/tillerctl"));
        assert_eq!(
            serde_json::from_str::<String>(&literal).expect("decodes as JSON"),
            path
        );
    }

    #[test]
    fn json_literal_escapes_quotes_backslashes_and_controls() {
        assert_eq!(
            json_string_literal("say \"hi\" \\ ok"),
            "\"say \\\"hi\\\" \\\\ ok\""
        );
        assert_eq!(json_string_literal("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(
            serde_json::from_str::<String>(&json_string_literal("a\"b\\c\nd")).expect("decodes"),
            "a\"b\\c\nd"
        );
    }

    #[test]
    fn json_literal_is_a_valid_toml_basic_string() {
        // The whole point: the literal must be consumable by a TOML parser.
        // A minimal check — no control characters outside TOML's escapes, no
        // `\/`, quotes/backslashes escaped — is what makes the Codex
        // `-c notify=[...]` override load.
        let literal = json_string_literal("/Users/me/tiller");
        assert!(!literal.contains("\\/"));
        assert!(literal.starts_with('"') && literal.ends_with('"'));
    }
}

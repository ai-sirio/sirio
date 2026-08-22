//! Shell quoting helpers shared by the adapters, ported from
//! `TillerAgents/ShellQuote.swift` and `TillerCore/ShellQuote.swift`.
//!
//! Both helpers exist because generated commands are executed by a shell
//! (single-quote quoting) and parsed as config by the agent CLIs (JSON /
//! TOML string literals) — two different escaping worlds that must not be
//! conflated.

/// Quotes a value for the shell that will actually run the generated command.
///
/// The adapters compose command lines that later run through
/// `tiller_terminal::command_shell_invocation`: `$SHELL -lc` on POSIX
/// systems, `cmd.exe /C` on Windows. The two shells speak different quoting
/// languages, so which one the returned string must speak is a property of
/// the platform, not of the value — a single-quoted token means nothing to
/// cmd.exe and arrives at the child program letter-for-letter, splitting a
/// path with spaces into several arguments.
///
/// POSIX form (ported byte-for-byte from `TillerCore.shellQuote`):
/// single-quotes the string, escaping embedded single quotes as `'\''`.
#[cfg(not(windows))]
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Windows form: cmd.exe has no single-quote grouping, so the value rides
/// one double-quoted token with each embedded `"` written doubled.
///
/// Two parsers see this string in sequence, and the form has to survive
/// both. **cmd.exe first**: it scans the command tail for its own
/// metacharacters (`&`, `|`, `>`, `^`) and treats a double-quoted region as
/// off limits for them, then hands the line to `CreateProcess` — it does
/// *not* collapse the doubled quotes itself. **The child's argv parser
/// second** (`CommandLineToArgvW` and the CRT rules every Rust and MSVC
/// program inherits): it splits the line into arguments and, inside a
/// quoted token, collapses each `""` back to one literal `"`.
///
/// WHY not `\"`, even though the CRT accepts it as an escaped quote: cmd
/// runs first and knows nothing about backslash escapes. It reads that `"`
/// as *closing* the quoted region, so everything after it — spaces, `&`, a
/// redirection character — is parsed unquoted, and a value carrying JSON
/// falls apart before the child is even spawned. The doubling form is the
/// one both parsers agree on, which is what makes it safe for values that
/// smuggle JSON with embedded quotes, like Codex's `-c notify=[...]`
/// override.
///
/// The one thing this cannot protect: cmd expands `%VAR%` inside double
/// quotes too, so a value containing `%` is not delivered byte-for-byte.
/// No caller passes one today — these are paths and prompts — and escaping
/// it would need a `^` dance that only applies outside quotes.
#[cfg(windows)]
pub fn shell_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
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

    #[cfg(not(windows))]
    #[test]
    fn shell_quote_wraps_in_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        // O'Brien → 'O'\''Brien' — the classic shell quoting idiom.
        assert_eq!(shell_quote("O'Brien"), "'O'\\''Brien'");
        assert_eq!(shell_quote("a'b'c"), "'a'\\''b'\\''c'");
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_quote_leaves_spaces_and_double_quotes_alone() {
        assert_eq!(
            shell_quote("/Users/John Smith/bin/tillerctl"),
            "'/Users/John Smith/bin/tillerctl'"
        );
        assert_eq!(shell_quote("say \"hi\""), "'say \"hi\"'");
        assert_eq!(shell_quote(""), "''");
    }

    #[cfg(windows)]
    #[test]
    fn shell_quote_wraps_in_double_quotes_for_cmd() {
        // Plain values gain an outer pair only; a value with spaces stays
        // one token because cmd groups inside "…".
        assert_eq!(shell_quote("plain"), "\"plain\"");
        assert_eq!(shell_quote("sess ref"), "\"sess ref\"");
        assert_eq!(shell_quote(""), "\"\"");
    }

    #[cfg(windows)]
    #[test]
    fn shell_quote_doubles_embedded_quotes_for_cmd() {
        // cmd.exe cannot escape a quote — its own convention is "" inside a
        // quoted token, collapsed back to one " before the child parses.
        assert_eq!(shell_quote("say \"hi\""), "\"say \"\"hi\"\"\"");
        // A JSON array of quoted strings with spaces: the exact shape of
        // Codex's `-c notify=[...]` override once its literals are built.
        assert_eq!(
            shell_quote("notify=[\"C:\\Program Files\\tiller\\tillerctl.exe\",\"notify\"]"),
            "\"notify=[\"\"C:\\Program Files\\tiller\\tillerctl.exe\"\",\"\"notify\"\"]\""
        );
    }

    /// The inverse of the cmd form for round-trip assertions: strips the
    /// outer quotes and collapses each `""` back to `"` — what the child's
    /// argv parser (`CommandLineToArgvW`/the CRT rules) does once cmd.exe
    /// has passed the line through.
    #[cfg(windows)]
    fn cmd_unquote(quoted: &str) -> String {
        assert!(
            quoted.starts_with('"') && quoted.ends_with('"'),
            "double-quoted: {quoted}"
        );
        quoted[1..quoted.len() - 1].replace("\"\"", "\"")
    }

    #[cfg(windows)]
    #[test]
    fn shell_quote_round_trips_through_cmd_unquote() {
        for value in [
            "plain",
            "sess ref",
            "",
            "say \"hi\"",
            "notify=[\"C:\\Program Files\\tiller\\tillerctl.exe\",\"notify\"]",
        ] {
            assert_eq!(
                cmd_unquote(&shell_quote(value)),
                value,
                "cmd must deliver the value byte-for-byte"
            );
        }
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

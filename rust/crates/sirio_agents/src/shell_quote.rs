//! Shell quoting helpers shared by the adapters, ported from
//! `SirioAgents/ShellQuote.swift` and `TillerCore/ShellQuote.swift`.
//!
//! Both helpers exist because generated commands are executed by a shell
//! (single-quote quoting) and parsed as config by the agent CLIs (JSON /
//! TOML string literals) — two different escaping worlds that must not be
//! conflated.

/// Quotes a value for the shell that will actually run the generated command.
///
/// The adapters compose command lines that later run through
/// `sirio_terminal::command_shell_invocation`: `$SHELL -lc` on POSIX
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
/// one double-quoted token with each embedded `"` written doubled AND each
/// run of backslashes immediately before a `"` written doubled.
///
/// Two parsers see this string in sequence, and the form has to survive
/// both — and it matters which one does what, because a previous version
/// of this comment had them swapped. **cmd.exe first**: it scans the
/// command tail for its own metacharacters (`&`, `|`, `>`, `%VAR%`, `^`),
/// treats a double-quoted region as off limits to most of them, and hands
/// the line to CreateProcess essentially verbatim — it does *not* collapse
/// doubled quotes or backslash runs itself. **The child's argv parser
/// second** (`CommandLineToArgvW` and the CRT rules every Rust and MSVC
/// program inherits): it splits the line into arguments and, inside a
/// quoted token, consumes each run of backslashes directly before a `"`
/// in pairs and collapses each `""` pair back to one literal `"`.
///
/// WHY not `\"`, even though the argv parser accepts it as an escaped
/// quote: cmd runs first and knows nothing about backslash escapes. It
/// reads that `"` as *closing* the quoted region, so everything after it
/// — spaces, `&`, a redirection character — is parsed unquoted, and a
/// value carrying JSON falls apart before the child is even spawned. The
/// doubling form is the one both parsers agree on, which is what makes it
/// safe for values that smuggle JSON with embedded quotes, like Codex's
/// `-c notify=[...]` override.
///
/// WHY the backslash doubling: the argv parser pairs up backslashes only
/// when they sit directly before a quote, so an odd run there loses its
/// last member — and a value that merely ENDS with `\` always qualifies,
/// because our closing quote follows it (`…sirio\` would arrive as
/// `…sirio"`). Doubling every run that touches any quote keeps the count
/// even everywhere it matters; backslashes elsewhere are literal and stay
/// untouched.
///
/// THE REMAINING HOLE, stated precisely because an earlier version of this
/// comment claimed there was none: cmd expands `%VAR%` between two `%`
/// signs even inside double quotes, and `%` has no escape on the command
/// line (`^` only takes effect outside quoted regions). `%` is legal in
/// NTFS names, so a worktree path like `C:\src\100%\repo` — riding into
/// omp's `--hook` argument or into the sirioctl path inside Codex's
/// notify JSON — would be silently rewritten wherever its `%..%` happens
/// to name an environment variable. This is documented rather than solved:
/// [`shell_quote`] returns a `String`, so rejecting here is not expressible
/// without rippling a `Result` through every adapter for a hole that only
/// bites Windows paths containing a `%NAME%`-shaped substring; callers
/// wanting a hard guarantee should reject such paths before they reach an
/// adapter.
#[cfg(windows)]
pub fn shell_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    let mut slashes = 0_usize;
    for ch in value.chars() {
        match ch {
            '\\' => slashes += 1,
            '"' => {
                // The argv parser eats this run in pairs before the quote;
                // doubling it makes the count even everywhere a quote can
                // follow. The quote itself rides as a "" pair — cmd cannot
                // be taught \" (see above).
                for _ in 0..slashes * 2 {
                    quoted.push('\\');
                }
                quoted.push_str("\"\"");
                slashes = 0;
            }
            _ => {
                for _ in 0..slashes {
                    quoted.push('\\');
                }
                slashes = 0;
                quoted.push(ch);
            }
        }
    }
    // A trailing run sits directly against OUR closing quote — same rule as
    // any embedded one.
    for _ in 0..slashes * 2 {
        quoted.push('\\');
    }
    quoted.push('"');
    quoted
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
            shell_quote("/Users/John Smith/bin/sirioctl"),
            "'/Users/John Smith/bin/sirioctl'"
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
            shell_quote("notify=[\"C:\\Program Files\\sirio\\sirioctl.exe\",\"notify\"]"),
            "\"notify=[\"\"C:\\Program Files\\sirio\\sirioctl.exe\"\",\"\"notify\"\"]\""
        );
    }

    #[cfg(windows)]
    #[test]
    fn shell_quote_doubles_backslash_runs_touching_a_quote() {
        // The argv parser consumes backslashes in pairs before a quote, so a
        // value merely ENDING with a backslash sits one backslash away from
        // our own closing quote and would lose it (…\" closes as …") —
        // the run must be doubled wherever it touches ANY quote.
        assert_eq!(
            shell_quote("C:\\Program Files\\sirio\\"),
            "\"C:\\Program Files\\sirio\\\\\""
        );
        // An embedded quote preceded by a backslash: same rule mid-value.
        assert_eq!(shell_quote("a\\\"b"), "\"a\\\\\"\"b\"");
        // Backslashes NOT touching a quote are literal and stay single.
        assert_eq!(shell_quote("C:\\dir\\deep"), "\"C:\\dir\\deep\"");
    }

    /// The inverse of the cmd form for round-trip assertions. It models what
    /// the child's argv parser (`CommandLineToArgvW`/the CRT rules) does to
    /// the FULL token, quotes included: each backslash run touching a `"` is
    /// halved (an odd member escapes the quote instead of closing the
    /// region), each `""` pair collapses to one literal `"`, and any other
    /// `"` opens or closes the quoted region. The previous version stripped
    /// the outer quotes first and collapsed only `""` — it modeled no
    /// backslash handling at all, which is exactly why this round-trip kept
    /// passing while the real behaviour lost trailing backslashes.
    #[cfg(windows)]
    fn cmd_unquote(quoted: &str) -> String {
        let chars: Vec<char> = quoted.chars().collect();
        let mut out = String::new();
        let mut in_quotes = false;
        let mut slashes = 0_usize;
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '\\' => slashes += 1,
                '"' => {
                    for _ in 0..slashes / 2 {
                        out.push('\\');
                    }
                    if slashes % 2 == 1 {
                        // An escaped quote; still inside the region.
                        out.push('"');
                    } else if in_quotes && chars.get(index + 1) == Some(&'"') {
                        // A "" pair collapses to one literal quote.
                        out.push('"');
                        index += 1;
                    } else {
                        in_quotes = !in_quotes;
                    }
                    slashes = 0;
                }
                ch => {
                    for _ in 0..slashes {
                        out.push('\\');
                    }
                    slashes = 0;
                    out.push(ch);
                }
            }
            index += 1;
        }
        for _ in 0..slashes {
            out.push('\\');
        }
        out
    }

    #[cfg(windows)]
    #[test]
    fn shell_quote_round_trips_through_cmd_unquote() {
        for value in [
            "plain",
            "sess ref",
            "",
            "say \"hi\"",
            "notify=[\"C:\\Program Files\\sirio\\sirioctl.exe\",\"notify\"]",
            // Trailing backslash, backslash-before-quote, doubled backslash:
            // the cases the old cmd_unquote could not even model.
            "C:\\Program Files\\sirio\\",
            "a\\\"b",
            "C:\\dir\\deep",
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
        let path = "/usr/local/bin/sirioctl";
        let literal = json_string_literal(path);

        assert!(
            !literal.contains("\\/"),
            "slash escaping would break TOML: {literal}"
        );
        assert!(literal.contains("/usr/local/bin/sirioctl"));
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
        let literal = json_string_literal("/Users/me/sirio");
        assert!(!literal.contains("\\/"));
        assert!(literal.starts_with('"') && literal.ends_with('"'));
    }
}

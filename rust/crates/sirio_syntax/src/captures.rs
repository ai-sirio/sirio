//! Rewriting a grammar's own capture names into the vocabulary bezel speaks.
//!
//! Every grammar crate ships a `highlights.scm`, and no two ship quite the
//! same dialect: `tree-sitter-lua` writes `@field` and `@repeat`,
//! `tree-sitter-c` writes `@delimiter`, `tree-sitter-md` still writes the
//! pre-2023 `@text.title`. bezel's
//! [`kind_of`](syntax::lang::kind_of) knows the nvim-treesitter names and
//! answers `HighlightKind::Variable` — the plain text colour — to everything
//! else.
//!
//! That fallback is the failure mode worth naming: it is silent. A query
//! full of names bezel does not know still compiles, still parses, still
//! produces spans; they are simply all the colour of ordinary text, which
//! reads as "tree-sitter is not working" and is indistinguishable from it.
//! So the names are translated once, here, on the way in.
//!
//! One name is deleted rather than translated. `@spell` is not a colour:
//! nvim uses it to mark a region for its spellchecker, and Swift, SQL and
//! Zig all write it as a *second* capture on a node the same pattern has
//! just captured as `@comment`:
//!
//! ```scheme
//! [(comment) (multiline_comment)] @comment @spell
//! ```
//!
//! `tree-sitter-highlight` resolves a node captured more than once by taking
//! the last capture (`highlight.rs`: "keep iterating over any later
//! highlighting patterns that also match this node and set the match to
//! it"). So `@spell` wins, `kind_of` has no slot for it, and every comment
//! in a Swift or SQL file is painted the colour of ordinary text. Measured,
//! not reasoned: leaving it alone put `(0..6, Variable)` on Swift's `// doc`.
//!
//! Renaming it to a `_`-prefixed name — the class bezel drops before
//! configuring the query — is not enough either. The later capture still
//! replaces the earlier one; it just resolves to no highlight, so the
//! comment ends up with no span at all instead of the wrong one. Only
//! removing the capture from the query leaves `@comment` standing.
//!
//! `@none` is the opposite case and needs nothing done to it. It marks a
//! node that should take no colour — Kotlin's `${…}` inside a string,
//! Markdown's fenced code — and `kind_of`'s fallback already answers
//! `Variable`, the plain text colour. That *is* what `@none` asks for.

/// What each foreign capture name becomes. Keys are exact, whole names: a
/// query saying `@string.special.symbol` is rewritten, one saying `@string`
/// is left alone because bezel already knows it.
///
/// Every entry here was found by scanning the shipped queries of the
/// grammars in [`crate::GRAMMARS`] for names `kind_of` does not answer, so
/// the table is exactly as large as it needs to be and no larger.
const ALIASES: &[(&str, &str)] = &[
    // A character literal is a string of one; bezel keeps `character.special`
    // for escapes and has no separate slot for the ordinary case.
    ("character", "string"),
    ("conditional", "keyword.conditional"),
    ("constant.macro", "macro"),
    ("delimiter", "punctuation.delimiter"),
    // Ruby's `#{`/`}` and XML's `<?`…`?>`: the marks around an embedded
    // region, not the region itself.
    ("embedded", "punctuation.special"),
    ("error", "invalid"),
    ("exception", "keyword.exception"),
    ("field", "variable.member"),
    ("float", "number"),
    ("function.method.builtin", "function.builtin"),
    // C's `@function.special` is the function-like macro case.
    ("function.special", "function.macro"),
    ("include", "keyword.import"),
    ("markup", "variable"),
    ("markup.heading", "keyword"),
    ("markup.link", "string.special.url"),
    ("markup.raw", "string"),
    ("method", "function.method"),
    ("method.call", "function.method.call"),
    ("namespace", "module"),
    ("number.float", "number"),
    ("preproc", "keyword.directive"),
    ("repeat", "keyword.repeat"),
    ("storageclass", "keyword.modifier"),
    ("string.regex", "string.regexp"),
    ("string.special.regex", "string.regexp"),
    ("string.special.symbol", "string.special"),
    ("tag.error", "invalid"),
    ("text.literal", "string"),
    ("text.reference", "constant"),
    ("text.title", "keyword"),
    ("text.uri", "string.special.url"),
    ("type.qualifier", "keyword.modifier"),
];

/// Capture names that are removed from the query outright, rather than
/// renamed. See the module docs: a name here is one that shadows a real
/// colour by being captured second on the same node.
const DROPPED: &[&str] = &["spell"];

/// `query` with every capture name [`ALIASES`] covers replaced, and every
/// name in [`DROPPED`] removed.
///
/// Scanning rather than a plain string replace, because a `.scm` is full of
/// `@` that is not a capture. `tree-sitter-css` matches CSS at-rules with
/// `"@media" @keyword`, and `tree-sitter-zig` with
/// `(#any-of? @keyword.import "@import" "@cImport")` — replacing inside those
/// string literals would change what the query *matches*, not how it is
/// painted, and `#any-of?` failing is silent in exactly the same way the
/// unknown-name fallback is.
pub(crate) fn rewrite(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut out = String::with_capacity(query.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // A comment runs to the end of the line.
            b';' => {
                let end = memchr_newline(bytes, index);
                out.push_str(&query[index..end]);
                index = end;
            }
            b'"' => {
                let end = string_end(bytes, index);
                out.push_str(&query[index..end]);
                index = end;
            }
            b'@' => {
                let end = name_end(bytes, index + 1);
                let name = &query[index + 1..end];
                if DROPPED.contains(&name) {
                    // Take the blank that separated it from the capture
                    // before it with it, so `@comment @spell` does not
                    // become `@comment ` with a dangling space.
                    while out.ends_with(' ') || out.ends_with('\t') {
                        out.pop();
                    }
                } else {
                    out.push('@');
                    out.push_str(alias_for(name).unwrap_or(name));
                }
                index = end;
            }
            _ => {
                // Push the byte through as a `char` boundary-safe slice:
                // every byte matched above is ASCII, so anything else is
                // either ASCII or inside a multi-byte sequence we copy whole.
                let end = next_interesting(bytes, index);
                out.push_str(&query[index..end]);
                index = end;
            }
        }
    }
    out
}

fn alias_for(name: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(from, _)| *from == name)
        .map(|(_, to)| *to)
}

fn memchr_newline(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

/// The index just past the closing quote, or the end of the text for an
/// unterminated literal — which is a query that will not compile anyway, and
/// is not this function's problem to diagnose.
fn string_end(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn name_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len()
        && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'.' || bytes[index] == b'_')
    {
        index += 1;
    }
    index
}

fn next_interesting(bytes: &[u8], from: usize) -> usize {
    let mut index = from + 1;
    while index < bytes.len() && !matches!(bytes[index], b';' | b'"' | b'@') {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_foreign_name_becomes_the_one_bezel_knows() {
        assert_eq!(rewrite("(field) @field"), "(field) @variable.member");
    }

    #[test]
    fn a_name_bezel_already_knows_is_left_alone() {
        let query = "(comment) @comment\n(string_literal) @string";
        assert_eq!(rewrite(query), query);
    }

    #[test]
    fn a_longer_name_is_not_rewritten_by_a_shorter_key() {
        // `field` is a key; `variable.member` must not become
        // `variable.variable.member`, and `field_expression` — a node name,
        // not a capture — must not be touched at all.
        let query = "(field_expression) @variable.member";
        assert_eq!(rewrite(query), query);
    }

    #[test]
    fn an_at_sign_inside_a_string_literal_survives() {
        // tree-sitter-css matches at-rules by their literal text. Rewriting
        // this would stop the pattern matching anything — and a query that
        // matches nothing looks exactly like a grammar that is not loaded.
        let query = r#""@import" @keyword.import"#;
        assert_eq!(rewrite(query), query);
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string_early() {
        let query = r#"(#eq? @x "a\"@field b") (y) @field"#;
        assert_eq!(
            rewrite(query),
            r#"(#eq? @x "a\"@field b") (y) @variable.member"#
        );
    }

    #[test]
    fn an_at_sign_inside_a_comment_survives() {
        let query = "; see @field above\n(x) @field";
        assert_eq!(rewrite(query), "; see @field above\n(x) @variable.member");
    }

    #[test]
    fn spell_is_removed_so_the_capture_beside_it_survives() {
        // The last capture on a node wins. `@spell` has no colour, so it
        // must not be the last one — and renaming it is not enough, because
        // it would still displace `@comment`. It has to go.
        assert_eq!(rewrite("(comment) @comment @spell"), "(comment) @comment");
        assert_eq!(rewrite("] @comment @spell\n"), "] @comment\n");
    }

    #[test]
    fn none_is_left_alone_because_the_fallback_already_means_what_it_says() {
        // `@none` asks for no colour; `kind_of` answers `Variable`, the
        // plain text colour, to every name it does not know. Rewriting it
        // would be work that changes nothing.
        assert_eq!(
            rewrite("(code_fence_content) @none"),
            "(code_fence_content) @none"
        );
    }

    #[test]
    fn rewriting_twice_changes_nothing_the_second_time() {
        // No alias target is itself a key, so the table cannot chain.
        let query = "(a) @field (b) @repeat (c) @text.title (d) @none";
        let once = rewrite(query);
        assert_eq!(rewrite(&once), once);
    }

    #[test]
    fn no_alias_target_is_also_an_alias_key() {
        for (_, to) in ALIASES {
            assert!(
                !ALIASES.iter().any(|(from, _)| from == to),
                "`{to}` is both a target and a key, so rewriting would chain"
            );
        }
    }

    #[test]
    fn multibyte_text_passes_through_unharmed() {
        // Queries carry comments in whatever language their author wrote in.
        let query = "; naïve — see @field\n(x) @field";
        assert_eq!(rewrite(query), "; naïve — see @field\n(x) @variable.member");
    }
}

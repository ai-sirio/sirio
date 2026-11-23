//! A quoted message travels as a Markdown `> ` block at the head of the prompt text (spec §5.4).
//! `join_quote` writes the block and `split_quote` reads it back, so every reader of a user text
//! shares one rule.

/// A message held in the composer to be quoted: the author is only for the composer's card and is
/// never sent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Quote {
    pub(crate) author: String,
    pub(crate) text: String,
}

/// The prompt text for `typed` with `quote` at its head: every line of the quote prefixed with
/// `> ` (an empty line becomes `>`), an empty line, then `typed`.
///
/// Returns `typed` unchanged when there is no quote or it is blank. With nothing typed it returns
/// the block alone, with no trailing blank line.
// Task 5/6 (the composer and the user row) are its first callers.
#[allow(dead_code)]
pub(crate) fn join_quote(quote: Option<&str>, typed: &str) -> String {
    let Some(quote) = quote.filter(|quote| !quote.trim().is_empty()) else {
        return typed.to_string();
    };
    // `lines` reads CRLF as LF; only the newlines at the very end are dropped, so a trailing
    // newline does not add an empty quoted line.
    let block = quote
        .trim_end_matches(['\n', '\r'])
        .lines()
        .map(|line| if line.is_empty() { ">".to_string() } else { format!("> {line}") })
        .collect::<Vec<_>>()
        .join("\n");
    if typed.is_empty() { block } else { format!("{block}\n\n{typed}") }
}

/// The inverse of `join_quote`: when the leading lines of `text` all start with `>` and are
/// followed by an empty line or by the end of the text, the quoted text (one `> ` or `>` stripped
/// per line) and the text after the empty line. Anything else gives `(None, text)`.
// Task 5/6 (the composer and the user row) are its first callers.
#[allow(dead_code)]
pub(crate) fn split_quote(text: &str) -> (Option<String>, &str) {
    let mut quoted: Vec<&str> = Vec::new();
    let mut consumed = 0;
    let mut rest = None;
    for raw in text.split_inclusive('\n') {
        consumed += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            // The empty line ends the block.
            rest = Some(&text[consumed..]);
            break;
        }
        let Some(body) = line.strip_prefix('>') else {
            return (None, text);
        };
        quoted.push(body.strip_prefix(' ').unwrap_or(body));
    }
    let quote = quoted.join("\n");
    // No quoted line before the empty line, or only blank ones: `join_quote` writes no such block,
    // so it is plain text.
    if quote.trim().is_empty() {
        return (None, text);
    }
    (Some(quote), rest.unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = join_quote(Some("a\nb"), "why?");
        assert_eq!(text, "> a\n> b\n\nwhy?");
        assert_eq!(split_quote(&text), (Some("a\nb".to_string()), "why?"));
    }

    #[test]
    fn an_empty_quoted_line_is_a_bare_marker() {
        let text = join_quote(Some("a\n\nb"), "x");
        assert_eq!(text, "> a\n>\n> b\n\nx");
        assert_eq!(split_quote(&text), (Some("a\n\nb".to_string()), "x"));
    }

    #[test]
    fn crlf_is_quoted_as_lf() {
        assert_eq!(join_quote(Some("a\r\nb"), "x"), "> a\n> b\n\nx");
    }

    #[test]
    fn a_trailing_newline_adds_no_empty_quoted_line() {
        assert_eq!(join_quote(Some("a\n"), "x"), "> a\n\nx");
    }

    #[test]
    fn a_blank_only_quote_quotes_nothing() {
        assert_eq!(join_quote(Some("  \n"), "x"), "x");
        assert_eq!(join_quote(None, "x"), "x");
    }

    #[test]
    fn a_quote_alone_has_no_blank_tail() {
        let text = join_quote(Some("a"), "");
        assert_eq!(text, "> a");
        assert_eq!(split_quote(&text), (Some("a".to_string()), ""));
    }

    #[test]
    fn a_marker_without_space_is_read() {
        assert_eq!(split_quote(">a\n\nb"), (Some("a".to_string()), "b"));
    }

    #[test]
    fn a_block_not_followed_by_a_blank_line_is_not_a_quote() {
        assert_eq!(split_quote("> a\nb"), (None, "> a\nb"));
    }

    #[test]
    fn a_quote_after_text_is_not_a_quote() {
        assert_eq!(split_quote("x\n> a"), (None, "x\n> a"));
    }

    #[test]
    fn a_whole_text_of_quote_is_all_quote() {
        assert_eq!(split_quote("> a"), (Some("a".to_string()), ""));
    }

    #[test]
    fn a_blank_quote_block_is_not_a_quote() {
        for text in [">\n\nciao", ">\n>\n\nciao", ">"] {
            assert_eq!(split_quote(text), (None, text));
        }
    }

    #[test]
    fn crlf_is_read_as_lf() {
        assert_eq!(split_quote("> a\r\n\r\nb"), (Some("a".to_string()), "b"));
    }

    #[test]
    fn a_leading_empty_line_is_not_a_quote() {
        assert_eq!(split_quote("\n> a"), (None, "\n> a"));
    }

    #[test]
    fn typed_text_that_starts_with_a_quote_stays_typed() {
        let text = join_quote(Some("a"), "> b\n\nc");
        assert_eq!(split_quote(&text), (Some("a".to_string()), "> b\n\nc"));
    }
}

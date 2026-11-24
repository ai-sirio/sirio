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

/// The author a quote read back from a prompt's text is given: the text does not carry who wrote
/// it, because the author is never sent.
pub(crate) const READ_BACK_AUTHOR: &str = "Quote";

/// The most characters of a quote a card is given: more than any card is wide, so the card's own
/// ellipsis shows where the line is cut.
pub(crate) const CARD_LINE_MAX_CHARS: usize = 300;

/// The one line of text a quote's card shows: its lines, each trimmed, with the blank ones
/// skipped, joined by single spaces and cut at `CARD_LINE_MAX_CHARS` characters. Ely's card draws
/// the first line of what it is given, so a quote that opens with a blank line, or runs over
/// several, would show an empty card or one line of many.
pub(crate) fn card_line(quote: &str) -> String {
    let mut line = String::new();
    let mut chars = 0;
    // Chars are taken one by one up to the cap, so a quote of one huge line (a card is built
    // every frame) is not copied whole to be cut.
    'parts: for part in quote.lines().map(str::trim).filter(|part| !part.is_empty()) {
        if !line.is_empty() {
            if chars >= CARD_LINE_MAX_CHARS {
                break;
            }
            line.push(' ');
            chars += 1;
        }
        for ch in part.chars() {
            if chars >= CARD_LINE_MAX_CHARS {
                break 'parts;
            }
            line.push(ch);
            chars += 1;
        }
    }
    // The cap can fall just after a space (a separator, or one inside a line): the card's
    // ellipsis follows the text, not a blank.
    line.truncate(line.trim_end().len());
    line
}

/// Whether `text`, a prompt as typed, is a slash command: its first word is `/`, a letter, then
/// letters, digits, `-`, `_` or `:` (`/compact`, `/mcp:foo`). A path that starts with `/` is text.
/// The agent reads a command at the head of the prompt, so a quote is not put ahead of one.
pub(crate) fn is_slash_command(text: &str) -> bool {
    let word = text.split_whitespace().next().unwrap_or_default();
    let Some(name) = word.strip_prefix('/') else {
        return false;
    };
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
}

/// The line that names a user text in a list (the turn's fold row, the rail's tick): the first
/// line of what was typed after a leading quote, or the quote's first line with text in it when
/// nothing was typed; the first line of any other text. The block's `>` markers name nothing.
pub(crate) fn headline(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let (quoted, rest) = split_quote(text);
    match quoted {
        Some(quoted) if rest.trim().is_empty() => Cow::Owned(
            quoted
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or(&quoted)
                .to_string(),
        ),
        _ => Cow::Borrowed(rest.lines().next().unwrap_or(rest)),
    }
}

/// The prompt text for `typed` with `quote` at its head: every line of the quote prefixed with
/// `> ` (an empty line becomes `>`), an empty line, then `typed`.
///
/// Returns `typed` unchanged when there is no quote or it is blank. With nothing typed, or only
/// whitespace (what is left of a draft whose mention was lifted), it returns the block alone, with
/// no trailing blank line or whitespace.
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
    if typed.trim().is_empty() { block } else { format!("{block}\n\n{typed}") }
}

/// The inverse of `join_quote`: when the leading lines of `text` all start with `>` and are
/// followed by an empty line or by the end of the text, the quoted text (one `> ` or `>` stripped
/// per line) and the text after the empty line. Anything else gives `(None, text)`.
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

    // The card's line (`card_line`): a quote that opens with a blank line must not draw an empty
    // card, a long one must be cut where the card's own ellipsis shows it, and the cut must not
    // fall inside a character.
    #[test]
    fn a_card_line_skips_the_blank_lines_before_the_text() {
        assert_eq!(card_line("\n\n  abc"), "abc");
    }

    #[test]
    fn a_card_line_joins_the_lines_with_single_spaces() {
        assert_eq!(card_line("a\n\n  b  \r\nc\n"), "a b c");
    }

    #[test]
    fn a_card_line_of_nothing_is_empty() {
        assert_eq!(card_line(""), "");
        assert_eq!(card_line(" \n\t\n"), "");
    }

    #[test]
    fn a_card_line_is_cut_at_the_cap() {
        let long = "x".repeat(CARD_LINE_MAX_CHARS * 2);
        assert_eq!(card_line(&long).chars().count(), CARD_LINE_MAX_CHARS);
    }

    #[test]
    fn a_card_line_is_cut_on_a_character_boundary() {
        let long = "\u{e9}\u{1f600}".repeat(CARD_LINE_MAX_CHARS);
        let line = card_line(&long);
        assert_eq!(line.chars().count(), CARD_LINE_MAX_CHARS);
        assert!(long.starts_with(&line));
    }

    #[test]
    fn a_card_line_under_the_cap_is_whole() {
        let text = "y".repeat(CARD_LINE_MAX_CHARS);
        assert_eq!(card_line(&text), text);
    }

    #[test]
    fn a_card_line_does_not_end_in_the_separator_the_cap_cut_after() {
        // The separator is the 300th character: the next line has no room and the line must not
        // end in a space.
        let text = format!("{}\nb", "a".repeat(CARD_LINE_MAX_CHARS - 1));
        assert_eq!(card_line(&text), "a".repeat(CARD_LINE_MAX_CHARS - 1));
    }

    #[test]
    fn a_card_line_does_not_end_in_a_space_the_cap_cut_after() {
        let text = format!("{} b", "a".repeat(CARD_LINE_MAX_CHARS - 1));
        assert_eq!(card_line(&text), "a".repeat(CARD_LINE_MAX_CHARS - 1));
    }

    // Typed text that is only whitespace (the tail of a draft whose mention was lifted) adds
    // nothing after a quote: the prompt is the bare block.
    #[test]
    fn whitespace_typed_after_a_quote_sends_the_bare_block() {
        for typed in ["  \n", " ", "\n\n", "\t"] {
            assert_eq!(join_quote(Some("a"), typed), "> a", "typed {typed:?}");
        }
    }

    #[test]
    fn whitespace_typed_with_no_quote_stays() {
        assert_eq!(join_quote(None, "  \n"), "  \n");
        assert_eq!(join_quote(Some(" \n"), "  "), "  ");
    }

    // A command is the first word of the typed text: `/` then a letter, then letters, digits,
    // `-`, `_` or `:`. A path that starts with `/` is text.
    #[test]
    fn a_slash_word_is_a_command() {
        for text in ["/compact", "/mcp:foo", "/c-d_e:f9", "/compact now", "  /compact", "/a\nmore"] {
            assert!(is_slash_command(text), "{text:?}");
        }
    }

    #[test]
    fn a_path_or_a_bare_slash_is_not_a_command() {
        for text in ["/etc/hosts is wrong", "/", "/1abc", "//x", "/a/b", "/a.b", "", "x /compact", "/ compact"] {
            assert!(!is_slash_command(text), "{text:?}");
        }
    }

    // What names a user text in a list: what was typed after a quote, else the quote.
    #[test]
    fn a_headline_is_the_first_typed_line_after_a_quote() {
        assert_eq!(headline("> a\n> b\n\nwhy?\nmore"), "why?");
    }

    #[test]
    fn a_headline_of_a_quote_alone_is_its_first_text_line() {
        assert_eq!(headline("> \n> a\n> b"), "a");
        assert_eq!(headline("> a\n\n  \n"), "a");
    }

    #[test]
    fn a_headline_of_plain_text_is_its_first_line() {
        assert_eq!(headline("hello\nworld"), "hello");
        assert_eq!(headline(""), "");
        assert_eq!(headline("x\n> a"), "x");
    }
}

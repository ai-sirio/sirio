//! CommonMark-aware fence scanning for the `open` flag on code blocks.
//!
//! [`pulldown_cmark`] closes an unterminated fence at end of input, so the
//! event stream cannot tell an explicitly closed fence from one cut off by
//! the stream ending. This module answers that one question — "does the
//! source end inside an unclosed fence?" — with a lightweight line scan that
//! follows the CommonMark rules that matter for it: a code fence is 0-3
//! spaces of indent plus 3+ backticks or tildes, it does *not* interrupt a
//! paragraph, and a closing fence must match the opening character with at
//! least as many marks and nothing but whitespace after. Blockquote and list
//! prefixes are stripped before classification so fences nested inside them
//! are still found.

/// True when `source` ends inside an unclosed fenced code block.
pub(crate) fn ends_inside_fence(source: &str) -> bool {
    let mut fence: Option<(char, usize)> = None;
    let mut fence_in_quote = false;
    let mut in_quote = false;
    let mut in_paragraph = false;
    for raw_line in source.split('\n') {
        // Track whether the current line is inside a blockquote: a `>`
        // prefix puts it there, and it stays there across blank lines until
        // a non-blank, non-quoted line appears.
        if let Some(rest) = raw_line.strip_prefix('>') {
            let _ = rest;
            in_quote = true;
        } else if !raw_line.trim().is_empty() {
            in_quote = false;
        }

        let line = if fence.is_none() {
            // Outside a fence, strip blockquote prefixes (a quote may wrap
            // any block) and one list marker before classifying the line.
            let mut line = raw_line;
            while let Some(rest) = line.strip_prefix('>') {
                line = rest.strip_prefix(' ').unwrap_or(rest);
            }
            strip_list_marker(line)
        } else if fence_in_quote {
            // Inside a fence opened in a blockquote, every content line is
            // also quoted: strip the marker so a `> ``` ` line can close it.
            // (A top-level fence's `> ` lines are content, not markers.)
            raw_line
                .strip_prefix('>')
                .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
                .unwrap_or(raw_line)
        } else {
            raw_line
        };

        if let Some((mark, len)) = fence {
            if is_closing_fence(line, mark, len) {
                fence = None;
            }
        } else if line.trim().is_empty() {
            in_paragraph = false;
        } else if is_heading_or_thematic_break(line) {
            // Headings and thematic breaks interrupt paragraphs.
            in_paragraph = false;
        } else if !in_paragraph && let Some(opened) = opening_fence(line) {
            fence = Some(opened);
            fence_in_quote = in_quote;
        } else {
            in_paragraph = true;
        }
    }
    fence.is_some()
}

/// An opening fence: 0-3 spaces, then 3+ backticks or tildes. A backtick
/// fence's info string may not contain a backtick.
fn opening_fence(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let mark = rest.as_bytes().first().copied()? as char;
    if mark != '`' && mark != '~' {
        return None;
    }
    let count = rest
        .bytes()
        .take_while(|byte| *byte as char == mark)
        .count();
    if count < 3 {
        return None;
    }
    if mark == '`' && rest[count..].contains('`') {
        return None;
    }
    Some((mark, count))
}

/// A closing fence for the fence `(mark, len)`: 0-3 spaces, at least `len`
/// of `mark`, then only whitespace.
fn is_closing_fence(line: &str, mark: char, len: usize) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let count = rest
        .bytes()
        .take_while(|byte| *byte as char == mark)
        .count();
    count >= len
        && rest[count..]
            .bytes()
            .all(|byte| byte == b' ' || byte == b'\t')
}

/// An ATX heading (`### text`) or a thematic break (`---`, `***`, `___`).
/// Both interrupt a paragraph, which matters because a fence cannot.
fn is_heading_or_thematic_break(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let bytes = rest.as_bytes();

    let hashes = bytes.iter().take_while(|byte| **byte == b'#').count();
    if (1..=6).contains(&hashes) && (hashes == bytes.len() || bytes.get(hashes) == Some(&b' ')) {
        return true;
    }

    let marks: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|byte| *byte != b' ' && *byte != b'\t')
        .collect();
    marks.len() >= 3
        && marks.iter().all(|byte| *byte == marks[0])
        && matches!(marks[0], b'-' | b'_' | b'*')
}

/// Strips one list marker: `- `, `* `, `+ `, or `1. ` / `1) `.
fn strip_list_marker(line: &str) -> &str {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest;
        }
    }
    let bytes = line.as_bytes();
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if (1..=9).contains(&digits)
        && bytes
            .get(digits)
            .is_some_and(|byte| *byte == b'.' || *byte == b')')
        && bytes.get(digits + 1) == Some(&b' ')
    {
        return &line[digits + 2..];
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unterminated_fence_is_detected() {
        assert!(ends_inside_fence("```rust\nfn main() {}\n"));
        assert!(ends_inside_fence("```rust\nfn main() {}"));
        assert!(ends_inside_fence("```"));
        assert!(ends_inside_fence("~~~\ncode"));
        assert!(ends_inside_fence("```\n\n\n"));
    }

    #[test]
    fn a_closed_fence_is_not_open() {
        assert!(!ends_inside_fence("```rust\nfn main() {}\n```"));
        assert!(!ends_inside_fence("```\n```"));
        assert!(!ends_inside_fence("```rust\nfn main() {}\n```\n\nafter"));
        assert!(!ends_inside_fence("~~~\ncode\n~~~"));
    }

    #[test]
    fn fences_do_not_interrupt_paragraphs() {
        // A fence-like line in the middle of a paragraph is paragraph text.
        assert!(!ends_inside_fence("hello\n```\nworld"));
        // After a heading it opens a fence.
        assert!(ends_inside_fence("## heading\n```"));
        // A closing fence must be at least as long as the opening one.
        assert!(ends_inside_fence("````\ncode\n```"));
    }

    #[test]
    fn fences_inside_quotes_and_lists_are_found() {
        assert!(ends_inside_fence("> ```rust\n> fn main() {}"));
        assert!(!ends_inside_fence("> ```rust\n> fn main() {}\n> ```"));
        assert!(ends_inside_fence("- ```\n  code"));
        assert!(!ends_inside_fence("- ```\n  code\n  ```"));
    }

    #[test]
    fn tildes_and_backticks_do_not_cross_close() {
        assert!(ends_inside_fence("```\n~~~"));
        assert!(ends_inside_fence("~~~\n```"));
        assert!(!ends_inside_fence("```\n```"));
        assert!(!ends_inside_fence("~~~\n~~~"));
    }
}

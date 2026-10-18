use std::{ops::Range, rc::Rc};

use gpui::{App, HighlightStyle};

use super::buffer::Buffer;
use crate::forms::{Kind, code_highlights, lex};

/// Each line that opens a block, and the last line the block holds, read from indentation. One pass: a header stays open until a line no deeper than it, and ends at the last line before that one.
pub(crate) fn folds(buffer: &Buffer) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // Headers whose block is still running, each with its indent, shallowest first.
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut last = None;
    for line in 0..buffer.lines() {
        if buffer.line(line).trim().is_empty() {
            continue;
        }
        let indent = buffer.indent(line);
        while let Some(&(header, shallow)) = open.last() {
            if indent > shallow {
                break;
            }
            open.pop();
            if let Some(end) = last.filter(|end| *end > header) {
                out.push((header, end));
            }
        }
        open.push((line, indent));
        last = Some(line);
    }
    for (header, _) in open {
        if let Some(end) = last.filter(|end| *end > header) {
            out.push((header, end));
        }
    }
    out.sort_unstable();
    out
}

/// A bracket in the code, how deep it sits, and where its partner is when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bracket {
    pub offset: usize,
    pub depth: usize,
    pub partner: Option<usize>,
}

fn opener(close: char) -> char {
    match close {
        ')' => '(',
        ']' => '[',
        _ => '{',
    }
}

/// Every bracket outside strings and comments; a closer takes its opener's depth. The whole-text read the index replaces; kept to check it against.
#[cfg(test)]
pub(crate) fn brackets(buffer: &Buffer) -> Vec<Bracket> {
    let mut out: Vec<Bracket> = Vec::new();
    let mut open: Vec<(usize, char)> = Vec::new();
    for line in 0..buffer.lines() {
        let start = buffer.line_range(line).start;
        let text = buffer.line(line);
        for (range, kind) in lex(text) {
            if kind != Kind::Mark {
                continue;
            }
            let Some(ch) = text[range.clone()].chars().next() else {
                continue;
            };
            let offset = start + range.start;
            match ch {
                '(' | '[' | '{' => {
                    open.push((out.len(), ch));
                    out.push(Bracket {
                        offset,
                        depth: open.len() - 1,
                        partner: None,
                    });
                }
                ')' | ']' | '}' => match open.last() {
                    Some((ix, wanted)) if *wanted == opener(ch) => {
                        let ix = *ix;
                        open.pop();
                        let partner = out[ix].offset;
                        out[ix].partner = Some(offset);
                        out.push(Bracket {
                            offset,
                            depth: open.len(),
                            partner: Some(partner),
                        });
                    }
                    _ => out.push(Bracket {
                        offset,
                        depth: open.len(),
                        partner: None,
                    }),
                },
                _ => {}
            }
        }
    }
    out
}

/// Whether a block opens on `line`: it is not blank, and the next line that is not blank sits deeper.
pub(crate) fn opens(buffer: &Buffer, line: usize) -> bool {
    let blank = |line: usize| buffer.line(line).trim().is_empty();
    if blank(line) {
        return false;
    }
    let indent = buffer.indent(line);
    (line + 1..buffer.lines())
        .find(|next| !blank(*next))
        .is_some_and(|next| buffer.indent(next) > indent)
}

/// The headers of the blocks that hold `line`, outermost first: what `folds` gives for the blocks with `header < line <= end`, found by climbing from the line instead of reading every fold. The climb stops at a line of no indent, which no block can hold above it.
pub(crate) fn enclosing(buffer: &Buffer, line: usize) -> Vec<usize> {
    let blank = |line: usize| buffer.line(line).trim().is_empty();
    // A block holds `line` only if it holds the first line from there on that is not blank.
    let Some(anchor) = (line..buffer.lines()).find(|at| !blank(*at)) else {
        return Vec::new();
    };
    let mut shallowest = buffer.indent(anchor);
    let mut headers = Vec::new();
    for above in (0..line).rev() {
        if shallowest == 0 {
            break;
        }
        if blank(above) {
            continue;
        }
        let indent = buffer.indent(above);
        if indent < shallowest {
            headers.push(above);
            shallowest = indent;
        }
    }
    headers.reverse();
    headers
}

/// The brackets of a line as (byte in the line, character), outside strings and comments.
fn line_brackets(text: &str) -> Vec<(usize, char)> {
    // A line with none of the six characters has nothing to read.
    if !text
        .bytes()
        .any(|byte| matches!(byte, b'(' | b'[' | b'{' | b')' | b']' | b'}'))
    {
        return Vec::new();
    }
    lex(text)
        .into_iter()
        .filter(|(_, kind)| *kind == Kind::Mark)
        .filter_map(|(range, _)| {
            let ch = text[range.clone()].chars().next()?;
            matches!(ch, '(' | '[' | '{' | ')' | ']' | '}').then_some((range.start, ch))
        })
        .collect()
}

/// Every bracket outside strings and comments, kept from one frame to the next: a closer takes its opener's depth. A text that changed is read again only between the first and the last line the edits since the last read touched (`Buffer::kept_since`), so an edit costs its own lines and not the file; pairing the brackets is a pass over the brackets alone.
#[derive(Default)]
pub(crate) struct BracketIndex {
    /// Each bracket of the text as of `stamp`, as (offset, character), in order.
    marks: Vec<(usize, char)>,
    found: Rc<Vec<Bracket>>,
    stamp: u64,
    /// Lines the last `update` read.
    pub(crate) lexed: usize,
}

impl BracketIndex {
    /// Reads the brackets of lines `lines` of `buffer`.
    fn read(buffer: &Buffer, lines: Range<usize>) -> Vec<(usize, char)> {
        let mut marks = Vec::new();
        for line in lines {
            let start = buffer.line_range(line).start;
            marks.extend(
                line_brackets(buffer.line(line))
                    .into_iter()
                    .map(|(at, ch)| (start + at, ch)),
            );
        }
        marks
    }

    /// The brackets of `buffer`.
    pub(crate) fn update(&mut self, buffer: &Buffer) -> &[Bracket] {
        if self.stamp == buffer.stamp() {
            self.lexed = 0;
            return &self.found;
        }
        let new = buffer.text();
        // What no edit touched since the last read: the lines between are read again; the brackets before them stay, those after move by what the edits added.
        let Some(kept) = buffer.kept_since(self.stamp) else {
            self.marks = Self::read(buffer, 0..buffer.lines());
            self.lexed = buffer.lines();
            self.stamp = buffer.stamp();
            self.rebuild();
            return &self.found;
        };
        let (old_len, new_len) = (kept.len, new.len());
        // Edits can only have kept what both texts hold.
        let prefix = kept.prefix.min(new_len);
        let suffix = kept.suffix.min(new_len - prefix);
        let front = buffer.line_of(prefix);
        let back = buffer.line_of(new_len - suffix) + 1;
        let from = buffer.line_range(front).start;
        let to = if back < buffer.lines() {
            buffer.line_range(back).start
        } else {
            new_len
        };
        // Where the lines after the re-read ones began in the old text.
        let old_to = (to + old_len).checked_sub(new_len);
        match old_to {
            Some(old_to) if old_to >= from => {
                let head = self.marks.partition_point(|(offset, _)| *offset < from);
                let tail = self.marks.partition_point(|(offset, _)| *offset < old_to);
                let after = self.marks.split_off(tail);
                self.marks.truncate(head);
                self.marks.extend(Self::read(buffer, front..back));
                self.marks.extend(
                    after
                        .into_iter()
                        .map(|(offset, ch)| (offset - old_to + to, ch)),
                );
                self.lexed = back - front;
            }
            _ => {
                self.marks = Self::read(buffer, 0..buffer.lines());
                self.lexed = buffer.lines();
            }
        }
        self.stamp = buffer.stamp();
        self.rebuild();
        &self.found
    }

    /// Pairs the brackets of `marks`.
    fn rebuild(&mut self) {
        let mut out: Vec<Bracket> = Vec::with_capacity(self.marks.len());
        let mut open: Vec<(usize, char)> = Vec::new();
        for &(offset, ch) in &self.marks {
            match ch {
                '(' | '[' | '{' => {
                    open.push((out.len(), ch));
                    out.push(Bracket {
                        offset,
                        depth: open.len() - 1,
                        partner: None,
                    });
                }
                _ => match open.last() {
                    Some((ix, wanted)) if *wanted == opener(ch) => {
                        let ix = *ix;
                        open.pop();
                        let partner = out[ix].offset;
                        out[ix].partner = Some(offset);
                        out.push(Bracket {
                            offset,
                            depth: open.len(),
                            partner: Some(partner),
                        });
                    }
                    _ => out.push(Bracket {
                        offset,
                        depth: open.len(),
                        partner: None,
                    }),
                },
            }
        }
        self.found = Rc::new(out);
    }

    /// The brackets as of the last `update`, to hold across a frame.
    pub(crate) fn shared(&self) -> Rc<Vec<Bracket>> {
        self.found.clone()
    }
}

/// What the whole text of a buffer gives, kept for the revision it was read from.
#[derive(Default)]
pub(crate) struct Structure {
    folds_stamp: u64,
    folds: Rc<Vec<(usize, usize)>>,
    brackets: BracketIndex,
}

impl Structure {
    pub(crate) fn folds(&mut self, buffer: &Buffer) -> Rc<Vec<(usize, usize)>> {
        if self.folds_stamp != buffer.stamp() {
            self.folds = Rc::new(folds(buffer));
            self.folds_stamp = buffer.stamp();
        }
        self.folds.clone()
    }

    pub(crate) fn brackets(&mut self, buffer: &Buffer) -> Rc<Vec<Bracket>> {
        self.brackets.update(buffer);
        self.brackets.shared()
    }
}

/// The pair beside a caret: the bracket just after it, or else the one just before.
pub(crate) fn matched(brackets: &[Bracket], caret: usize) -> Option<(usize, usize)> {
    let at = |offset: usize| {
        brackets
            .binary_search_by_key(&offset, |bracket| bracket.offset)
            .ok()
            .map(|ix| brackets[ix])
    };
    let bracket = at(caret).or_else(|| caret.checked_sub(1).and_then(at))?;
    Some((bracket.offset, bracket.partner?))
}

/// A line's syntax colors as highlight styles.
pub(crate) fn colors(text: &str, cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
    code_highlights(text, cx)
        .into_iter()
        .map(|(range, highlight)| {
            (
                range,
                HighlightStyle {
                    color: Some(highlight.color),
                    background_color: highlight.background,
                    ..HighlightStyle::default()
                },
            )
        })
        .collect()
}

/// Where `needle` appears in the lines of `range`, as offsets, whole words only when it is a word.
pub(crate) fn occurrences(buffer: &Buffer, needle: &str, lines: Range<usize>) -> Vec<Range<usize>> {
    if needle.is_empty() || needle.contains('\n') {
        return Vec::new();
    }
    let word = needle.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
    let boundary = |ch: Option<char>| ch.is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
    let mut out = Vec::new();
    for line in lines.start..lines.end.min(buffer.lines()) {
        let start = buffer.line_range(line).start;
        let text = buffer.line(line);
        for (at, _) in text.match_indices(needle) {
            let end = at + needle.len();
            let whole =
                boundary(text[..at].chars().next_back()) && boundary(text[end..].chars().next());
            if !word || whole {
                out.push(start + at..start + end);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_fold_by_indentation_and_skip_blank_lines() {
        let buffer = Buffer::new("fn a() {\n    one();\n\n    two();\n}\nfn b() {}");
        assert_eq!(folds(&buffer), [(0, 3)]);
    }

    #[test]
    fn brackets_pair_by_depth_outside_strings_and_comments() {
        let buffer = Buffer::new("f(a[0], \"(\") // )\n{ }");
        let found = brackets(&buffer);
        let offsets: Vec<usize> = found.iter().map(|bracket| bracket.offset).collect();
        assert_eq!(
            offsets,
            [1, 3, 5, 11, 18, 20],
            "the quoted and the commented ones do not count"
        );
        assert_eq!(found[1].depth, 1);
        assert_eq!(found[3].partner, Some(1));
        assert_eq!(matched(&found, 12), Some((11, 1)), "just before the caret");
        assert_eq!(matched(&found, 18), Some((18, 20)), "just after the caret");
    }

    // Task 12b fix 1: `folds` (now one pass) and `BracketIndex` replace two
    // whole-file passes that ran on every frame and every caret move. Each must
    // give exactly what the pass it replaces gave, and the index must do only
    // as much lexing as the edit calls for. The ways they can fail, written
    // before the code:
    //   1. a block whose end is a blank line, or whose last line is blank;
    //   2. a block closed by a dedent of several levels at once;
    //   3. a file that ends inside a block, or has one line, or none;
    //   4. tabs against spaces, and lines of only whitespace (blank);
    //   5. a header whose next non-blank line is not deeper (no fold);
    //   6. the index keeping a bracket an edit removed, or missing one it added;
    //   7. an edit that adds or removes line breaks shifting the lines after it;
    //   8. an edit on the first line, on the last, and one that changes everything;
    //   9. an edit that leaves the text as it was, under a new stamp;
    //  10. the index reading the whole file again for a one-character edit.

    /// What the editor computed on every frame before the one pass: each line scans on to the end of its block.
    fn reference_folds(buffer: &Buffer) -> Vec<(usize, usize)> {
        let blank = |line: usize| buffer.line(line).trim().is_empty();
        let mut out = Vec::new();
        for line in 0..buffer.lines() {
            if blank(line) {
                continue;
            }
            let indent = buffer.indent(line);
            let mut end = None;
            for next in line + 1..buffer.lines() {
                if blank(next) {
                    continue;
                }
                if buffer.indent(next) <= indent {
                    break;
                }
                end = Some(next);
            }
            if let Some(end) = end {
                out.push((line, end));
            }
        }
        out
    }

    /// A deterministic spread of indentation: 0..=4 levels of 2 spaces or a tab, some blank lines.
    fn scattered(seed: u64, lines: usize) -> String {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut next = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };
        let mut out = String::new();
        for _ in 0..lines {
            match next() % 7 {
                0 => out.push_str("   "),
                1 => {}
                _ => {
                    let level = next() % 5;
                    if next() % 4 == 0 {
                        out.push_str(&"\t".repeat(level));
                    } else {
                        out.push_str(&"  ".repeat(level));
                    }
                    out.push_str("x");
                }
            }
            out.push('\n');
        }
        out.pop();
        out
    }

    #[test]
    fn one_pass_folds_match_the_pass_they_replace() {
        let fixed = [
            "",
            "a",
            "a\n  b",
            "a\n  b\n\n",
            "a\n\n  b\n\n\n  c\nd",
            "a\n  b\n    c\n      d\ne",
            "a\n  b\n    c\n      d\n  e\nf",
            "a\n  b\nc\n  d",
            "a\n\tb\n\t\tc\nd",
            "a\n    \n  b",
            "  a\nb\n  c",
            "a\nb\nc",
            "a\n  b\n  c\nd\n  e",
        ];
        for text in fixed {
            let buffer = Buffer::new(text);
            assert_eq!(
                folds(&buffer),
                reference_folds(&buffer),
                "{text:?}"
            );
        }
        for seed in 0..300 {
            let text = scattered(seed, 1 + (seed as usize % 40));
            let buffer = Buffer::new(text.clone());
            assert_eq!(
                folds(&buffer),
                reference_folds(&buffer),
                "seed {seed}: {text:?}"
            );
        }
    }

    fn code(lines: usize) -> String {
        (0..lines)
            .map(|n| format!("fn f{n}(a: [u8; 4]) {{ let s = \"({n}\"; g(a[0]) }} // )\n"))
            .collect()
    }

    fn line_start(buffer: &Buffer, line: usize) -> usize {
        buffer.line_range(line).start
    }

    #[test]
    fn the_bracket_index_follows_every_kind_of_edit() {
        let mut buffer = Buffer::new(code(400));
        let mut index = BracketIndex::default();
        assert_eq!(index.update(&buffer), brackets(&buffer).as_slice(), "first read");
        let edits: Vec<Box<dyn Fn(&Buffer) -> (std::ops::Range<usize>, String)>> = vec![
            Box::new(|_| (0..0, "(".into())),
            Box::new(|b| (b.line_range(0).start..b.line_range(1).start, String::new())),
            Box::new(|b| {
                let at = line_start(b, 200) + 6;
                (at..at + 3, "{ [".into())
            }),
            Box::new(|b| {
                let at = line_start(b, 120) + 4;
                (at..at, "\n}\n)\n".into())
            }),
            Box::new(|b| (line_start(b, 50)..line_start(b, 300), String::new())),
            Box::new(|b| (b.len()..b.len(), "tail(\n".into())),
            Box::new(|b| {
                let last = b.lines() - 1;
                (line_start(b, last)..b.len(), "}".into())
            }),
            Box::new(|b| {
                let same = b.text()[3..40].to_string();
                (3..40, same)
            }),
            Box::new(|b| (0..b.len(), "x".into())),
            Box::new(|b| (0..b.len(), "([{}])\n(".into())),
        ];
        for (n, edit) in edits.iter().enumerate() {
            let (range, text) = edit(&buffer);
            buffer.replace(range, &text);
            assert_eq!(
                index.update(&buffer),
                brackets(&buffer).as_slice(),
                "after edit {n}"
            );
        }
    }

    #[test]
    fn a_one_character_edit_reads_only_its_own_line() {
        let mut buffer = Buffer::new(code(2000));
        let mut index = BracketIndex::default();
        index.update(&buffer);
        assert_eq!(index.lexed, buffer.lines(), "the first read takes every line");
        let at = line_start(&buffer, 1000) + 2;
        buffer.replace(at..at, "(");
        index.update(&buffer);
        assert!(
            (1..=2).contains(&index.lexed),
            "an edit on one line lexes about one line, not {}",
            index.lexed
        );
        buffer.replace(0..0, "x\ny\n");
        index.update(&buffer);
        assert!(
            (1..=3).contains(&index.lexed),
            "lines pushed down are not read again, {} were",
            index.lexed
        );
        index.update(&buffer);
        assert_eq!(index.lexed, 0, "the same text is not read twice");
    }

    // Task 12b fix 1, second part: the frame no longer reads every fold of the
    // file. `opens` and `enclosing` answer for one line what `folds` answers for
    // all, and must agree with it. The ways they can fail, written before the code:
    //   1. `opens` true for a blank line, or for a header whose block is only
    //      blank lines, or false for a header followed by blank lines and then
    //      a deeper one;
    //   2. `enclosing` naming a block that ended before the line, or one that
    //      starts on the line itself (a block holds the lines after its header);
    //   3. `enclosing` on a blank line inside a block, between two blocks, or
    //      after the last line;
    //   4. a dedent of several levels at once leaving an outer block out;
    //   5. headers out of order, innermost first instead of outermost first;
    //   6. the scan climbing a whole flat file for a line of no indent.

    #[test]
    fn one_line_answers_agree_with_the_folds_of_the_whole_text() {
        let mut texts: Vec<String> = [
            "",
            "a",
            "a\n  b\n    c\n      d\n  e\nf",
            "a\n\n  b\n\n\n  c\nd",
            "a\n  b\n\n",
            "a\n    \n  b",
            "a\n\tb\n\t\tc\nd",
            "  a\nb\n  c",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        texts.extend((0..300).map(|seed| scattered(seed, 1 + (seed as usize % 40))));
        for text in texts {
            let buffer = Buffer::new(text.clone());
            let all = folds(&buffer);
            for line in 0..buffer.lines() {
                assert_eq!(
                    opens(&buffer, line),
                    all.iter().any(|(header, _)| *header == line),
                    "opens, line {line} of {text:?}"
                );
                let held: Vec<usize> = all
                    .iter()
                    .filter(|(header, end)| *header < line && line <= *end)
                    .map(|(header, _)| *header)
                    .collect();
                assert_eq!(enclosing(&buffer, line), held, "enclosing, line {line} of {text:?}");
            }
        }
    }

    #[test]
    fn occurrences_of_a_word_skip_longer_words() {
        let buffer = Buffer::new("let total = total_cost + total;");
        let found = occurrences(&buffer, "total", 0..1);
        assert_eq!(found, [4..9, 25..30]);
    }
}

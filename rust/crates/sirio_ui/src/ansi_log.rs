//! A CI job's log as lines of styled text, groups that fold, and its first
//! error (spec §7.4, §15.2). Pure: no gpui and no theme — the log tab maps
//! `LogColor` onto Ely's palette. A log's text never leaves this module for
//! a trace or the disk.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFlavor {
    GitHub,
    GitLab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogColor {
    /// One of the sixteen terminal colours: the theme's `ely.ansi`.
    Ansi(u8),
    /// 16..=255 of the xterm palette.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogStyle {
    pub fg: Option<LogColor>,
    pub bg: Option<LogColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Error,
    Warning,
    Notice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// What is drawn and copied: no escape sequence, no marker.
    pub text: String,
    /// Byte ranges of `text` with a style other than the default.
    pub spans: Vec<(Range<usize>, LogStyle)>,
    pub mark: Option<Mark>,
    /// The innermost group the line sits in. A group's own header line sits
    /// in the group's parent.
    pub group: Option<usize>,
    /// This line is the header of that group, and its text the group's title.
    pub opens: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogGroup {
    pub title: String,
    /// The header line.
    pub header: usize,
    /// One past the group's last line.
    pub end: usize,
    pub parent: Option<usize>,
    /// GitHub groups start folded; a GitLab section only when it says so.
    pub folded_at_start: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogDoc {
    pub lines: Vec<LogLine>,
    pub groups: Vec<LogGroup>,
    pub first_error: Option<usize>,
}

/// Reads a whole log. Escape state carries from one line to the next, as in
/// a terminal; a carriage return keeps what follows the last one.
pub fn parse(bytes: &[u8], flavor: LogFlavor) -> LogDoc {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let text = strip_control_strings(text);
    let mut doc = LogDoc::default();
    let mut open: Vec<usize> = Vec::new();
    let mut open_names: Vec<String> = Vec::new();
    let mut style = LogStyle::default();
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    if raw_lines.last() == Some(&"") {
        raw_lines.pop();
    }
    for raw in raw_lines {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let raw = match flavor {
            LogFlavor::GitHub => strip_github_stamp(raw),
            LogFlavor::GitLab => raw.trim_start_matches("\x1b[0K"),
        };
        let mut mark = None;
        let mut body = raw;
        match flavor {
            LogFlavor::GitHub => {
                if let Some(title) = raw.strip_prefix("##[group]") {
                    let (text, spans) = render(title, &mut style);
                    open_group(
                        &mut doc,
                        &mut open,
                        &mut open_names,
                        text,
                        spans,
                        String::new(),
                        true,
                    );
                    continue;
                }
                if raw.starts_with("##[endgroup]") {
                    if let Some(group) = open.pop() {
                        open_names.pop();
                        doc.groups[group].end = doc.lines.len();
                    }
                    continue;
                }
                for (marker, kind) in [
                    ("##[error]", Some(Mark::Error)),
                    ("##[warning]", Some(Mark::Warning)),
                    ("##[notice]", Some(Mark::Notice)),
                    ("##[debug]", None),
                    ("##[command]", None),
                ] {
                    if let Some(rest) = raw.strip_prefix(marker) {
                        mark = kind;
                        body = rest;
                        break;
                    }
                }
            }
            LogFlavor::GitLab => {
                if let Some(rest) = raw.strip_prefix("section_start:") {
                    if let Some((head, tail)) = section_parts(rest) {
                        let (name, collapsed) = section_name(head);
                        let (mut text, mut spans) = render(tail, &mut style);
                        if text.trim().is_empty() {
                            text = name.to_string();
                            spans.clear();
                        }
                        open_group(
                            &mut doc,
                            &mut open,
                            &mut open_names,
                            text,
                            spans,
                            name.to_string(),
                            collapsed,
                        );
                        continue;
                    }
                }
                if let Some(rest) = raw.strip_prefix("section_end:") {
                    if let Some((head, _)) = section_parts(rest) {
                        let (name, _) = section_name(head);
                        if let Some(depth) = open_names
                            .iter()
                            .rposition(|open_name| open_name == name)
                        {
                            while open.len() > depth {
                                let group = open.pop().expect("depth is within the stack");
                                open_names.pop();
                                doc.groups[group].end = doc.lines.len();
                            }
                        }
                        continue;
                    }
                }
            }
        }
        let (text, spans) = render(body, &mut style);
        if flavor == LogFlavor::GitLab && text.starts_with("ERROR: ") {
            mark = Some(Mark::Error);
        }
        if mark == Some(Mark::Error) && doc.first_error.is_none() {
            doc.first_error = Some(doc.lines.len());
        }
        doc.lines.push(LogLine {
            text,
            spans,
            mark,
            group: open.last().copied(),
            opens: None,
        });
    }
    for group in open {
        doc.groups[group].end = doc.lines.len();
    }
    doc
}

fn open_group(
    doc: &mut LogDoc,
    open: &mut Vec<usize>,
    open_names: &mut Vec<String>,
    title: String,
    spans: Vec<(Range<usize>, LogStyle)>,
    name: String,
    folded_at_start: bool,
) {
    let index = doc.groups.len();
    let header = doc.lines.len();
    let parent = open.last().copied();
    doc.groups.push(LogGroup {
        title: title.clone(),
        header,
        end: header + 1,
        parent,
        folded_at_start,
    });
    doc.lines.push(LogLine {
        text: title,
        spans,
        mark: None,
        group: parent,
        opens: Some(index),
    });
    open.push(index);
    open_names.push(name);
}

/// `<time>:<name>[options]\r<title>` → (`<time>:<name>[options]`, `<title>`).
fn section_parts(rest: &str) -> Option<(&str, &str)> {
    let (head, tail) = rest.split_once('\r').unwrap_or((rest, ""));
    let (_, name) = head.split_once(':')?;
    (!name.is_empty()).then_some((head, tail))
}

/// `<time>:<name>[collapsed=true]` → (`<name>`, collapsed).
fn section_name(head: &str) -> (&str, bool) {
    let name = head.split_once(':').map_or(head, |(_, name)| name);
    match name.split_once('[') {
        Some((name, options)) => (name, options.contains("collapsed=true")),
        None => (name, false),
    }
}

/// A control string ends at its terminator or its line's end. A stray
/// introducer must never hide later lines, especially the job's errors.
fn strip_control_strings(text: &str) -> Cow<'_, str> {
    let mut clean = String::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if ch != '\x1b' {
            continue;
        }
        let Some(kind @ (']' | 'P' | 'X' | '^' | '_')) = chars.peek().map(|(_, ch)| *ch) else {
            continue;
        };
        clean.push_str(&text[start..at]);
        chars.next();
        let mut end = text.len();
        while let Some((at, next)) = chars.next() {
            if next == '\n' {
                end = at;
                break;
            }
            if next == '\x07' && kind == ']' {
                end = at + 1;
                break;
            }
            if next == '\x1b' && chars.peek().map(|(_, after)| *after) == Some('\\') {
                end = chars.next().expect("terminator was peeked").0 + 1;
                break;
            }
        }
        start = end;
    }
    if start == 0 {
        Cow::Borrowed(text)
    } else {
        clean.push_str(&text[start..]);
        Cow::Owned(clean)
    }
}

/// `2026-10-04T18:41:28.8717956Z ` at the start of every GitHub line.
fn strip_github_stamp(line: &str) -> &str {
    let bytes = line.as_bytes();
    let digits = |range: Range<usize>| {
        bytes.get(range).is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    let shape = bytes.len() > 20
        && digits(0..4)
        && bytes[4] == b'-'
        && digits(5..7)
        && bytes[7] == b'-'
        && digits(8..10)
        && bytes[10] == b'T'
        && digits(11..13)
        && bytes[13] == b':'
        && digits(14..16)
        && bytes[16] == b':'
        && digits(17..19);
    if !shape {
        return line;
    }
    let mut at = 19;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
    }
    if bytes.get(at) == Some(&b'Z') && bytes.get(at + 1) == Some(&b' ') {
        &line[at + 2..]
    } else {
        line
    }
}

/// Turns one line's text into what is drawn: SGR codes become spans, every
/// other escape and control character is dropped, a tab is four spaces.
fn render(line: &str, style: &mut LogStyle) -> (String, Vec<(Range<usize>, LogStyle)>) {
    let mut text = String::with_capacity(line.len());
    let mut spans: Vec<(Range<usize>, LogStyle)> = Vec::new();
    fn push(
        text: &mut String,
        spans: &mut Vec<(Range<usize>, LogStyle)>,
        piece: &str,
        style: LogStyle,
    ) {
        if piece.is_empty() {
            return;
        }
        let start = text.len();
        text.push_str(piece);
        if style == LogStyle::default() {
            return;
        }
        match spans.last_mut() {
            Some((range, last)) if *last == style && range.end == start => range.end = text.len(),
            _ => spans.push((start..text.len(), style)),
        }
    }
    let mut chars = line.trim_end_matches('\r').char_indices().peekable();
    let mut buffer = [0u8; 4];
    while let Some((_, ch)) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().map(|(_, next)| *next) {
                Some('[') => {
                    chars.next();
                    let mut params = String::new();
                    let mut last = None;
                    for (_, next) in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&next) {
                            last = Some(next);
                            break;
                        }
                        params.push(next);
                    }
                    if last == Some('m') {
                        apply_sgr(&params, style);
                    }
                }
                Some(_) => {
                    // A two-character escape (`ESC 7`), or one with
                    // intermediates (`ESC ( B`): drop it whole.
                    while chars
                        .peek()
                        .is_some_and(|(_, next)| ('\x20'..='\x2f').contains(next))
                    {
                        chars.next();
                    }
                    chars.next();
                }
                None => {}
            },
            '\r' => {
                // Rewriting a progress line clears its text, but SGR state
                // already applied by the overwritten text still carries.
                text.clear();
                spans.clear();
            }
            '\t' => push(&mut text, &mut spans, "    ", *style),
            ch if ch.is_control() => {}
            ch => push(&mut text, &mut spans, ch.encode_utf8(&mut buffer), *style),
        }
    }
    (text, spans)
}

/// Applies one SGR's parameters. A malformed colour stops the sequence and
/// leaves the style as it was before that colour.
fn apply_sgr(params: &str, style: &mut LogStyle) {
    let codes: Vec<Option<u16>> = params
        .split([';', ':'])
        .map(|code| {
            if code.is_empty() {
                Some(0)
            } else {
                code.parse().ok()
            }
        })
        .collect();
    let mut index = 0;
    while index < codes.len() {
        let Some(code) = codes[index] else {
            return;
        };
        match code {
            0 => *style = LogStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            30..=37 => style.fg = Some(LogColor::Ansi((code - 30) as u8)),
            39 => style.fg = None,
            40..=47 => style.bg = Some(LogColor::Ansi((code - 40) as u8)),
            49 => style.bg = None,
            90..=97 => style.fg = Some(LogColor::Ansi((code - 90 + 8) as u8)),
            100..=107 => style.bg = Some(LogColor::Ansi((code - 100 + 8) as u8)),
            38 | 48 => {
                let colour = match codes.get(index + 1).copied().flatten() {
                    Some(5) => {
                        let value = codes
                            .get(index + 2)
                            .copied()
                            .flatten()
                            .filter(|value| *value <= 255);
                        index += 2;
                        value.map(|value| {
                            if value < 16 {
                                LogColor::Ansi(value as u8)
                            } else {
                                LogColor::Indexed(value as u8)
                            }
                        })
                    }
                    Some(2) => {
                        let part = |at: usize| {
                            codes
                                .get(at)
                                .copied()
                                .flatten()
                                .filter(|value| *value <= 255)
                                .map(|value| value as u8)
                        };
                        let rgb = (part(index + 2), part(index + 3), part(index + 4));
                        index += 4;
                        match rgb {
                            (Some(red), Some(green), Some(blue)) => Some(LogColor::Rgb(red, green, blue)),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let Some(colour) = colour else {
                    return;
                };
                if code == 38 {
                    style.fg = Some(colour);
                } else {
                    style.bg = Some(colour);
                }
            }
            _ => {}
        }
        index += 1;
    }
}

/// The xterm palette's colour for 16..=255 (0..=15 are the theme's).
pub fn indexed_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        16..=231 => {
            let level = |value: u8| {
                if value == 0 { 0 } else { 55 + value * 40 }
            };
            let value = index - 16;
            (level(value / 36), level((value / 6) % 6), level(value % 6))
        }
        232..=255 => {
            let grey = 8 + (index - 232) * 10;
            (grey, grey, grey)
        }
        _ => (0, 0, 0),
    }
}

/// The lines left when the `folded` groups hide their members.
pub fn visible_lines(doc: &LogDoc, folded: &BTreeSet<usize>) -> Vec<usize> {
    let mut hidden = vec![false; doc.lines.len()];
    for &group in folded {
        if let Some(group) = doc.groups.get(group) {
            for line in group.header + 1..group.end.min(doc.lines.len()) {
                hidden[line] = true;
            }
        }
    }
    (0..doc.lines.len()).filter(|line| !hidden[*line]).collect()
}

/// The folds a log opens with: the groups that start folded, except those
/// holding the first error, so *Jump to first error* has somewhere to land.
pub fn default_folds(doc: &LogDoc) -> BTreeSet<usize> {
    let mut folds: BTreeSet<usize> = (0..doc.groups.len())
        .filter(|group| doc.groups[*group].folded_at_start)
        .collect();
    let mut holder = doc.first_error.and_then(|line| doc.lines[line].group);
    while let Some(group) = holder {
        folds.remove(&group);
        holder = doc.groups[group].parent;
    }
    folds
}

/// A group's text as *Copy* puts it on the clipboard: its title, then its lines.
pub fn group_text(doc: &LogDoc, group: usize) -> String {
    let Some(group) = doc.groups.get(group) else {
        return String::new();
    };
    lines_text(&doc.lines[group.header..group.end.min(doc.lines.len())])
}

/// The whole log as *Copy log* puts it on the clipboard.
pub fn plain_text(doc: &LogDoc) -> String {
    lines_text(&doc.lines)
}

fn lines_text(lines: &[LogLine]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.text);
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(text: &str) -> LogDoc {
        parse(text.as_bytes(), LogFlavor::GitHub)
    }
    fn gl(text: &str) -> LogDoc {
        parse(text.as_bytes(), LogFlavor::GitLab)
    }
    fn texts(doc: &LogDoc) -> Vec<&str> {
        doc.lines.iter().map(|line| line.text.as_str()).collect()
    }
    fn styled(doc: &LogDoc, line: usize, piece: &str) -> LogStyle {
        let line = &doc.lines[line];
        let start = line.text.find(piece).expect("piece is on the line");
        line.spans
            .iter()
            .find(|(range, _)| range.start <= start && start + piece.len() <= range.end)
            .map(|(_, style)| *style)
            .unwrap_or_default()
    }
    const E: &str = "\x1b";

    #[test]
    fn an_empty_log_has_no_lines() {
        assert_eq!(gh(""), LogDoc::default());
        assert_eq!(gh("\u{feff}"), LogDoc::default());
    }

    #[test]
    fn githubs_bom_and_line_times_are_not_shown() {
        let doc = gh("\u{feff}2026-10-04T18:41:28.8717956Z Current runner\n2026-10-04T18:41:28.87Z next\nno stamp here\n");
        assert_eq!(texts(&doc), ["Current runner", "next", "no stamp here"]);
        // Only GitHub stamps lines: GitLab keeps the text as it is.
        assert_eq!(texts(&gl("2026-10-04T18:41:28.8717956Z kept\n")), ["2026-10-04T18:41:28.8717956Z kept"]);
    }

    #[test]
    fn a_group_folds_its_lines_and_its_end_marker_is_not_a_line() {
        let doc = gh("before\n##[group]Run tests\none\ntwo\n##[endgroup]\nafter\n");
        assert_eq!(texts(&doc), ["before", "Run tests", "one", "two", "after"]);
        assert_eq!(doc.groups.len(), 1);
        let group = &doc.groups[0];
        assert_eq!((group.title.as_str(), group.header, group.end, group.parent), ("Run tests", 1, 4, None));
        assert_eq!(doc.lines[1].opens, Some(0));
        assert_eq!(doc.lines[1].group, None);
        assert_eq!(doc.lines[2].group, Some(0));
        assert_eq!(doc.lines[4].group, None);
        assert!(group.folded_at_start);
    }

    #[test]
    fn a_group_that_never_closes_runs_to_the_end() {
        let doc = gh("##[group]Setup\na\nb\n");
        assert_eq!(doc.groups[0].end, 3);
        assert_eq!(doc.lines[2].group, Some(0));
        // A stray end with nothing open is dropped, not drawn.
        assert_eq!(texts(&gh("##[endgroup]\nx\n")), ["x"]);
    }

    #[test]
    fn markers_set_a_mark_and_the_first_error_is_found() {
        let doc = gh("ok\n##[warning]careful\n##[error]broke\n##[notice]fyi\n##[error]again\n##[debug]dbg\n");
        assert_eq!(texts(&doc), ["ok", "careful", "broke", "fyi", "again", "dbg"]);
        assert_eq!(doc.lines[1].mark, Some(Mark::Warning));
        assert_eq!(doc.lines[2].mark, Some(Mark::Error));
        assert_eq!(doc.lines[3].mark, Some(Mark::Notice));
        assert_eq!(doc.first_error, Some(2));
        assert_eq!(gh("all fine\n").first_error, None);
    }

    #[test]
    fn sixteen_colours_bold_and_reset() {
        let doc = gh(&format!("{E}[31mred{E}[0m plain {E}[1;94mbright{E}[22m thin{E}[39m none\n"));
        let line = &doc.lines[0];
        assert_eq!(line.text, "red plain bright thin none");
        assert_eq!(styled(&doc, 0, "red").fg, Some(LogColor::Ansi(1)));
        assert_eq!(styled(&doc, 0, "plain"), LogStyle::default());
        let bright = styled(&doc, 0, "bright");
        assert_eq!((bright.fg, bright.bold), (Some(LogColor::Ansi(12)), true));
        let thin = styled(&doc, 0, "thin");
        assert_eq!((thin.fg, thin.bold), (Some(LogColor::Ansi(12)), false));
        assert_eq!(styled(&doc, 0, "none"), LogStyle::default());
        let bg = gh(&format!("{E}[42;103mx{E}[49my\n"));
        assert_eq!(styled(&bg, 0, "x").bg, Some(LogColor::Ansi(11)));
        assert_eq!(styled(&bg, 0, "y").bg, None);
        assert_eq!(styled(&bg, 0, "y").fg, None);
    }

    #[test]
    fn dim_italic_and_underline_switch_on_and_off() {
        let doc = gh(&format!("{E}[2;3;4mall{E}[23;24mdim{E}[0m\n"));
        let all = styled(&doc, 0, "all");
        assert!(all.dim && all.italic && all.underline);
        let dim = styled(&doc, 0, "dim");
        assert!(dim.dim && !dim.italic && !dim.underline);
    }

    #[test]
    fn colour_256_and_true_colour() {
        let doc = gh(&format!("{E}[38;5;208ma{E}[38;5;3mb{E}[38;2;10;20;30mc{E}[48;5;17md{E}[48:2:1:2:3me\n"));
        assert_eq!(styled(&doc, 0, "a").fg, Some(LogColor::Indexed(208)));
        assert_eq!(styled(&doc, 0, "b").fg, Some(LogColor::Ansi(3)));
        assert_eq!(styled(&doc, 0, "c").fg, Some(LogColor::Rgb(10, 20, 30)));
        assert_eq!(styled(&doc, 0, "d").bg, Some(LogColor::Indexed(17)));
        assert_eq!(styled(&doc, 0, "e").bg, Some(LogColor::Rgb(1, 2, 3)));
        assert_eq!(indexed_rgb(196), (255, 0, 0));
        assert_eq!(indexed_rgb(16), (0, 0, 0));
        assert_eq!(indexed_rgb(231), (255, 255, 255));
        assert_eq!(indexed_rgb(244), (128, 128, 128));
    }

    #[test]
    fn a_malformed_sgr_changes_nothing_and_shows_nothing() {
        for sequence in ["[38;5m", "[38;2;1;2m", "[38;5;300m", "[999m", "[38m", "[;;m"] {
            let doc = gh(&format!("{E}[31ma{E}{sequence}b\n"));
            assert_eq!(doc.lines[0].text, "ab", "{sequence}");
            // `;;` is three empty parameters, i.e. three resets.
            if sequence != "[;;m" {
                assert_eq!(styled(&doc, 0, "b").fg, Some(LogColor::Ansi(1)), "{sequence}");
            }
        }
        // GitLab's `ESC[0;m` is a reset.
        assert_eq!(styled(&gl(&format!("{E}[31ma{E}[0;mb\n")), 0, "b"), LogStyle::default());
    }

    #[test]
    fn every_other_escape_is_dropped() {
        let doc = gh(&format!(
            "{E}[2Kerase {E}[0Kline {E}]8;;https://x{E}\\link{E}]8;;{E}\\ {E}]0;title\x07bell {E}(Bcharset {E}7save tab\tend{E}\n"
        ));
        assert_eq!(doc.lines[0].text, "erase line link bell charset save tab    end");
        assert!(!doc.lines[0].text.contains('\x1b'));
        assert_eq!(gh("a\x08b\x00c\n").lines[0].text, "abc");
    }

    #[test]
    fn a_carriage_return_keeps_the_last_state() {
        assert_eq!(texts(&gh("Downloading 10%\rDownloading 55%\rDownloading 100%\nnext\n")), ["Downloading 100%", "next"]);
        // A Windows line end is not a progress line.
        assert_eq!(texts(&gh("crlf\r\nnext\r\n")), ["crlf", "next"]);
        // A trailing return with nothing after it keeps what came before.
        assert_eq!(texts(&gh("kept\r\n")), ["kept"]);
    }

    #[test]
    fn style_carries_to_the_next_line_until_reset() {
        let doc = gh(&format!("{E}[33mstart\nstill\n{E}[0mdone\n"));
        assert_eq!(styled(&doc, 1, "still").fg, Some(LogColor::Ansi(3)));
        assert_eq!(styled(&doc, 2, "done"), LogStyle::default());
    }

    #[test]
    fn gitlab_sections_fold_with_their_title_nesting_and_collapsed_flag() {
        let doc = gl(&format!(
            "intro\nsection_start:1:outer[collapsed=true]\r{E}[0K{E}[36;1mOuter title{E}[0;m\na\nsection_start:2:inner\r{E}[0KInner\nb\nsection_end:3:inner\r{E}[0K\nc\nsection_end:4:outer\r{E}[0K\nafter\n"
        ));
        assert_eq!(texts(&doc), ["intro", "Outer title", "a", "Inner", "b", "c", "after"]);
        assert_eq!(doc.groups.len(), 2);
        let (outer, inner) = (&doc.groups[0], &doc.groups[1]);
        assert_eq!((outer.header, outer.end, outer.parent, outer.folded_at_start), (1, 6, None, true));
        assert_eq!((inner.header, inner.end, inner.parent, inner.folded_at_start), (3, 5, Some(0), false));
        assert_eq!(doc.lines[3].group, Some(0));
        assert_eq!(doc.lines[4].group, Some(1));
        assert_eq!(doc.lines[5].group, Some(0));
        // A section with no title shows its name.
        assert_eq!(gl("section_start:1:build_step\r\nx\n").groups[0].title, "build_step");
        // An end naming no open section is dropped and closes nothing.
        let stray = gl("section_start:1:a\rA\nsection_end:2:zzz\r\nx\n");
        assert_eq!(texts(&stray), ["A", "x"]);
        assert_eq!(stray.groups[0].end, 2);
    }

    #[test]
    fn a_gitlab_error_line_is_the_first_error() {
        let doc = gl(&format!("fine\n{E}[31;1mERROR: Job failed: exit code 1{E}[0;m\n"));
        assert_eq!(doc.first_error, Some(1));
        assert_eq!(doc.lines[1].mark, Some(Mark::Error));
        assert_eq!(gl("an ERROR: inside a line\n").first_error, None);
    }

    #[test]
    fn a_line_longer_than_any_screen_is_kept_whole() {
        let long = "x".repeat(100_000);
        assert_eq!(gh(&format!("{long}\n")).lines[0].text.len(), 100_000);
    }

    #[test]
    fn bytes_that_are_not_utf8_do_not_stop_the_log() {
        let doc = parse(b"ok\n\xff\xfebad\nafter\n", LogFlavor::GitHub);
        assert_eq!(doc.lines.len(), 3);
        assert!(doc.lines[1].text.ends_with("bad"));
        assert_eq!(doc.lines[2].text, "after");
    }

    #[test]
    fn a_folded_group_hides_its_members_but_not_its_header() {
        let doc = gh("a\n##[group]G\nb\nc\n##[endgroup]\nd\n");
        let none = BTreeSet::new();
        assert_eq!(visible_lines(&doc, &none), [0, 1, 2, 3, 4]);
        assert_eq!(visible_lines(&doc, &BTreeSet::from([0])), [0, 1, 4]);
        let nested = gl("section_start:1:o\rO\nsection_start:2:i\rI\nx\nsection_end:3:i\r\nsection_end:4:o\r\ny\n");
        assert_eq!(visible_lines(&nested, &BTreeSet::from([1])), [0, 1, 3]);
        assert_eq!(visible_lines(&nested, &BTreeSet::from([0])), [0, 3]);
        assert_eq!(visible_lines(&nested, &BTreeSet::from([0, 1])), [0, 3]);
    }

    #[test]
    fn the_group_holding_the_first_error_starts_open() {
        let doc = gh("##[group]Setup\ns\n##[endgroup]\n##[group]Test\n##[error]broke\n##[endgroup]\n");
        assert_eq!(default_folds(&doc), BTreeSet::from([0]));
        let nested = gl(&format!("section_start:1:o[collapsed=true]\rO\nsection_start:2:i[collapsed=true]\rI\n{E}[31mERROR: x{E}[0m\n"));
        assert_eq!(default_folds(&nested), BTreeSet::new());
    }

    #[test]
    fn copied_text_has_no_escapes_and_keeps_group_titles() {
        let doc = gh(&format!("a\n##[group]G\n{E}[31mred{E}[0m\n##[endgroup]\n"));
        assert_eq!(plain_text(&doc), "a\nG\nred\n");
        assert_eq!(group_text(&doc, 0), "G\nred\n");
    }

    #[test]
    fn control_strings_and_intermediate_escapes_are_dropped_whole() {
        for introducer in ['P', 'X', '^', '_', ']'] {
            let doc = gh(&format!("left{E}{introducer}hidden{E}[31m{E}\\right\n"));
            assert_eq!(texts(&doc), ["leftright"], "{introducer}");
            assert_eq!(styled(&doc, 0, "right"), LogStyle::default());
            let unfinished = gh(&format!("left{E}{introducer}hidden\n"));
            assert_eq!(texts(&unfinished), ["left"], "{introducer}");
            let multiline = gh(&format!(
                "left{E}{introducer}hidden\n##[group]hidden group\n{E}[31mhidden{E}\\right\n"
            ));
            assert_eq!(texts(&multiline), ["left", "hidden group", "hiddenright"], "{introducer}");
            assert_eq!(multiline.groups.len(), 1);
            assert_eq!(styled(&multiline, 2, "right").fg, Some(LogColor::Ansi(1)));
        }
        assert_eq!(texts(&gh(&format!("left{E}((Bright\n"))), ["leftright"]);
    }

    #[test]
    fn a_progress_rewrite_preserves_style_and_restarts_byte_ranges() {
        let doc = gh(&format!("{E}[31m10%\r完了\nstill\n{E}[0mdone\n"));
        assert_eq!(texts(&doc), ["完了", "still", "done"]);
        assert_eq!(styled(&doc, 0, "完了").fg, Some(LogColor::Ansi(1)));
        assert_eq!(doc.lines[0].spans[0].0, 0..6);
        assert_eq!(styled(&doc, 1, "still").fg, Some(LogColor::Ansi(1)));
        assert_eq!(styled(&doc, 2, "done"), LogStyle::default());
        let reset = gh(&format!("{E}[32mold\r{E}[0mnew\nplain\n"));
        assert_eq!(styled(&reset, 0, "new"), LogStyle::default());
        assert_eq!(styled(&reset, 1, "plain"), LogStyle::default());
    }

    #[test]
    fn gitlab_section_markers_after_a_clear_line_still_fold() {
        let doc = gl(&format!(
            "{E}[0Ksection_start:1:build[collapsed=true]\r{E}[0KBuild\ninside\n{E}[0Ksection_end:2:build\r{E}[0K\noutside\n"
        ));
        assert_eq!(texts(&doc), ["Build", "inside", "outside"]);
        assert_eq!(doc.groups.len(), 1);
        assert_eq!((doc.groups[0].header, doc.groups[0].end), (0, 2));
        assert_eq!(default_folds(&doc), BTreeSet::from([0]));
        assert_eq!(visible_lines(&doc, &default_folds(&doc)), [0, 2]);
    }

    #[test]
    fn an_unterminated_control_string_does_not_hide_later_errors() {
        for introducer in ['P', 'X', '^', '_', ']'] {
            let doc = gl(&format!("left{E}{introducer}hidden\nERROR: job failed\nafter\n"));
            assert_eq!(doc.first_error, Some(1), "{introducer}");
            assert_eq!(texts(&doc), ["left", "ERROR: job failed", "after"]);
            assert_eq!(doc.lines[1].mark, Some(Mark::Error));
        }
    }
}

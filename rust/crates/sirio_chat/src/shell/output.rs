//! The output of one shell command, normalised for display and for the agent, and capped in memory.
//!
//! A command's bytes arrive in pieces cut anywhere, so [`ShellOutput`] keeps the raw bytes of the
//! unfinished line until its newline comes: an escape sequence split across two reads is still
//! removed whole. Finished lines are stored normalised.

use std::collections::VecDeque;

/// The most a command's output keeps in memory (and in [`ShellOutput::text`], bar its marker line).
pub const KEEP_BYTES: usize = 1024 * 1024;

/// How much of the first lines stay when the output is longer than [`KEEP_BYTES`].
pub const HEAD_BYTES: usize = 64 * 1024;

/// What the lines after the head and the unfinished line share between them, and so the longest a
/// single line can be.
const TAIL_BYTES: usize = KEEP_BYTES - HEAD_BYTES;

/// One line of output as it is shown and sent, without its `\n`.
///
/// - Bytes are decoded as UTF-8, lossily.
/// - Trailing `\r`s (CRLF) are dropped, and for an inner `\r` only what follows the last one is kept,
///   so a progress bar that redraws collapses to its last state.
/// - CSI (`ESC [ … final byte`) and OSC (`ESC ] … BEL` or `ESC \`) sequences are removed, and so is
///   any other `ESC x` pair.
/// - Every other control character except `\t` is removed.
#[must_use]
pub fn normalize_line(raw: &[u8]) -> String {
    // `\r` is ASCII, so cutting the bytes at one never splits a multi-byte character.
    let end = raw.iter().rposition(|&b| b != b'\r').map_or(0, |i| i + 1);
    let raw = &raw[..end];
    let raw = match raw.iter().rposition(|&b| b == b'\r') {
        Some(i) => &raw[i + 1..],
        None => raw,
    };
    strip_escapes(&String::from_utf8_lossy(raw))
        .chars()
        .filter(|&c| c == '\t' || !c.is_control())
        .collect()
}

/// `text` without its escape sequences.
fn strip_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != 0x1b {
            at += 1;
            continue;
        }
        out.push_str(&text[copied..at]);
        at = escape_end(text, at);
        copied = at;
    }
    out.push_str(&text[copied..]);
    out
}

/// Where the sequence that starts with the `ESC` at `esc` ends. Always past `esc`, and never inside a
/// character: only ASCII bytes are consumed one at a time.
fn escape_end(text: &str, esc: usize) -> usize {
    let bytes = text.as_bytes();
    let mut at = esc + 1;
    match bytes.get(at) {
        None => at,
        Some(b'[') => {
            at += 1;
            while let Some(&b) = bytes.get(at) {
                match b {
                    // Parameter and intermediate bytes.
                    0x20..=0x3f => at += 1,
                    // The final byte.
                    0x40..=0x7e => return at + 1,
                    // Not part of a sequence: it ends this one unfinished and is read as text.
                    _ => break,
                }
            }
            at
        }
        Some(b']') => {
            at += 1;
            while let Some(&b) = bytes.get(at) {
                match b {
                    0x07 => return at + 1,
                    0x1b if bytes.get(at + 1) == Some(&b'\\') => return at + 2,
                    // An escape that is not the terminator starts a sequence of its own.
                    0x1b => return at,
                    _ => at += 1,
                }
            }
            at
        }
        // `ESC ESC`: this one is dropped and the next starts its own sequence.
        Some(0x1b) => at,
        // Any other pair, whole: the second one may be a multi-byte character.
        Some(_) => at + text[at..].chars().next().map_or(1, char::len_utf8),
    }
}

/// Cuts `line` down to its last `max` bytes, on a character boundary, and says how many it cut.
fn keep_last(line: &mut String, max: usize) -> usize {
    if line.len() <= max {
        return 0;
    }
    let mut cut = line.len() - max;
    while !line.is_char_boundary(cut) {
        cut += 1;
    }
    line.drain(..cut);
    cut
}

/// The output of one command: its lines, normalised, within [`KEEP_BYTES`].
///
/// - Past the cap the first [`HEAD_BYTES`] of lines stay, the oldest lines after them are dropped,
///   and one marker line, `[… {n} bytes not kept …]`, sits between the head and the rest.
/// - A line longer than the cap keeps only its last bytes, whether it is finished or not.
/// - The marker is a line of [`Self::text`], [`Self::tail`] and [`Self::kept_lines`], and it is not
///   one of [`Self::total_lines`], so `kept_lines` can be one more than `total_lines`.
#[derive(Clone, Debug, Default)]
pub struct ShellOutput {
    /// The first lines, normalised, while they fit [`HEAD_BYTES`] (each counted with its `\n`).
    head: Vec<String>,
    head_bytes: usize,
    /// Set by the first line that did not fit the head: no later line goes there.
    head_closed: bool,
    /// The most recent finished lines, normalised. With the raw unfinished line they hold at most
    /// [`TAIL_BYTES`] (each line counted with its `\n`).
    tail: VecDeque<String>,
    tail_bytes: usize,
    /// What was cut from the output to hold the caps, in bytes.
    dropped_bytes: u64,
    /// The raw bytes of the line that has not ended, its last [`TAIL_BYTES`] at most.
    pending: VecDeque<u8>,
    /// Every line that ended, the dropped ones included.
    lines_done: u64,
}

/// What [`ShellOutput`] shows: its lines in order, with the marker and the unfinished line in place.
struct Shown<'a> {
    head: &'a [String],
    marker: Option<String>,
    tail: &'a VecDeque<String>,
    unfinished: Option<String>,
}

impl Shown<'_> {
    fn count(&self) -> usize {
        self.head.len()
            + usize::from(self.marker.is_some())
            + self.tail.len()
            + usize::from(self.unfinished.is_some())
    }

    fn iter(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.head
            .iter()
            .map(String::as_str)
            .chain(self.marker.as_deref())
            .chain(self.tail.iter().map(String::as_str))
            .chain(self.unfinished.as_deref())
    }
}

impl ShellOutput {
    /// Takes the next bytes of the output, cut anywhere.
    pub fn push(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some(newline) = rest.iter().position(|&b| b == b'\n') {
            self.append_pending(&rest[..newline]);
            rest = &rest[newline + 1..];
            let line = normalize_line(self.pending.make_contiguous());
            self.pending.clear();
            self.add_line(line);
        }
        self.append_pending(rest);
        self.evict_tail();
    }

    /// The output has ended: the line that had no newline is a line, unless it shows nothing.
    pub fn finish(&mut self) {
        let line = self.unfinished_line();
        self.pending.clear();
        if let Some(line) = line {
            self.add_line(line);
        }
    }

    /// Every line kept, joined by `\n`, with no `\n` after the last one. The unfinished line, when it
    /// shows something, is the last.
    #[must_use]
    pub fn text(&self) -> String {
        let shown = self.shown();
        let mut out = String::new();
        for (i, line) in shown.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }
        out
    }

    /// The last `n` lines of [`Self::text`].
    #[must_use]
    pub fn tail(&self, n: usize) -> String {
        let shown = self.shown();
        let mut lines: Vec<&str> = shown.iter().rev().take(n).collect();
        lines.reverse();
        lines.join("\n")
    }

    /// How many lines [`Self::text`] has.
    #[must_use]
    pub fn kept_lines(&self) -> usize {
        self.shown().count()
    }

    /// Every line the command produced, the dropped ones included.
    #[must_use]
    pub fn total_lines(&self) -> u64 {
        self.lines_done + u64::from(self.unfinished_line().is_some())
    }

    /// A block restored from its stored text, which is already what [`Self::text`] gave. The text is
    /// read again as untrusted. `total_lines` is what the command produced, never less than the lines
    /// held.
    #[must_use]
    pub fn from_text(text: &str, total_lines: u64) -> Self {
        let mut output = Self::default();
        if !text.is_empty() {
            for line in text.split('\n') {
                output.add_line(normalize_line(line.as_bytes()));
            }
        }
        output.lines_done = output.lines_done.max(total_lines);
        output
    }

    fn shown(&self) -> Shown<'_> {
        let (unfinished, cut) = match self.unfinished_line() {
            Some(mut line) => {
                // The unfinished line shares the cap with the lines before it.
                let cut = keep_last(&mut line, TAIL_BYTES.saturating_sub(self.tail_bytes));
                (Some(line), cut)
            }
            None => (None, 0),
        };
        let dropped = self.dropped_bytes + cut as u64;
        Shown {
            head: &self.head,
            marker: (dropped > 0).then(|| format!("[… {dropped} bytes not kept …]")),
            tail: &self.tail,
            unfinished,
        }
    }

    /// The unfinished line as it would end, or `None` when there is none or it shows nothing.
    fn unfinished_line(&self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        let (front, back) = self.pending.as_slices();
        let line = normalize_line(&[front, back].concat());
        (!line.is_empty()).then_some(line)
    }

    /// Adds the bytes of the unfinished line, keeping its last [`TAIL_BYTES`].
    fn append_pending(&mut self, bytes: &[u8]) {
        let skipped = bytes.len().saturating_sub(TAIL_BYTES);
        self.pending.extend(&bytes[skipped..]);
        let over = self.pending.len().saturating_sub(TAIL_BYTES);
        self.pending.drain(..over);
        let mut dropped = skipped + over;
        if dropped > 0 {
            // The new front may be inside a multi-byte character: skip what is left of it.
            for _ in 0..3 {
                if !matches!(self.pending.front(), Some(0x80..=0xbf)) {
                    break;
                }
                self.pending.pop_front();
                dropped += 1;
            }
        }
        self.dropped_bytes += dropped as u64;
    }

    /// Adds a finished line, normalised, to the head while the head has room, else to the tail.
    fn add_line(&mut self, mut line: String) {
        self.lines_done += 1;
        // Room for its `\n` too.
        self.dropped_bytes += keep_last(&mut line, TAIL_BYTES - 1) as u64;
        let size = line.len() + 1;
        if !self.head_closed {
            if self.head_bytes + size <= HEAD_BYTES {
                self.head_bytes += size;
                self.head.push(line);
                return;
            }
            self.head_closed = true;
        }
        self.tail_bytes += size;
        self.tail.push_back(line);
        self.evict_tail();
    }

    /// Drops the oldest lines of the tail until it and the unfinished line fit [`TAIL_BYTES`].
    fn evict_tail(&mut self) {
        while self.tail_bytes + self.pending.len() > TAIL_BYTES {
            let Some(oldest) = self.tail.pop_front() else {
                break;
            };
            let size = oldest.len() + 1;
            self.tail_bytes -= size;
            self.dropped_bytes += size as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- normalize_line -------------------------------------------------

    #[test]
    fn strips_csi_and_osc() {
        assert_eq!(
            normalize_line(b"\x1b[31mred\x1b[0m \x1b]0;title\x07ok"),
            "red ok"
        );
    }

    #[test]
    fn carriage_return_keeps_the_last_redraw() {
        assert_eq!(normalize_line(b"10%\r50%\r100%"), "100%");
    }

    #[test]
    fn crlf_is_a_plain_line_end() {
        assert_eq!(normalize_line(b"done\r"), "done");
    }

    #[test]
    fn trailing_carriage_returns_are_all_a_line_end() {
        // `a\r\r\n` must not turn into an empty line by "keeping what follows the last \r".
        assert_eq!(normalize_line(b"a\r\r"), "a");
        assert_eq!(normalize_line(b"10%\r100%\r"), "100%");
    }

    #[test]
    fn keeps_tabs_drops_other_controls() {
        assert_eq!(normalize_line(b"a\tb\x07c\x00d"), "a\tbcd");
    }

    #[test]
    fn c1_controls_in_valid_utf8_are_removed() {
        assert_eq!(normalize_line("a\u{85}b\u{9b}c\u{7f}d".as_bytes()), "abcd");
    }

    #[test]
    fn invalid_utf8_is_replaced_not_panicked() {
        assert_eq!(normalize_line(&[b'a', 0xff, b'b']), "a\u{fffd}b");
    }

    #[test]
    fn osc_may_end_with_the_string_terminator() {
        assert_eq!(normalize_line(b"a\x1b]8;;http://x\x1b\\b\x1b]0;t\x1b\\c"), "abc");
    }

    #[test]
    fn other_escape_pairs_are_removed_whole() {
        assert_eq!(normalize_line(b"a\x1bMb"), "ab");
        assert_eq!(normalize_line("a\x1b\u{e9}b".as_bytes()), "ab");
    }

    #[test]
    fn a_lone_escape_or_unterminated_sequence_at_the_end_shows_nothing() {
        assert_eq!(normalize_line(b"a\x1b"), "a");
        assert_eq!(normalize_line(b"a\x1b["), "a");
        assert_eq!(normalize_line(b"a\x1b[31"), "a");
        assert_eq!(normalize_line(b"ok\x1b]0;never ends"), "ok");
    }

    #[test]
    fn a_truncated_csi_does_not_swallow_the_text_after_it() {
        assert_eq!(normalize_line("a\x1b[12\u{e9}b".as_bytes()), "a\u{e9}b");
        assert_eq!(normalize_line(b"a\x1b[12\x07b"), "ab");
    }

    #[test]
    fn an_escape_inside_an_osc_starts_a_new_sequence() {
        assert_eq!(normalize_line(b"a\x1b]0;t\x1b[31mb"), "ab");
        assert_eq!(normalize_line(b"a\x1b\x1b[31mb"), "ab");
    }

    #[test]
    fn a_wall_of_escapes_does_not_blow_the_stack_or_loop() {
        let wall = b"\x1b]".repeat(500_000);
        assert_eq!(normalize_line(&wall), "");
        let wall = b"\x1b\x1b[".repeat(500_000);
        assert_eq!(normalize_line(&wall), "");
    }

    // ---- ShellOutput: lines and escapes across pushes ---------------------

    #[test]
    fn escape_split_across_pushes_is_still_removed() {
        let mut o = ShellOutput::default();
        o.push(b"x\x1b[");
        o.push(b"1mY\n");
        assert_eq!(o.text(), "xY");
    }

    #[test]
    fn chunking_never_changes_the_text() {
        let sample = "ls\r\n\x1b[1;32mgreen\x1b[0m done\n10%\r50%\r100%\n\x1b]0;t\x07caf\u{e9} \u{1f600}\n\ttabbed\r\n\nlast"
            .as_bytes();
        let mut whole = ShellOutput::default();
        whole.push(sample);
        whole.finish();
        let mut bytewise = ShellOutput::default();
        for b in sample {
            bytewise.push(std::slice::from_ref(b));
        }
        bytewise.finish();
        assert_eq!(whole.text(), bytewise.text());
        assert_eq!(
            whole.text(),
            "ls\ngreen done\n100%\ncaf\u{e9} \u{1f600}\n\ttabbed\n\nlast"
        );
        assert_eq!(whole.total_lines(), bytewise.total_lines());
    }

    #[test]
    fn binary_garbage_never_panics_and_has_no_controls() {
        let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
        let mut o = ShellOutput::default();
        o.push(&bytes);
        o.finish();
        assert!(
            o.text()
                .chars()
                .all(|c| c == '\n' || c == '\t' || !c.is_control())
        );
    }

    #[test]
    fn blank_lines_are_lines_and_the_trailing_newline_is_not() {
        let mut o = ShellOutput::default();
        o.push(b"a\n\nb\n");
        assert_eq!(o.text(), "a\n\nb");
        assert_eq!(o.kept_lines(), 3);
        assert_eq!(o.total_lines(), 3);
        o.push(b"\n");
        assert_eq!(o.text(), "a\n\nb\n");
    }

    #[test]
    fn an_unfinished_line_is_the_last_line_until_it_is_finished() {
        let mut o = ShellOutput::default();
        o.push(b"a\nb");
        assert_eq!(o.text(), "a\nb");
        assert_eq!(o.kept_lines(), 2);
        assert_eq!(o.total_lines(), 2);
        o.finish();
        o.finish();
        assert_eq!(o.text(), "a\nb");
        assert_eq!(o.kept_lines(), 2);
        assert_eq!(o.total_lines(), 2);
    }

    #[test]
    fn an_unfinished_line_that_shows_nothing_adds_no_line() {
        let mut o = ShellOutput::default();
        o.push(b"a\n\x1b[0m");
        assert_eq!(o.text(), "a");
        assert_eq!(o.kept_lines(), 1);
        o.finish();
        assert_eq!(o.text(), "a");
        assert_eq!(o.kept_lines(), 1);
        assert_eq!(o.total_lines(), 1);
    }

    #[test]
    fn nothing_pushed_is_no_text_and_no_lines() {
        let mut o = ShellOutput::default();
        o.finish();
        assert_eq!(o.text(), "");
        assert_eq!(o.kept_lines(), 0);
        assert_eq!(o.total_lines(), 0);
        assert_eq!(o.tail(5), "");
    }

    // ---- ShellOutput: the cap ---------------------------------------------

    fn numbered(count: usize) -> ShellOutput {
        let mut o = ShellOutput::default();
        for i in 0..count {
            o.push(format!("line {i:06}\n").as_bytes());
        }
        o
    }

    #[test]
    fn output_under_the_cap_is_kept_whole_with_no_marker() {
        let o = numbered(10_000);
        assert!(!o.text().contains("not kept"));
        assert_eq!(o.kept_lines(), 10_000);
        assert_eq!(o.total_lines(), 10_000);
        assert!(o.text().starts_with("line 000000\n"));
        assert!(o.text().ends_with("line 009999"));
    }

    #[test]
    fn caps_many_lines_at_head_and_tail() {
        let mut o = ShellOutput::default();
        for i in 0..200_000 {
            o.push(format!("line {i:06}\n").as_bytes());
        }
        let t = o.text();
        assert!(t.len() <= KEEP_BYTES + 128);
        assert!(t.starts_with("line 000000"));
        assert!(t.ends_with("line 199999"));
        assert!(t.contains("bytes not kept …]"));
        assert_eq!(o.total_lines(), 200_000);
    }

    #[test]
    fn the_marker_sits_once_between_a_head_and_a_tail_that_are_contiguous() {
        let o = numbered(200_000);
        let text = o.text();
        let lines: Vec<&str> = text.split('\n').collect();
        let markers: Vec<usize> = (0..lines.len())
            .filter(|&i| lines[i].contains("not kept"))
            .collect();
        assert_eq!(markers.len(), 1, "exactly one marker");
        let at = markers[0];
        assert!(at > 0 && at + 1 < lines.len(), "marker is neither first nor last");
        let number = |l: &str| l["line ".len()..].parse::<u64>().unwrap();
        // The head is a prefix and the tail a suffix, each without a gap.
        for pair in lines[..at].windows(2) {
            assert_eq!(number(pair[1]), number(pair[0]) + 1);
        }
        for pair in lines[at + 1..].windows(2) {
            assert_eq!(number(pair[1]), number(pair[0]) + 1);
        }
        assert!(number(lines[at + 1]) > number(lines[at - 1]) + 1, "lines were dropped");
        // The head is about HEAD_BYTES of lines.
        let head_len: usize = lines[..at].iter().map(|l| l.len() + 1).sum();
        assert!(head_len <= HEAD_BYTES && head_len + 32 > HEAD_BYTES, "head is {head_len}");
        // The oldest lines after the head are the ones dropped: the tail reaches the last line.
        assert_eq!(number(lines[lines.len() - 1]), 199_999);
        assert_eq!(o.kept_lines(), lines.len());
    }

    #[test]
    fn the_marker_count_follows_the_dropped_bytes_without_a_second_marker() {
        let dropped = |o: &ShellOutput| -> u64 {
            let t = o.text();
            let start = t.find("[… ").unwrap() + "[… ".len();
            let end = t[start..].find(' ').unwrap();
            t[start..start + end].parse().unwrap()
        };
        let mut o = numbered(150_000);
        let first = dropped(&o);
        for i in 150_000..200_000 {
            o.push(format!("line {i:06}\n").as_bytes());
        }
        assert!(dropped(&o) > first);
        assert_eq!(o.text().matches("not kept").count(), 1);
    }

    #[test]
    fn caps_one_endless_line() {
        let mut o = ShellOutput::default();
        for _ in 0..3 * 1024 {
            o.push(&[b'z'; 1024]);
        }
        assert!(o.text().len() <= KEEP_BYTES);
    }

    #[test]
    fn an_endless_line_keeps_its_last_bytes_and_says_how_much_went() {
        let mut o = ShellOutput::default();
        o.push(b"START");
        for _ in 0..2 * 1024 {
            o.push(&[b'z'; 1024]);
        }
        o.push(b"END");
        assert!(o.text().len() <= KEEP_BYTES);
        assert!(o.text().ends_with("zzzEND"));
        assert!(!o.text().contains("START"));
        assert!(o.text().contains("bytes not kept …]"));
        assert_eq!(o.total_lines(), 1);
        o.finish();
        assert!(o.text().len() <= KEEP_BYTES);
        assert!(o.text().ends_with("zzzEND"));
        assert_eq!(o.total_lines(), 1);
    }

    #[test]
    fn one_push_of_a_huge_line_and_a_huge_unfinished_tail_obey_the_cap() {
        let mut o = ShellOutput::default();
        let mut huge = vec![b'y'; 3 * 1024 * 1024];
        huge.push(b'\n');
        o.push(&huge);
        o.push(b"after\n");
        assert!(o.text().len() <= KEEP_BYTES + 128);
        assert!(o.text().ends_with("after"));
        assert_eq!(o.total_lines(), 2);
        o.push(&vec![b'x'; 3 * 1024 * 1024]);
        assert!(o.text().len() <= KEEP_BYTES + 128);
        o.finish();
        assert!(o.text().len() <= KEEP_BYTES + 128);
        assert!(o.text().ends_with('x'));
    }

    #[test]
    fn an_unfinished_line_counts_against_the_same_budget_as_a_full_tail() {
        let mut o = numbered(200_000);
        let chunk = vec![b'u'; 64 * 1024];
        for _ in 0..32 {
            o.push(&chunk);
            assert!(o.text().len() <= KEEP_BYTES + 128);
        }
        assert!(o.text().ends_with("uuu"));
        o.finish();
        assert!(o.text().len() <= KEEP_BYTES + 128);
        assert!(o.text().starts_with("line 000000"));
        assert!(o.text().ends_with("uuu"));
    }

    #[test]
    fn invalid_utf8_that_grows_when_decoded_still_obeys_the_cap() {
        let mut o = ShellOutput::default();
        for _ in 0..900 {
            o.push(&[0xff; 1024]);
        }
        assert!(o.text().len() <= KEEP_BYTES + 128);
        o.finish();
        assert!(o.text().len() <= KEEP_BYTES + 128);
        assert!(o.text().contains('\u{fffd}'));
        let mut o = ShellOutput::default();
        let mut line = vec![0xff; 900 * 1024];
        line.push(b'\n');
        o.push(&line);
        assert!(o.text().len() <= KEEP_BYTES + 128);
    }

    #[test]
    fn a_cut_never_lands_inside_a_multibyte_character() {
        // Raw bytes of an unfinished line are cut: the end is made unaligned with an `x`, so the cut
        // falls inside an emoji and must not leave a piece of one at the front.
        let emoji = "\u{1f600}";
        let mut o = ShellOutput::default();
        for _ in 0..600 {
            o.push(emoji.repeat(1000).as_bytes());
        }
        o.push(b"x");
        assert!(o.text().len() <= KEEP_BYTES);
        let text = o.text();
        let body = text.split('\n').nth(1).unwrap();
        assert!(!body.contains('\u{fffd}'), "no stray piece of a character");
        assert!(body.len() > HEAD_BYTES);
        assert!(body.trim_end_matches('x').chars().all(|c| c == '\u{1f600}'));
        // A finished line is cut on the decoded text.
        o.finish();
        let text = o.text();
        let body = text.split('\n').nth(1).unwrap();
        assert!(!body.contains('\u{fffd}'));
        assert!(body.trim_end_matches('x').chars().all(|c| c == '\u{1f600}'));
        let mut o = ShellOutput::default();
        let mut line = "\u{e9}".repeat(700 * 1024).into_bytes();
        line.push(b'\n');
        o.push(&line);
        assert!(o.text().len() <= KEEP_BYTES);
        let text = o.text();
        let body = text.split('\n').nth(1).unwrap();
        assert!(body.len() > HEAD_BYTES && body.chars().all(|c| c == '\u{e9}'));
    }

    // ---- ShellOutput: tail, counts, restore -------------------------------

    #[test]
    fn tail_returns_the_last_lines() {
        let mut o = ShellOutput::default();
        o.push(b"a\nb\nc\n");
        assert_eq!(o.tail(2), "b\nc");
    }

    #[test]
    fn tail_edges_and_the_unfinished_line() {
        let mut o = ShellOutput::default();
        o.push(b"a\nb\nc");
        assert_eq!(o.tail(0), "");
        assert_eq!(o.tail(1), "c");
        assert_eq!(o.tail(3), o.text());
        assert_eq!(o.tail(99), o.text());
    }

    #[test]
    fn tail_and_kept_lines_count_the_marker_as_a_line() {
        let o = numbered(200_000);
        let text = o.text();
        assert_eq!(o.kept_lines(), text.split('\n').count());
        assert_eq!(o.tail(o.kept_lines()), text);
        assert_eq!(o.tail(o.kept_lines() + 7), text);
        assert_eq!(o.tail(1), "line 199999");
        assert_eq!(o.tail(3), "line 199997\nline 199998\nline 199999");
        assert!(o.total_lines() > o.kept_lines() as u64);
        assert!(o.tail(o.kept_lines()).contains("not kept"));
    }

    #[test]
    fn from_text_round_trips() {
        let o = ShellOutput::from_text("a\nb", 9);
        assert_eq!(o.text(), "a\nb");
        assert_eq!(o.total_lines(), 9);
    }

    #[test]
    fn from_text_keeps_a_blank_last_line_and_an_empty_text_has_no_lines() {
        let mut o = ShellOutput::default();
        o.push(b"a\n\n");
        let restored = ShellOutput::from_text(&o.text(), o.total_lines());
        assert_eq!(restored.text(), o.text());
        assert_eq!(restored.kept_lines(), 2);
        assert_eq!(restored.total_lines(), 2);
        let empty = ShellOutput::from_text("", 0);
        assert_eq!(empty.kept_lines(), 0);
        assert_eq!(empty.text(), "");
    }

    #[test]
    fn from_text_is_not_trusted_and_never_reports_fewer_lines_than_it_holds() {
        let o = ShellOutput::from_text("a\x1b[31mb\r\nc\x07", 1);
        assert_eq!(o.text(), "ab\nc");
        assert_eq!(o.kept_lines(), 2);
        assert!(o.total_lines() >= o.kept_lines() as u64);
    }

    #[test]
    fn a_restored_block_can_take_more_output() {
        let mut o = ShellOutput::from_text("a\nb", 9);
        o.push(b"c\n");
        assert_eq!(o.text(), "a\nb\nc");
        assert_eq!(o.total_lines(), 10);
    }
}

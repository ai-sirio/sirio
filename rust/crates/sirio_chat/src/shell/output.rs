//! The output of one shell command, normalised for display and for the agent, and capped in memory.
//!
//! A command's bytes arrive in pieces cut anywhere, so [`ShellOutput`] keeps the raw bytes of the
//! unfinished line until its newline comes: an escape sequence split across two reads is still
//! removed whole. Finished lines are stored normalised.

use std::collections::{VecDeque, vec_deque};

/// The most a command's output keeps in memory (and in [`ShellOutput::text`], bar its marker line).
pub const KEEP_BYTES: usize = 1024 * 1024;

/// How much of the first lines stay when the output is longer than [`KEEP_BYTES`].
pub const HEAD_BYTES: usize = 64 * 1024;

/// What the lines after the head and the unfinished line share between them, and so the most one
/// line can take.
const TAIL_BYTES: usize = KEEP_BYTES - HEAD_BYTES;

/// What a kept line costs beyond its text and its `\n`: its entry in the index of lengths (4 bytes,
/// and as much again for the room a growing queue keeps free). It is charged to the cap, so a flood
/// of tiny lines holds about [`KEEP_BYTES`] of memory and not a multiple of it.
const LINE_INDEX_BYTES: usize = 8;

/// The least a buffer grows by, and the room it may keep empty when it is small.
const MIN_GROW: usize = 4 * 1024;

/// How much room to ask for when a buffer holding `len` is full and `need` more must fit: a quarter
/// of what it holds, so growth is amortised without the doubling that would leave up to half of a
/// buffer empty, and never less than `floor`.
fn grow_by(len: usize, need: usize, floor: usize) -> usize {
    need.max(len / 4).max(floor)
}

/// One line of output as it is shown and sent, without its `\n`.
///
/// - Bytes are decoded as UTF-8, lossily.
/// - Trailing `\r`s (CRLF) are dropped, and for an inner `\r` only what follows the last one is kept,
///   so a progress bar that redraws collapses to its last state.
/// - Escape sequences are removed whole: CSI (`ESC [ … final byte`), OSC, DCS, SOS, PM and APC
///   strings (`ESC ] … BEL` or `ESC \`, and likewise for `P`, `X`, `^` and `_`), nF sequences
///   (`ESC`, intermediate bytes, a final byte) and any other `ESC x` pair.
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
        // OSC, DCS, SOS, PM and APC: a string, up to its terminator.
        Some(b']' | b'P' | b'X' | b'^' | b'_') => {
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
        // nF: intermediate bytes, then one final byte (`ESC ( B`, `ESC # 8`, `ESC % G`).
        Some(0x20..=0x2f) => {
            while matches!(bytes.get(at), Some(0x20..=0x2f)) {
                at += 1;
            }
            if matches!(bytes.get(at), Some(0x30..=0x7e)) {
                at += 1;
            }
            at
        }
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

/// Lines back to back in one buffer, each with its `\n`, and an index of their lengths: a short line
/// costs its bytes and [`LINE_INDEX_BYTES`], not an allocation of its own.
#[derive(Clone, Debug, Default)]
struct LineStore {
    /// The live lines from `start` on. Before `start` are lines already dropped, until the next
    /// compaction.
    text: String,
    start: usize,
    /// The length of each live line, without its `\n`.
    lens: VecDeque<u32>,
}

impl LineStore {
    /// What a line of `len` bytes is charged to the cap.
    fn cost(len: usize) -> usize {
        len + 1 + LINE_INDEX_BYTES
    }

    fn len(&self) -> usize {
        self.lens.len()
    }

    /// What the live lines are charged to the cap.
    fn charged(&self) -> usize {
        self.text.len() - self.start + self.lens.len() * LINE_INDEX_BYTES
    }

    /// Adds a line. The buffers grow by a quarter, never by doubling, and a dead prefix of an eighth
    /// or more is dropped before they do, so a store that only turns over keeps the size it has.
    fn push(&mut self, line: &str) {
        let need = line.len() + 1;
        if self.text.capacity() - self.text.len() < need {
            if self.start * 8 >= self.text.len() {
                self.compact();
            }
            if self.text.capacity() - self.text.len() < need {
                self.text
                    .reserve_exact(grow_by(self.text.len(), need, MIN_GROW));
            }
        }
        self.text.push_str(line);
        self.text.push('\n');
        if self.lens.len() == self.lens.capacity() {
            self.lens
                .reserve_exact(grow_by(self.lens.len(), 1, MIN_GROW / size_of::<u32>()));
        }
        self.lens
            .push_back(u32::try_from(line.len()).expect("a kept line is shorter than the cap"));
    }

    /// Drops the oldest line and says what it was charged. The buffers are given back once they are
    /// half again as big as what is left, so a store that empties does not keep its room.
    fn pop_front(&mut self) -> Option<usize> {
        let len = self.lens.pop_front()? as usize;
        self.start += len + 1;
        let live = self.text.len() - self.start;
        if self.text.capacity() > live + live / 2 + MIN_GROW {
            self.compact();
            self.text.shrink_to(live + live / 4);
        }
        let held = self.lens.len();
        if self.lens.capacity() > held + held / 2 + MIN_GROW / size_of::<u32>() {
            self.lens.shrink_to(held + held / 4);
        }
        Some(Self::cost(len))
    }

    /// Moves the live lines to the front of the buffer.
    fn compact(&mut self) {
        if self.start > 0 {
            self.text.drain(..self.start);
            self.start = 0;
        }
    }

    /// Gives back every byte of room.
    fn shrink_to_fit(&mut self) {
        self.compact();
        self.text.shrink_to_fit();
        self.lens.shrink_to_fit();
    }

    fn iter(&self) -> Lines<'_> {
        Lines {
            text: &self.text,
            front: self.start,
            back: self.text.len(),
            lens: self.lens.iter(),
        }
    }
}

/// The lines of a [`LineStore`], from either end.
struct Lines<'a> {
    text: &'a str,
    front: usize,
    back: usize,
    lens: vec_deque::Iter<'a, u32>,
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let len = *self.lens.next()? as usize;
        let line = &self.text[self.front..self.front + len];
        self.front += len + 1;
        Some(line)
    }
}

impl DoubleEndedIterator for Lines<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let len = *self.lens.next_back()? as usize;
        self.back -= len + 1;
        Some(&self.text[self.back..self.back + len])
    }
}

/// The output of one command: its lines, normalised, within [`KEEP_BYTES`].
///
/// - Past the cap the first [`HEAD_BYTES`] of lines stay, the oldest lines after them are dropped,
///   and one marker line, `[… {n} bytes not kept …]`, sits between the head and the rest.
/// - A line longer than the cap keeps only its last bytes, whether it is finished or not.
/// - A line that a `\r` redraws is not a loss: what the redraw replaces costs nothing and is not in
///   the marker.
/// - The marker is a line of [`Self::text`], [`Self::tail`] and [`Self::kept_lines`], and it is not
///   one of [`Self::total_lines`], so `kept_lines` can be one more than `total_lines`.
#[derive(Clone, Debug, Default)]
pub struct ShellOutput {
    /// The first lines, normalised, while they fit [`HEAD_BYTES`] (each charged [`LineStore::cost`]).
    head: LineStore,
    /// Set by the first line that did not fit the head, and by the first bytes dropped: no later
    /// line goes to the head, so the marker never comes after a line that arrived later.
    head_closed: bool,
    /// The most recent finished lines, normalised. With the raw unfinished line they hold at most
    /// [`TAIL_BYTES`].
    tail: LineStore,
    /// What was cut from the output to hold the caps, in bytes.
    dropped_bytes: u64,
    /// The raw bytes of the line that has not ended, its last [`TAIL_BYTES`] at most, and with no
    /// `\r` that more text follows: a redraw has replaced what came before it.
    pending: VecDeque<u8>,
    /// What was cut from the front of `pending`. It is lost only if the line ends as it is: a
    /// redraw clears it.
    pending_cut: u64,
    /// Every line that ended, the dropped ones included.
    lines_done: u64,
}

/// What [`ShellOutput`] shows: its lines in order, with the marker and the unfinished line in place.
struct Shown<'a> {
    head: &'a LineStore,
    marker: Option<String>,
    tail: &'a LineStore,
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
            .chain(self.marker.as_deref())
            .chain(self.tail.iter())
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
            self.clear_pending();
            let cut = std::mem::take(&mut self.pending_cut);
            self.drop_bytes(cut);
            self.add_line(line);
        }
        self.append_pending(rest);
        self.evict_tail(0);
    }

    /// The output has ended: the line that had no newline is a line, unless it shows nothing.
    pub fn finish(&mut self) {
        let line = self.unfinished_line();
        self.clear_pending();
        let cut = std::mem::take(&mut self.pending_cut);
        self.drop_bytes(cut);
        if let Some(line) = line {
            self.add_line(line);
        }
        self.shrink_to_fit();
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
        output.shrink_to_fit();
        output
    }

    /// What the buffers have allocated, in bytes: the memory this holds, not the text it holds.
    #[cfg(test)]
    fn capacity_bytes(&self) -> usize {
        self.head.text.capacity()
            + self.tail.text.capacity()
            + (self.head.lens.capacity() + self.tail.lens.capacity()) * size_of::<u32>()
            + self.pending.capacity()
    }

    fn shown(&self) -> Shown<'_> {
        let mut dropped = self.dropped_bytes + self.pending_cut;
        let unfinished = self.unfinished_line().map(|mut line| {
            // The unfinished line shares the cap with the lines before it.
            let room = TAIL_BYTES.saturating_sub(self.tail.charged());
            dropped += keep_last(&mut line, room) as u64;
            line
        });
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
    fn append_pending(&mut self, mut bytes: &[u8]) {
        // A `\r` that more text follows is a redraw: `normalize_line` keeps only what comes after
        // it, so what came before is dropped here, at no cost to the cap and with no marker. The
        // pending bytes hold no such `\r`, so only the new ones and the pending line's own trailing
        // `\r` can be one.
        if let Some(last) = bytes.iter().rposition(|&b| b != b'\r') {
            match bytes[..last].iter().rposition(|&b| b == b'\r') {
                Some(redraw) => {
                    self.clear_pending();
                    self.pending_cut = 0;
                    bytes = &bytes[redraw + 1..];
                }
                None if self.pending.back() == Some(&b'\r') => {
                    self.clear_pending();
                    self.pending_cut = 0;
                }
                None => {}
            }
        }
        let skipped = bytes.len().saturating_sub(TAIL_BYTES);
        let new = &bytes[skipped..];
        // Drop the oldest pending bytes before adding, so the queue never holds more than the cap.
        let over = (self.pending.len() + new.len()).saturating_sub(TAIL_BYTES);
        self.pending.drain(..over);
        if self.pending.capacity() - self.pending.len() < new.len() {
            let grow = grow_by(self.pending.len(), new.len(), MIN_GROW);
            self.pending
                .reserve_exact(grow.min(TAIL_BYTES - self.pending.len()));
        }
        self.pending.extend(new);
        let mut cut = skipped + over;
        if cut > 0 {
            // The new front may be inside a multi-byte character: skip what is left of it.
            for _ in 0..3 {
                if !matches!(self.pending.front(), Some(0x80..=0xbf)) {
                    break;
                }
                self.pending.pop_front();
                cut += 1;
            }
        }
        self.pending_cut += cut as u64;
    }

    /// Adds a finished line, normalised, to the head while the head has room, else to the tail.
    fn add_line(&mut self, mut line: String) {
        self.lines_done += 1;
        let cut = keep_last(&mut line, TAIL_BYTES - LineStore::cost(0));
        self.drop_bytes(cut as u64);
        if !self.head_closed {
            if self.head.charged() + LineStore::cost(line.len()) <= HEAD_BYTES {
                self.head.push(&line);
                return;
            }
            self.head_closed = true;
        }
        // Make room first, so the tail never holds more than the cap, not even for a moment.
        self.evict_tail(LineStore::cost(line.len()));
        debug_assert!(self.tail.charged() + LineStore::cost(line.len()) <= TAIL_BYTES);
        self.tail.push(&line);
    }

    /// Empties the unfinished line, and gives back its room unless it is small.
    fn clear_pending(&mut self) {
        self.pending.clear();
        if self.pending.capacity() > 2 * MIN_GROW {
            self.pending.shrink_to(MIN_GROW);
        }
    }

    /// Gives back all the room the buffers hold. The output has ended.
    fn shrink_to_fit(&mut self) {
        self.head.shrink_to_fit();
        self.tail.shrink_to_fit();
        self.pending.shrink_to_fit();
    }

    /// Drops the oldest lines of the tail until it, the unfinished line and `extra` more bytes fit
    /// [`TAIL_BYTES`].
    fn evict_tail(&mut self, extra: usize) {
        while self.tail.charged() + self.pending.len() + extra > TAIL_BYTES {
            let Some(cost) = self.tail.pop_front() else {
                break;
            };
            self.drop_bytes((cost - LINE_INDEX_BYTES) as u64);
        }
    }

    /// Counts bytes as not kept, and closes the head: whatever comes next belongs after the marker.
    fn drop_bytes(&mut self, bytes: u64) {
        if bytes > 0 {
            self.dropped_bytes += bytes;
            self.head_closed = true;
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

    #[test]
    fn nf_escape_sequences_are_removed_whole() {
        // `tput sgr0` on xterm is `ESC ( B` then `ESC [ m`: the `B` is part of the first.
        assert_eq!(normalize_line(b"\x1b[1mbold\x1b(B\x1b[m done"), "bold done");
        assert_eq!(normalize_line(b"a\x1b#8b\x1b%Gc"), "abc");
        assert_eq!(normalize_line(b"a\x1b Fb\x1b$)Cc"), "abc");
    }

    #[test]
    fn an_nf_sequence_cut_short_shows_nothing_of_itself() {
        assert_eq!(normalize_line(b"a\x1b("), "a");
        assert_eq!(normalize_line(b"a\x1b($"), "a");
        assert_eq!(normalize_line(b"a\x1b(\x07b"), "ab");
    }

    #[test]
    fn device_control_and_application_strings_are_removed_with_their_payload() {
        // Sixel (DCS), kitty graphics (APC), then PM and SOS.
        assert_eq!(normalize_line(b"a\x1bPq#0;2;0;0;0\x1b\\b"), "ab");
        assert_eq!(normalize_line(b"a\x1b_Gf=100;AAAA\x1b\\b"), "ab");
        assert_eq!(normalize_line(b"a\x1b^private\x1b\\b\x1bXtext\x07c"), "abc");
        assert_eq!(normalize_line(b"ok\x1bPq never ends"), "ok");
        assert_eq!(normalize_line(b"a\x1b_x\x1b[31mb"), "ab");
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
        // The head is HEAD_BYTES of lines, each charged for its index entry too.
        let head_len: usize = lines[..at].iter().map(|l| l.len() + 1).sum();
        assert!(head_len <= HEAD_BYTES && head_len * 2 > HEAD_BYTES, "head is {head_len}");
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

    fn real_lines(count: usize) -> ShellOutput {
        let mut o = ShellOutput::default();
        for i in 0..count {
            o.push(format!("real line {i:05}\n").as_bytes());
        }
        o
    }

    #[test]
    fn a_long_redraw_on_an_unfinished_line_evicts_nothing_and_adds_no_marker() {
        // A progress bar: the line so far is replaced again and again, and costs nothing.
        let mut o = real_lines(20_000);
        let before = o.kept_lines();
        for i in 0..100_000 {
            o.push(format!("\rprogress {i:06}").as_bytes());
        }
        assert_eq!(o.kept_lines(), before + 1);
        assert!(!o.text().contains("not kept"));
        assert!(o.text().contains("real line 19999"));
        assert!(o.text().ends_with("progress 099999"));
        // The same with the `\r` at the end of each read, then a CRLF.
        let mut o = real_lines(20_000);
        for i in 0..100_000 {
            o.push(format!("progress {i:06}\r").as_bytes());
        }
        assert_eq!(o.kept_lines(), before + 1);
        assert!(!o.text().contains("not kept"));
        o.push(b"\ndone\n");
        assert!(o.text().contains("real line 19999\nprogress 099999\ndone"));
        assert_eq!(o.total_lines(), 20_002);
    }

    #[test]
    fn redraws_between_lines_leave_no_marker_and_the_lines_in_order() {
        let mut o = ShellOutput::default();
        o.push(b"a\n");
        for i in 0..200_000 {
            o.push(format!("\r{i:06}%").as_bytes());
        }
        o.push(b"\nb\n");
        assert_eq!(o.text(), "a\n199999%\nb");
        assert_eq!(o.total_lines(), 3);
    }

    #[test]
    fn a_redraw_that_arrives_late_drops_the_collapsed_line_without_a_marker() {
        let mut o = ShellOutput::default();
        o.push(b"a\n");
        for _ in 0..2 * 1024 {
            o.push(&[b'j'; 1024]);
        }
        o.push(b"\rok\nb\n");
        assert_eq!(o.text(), "a\nok\nb");
        assert_eq!(o.total_lines(), 3);
        // The `\r` and the text after it in reads of their own.
        let mut o = ShellOutput::default();
        o.push(b"a\n");
        for _ in 0..2 * 1024 {
            o.push(&[b'j'; 1024]);
        }
        o.push(b"\r");
        o.push(b"ok\nb\n");
        assert_eq!(o.text(), "a\nok\nb");
    }

    #[test]
    fn the_marker_never_comes_after_a_line_that_arrived_later() {
        // The unfinished line is cut and ends as a line that shows nothing: it would fit the head,
        // but lines were dropped before it, so it and every later line belong after the marker.
        let mut o = ShellOutput::default();
        o.push(b"a\n");
        let junk = b"\x1b[0m".repeat(1024);
        for _ in 0..600 {
            o.push(&junk);
        }
        o.push(b"\nb\nc\n");
        let text = o.text();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[0], "a");
        assert!(lines[1].contains("not kept"), "{lines:?}");
        assert_eq!(&lines[lines.len() - 2..], ["b", "c"]);
        // A long line that is cut and then pushed out by the next one.
        let mut o = ShellOutput::default();
        o.push(b"a\n");
        for _ in 0..2 * 1024 {
            o.push(&[b'j'; 1024]);
        }
        o.push(b"\nb\n");
        let text = o.text();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[0], "a");
        assert!(lines[1].contains("not kept"));
        assert_eq!(lines[2], "b");
    }

    #[test]
    fn a_flood_of_tiny_lines_keeps_a_bounded_number_of_them() {
        // A kept line costs memory beyond its text, so the cap cannot be met by a million of them.
        let mut o = ShellOutput::default();
        let yes = b"y\n".repeat(64 * 1024);
        for _ in 0..32 {
            o.push(&yes);
        }
        assert_eq!(o.total_lines(), 32 * 64 * 1024);
        assert!(o.kept_lines() <= KEEP_BYTES / 8, "{} lines", o.kept_lines());
        assert!(o.text().len() <= KEEP_BYTES);
        assert!(o.text().contains("not kept"));
        let mut o = ShellOutput::default();
        o.push(&b"\n".repeat(2 * 1024 * 1024));
        assert_eq!(o.total_lines(), 2 * 1024 * 1024);
        assert!(o.kept_lines() <= KEEP_BYTES / 8, "{} lines", o.kept_lines());
    }

    #[test]
    fn lines_of_every_length_come_back_intact_after_many_evictions() {
        let mut o = ShellOutput::default();
        for i in 0..150_000u32 {
            o.push(format!("{i}:{}\n", "x".repeat((i % 37) as usize)).as_bytes());
        }
        let text = o.text();
        let lines: Vec<&str> = text.split('\n').collect();
        let (mut last, mut gaps) = (None, 0);
        for line in lines.iter().filter(|l| !l.contains("not kept")) {
            let (n, xs) = line.split_once(':').unwrap();
            let n: u32 = n.parse().unwrap();
            assert_eq!(xs.len(), (n % 37) as usize, "line {n} is whole");
            if last.is_some_and(|p| n != p + 1) {
                gaps += 1;
            }
            last = Some(n);
        }
        assert_eq!(gaps, 1);
        assert_eq!(last, Some(149_999));
        assert_eq!(o.tail(5), lines[lines.len() - 5..].join("\n"));
        assert_eq!(o.kept_lines(), lines.len());
    }

    // ---- ShellOutput: the memory held, not only the text kept --------------

    /// Room the buffers may keep empty at any size.
    const CAPACITY_SLACK: usize = 64 * 1024;

    /// Feeds the chunks, then ends the output. The buffers must hold at most 1.5 x the cap while the
    /// command runs, and the cap once it has ended.
    fn assert_memory_is_bounded(shape: &str, chunks: impl IntoIterator<Item = Vec<u8>>) {
        let mut o = ShellOutput::default();
        let mut worst = 0;
        for chunk in chunks {
            o.push(&chunk);
            worst = worst.max(o.capacity_bytes());
        }
        assert!(
            worst <= KEEP_BYTES * 3 / 2 + CAPACITY_SLACK,
            "{shape}: {worst} bytes held while running"
        );
        o.finish();
        assert!(
            o.capacity_bytes() <= KEEP_BYTES + CAPACITY_SLACK,
            "{shape}: {} bytes held after finish",
            o.capacity_bytes()
        );
        // The text is still there: bounding the memory did not cut the output.
        // (A line is kept whole or not at all, so two lines of 500 KB keep only one of them.)
        assert!(o.text().len() > KEEP_BYTES / 4, "{shape}: kept {} bytes", o.text().len());
    }

    fn lines_of(len: usize, count: usize) -> impl Iterator<Item = Vec<u8>> {
        (0..count).map(move |_| {
            let mut line = vec![b'k'; len];
            line.push(b'\n');
            line
        })
    }

    #[test]
    fn memory_is_bounded_for_one_kilobyte_lines() {
        assert_memory_is_bounded("1 KB lines", lines_of(1024, 6_000));
    }

    #[test]
    fn memory_is_bounded_for_twenty_kilobyte_lines() {
        assert_memory_is_bounded("20 KB lines", lines_of(20 * 1024, 300));
    }

    #[test]
    fn memory_is_bounded_for_lines_a_third_and_a_half_of_the_cap() {
        assert_memory_is_bounded("300 KB lines", lines_of(300 * 1024, 20));
        assert_memory_is_bounded("500 KB lines", lines_of(500 * 1024, 12));
    }

    #[test]
    fn memory_is_bounded_for_one_endless_unfinished_line() {
        let chunks = (0..2 * 1024).map(|_| vec![b'z'; 1024]);
        assert_memory_is_bounded("one 2 MiB unfinished line", chunks);
    }

    #[test]
    fn memory_is_bounded_for_small_lines_then_one_almost_as_big_as_the_cap() {
        let chunks = lines_of(20 * 1024, 100).chain(lines_of(950 * 1024, 1));
        assert_memory_is_bounded("20 KB lines then a 950 KB line", chunks);
    }

    #[test]
    fn memory_is_bounded_after_a_flood_of_tiny_lines_gave_way_to_big_ones() {
        let flood = (0..32).map(|_| b"y\n".repeat(64 * 1024));
        let chunks = flood
            .chain(lines_of(20 * 1024, 100))
            .chain(lines_of(950 * 1024, 1));
        assert_memory_is_bounded("tiny flood then 20 KB lines then a 950 KB line", chunks);
    }

    #[test]
    fn memory_is_bounded_when_big_lines_give_way_to_an_endless_one() {
        // The tail's buffers are released as the unfinished line takes its share of the cap.
        let chunks = lines_of(1024, 1_500)
            .chain((0..1024).map(|_| vec![b'z'; 1024]));
        assert_memory_is_bounded("1 KB lines then a 1 MiB unfinished line", chunks);
    }

    #[test]
    fn a_restored_block_holds_no_more_than_its_text() {
        let text = (0..3_000).map(|i| format!("{i:04} {}", "r".repeat(300))).collect::<Vec<_>>().join("\n");
        let o = ShellOutput::from_text(&text, 3_000);
        assert!(o.capacity_bytes() <= o.text().len() + o.kept_lines() * 8 + CAPACITY_SLACK / 4);
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

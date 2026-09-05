//! A minimal VT screen model for the Windows usage probe.
//!
//! ConPTY does not hand its reader the bytes `claude` wrote. conhost owns
//! the screen and re-emits it to the reader as a *diff*: cursor positioning
//! (`CSI r;c H`), cursor-forward in place of runs of spaces (`CSI n C`),
//! erase sequences, and only the cells that changed. Read as a text
//! stream, that diff has words glued together, lines out of their visual
//! order, and every repaint of the panel appended after the last one
//! (confirmed live: `Accessingworkspace:…` for the trust dialog, and a
//! `/usage` panel whose `Resets` lines sat several lines away from their
//! percents). The parser in `claude.rs` was written against a text stream —
//! the Unix pty hands over exactly what the TUI wrote — so the Windows probe
//! replays the diff into this grid and hands the parser the screen as it
//! currently reads.
//!
//! Deliberately small: enough of ECMA-48 to render Ink output through
//! conhost. Everything unrecognised is consumed and ignored. Character
//! widths are not modelled (a wide glyph takes one cell here, two in
//! conhost), which can shift a line's later cells by a column — harmless to
//! a parser that strips whitespace before matching.

/// Scrolled-off rows kept for the parser, oldest dropped first.
const MAX_SCROLLBACK_ROWS: usize = 2_000;

/// The unread tail of a `feed` that ended inside a UTF-8 sequence is at
/// most this long; anything longer is malformed and flushed.
const MAX_PENDING_UTF8: usize = 4;

/// A fixed-size character grid with a cursor, plus the rows that scrolled
/// off its top.
pub struct Screen {
    rows: usize,
    cols: usize,
    grid: Vec<Vec<char>>,
    scrollback: Vec<String>,
    row: usize,
    col: usize,
    saved_cursor: Option<(usize, usize)>,
    state: State,
    pending: Vec<u8>,
}

enum State {
    Ground,
    Escape,
    /// An `ESC` intermediate (`ESC (`, `ESC )`, …) whose one-byte argument
    /// is still to come.
    EscapeIntermediate,
    Csi {
        params: String,
        intermediates: String,
    },
    Osc {
        escape_seen: bool,
    },
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            grid: (0..rows).map(|_| vec![' '; cols]).collect(),
            scrollback: Vec::new(),
            row: 0,
            col: 0,
            saved_cursor: None,
            state: State::Ground,
            pending: Vec::new(),
        }
    }

    /// Replays one chunk of pty output. Chunks may end anywhere — inside a
    /// UTF-8 sequence or an escape sequence — and the split is carried to
    /// the next call.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut input = std::mem::take(&mut self.pending);
        input.extend_from_slice(bytes);
        let mut rest = input.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    text.chars().for_each(|ch| self.step(ch));
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // SAFETY-FREE: `valid_up_to` guarantees this prefix is UTF-8.
                    let text = std::str::from_utf8(&rest[..valid]).unwrap_or_default();
                    text.chars().for_each(|ch| self.step(ch));
                    rest = &rest[valid..];
                    match error.error_len() {
                        // Cut mid-sequence: keep the tail for the next feed.
                        None if rest.len() <= MAX_PENDING_UTF8 => {
                            self.pending = rest.to_vec();
                            break;
                        }
                        None => {
                            self.step('\u{fffd}');
                            break;
                        }
                        Some(bad) => {
                            self.step('\u{fffd}');
                            rest = &rest[bad..];
                        }
                    }
                }
            }
        }
    }

    /// The screen as it currently reads: scrolled-off rows first, then the
    /// grid, each row trimmed at its end, trailing blank grid rows dropped.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = self.scrollback.clone();
        lines.extend(self.grid.iter().map(|row| row_text(row)));
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    fn step(&mut self, ch: char) {
        match std::mem::replace(&mut self.state, State::Ground) {
            State::Ground => self.ground(ch),
            State::Escape => self.escape(ch),
            State::EscapeIntermediate => {}
            State::Csi {
                mut params,
                mut intermediates,
            } => match ch {
                '0'..='?' => {
                    params.push(ch);
                    self.state = State::Csi {
                        params,
                        intermediates,
                    };
                }
                ' '..='/' => {
                    intermediates.push(ch);
                    self.state = State::Csi {
                        params,
                        intermediates,
                    };
                }
                '@'..='~' => self.csi(&params, &intermediates, ch),
                // A control character inside a sequence is executed, the
                // sequence abandoned — the usual terminal behaviour.
                _ => self.ground(ch),
            },
            State::Osc { escape_seen } => match ch {
                '\u{7}' => {}
                '\\' if escape_seen => {}
                '\u{1b}' => self.state = State::Osc { escape_seen: true },
                _ if escape_seen => self.escape(ch),
                _ => self.state = State::Osc { escape_seen: false },
            },
        }
    }

    fn ground(&mut self, ch: char) {
        match ch {
            '\u{1b}' => self.state = State::Escape,
            '\r' => self.col = 0,
            '\n' | '\u{b}' | '\u{c}' => self.linefeed(),
            '\u{8}' => self.col = self.col.saturating_sub(1),
            '\t' => self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1),
            ch if ch.is_control() => {}
            ch => self.put(ch),
        }
    }

    fn escape(&mut self, ch: char) {
        match ch {
            '[' => {
                self.state = State::Csi {
                    params: String::new(),
                    intermediates: String::new(),
                }
            }
            ']' => self.state = State::Osc { escape_seen: false },
            '7' => self.saved_cursor = Some((self.row, self.col)),
            '8' => self.restore_cursor(),
            'D' => self.linefeed(),
            'E' => {
                self.col = 0;
                self.linefeed();
            }
            'M' => self.row = self.row.saturating_sub(1),
            'c' => *self = Screen::new(self.rows, self.cols),
            ' '..='/' => self.state = State::EscapeIntermediate,
            _ => {}
        }
    }

    fn csi(&mut self, params: &str, intermediates: &str, final_byte: char) {
        // DEC private and vendor sequences (`?25l`, `?2004h`, `>4;2m`,
        // `<u`, …) never move text; the probe has no modes to keep.
        if params.starts_with(['?', '>', '<', '=']) || !intermediates.is_empty() {
            return;
        }
        let values: Vec<usize> = params
            .split(';')
            .map(|value| value.parse().unwrap_or(0))
            .collect();
        let arg = |index: usize, default: usize| match values.get(index) {
            Some(0) | None => default,
            Some(value) => *value,
        };
        let last_row = self.rows - 1;
        let last_col = self.cols - 1;
        match final_byte {
            'H' | 'f' => {
                self.row = (arg(0, 1) - 1).min(last_row);
                self.col = (arg(1, 1) - 1).min(last_col);
            }
            'A' => self.row = self.row.saturating_sub(arg(0, 1)),
            'B' => self.row = (self.row + arg(0, 1)).min(last_row),
            'C' => self.col = (self.col + arg(0, 1)).min(last_col),
            'D' => self.col = self.col.saturating_sub(arg(0, 1)),
            'E' => {
                self.col = 0;
                self.row = (self.row + arg(0, 1)).min(last_row);
            }
            'F' => {
                self.col = 0;
                self.row = self.row.saturating_sub(arg(0, 1));
            }
            'G' => self.col = (arg(0, 1) - 1).min(last_col),
            'd' => self.row = (arg(0, 1) - 1).min(last_row),
            'J' => match values.first().copied().unwrap_or(0) {
                0 => {
                    self.clear_line_from(self.col);
                    for row in self.row + 1..self.rows {
                        self.grid[row].fill(' ');
                    }
                }
                1 => {
                    for row in 0..self.row {
                        self.grid[row].fill(' ');
                    }
                    self.clear_line_to(self.col);
                }
                2 => self.grid.iter_mut().for_each(|row| row.fill(' ')),
                _ => {
                    self.grid.iter_mut().for_each(|row| row.fill(' '));
                    self.scrollback.clear();
                }
            },
            'K' => match values.first().copied().unwrap_or(0) {
                0 => self.clear_line_from(self.col),
                1 => self.clear_line_to(self.col),
                _ => self.grid[self.row].fill(' '),
            },
            'X' => {
                let end = (self.col + arg(0, 1)).min(self.cols);
                self.grid[self.row][self.col..end].fill(' ');
            }
            'P' => {
                let count = arg(0, 1).min(self.cols - self.col);
                let line = &mut self.grid[self.row];
                line.drain(self.col..self.col + count);
                line.extend(std::iter::repeat_n(' ', count));
            }
            '@' => {
                let count = arg(0, 1).min(self.cols - self.col);
                let line = &mut self.grid[self.row];
                line.truncate(self.cols - count);
                line.splice(self.col..self.col, std::iter::repeat_n(' ', count));
            }
            'L' => {
                for _ in 0..arg(0, 1).min(self.rows - self.row) {
                    self.grid.pop();
                    self.grid.insert(self.row, vec![' '; self.cols]);
                }
            }
            'M' => {
                for _ in 0..arg(0, 1).min(self.rows - self.row) {
                    self.grid.remove(self.row);
                    self.grid.push(vec![' '; self.cols]);
                }
            }
            'S' => {
                for _ in 0..arg(0, 1) {
                    self.scroll_up();
                }
            }
            'T' => {
                for _ in 0..arg(0, 1).min(self.rows) {
                    self.grid.pop();
                    self.grid.insert(0, vec![' '; self.cols]);
                }
            }
            's' => self.saved_cursor = Some((self.row, self.col)),
            'u' => self.restore_cursor(),
            _ => {}
        }
    }

    fn put(&mut self, ch: char) {
        if self.col >= self.cols {
            self.col = 0;
            self.linefeed();
        }
        self.grid[self.row][self.col] = ch;
        self.col += 1;
    }

    fn linefeed(&mut self) {
        if self.row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.row += 1;
        }
    }

    fn scroll_up(&mut self) {
        let top = self.grid.remove(0);
        self.scrollback.push(row_text(&top));
        if self.scrollback.len() > MAX_SCROLLBACK_ROWS {
            self.scrollback.remove(0);
        }
        self.grid.push(vec![' '; self.cols]);
    }

    fn restore_cursor(&mut self) {
        if let Some((row, col)) = self.saved_cursor {
            self.row = row.min(self.rows - 1);
            self.col = col.min(self.cols - 1);
        }
    }

    fn clear_line_from(&mut self, col: usize) {
        self.grid[self.row][col..].fill(' ');
    }

    fn clear_line_to(&mut self, col: usize) {
        let end = (col + 1).min(self.cols);
        self.grid[self.row][..end].fill(' ');
    }
}

fn row_text(row: &[char]) -> String {
    row.iter().collect::<String>().trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(rows: usize, cols: usize, chunks: &[&[u8]]) -> String {
        let mut screen = Screen::new(rows, cols);
        for chunk in chunks {
            screen.feed(chunk);
        }
        screen.text()
    }

    #[test]
    fn plain_text_with_crlf_forms_lines() {
        assert_eq!(render(5, 20, &[b"one\r\ntwo\r\n"]), "one\ntwo");
    }

    #[test]
    fn cursor_positioning_and_forward_place_text_where_conhost_put_it() {
        let text = render(5, 20, &[b"\x1b[3;2HHi\x1b[1Cthere"]);
        assert_eq!(text, "\n\n Hi there");
    }

    #[test]
    fn erase_to_end_and_rewrite_replace_the_line() {
        assert_eq!(render(3, 20, &[b"old text\r\x1b[Knew"]), "new");
        assert_eq!(render(3, 20, &[b"old text\x1b[3D\x1b[K"]), "old t");
    }

    #[test]
    fn cursor_up_repaint_overwrites_earlier_rows() {
        let text = render(5, 20, &[b"line1\r\nline2\r\n\x1b[2A\x1b[2KX"]);
        assert_eq!(text, "X\nline2");
    }

    #[test]
    fn scrolling_at_the_bottom_keeps_the_rows_that_left() {
        assert_eq!(render(2, 20, &[b"a\r\nb\r\nc"]), "a\nb\nc");
    }

    #[test]
    fn private_modes_sgr_osc_and_queries_leave_no_text() {
        let text = render(
            3,
            20,
            &[b"\x1b[?25l\x1b[38;2;1;2;3m\x1b]0;title\x07\x1b]8;;http://x\x1b\\\x1b[6n\x1b[>4;2m\x1b[<uA"],
        );
        assert_eq!(text, "A");
    }

    #[test]
    fn a_sequence_split_across_feeds_is_still_one_sequence() {
        assert_eq!(render(3, 20, &[b"\x1b[", b"2;3H", b"X"]), "\n  X");
    }

    #[test]
    fn incomplete_utf8_across_feeds_is_joined() {
        assert_eq!(render(3, 20, &[b"\xE2\x94", b"\x80"]), "\u{2500}");
        assert_eq!(render(3, 20, &[b"a\xFFb"]), "a\u{fffd}b");
    }

    /// The trust dialog exactly as conhost emitted it (from the live dump
    /// that found it): two options on their own rows, spaces restored.
    #[test]
    fn a_conpty_dialog_diff_renders_its_options_on_their_rows() {
        let diff: &[u8] = "\x1b[14;2H❯\x1b[1CNo,\x1b[1Cexit\x1b[m\x1b[15;4HYes,\x1b[1CI\x1b[1Ctrust\x1b[1Cthis\x1b[1Cfolder\x1b[38;2;153;153;153m\x1b[17;2HEnter\x1b[1Cto\x1b[1Cconfirm"
            .as_bytes();
        let text = render(40, 120, &[diff]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[13], " ❯ No, exit");
        assert_eq!(lines[14], "   Yes, I trust this folder");
        assert_eq!(lines[16], " Enter to confirm");
    }
}

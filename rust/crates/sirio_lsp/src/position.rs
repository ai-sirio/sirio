//! Byte offsets on one side, LSP positions on the other.
//!
//! LSP counts `character` in **UTF-16 code units**, not bytes and not
//! characters: an accented letter is 2 UTF-8 bytes but 1 unit, an emoji is
//! 4 bytes but 2 units (a surrogate pair). Keeping that arithmetic in this
//! one type is why `sirio_ui` gets to speak plain byte offsets — every
//! surface that re-derived it would derive it differently.

use lsp_types::Position;

/// A document's text plus the byte offset each of its lines starts at.
///
/// Owns the text because both directions need it: counting UTF-16 units
/// requires reading the bytes, not just knowing where lines begin.
#[derive(Debug, Clone)]
pub struct LineIndex {
    text: String,
    /// Ascending, always non-empty — an empty document is one empty line
    /// starting at 0, which is what makes `position(0)` well defined.
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self { text, line_starts }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The position of a byte offset. Offsets past the end clamp to the end,
    /// and an offset landing inside a multi-byte character floors to that
    /// character's first byte rather than panicking on a slice boundary.
    pub fn position(&self, offset: usize) -> Position {
        let offset = self.floor_char_boundary(offset.min(self.text.len()));
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let character = self.text[line_start..offset].encode_utf16().count();
        Position {
            line: line as u32,
            character: character as u32,
        }
    }

    fn floor_char_boundary(&self, offset: usize) -> usize {
        let mut offset = offset;
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// The byte offset of a position, or `None` when the line does not
    /// exist. A `character` past the line's content clamps to the line's
    /// end — servers do send such positions, and a hard error there would
    /// turn a harmless off-by-one into a dead feature.
    pub fn offset(&self, position: Position) -> Option<usize> {
        let line_start = *self.line_starts.get(position.line as usize)?;
        let line_end = self
            .line_starts
            .get(position.line as usize + 1).copied()
            .unwrap_or(self.text.len());
        // The terminator is not part of the line's content: a position can
        // address the place after the last character, never the newline.
        let content = self.text[line_start..line_end]
            .trim_end_matches('\n')
            .trim_end_matches('\r');

        let mut units = 0u32;
        for (byte_offset, character) in content.char_indices() {
            if units >= position.character {
                return Some(line_start + byte_offset);
            }
            units += character.len_utf16() as u32;
        }
        Some(line_start + content.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_in_the_first_line_maps_to_line_zero() {
        let index = LineIndex::new("fn main() {}\nlet x = 1;\n");
        assert_eq!(index.position(3), Position { line: 0, character: 3 });
    }

    #[test]
    fn an_offset_in_a_later_line_counts_from_that_lines_start() {
        let index = LineIndex::new("fn main() {}\nlet x = 1;\n");
        // byte 13 is the `e` of `let`, one past that line's start (13).
        assert_eq!(index.position(14), Position { line: 1, character: 1 });
    }

    #[test]
    fn an_empty_document_still_has_one_line() {
        let index = LineIndex::new("");
        assert_eq!(index.position(0), Position { line: 0, character: 0 });
    }

    #[test]
    fn an_accented_letter_is_two_bytes_but_one_utf16_unit() {
        // "città" — the à is 2 UTF-8 bytes (0xC3 0xA0), 1 UTF-16 unit.
        let index = LineIndex::new("città x");
        // byte 6 is the space: 4 ASCII + 2 bytes of à.
        assert_eq!(index.position(6), Position { line: 0, character: 5 });
    }

    #[test]
    fn an_emoji_is_four_bytes_but_two_utf16_units() {
        // "a🦀b" — the crab is 4 UTF-8 bytes and a surrogate pair in UTF-16.
        let index = LineIndex::new("a🦀b");
        assert_eq!(index.position(5), Position { line: 0, character: 3 });
    }

    #[test]
    fn an_offset_inside_a_multibyte_character_floors_to_its_start() {
        let index = LineIndex::new("a🦀b");
        // Bytes 2, 3 and 4 are inside the crab; all floor to its first byte.
        assert_eq!(index.position(3), index.position(1));
    }

    #[test]
    fn crlf_does_not_add_a_line() {
        let index = LineIndex::new("one\r\ntwo\r\n");
        assert_eq!(index.position(5), Position { line: 1, character: 0 });
    }

    #[test]
    fn a_position_maps_back_to_its_byte_offset() {
        let index = LineIndex::new("fn main() {}\nlet x = 1;\n");
        assert_eq!(index.offset(Position { line: 1, character: 1 }), Some(14));
    }

    #[test]
    fn a_position_round_trips_through_an_emoji() {
        let index = LineIndex::new("a🦀b\nsecond\n");
        let position = index.position(5);
        assert_eq!(index.offset(position), Some(5));
    }

    #[test]
    fn a_character_past_the_end_of_a_line_clamps_to_the_line_end() {
        let index = LineIndex::new("ab\ncd\n");
        // Line 0 holds 2 characters; 99 clamps to just before the newline.
        assert_eq!(index.offset(Position { line: 0, character: 99 }), Some(2));
    }

    #[test]
    fn a_line_past_the_end_of_the_document_is_none() {
        let index = LineIndex::new("ab\n");
        assert_eq!(index.offset(Position { line: 42, character: 0 }), None);
    }
}

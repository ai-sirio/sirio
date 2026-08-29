//! The composer's document model: text runs and chips as one atomic edit
//! surface.
//!
//! Mirrors the reference app's `ComposerDocument`: a chip occupies one
//! indivisible position in the draft — caret movement, backspace, and
//! selection treat it as a single unit — and the draft serializes into the
//! exact triple `ChatPromptBuilder` consumes (trimmed text, mention paths,
//! image attachments).

use sirio_acp::ImageAttachment;

/// A composer token rendered as an inline chip. Like the reference app's
/// `U+FFFC` attachment, one chip is one atomic position in the draft.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ComposerChip {
    /// A slash-command token; serializes as the `/name ` prefix on send.
    Skill { name: String },
    /// A `@`-mention; serializes as a separate mention path, not text.
    File { path: String },
    /// An attached image; serializes as an image block, not text.
    Image { mime: String, base64: String },
}

impl ComposerChip {
    pub(crate) fn label(&self) -> String {
        match self {
            ComposerChip::Skill { name } => name.clone(),
            ComposerChip::File { path } => {
                let file_name = path.rsplit('/').next().unwrap_or(path);
                file_name.to_string()
            }
            ComposerChip::Image { .. } => "Image".to_string(),
        }
    }
}

/// One editable element of the draft.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ComposerPart {
    Text(String),
    Chip(ComposerChip),
}

/// A caret position: `(part index, char offset within that Text part)`.
/// On a chip part the offset is always 0 (the caret sits before the chip);
/// `(parts.len(), 0)` is the end of the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Cursor {
    pub(crate) part: usize,
    pub(crate) offset: usize,
}

/// What the composer hands to the ACP layer: the same triple the reference
/// app's `ComposerDraft` produces.
pub(crate) struct ComposerDraft {
    pub(crate) text: String,
    pub(crate) mention_paths: Vec<String>,
    pub(crate) images: Vec<ImageAttachment>,
}

/// The composer's draft state: parts, caret, and selection.
pub(crate) struct Composer {
    parts: Vec<ComposerPart>,
    cursor: Cursor,
    selection_anchor: Option<Cursor>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    pub(crate) fn new() -> Self {
        Self {
            parts: Vec::new(),
            cursor: Cursor::default(),
            selection_anchor: None,
        }
    }

    pub(crate) fn parts(&self) -> &[ComposerPart] {
        &self.parts
    }

    /// Where the caret sits: `(part index, char offset in that part)`, with
    /// `part == parts.len()` meaning end-of-document. Read by the renderer
    /// to place the blinking bar between the draft's inline parts.
    pub(crate) fn cursor(&self) -> (usize, usize) {
        (self.cursor.part, self.cursor.offset)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The plain text runs, chips omitted — what a placeholder check or a
    /// text-only consumer sees.
    pub(crate) fn text(&self) -> String {
        let mut text = String::new();
        for part in &self.parts {
            if let ComposerPart::Text(run) = part {
                text.push_str(run);
            }
        }
        text
    }

    /// The caret's position in the chip-inclusive document (a chip is one
    /// position). Test-only today: production editing goes through the
    /// cursor-aware operations.
    #[cfg(test)]
    pub(crate) fn doc_position(&self) -> usize {
        self.doc_position_of(self.cursor)
    }

    fn part_len(part: &ComposerPart) -> usize {
        match part {
            ComposerPart::Text(text) => text.chars().count(),
            ComposerPart::Chip(_) => 1,
        }
    }

    fn doc_position_of(&self, cursor: Cursor) -> usize {
        let mut position = self.parts[..cursor.part]
            .iter()
            .map(Self::part_len)
            .sum::<usize>();
        if cursor.part < self.parts.len()
            && matches!(self.parts[cursor.part], ComposerPart::Text(_))
        {
            position += cursor.offset;
        }
        position
    }

    /// Resolves a chip-inclusive document position back to a caret. Positions
    /// on a chip resolve to before the chip; the end resolves past the last
    /// part.
    fn cursor_at_doc_pos(&self, position: usize) -> Cursor {
        let mut remaining = position;
        for (index, part) in self.parts.iter().enumerate() {
            let length = Self::part_len(part);
            if remaining < length {
                return match part {
                    ComposerPart::Text(_) => Cursor {
                        part: index,
                        offset: remaining,
                    },
                    ComposerPart::Chip(_) => Cursor {
                        part: index,
                        offset: 0,
                    },
                };
            }
            remaining -= length;
        }
        Cursor {
            part: self.parts.len(),
            offset: 0,
        }
    }

    /// Merges adjacent text runs, drops empty ones, and re-resolves the
    /// caret and selection anchor against the folded parts.
    fn normalize(&mut self) {
        let cursor_position = self.doc_position_of(self.cursor);
        let anchor_position = self
            .selection_anchor
            .map(|anchor| self.doc_position_of(anchor));
        let mut merged: Vec<ComposerPart> = Vec::with_capacity(self.parts.len());
        for part in self.parts.drain(..) {
            match part {
                ComposerPart::Text(text) if text.is_empty() => {}
                ComposerPart::Text(text) => {
                    if let Some(ComposerPart::Text(previous)) = merged.last_mut() {
                        previous.push_str(&text);
                    } else {
                        merged.push(ComposerPart::Text(text));
                    }
                }
                chip => merged.push(chip),
            }
        }
        self.parts = merged;
        let end = self.doc_position_of(Cursor {
            part: self.parts.len(),
            offset: 0,
        });
        self.cursor = self.cursor_at_doc_pos(cursor_position.min(end));
        self.selection_anchor =
            anchor_position.map(|position| self.cursor_at_doc_pos(position.min(end)));
    }

    /// The caret position before the current one, in the chip-inclusive
    /// document. Doc-position based: `(part, len)` and `(part+1, 0)` are the
    /// same position, and only the doc arithmetic can tell them apart.
    fn previous_pos(&self) -> Option<Cursor> {
        let position = self.doc_position_of(self.cursor);
        if position == 0 {
            return None;
        }
        Some(self.cursor_at_doc_pos(position - 1))
    }

    fn next_pos(&self) -> Option<Cursor> {
        let end = self.doc_position_of(Cursor {
            part: self.parts.len(),
            offset: 0,
        });
        let position = self.doc_position_of(self.cursor);
        if position >= end {
            return None;
        }
        Some(self.cursor_at_doc_pos(position + 1))
    }

    pub(crate) fn insert_text(&mut self, text: &str) {
        self.delete_selected();
        if text.is_empty() {
            return;
        }
        let insertion = self.doc_position_of(self.cursor);
        if self.cursor.part < self.parts.len()
            && matches!(self.parts[self.cursor.part], ComposerPart::Chip(_))
        {
            self.parts
                .insert(self.cursor.part, ComposerPart::Text(text.to_string()));
        } else if self.cursor.part < self.parts.len() {
            if let ComposerPart::Text(existing) = &mut self.parts[self.cursor.part] {
                existing.insert_str(self.cursor.offset, text);
            }
        } else {
            self.parts.push(ComposerPart::Text(text.to_string()));
        }
        let inserted = text.chars().count();
        self.normalize();
        self.cursor = self.cursor_at_doc_pos(insertion + inserted);
    }

    pub(crate) fn insert_chip_at_cursor(&mut self, chip: ComposerChip) {
        self.delete_selected();
        let insertion = self.doc_position_of(self.cursor);
        self.parts.insert(
            self.cursor.part.min(self.parts.len()),
            ComposerPart::Chip(chip),
        );
        self.cursor = self.cursor_at_doc_pos(insertion + 1);
    }

    pub(crate) fn remove_chip(&mut self, part_index: usize) {
        if !matches!(self.parts.get(part_index), Some(ComposerPart::Chip(_))) {
            return;
        }
        let cursor_position = self.doc_position_of(self.cursor);
        let removed_position = self.doc_position_of(Cursor {
            part: part_index,
            offset: 0,
        });
        self.parts.remove(part_index);
        self.cursor = if cursor_position > removed_position {
            self.cursor_at_doc_pos(cursor_position - 1)
        } else {
            self.cursor_at_doc_pos(cursor_position)
        };
        self.normalize();
    }

    pub(crate) fn backspace(&mut self) {
        if self.delete_selected() {
            return;
        }
        if let Some(previous) = self.previous_pos() {
            self.delete_range(previous, self.cursor);
        }
    }

    pub(crate) fn delete_forward(&mut self) {
        if self.delete_selected() {
            return;
        }
        if let Some(next) = self.next_pos() {
            self.delete_range(self.cursor, next);
        }
    }

    pub(crate) fn move_left(&mut self, extend_selection: bool) {
        if !extend_selection && let Some(range) = self.selected_range() {
            self.cursor = range.0;
            self.selection_anchor = None;
            return;
        }
        let target = self.previous_pos().unwrap_or(self.cursor);
        self.move_cursor(target, extend_selection);
    }

    pub(crate) fn move_right(&mut self, extend_selection: bool) {
        if !extend_selection && let Some(range) = self.selected_range() {
            self.cursor = range.1;
            self.selection_anchor = None;
            return;
        }
        let target = self.next_pos().unwrap_or(self.cursor);
        self.move_cursor(target, extend_selection);
    }

    pub(crate) fn move_home(&mut self, extend_selection: bool) {
        self.move_cursor(Cursor::default(), extend_selection);
    }

    pub(crate) fn move_end(&mut self, extend_selection: bool) {
        self.move_cursor(
            Cursor {
                part: self.parts.len(),
                offset: 0,
            },
            extend_selection,
        );
    }

    pub(crate) fn select_all(&mut self) {
        self.selection_anchor = Some(Cursor::default());
        self.cursor = Cursor {
            part: self.parts.len(),
            offset: 0,
        };
    }

    fn move_cursor(&mut self, cursor: Cursor, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = cursor;
    }

    pub(crate) fn selected_range(&self) -> Option<(Cursor, Cursor)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(
            if self.doc_position_of(anchor) < self.doc_position_of(self.cursor) {
                (anchor, self.cursor)
            } else {
                (self.cursor, anchor)
            },
        )
    }

    pub(crate) fn delete_selected(&mut self) -> bool {
        let Some((start, end)) = self.selected_range() else {
            return false;
        };
        self.delete_range(start, end);
        true
    }

    /// Deletes `[start, end)` in the chip-inclusive document. Chips are
    /// atomic: a range that covers a chip removes the whole chip.
    fn delete_range(&mut self, start: Cursor, end: Cursor) {
        if self.doc_position_of(start) == self.doc_position_of(end) {
            return;
        }
        let mut kept: Vec<ComposerPart> = Vec::new();
        kept.extend(self.parts[..start.part].iter().cloned());
        if start.part < self.parts.len()
            && start.offset > 0
            && let ComposerPart::Text(text) = &self.parts[start.part]
        {
            let prefix: String = text.chars().take(start.offset).collect();
            if !prefix.is_empty() {
                kept.push(ComposerPart::Text(prefix));
            }
        }
        if end.part < self.parts.len() {
            match &self.parts[end.part] {
                ComposerPart::Text(text) => {
                    if end.offset < text.chars().count() {
                        let suffix: String = text.chars().skip(end.offset).collect();
                        if !suffix.is_empty() {
                            kept.push(ComposerPart::Text(suffix));
                        }
                        kept.extend(self.parts[end.part + 1..].iter().cloned());
                    } else {
                        kept.extend(self.parts[end.part + 1..].iter().cloned());
                    }
                }
                ComposerPart::Chip(_) => {
                    // `end` sits before the chip: the chip survives.
                    kept.extend(self.parts[end.part..].iter().cloned());
                }
            }
        }
        self.parts = kept;
        self.cursor = start;
        self.normalize();
    }

    /// The active `/`-token: present when the whole draft is one unbroken
    /// text run starting with a slash, exactly like the reference
    /// `slashTokenRange` (whitespace anywhere ends the token).
    pub(crate) fn slash_token(&self) -> Option<String> {
        match self.parts.as_slice() {
            [ComposerPart::Text(text)]
                if text.starts_with('/') && !text.chars().any(char::is_whitespace) =>
            {
                Some(text[1..].to_string())
            }
            _ => None,
        }
    }

    /// Swaps the in-progress `/token` for a skill chip plus a trailing space,
    /// the same shape the reference `replaceSlashToken` builds.
    pub(crate) fn replace_slash_token(&mut self, name: &str) {
        if self.slash_token().is_none() {
            return;
        }
        self.parts = vec![
            ComposerPart::Chip(ComposerChip::Skill {
                name: name.to_string(),
            }),
            ComposerPart::Text(" ".to_string()),
        ];
        self.cursor = Cursor {
            part: self.parts.len(),
            offset: 0,
        };
        self.selection_anchor = None;
    }

    /// The active `@`-token: the text after the last `@`, with no whitespace
    /// inside, when the trailing part of the draft is still plain text.
    pub(crate) fn mention_token(&self) -> Option<String> {
        let ComposerPart::Text(text) = self.parts.last()? else {
            return None;
        };
        let at = text.rfind('@')?;
        let token = &text[at + 1..];
        if token.chars().any(char::is_whitespace) {
            return None;
        }
        Some(token.to_string())
    }

    /// Swaps the trailing `@token` for a file chip at the same position.
    pub(crate) fn accept_mention(&mut self, path: &str) {
        if self.mention_token().is_none() {
            return;
        }
        let ComposerPart::Text(text) = self.parts.pop().unwrap() else {
            unreachable!("mention token implies a trailing text part")
        };
        let at = text.rfind('@').unwrap();
        let before = &text[..at];
        if !before.is_empty() {
            self.parts.push(ComposerPart::Text(before.to_string()));
        }
        self.parts.push(ComposerPart::Chip(ComposerChip::File {
            path: path.to_string(),
        }));
        self.cursor = Cursor {
            part: self.parts.len(),
            offset: 0,
        };
        self.normalize();
    }

    /// Serializes the draft the way `ComposerDraft.parse` does: skill chips
    /// re-serialize as their `/name ` prefix, file chips join the mention
    /// paths (deduplicated), image chips join the attachments.
    pub(crate) fn draft(&self) -> ComposerDraft {
        let mut draft = ComposerDraft {
            text: String::new(),
            mention_paths: Vec::new(),
            images: Vec::new(),
        };
        for part in &self.parts {
            match part {
                ComposerPart::Text(text) => draft.text.push_str(text),
                ComposerPart::Chip(ComposerChip::Skill { name }) => {
                    draft.text.push('/');
                    draft.text.push_str(name);
                    draft.text.push(' ');
                }
                ComposerPart::Chip(ComposerChip::File { path }) => {
                    if !draft.mention_paths.contains(path) {
                        draft.mention_paths.push(path.clone());
                    }
                }
                ComposerPart::Chip(ComposerChip::Image { mime, base64 }) => {
                    draft.images.push(ImageAttachment {
                        mime_type: mime.clone(),
                        base64_data: base64.clone(),
                    });
                }
            }
        }
        draft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str) -> ComposerChip {
        ComposerChip::Skill {
            name: name.to_string(),
        }
    }

    fn file(path: &str) -> ComposerChip {
        ComposerChip::File {
            path: path.to_string(),
        }
    }

    fn image(mime: &str, data: &str) -> ComposerChip {
        ComposerChip::Image {
            mime: mime.to_string(),
            base64: data.to_string(),
        }
    }

    #[test]
    fn text_editing_behaves_like_the_previous_string_model() {
        let mut composer = Composer::new();
        composer.insert_text("hello");
        assert_eq!(composer.text(), "hello");

        composer.insert_text(" world");
        assert_eq!(composer.text(), "hello world");

        composer.backspace();
        composer.backspace();
        assert_eq!(composer.text(), "hello wor");

        composer.move_left(false);
        composer.move_left(false);
        composer.insert_text("X");
        assert_eq!(composer.text(), "hello wXor");

        // Shift-left selects the inserted character; delete removes the
        // selection.
        composer.move_left(true);
        composer.delete_forward();
        assert_eq!(composer.text(), "hello wor");

        composer.select_all();
        assert!(composer.delete_selected());
        assert!(composer.is_empty());
    }

    #[test]
    fn chips_are_atomic_under_cursor_movement_and_deletion() {
        let mut composer = Composer::new();
        composer.insert_text("a");
        composer.insert_chip_at_cursor(skill("cr"));
        composer.insert_text("b");
        assert_eq!(composer.text(), "ab");
        assert_eq!(composer.parts().len(), 3);

        // Cursor starts after "b": backspace removes the text char.
        composer.backspace();
        assert_eq!(composer.text(), "a");
        assert_eq!(composer.parts().len(), 2);

        // Now the cursor sits right after the chip: backspace removes the
        // whole chip in one step, never a piece of it.
        composer.backspace();
        assert_eq!(composer.text(), "a");
        assert_eq!(composer.parts().len(), 1);

        // A chip survives deletion of the text in front of it.
        composer.insert_chip_at_cursor(file("src/main.rs"));
        composer.insert_text("z");
        composer.move_left(false);
        composer.move_left(false);
        composer.backspace();
        assert_eq!(
            composer.draft().mention_paths,
            vec!["src/main.rs".to_string()]
        );
        assert_eq!(composer.text(), "z");

        // delete_forward on the chip removes exactly the chip.
        composer.delete_forward();
        assert_eq!(composer.draft().mention_paths, Vec::<String>::new());
        assert_eq!(composer.text(), "z");
    }

    #[test]
    fn cursor_moves_across_a_chip_as_one_step() {
        let mut composer = Composer::new();
        composer.insert_text("x");
        composer.insert_chip_at_cursor(skill("cr"));
        composer.insert_text("y");
        composer.move_home(false);

        // home → after "x" (before chip) → after chip (before "y") → end.
        // The chip occupies exactly one document position.
        composer.move_right(false);
        assert_eq!(composer.doc_position(), 1);
        composer.move_right(false);
        assert_eq!(composer.doc_position(), 2, "the chip is one step");
        composer.move_right(false);
        assert_eq!(composer.doc_position(), 3, "after the chip comes 'y'");
        composer.move_right(false);
        assert_eq!(composer.doc_position(), 3, "the end stays the end");
    }

    #[test]
    fn slash_token_requires_a_single_unbroken_prefix() {
        let mut composer = Composer::new();
        composer.insert_text("/");
        assert_eq!(composer.slash_token().as_deref(), Some(""));

        composer.insert_text("cr");
        assert_eq!(composer.slash_token().as_deref(), Some("cr"));

        composer.insert_text("eate");
        assert_eq!(composer.slash_token().as_deref(), Some("create"));

        composer.insert_text(" now");
        assert_eq!(composer.slash_token(), None, "whitespace ends the token");

        let mut composer = Composer::new();
        composer.insert_text("x/cr");
        assert_eq!(
            composer.slash_token(),
            None,
            "the token must lead the draft"
        );
    }

    #[test]
    fn replace_slash_token_swaps_the_token_for_a_skill_chip_and_space() {
        let mut composer = Composer::new();
        composer.insert_text("/create");
        composer.replace_slash_token("create-plan");

        assert_eq!(composer.slash_token(), None);
        assert_eq!(composer.text(), " ");
        let draft = composer.draft();
        // The skill chip serializes as `/name ` and the trailing space run
        // keeps its own space — the same two-space wire shape the reference
        // `ComposerDraft.parse` produces (trimmed to `/name` at send time).
        assert_eq!(draft.text, "/create-plan  ");
        assert!(draft.mention_paths.is_empty());
        assert!(draft.images.is_empty());
    }

    #[test]
    fn mention_token_runs_from_the_last_at_to_the_end_without_whitespace() {
        let mut composer = Composer::new();
        composer.insert_text("@");
        assert_eq!(composer.mention_token().as_deref(), Some(""));

        composer.insert_text("src/main");
        assert_eq!(composer.mention_token().as_deref(), Some("src/main"));

        composer.insert_text(" ");
        assert_eq!(composer.mention_token(), None, "whitespace ends the token");

        let mut composer = Composer::new();
        composer.insert_text("please read @docs");
        assert_eq!(composer.mention_token().as_deref(), Some("docs"));
    }

    #[test]
    fn accept_mention_swaps_the_token_for_a_file_chip_in_place() {
        let mut composer = Composer::new();
        composer.insert_text("please read @docs/guide");
        composer.accept_mention("docs/guide.md");

        assert_eq!(composer.mention_token(), None);
        let draft = composer.draft();
        assert_eq!(draft.text, "please read ");
        assert_eq!(draft.mention_paths, vec!["docs/guide.md".to_string()]);
    }

    #[test]
    fn draft_serializes_text_and_all_chip_kinds_and_deduplicates_paths() {
        let mut composer = Composer::new();
        composer.insert_text("explain ");
        composer.insert_chip_at_cursor(skill("cr"));
        composer.insert_chip_at_cursor(file("src/main.rs"));
        composer.insert_chip_at_cursor(file("src/main.rs"));
        composer.insert_chip_at_cursor(image("image/png", "AAAA"));

        let draft = composer.draft();
        assert_eq!(draft.text, "explain /cr ");
        assert_eq!(draft.mention_paths, vec!["src/main.rs".to_string()]);
        assert_eq!(draft.images.len(), 1);
        assert_eq!(draft.images[0].mime_type, "image/png");
        assert_eq!(draft.images[0].base64_data, "AAAA");
    }

    #[test]
    fn remove_chip_removes_the_named_chip_and_merges_text() {
        let mut composer = Composer::new();
        composer.insert_text("a");
        composer.insert_chip_at_cursor(file("x.rs"));
        composer.insert_text("b");
        composer.insert_chip_at_cursor(image("image/png", "AAAA"));
        composer.insert_text("c");
        assert_eq!(composer.parts().len(), 5);

        composer.remove_chip(3);
        assert_eq!(composer.parts().len(), 3, "adjacent text merges");
        assert_eq!(composer.text(), "abc");
        let draft = composer.draft();
        assert_eq!(draft.mention_paths, vec!["x.rs".to_string()]);
        assert!(draft.images.is_empty());
    }

    #[test]
    fn selection_delete_spanning_chips_removes_them_whole() {
        let mut composer = Composer::new();
        composer.insert_text("ab");
        composer.insert_chip_at_cursor(skill("cr"));
        composer.insert_text("cd");

        composer.move_home(false);
        composer.move_right(true);
        composer.move_right(true);
        composer.move_right(true);
        composer.move_right(true); // selects "ab" + chip + "c"
        assert!(composer.delete_selected());
        assert_eq!(composer.text(), "d");
        assert!(composer.draft().mention_paths.is_empty());
    }

    #[test]
    fn a_chip_only_draft_is_sendable_and_serializes() {
        let mut composer = Composer::new();
        composer.insert_chip_at_cursor(image("image/jpeg", "BBBB"));
        assert!(!composer.is_empty());
        assert_eq!(composer.text(), "");
        let draft = composer.draft();
        assert_eq!(draft.text, "");
        assert_eq!(draft.images.len(), 1);
        assert_eq!(draft.images[0].mime_type, "image/jpeg");
    }
}

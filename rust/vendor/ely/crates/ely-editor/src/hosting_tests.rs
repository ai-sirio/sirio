//! Tests of the hooks Sirio's file view hosts the editor through. A plain test module, so the repository's gate runs it.

use gpui::{ClipboardItem, Entity, EntityInputHandler, TestAppContext, VisualTestContext};

use super::CodeEditor;
use crate::theme::Theme;

fn editor<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CodeEditor>, &'a mut VisualTestContext) {
    cx.update(Theme::init);
    let text = text.to_string();
    cx.add_window_view(move |window, cx| CodeEditor::new(text, window, cx))
}

/// Failure list, written before the code:
/// 1. The paste keeps its `\r`, so the buffer holds a character that is not a line break.
/// 2. A lone `\r` is not read as a break at all.
/// 3. The paste is not one undo step: one undo leaves part of it behind.
#[gpui::test]
fn a_crlf_paste_lands_as_lf_with_the_caret_after_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor("", cx);
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            EntityInputHandler::paste(
                editor,
                ClipboardItem::new_string("a\r\nb\rc".into()),
                window,
                cx,
            );
            assert_eq!(
                editor.text(),
                "a\nb\nc",
                "the buffer keeps the lines, not the carriage returns"
            );
            assert_eq!(
                editor.primary().head,
                5,
                "the caret sits after the pasted text"
            );
            editor.undo(cx);
            assert_eq!(editor.text(), "", "one undo takes the whole paste back");
        })
    });
}

/// Failure list, written before the code:
/// 4. The composition commit path (`replace_text_in_range`) keeps a `\r\n`
///    too, so text typed through an input method differs from the paste path.
#[gpui::test]
fn a_composed_text_with_a_crlf_lands_as_lf(cx: &mut TestAppContext) {
    let (editor, cx) = editor("", cx);
    editor.update_in(cx, |editor, window, cx| {
        editor.replace_text_in_range(None, "x\r\ny", window, cx);
        assert_eq!(editor.text(), "x\ny");
    });
}

/// Failure list, written before the code:
/// 1. `reset_keeping_view` does not exist, so a disk adoption has to reset the
///    cursors, and a reset moves the view to the caret.
/// 2. A cursor past the new end is kept there, and the next edit indexes past
///    the text.
/// 3. A cursor inside a character is kept there, which the editor refuses to
///    edit at.
/// 4. The history survives, so an undo brings back text the disk replaced.
/// 5. Cursors that the clamp puts on one offset stay as several.
#[gpui::test]
fn a_silent_reset_keeps_its_cursors_clamped_and_its_history_empty(cx: &mut TestAppContext) {
    let (editor, cx) = editor("abc\ndef\nghi", cx);
    editor.update(cx, |editor, cx| {
        editor.select([11..11], cx);
        editor.type_text("!", cx);
        editor.select([2..4, 8..8], cx);
        editor.reset_keeping_view("abcde", cx);
        let ranges: Vec<_> = editor.selections().iter().map(|s| s.range()).collect();
        assert_eq!(
            ranges,
            [2..4, 5..5],
            "each cursor keeps its place; the one past the end is clamped to it"
        );
        editor.undo(cx);
        assert_eq!(editor.text(), "abcde", "the reset cleared the history");
        editor.reset_keeping_view("ab", cx);
        let ranges: Vec<_> = editor.selections().iter().map(|s| s.range()).collect();
        assert_eq!(
            ranges,
            [2..2],
            "cursors clamped onto one offset are joined"
        );
    });
}

#[gpui::test]
fn a_silent_reset_moves_a_cursor_inside_a_character_back_onto_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor("xyz", cx);
    editor.update(cx, |editor, cx| {
        editor.select([2..2], cx);
        editor.reset_keeping_view("aé", cx);
        assert_eq!(
            editor.primary().head,
            1,
            "byte 2 is inside é, so the caret floors to the start of é"
        );
        assert_eq!(editor.text(), "aé");
    });
}

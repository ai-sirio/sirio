//! The file tab's code, drawn and edited by Ely's `CodeEditor`.
//!
//! The `Editor` owns the text, the disk state and the Markdown operations;
//! the code editor owns what is on screen and what the keyboard does. Text
//! flows `CodeEditor` → `Editor` on every `Changed` event (the user's own
//! edits), and `Editor` → `CodeEditor` through `reset` whenever the model
//! changed the text on its own (a reload, a disk adoption, a Markdown op).
//! The two are kept equal after every such event.

use std::cell::RefCell;
use std::ops::Range;

use ely_gpui_component::editor::{CodeEditor, EditorEvent};
use gpui::{App, AppContext as _, Context, Entity, FocusHandle, Focusable, HighlightStyle, Window};
use sirio_theme::Theme;

use super::{CodeSpan, FileView, MarkdownMode, code_spans, line_highlights};
use crate::editor::{Language, LineEnding, LoadStatus, Selection, markdown_links_in_line};
use crate::editor_prefs::EditorPrefs;

impl FileView {
    /// The code editor: `None` until the file has loaded and a frame has drawn
    /// it. It stays for the life of the tab, and keeps its place in Preview.
    pub fn code_editor(&self) -> Option<&Entity<CodeEditor>> {
        self.code_editor.as_ref()
    }

    /// Creates the code editor once the file is loaded. Called from `render`,
    /// because building an editor needs a window and the load does not have one.
    pub(super) fn ensure_code_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.code_editor.is_some() {
            return;
        }
        let Some(editor) = self.editor() else {
            return;
        };
        if editor.status() != &LoadStatus::Loaded {
            return;
        }
        let language = editor.language();
        let text = editor.buffer().to_owned();
        let read_only = self.snapshot.is_some();
        let prefs = EditorPrefs::current(cx).view_options();
        let code = cx.new(|cx| {
            let mut code = CodeEditor::new(text, window, cx)
                .language(language.name())
                .highlighter(code_highlighter(language))
                .without_breakpoints();
            if read_only {
                code = code.read_only();
            }
            code.set_view_options(prefs, cx);
            code
        });
        let focus = code.read(cx).focus_handle(cx);
        let subscriptions = [
            cx.subscribe(&code, |view, code, event: &EditorEvent, cx| {
                if *event == EditorEvent::Changed {
                    view.code_changed(code, cx);
                }
            }),
            cx.on_focus(&focus, window, |view, _window, cx| view.check_external(cx)),
        ];
        self.code_subscriptions.extend(subscriptions);
        self.code_editor = Some(code);
        self.apply_pending_reveal(cx);
    }

    /// Pushes the app's editor settings into the code editor when they differ
    /// from what it shows. Called every frame: a settings save reaches open
    /// tabs on their next draw.
    pub(super) fn sync_code_prefs(&self, cx: &mut Context<Self>) {
        let Some(code) = &self.code_editor else {
            return;
        };
        let prefs = EditorPrefs::current(cx).view_options();
        if prefs != code.read(cx).view_options() {
            code.update(cx, |code, cx| code.set_view_options(prefs, cx));
        }
    }

    /// The code editor's text changed. The `Editor` takes it unless it already
    /// holds it (a `reset` or a format op that wrote the `Editor` first). The
    /// `Editor` is LF-only, so a pasted CRLF is normalised there; a code
    /// editor still holding that carriage return is then brought back to the
    /// `Editor`'s text, which keeps the two offsets equal.
    fn code_changed(&mut self, code: Entity<CodeEditor>, cx: &mut Context<Self>) {
        let text = code.read(cx).text().to_owned();
        if let Some(editor) = self.editor_mut()
            && editor.buffer() != text
        {
            editor.replace_all(&text);
        }
        self.sync_code_from_editor(cx);
        cx.notify();
    }

    /// Whether the code editor is what the tab shows right now: the file is
    /// loaded, the mode is Code and the editor exists.
    fn code_shown(&self) -> bool {
        self.code_editor.is_some()
            && self
                .editor()
                .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
            && self.effective_mode() == MarkdownMode::Code
    }

    /// The handle the keyboard goes to: the code editor's while it draws the
    /// file, otherwise the view's own.
    pub(super) fn focus_target(&self, cx: &App) -> FocusHandle {
        match &self.code_editor {
            Some(code) if self.code_shown() => code.read(cx).focus_handle(cx),
            _ => self.editor_focus.clone(),
        }
    }

    /// Moves the keyboard when the code editor appears, or the mode changes
    /// under it: a freshly opened file takes typing at once, and a switch to
    /// Preview gives the keyboard back to the view.
    pub(super) fn hand_over_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(code) = &self.code_editor else {
            return;
        };
        let code_focus = code.read(cx).focus_handle(cx);
        if self.code_shown() {
            if self.editor_focus.is_focused(window) {
                code_focus.focus(window, cx);
            }
        } else if code_focus.is_focused(window) {
            self.editor_focus.focus(window, cx);
        }
    }

    /// Brings the code editor to the `Editor`'s text after the model changed
    /// it on its own. The caret keeps its offset, clamped to the new text and
    /// moved back to a character boundary. Does nothing when the texts agree.
    pub(super) fn sync_code_from_editor(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let Some(editor) = self.editor() else {
            return;
        };
        let text = editor.buffer();
        if code.read(cx).text() == text {
            return;
        }
        let text = text.to_owned();
        let head = code.read(cx).primary().head;
        code.update(cx, |code, cx| {
            code.reset(text, cx);
            let at = floor_char_boundary(code.text(), head);
            code.select([at..at], cx);
        });
        cx.notify();
    }

    /// The code editor's primary selection, in its own byte offsets. `None`
    /// while no code editor exists.
    pub(super) fn code_selection(&self, cx: &App) -> Option<Selection> {
        let range = self.code_editor.as_ref()?.read(cx).primary().range();
        Some(Selection {
            start: range.start,
            end: range.end,
        })
    }

    /// The code editor's primary caret, or 0 while no code editor exists.
    pub(super) fn code_caret(&self, cx: &App) -> usize {
        self.code_editor
            .as_ref()
            .map_or(0, |code| code.read(cx).primary().head)
    }

    /// Replaces the code editor's primary selection with `text`, as one undo
    /// step. A snapshot refuses. Line endings are normalised first, as the
    /// `Editor` would normalise them.
    pub(super) fn replace_code_selection(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.snapshot.is_some() {
            return;
        }
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let text = LineEnding::normalize(text);
        code.update(cx, |code, cx| {
            let range = code.primary().range();
            code.edit([(range, text)], cx);
        });
    }

    /// Jumps the caret to `line` (0-based) and scrolls it into view. Only the
    /// code editor can place a caret, so this is a no-op until it exists.
    pub(super) fn apply_pending_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(line) = self.pending_reveal else {
            return;
        };
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        self.pending_reveal = None;
        code.update(cx, |code, cx| {
            let start = line_start_offset(code.text(), line);
            code.select([start..start], cx);
            code.reveal_primary(cx);
        });
        cx.notify();
    }
}

/// The code editor's colours for each line: the grammar's spans and the
/// Markdown link rules, read from the theme at each call so a theme switch
/// recolours. The spans of a line are memoized by its index. An entry is
/// reused only while the line's text is the same, and the table is bounded by
/// the file's line count. The minimap asks for every row on every render, so
/// a cache keyed by a fixed count would clear and re-run the grammar on every
/// caret blink.
fn code_highlighter(
    language: Language,
) -> impl Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)> + 'static {
    let cache: RefCell<Vec<Option<(String, Vec<CodeSpan>)>>> = RefCell::new(Vec::new());
    move |index, line, cx| {
        let mut cache = cache.borrow_mut();
        if cache.len() <= index {
            cache.resize_with(index + 1, || None);
        }
        if !matches!(&cache[index], Some((text, _)) if text == line) {
            cache[index] = Some((line.to_owned(), code_spans(language, line)));
        }
        let Some((_, spans)) = &cache[index] else {
            return Vec::new();
        };
        let links: Vec<(Range<usize>, String)> = if language == Language::Markdown {
            markdown_links_in_line(line)
                .into_iter()
                .map(|span| (span.label_range, span.target))
                .collect()
        } else {
            Vec::new()
        };
        let theme = Theme::get(cx);
        let palette = theme.syntax_palette();
        line_highlights(spans, &links, &[], theme, &palette)
    }
}

/// Where line `line` (0-based) begins. A line past the end begins where the
/// last line does.
fn line_start_offset(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    match text.match_indices('\n').nth(line - 1) {
        Some((newline, _)) => newline + 1,
        None => text.rfind('\n').map_or(0, |newline| newline + 1),
    }
}

/// `offset` clamped to `text` and moved back onto a character boundary.
fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

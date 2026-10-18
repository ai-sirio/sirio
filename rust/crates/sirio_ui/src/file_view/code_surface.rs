//! The file tab's code, drawn and edited by Ely's `CodeEditor`.
//!
//! The `Editor` owns the text, the disk state and the Markdown operations;
//! the code editor owns what is on screen and what the keyboard does. Text
//! flows `CodeEditor` → `Editor` on every `Changed` event (the user's own
//! edits), and `Editor` → `CodeEditor` through `reset_keeping_view` whenever
//! the model changed the text on its own (a reload, a disk adoption, a
//! snapshot retry), or through `edit` for a Markdown op, as one undo step.
//! The two are kept equal after every such event.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use ely_gpui_component::editor::{CodeEditor, Diagnostic, DiffHunk, EditorEvent, GitMark};
use ely_gpui_component::primitives::Severity;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, HighlightStyle, Pixels, Point,
    Window,
};
use sirio_git::FileDiff;
use sirio_theme::Theme;

use super::{
    CodeSpan, DiagnosticSeverity, FileDiagnostic, FileView, FileViewEvent, MarkdownMode, code_spans,
    git_marks, line_highlights,
};
use crate::editor::{Language, LoadStatus, Selection, markdown_links_in_line};
use crate::editor_prefs::EditorPrefs;
use crate::file_context_menu::FileContextAction;

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
        let spans = self.span_cache.clone();
        let code = cx.new(|cx| {
            let mut code = CodeEditor::new(text, window, cx)
                .language(language.name())
                .highlighter(code_highlighter(language, spans))
                .without_breakpoints();
            if read_only {
                code = code.read_only();
            }
            code.set_view_options(prefs, cx);
            code
        });
        let focus = code.read(cx).focus_handle(cx);
        let subscriptions = [
            cx.subscribe_in(&code, window, |view, code, event: &EditorEvent, window, cx| {
                match event {
                    EditorEvent::Changed => view.code_changed(code.clone(), cx),
                    EditorEvent::Pointer {
                        offset: Some(offset),
                        position,
                    } => view.hover_moved(*offset, *position, cx),
                    EditorEvent::Pointer { offset: None, .. } => view.dismiss_hover(cx),
                    EditorEvent::PlatformClick { offset } => {
                        view.code_platform_click(*offset, code, cx);
                    }
                    EditorEvent::SecondaryClick { offset, position } => {
                        view.code_secondary_click(*offset, *position, code, window, cx);
                    }
                    EditorEvent::GitMarkPressed { line } => view.toggle_hunk(*line, cx),
                    // The read-only banner's "Make editable" unlocks the code
                    // editor. A snapshot stays read-only, so the lock goes back on
                    // before any key can reach it.
                    EditorEvent::Unlock if view.is_snapshot() => {
                        code.update(cx, |code, cx| code.set_read_only(true, cx));
                    }
                    _ => {}
                }
            }),
            cx.on_focus(&focus, window, |view, _window, cx| view.gained_focus(cx)),
            // The editor says nothing of a caret move, but it notifies, and the
            // breadcrumb names the caret's line. Cheap: a blink notifies too.
            cx.observe(&code, |view, _code, cx| {
                if view.refresh_caret_line(cx) {
                    cx.notify();
                }
            }),
            // Any key on the code dismisses the hover card. An observer, not a
            // key-down listener: gpui runs a bound key's action (arrows, Enter,
            // Backspace) before key-down listeners and stops there, capture
            // phase included, so only an observer sees every key.
            cx.observe_keystrokes(|view, _, window, cx| {
                let focused = view
                    .code_editor
                    .as_ref()
                    .is_some_and(|code| code.read(cx).focus_handle(cx).is_focused(window));
                if focused {
                    view.dismiss_hover(cx);
                }
            }),
        ];
        self.code_subscriptions.extend(subscriptions);
        self.code_editor = Some(code);
        self.refresh_caret_line(cx);
        // Diagnostics may have arrived before the first draw.
        self.push_diagnostics(cx);
        self.push_git(cx);
        self.apply_pending_reveal(cx);
    }

    /// Hands the code editor the findings as Ely draws them, for the text it
    /// holds now. Ely keeps the ones that fit that text. Called whenever the
    /// findings change, and once the editor exists.
    pub(super) fn push_diagnostics(&self, cx: &mut Context<Self>) {
        let Some(code) = &self.code_editor else {
            return;
        };
        let drawn = ely_diagnostics(&self.diagnostics, code.read(cx).text());
        code.update(cx, |code, cx| code.set_diagnostics(drawn, cx));
    }

    /// The file's diff against HEAD, as the app last pushed it. The gutter
    /// marks it, and a hunk drawn in place stays open while the new diff still
    /// opens one on the same line. A snapshot shows a revision, not the working
    /// file, so it keeps no diff and no marks: a diff against HEAD would
    /// describe a text the snapshot is not.
    pub fn set_git_diff(&mut self, diff: Option<FileDiff>, cx: &mut Context<Self>) {
        self.git_diff = if self.is_snapshot() { None } else { diff };
        self.open_hunk = self.open_hunk.filter(|open| {
            self.hunks_at(*open)
                .first()
                .is_some_and(|hunk| git_marks::opening_line(hunk) == *open)
        });
        self.push_git(cx);
        cx.notify();
    }

    /// Asks to start a fetch of this file's diff; `true` means the caller fetches
    /// and answers through `land_git_diff`. While a fetch runs the answer is
    /// `false` and the request is remembered: a run per request would pile up
    /// whole-worktree scans while an agent keeps rewriting the file, and one
    /// more run when the fetch lands covers all of them.
    pub fn begin_git_diff(&mut self) -> bool {
        if self.git_diff_in_flight {
            self.git_diff_again = true;
            return false;
        }
        self.git_diff_in_flight = true;
        true
    }

    /// Shows the diff a fetch found, and asks for one more fetch when the diff
    /// went stale while it ran.
    pub fn land_git_diff(&mut self, diff: Option<FileDiff>, cx: &mut Context<Self>) {
        self.git_diff_in_flight = false;
        self.set_git_diff(diff, cx);
        if std::mem::take(&mut self.git_diff_again) {
            cx.emit(FileViewEvent::GitDiffStale(self.path.clone()));
        }
    }

    /// The gutter marks of the diff: each changed line of the file, sorted.
    pub fn git_mark_lines(&self) -> Vec<(usize, GitMark)> {
        self.git_diff
            .as_ref()
            .map(git_marks::marks)
            .unwrap_or_default()
    }

    /// The gutter marks as the editor's state reports them: `line:kind` pairs,
    /// sorted, joined by commas (`3:added,5:modified`), empty when there are none.
    pub fn git_marks_text(&self) -> String {
        self.git_mark_lines()
            .iter()
            .map(|(line, mark)| {
                let kind = match mark {
                    GitMark::Added => "added",
                    GitMark::Modified => "modified",
                    GitMark::Deleted => "deleted",
                };
                format!("{line}:{kind}")
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The gutter line of the hunk drawn in place, if one is open.
    pub fn open_hunk_line(&self) -> Option<usize> {
        self.open_hunk
    }

    /// The hunks the diff opens on `line`, none without a diff.
    fn hunks_at(&self, line: usize) -> Vec<DiffHunk> {
        self.git_diff
            .as_ref()
            .map(|diff| git_marks::hunks_at(diff, line))
            .unwrap_or_default()
    }

    /// A press on the gutter at `line` (`EditorEvent::GitMarkPressed`). A line
    /// that opens a hunk draws it in place, or closes it when it is the hunk
    /// already drawn. Every hunk the line opens is drawn or closed together. A
    /// line that opens no hunk changes nothing.
    pub fn toggle_hunk(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(opening) = self.hunks_at(line).first().map(git_marks::opening_line) else {
            return;
        };
        self.open_hunk = if self.open_hunk == Some(opening) {
            None
        } else {
            Some(opening)
        };
        self.push_git(cx);
        cx.notify();
    }

    /// Closes the hunk drawn in place, if one is open.
    pub(super) fn close_hunk(&mut self, cx: &mut Context<Self>) {
        self.open_hunk = None;
        self.push_git(cx);
        cx.notify();
    }

    /// Hands the code editor the gutter marks and the hunks drawn in place, for
    /// the text it holds now. Called when either changes, and once the editor
    /// exists. A gap past the last line, left by an edit since the diff was
    /// pushed, is drawn above that line (`fit`).
    pub(super) fn push_git(&self, cx: &mut Context<Self>) {
        let Some(code) = &self.code_editor else {
            return;
        };
        let lines = code.read(cx).text().matches('\n').count() + 1;
        let hunks: Vec<DiffHunk> = self
            .open_hunk
            .map(|open| self.hunks_at(open))
            .unwrap_or_default()
            .into_iter()
            .map(|hunk| git_marks::fit(hunk, lines))
            .collect();
        let marks = self.git_mark_lines();
        code.update(cx, |code, cx| {
            code.set_git_marks(marks, cx);
            code.set_diff(hunks, cx);
        });
    }

    /// A left press and release with the platform modifier at `offset`. A
    /// Markdown link's label opens its target, and anything else asks for the
    /// definition. Ely's `PlatformClick` carries no position, so the definition
    /// card goes where the pointer last entered `offset`: `hover_point` moves
    /// only when the offset changes, so it is within a glyph of the click.
    fn code_platform_click(
        &mut self,
        offset: usize,
        code: &Entity<CodeEditor>,
        cx: &mut Context<Self>,
    ) {
        if self.is_markdown() {
            // A click on the label, the part the highlighter underlines as a
            // link; the `(target)` beside it is not a link to press.
            let target = {
                let text = code.read(cx).text();
                let line_start = text[..offset].rfind('\n').map_or(0, |newline| newline + 1);
                let line_end = text[offset..]
                    .find('\n')
                    .map_or(text.len(), |newline| offset + newline);
                markdown_links_in_line(&text[line_start..line_end])
                    .into_iter()
                    .find(|span| span.label_range.contains(&(offset - line_start)))
                    .map(|span| span.target)
            };
            if let Some(target) = target {
                self.open_markdown_link(&target, cx);
                return;
            }
        }
        self.request_definition(offset, Some(self.hover_point), cx);
    }

    /// A right press on the code. A press off the selection moves the caret to
    /// it, without scrolling, so "Go to definition" from the menu asks about
    /// the word under the pointer. A press inside the selection keeps it, and
    /// a press on the gutter (`offset` is `None`) moves nothing. The menu opens
    /// either way, at the pointer.
    ///
    /// "Inside" is the primary selection only. A press inside a secondary
    /// cursor's selection, but outside the primary's, moves the caret too, so
    /// it collapses every cursor to one caret at the press. That is harmless:
    /// a right press edits nothing, and the menu acts at a single point.
    fn code_secondary_click(
        &mut self,
        offset: Option<usize>,
        position: Point<Pixels>,
        code: &Entity<CodeEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(offset) = offset {
            // Both ends count as inside, so a press on the last character of a
            // selection, which Ely reads as the boundary after it, keeps it.
            let selected = code.read(cx).primary().range();
            if !(selected.start..=selected.end).contains(&offset) {
                code.update(cx, |code, cx| code.select_in_place([offset..offset], cx));
            }
        }
        self.open_context_menu(position, window, cx);
    }

    /// The debug verbs' platform click at `offset`: the path Ely's
    /// `PlatformClick` takes (`code_platform_click`). Ely puts the caret at the
    /// release before it emits the click (`line.rs`), so the caret goes there
    /// first, as it does for a real click.
    pub fn platform_click_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if let Some(code) = self.code_editor.clone() {
            code.update(cx, |code, cx| code.select_in_place([offset..offset], cx));
            self.code_platform_click(offset, &code, cx);
        }
    }

    /// The debug verbs' plain click: the caret moves to `offset`, without a scroll.
    pub fn place_caret_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if let Some(code) = &self.code_editor {
            code.update(cx, |code, cx| code.select_in_place([offset..offset], cx));
        }
    }

    /// The debug verbs' right press at `offset`, then the menu row's click for
    /// `action`: `code_secondary_click`, then `context_effect` and the effect it
    /// names, as the menu runs them. The press comes first because it changes
    /// what the row can do (a press off the selection collapses it). A row the
    /// click would refuse answers its reason, and the menu stays open as it
    /// does after a click on a disabled row.
    pub fn context_action_at(
        &mut self,
        offset: usize,
        position: Point<Pixels>,
        action: FileContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let code = self
            .code_editor
            .clone()
            .ok_or_else(|| "the code editor is not drawn".to_string())?;
        self.code_secondary_click(Some(offset), position, &code, window, cx);
        let effect = self.context_effect(action, cx)?;
        self.handle_context_action(effect, window, cx);
        Ok(())
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
    /// holds it (a `reset_keeping_view` or a format op that wrote the `Editor`
    /// first). The code editor never holds a carriage return (Ely reads each
    /// one as a line break), so after `replace_all` the two texts agree; a
    /// carriage return that got through is a bug, reported by its lengths and
    /// left as it is.
    fn code_changed(&mut self, code: Entity<CodeEditor>, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut()
            && editor.buffer() != code.read(cx).text()
        {
            let text = code.read(cx).text();
            editor.replace_all(text);
            if text.contains('\r') && editor.buffer() != text {
                eprintln!(
                    "[files] code editor kept a carriage return: editor {} bytes, code {} bytes",
                    editor.buffer().len(),
                    text.len()
                );
            }
        }
        self.find_text_changed(cx);
        cx.notify();
    }

    /// Whether the code editor is what the tab shows right now: the file is
    /// loaded, the mode is Code and the editor exists.
    pub fn code_shown(&self) -> bool {
        self.code_editor.is_some()
            && self
                .editor()
                .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
            && self.effective_mode() == MarkdownMode::Code
    }

    /// The handle the keyboard goes to: the code editor's while it draws the
    /// file, otherwise the view's own.
    pub fn focus_target(&self, cx: &App) -> FocusHandle {
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
    /// it on its own. The cursors keep their places and the view stays where
    /// it is (`reset_keeping_view`). Does nothing when the texts agree.
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
        code.update(cx, |code, cx| code.reset_keeping_view(text, cx));
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
        // `edit` takes text as it is; the code editor's own paste reads these too.
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
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
        // A code editor left over from before a load has the old text; the
        // reveal waits for the loaded one.
        if self
            .editor()
            .is_none_or(|editor| editor.status() != &LoadStatus::Loaded)
        {
            return;
        }
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

/// The grammar's spans per line text, in two generations. A lookup hits the
/// current generation; failing that it takes the entry from the previous one;
/// failing that it runs the grammar. A new generation starts each time the
/// file view renders (`FileView::render`). The minimap asks for every row
/// during the editor's own render, so after an edit or a caret move every row
/// comes back through the previous generation, and a row that stops being
/// drawn is dropped one generation later. Keyed by text, so a line that moves
/// (an Enter above it) keeps its spans, and a caret blink, which renders the
/// editor alone, hits. Bounded: cleared when the current generation holds more
/// than eight entries per line of the file, and at least `MIN_CACHED_LINES`.
#[derive(Default)]
pub(super) struct SpanCache {
    current: HashMap<String, Vec<CodeSpan>>,
    previous: HashMap<String, Vec<CodeSpan>>,
    highest: usize,
}

/// The most lines a span cache keeps, whatever the length of the file.
const MIN_CACHED_LINES: usize = 65_536;

impl SpanCache {
    /// Starts a new generation: what was current becomes the previous one, and
    /// what was only in the previous one is dropped.
    pub(super) fn rotate(&mut self) {
        self.previous = std::mem::take(&mut self.current);
    }

    /// The spans of `line`, which is line `index`.
    fn spans(&mut self, index: usize, language: Language, line: &str) -> &[CodeSpan] {
        self.highest = self.highest.max(index);
        let bound = (8 * (self.highest + 1)).max(MIN_CACHED_LINES);
        if self.current.len() > bound {
            self.current.clear();
            self.previous.clear();
        }
        if !self.current.contains_key(line) {
            let (key, spans) = match self.previous.remove_entry(line) {
                Some(entry) => entry,
                None => (line.to_owned(), code_spans(language, line)),
            };
            self.current.insert(key, spans);
        }
        &self.current[line]
    }
}

/// The code editor's colours for each line: the grammar's spans (from `cache`)
/// and the Markdown link rules, read from the theme at each call so a theme
/// switch recolours.
fn code_highlighter(
    language: Language,
    cache: Rc<RefCell<SpanCache>>,
) -> impl Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)> + 'static {
    move |index, line, cx| {
        let mut cache = cache.borrow_mut();
        let spans = cache.spans(index, language, line);
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
        line_highlights(spans, &links, theme, &palette)
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

/// The squiggles Ely draws for `diagnostics` in `text`, worst drawn last.
///
/// Ely stacks overlapping styles in list order, the later one on top, so the
/// sort puts the worst finding after the milder ones it overlaps. The sort is
/// stable: findings of one severity keep the order they came in.
fn ely_diagnostics(diagnostics: &[FileDiagnostic], text: &str) -> Vec<Diagnostic> {
    let mut drawn: Vec<Diagnostic> = diagnostics
        .iter()
        .filter_map(|found| {
            Some(Diagnostic {
                range: marked_range(&found.range, text)?,
                severity: ely_severity(found.severity),
                message: found.message.clone().into(),
            })
        })
        .collect();
    drawn.sort_by_key(|diagnostic| severity_rank(diagnostic.severity));
    drawn
}

/// The bytes a finding marks, or `None` when it marks nothing. A range that
/// does not fit the text is dropped, as Ely drops it, rather than moved onto
/// bytes it does not name. A zero-width finding (one at a caret, such as
/// "expected `;`") takes the character after the caret, or the one before it
/// at a line end, because a zero-width run paints nothing. An empty line has
/// neither, and marks nothing.
fn marked_range(range: &Range<usize>, text: &str) -> Option<Range<usize>> {
    let fits = |at: usize| at <= text.len() && text.is_char_boundary(at);
    if range.start > range.end || !fits(range.start) || !fits(range.end) {
        return None;
    }
    if !range.is_empty() {
        return Some(range.clone());
    }
    let at = range.start;
    if let Some(next) = text[at..].chars().next().filter(|next| *next != '\n') {
        return Some(at..at + next.len_utf8());
    }
    text[..at]
        .chars()
        .next_back()
        .filter(|previous| *previous != '\n')
        .map(|previous| at - previous.len_utf8()..at)
}

fn ely_severity(severity: DiagnosticSeverity) -> Severity {
    match severity {
        DiagnosticSeverity::Error => Severity::Danger,
        DiagnosticSeverity::Warning => Severity::Warning,
        DiagnosticSeverity::Information | DiagnosticSeverity::Hint => Severity::Info,
    }
}

/// The order `ely_diagnostics` sorts by: Ely's `Severity` has none of its own.
pub(super) fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Info | Severity::Success => 0,
        Severity::Warning => 1,
        Severity::Danger => 2,
    }
}

#[cfg(test)]
mod tests {
    //! How `ely_diagnostics` can fail. Each failure has a test below.
    //!
    //! - A zero-width finding paints nothing. Ely's layout paints between
    //!   distinct edges, so an empty range is kept and is invisible.
    //! - A zero-width finding widens by one byte, not one character, and cuts a
    //!   multi-byte character in half.
    //! - A zero-width finding at a line end widens forward onto the newline, so
    //!   the squiggle sits on a line break.
    //! - A zero-width finding on an empty line is drawn over the line break
    //!   that ends it.
    //! - A range that runs past the text is kept and marks bytes the finding
    //!   never named.
    //! - A range that splits a multi-byte character is kept, and Ely would draw
    //!   it across a boundary the text system rejects.
    //! - A range whose start is past its end is kept.
    //! - A multi-line finding is clipped to its first line, so its later lines
    //!   lose it.
    //! - The worst finding is drawn first, under a milder one that overlaps it.
    //! - Findings of one severity are reordered, so two that overlap swap
    //!   places from one frame to the next.
    //! - A severity maps to the wrong colour: Error to Danger, Warning to
    //!   Warning, Information and Hint to Info.

    use super::*;

    fn finding(range: Range<usize>, severity: DiagnosticSeverity, message: &str) -> FileDiagnostic {
        FileDiagnostic {
            line: 0,
            range,
            severity,
            message: message.to_owned(),
        }
    }

    fn marks(found: &[Diagnostic]) -> Vec<(Range<usize>, Severity)> {
        found
            .iter()
            .map(|diagnostic| (diagnostic.range.clone(), diagnostic.severity))
            .collect()
    }

    fn messages(found: &[Diagnostic]) -> Vec<String> {
        found
            .iter()
            .map(|diagnostic| diagnostic.message.to_string())
            .collect()
    }

    #[test]
    fn a_finding_marks_its_own_span_not_the_whole_line() {
        let found = ely_diagnostics(
            &[finding(4..5, DiagnosticSeverity::Warning, "unused")],
            "let x = 1;",
        );
        assert_eq!(marks(&found), vec![(4..5, Severity::Warning)]);
    }

    #[test]
    fn a_multi_line_finding_keeps_its_whole_span() {
        // The span runs from the first line into the second.
        let found = ely_diagnostics(
            &[finding(4..15, DiagnosticSeverity::Error, "boom")],
            "let a = 1;\nlet b = 2;\n",
        );
        assert_eq!(marks(&found), vec![(4..15, Severity::Danger)]);
    }

    #[test]
    fn the_worst_finding_is_drawn_last_so_it_shows_over_a_milder_one() {
        let found = ely_diagnostics(
            &[
                finding(2..8, DiagnosticSeverity::Warning, "unused"),
                finding(0..5, DiagnosticSeverity::Error, "boom"),
            ],
            "let x = 1;",
        );
        assert_eq!(
            marks(&found),
            vec![(2..8, Severity::Warning), (0..5, Severity::Danger)],
        );
    }

    #[test]
    fn findings_of_one_severity_keep_their_order() {
        // Forty findings, every third one Danger and the rest Warnings. The
        // standard library's unstable sort keeps a short slice in order, so a
        // handful would not show that the sort is stable.
        let input: Vec<(DiagnosticSeverity, String)> = (0..40)
            .map(|i| {
                let severity = if i % 3 == 2 {
                    DiagnosticSeverity::Error
                } else {
                    DiagnosticSeverity::Warning
                };
                (severity, format!("finding {i:02}"))
            })
            .collect();
        let findings: Vec<FileDiagnostic> = input
            .iter()
            .map(|(severity, message)| finding(0..3, *severity, message))
            .collect();
        // Warnings first, then Danger, each in the order it came in.
        let expected: Vec<String> = input
            .iter()
            .filter(|(severity, _)| *severity == DiagnosticSeverity::Warning)
            .chain(input.iter().filter(|(severity, _)| *severity == DiagnosticSeverity::Error))
            .map(|(_, message)| message.clone())
            .collect();
        let found = ely_diagnostics(&findings, "let x = 1;");
        assert_eq!(messages(&found), expected);
    }

    #[test]
    fn a_zero_width_finding_widens_to_the_next_character() {
        // "expected `;`" arrives as an empty range at the caret.
        let found = ely_diagnostics(
            &[finding(4..4, DiagnosticSeverity::Error, "expected `;`")],
            "let x = 1;",
        );
        assert_eq!(marks(&found), vec![(4..5, Severity::Danger)]);
    }

    #[test]
    fn a_zero_width_finding_widens_by_a_whole_character_not_a_byte() {
        // Widening by one byte would cut the crab in half.
        let found = ely_diagnostics(
            &[finding(4..4, DiagnosticSeverity::Error, "boom")],
            "let \u{1f980} = 2;",
        );
        assert_eq!(marks(&found), vec![(4..8, Severity::Danger)]);
    }

    #[test]
    fn a_zero_width_finding_at_a_line_end_widens_backwards_not_onto_the_newline() {
        // Offset 10 is the newline that ends "let x = 1;".
        let found = ely_diagnostics(
            &[finding(10..10, DiagnosticSeverity::Error, "boom")],
            "let x = 1;\nlet",
        );
        assert_eq!(marks(&found), vec![(9..10, Severity::Danger)]);
    }

    #[test]
    fn a_zero_width_finding_on_an_empty_line_marks_nothing() {
        // "a\n\nb": the empty second line has only the newline that ends it,
        // and the newline before it.
        let found = ely_diagnostics(
            &[finding(2..2, DiagnosticSeverity::Error, "boom")],
            "a\n\nb",
        );
        assert!(found.is_empty(), "no character on that line to mark");
    }

    #[test]
    fn a_range_past_the_end_of_the_text_is_dropped() {
        let found = ely_diagnostics(
            &[
                finding(8..40, DiagnosticSeverity::Error, "stale"),
                finding(40..41, DiagnosticSeverity::Error, "stale"),
            ],
            "let x = 1;",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn a_range_that_splits_a_character_is_dropped() {
        // The crab is four bytes, at 0..4; offset 1 is inside it.
        let found = ely_diagnostics(
            &[finding(1..2, DiagnosticSeverity::Error, "boom")],
            "\u{1f980}",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn an_inverted_range_is_dropped() {
        let found = ely_diagnostics(
            &[finding(6..2, DiagnosticSeverity::Error, "boom")],
            "let x = 1;",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn each_severity_maps_to_the_colour_its_weight_earns() {
        let cases = [
            (DiagnosticSeverity::Error, Severity::Danger),
            (DiagnosticSeverity::Warning, Severity::Warning),
            (DiagnosticSeverity::Information, Severity::Info),
            (DiagnosticSeverity::Hint, Severity::Info),
        ];
        for (input, expected) in cases {
            let found = ely_diagnostics(&[finding(0..1, input, "x")], "x");
            assert_eq!(marks(&found), vec![(0..1, expected)], "{input:?}");
        }
    }
}


//! File-backed centre-column tabs — an editor, not a viewer (P34).
//!
//! The editor's *substance* — load, edit, save, conflict detection, dirty
//! state, language detection, the Markdown formatting operations — lives in
//! [`crate::editor`] as pure synchronous code, tested without a display.
//! This view owns only the pixels and the shell-facing surface:
//!
//! - it loads the path into an [`Editor`] on GPUI's background executor
//!   (the same split the old read-only viewer used: no filesystem work
//!   during render);
//! - it paints the buffer, or the specific failure state (missing vs
//!   unreadable vs binary vs too large — each a distinct message, F-EDIT-13);
//! - it exposes `is_dirty` (the flag F-TAB-16's close confirmation needs),
//!   `conflict`, `check_external`, `save`, `reload` and `keep` for the shell
//!   to drive: save on ⌘S, conflict check on tab activation, close
//!   confirmation when dirty;
//! - the conflict banner, Markdown toolbar, and Code/Preview switch are
//!   rendered here as interactive controls over the model operations.

use gpui::{
    AnyElement, App, BorderStyle, Bounds, Context, CursorStyle, DispatchPhase, Edges, Element,
    ElementId, FocusHandle, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, Rgba, StyledText, Subscription, Task, UnderlineStyle, Window,
    div, point, prelude::*, px, quad, transparent_black,
};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tiller_markdown::{Document, FileSystemEvent, FileSystemEventMonitor, parse};
use tiller_project::resolve_file_link;
use tiller_theme::Theme;

use crate::chat::{Chat, LinkClickOverride};
use crate::editor::{
    Conflict, Editor, Language, LoadStatus, Selection, markdown_links_in_line, word_range_at,
};

/// The rendered-markdown column: the frozen 720px content column (waku
/// `CONTENT_MAX_WIDTH`). Prose sits on the same measured column as the
/// transcript (P32 moved it from an unrecorded 800).
pub(crate) const MARKDOWN_COLUMN_WIDTH: f32 = 720.0;

#[derive(Debug)]
enum ViewState {
    /// The background load has not completed; nothing is known yet.
    Loading,
    /// The editor model is installed and owns the truth about this tab.
    Ready(Editor),
}

/// F-EDIT-01: the two Markdown modes. Code shows the editable source;
/// Preview shows the rendered document. Switching between them is
/// behaviour (tested against the drawn frame); the typography inside each
/// mode is appearance and stays outside the behavior contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownMode {
    Code,
    Preview,
}

/// A tab showing one path. The entity remains owned by the workspace while
/// another tab is active, so switching away never reloads or loses content
/// — and the editor's dirty/conflict state survives tab switches, which is
/// exactly what F-EDIT-05 ("return to Tiller and see the conflict") needs.
pub struct FileView {
    path: PathBuf,
    state: ViewState,
    load_task: Option<Task<()>>,
    file_monitor: Option<FileSystemEventMonitor>,
    _file_monitor_task: Option<Task<()>>,
    /// A user-visible message raised by the shell, such as a failed save.
    notice: Option<String>,
    /// F-EDIT-01: which Markdown mode is active. Only meaningful for
    /// Markdown files; other languages always render as code. A large
    /// Markdown file (preview locked, F-EDIT-03) starts in Code with the
    /// manual-preview notice, and clicking Preview unlocks it.
    markdown_mode: MarkdownMode,
    /// The source range selected by clicking a rendered line. This is the
    /// small, honest selection seam the formatting toolbar needs until the
    /// code surface grows a native text editor.
    source_selection: Option<Selection>,
    /// The insertion point used by the lightweight code surface. The model
    /// owns text mutation; the view owns only this display/input state.
    caret: usize,
    /// The anchor for a shift-extended selection, if one is being built.
    selection_anchor: Option<usize>,
    /// Focus target for the source surface. GPUI sends raw key events to the
    /// focused element, so this is the missing input tier over `Editor`.
    editor_focus: FocusHandle,
    /// Re-check the disk snapshot whenever this tab's editor focus is
    /// regained after another surface owned it.
    focus_subscription: Option<Subscription>,
    /// Whether a mouse-driven selection (F-EDIT-02) is currently being
    /// dragged. Distinct from `selection_anchor.is_some()`, which also
    /// covers a shift+arrow selection that should not keep extending on
    /// an unrelated mouse move.
    dragging: bool,
}

/// Emitted so the shell can act on a gesture that started inside this tab
/// but resolves outside it. Today that is only F-CORE-FILE-04's link click:
/// resolving the click is this view's job, but opening the resulting path
/// is the workspace's (the same `add_file_tab` path the Files panel's
/// "Open" and the chat transcript's link clicks already use).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileViewEvent {
    OpenFile(PathBuf),
}

impl gpui::EventEmitter<FileViewEvent> for FileView {}

impl FileView {
    /// Starts loading `path` without doing filesystem work during render.
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let path_for_task = path.clone();
        let load_task = cx.spawn(async move |this, cx| {
            let editor = cx
                .background_spawn(async move { Editor::open(&path_for_task) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.markdown_mode = Self::initial_markdown_mode(&editor);
                view.state = ViewState::Ready(editor);
                view.load_task = None;
                cx.notify();
            });
        });
        let file_monitor = FileSystemEventMonitor::new(&path).ok();
        let file_monitor_task = file_monitor.as_ref().map(|_| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    if this
                        .update(cx, |view, cx| view.poll_file_system_events(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        Self {
            path,
            state: ViewState::Loading,
            load_task: Some(load_task),
            file_monitor,
            _file_monitor_task: file_monitor_task,
            notice: None,
            markdown_mode: MarkdownMode::Preview,
            source_selection: None,
            caret: 0,
            selection_anchor: None,
            editor_focus: cx.focus_handle().tab_stop(true),
            focus_subscription: None,
            dragging: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The loaded editor, once the background read completes.
    pub fn editor(&self) -> Option<&Editor> {
        match &self.state {
            ViewState::Ready(editor) => Some(editor),
            ViewState::Loading => None,
        }
    }

    /// Mutable editor access for the shell's edit path (a textarea that
    /// does not exist yet — the model ops it needs are all here).
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            ViewState::Ready(editor) => Some(editor),
            ViewState::Loading => None,
        }
    }

    // ── Shell-facing surface ───────────────────────────────────────────

    /// The dirty flag F-TAB-16's close confirmation reads. False while the
    /// file is still loading (nothing to lose yet).
    pub fn is_dirty(&self) -> bool {
        self.editor().is_some_and(Editor::is_dirty)
    }

    /// The current conflict state, or none while loading.
    pub fn conflict(&self) -> Conflict {
        self.editor().map_or(Conflict::None, Editor::conflict)
    }

    pub fn set_notice(&mut self, notice: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(notice.into());
        cx.notify();
    }

    pub fn clear_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        cx.notify();
    }

    /// Re-reads the file and compares it against the last known disk state
    /// (F-EDIT-05/06). The shell calls this when a file tab is activated —
    /// "modify externally, return to Tiller" — and on window focus.
    pub fn check_external(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.check_external();
            cx.notify();
        }
    }

    /// Applies one inotify event from this file's monitor. The monitor is
    /// filtered to the path, but the path check keeps this public seam safe
    /// for host-driven tests and future directory-level monitors.
    pub fn handle_file_system_event(&mut self, event: FileSystemEvent, cx: &mut Context<Self>) {
        if event.path == self.path {
            self.check_external(cx);
        }
    }

    fn poll_file_system_events(&mut self, cx: &mut Context<Self>) {
        let Some(monitor) = &self.file_monitor else {
            return;
        };
        let events = match monitor.poll() {
            Ok(events) => events,
            Err(error) => {
                eprintln!(
                    "[files] watcher failed for {}: {error}",
                    self.path.display()
                );
                return;
            }
        };
        for event in events {
            self.handle_file_system_event(event, cx);
        }
    }

    /// Saves the buffer (F-EDIT-04/06), recreating a deleted file. Errors
    /// are returned and kept visible on the editor.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let result = match self.editor_mut() {
            Some(editor) => editor.save(),
            None => Err("the file is still loading".to_string()),
        };
        cx.notify();
        result
    }

    /// F-EDIT-05 "Reload": adopt the on-disk content.
    pub fn reload(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let result = match self.editor_mut() {
            Some(editor) => editor.reload(),
            None => Err("the file is still loading".to_string()),
        };
        cx.notify();
        result
    }

    /// F-EDIT-05 "Keep": dismiss the banner, keep the buffer.
    pub fn keep(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.keep();
            cx.notify();
        }
    }

    /// F-EDIT-03: unlock the Markdown preview for a large file.
    pub fn request_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.request_preview();
            cx.notify();
        }
    }

    /// A Markdown formatting operation (F-EDIT-02), applied to the current
    /// buffer through the editor model. The toolbar will pass the visible
    /// selection; until a textarea exists this is the shell's entry point.
    pub fn format_markdown(
        &mut self,
        op: MarkdownFormatOp,
        selection: Selection,
        cx: &mut Context<Self>,
    ) -> Option<Selection> {
        let result = self.editor_mut().map(|editor| op.apply(editor, selection));
        if result.is_some() {
            self.source_selection = result;
            if let Some(selection) = result {
                self.caret = selection.end;
                self.selection_anchor = (!selection.is_collapsed()).then_some(selection.start);
            }
        }
        cx.notify();
        result
    }

    // ── F-EDIT-02: real mouse-driven caret placement + drag-select ─────
    //
    // These are the mouse-side counterparts of `move_caret` above: a click
    // starts a collapsed selection at the clicked buffer offset, a drag
    // extends it exactly like Shift+Arrow does, and a double-click snaps to
    // the touched word. Before this, the only way `source_selection` became
    // non-`None` was the keyboard — a click landed formatting at whatever
    // `source_selection` last held (or end-of-buffer), never at the click.

    /// Starts a mouse selection at `position` (a buffer offset). Called on
    /// mouse-down; a following drag calls `extend_mouse_selection`.
    fn begin_mouse_selection(&mut self, position: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let position = position.min(editor.buffer().len());
        self.caret = position;
        self.selection_anchor = Some(position);
        self.source_selection = None;
        self.dragging = true;
        cx.notify();
    }

    /// Extends the in-progress mouse selection to `position`. A no-op once
    /// `end_mouse_selection` has fired, so a stray mouse-move after the
    /// button is released (or a drag that started elsewhere, e.g. on the
    /// toolbar) cannot silently move the caret.
    fn extend_mouse_selection(&mut self, position: usize, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        let Some(editor) = self.editor() else {
            return;
        };
        let buffer = editor.buffer().to_owned();
        self.move_caret(position, true, &buffer);
        cx.notify();
    }

    /// Ends the in-progress mouse selection (mouse-up).
    fn end_mouse_selection(&mut self, cx: &mut Context<Self>) {
        if self.dragging {
            self.dragging = false;
            cx.notify();
        }
    }

    /// Double-click word selection: selects the word touching `position`,
    /// or just places the caret there when it is not on a word.
    fn select_word_at(&mut self, position: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        let buffer = editor.buffer().to_owned();
        let position = position.min(buffer.len());
        match word_range_at(&buffer, position) {
            Some(range) => {
                self.caret = range.end;
                self.selection_anchor = Some(range.start);
                self.source_selection = Selection::new(&buffer, range.start, range.end);
            }
            None => {
                self.caret = position;
                self.selection_anchor = None;
                self.source_selection = None;
            }
        }
        self.dragging = false;
        cx.notify();
    }

    /// F-CORE-FILE-04: resolves a clicked link's raw target against the
    /// directory the open file lives in (Markdown's own relative-link
    /// convention) and, if it resolves to a real path, asks the shell to
    /// open it — the same `FileViewEvent::OpenFile` -> `add_file_tab` path
    /// used by the Files panel's "Open" and the chat transcript's link
    /// clicks (`RightPanelEvent::OpenFile` / `ChatEvent::OpenFile`).
    fn open_markdown_link(&mut self, target: &str, cx: &mut Context<Self>) {
        let base = self.path.parent().unwrap_or_else(|| Path::new("/"));
        if let Some(resolved) = resolve_file_link(target, base) {
            cx.emit(FileViewEvent::OpenFile(resolved.path));
        }
    }

    fn on_editor_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.control && event.keystroke.key == "a" {
            self.select_all();
            cx.notify();
            return;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            // Command chords, including the shell-owned Ctrl-S save path,
            // must continue to resolve outside this raw text-input tier.
            return;
        }

        let Some(editor) = self.editor() else {
            return;
        };
        if editor.status() != &LoadStatus::Loaded || self.effective_mode() != MarkdownMode::Code {
            return;
        }

        let key = event.keystroke.key.as_str();
        let extend = event.keystroke.modifiers.shift;
        match key {
            "home" => self.move_to_line_edge(true, extend),
            "end" => self.move_to_line_edge(false, extend),
            "left" => self.move_horizontal(false, extend),
            "right" => self.move_horizontal(true, extend),
            "backspace" => self.delete_backward(),
            "delete" => self.delete_forward(),
            "enter" | "return" => self.replace_selection("\n"),
            "tab" => self.replace_selection("    "),
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && character != "\n"
                {
                    self.replace_selection(character);
                } else {
                    return;
                }
            }
        }
        cx.notify();
    }

    fn current_selection(&self, editor: &Editor) -> Selection {
        self.source_selection
            .filter(|selection| {
                selection.end <= editor.buffer().len()
                    && editor.buffer().is_char_boundary(selection.start)
                    && editor.buffer().is_char_boundary(selection.end)
            })
            .unwrap_or_else(|| {
                let caret = self.caret.min(editor.buffer().len());
                Selection::point(caret)
            })
    }

    fn move_to_line_edge(&mut self, start: bool, extend: bool) {
        let Some(editor) = self.editor() else {
            return;
        };
        let selection = self.current_selection(editor);
        let buffer = editor.buffer().to_owned();
        let caret = selection.end.min(buffer.len());
        let position = if start {
            buffer[..caret].rfind('\n').map_or(0, |newline| newline + 1)
        } else {
            buffer[caret..]
                .find('\n')
                .map_or(buffer.len(), |newline| caret + newline)
        };
        self.move_caret(position, extend, &buffer);
    }

    fn move_horizontal(&mut self, right: bool, extend: bool) {
        let Some(editor) = self.editor() else {
            return;
        };
        let selection = self.current_selection(editor);
        let buffer = editor.buffer().to_owned();
        let position = if !extend && !selection.is_collapsed() {
            if right {
                selection.end
            } else {
                selection.start
            }
        } else if right {
            next_char_boundary(&buffer, self.caret.max(selection.end))
        } else {
            previous_char_boundary(&buffer, self.caret.min(selection.start))
        };
        self.move_caret(position, extend, &buffer);
    }

    fn move_caret(&mut self, position: usize, extend: bool, buffer: &str) {
        let position = position.min(buffer.len());
        if extend {
            let anchor = self.selection_anchor.unwrap_or(self.caret);
            self.selection_anchor = Some(anchor);
            self.caret = position;
            self.source_selection =
                Selection::new(buffer, anchor.min(position), anchor.max(position))
                    .filter(|selection| !selection.is_collapsed());
        } else {
            self.caret = position;
            self.source_selection = None;
            self.selection_anchor = None;
        }
    }

    fn replace_selection(&mut self, text: &str) {
        let Some(selection) = self.editor().map(|editor| self.current_selection(editor)) else {
            return;
        };
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if editor.replace(selection, text).is_ok() {
            self.caret = selection.start + text.len();
            self.source_selection = None;
            self.selection_anchor = None;
        }
    }

    fn delete_backward(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let selection = self.current_selection(editor);
        let range = if selection.is_collapsed() {
            let start = previous_char_boundary(editor.buffer(), selection.start);
            Selection::new(editor.buffer(), start, selection.start)
        } else {
            Some(selection)
        };
        if let Some(range) = range {
            self.replace_selection_range(range, "");
        }
    }

    fn delete_forward(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let selection = self.current_selection(editor);
        let range = if selection.is_collapsed() {
            let end = next_char_boundary(editor.buffer(), selection.end);
            Selection::new(editor.buffer(), selection.start, end)
        } else {
            Some(selection)
        };
        if let Some(range) = range {
            self.replace_selection_range(range, "");
        }
    }

    fn replace_selection_range(&mut self, selection: Selection, text: &str) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if editor.replace(selection, text).is_ok() {
            self.caret = selection.start + text.len();
            self.source_selection = None;
            self.selection_anchor = None;
        }
    }

    fn select_all(&mut self) {
        if let Some(editor) = self.editor() {
            let buffer = editor.buffer().to_owned();
            self.caret = buffer.len();
            self.source_selection = Selection::new(&buffer, 0, buffer.len())
                .filter(|selection| !selection.is_collapsed());
            self.selection_anchor = Some(0);
        }
    }

    /// The selection the toolbar formats. This mirrors `current_selection`'s
    /// fallback exactly (collapse to the tracked caret) rather than
    /// hard-coding end-of-buffer: a plain click (or an un-shifted arrow
    /// key) leaves `source_selection` `None` by design — that is a real
    /// collapsed caret at `self.caret`, not "no position is known". Before
    /// this fix, *any* collapsed position — including one a real mouse
    /// click had just placed — silently formatted at end-of-buffer instead
    /// (F-EDIT-02).
    fn formatting_selection(&self, editor: &Editor) -> Selection {
        self.source_selection
            .filter(|selection| {
                selection.end <= editor.buffer().len()
                    && editor.buffer().is_char_boundary(selection.start)
                    && editor.buffer().is_char_boundary(selection.end)
            })
            .unwrap_or_else(|| Selection::point(self.caret.min(editor.buffer().len())))
    }

    /// F-EDIT-01: switch the Markdown mode. Selecting Preview on a large
    /// (preview-locked) file unlocks the preview first — the manual-preview
    /// state (F-EDIT-03) is "the preview is not auto-rendered", not "the
    /// preview is forbidden" — so both modes are always reachable.
    pub fn set_markdown_mode(&mut self, mode: MarkdownMode, cx: &mut Context<Self>) {
        self.markdown_mode = mode;
        if mode == MarkdownMode::Preview
            && let Some(editor) = self.editor_mut()
        {
            editor.request_preview();
        }
        cx.notify();
    }

    /// The active Markdown mode (F-EDIT-01).
    pub fn markdown_mode(&self) -> MarkdownMode {
        self.markdown_mode
    }

    /// The mode the surface actually renders: Preview only for a Markdown
    /// file whose preview is not locked. A large Markdown file (F-EDIT-03)
    /// renders source even if Preview was requested, until the unlock
    /// happens — the switcher highlights this effective mode so the mark
    /// never lies about what is on screen.
    fn effective_mode(&self) -> MarkdownMode {
        match self.editor() {
            Some(editor)
                if editor.language() == Language::Markdown
                    && !editor.preview_locked()
                    && self.markdown_mode == MarkdownMode::Preview =>
            {
                MarkdownMode::Preview
            }
            _ => MarkdownMode::Code,
        }
    }

    /// The initial mode once the file is loaded: Preview for an ordinary
    /// Markdown file, Code for everything else (a large Markdown file opens
    /// as source with the manual-preview notice — F-EDIT-03 — and a code
    /// file has nothing to preview).
    fn initial_markdown_mode(editor: &Editor) -> MarkdownMode {
        if editor.language() == Language::Markdown && !editor.preview_locked() {
            MarkdownMode::Preview
        } else {
            MarkdownMode::Code
        }
    }

    /// Whether the Code/Preview switcher should render: only for Markdown
    /// files — a code file has nothing to preview, so offering the switch
    /// would be a lie.
    fn is_markdown(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.language() == Language::Markdown)
    }

    fn render_header(&self, theme: Theme, entity: gpui::Entity<Self>) -> impl IntoElement {
        let language = self.editor().map(|editor| editor.language().name());
        let dirty = self.is_dirty();
        div()
            .w_full()
            .border_b_1()
            .border_color(theme.hairline)
            .px(px(20.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.meta)
            .child(self.path.display().to_string())
            .when(dirty, |this| {
                this.child(
                    div()
                        .text_size(theme.typography.caption2)
                        .text_color(theme.accent)
                        .child("● edited"),
                )
            })
            .when(language.is_some(), |this| {
                this.child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(theme.radii.chip)
                        .bg(theme.raised)
                        .text_color(theme.subtitle)
                        .child(language.unwrap_or_default()),
                )
            })
            .when(self.is_markdown(), |this| {
                this.child(render_mode_switch(self.effective_mode(), theme, entity))
            })
    }

    fn render_state(&self, theme: Theme, entity: gpui::Entity<Self>) -> AnyElement {
        if let Some(message) = &self.notice {
            return notice(message.clone(), theme);
        }

        match &self.state {
            ViewState::Loading => notice("Loading file…", theme),
            ViewState::Ready(editor) => match editor.status() {
                LoadStatus::Loaded => {
                    let conflict = editor.conflict();
                    let mode = self.effective_mode();
                    let selection = self.formatting_selection(editor);
                    let editor_entity = entity.clone();
                    let editor_focus = self.editor_focus.clone();
                    div()
                        .id("file-editor")
                        .key_context("FileEditor")
                        .track_focus(&editor_focus)
                        .focusable()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            editor_entity.update(cx, |view, cx| {
                                view.editor_focus.focus(window, cx);
                            });
                        })
                        .on_key_down({
                            let key_entity = entity.clone();
                            move |event, window, cx| {
                                key_entity.update(cx, |view, cx| {
                                    view.on_editor_key(event, window, cx);
                                });
                            }
                        })
                        .size_full()
                        .flex()
                        .flex_col()
                        .when(conflict != Conflict::None, |this| {
                            this.child(render_conflict_banner(conflict, theme, entity.clone()))
                        })
                        .when(
                            editor.language() == Language::Markdown && mode == MarkdownMode::Code,
                            |this| {
                                this.child(render_markdown_toolbar(
                                    theme,
                                    entity.clone(),
                                    selection,
                                ))
                            },
                        )
                        .child(render_content(
                            editor,
                            mode,
                            theme,
                            entity.clone(),
                            self.source_selection,
                        ))
                        .into_any_element()
                }
                _ => notice(
                    editor
                        .load_message()
                        .unwrap_or_else(|| "Unable to open this file.".to_string()),
                    theme,
                ),
            },
        }
    }
}

impl Render for FileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_subscription.is_none() {
            let editor_focus = self.editor_focus.clone();
            self.focus_subscription =
                Some(cx.on_focus(&editor_focus, window, |view, _window, cx| {
                    view.check_external(cx)
                }));
        }
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.chat_surface)
            .child(self.render_header(theme, entity))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_state(theme, cx.entity())),
            )
    }
}

/// F-EDIT-01: the Code/Preview switcher for Markdown files. Each option is
/// a real button in the drawn frame; clicking it switches the mode through
/// the view entity, and the active option is marked with the same 6%
/// selected-row layer the rest of the app uses — never a colour, because
/// this is selection state, not meaning.
fn render_mode_switch(
    mode: MarkdownMode,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let preview = render_mode_option(
        "Preview",
        "file-mode-preview",
        mode == MarkdownMode::Preview,
        MarkdownMode::Preview,
        entity.clone(),
        theme,
    );
    let code = render_mode_option(
        "Code",
        "file-mode-code",
        mode == MarkdownMode::Code,
        MarkdownMode::Code,
        entity,
        theme,
    );
    div()
        .id("file-mode-switch")
        .debug_selector(|| "file-mode-switch".into())
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(theme.radii.control)
        .bg(theme.raised)
        .child(preview)
        .child(code)
}

fn render_mode_option(
    label: &'static str,
    selector: &'static str,
    active: bool,
    mode: MarkdownMode,
    entity: gpui::Entity<FileView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .px(px(8.0))
        .py(px(2.0))
        .rounded(theme.radii.chip)
        .text_size(theme.typography.caption2)
        .text_color(if active {
            theme.title_selected
        } else {
            theme.meta
        })
        .when(active, |this| this.bg(theme.selected_fill))
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            entity.update(cx, |view, cx| view.set_markdown_mode(mode, cx));
        })
        .child(label)
}

/// The conflict banner (F-EDIT-05/06): a real surface with working
/// resolutions, rendered above the content. Reload and Keep are wired to
/// the view's entity, which resolves them through the editor model; a
/// deleted file offers no Reload (there is nothing to reload) — the message
/// says saving recreates it. Both controls are exercised against the drawn
/// frame and resolve through the existing editor model.
fn render_conflict_banner(
    conflict: Conflict,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let message = match conflict {
        Conflict::ChangedOnDisk => "This file changed on disk.".to_string(),
        Conflict::DeletedOnDisk => "This file was deleted. Saving will recreate it.".to_string(),
        Conflict::None => unreachable!("only rendered for a conflict"),
    };
    let reload_entity = entity.clone();
    let keep_entity = entity;
    div()
        .id("file-conflict-banner")
        .debug_selector(|| "file-conflict-banner".into())
        .w_full()
        .px(px(20.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .bg(theme.danger_soft)
        .border_b_1()
        .border_color(theme.hairline)
        .text_size(theme.typography.footnote)
        .text_color(theme.title)
        .child(div().flex_1().child(message))
        .when(conflict == Conflict::ChangedOnDisk, |this| {
            this.child(
                div()
                    .id("file-conflict-reload")
                    .debug_selector(|| "file-conflict-reload".into())
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        let _ = reload_entity.update(cx, |view, cx| view.reload(cx));
                    })
                    .child("Reload"),
            )
            .child(
                div()
                    .id("file-conflict-keep")
                    .debug_selector(|| "file-conflict-keep".into())
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        keep_entity.update(cx, |view, cx| view.keep(cx));
                    })
                    .child("Keep"),
            )
        })
}

/// The Markdown formatting toolbar (F-EDIT-02). It is deliberately shown in
/// Code mode, where the source line selection is visible; Preview remains a
/// reading surface. The link URL is a deterministic placeholder until the
/// view has a text prompt seam of its own.
fn render_markdown_toolbar(
    theme: Theme,
    entity: gpui::Entity<FileView>,
    selection: Selection,
) -> impl IntoElement {
    div()
        .id("file-format-toolbar")
        .debug_selector(|| "file-format-toolbar".into())
        .w_full()
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .flex()
        .items_center()
        .gap(theme.spacing.titlebar_control_spacing)
        .border_b_1()
        .border_color(theme.hairline)
        .bg(theme.raised)
        .child(render_format_button(
            "B",
            "file-format-bold",
            MarkdownFormatOp::Bold,
            selection,
            entity.clone(),
            theme,
        ))
        .child(render_format_button(
            "I",
            "file-format-italic",
            MarkdownFormatOp::Italic,
            selection,
            entity.clone(),
            theme,
        ))
        .child(render_format_button(
            "H",
            "file-format-heading",
            MarkdownFormatOp::Heading,
            selection,
            entity.clone(),
            theme,
        ))
        .child(render_format_button(
            "List",
            "file-format-list",
            MarkdownFormatOp::List,
            selection,
            entity.clone(),
            theme,
        ))
        .child(render_format_button(
            "Link",
            "file-format-link",
            MarkdownFormatOp::Link {
                url: "https://example.com".to_owned(),
            },
            selection,
            entity,
            theme,
        ))
}

fn render_format_button(
    label: &'static str,
    selector: &'static str,
    operation: MarkdownFormatOp,
    selection: Selection,
    entity: gpui::Entity<FileView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(|| selector.into())
        .min_h(theme.spacing.titlebar_control_frame.height)
        .px(theme.spacing.titlebar_control_spacing)
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(theme.title)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            entity.update(cx, |view, cx| {
                let _ = view.format_markdown(operation.clone(), selection, cx);
            });
        })
        .child(label)
}

fn render_content(
    editor: &Editor,
    mode: MarkdownMode,
    theme: Theme,
    entity: gpui::Entity<FileView>,
    selection: Option<Selection>,
) -> AnyElement {
    let is_markdown = editor.language() == Language::Markdown;
    // Preview renders the parsed document; a locked preview (large file,
    // F-EDIT-03) or Code mode renders the source. The two modes are
    // behaviour — each shows the *right* content — while the rendering
    // inside them is appearance.
    if is_markdown && mode == MarkdownMode::Preview && !editor.preview_locked() {
        let document = markdown_document(editor.path(), editor.buffer())
            .expect("markdown render path implies a markdown document");
        // F-CORE-FILE-04: Preview is the file view's *default* mode, so its
        // rendered links must resolve against the open file's directory the
        // same way the Code-mode + platform-click path does — not fall
        // through to Chat::render_markdown_document's plain cx.open_url,
        // which never routes back to FileViewEvent::OpenFile at all.
        let base = editor
            .path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"));
        let link_entity = entity.clone();
        let link_click: LinkClickOverride =
            Rc::new(
                move |target, _window, cx| match resolve_file_link(target, &base) {
                    Some(resolved) => {
                        link_entity.update(cx, |_, cx| {
                            cx.emit(FileViewEvent::OpenFile(resolved.path));
                        });
                    }
                    None => cx.open_url(target),
                },
            );
        return div()
            .id("file-markdown-scroll")
            .debug_selector(|| "file-markdown-scroll".into())
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(MARKDOWN_COLUMN_WIDTH))
                    .mx_auto()
                    .p(px(24.0))
                    .child(Chat::render_markdown_document_with_link_override(
                        document.clone(),
                        &theme,
                        link_click,
                    )),
            )
            .into_any_element();
    }

    let mut offset = 0;
    let lines: Vec<(usize, String, Selection)> = editor
        .buffer()
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, raw)| {
            let line = raw.strip_suffix('\n').unwrap_or(raw).to_owned();
            let start = offset;
            let end = start + line.len();
            offset += raw.len();
            (
                index,
                line,
                Selection::new(editor.buffer(), start, end).expect("line range is valid"),
            )
        })
        .collect();
    div()
        .id("file-text-scroll")
        .debug_selector(|| "file-text-scroll".into())
        .size_full()
        .overflow_y_scroll()
        .p(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .when(is_markdown && editor.preview_locked(), |this| {
                    // F-EDIT-03: a large Markdown file opens as source with
                    // a manual-preview state until `request_preview`.
                    let preview_entity = entity.clone();
                    this.child(
                        div()
                            .id("file-manual-preview")
                            .debug_selector(|| "file-manual-preview".into())
                            .w_full()
                            .mb(px(8.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .bg(theme.raised)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.subtitle)
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(div().flex_1().child("Large file — manual preview"))
                            .child(
                                div()
                                    .id("file-manual-preview-render")
                                    .debug_selector(|| "file-manual-preview-render".into())
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(theme.radii.control)
                                    .hover(|style| style.bg(theme.row_hover))
                                    .on_click(move |_, _, cx| {
                                        preview_entity.update(cx, |view, cx| {
                                            view.set_markdown_mode(MarkdownMode::Preview, cx);
                                        });
                                    })
                                    .child("Render preview"),
                            ),
                    )
                })
                .font_family(theme.typography.code_family)
                .text_size(theme.typography.code_size)
                .text_color(theme.title)
                .children(lines.iter().map(|(index, line, line_selection)| {
                    div()
                        .id(("file-line", *index))
                        .debug_selector({
                            let selector = format!("file-source-line-{index}");
                            move || selector.clone()
                        })
                        .w_full()
                        .min_h(px(18.0))
                        .flex()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .w(px(52.0))
                                .flex_none()
                                .text_color(theme.meta)
                                .child(format!("{:>5} ", index + 1)),
                        )
                        .child(EditableLine::new(
                            ("file-line-text", *index),
                            line.clone(),
                            line_selection.start,
                            editor.language(),
                            theme,
                            entity.clone(),
                            selection,
                        ))
                })),
        )
        .into_any_element()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeSpanKind {
    Keyword,
    Literal,
    Comment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeSpan {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) kind: CodeSpanKind,
}

/// A deliberately small, dependency-free syntax pass for the editor's code
/// surface. Language detection is not merely a badge: the detected language
/// selects a keyword vocabulary and produces different styled spans. A full
/// parser/highlighter can replace this seam later without changing FileView.
///
/// F-EDIT-07: also reused by `chat.rs`'s Markdown-preview `CodeBlock`
/// rendering, so a fenced code block in a chat transcript gets the same
/// per-token highlighting as the editor's own code surface instead of
/// falling back to flat plain text.
pub(crate) fn code_spans(language: Language, line: &str) -> Vec<CodeSpan> {
    let keywords: &[&str] = match language {
        Language::Rust => &["fn", "let", "mut", "pub", "struct", "impl", "use", "match"],
        Language::Python => &["def", "class", "import", "from", "return", "for", "in"],
        Language::JavaScript | Language::TypeScript => {
            &["const", "let", "function", "return", "import", "from"]
        }
        Language::Shell => &["if", "then", "fi", "for", "in", "do", "done"],
        Language::Go => &["func", "package", "import", "return", "type", "struct"],
        Language::Swift => &["func", "let", "var", "struct", "import", "return"],
        Language::Java | Language::Kotlin | Language::C | Language::Cpp => {
            &["class", "public", "private", "return", "void", "int"]
        }
        Language::PlainText
        | Language::Markdown
        | Language::Json
        | Language::Yaml
        | Language::Toml
        | Language::Ruby
        | Language::Php
        | Language::Html
        | Language::Css
        | Language::Sql
        | Language::Xml
        | Language::Lua
        | Language::Zig => &[],
    };

    let comment_marker = match language {
        Language::Python | Language::Shell | Language::Ruby | Language::Yaml => Some('#'),
        Language::Rust
        | Language::JavaScript
        | Language::TypeScript
        | Language::Go
        | Language::Swift
        | Language::Java
        | Language::Kotlin
        | Language::C
        | Language::Cpp
        | Language::Php
        | Language::Zig => Some('/'),
        _ => None,
    };
    let comment_start = comment_marker.and_then(|marker| {
        let marker = if marker == '/' { "//" } else { "#" };
        line.find(marker)
    });
    let code_end = comment_start.unwrap_or(line.len());
    let mut spans = Vec::new();

    if let Some(start) = comment_start {
        spans.push(CodeSpan {
            range: start..line.len(),
            kind: CodeSpanKind::Comment,
        });
    }

    let mut quote: Option<(char, usize)> = None;
    for (index, character) in line[..code_end].char_indices() {
        match quote {
            Some((open, start)) if open == character => {
                spans.push(CodeSpan {
                    range: start..index + character.len_utf8(),
                    kind: CodeSpanKind::Literal,
                });
                quote = None;
            }
            None if character == '"' || character == '\'' => quote = Some((character, index)),
            _ => {}
        }
    }

    let mut word_start = None;
    for (index, character) in line[..code_end].char_indices() {
        if character.is_alphanumeric() || character == '_' {
            word_start.get_or_insert(index);
        } else if let Some(start) = word_start.take() {
            let word = &line[start..index];
            if keywords.contains(&word) {
                spans.push(CodeSpan {
                    range: start..index,
                    kind: CodeSpanKind::Keyword,
                });
            }
        }
    }
    if let Some(start) = word_start {
        let word = &line[start..code_end];
        if keywords.contains(&word) {
            spans.push(CodeSpan {
                range: start..code_end,
                kind: CodeSpanKind::Keyword,
            });
        }
    }

    spans.sort_by_key(|span| span.range.start);
    let mut non_overlapping = Vec::new();
    for span in spans {
        if non_overlapping
            .last()
            .is_none_or(|previous: &CodeSpan| previous.range.end <= span.range.start)
        {
            non_overlapping.push(span);
        }
    }
    non_overlapping
}

/// One rendered code-surface line: painted text plus the mouse machinery
/// that makes it a real caret/selection surface (F-EDIT-02) instead of the
/// old click-selects-the-whole-line stand-in, and makes a rendered Markdown
/// link a real click target (F-CORE-FILE-04). Modeled on
/// `Chat`'s `TranscriptSelectableText` — same `StyledText` + hitbox +
/// `index_for_position`/`position_for_index` shape — but scoped to a single
/// source line, since each line already carries its own buffer-offset
/// range.
struct EditableLine {
    id: ElementId,
    text: StyledText,
    line_start: usize,
    line_len: usize,
    view: gpui::Entity<FileView>,
    selection: Option<Selection>,
    selection_fill: Rgba,
    /// Byte ranges *local to this line* of clickable Markdown link labels,
    /// paired with their raw (unresolved) target text.
    links: Vec<(Range<usize>, String)>,
    /// Set on mouse-down, consumed on mouse-up: the down position and
    /// whether the platform modifier was held, so a same-position mouse-up
    /// on a link (not a drag) can open it (F-CORE-FILE-04) while a plain
    /// click still only places the caret.
    pressed: std::rc::Rc<std::cell::Cell<Option<(usize, bool)>>>,
}

impl EditableLine {
    fn new(
        id: impl Into<ElementId>,
        line: String,
        line_start: usize,
        language: Language,
        theme: Theme,
        view: gpui::Entity<FileView>,
        selection: Option<Selection>,
    ) -> Self {
        let line_len = line.len();
        let links: Vec<(Range<usize>, String)> = if language == Language::Markdown {
            markdown_links_in_line(&line)
                .into_iter()
                .map(|span| (span.label_range, span.target))
                .collect()
        } else {
            Vec::new()
        };
        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = code_spans(language, &line)
            .into_iter()
            .map(|span| {
                let color = match span.kind {
                    CodeSpanKind::Keyword => theme.accent,
                    CodeSpanKind::Literal => theme.diff_addition,
                    CodeSpanKind::Comment => theme.meta,
                };
                (
                    span.range,
                    HighlightStyle {
                        color: Some(color.into()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        highlights.extend(links.iter().map(|(range, _)| {
            (
                range.clone(),
                HighlightStyle {
                    color: Some(theme.accent.into()),
                    underline: Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme.accent.into()),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            )
        }));
        // `code_spans` only ever fires for non-Markdown languages and
        // `links` only for Markdown, so the two sets never overlap — this
        // only needs a stable sort for `StyledText::with_highlights`, which
        // expects highlights in range order.
        highlights.sort_by_key(|(range, _)| range.start);
        let text = StyledText::new(line).with_highlights(highlights);
        Self {
            id: id.into(),
            text,
            line_start,
            line_len,
            view,
            selection,
            selection_fill: theme.selected_fill,
            links,
            pressed: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// Paints the part of `self.selection` that falls on this line, clipped
    /// to this line's own bounds. Each line computes its own overlap
    /// independently, so a selection spanning several lines highlights the
    /// exact selected run on the first/last line and the full width of any
    /// line fully inside it — never a whole line that is only partly
    /// selected, which is what the old line-level background did.
    fn paint_selection(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let Some(selection) = self.selection.filter(|selection| !selection.is_collapsed()) else {
            return;
        };
        let line_end = self.line_start + self.line_len;
        let start = selection.start.max(self.line_start);
        let end = selection.end.min(line_end);
        if start >= end {
            return;
        }
        let layout = self.text.layout();
        let line_height = layout.line_height();
        let start_position = layout
            .position_for_index(start - self.line_start)
            .unwrap_or(bounds.origin);
        let end_position = layout
            .position_for_index(end - self.line_start)
            .unwrap_or(gpui::point(bounds.right(), bounds.origin.y));
        if end_position.x <= start_position.x {
            return;
        }
        window.paint_quad(quad(
            Bounds::from_corners(
                point(start_position.x, bounds.origin.y),
                point(end_position.x, bounds.origin.y + line_height),
            ),
            px(0.0),
            self.selection_fill,
            Edges::default(),
            transparent_black(),
            BorderStyle::default(),
        ));
    }
}

impl Element for EditableLine {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, state, window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paint_selection(bounds, window);
        window.set_cursor_style(CursorStyle::IBeam, hitbox);

        let layout = self.text.layout().clone();
        let line_start = self.line_start;
        let line_len = self.line_len;
        let view = self.view.clone();
        let pressed = self.pressed.clone();
        let hitbox_for_down = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox_for_down.is_hovered(window)
            {
                let local = match layout.index_for_position(event.position) {
                    Ok(index) | Err(index) => index.min(line_len),
                };
                let global = line_start + local;
                pressed.set(Some((global, event.modifiers.platform)));
                if event.click_count >= 2 {
                    view.update(cx, |view, cx| view.select_word_at(global, cx));
                } else {
                    view.update(cx, |view, cx| view.begin_mouse_selection(global, cx));
                }
                window.prevent_default();
            }
        });

        let layout = self.text.layout().clone();
        let line_start = self.line_start;
        let line_len = self.line_len;
        let view = self.view.clone();
        let hitbox_for_move = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.dragging()
                && hitbox_for_move.is_hovered(window)
            {
                let local = match layout.index_for_position(event.position) {
                    Ok(index) | Err(index) => index.min(line_len),
                };
                view.update(cx, |view, cx| {
                    view.extend_mouse_selection(line_start + local, cx)
                });
            }
        });

        let layout = self.text.layout().clone();
        let line_start = self.line_start;
        let line_len = self.line_len;
        let view = self.view.clone();
        let links = self.links.clone();
        let pressed_for_up = self.pressed.clone();
        let hitbox_for_up = hitbox.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox_for_up.is_hovered(window)
            {
                if let Some((down_global, down_platform)) = pressed_for_up.take() {
                    let local = match layout.index_for_position(event.position) {
                        Ok(index) | Err(index) => index.min(line_len),
                    };
                    let up_global = line_start + local;
                    let clicked_in_place = down_global == up_global;
                    let platform_held = down_platform || event.modifiers.platform;
                    if clicked_in_place && platform_held {
                        let local_click = up_global - line_start;
                        if let Some((_, target)) =
                            links.iter().find(|(range, _)| range.contains(&local_click))
                        {
                            let target = target.clone();
                            view.update(cx, |view, cx| view.open_markdown_link(&target, cx));
                        }
                    }
                }
                view.update(cx, |view, cx| view.end_mouse_selection(cx));
                window.prevent_default();
            }
        });

        self.text
            .paint(id, inspector_id, bounds, state, &mut (), window, cx);
    }
}

impl IntoElement for EditableLine {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The Markdown formatting operations the toolbar offers (F-EDIT-02),
/// expressed as a command so the shell can route them generically. The
/// implementations are the pure text transformations on
/// [`crate::editor::Editor`]. The URL for a link comes from the toolbar's
/// own prompting; the command only carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownFormatOp {
    Bold,
    Italic,
    Heading,
    List,
    Link { url: String },
}

impl MarkdownFormatOp {
    fn apply(self, editor: &mut Editor, selection: Selection) -> Selection {
        match self {
            MarkdownFormatOp::Bold => editor.format_bold(selection),
            MarkdownFormatOp::Italic => editor.format_italic(selection),
            MarkdownFormatOp::Heading => editor.toggle_heading(selection),
            MarkdownFormatOp::List => editor.toggle_list(selection),
            MarkdownFormatOp::Link { url } => editor.make_link(selection, &url),
        }
    }
}

fn notice(message: impl Into<String>, theme: Theme) -> AnyElement {
    div()
        .id("file-view-notice")
        .debug_selector(|| "file-view-notice".into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .text_size(theme.typography.headline)
        .text_color(theme.subtitle)
        .child(message.into())
        .into_any_element()
}

/// Renders `text` as a Markdown document when the path is Markdown, for the
/// preview half of the surface (F-EDIT-01's Code/Preview split is display-
/// bound; the parse is not).
fn markdown_document(path: &Path, text: &str) -> Option<Document> {
    if Language::from_path(path) == Language::Markdown {
        Some(parse(text))
    } else {
        None
    }
}

fn previous_char_boundary(buffer: &str, position: usize) -> usize {
    buffer[..position.min(buffer.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_char_boundary(buffer: &str, position: usize) -> usize {
    let position = position.min(buffer.len());
    buffer[position..]
        .chars()
        .next()
        .map_or(position, |character| position + character.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, VisualTestContext};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tiller_markdown::{Block, Inline};

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-file-view-{name}-{}-{id}",
                std::process::id()
            ));
            std::fs::write(&path, contents).expect("write temporary file");
            Self(path)
        }

        /// Like [`TempFile::new`] but the on-disk name ends with
        /// `extension`, so language detection sees it.
        fn with_extension(extension: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-file-view-{}-{id}.{extension}",
                std::process::id()
            ));
            std::fs::write(&path, contents).expect("write temporary file");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn markdown_loads_into_expected_blocks() {
        let file = TempFile::with_extension("md", "# Title\n\nHello **world**.");
        let document = markdown_document(&file.0, "# Title\n\nHello **world**.")
            .expect("markdown parses into a document");
        assert_eq!(
            document.blocks,
            vec![
                Block::Heading {
                    level: 1,
                    inline: vec![Inline::Text("Title".into())],
                },
                Block::Paragraph {
                    inline: vec![
                        Inline::Text("Hello ".into()),
                        Inline::Strong(vec![Inline::Text("world".into())]),
                        Inline::Text(".".into()),
                    ],
                },
            ]
        );
    }

    #[test]
    fn plain_text_never_renders_as_markdown() {
        let path = Path::new("/tmp/note.txt");
        assert!(
            markdown_document(path, "hello").is_none(),
            "only Markdown paths render as Markdown"
        );
    }

    #[test]
    fn different_languages_produce_different_code_spans() {
        let rust = code_spans(Language::Rust, "fn main() { let value = \"rust\"; }");
        let python = code_spans(Language::Python, "def main():\n    return 'python'");

        assert!(
            rust.iter()
                .any(|span| { span.kind == CodeSpanKind::Keyword && span.range == (0..2) })
        );
        assert!(
            python
                .iter()
                .any(|span| { span.kind == CodeSpanKind::Keyword && span.range == (0..3) })
        );
        assert!(rust.iter().any(|span| span.kind == CodeSpanKind::Literal));
        assert!(python.iter().any(|span| span.kind == CodeSpanKind::Literal));
        assert_ne!(
            rust, python,
            "language detection must select different spans"
        );
    }

    // ── The shell-facing surface, through the actual view ──────────────

    #[gpui::test]
    async fn dirty_state_is_reachable_through_the_view(cx: &mut gpui::TestAppContext) {
        let file = TempFile::new("dirty", "hello\n");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Loaded and clean: the F-TAB-16 flag is false before any edit.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(!view.is_dirty(), "a freshly opened file is clean");
            assert_eq!(view.conflict(), Conflict::None);
        });

        // Edit through the view's editor: the flag the shell's close path
        // reads becomes true. This is the wiring F-TAB-16 needs.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| {
                let editor = view.editor_mut().expect("editor loaded");
                let selection = Selection::point(editor.buffer().len());
                editor.insert(selection.start, "more\n").expect("insert");
                cx.notify();
            });
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(view.is_dirty(), "an edit makes the document dirty");
        });

        // Save through the view: the flag clears, matching the disk.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.save(cx).expect("save"));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(!view.is_dirty(), "saving clears the dirty flag");
        });
        assert_eq!(
            std::fs::read_to_string(file.path()).expect("read back"),
            "hello\nmore\n",
            "the view's save wrote the buffer to disk"
        );
    }

    #[gpui::test]
    async fn keyboard_input_edits_the_buffer_through_the_focused_file_view(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("keyboard-input", "hello\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        let line = cx
            .debug_bounds("file-source-line-0")
            .expect("the source line is drawn");
        cx.simulate_click(line.center(), Modifiers::none());
        cx.simulate_keystrokes("home");
        cx.simulate_input("typed ");
        cx.simulate_keystrokes("end");
        cx.simulate_input(" end");
        cx.simulate_keystrokes("home shift-end");
        cx.simulate_input("replacement");
        cx.simulate_keystrokes("end enter");
        cx.simulate_input("next");
        cx.simulate_keystrokes("home delete end backspace");

        let buffer = cx.update(|window, cx| {
            window
                .root::<FileView>()
                .flatten()
                .expect("file view root")
                .read(cx)
                .editor()
                .expect("editor loaded")
                .buffer()
                .to_owned()
        });
        assert_eq!(buffer, "replacement\nex\n");
        assert!(cx.update(|window, cx| {
            window
                .root::<FileView>()
                .flatten()
                .expect("file view root")
                .read(cx)
                .is_dirty()
        }));
    }

    #[gpui::test]
    async fn conflict_detection_and_resolutions_work_through_the_view(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("view-conflict", "original\n");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // External mutation while the tab is open, then the shell's
        // activation hook fires check_external.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| {
                let editor = view.editor_mut().expect("editor loaded");
                let at = editor.buffer().len();
                editor.insert(at, "user edit\n").expect("local edit");
                cx.notify();
            });
        });
        std::fs::write(file.path(), "external\n").expect("external write");
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.check_external(cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("file-conflict-banner").is_some());
        assert!(cx.debug_bounds("file-conflict-reload").is_some());
        assert!(cx.debug_bounds("file-conflict-keep").is_some());
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::ChangedOnDisk);
        });

        // Reload adopts the disk content and clears the flag through the
        // drawn control, not just by calling the view method directly.
        let reload = cx
            .debug_bounds("file-conflict-reload")
            .expect("Reload is drawn");
        cx.simulate_click(reload.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::None);
            assert_eq!(
                view.editor().expect("editor").buffer(),
                "external\n",
                "Reload adopted the on-disk content"
            );
        });

        // Keep keeps a new local edit and leaves the tab dirty until saved.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| {
                let editor = view.editor_mut().expect("editor loaded");
                let at = editor.buffer().len();
                editor.insert(at, "kept local\n").expect("local edit");
                cx.notify();
            });
        });
        std::fs::write(file.path(), "second external\n").expect("external write");
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.check_external(cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let keep = cx
            .debug_bounds("file-conflict-keep")
            .expect("Keep is drawn");
        cx.simulate_click(keep.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::None);
            assert_eq!(
                view.editor().expect("editor").buffer(),
                "external\nkept local\n",
                "Keep preserved the buffer"
            );
            assert!(view.is_dirty(), "Keep leaves the tab dirty until saved");
        });
    }

    #[gpui::test]
    async fn filesystem_events_reload_clean_content_and_surface_rename_conflicts(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("watcher", "original\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        std::fs::write(file.path(), "external\n").expect("external write");
        for _ in 0..40 {
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_millis(100));
            cx.cx.run_until_parked();
            let updated = view.read_with(&cx.cx, |view, _| {
                view.editor()
                    .is_some_and(|editor| editor.buffer() == "external\n")
            });
            if updated {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            view.read_with(&cx.cx, |view, _| {
                view.editor().expect("editor loaded").buffer().to_owned()
            }),
            "external\n",
            "a clean editor adopts a watcher-delivered external write"
        );

        view.update(&mut cx.cx, |view, cx| {
            let editor = view.editor_mut().expect("editor loaded");
            editor
                .insert(editor.buffer().len(), "local\n")
                .expect("local edit");
            cx.notify();
        });
        std::fs::remove_file(file.path()).expect("external rename/delete");
        view.update(&mut cx.cx, |view, cx| {
            view.handle_file_system_event(
                tiller_markdown::FileSystemEvent {
                    path: file.path().to_path_buf(),
                    kind: tiller_markdown::FileEventKind::Renamed,
                },
                cx,
            );
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.conflict()),
            Conflict::DeletedOnDisk,
            "a watcher rename event surfaces the missing file conflict"
        );
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
            window.simulate_next_frame(app);
        });
        assert!(cx.debug_bounds("file-conflict-banner").is_some());
    }

    #[gpui::test]
    async fn a_missing_file_tab_reports_the_specific_state(cx: &mut gpui::TestAppContext) {
        let missing =
            std::env::temp_dir().join(format!("tiller-file-view-missing-{}", std::process::id()));

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(missing.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            let editor = view.editor().expect("editor installed");
            assert_eq!(editor.status(), &LoadStatus::Missing);
            let message = editor.load_message().expect("message");
            assert!(
                message.contains("does not exist"),
                "missing files get the missing message: {message}"
            );
            assert!(!view.is_dirty(), "a missing file is not dirty");
        });
    }

    #[gpui::test]
    async fn a_file_notice_is_drawn_and_can_be_cleared(cx: &mut gpui::TestAppContext) {
        let file = TempFile::new("notice", "hello\n");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.set_notice("save failed", cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-notice").is_some(),
            "a raised notice is drawn in the file view"
        );

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.clear_notice(cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-notice").is_none(),
            "clearing the notice removes it from the drawn frame"
        );
    }

    // ── F-EDIT-01: Code and Preview modes are behaviour ────────────────
    //
    // The drawn frame finds the real switcher, real clicks switch the mode,
    // and each mode renders the content that belongs to it — rendered
    // Markdown in Preview, the numbered source in Code. The typography
    // inside either mode is appearance and is not asserted.

    /// Mounts a `FileView` in a drawn window and pumps until its background
    /// load has actually installed the editor — the same hardened pump the
    /// drawn git tests use, so the assertions below run against a loaded
    /// editor, never on timing luck.
    fn mounted_file_view(
        cx: &mut gpui::TestAppContext,
        path: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<FileView>) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(path, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
            let loaded = cx.update(|window, app| {
                window
                    .root::<FileView>()
                    .flatten()
                    .is_some_and(|view| view.read(app).editor().is_some())
            });
            if loaded {
                break;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        let entity =
            cx.update(|window, _| window.root::<FileView>().flatten().expect("file view root"));
        assert!(
            entity.read_with(&cx.cx, |view, _| view.editor().is_some()),
            "the background load must install the editor before the test proceeds"
        );
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        (cx, entity)
    }

    #[gpui::test]
    async fn markdown_preview_and_code_modes_switch_in_the_drawn_frame(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("md", "# Title\n\nHello **world**.\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // A Markdown file opens in Preview: the rendered document is on
        // screen and the source is not.
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "Preview mode renders the Markdown document"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "Preview mode does not render the source"
        );
        assert!(
            cx.debug_bounds("file-mode-preview").is_some()
                && cx.debug_bounds("file-mode-code").is_some(),
            "the Code/Preview switcher is in the drawn frame"
        );

        // Click Code: the source replaces the rendered document.
        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code option is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "Code mode renders the numbered source"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "Code mode hides the rendered document"
        );
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.markdown_mode(), MarkdownMode::Code);
        });

        // Click Preview again: the rendered document returns.
        let preview = cx
            .debug_bounds("file-mode-preview")
            .expect("the Preview option is drawn");
        cx.simulate_click(preview.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "clicking Preview restores the rendered document"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "the source is hidden again in Preview"
        );
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.markdown_mode(), MarkdownMode::Preview);
        });
    }

    #[gpui::test]
    async fn a_code_file_has_no_mode_switch_and_always_renders_source(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "a code file renders its source"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "a code file never renders Markdown"
        );
        assert!(
            cx.debug_bounds("file-mode-preview").is_none()
                && cx.debug_bounds("file-mode-code").is_none(),
            "a code file has nothing to preview, so no switcher is offered"
        );
    }

    #[gpui::test]
    async fn a_large_markdown_file_opens_in_code_with_manual_preview_and_unlocks(
        cx: &mut gpui::TestAppContext,
    ) {
        // Over the preview threshold but under the load limit: F-EDIT-03's
        // manual-preview state, now with a working way out.
        let big = "x".repeat(256 * 1024 + 100);
        let file = TempFile::with_extension("md", &big);
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // The large file opens as source with the manual-preview notice.
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "a large Markdown file opens in Code"
        );
        assert!(
            cx.debug_bounds("file-manual-preview").is_some(),
            "the manual-preview notice is shown (F-EDIT-03)"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "the preview is not auto-rendered for a large file"
        );

        // Clicking the notice unlocks it: the manual state is "not
        // automatic", not "forbidden".
        let render_preview = cx
            .debug_bounds("file-manual-preview-render")
            .expect("the manual preview control is drawn");
        cx.simulate_click(render_preview.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "Preview on a locked file unlocks the preview and renders it"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "the source is replaced once the preview is unlocked"
        );
    }

    #[gpui::test]
    async fn markdown_toolbar_controls_change_the_selected_source(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("md", "word\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // Formatting belongs to Code mode. The source line is the real
        // selection seam until a richer text editor supplies native ranges.
        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code option is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let line = cx
            .debug_bounds("file-source-line-0")
            .expect("the source line is drawn");
        cx.simulate_click(line.center(), Modifiers::none());
        cx.run_until_parked();

        for selector in [
            "file-format-bold",
            "file-format-italic",
            "file-format-heading",
            "file-format-list",
            "file-format-link",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "the Markdown toolbar draws {selector}"
            );
        }

        let mut previous = String::from("word\n");
        for selector in [
            "file-format-bold",
            "file-format-italic",
            "file-format-heading",
            "file-format-list",
            "file-format-link",
        ] {
            let button = cx
                .debug_bounds(selector)
                .expect("formatting control remains in the drawn frame");
            cx.simulate_click(button.center(), Modifiers::none());
            cx.run_until_parked();
            let current = cx.update(|window, cx| {
                window
                    .root::<FileView>()
                    .flatten()
                    .expect("file view root")
                    .read(cx)
                    .editor()
                    .expect("editor loaded")
                    .buffer()
                    .to_owned()
            });
            assert_ne!(current, previous, "{selector} changes Markdown source");
            previous = current;
        }
    }

    // ── F-EDIT-02: real mouse-driven caret placement + drag-select ─────

    #[gpui::test]
    async fn mouse_click_formats_the_clicked_line_not_end_of_buffer(cx: &mut gpui::TestAppContext) {
        // Two lines: a short first line and a much longer second one. The
        // old defect fell back to `Selection::point(buffer.len())` on any
        // mouse click, which lands inside the *second* line here — so a
        // click on line 0 followed by Bold proves the fix only if the
        // markers show up in line 0, not at the very end of the buffer.
        let file = TempFile::with_extension("md", "ab\nsecond line is much longer\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code option is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let line0 = cx
            .debug_bounds("file-source-line-0")
            .expect("line 0 is drawn");
        // A couple of pixels past the 52px line-number gutter: inside line
        // 0's own rendered text for any non-empty first line, regardless of
        // the exact glyph metrics.
        cx.simulate_click(
            point(line0.left() + px(53.0), line0.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();

        let bold = cx
            .debug_bounds("file-format-bold")
            .expect("the Bold control is drawn");
        cx.simulate_click(bold.center(), Modifiers::none());
        cx.run_until_parked();

        let buffer = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor loaded").buffer().to_owned()
        });
        let first_line = buffer.split('\n').next().unwrap_or_default();
        assert!(
            first_line.contains("**"),
            "clicking line 0 then Bold must format there, not fall back to \
             end-of-buffer: buffer was {buffer:?}"
        );
    }

    #[gpui::test]
    async fn drag_selection_produces_the_dragged_range_not_a_whole_line(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("md", "hello world\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        // Mirrors what the drawn `EditableLine` element does on
        // down/move/up: it resolves a pixel to a buffer offset and calls
        // exactly these three methods, which is the state machine actually
        // under test here (the pixel->offset half is GPUI's own
        // `index_for_position`, already exercised by `Chat`'s identical use
        // of it elsewhere in this crate).
        view.update(&mut cx.cx, |view, cx| {
            view.begin_mouse_selection(2, cx); // inside "hello"
            view.extend_mouse_selection(4, cx); // still inside "hello"
            view.end_mouse_selection(cx);
        });

        let (selection, dragging) =
            view.read_with(&cx.cx, |view, _| (view.source_selection, view.dragging));
        assert_eq!(
            selection,
            Selection::new("hello world\n", 2, 4),
            "the selection is exactly the dragged range"
        );
        assert!(!dragging, "mouse-up ends the drag");

        // And formatting acts on that exact range, not the whole line.
        view.update(&mut cx.cx, |view, cx| {
            let selection = selection.expect("drag left a real selection");
            view.format_markdown(MarkdownFormatOp::Bold, selection, cx);
        });
        let buffer = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor loaded").buffer().to_owned()
        });
        assert_eq!(buffer, "he**ll**o world\n");
    }

    #[gpui::test]
    async fn double_click_selects_the_touched_word_not_the_whole_line(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("md", "hello world\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        view.update(&mut cx.cx, |view, cx| {
            view.select_word_at(8, cx); // inside "world" (offsets 6..11)
        });
        let selection = view.read_with(&cx.cx, |view, _| view.source_selection);
        assert_eq!(
            selection,
            Selection::new("hello world\n", 6, 11),
            "double-click selects the word, not the whole line"
        );

        view.update(&mut cx.cx, |view, cx| {
            view.format_markdown(MarkdownFormatOp::Italic, selection.unwrap(), cx);
        });
        let buffer = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor loaded").buffer().to_owned()
        });
        assert_eq!(
            buffer, "hello *world*\n",
            "Italic wraps the double-clicked word, not the whole line"
        );
    }

    // ── F-CORE-FILE-04: a rendered link is a real click target ─────────

    #[gpui::test]
    async fn platform_click_on_a_link_label_resolves_and_emits_open_file(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "tiller-file-view-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create worktree dir");
        let note = dir.join("note.md");
        std::fs::write(&note, "see [setup](setup.md) for details\n").expect("write file");
        std::fs::write(dir.join("setup.md"), "# setup\n").expect("write target file");

        let (mut cx, view) = mounted_file_view(cx, note.clone());

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        view.update(&mut cx.cx, |view, cx| {
            view.open_markdown_link("setup.md", cx);
        });

        assert_eq!(
            events.borrow().as_slice(),
            [FileViewEvent::OpenFile(dir.join("setup.md"))],
            "a relative link resolves against the open file's directory and \
             asks the shell to open it, the same OpenFile path RightPanel \
             and Chat already use"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression for F-CORE-FILE-04: the previous test above only proved
    /// `open_markdown_link` resolves correctly — it never proved the
    /// *rendered* link in Preview (the file view's default mode) actually
    /// calls it. Preview renders through `Chat::render_inline`, whose
    /// on_click used to call `cx.open_url` unconditionally, so a real click
    /// on a real rendered link in Preview never emitted `FileViewEvent`.
    /// This test drives an actual click through the drawn frame, the same
    /// gesture wayland-drive used live.
    #[gpui::test]
    async fn clicking_a_rendered_link_in_preview_mode_emits_open_file(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "tiller-file-view-preview-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create worktree dir");
        let note = dir.join("note.md");
        // The whole first (only) paragraph is the link, so a click near the
        // top-left of the rendered document — inside the padded content
        // column, no text-layout math required — always lands on it.
        std::fs::write(&note, "[setup](setup.md)\n").expect("write file");
        std::fs::write(dir.join("setup.md"), "# setup\n").expect("write target file");

        let (mut cx, view) = mounted_file_view(cx, note.clone());
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.markdown_mode()),
            MarkdownMode::Preview,
            "Markdown opens in Preview by default"
        );

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let scroll = cx
            .debug_bounds("file-markdown-scroll")
            .expect("Preview renders the markdown document");
        // The rendered content sits in a `max_w(MARKDOWN_COLUMN_WIDTH)`
        // column, `mx_auto`-centered inside the (wider, maximized-test-
        // window) scroll container, with 24px of its own padding — account
        // for both so the click lands on the link glyphs themselves rather
        // than on the container's own left edge.
        let column_left =
            scroll.origin.x + ((scroll.size.width - px(MARKDOWN_COLUMN_WIDTH)) / 2.0).max(px(0.0));
        let click_point = gpui::point(column_left + px(30.0), scroll.origin.y + px(30.0));
        cx.simulate_click(click_point, Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            [FileViewEvent::OpenFile(dir.join("setup.md"))],
            "a real click on the rendered Preview link must resolve and \
             emit FileViewEvent::OpenFile, not fall through to cx.open_url"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

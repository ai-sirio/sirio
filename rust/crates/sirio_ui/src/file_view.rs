//! File-backed centre-column tabs — an editor, not a viewer (P34).
//!
//! The editor's I/O substance — load, save, conflict detection, dirty state,
//! and language detection — lives in [`crate::editor`] as pure synchronous
//! code, tested without a display. This view owns the pixels and the
//! shell-facing surface: Sirio's no-wrap source editor with bezel-syntax
//! highlighting, plus the rendered Markdown preview.
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

use bezel::motion::Painter;
use bezel::ui::scroll::{self, ScrollbarState};
use gpui::{
    AnyElement, App, BorderStyle, Bounds, Context, CursorStyle, DispatchPhase, Edges, Element,
    ElementId, FocusHandle, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, KeyDownEvent, LayoutId, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Rgba, ScrollHandle,
    StyledText, Subscription, Task, UnderlineStyle, UniformListScrollHandle, Window, anchored,
    canvas, deferred, div, point, prelude::*, px, quad, size, transparent_black, uniform_list,
};
use sirio_markdown::{Document, FileSystemEvent, FileSystemEventMonitor, parse};
use sirio_project::{display_absolute_path, resolve_file_link};
use sirio_theme::Theme;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::caret;
use crate::chat::{Chat, LinkClickOverride};
use crate::editor::{
    Conflict, Editor, Language, LoadStatus, Selection, markdown_links_in_line, word_range_at,
};
use crate::file_context_menu::{self, FileContextAction, FileContextFacts};
use crate::loading;

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
/// exactly what F-EDIT-05 ("return to Sirio and see the conflict") needs.
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
    /// The source range selected in the custom code editor.
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
    /// Blink state of the source surface's insertion caret, and the
    /// (caret, selection) signature it was last rendered against — a
    /// changed signature means the user moved/edited, so the bar wakes
    /// instead of blinking off mid-interaction.
    editor_blink: caret::Blink,
    editor_caret_sig: (usize, Option<(usize, usize)>),
    /// Whether the last drawn frame painted the insertion bar — the same
    /// `field_caret_visible`/`modal_caret_visible` shape the other editable
    /// surfaces keep, so the blink is observable without reading pixels.
    editor_caret_visible: bool,
    /// Scroll state of the two scrolling surfaces, so each can wear bezel's
    /// bar.
    scroll: SurfaceScroll,
    /// The window-absolute point of the right-click that opened the context
    /// menu, `None` when it is closed. Window-absolute because
    /// `anchored().position()` takes a window coordinate as-is (P129).
    context_menu: Option<Point<Pixels>>,
    /// Facts only the workspace can answer — whether this file is in a git
    /// repository, whether that repository has a GitHub remote, whether this
    /// worktree has an agent chat open. Each costs a subprocess or a walk of
    /// the open tabs, so they are pushed down rather than asked per click.
    shell_facts: FileContextFacts,
    /// Findings published by the language server for this file, in buffer
    /// terms. Replaced wholesale on every publish — an empty list is how a
    /// server says the errors are gone, so it must clear, not be ignored.
    diagnostics: Vec<FileDiagnostic>,
    /// The offset the pointer is resting on, the frame point it rests at,
    /// and the timer that will turn that rest into a question. Replacing
    /// the `Task` cancels it, which is the whole of dwell cancellation.
    hover_offset: Option<usize>,
    hover_point: Point<Pixels>,
    hover_task: Option<Task<()>>,
    /// Monotonic, bumped on every dwell. A reply carrying an older number
    /// is an answer about a place the pointer has left, and is dropped.
    /// Without this the card appears where the cursor no longer is.
    hover_seq: u64,
    hover_card: Option<String>,
    /// A line to scroll to as soon as there is something to scroll. Held
    /// rather than applied because a tab opened by "go to definition" is
    /// still loading its content: revealing immediately addresses lines
    /// that do not exist yet, and does nothing at all.
    pending_reveal: Option<usize>,
}

/// The scroll handles of the source list and the Markdown preview, plus the
/// bar state each bezel scrollbar carries its drag in. Tracked because an
/// untracked surface scrolls just as well but reports no viewport and no
/// overflow — a bar with nothing to draw from. Held by the view, never
/// rebuilt per frame: the handle *is* the scroll position across frames.
struct SurfaceScroll {
    source: UniformListScrollHandle,
    source_bar: ScrollbarState,
    preview: ScrollHandle,
    preview_bar: ScrollbarState,
}

impl SurfaceScroll {
    fn new(painter: Painter) -> Self {
        Self {
            source: UniformListScrollHandle::new(),
            source_bar: ScrollbarState::new(painter),
            preview: ScrollHandle::new(),
            preview_bar: ScrollbarState::new(painter),
        }
    }
}

/// Emitted so the shell can act on a gesture that started inside this tab
/// but resolves outside it. Today that is only F-CORE-FILE-04's link click:
/// resolving the click is this view's job, but opening the resulting path
/// is the workspace's (the same `add_file_tab` path the Files panel's
/// "Open" and the chat transcript's link clicks already use).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileViewEvent {
    OpenFile(PathBuf),
    /// The background read landed and this view now has a buffer.
    ///
    /// Emitted because the shell cannot tell from the outside: a `FileView`
    /// is `ViewState::Loading` for its first turns, and anything that reads
    /// `editor()` before this arrives sees `None`. Telling a language server
    /// about a file at that moment describes it as empty.
    Loaded(PathBuf),
    SendSelectionToAgent {
        path: PathBuf,
        lines: (usize, usize),
        text: String,
        language: &'static str,
    },
    RevealInFileManager(PathBuf),
    OpenInTerminal(PathBuf),
    CopyPermalink {
        path: PathBuf,
        lines: (usize, usize),
    },
    ViewFileHistory(PathBuf),
    Hover { path: PathBuf, offset: usize, seq: u64 },
    GoToDefinition { path: PathBuf, offset: usize },
    FindReferences { path: PathBuf, offset: usize },
}

impl gpui::EventEmitter<FileViewEvent> for FileView {}

/// How bad a diagnostic is, in the only terms the view needs. Ordered
/// worst-first so `min()` over a line picks the mark to paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// One finding, already in this view's own terms: a line index and a byte
/// range into the buffer. The conversion from the protocol's UTF-16
/// positions happens in the app, which owns the buffer — this crate never
/// learns what a UTF-16 code unit is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiagnostic {
    pub line: usize,
    pub range: std::ops::Range<usize>,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// Installs bezel-editor's key bindings for the application. The app crate
/// calls through this module so the dependency remains owned by `sirio_ui`.
pub fn init(cx: &mut App) {
    ::editor::init(cx);
}

const HOVER_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

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
                view.apply_pending_reveal(cx);
                cx.emit(FileViewEvent::Loaded(view.path.clone()));
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
            editor_blink: caret::Blink::new(),
            editor_caret_sig: (0, None),
            editor_caret_visible: false,
            scroll: SurfaceScroll::new(Painter::of(cx)),
            context_menu: None,
            shell_facts: FileContextFacts::default(),
            diagnostics: Vec::new(),
            hover_offset: None,
            hover_point: point(px(0.), px(0.)),
            hover_task: None,
            hover_seq: 0,
            hover_card: None,
            pending_reveal: None,
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

    /// Called by the workspace when the tab is opened and whenever the facts
    /// could have changed. See `FileContextFacts` for why these are pushed.
    pub fn set_shell_facts(&mut self, facts: FileContextFacts, cx: &mut Context<Self>) {
        if self.shell_facts != facts {
            self.shell_facts = facts;
            cx.notify();
        }
    }

    pub fn set_diagnostics(&mut self, diagnostics: Vec<FileDiagnostic>, cx: &mut Context<Self>) {
        self.diagnostics = diagnostics;
        cx.notify();
    }

    pub fn diagnostics(&self) -> &[FileDiagnostic] {
        &self.diagnostics
    }

    /// Called from every pointer move over the source surface, which is to
    /// say very often. **An unchanged offset returns immediately**, without
    /// notifying — this is a hot path and a `notify` here would redraw the
    /// file on every pixel of travel.
    pub fn hover_moved(&mut self, offset: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.hover_offset == Some(offset) {
            return;
        }
        self.hover_offset = Some(offset);
        self.hover_point = at;
        let had_card = self.hover_card.take().is_some();
        // The diagnostic half of the card is already here, so it shows
        // immediately — no round trip, and a server with no hoverProvider
        // still shows its errors. The reply joins it later in `set_hover`.
        let local = self
            .diagnostics_at(offset)
            .iter()
            .map(|found| found.message.clone())
            .collect::<Vec<_>>();
        self.hover_card = (!local.is_empty()).then(|| local.join("\n"));
        if had_card || self.hover_card.is_some() {
            cx.notify();
        }
        self.hover_seq = self.hover_seq.wrapping_add(1);
        let seq = self.hover_seq;
        let path = self.path.clone();
        self.hover_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update(cx, |_view, cx| {
                cx.emit(FileViewEvent::Hover { path, offset, seq });
            });
        }));
    }

    /// Accepts an answer only if it is about where the pointer is now.
    /// The server's text joins the diagnostics already on the card, under
    /// them — it never replaces them.
    pub fn set_hover(&mut self, seq: u64, text: Option<String>, cx: &mut Context<Self>) {
        if seq != self.hover_seq {
            return;
        }
        let local = self.hover_offset.map(|offset| {
            self.diagnostics_at(offset)
                .iter()
                .map(|found| found.message.clone())
                .collect::<Vec<_>>()
        }).unwrap_or_default();
        let mut parts = local;
        parts.extend(text);
        self.hover_card = (!parts.is_empty()).then(|| parts.join("\n"));
        cx.notify();
    }

    pub fn hover_sequence(&self) -> u64 {
        self.hover_seq
    }

    pub fn hover_card(&self) -> Option<&str> {
        self.hover_card.as_deref()
    }

    pub fn reveal_at(&mut self, line: usize, cx: &mut Context<Self>) {
        self.pending_reveal = Some(line);
        self.apply_pending_reveal(cx);
    }

    fn apply_pending_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(line) = self.pending_reveal else { return };
        // No editor yet means no lines yet: keep holding.
        if self.editor().is_none() {
            return;
        }
        self.pending_reveal = None;
        self.scroll
            .source
            .scroll_to_item(line, gpui::ScrollStrategy::Center);
        cx.notify();
    }

    pub fn request_definition(&mut self, offset: usize, cx: &mut Context<Self>) {
        cx.emit(FileViewEvent::GoToDefinition {
            path: self.path.clone(),
            offset,
        });
    }

    /// The mark a line earns: the worst severity among its diagnostics.
    pub fn mark_for_line(&self, line: usize) -> Option<DiagnosticSeverity> {
        self.diagnostics
            .iter()
            .filter(|found| found.line == line)
            .map(|found| found.severity)
            .min()
    }

    /// Every message covering a byte offset, worst first.
    fn diagnostics_at(&self, offset: usize) -> Vec<&FileDiagnostic> {
        let mut found: Vec<_> = self
            .diagnostics
            .iter()
            .filter(|d| d.range.contains(&offset) || d.range.start == offset)
            .collect();
        found.sort_by_key(|d| d.severity);
        found
    }

    fn dismiss_hover(&mut self, cx: &mut Context<Self>) {
        self.hover_offset = None;
        self.hover_task = None;
        if self.hover_card.take().is_some() {
            cx.notify();
        }
    }

    /// The shell's facts plus the two this view answers itself.
    fn menu_facts(&self) -> FileContextFacts {
        let has_selection = self
            .editor()
            .map(|editor| self.current_selection(editor))
            .is_some_and(|selection| selection.start != selection.end);
        FileContextFacts {
            has_selection,
            is_markdown: self.is_markdown(),
            ..self.shell_facts
        }
    }

    fn open_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_hover(cx);
        self.editor_focus.focus(window, cx);
        self.context_menu = Some(event.position);
        cx.notify();
    }

    fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    fn handle_context_action(
        &mut self,
        action: FileContextAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_context_menu(cx);
        match action {
            FileContextAction::Copy => self.copy_selection(false, cx),
            FileContextAction::CopyAndTrim => self.copy_selection(true, cx),
            FileContextAction::Cut => {
                self.copy_selection(false, cx);
                self.replace_selection("");
                cx.notify();
            }
            FileContextAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_selection(&text);
                    cx.notify();
                }
            }
            FileContextAction::OpenMarkdownPreview => {
                self.set_markdown_mode(MarkdownMode::Preview, cx);
            }
            FileContextAction::SendToAgent => {
                if let Some(event) = self.selection_event(cx) {
                    cx.emit(event);
                }
            }
            FileContextAction::RevealInFileManager => {
                cx.emit(FileViewEvent::RevealInFileManager(self.path.clone()));
            }
            FileContextAction::OpenInTerminal => {
                cx.emit(FileViewEvent::OpenInTerminal(self.path.clone()));
            }
            FileContextAction::CopyPermalink => {
                if let Some(lines) = self.selection_lines() {
                    cx.emit(FileViewEvent::CopyPermalink {
                        path: self.path.clone(),
                        lines,
                    });
                }
            }
            FileContextAction::ViewFileHistory => {
                cx.emit(FileViewEvent::ViewFileHistory(self.path.clone()));
            }
            FileContextAction::GoToDefinition => {
                cx.emit(FileViewEvent::GoToDefinition {
                    path: self.path.clone(),
                    offset: self.caret,
                });
            }
            FileContextAction::FindReferences => {
                cx.emit(FileViewEvent::FindReferences {
                    path: self.path.clone(),
                    offset: self.caret,
                });
            }
        }
    }

    /// The selected text, or `None` when the selection is collapsed.
    fn selected_text(&self) -> Option<String> {
        let editor = self.editor()?;
        let selection = self.current_selection(editor);
        (selection.start != selection.end)
            .then(|| editor.buffer()[selection.start..selection.end].to_owned())
    }

    fn copy_selection(&mut self, trim: bool, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text() else {
            return;
        };
        let text = if trim {
            file_context_menu::trim_common_indent(&text)
        } else {
            text
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }

    /// The 1-based, inclusive line range the caret or selection covers.
    fn selection_lines(&self) -> Option<(usize, usize)> {
        let editor = self.editor()?;
        Some(crate::editor::line_range_for(
            editor.buffer(),
            self.current_selection(editor),
        ))
    }

    /// The payload "Add to Agent Thread" carries: where the code is, the code
    /// itself already trimmed flush left, and the fence tag to wrap it in.
    fn selection_event(&self, _cx: &mut Context<Self>) -> Option<FileViewEvent> {
        let text = file_context_menu::trim_common_indent(&self.selected_text()?);
        Some(FileViewEvent::SendSelectionToAgent {
            path: self.path.clone(),
            lines: self.selection_lines()?,
            text,
            language: self.editor()?.language().fence_tag(),
        })
    }

    /// Re-reads the file and compares it against the last known disk state
    /// (F-EDIT-05/06). The shell calls this when a file tab is activated —
    /// "modify externally, return to Sirio" — and on window focus.
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

    /// The path as the editor header shows it (#214).
    ///
    /// Rendered through `display_absolute_path`, not `Path::display`:
    /// the verbatim prefix is stripped from the *string only*, so
    /// `self.path` keeps the long-path capability every filesystem
    /// call in this view depends on.
    fn breadcrumb_text(&self) -> String {
        display_absolute_path(&self.path)
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
        self.dismiss_hover(cx);
        if event.keystroke.modifiers.control && event.keystroke.key == "a" {
            self.select_all();
            cx.notify();
            return;
        }
        if (event.keystroke.modifiers.platform || event.keystroke.modifiers.control)
            && event.keystroke.key == "end"
        {
            let Some(editor) = self.editor() else {
                return;
            };
            if editor.status() != &LoadStatus::Loaded || self.effective_mode() != MarkdownMode::Code
            {
                return;
            }
            let buffer = editor.buffer().to_owned();
            self.move_caret(buffer.len(), event.keystroke.modifiers.shift, &buffer);
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

    fn apply_markdown_format(&mut self, operation: MarkdownFormatOp, cx: &mut Context<Self>) {
        let Some(selection) = self.editor().map(|editor| self.current_selection(editor)) else {
            return;
        };
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let selection = match operation {
            MarkdownFormatOp::Bold => editor.format_bold(selection),
            MarkdownFormatOp::Italic => editor.format_italic(selection),
            MarkdownFormatOp::Heading => editor.toggle_heading(selection),
            MarkdownFormatOp::List => editor.toggle_list(selection),
            MarkdownFormatOp::Link { url } => editor.make_link(selection, &url),
        };
        self.caret = selection.end;
        self.source_selection = Some(selection);
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
            .border_color(theme.border)
            .px(px(20.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.text_faint)
            // #214: rendered through the helper written for this, not
            // `Path::display`, which put a verbatim `\\?\` prefix on
            // screen. The prefix is stripped from the *string only* --
            // `self.path` is untouched, so every filesystem call this
            // view makes keeps its long-path capability.
            .child(self.breadcrumb_text())
            .when(dirty, |this| {
                this.child(
                    div()
                        .text_size(theme.typography.caption2)
                        .text_color(theme.text)
                        .child("● edited"),
                )
            })
            .when(language.is_some(), |this| {
                this.child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(theme.radii.chip)
                        .bg(theme.surface_raised)
                        .text_color(theme.text_muted)
                        .child(language.unwrap_or_default()),
                )
            })
            .when(self.is_markdown(), |this| {
                this.child(render_mode_switch(self.effective_mode(), theme, entity))
            })
    }

    fn render_state(
        &self,
        theme: Theme,
        entity: gpui::Entity<Self>,
        caret_visible: bool,
        caret_offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(message) = &self.notice {
            return notice(message.clone(), theme);
        }

        match &self.state {
            ViewState::Loading => div()
                .id("file-loading")
                .debug_selector(|| "file-loading".to_owned())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.card_gap)
                .text_color(theme.text_muted)
                .child(loading::indeterminate(
                    "file-loading-orb",
                    loading::GENERIC_ORB,
                    &theme,
                    window,
                    cx,
                ))
                .child("Loading file…")
                .into_any_element(),
            ViewState::Ready(editor) => match editor.status() {
                LoadStatus::Loaded => {
                    let conflict = editor.conflict();
                    let mode = self.effective_mode();
                    let markdown_editing =
                        editor.language() == Language::Markdown && mode == MarkdownMode::Code;
                    let editor_entity = entity.clone();
                    let key_entity = entity.clone();
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
                        .on_key_down(move |event, window, cx| {
                            key_entity.update(cx, |view, cx| {
                                view.on_editor_key(event, window, cx);
                            });
                        })
                        .size_full()
                        .flex()
                        .flex_col()
                        .when(conflict != Conflict::None, |this| {
                            this.child(render_conflict_banner(conflict, theme, entity.clone()))
                        })
                        .when(markdown_editing, |this| {
                            this.child(render_markdown_toolbar(theme, entity.clone()))
                        })
                        .child(render_content(
                            editor,
                            mode,
                            theme,
                            entity.clone(),
                            self.source_selection,
                            caret_visible,
                            caret_offset,
                            theme.syntax_palette(),
                            &self.scroll,
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

    /// Blink timer tick for the source surface's insertion caret.
    fn flip_editor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.flip();
        cx.notify();
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
        // Production installs bezel alongside Sirio's theme. Some isolated
        // shell fixtures set only the Sirio global, so establish the same
        // invariant before anything below reaches for a bezel component
        // (mirrors the sidebar's popup guard). The source surface no longer
        // depends on it — its palette is `Theme::syntax_palette` — but the
        // bezel primitives this view renders still do.
        if cx.try_global::<bezel::theme::Theme>().is_none() {
            theme.install_into_bezel(cx);
        }
        let entity = cx.entity();
        // The source surface's insertion caret: wake on any caret/selection
        // move since the last frame, then arm the single toggle timer while
        // the editor owns focus.
        let editor_focused = self.editor_focus.is_focused(window);
        let caret_sig = (
            self.caret,
            self.source_selection
                .map(|selection| (selection.start, selection.end)),
        );
        if caret_sig != self.editor_caret_sig {
            self.editor_blink.wake();
            self.editor_caret_sig = caret_sig;
        }
        let caret_active = editor_focused && self.effective_mode() != MarkdownMode::Preview;
        caret::schedule(
            &mut self.editor_blink,
            caret_active,
            Self::flip_editor_blink,
            cx,
        );
        // The focus gate is what `caret::schedule` arms the timer on; the
        // paint gate additionally folds in the blink phase. Passing the
        // folded value to `schedule` instead would read a dark bar as
        // "unfocused" and latch the caret permanently on.
        self.editor_caret_visible = caret_active && self.editor_blink.visible();
        let caret_visible = self.editor_caret_visible;
        let caret_offset = self.caret.min(match &self.state {
            ViewState::Ready(editor) => editor.buffer().len(),
            _ => 0,
        });
        let context_menu = self.context_menu.map(|position| {
            let entity = cx.entity();
            let dismiss_entity = entity.clone();
            let mut menu = div()
                .id("file-context-menu")
                .debug_selector(|| "file-context-menu".to_owned())
                .w(px(240.0))
                .p(px(6.0))
                .rounded(theme.radii.user_pill)
                .border_1()
                .border_color(theme.border)
                .bg(theme.menu_surface())
                .shadow_lg();

            let items = file_context_menu::items(&self.menu_facts());
            let mut previous_group = None;
            for (index, item) in items.iter().enumerate() {
                if previous_group.is_some_and(|group| group != item.group) {
                    menu = menu.child(div().w_full().h(px(1.0)).my(px(4.0)).bg(theme.border));
                }
                previous_group = Some(item.group);

                let action = item.action;
                let item_entity = entity.clone();
                let selector = format!("file-context-item-{index}");
                let debug_selector = selector.clone();
                let disabled_reason = item.disabled_reason.clone();
                let is_disabled = disabled_reason.is_some();
                menu = menu.child(
                    div()
                        .id(selector)
                        .debug_selector(move || debug_selector.clone())
                        .w_full()
                        .min_h(px(29.0))
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(theme.radii.control)
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .text_size(theme.typography.footnote)
                        .when(is_disabled, |this| {
                            this.text_color(theme.text_faint).cursor_not_allowed()
                        })
                        .when(!is_disabled, |this| {
                            this.text_color(theme.text)
                                .hover(|style| style.bg(theme.element_hover))
                                .on_click(move |_, window, cx| {
                                    item_entity.update(cx, |view, cx| {
                                        view.handle_context_action(action, window, cx);
                                    });
                                })
                        })
                        .child(item.label)
                        .when_some(disabled_reason, |this, reason| {
                            this.child(
                                div()
                                    .debug_selector(move || {
                                        format!("file-context-item-{index}-reason")
                                    })
                                    .text_size(px(12.0))
                                    .text_color(theme.text_faint)
                                    .child(reason),
                            )
                        }),
                );
            }
            let menu = menu.on_mouse_down_out(move |_, _, cx| {
                dismiss_entity.update(cx, |view, cx| view.dismiss_context_menu(cx));
            });
            // `position` is window-absolute already (straight from
            // `MouseDownEvent::position`). A plain `.absolute().left()/.top()`
            // resolves against this container's own origin, which is itself
            // already window-absolute for any view not flush against the
            // window's top-left corner — double-counting it (P129).
            deferred(anchored().position(position).snap_to_window().child(menu)).priority(1)
        });
        let hover_card = self.hover_card.clone().map(|text| {
            let at = self.hover_point + point(px(12.), px(18.));
            deferred(
                anchored().position(at).snap_to_window().child(hover_card(&text, theme)),
            )
            .priority(1)
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
            .when_some(context_menu, |this, menu| this.child(menu))
            .when_some(hover_card, |this, card| this.child(card))
            .child(self.render_header(theme, entity))
            .child(div().flex_1().min_h(px(0.0)).child(self.render_state(
                theme,
                cx.entity(),
                caret_visible,
                caret_offset,
                window,
                cx,
            )))
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
        .bg(theme.surface_raised)
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
        .text_color(if active { theme.text } else { theme.text_faint })
        .when(active, |this| this.bg(theme.element_active))
        .hover(|style| style.bg(theme.element_hover))
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
        .bg(theme.danger_muted)
        .border_b_1()
        .border_color(theme.border)
        .text_size(theme.typography.footnote)
        .text_color(theme.text)
        .child(div().flex_1().child(message))
        .when(conflict == Conflict::ChangedOnDisk, |this| {
            this.child(
                div()
                    .id("file-conflict-reload")
                    .debug_selector(|| "file-conflict-reload".into())
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.element_hover))
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
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, cx| {
                        keep_entity.update(cx, |view, cx| view.keep(cx));
                    })
                    .child("Keep"),
            )
        })
}

/// The Markdown formatting toolbar (F-EDIT-02). It is deliberately shown in
/// edit mode; Preview remains a reading surface. The link URL is a
/// deterministic placeholder until the view has a text prompt seam of its own.
fn render_markdown_toolbar(theme: Theme, file_view: gpui::Entity<FileView>) -> impl IntoElement {
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
        .border_color(theme.border)
        .bg(theme.surface_raised)
        .child(render_format_button(
            "B",
            "file-format-bold",
            MarkdownFormatOp::Bold,
            file_view.clone(),
            theme,
        ))
        .child(render_format_button(
            "I",
            "file-format-italic",
            MarkdownFormatOp::Italic,
            file_view.clone(),
            theme,
        ))
        .child(render_format_button(
            "H",
            "file-format-heading",
            MarkdownFormatOp::Heading,
            file_view.clone(),
            theme,
        ))
        .child(render_format_button(
            "List",
            "file-format-list",
            MarkdownFormatOp::List,
            file_view.clone(),
            theme,
        ))
        .child(render_format_button(
            "Link",
            "file-format-link",
            MarkdownFormatOp::Link {
                url: "https://example.com".to_owned(),
            },
            file_view,
            theme,
        ))
}

fn render_format_button(
    label: &'static str,
    selector: &'static str,
    operation: MarkdownFormatOp,
    file_view: gpui::Entity<FileView>,
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
        .text_color(theme.text)
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| {
            operation.clone().apply(&file_view, cx);
        })
        .child(label)
}

fn render_content(
    editor: &Editor,
    mode: MarkdownMode,
    theme: Theme,
    entity: gpui::Entity<FileView>,
    selection: Option<Selection>,
    caret_visible: bool,
    caret_offset: usize,
    syntax_palette: bezel::theme::SyntaxPalette,
    scroll: &SurfaceScroll,
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
            .size_full()
            .relative()
            .child(
                div()
                    .id("file-markdown-scroll")
                    .debug_selector(|| "file-markdown-scroll".into())
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&scroll.preview)
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
                    ),
            )
            .child(scrollbar(
                "file-markdown-bar",
                &scroll.preview,
                &scroll.preview_bar,
            ))
            .into_any_element();
    }

    // One row per line, materialized only for the visible range (the same
    // `uniform_list` the history panel scrolls thousands of commits with).
    // The old surface built an element for *every* line on *every* frame —
    // each with its own tree-sitter pass and text shaping — and since gpui
    // draws the window in one pass, one long file stalled the whole app.
    // The buffer is segmented exactly as `split_inclusive('\n')` does: a
    // trailing newline does not open an extra empty line, and an empty
    // buffer still hosts one empty line so a freshly opened file shows an
    // insertion point instead of nothing.
    let buffer = editor.buffer();
    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0;
    for raw in buffer.split_inclusive('\n') {
        let line_len = raw.strip_suffix('\n').unwrap_or(raw).len();
        line_ranges.push((offset, offset + line_len));
        offset += raw.len();
    }
    if line_ranges.is_empty() {
        line_ranges.push((0, 0));
    }
    // Every row is as wide as the list's measured item, so measure the
    // longest line (F-EDIT-07: no wrapping, the horizontal scroller owns
    // the overflow) — measuring row 0 would clip everything wider than it.
    let longest_line = line_ranges
        .iter()
        .enumerate()
        .max_by_key(|(_, (start, end))| end - start)
        .map(|(index, _)| index);
    let line_count = line_ranges.len();
    let line_ranges = Rc::new(line_ranges);
    let language = editor.language();
    let row_entity = entity.clone();
    let lines = uniform_list("file-text-scroll", line_count, move |range, _window, cx| {
        // Read the buffer back through the entity instead of cloning up
        // to a mebibyte of text into the closure every frame. The
        // ranges were cut from this same buffer this same frame; the
        // checked slice only guards a mutation that cannot happen
        // between render and prepaint.
        let view = row_entity.read(cx);
        let Some(editor) = view.editor() else {
            return Vec::new();
        };
        let buffer = editor.buffer();
        range
            .filter_map(|index| line_ranges.get(index).map(|range| (index, *range)))
            .map(|(index, (start, end))| {
                let mark = row_entity.read(cx).mark_for_line(index);
                render_source_line(
                    index,
                    buffer.get(start..end).unwrap_or_default().to_owned(),
                    start,
                    end,
                    language,
                    theme,
                    row_entity.clone(),
                    selection,
                    caret_visible,
                    caret_offset,
                    &syntax_palette,
                    mark,
                )
            })
            .collect()
    })
    .debug_selector(|| "file-text-scroll".into())
    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
    .with_width_from_item(longest_line)
    .track_scroll(&scroll.source)
    .size_full()
    .p(px(16.0));
    let mut source = div().size_full().flex().flex_col();
    if is_markdown && mode == MarkdownMode::Code && editor.preview_locked() {
        let preview_entity = entity.clone();
        source = source.child(
            div()
                .id("file-manual-preview")
                .debug_selector(|| "file-manual-preview".into())
                .mx(px(16.0))
                .mt(px(16.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(theme.radii.control)
                .bg(theme.surface_raised)
                .text_size(theme.typography.footnote)
                .text_color(theme.text_muted)
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
                        .hover(|style| style.bg(theme.element_hover))
                        .on_click(move |_, _, cx| {
                            preview_entity.update(cx, |view, cx| {
                                view.set_markdown_mode(MarkdownMode::Preview, cx);
                            });
                        })
                        .child("Render preview"),
                ),
        );
    }
    // The bar overlays the list's own box, so it spans exactly the viewport
    // it reports on and never reflows the rows beneath it. It reads the
    // list handle's base `ScrollHandle`: the same offset and overflow the
    // list itself scrolls by.
    let source_handle = scroll.source.0.borrow().base_handle.clone();
    source
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .relative()
                .child(lines)
                .child(scrollbar(
                    "file-text-bar",
                    &source_handle,
                    &scroll.source_bar,
                )),
        )
        .into_any_element()
}

/// bezel's vertical bar over one tracked surface, under a selector the
/// drawn-frame tests can find. bezel's strip carries no selector of its own,
/// so the tag goes on a full-surface overlay that exists only while there is
/// overflow to show — the same guard bezel applies before it paints a thumb
/// — and that registers no hitbox, so the pointer still reaches the rows
/// beneath it.
///
/// While there is nothing to show, a canvas watches the handle instead:
/// the bar can only draw from the geometry the *previous* frame left, so
/// the frame that first lays the content out taller than its viewport
/// would otherwise end with no bar and nothing asking for the frame that
/// paints it (Preview has no caret blink to repaint on). The canvas
/// prepaints after its sibling has, sees this frame's overflow, and asks
/// for one more frame. Self-limiting: once the bar is up it is not here.
fn scrollbar(id: &'static str, handle: &ScrollHandle, state: &ScrollbarState) -> AnyElement {
    let has_overflow = |handle: &ScrollHandle| {
        scroll::thumb(
            handle.bounds().size.height,
            handle.max_offset().y,
            handle.offset().y,
            scroll::MIN_THUMB,
        )
        .is_some()
    };
    if !has_overflow(handle) {
        let watched = handle.clone();
        return canvas(
            move |_, window, _| {
                if has_overflow(&watched) {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element();
    }
    div()
        .debug_selector(move || id.to_string())
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .child(scroll::scrollbar(id, handle, state))
        .into_any_element()
}

/// One row of the virtualized source surface: the gutter number plus the
/// editable text run for the buffer bytes `start..end` (the line without
/// its newline). The caret bar lives on exactly one row — the one whose
/// range contains `caret_offset` — collapsed to that row's own bytes.
#[allow(clippy::too_many_arguments)]
fn render_source_line(
    index: usize,
    line: String,
    start: usize,
    end: usize,
    language: Language,
    theme: Theme,
    entity: gpui::Entity<FileView>,
    selection: Option<Selection>,
    caret_visible: bool,
    caret_offset: usize,
    syntax_palette: &bezel::theme::SyntaxPalette,
    mark: Option<DiagnosticSeverity>,
) -> gpui::Stateful<gpui::Div> {
    let caret = if caret_visible && caret_offset >= start && caret_offset <= end {
        Some((caret_offset - start).min(line.len()))
    } else {
        None
    };
    div()
        .id(("file-line", index))
        .debug_selector({
            let selector = format!("file-source-line-{index}");
            move || selector.clone()
        })
        .w_full()
        .min_h(px(18.0))
        .flex()
        .whitespace_nowrap()
        .font_family(theme.typography.code_family)
        .text_size(theme.typography.code_size)
        .text_color(theme.text)
        .child(
            div()
                .w(px(52.0))
                .flex_none()
                .flex()
                .flex_row()
                .child(
                    div()
                        .w(px(10.0))
                        .flex_none()
                        .when_some(mark, |element, severity| {
                            element
                                .debug_selector({
                                    let selector = format!("file-line-mark-{index}");
                                    move || selector.clone()
                                })
                                .text_color(match severity {
                                    DiagnosticSeverity::Error => theme.danger,
                                    DiagnosticSeverity::Warning => theme.warning,
                                    _ => theme.text_faint,
                                })
                                .child("●")
                        }),
                )
                .child(
                    div()
                        .w(px(42.0))
                        .flex_none()
                        .text_color(theme.text_faint)
                        .child(format!("{:>5} ", index + 1)),
                ),
        )
        .child(EditableLine::new(
            ("file-line-text", index),
            line,
            start,
            language,
            theme,
            entity,
            selection,
            syntax_palette.clone(),
            caret,
        ))
}

/// One classified span of a source line, in bezel-syntax's own vocabulary.
///
/// The kind is `bezel::theme::HighlightKind` rather than a reduction of it:
/// an earlier version funnelled 24 kinds into keyword/literal/comment and
/// dropped the rest, which is how a tree-sitter parse came out looking like
/// a three-colour regex highlighter — and how a number ended up painted in
/// the string colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeSpan {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) kind: bezel::theme::HighlightKind,
}

fn bezel_syntax_tag(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust => Some("rust"),
        Language::Python => Some("python"),
        Language::JavaScript => Some("javascript"),
        Language::TypeScript => Some("typescript"),
        Language::Shell => Some("bash"),
        Language::Json => Some("json"),
        Language::Toml => Some("toml"),
        Language::Go => Some("go"),
        _ => None,
    }
}

/// F-EDIT-07's code path stays Sirio's no-wrap editor; only token
/// classification is delegated to bezel-syntax. Unsupported grammars remain
/// legible plain text, which is bezel-syntax's documented fallback.
pub(crate) fn code_spans(language: Language, line: &str) -> Vec<CodeSpan> {
    let Some(tag) = bezel_syntax_tag(language) else {
        return Vec::new();
    };
    syntax::highlight(line, tag)
        .unwrap_or_default()
        .into_iter()
        .map(|(range, kind)| CodeSpan { range, kind })
        .collect()
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
    /// Local byte offset of the insertion caret when this line hosts it
    /// (`None` on every other line, or whenever the bar is hidden). Painted
    /// as a thin accent quad in `Element::paint`, after any selection.
    caret_offset: Option<usize>,
    /// Colour of the caret bar (the theme's accent).
    caret_color: Rgba,
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
        syntax_palette: bezel::theme::SyntaxPalette,
        caret_offset: Option<usize>,
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
                let color = syntax_palette.color(span.kind);
                (
                    span.range,
                    HighlightStyle {
                        color: Some(color),
                        ..Default::default()
                    },
                )
            })
            .collect();
        highlights.extend(links.iter().map(|(range, _)| {
            (
                range.clone(),
                HighlightStyle {
                    color: Some(theme.file_link.into()),
                    underline: Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme.file_link.into()),
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
            selection_fill: theme.element_active,
            caret_offset,
            caret_color: theme.text,
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
        // The insertion caret: a thin accent quad at the tracked offset,
        // painted after the selection so a collapsed selection shows the
        // bar rather than nothing.
        if let Some(caret) = self.caret_offset {
            let layout = self.text.layout();
            let line_height = layout.line_height();
            let position = layout
                .position_for_index(caret)
                .unwrap_or(point(bounds.right(), bounds.origin.y));
            window.paint_quad(quad(
                Bounds::new(
                    point(position.x.min(bounds.right()), bounds.origin.y),
                    size(crate::caret::BAR_WIDTH, line_height),
                ),
                px(0.0),
                self.caret_color,
                Edges::default(),
                transparent_black(),
                BorderStyle::default(),
            ));
        }
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
        let hitbox_for_hover = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && !event.dragging()
                && hitbox_for_hover.is_hovered(window)
            {
                let local = match layout.index_for_position(event.position) {
                    Ok(index) | Err(index) => index.min(line_len),
                };
                let at = event.position;
                view.update(cx, |view, cx| view.hover_moved(line_start + local, at, cx));
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
                        } else {
                            // Not a link — in a code file the same gesture
                            // means "where is this defined?".
                            view.update(cx, |view, cx| view.request_definition(up_global, cx));
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

/// The Markdown formatting operations the toolbar offers (F-EDIT-02). Every
/// button calls the same headless operation as the source editor's model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownFormatOp {
    Bold,
    Italic,
    Heading,
    List,
    Link { url: String },
}

impl MarkdownFormatOp {
    fn apply(self, file_view: &gpui::Entity<FileView>, cx: &mut App) {
        file_view.update(cx, |view, cx| view.apply_markdown_format(self, cx));
    }
}

/// The hover popover. Plain text in the code font: the server answers in
/// Markdown, and rendering it properly would put a parser in the paint
/// path for the sake of a few bold words. The signature — the part that
/// matters — reads correctly either way.
fn hover_card(text: &str, theme: Theme) -> AnyElement {
    div()
        .id("file-view-hover")
        .debug_selector(|| "file-view-hover".into())
        .max_w(px(520.0))
        .max_h(px(240.0))
        .overflow_hidden()
        .p(px(8.0))
        .bg(theme.menu_surface())
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radii.control)
        .font_family(theme.typography.code_family)
        .text_size(theme.typography.code_size)
        .text_color(theme.text)
        .child(text.to_owned())
        .into_any_element()
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
        .text_color(theme.text_muted)
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
    use sirio_markdown::{Block, Inline};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// #214: the breadcrumb must not put a verbatim prefix on screen.
    ///
    /// It rendered `self.path.display()` raw, so opening a file showed
    /// `\?\D:\Progetti\sirio\sirio\README.md` in the editor header --
    /// the same leak #118/#121 closed "across the control surface", in a
    /// surface that was not part of it.
    ///
    /// The `Path` itself is deliberately left verbatim here, and the test
    /// asserts that too: stripping the prefix from the value rather than
    /// from the rendered string would cost the long-path capability every
    /// filesystem call this view makes depends on.
    ///
    /// Windows-only, because the prefix only exists there.
    #[cfg(windows)]
    #[gpui::test]
    async fn the_breadcrumb_does_not_show_a_verbatim_prefix(cx: &mut gpui::TestAppContext) {
        let file = TempFile::new("verbatim-breadcrumb", "hello\n");
        let verbatim = PathBuf::from(format!(r"\\?\{}", file.path().display()));

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
        let window = cx.add_window(|_window, cx| FileView::new(verbatim.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let view = cx.update(|window, _| window.root::<FileView>().flatten().expect("view root"));
        cx.run_until_parked();

        let shown = view.read_with(&cx.cx, |view, _| view.breadcrumb_text());
        assert!(
            !shown.starts_with(r"\\?\"),
            "the breadcrumb must not show the verbatim prefix, got {shown:?}"
        );
        assert!(
            shown.ends_with("verbatim-breadcrumb") || shown.contains("verbatim-breadcrumb"),
            "and it must still name the file, got {shown:?}"
        );
        assert!(
            view.read_with(&cx.cx, |view, _| {
                view.path.to_string_lossy().starts_with(r"\\?\")
            }),
            "the Path itself stays verbatim -- only the rendered string is \
             stripped, or the view loses long-path capability"
        );
    }

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-file-view-{name}-{}-{id}",
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
                "sirio-file-view-{}-{id}.{extension}",
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
                .any(|span| { span.kind == bezel::theme::HighlightKind::Keyword && span.range == (0..2) })
        );
        assert!(
            python
                .iter()
                .any(|span| { span.kind == bezel::theme::HighlightKind::Keyword && span.range == (0..3) })
        );
        assert!(rust.iter().any(|span| span.kind == bezel::theme::HighlightKind::String));
        assert!(python.iter().any(|span| span.kind == bezel::theme::HighlightKind::String));
        assert_ne!(
            rust, python,
            "language detection must select different spans"
        );
    }

    #[test]
    fn bezel_syntax_classifies_rust_numeric_literals() {
        let source = "let answer = 42;";
        let raw = syntax::highlight(source, "rust");
        let spans = code_spans(Language::Rust, source);
        assert!(
            spans.iter().any(|span| {
                matches!(span.kind, bezel::theme::HighlightKind::Number | bezel::theme::HighlightKind::Constant)
                    && &source[span.range.clone()] == "42"
            }),
            "bezel-syntax's tree-sitter classification must reach the custom code surface; raw={raw:?} spans={spans:?}"
        );
    }

    /// The editor must paint what tree-sitter actually classified, not a
    /// three-bucket reduction of it. bezel-syntax resolves `usize` as
    /// `TypeBuiltin` and `compute_total` as `Function`; a surface that
    /// keeps only keyword/literal/comment is the pre-tree-sitter colour
    /// scheme wearing a tree-sitter parser.
    #[test]
    fn every_kind_bezel_classifies_reaches_the_code_surface() {
        let source = "let count: usize = compute_total(&items) + 42; // note";
        let offered = syntax::highlight(source, "rust").unwrap_or_default();
        let kept = code_spans(Language::Rust, source);

        let painted = |needle: &str| {
            let at = source.find(needle).expect("needle is in the fixture");
            let span = at..at + needle.len();
            kept.iter().any(|kept| kept.range == span)
        };

        assert!(painted("usize"), "a type name must be painted: kept={kept:?}");
        assert!(
            painted("compute_total"),
            "a function name must be painted: kept={kept:?}"
        );
        assert_eq!(
            kept.len(),
            offered.len(),
            "every span bezel-syntax classified must survive; offered={offered:?} kept={kept:?}"
        );
    }

    /// A number and a string are different colours in every editor Sirio is
    /// measured against. Collapsing both into one `Literal` bucket and then
    /// resolving that bucket as `HighlightKind::String` paints `42` in the
    /// string hue.
    #[test]
    fn a_number_is_not_painted_in_the_string_colour() {
        let palette = Theme::dark().syntax_palette();
        let source = "let n = 42; let s = \"text\";";
        let kept = code_spans(Language::Rust, source);

        let colour_of = |needle: &str| {
            let at = source.find(needle).expect("needle is in the fixture");
            let span = kept
                .iter()
                .find(|kept| kept.range == (at..at + needle.len()))
                .unwrap_or_else(|| panic!("{needle} must be classified: kept={kept:?}"));
            // Exactly what `EditableLine::new` paints with.
            palette.color(span.kind)
        };

        assert_ne!(
            colour_of("42"),
            colour_of("\"text\""),
            "a numeric literal must not resolve to the string colour"
        );
    }

    // ── The shell-facing surface, through the actual view ──────────────

    #[gpui::test]
    async fn dirty_state_is_reachable_through_the_view(cx: &mut gpui::TestAppContext) {
        let file = TempFile::new("dirty", "hello\n");

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
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

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
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
    async fn file_monitor_reload_is_linux_only_but_rename_conflicts_are_cross_platform(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("watcher", "original\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        std::fs::write(file.path(), "external\n").expect("external write");
        if cfg!(target_os = "linux") {
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
        } else {
            assert_eq!(
                view.read_with(&cx.cx, |view, _| {
                    view.editor().expect("editor loaded").buffer().to_owned()
                }),
                "original\n",
                "the inotify monitor is intentionally unavailable on macOS"
            );
        }

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
                sirio_markdown::FileSystemEvent {
                    path: file.path().to_path_buf(),
                    kind: sirio_markdown::FileEventKind::Renamed,
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
            std::env::temp_dir().join(format!("sirio-file-view-missing-{}", std::process::id()));

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
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

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
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

    #[gpui::test]
    async fn a_resting_pointer_asks_for_a_hover_only_after_the_delay(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                if let FileViewEvent::Hover { offset, .. } = event {
                    captured.borrow_mut().push(*offset);
                }
            })
            .detach();
        });

        view.update(&mut cx.cx, |view, cx| {
            view.hover_moved(3, point(px(10.), px(10.)), cx)
        });
        cx.cx.executor().advance_clock(std::time::Duration::from_millis(100));
        cx.cx.run_until_parked();
        assert!(
            events.borrow().is_empty(),
            "100ms is not a dwell — asking this early makes every pass of the mouse a request"
        );

        cx.cx.executor().advance_clock(std::time::Duration::from_millis(400));
        cx.cx.run_until_parked();
        assert_eq!(
            events.borrow().as_slice(),
            &[3],
            "exactly one request, for the resting offset"
        );
    }

    #[gpui::test]
    async fn moving_on_before_the_delay_asks_only_about_where_it_stopped(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                if let FileViewEvent::Hover { offset, .. } = event {
                    captured.borrow_mut().push(*offset);
                }
            })
            .detach();
        });

        for offset in [1usize, 2, 3, 4] {
            view.update(&mut cx.cx, |view, cx| {
                view.hover_moved(offset, point(px(10.), px(10.)), cx)
            });
            cx.cx.executor().advance_clock(std::time::Duration::from_millis(50));
        }
        cx.cx.executor().advance_clock(std::time::Duration::from_millis(400));
        cx.cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[4],
            "replacing the timer Task cancels it, so only the last rest asks"
        );
    }

    #[gpui::test]
    async fn a_reply_for_a_place_the_pointer_has_left_is_discarded(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        // Two dwells: the first reply arrives after the second has started.
        view.update(&mut cx.cx, |view, cx| {
            view.hover_moved(3, point(px(10.), px(10.)), cx)
        });
        cx.cx.executor().advance_clock(std::time::Duration::from_millis(400));
        cx.cx.run_until_parked();
        let stale = view.read_with(&cx.cx, |view, _| view.hover_sequence());

        view.update(&mut cx.cx, |view, cx| {
            view.hover_moved(9, point(px(20.), px(10.)), cx)
        });
        cx.cx.executor().advance_clock(std::time::Duration::from_millis(400));
        cx.cx.run_until_parked();

        view.update(&mut cx.cx, |view, cx| {
            view.set_hover(stale, Some("stale answer".to_owned()), cx)
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned)),
            None,
            "an answer for a place the pointer has left must not appear where it now is"
        );

        let current = view.read_with(&cx.cx, |view, _| view.hover_sequence());
        view.update(&mut cx.cx, |view, cx| {
            view.set_hover(current, Some("fresh answer".to_owned()), cx)
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned))
                .as_deref(),
            Some("fresh answer")
        );
    }

    #[gpui::test]
    async fn revealing_a_line_scrolls_it_into_view(cx: &mut gpui::TestAppContext) {
        let body = (0..400).map(|n| format!("line {n}\n")).collect::<String>();
        let file = TempFile::with_extension("rs", &body);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        assert!(
            cx.debug_bounds("file-source-line-350").is_none(),
            "line 350 starts off screen, or the test proves nothing"
        );
        view.update(&mut cx.cx, |view, cx| view.reveal_at(350, cx));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-source-line-350").is_some(),
            "reveal scrolls the line into view"
        );
    }

    #[gpui::test]
    async fn a_reveal_asked_before_the_file_loaded_still_happens(
        cx: &mut gpui::TestAppContext,
    ) {
        // This is the whole trap. A tab opened by "go to definition" loads
        // its content asynchronously, so revealing immediately after the tab
        // appears addresses lines that do not exist yet. Held, then applied.
        let body = (0..400).map(|n| format!("line {n}\n")).collect::<String>();
        let file = TempFile::with_extension("rs", &body);

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            let mut view = FileView::new(file.path().to_path_buf(), cx);
            // Before any load has completed.
            view.reveal_at(350, cx);
            view
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
            let ready = cx.update(|window, app| {
                window
                    .root::<FileView>()
                    .flatten()
                    .is_some_and(|view| view.read(app).editor().is_some())
            });
            if ready {
                break;
            }
            cx.cx.executor().advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-source-line-350").is_some(),
            "the reveal was held until the content existed, not dropped"
        );
    }

    #[gpui::test]
    async fn a_modifier_click_asks_for_the_definition(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "fn main() { helper(); }\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                if let FileViewEvent::GoToDefinition { offset, .. } = event {
                    captured.borrow_mut().push(*offset);
                }
            })
            .detach();
        });

        view.update(&mut cx.cx, |view, cx| view.request_definition(12, cx));
        cx.cx.run_until_parked();
        assert_eq!(&*events.borrow(), &[12]);
    }

    #[gpui::test]
    async fn a_diagnostic_marks_its_line_without_moving_the_numbers(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {\n    let x = 1;\n}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let before = cx
            .debug_bounds("file-source-line-1")
            .expect("line 1 is drawn");

        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic {
                    line: 1,
                    range: 16..17,
                    severity: DiagnosticSeverity::Warning,
                    message: "unused variable `x`".to_owned(),
                }],
                cx,
            )
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            cx.debug_bounds("file-line-mark-1").is_some(),
            "the marked line carries a mark"
        );
        let after = cx
            .debug_bounds("file-source-line-1")
            .expect("line 1 is still drawn");
        assert_eq!(
            before.origin.x, after.origin.x,
            "the mark lives in its own column: a file with an error must not \
             shift every line number sideways"
        );
    }

    #[gpui::test]
    async fn the_worst_severity_wins_the_mark(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "a\nb\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![
                    FileDiagnostic { line: 0, range: 0..1,
                        severity: DiagnosticSeverity::Hint, message: "hint".into() },
                    FileDiagnostic { line: 0, range: 0..1,
                        severity: DiagnosticSeverity::Error, message: "error".into() },
                ],
                cx,
            )
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.mark_for_line(0)),
            Some(DiagnosticSeverity::Error)
        );
    }

    #[gpui::test]
    async fn resting_on_a_diagnostic_shows_its_message_with_no_server_reply(
        cx: &mut gpui::TestAppContext,
    ) {
        // The diagnostic half of the card is built locally, so it appears
        // immediately — and a server with no hoverProvider still shows its
        // errors.
        let file = TempFile::with_extension("rs", "let x = 1;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic { line: 0, range: 4..5,
                    severity: DiagnosticSeverity::Error, message: "boom".into() }],
                cx,
            );
            view.hover_moved(4, point(px(10.), px(10.)), cx);
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned)).as_deref(),
            Some("boom"),
            "no round trip is involved: the message is already in the view"
        );
    }

    #[gpui::test]
    async fn an_empty_publish_clears_the_marks(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "a\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic { line: 0, range: 0..1,
                    severity: DiagnosticSeverity::Error, message: "boom".into() }],
                cx,
            );
            view.set_diagnostics(Vec::new(), cx);
        });
        assert_eq!(view.read_with(&cx.cx, |view, _| view.mark_for_line(0)), None);
    }

    /// Mounts a `FileView` in a drawn window and pumps until its background
    /// load has actually installed the editor — the same hardened pump the
    /// drawn git tests use, so the assertions below run against a loaded
    /// editor, never on timing luck.
    fn mounted_file_view(
        cx: &mut gpui::TestAppContext,
        path: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<FileView>) {
        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
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

    struct FileViewOriginFixture {
        view: gpui::Entity<FileView>,
    }

    impl Render for FileViewOriginFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .flex_row()
                .size_full()
                .child(
                    div()
                        .debug_selector(|| "file-view-origin-spacer".to_owned())
                        .w(px(280.0))
                        .h_full(),
                )
                .child(div().flex_1().h_full().child(self.view.clone()))
        }
    }

    #[gpui::test]
    async fn the_context_menu_paints_at_the_click_point_behind_a_non_zero_origin(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _view) = mounted_origin_fixture(cx, file.path().to_path_buf());

        let spacer = cx
            .debug_bounds("file-view-origin-spacer")
            .expect("spacer must be drawn");
        let origin_x = spacer.origin.x + spacer.size.width;
        assert!(
            origin_x > px(0.0),
            "fixture must place the view at a non-zero window origin, got {origin_x:?}"
        );

        let click = point(origin_x + px(40.0), spacer.origin.y + px(40.0));
        cx.simulate_mouse_down(click, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));

        let menu = cx
            .debug_bounds("file-context-menu")
            .expect("the context menu must be drawn");
        // P129 shifted this by the pane's entire 280px origin, so a 1px
        // tolerance cleanly separates "fixed" from "double-counted".
        assert!(
            (menu.origin.x - click.x).abs() < px(1.0) && (menu.origin.y - click.y).abs() < px(1.0),
            "menu must paint at the click point, not click + view origin (P129): \
             menu.origin={:?}, click={:?}",
            menu.origin,
            click
        );
    }

    fn mounted_origin_fixture(
        cx: &mut gpui::TestAppContext,
        path: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<FileView>) {
        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            let view = cx.new(|cx| FileView::new(path, cx));
            FileViewOriginFixture { view }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.cx.executor().allow_parking();
        let fixture = cx.update(|window, _| {
            window
                .root::<FileViewOriginFixture>()
                .flatten()
                .expect("fixture root")
        });
        let entity = fixture.read_with(&cx.cx, |fixture, _| fixture.view.clone());
        for _ in 0..600 {
            if entity.read_with(&cx.cx, |view, _| view.editor().is_some()) {
                break;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
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
    async fn copy_and_trim_pastes_flush_left(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension(
            "rs",
            "fn outer() {\n        let a = 1;\n        let b = 2;\n}\n",
        );
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        // Select the two indented lines, not the braces around them.
        let buffer = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor").buffer().to_string()
        });
        let start = buffer.find("        let a").expect("first indented line");
        let end = buffer.find("\n}").expect("closing brace");
        view.update(&mut cx.cx, |view, cx| {
            view.source_selection = Some(Selection { start, end });
            cx.notify();
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.handle_context_action(FileContextAction::CopyAndTrim, window, cx);
            });
        });

        let copied = cx
            .update(|_, cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .expect("Copy and Trim must write the clipboard");
        assert_eq!(
            copied, "let a = 1;\nlet b = 2;",
            "the indentation every line shares must be dropped"
        );
    }

    #[gpui::test]
    async fn reveal_and_send_leave_as_events_carrying_the_located_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let path = file.path().to_path_buf();

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // Select the whole second line.
        let buffer = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor").buffer().to_string()
        });
        let start = buffer.find("fn b").expect("second line");
        let end = start + "fn b() {}".len();
        view.update(&mut cx.cx, |view, cx| {
            view.source_selection = Some(Selection { start, end });
            cx.notify();
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.handle_context_action(FileContextAction::SendToAgent, window, cx);
                view.handle_context_action(FileContextAction::RevealInFileManager, window, cx);
            });
        });

        let seen = events.borrow().clone();
        assert_eq!(
            seen,
            vec![
                FileViewEvent::SendSelectionToAgent {
                    path: path.clone(),
                    lines: (2, 2),
                    text: "fn b() {}".to_owned(),
                    language: "rust",
                },
                FileViewEvent::RevealInFileManager(path),
            ],
            "both App-route entries must leave as typed events"
        );
    }

    #[gpui::test]
    async fn markdown_code_mode_edits_and_saves_the_raw_source(cx: &mut gpui::TestAppContext) {
        let original = "<p align=\"center\">\r\n  *A fork with its own terms.*\r\n</p>\r\n";
        let file = TempFile::with_extension("md", original);
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
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "Code mode edits the raw source surface"
        );

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.editor_focus.focus(window, cx));
            window.simulate_next_frame(cx);
        });
        cx.simulate_keystrokes("ctrl-end");
        cx.run_until_parked();
        cx.simulate_input("qa-edit-probe");
        cx.run_until_parked();
        view.update(&mut cx.cx, |view, cx| {
            view.save(cx).expect("raw Markdown source saves");
        });

        let expected = format!("{original}qa-edit-probe");
        assert_eq!(
            std::fs::read(file.path()).expect("saved file"),
            expected.as_bytes()
        );
    }

    /// Drives one blink cycle of the source surface's insertion bar.
    /// `caret::schedule` keeps the surface's `Blink` ticking, but the gate
    /// that decides whether the bar is painted never read that state back,
    /// so a focused editor drew a permanently-solid bar — the one thing a
    /// caret must not be.
    #[gpui::test]
    async fn the_editor_caret_blinks_while_the_source_surface_holds_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("txt", "hello\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.editor_focus.focus(window, cx));
        });
        cx.cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            view.read_with(&cx.cx, |view, _| view.editor_caret_visible),
            "a focused source surface paints its insertion bar"
        );

        cx.cx.executor().advance_clock(caret::BLINK_INTERVAL);
        cx.cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            !view.read_with(&cx.cx, |view, _| view.editor_caret_visible),
            "one blink interval later the bar must be dark — a bar that never \
             goes dark is a decoration, not a caret"
        );
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
            "Code mode renders the raw source editor"
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
            "a large Markdown file opens in the raw source editor"
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
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        // Formatting belongs to Code mode and routes through the same
        // headless operations as the source editor.
        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code option is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        cx.update(|window, app| {
            view.update(app, |view, cx| view.editor_focus.focus(window, cx));
        });
        cx.update(|window, app| window.simulate_next_frame(app));
        #[cfg(target_os = "macos")]
        cx.simulate_keystrokes("cmd-a");
        #[cfg(not(target_os = "macos"))]
        cx.simulate_keystrokes("ctrl-a");
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

        let mut previous = view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor loaded").buffer().to_owned()
        });
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

    // ── F-CORE-FILE-04: a rendered link is a real click target ─────────

    #[gpui::test]
    async fn platform_click_on_a_link_label_resolves_and_emits_open_file(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "sirio-file-view-link-{}-{}",
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
            "sirio-file-view-preview-link-{}-{}",
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

    /// A long file costs a frame only what fits the viewport: rows are
    /// materialized from the visible range, so the first line is drawn and
    /// a line thousands of rows below is never laid out. Before this, every
    /// line became an element on every frame — each with its own
    /// tree-sitter pass and text shaping — and since gpui draws the window
    /// in one pass, one long file stalled the whole app, not just the tab.
    #[gpui::test]
    async fn a_long_file_draws_only_the_visible_lines(cx: &mut gpui::TestAppContext) {
        let mut source = String::new();
        for i in 0..4000 {
            source.push_str(&format!(
                "fn function_{i}(value: u32) -> u32 {{ let result = value * {i}; // comment\n    result + 1 }}\n"
            ));
        }
        assert_eq!(source.lines().count(), 8000);
        let file = TempFile::with_extension("rs", &source);
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        let start = std::time::Instant::now();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        eprintln!(
            "[file_view] one frame over 8000 lines took {:?}",
            start.elapsed()
        );

        assert!(
            cx.debug_bounds("file-source-line-0").is_some(),
            "the first line is drawn"
        );
        assert!(
            cx.debug_bounds("file-source-line-7999").is_none(),
            "a line far below the viewport is not laid out at all"
        );
    }

    /// F-EDIT: a file taller than its viewport shows bezel's scrollbar over
    /// the source, and one that fits shows none — the bar reports how far
    /// down the reader is, so a document with nowhere to go has nothing to
    /// report. `file-text-bar` is the strip's own selector; bezel returns an
    /// empty element (no selector at all) when there is no overflow.
    #[gpui::test]
    async fn a_long_source_file_shows_a_scrollbar_and_a_short_one_does_not(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut long = String::new();
        for i in 0..400 {
            long.push_str(&format!("let line_{i} = {i};\n"));
        }
        let file = TempFile::with_extension("rs", &long);
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());
        // The bar draws from the handle as the previous frame left it, so
        // give the list one frame to report its overflow before asking.
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "the source surface is drawn"
        );
        assert!(
            cx.debug_bounds("file-text-bar").is_some(),
            "a source taller than the viewport shows its scrollbar"
        );

        let short = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _view) = mounted_file_view(&mut cx.cx, short.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-text-bar").is_none(),
            "a source that fits its viewport shows no scrollbar"
        );
    }

    /// The same contract for the rendered Markdown document in Preview.
    #[gpui::test]
    async fn a_long_markdown_preview_shows_a_scrollbar_and_a_short_one_does_not(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut long = String::from("# Title\n\n");
        for i in 0..300 {
            long.push_str(&format!("Paragraph {i} with a few words in it.\n\n"));
        }
        let file = TempFile::with_extension("md", &long);
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "the Markdown document is drawn in Preview"
        );
        assert!(
            cx.debug_bounds("file-markdown-bar").is_some(),
            "a document taller than the viewport shows its scrollbar"
        );

        let short = TempFile::with_extension("md", "# Title\n\nOne line.\n");
        let (mut cx, _view) = mounted_file_view(&mut cx.cx, short.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-bar").is_none(),
            "a document that fits its viewport shows no scrollbar"
        );
    }
}

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
//! - the conflict banner and the Markdown row — formatting controls plus
//!   the Code/Preview icons — are rendered here as interactive controls
//!   over the model operations. There is no header bar above them: the
//!   file's name and its dirty mark belong to the tab that hosts this
//!   view, and repeating them here cost a whole row of the document.

use bezel::motion::Painter;
use bezel::ui::scroll::{self, ScrollbarState};
use bezel::ui::tooltip::Tooltip;
use gpui::{
    AnyElement, App, BorderStyle, Bounds, Context, CursorStyle, DispatchPhase, Edges, Element,
    ElementId, FocusHandle, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior,
    InspectorElementId, KeyDownEvent, LayoutId, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Rgba, ScrollHandle,
    StyledText, Subscription, Task, UnderlineStyle, UniformListScrollHandle, Window, anchored,
    canvas, deferred, div, point, prelude::*, px, quad, size, transparent_black, uniform_list,
};
use sirio_markdown::{Document, FileSystemEvent, FileSystemEventMonitor, parse};
use sirio_project::resolve_file_link;
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
use crate::file_context_menu::{self, FileContextAction, FileContextFacts, ItemState};
use crate::horizontal_scroll::{self, HorizontalBarState};
use crate::loading;
use crate::sidebar::icons::{Icon, IconElement, IconSize};

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

/// One thing an offer's card can do, and what clicking it says.
///
/// The event is the currency the context menu's `FileContextRoute::App`
/// rows already pay in: a card action reaches the workspace through
/// `FileViewEvent`, the one path that is wired, rather than a second one
/// built beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageAction {
    pub label: String,
    pub event: FileViewEvent,
}

/// A message the shell raises about an open file: a definition that does
/// not exist, a save that failed, a language table that would not parse.
///
/// It is deliberately not the same thing as the messages `ViewState` shows
/// ("Unable to open this file."). Those describe a surface that genuinely
/// has nothing to display and are right to fill it. These answer something
/// the user just did while the file is sitting there perfectly readable, so
/// filling the surface with one throws away the very thing being talked
/// about — and a failed save is the sharpest case, because it erases the
/// text the user is trying to rescue. The two shared one field once; that
/// is the whole of this bug.
struct TransientMessage {
    text: String,
    /// Window-absolute point of the gesture that provoked it, when there
    /// was one. `None` for messages nothing pointed at — a failed save, a
    /// server that would not launch — which settle into the view's corner
    /// instead of appearing at a stale coordinate.
    at: Option<Point<Pixels>>,
    /// The buttons drawn under the sentence. Empty for a message, which is
    /// news and has nothing to offer; filled by `offer`, where each entry
    /// carries the event its click emits. An empty vector draws nothing,
    /// which is what keeps `set_message`'s callers exactly as they were.
    actions: Vec<MessageAction>,
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
    /// A message that answers something the user just did: a definition
    /// that does not exist, a save that failed, a language server that
    /// would not start. It is *about* the file, never instead of it —
    /// see `TransientMessage` for why that distinction is load-bearing.
    message: Option<TransientMessage>,
    /// Where the gesture asking "where is this defined?" happened,
    /// window-absolute, so the answer can be put back where the question
    /// was asked. Recorded when the request is made because the two places
    /// that can ask — the context menu and platform-click — both know the
    /// point and neither survives the round trip to the server.
    definition_gesture: Option<Point<Pixels>>,
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
    /// The column a vertical move is trying to hold, paired with the caret
    /// offset it was computed for. Walking down through a short line and
    /// back up returns to the column the caret started from rather than to
    /// where the short line clipped it.
    ///
    /// The pairing is what retires it, from both ends: `move_caret` clears
    /// it for every keyboard move, and every other writer of `caret` -- a
    /// click, a word select, an edit -- leaves the paired offset behind, so
    /// a stale column cannot be read back even from a path that has never
    /// heard of this field.
    goal_column: Option<(usize, usize)>,
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
    source_horizontal_bar: HorizontalBarState,
    preview: ScrollHandle,
    preview_bar: ScrollbarState,
}

impl SurfaceScroll {
    fn new(painter: Painter) -> Self {
        Self {
            source: UniformListScrollHandle::new(),
            source_bar: ScrollbarState::new(painter),
            source_horizontal_bar: HorizontalBarState::default(),
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
    /// A redirected menu row was clicked: fetch the server this file's
    /// language names rather than navigating anywhere.
    InstallLanguageServer { path: PathBuf },
    /// The offer card's second button: never offer this language's server
    /// again. Carries the path so the workspace can resolve the language
    /// the same way the install button does; silencing itself lives in the
    /// workspace and the settings, never in the view.
    SilenceLanguageServer { path: PathBuf },
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
            message: None,
            markdown_mode: MarkdownMode::Preview,
            source_selection: None,
            caret: 0,
            selection_anchor: None,
            goal_column: None,
            editor_focus: cx.focus_handle().tab_stop(true),
            focus_subscription: None,
            dragging: false,
            editor_blink: caret::Blink::new(),
            editor_caret_sig: (0, None),
            editor_caret_visible: false,
            scroll: SurfaceScroll::new(Painter::of(cx)),
            context_menu: None,
            definition_gesture: None,
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

    /// Raises a message about this file that nothing pointed at — a save
    /// that failed, a server that would not launch. It settles in the
    /// view's corner, over the content rather than in place of it, and
    /// carries no buttons: a message is news, and news is dismissed.
    pub fn set_message(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.message = Some(TransientMessage {
            text: text.into(),
            at: None,
            actions: Vec::new(),
        });
        cx.notify();
    }

    /// Raises an offer about this file: a sentence and the buttons that end
    /// it. Like `set_message` it settles in the corner over the content,
    /// and a click on the body dismisses it — but a click on an action runs
    /// it instead of dismissing, so a failed install finds its button where
    /// it left it. See `message_card`.
    pub fn offer(
        &mut self,
        text: impl Into<String>,
        actions: Vec<MessageAction>,
        cx: &mut Context<Self>,
    ) {
        self.message = Some(TransientMessage {
            text: text.into(),
            at: None,
            actions,
        });
        cx.notify();
    }

    /// Answers the definition gesture where it was made. The recorded point
    /// is consumed: a later message that nothing pointed at must not
    /// inherit this one's coordinate.
    pub fn answer_definition(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.message = Some(TransientMessage {
            text: text.into(),
            at: self.definition_gesture.take(),
            actions: Vec::new(),
        });
        cx.notify();
    }

    pub fn dismiss_message(&mut self, cx: &mut Context<Self>) {
        if self.message.take().is_some() {
            cx.notify();
        }
    }

    /// The card's sentence, if one is up. Test seam for the install-offer
    /// tests in `sirio`, which assert what the card says rather than only
    /// that something is drawn.
    pub fn message_text(&self) -> Option<String> {
        self.message.as_ref().map(|message| message.text.clone())
    }

    /// The card's button labels in draw order. Empty when no card is up or
    /// the card is a plain message.
    pub fn message_action_labels(&self) -> Vec<String> {
        self.message
            .as_ref()
            .map(|message| {
                message
                    .actions
                    .iter()
                    .map(|action| action.label.clone())
                    .collect()
            })
            .unwrap_or_default()
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

    /// Asks where the symbol at `offset` is defined. `at` is where the
    /// gesture happened, kept so the answer can be shown there; both real
    /// callers know it, and it is the last moment anyone does.
    pub fn request_definition(
        &mut self,
        offset: usize,
        at: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        self.definition_gesture = at;
        self.message = None;
        cx.emit(FileViewEvent::GoToDefinition {
            path: self.path.clone(),
            offset,
        });
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

    /// The menu exactly as it would be drawn right now.
    ///
    /// Public for the workspace's own tests, and used by `render` below so
    /// the two cannot drift. The facts reach this view by a push, and a
    /// test that asks the workspace what it *would* say cannot tell a push
    /// that happened from one that did not — which is the failure this
    /// accessor exists to make visible.
    pub fn context_menu_items(&self) -> Vec<file_context_menu::FileContextItem> {
        file_context_menu::items(&self.menu_facts())
    }

    /// Whether this file is Markdown — the one question the Code/Preview
    /// pair and the context menu's Preview entry both hang off. A code file
    /// has nothing to preview, so neither is offered for one.
    fn is_markdown(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.language() == Language::Markdown)
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
            ..self.shell_facts.clone()
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
        // Read the menu's own point before dismissing it: `dismiss` takes
        // the Option, and for Go to Definition that coordinate is the only
        // record of where the user was looking.
        let gesture = self.context_menu;
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
                self.request_definition(self.caret, gesture, cx);
            }
            FileContextAction::FindReferences => {
                cx.emit(FileViewEvent::FindReferences {
                    path: self.path.clone(),
                    offset: self.caret,
                });
            }
            FileContextAction::InstallLanguageServer => {
                cx.emit(FileViewEvent::InstallLanguageServer {
                    path: self.path.clone(),
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

    fn on_editor_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_hover(cx);
        // Both spellings of the chord: Ctrl-A everywhere, ⌘A on macOS. The
        // platform half was missing, which cost the select-all-then-copy
        // flow its keyboard route on the reference release platform.
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
            && event.keystroke.key == "a"
        {
            self.select_all();
            cx.notify();
            return;
        }
        // The clipboard chords. Copy, Cut and Paste reached the buffer only
        // from the right-click menu: nothing binds the `FileEditor` key
        // context, and the command-chord guard below returned before any of
        // them could be read, so the keyboard route into the clipboard did
        // not exist. They dispatch through `handle_context_action` so the
        // two routes cannot answer the same key differently.
        //
        // Unlike the terminal's `ctrl-shift-c`/`v` (which leaves plain
        // Ctrl-C to the PTY as the interrupt), a source surface is an
        // ordinary text field and takes the unshifted chord.
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            let action = match event.keystroke.key.as_str() {
                "c" => Some(FileContextAction::Copy),
                "x" => Some(FileContextAction::Cut),
                "v" => Some(FileContextAction::Paste),
                _ => None,
            };
            // Asked before acting rather than inside each arm: a paste while
            // the Markdown preview is up would rewrite the buffer at a caret
            // the reader cannot see, and a copy would take a source range
            // nothing on screen shows as selected.
            if let Some(action) = action.filter(|_| self.source_surface_ready()) {
                self.handle_context_action(action, window, cx);
                return;
            }
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
            "up" => self.move_vertical(false, 1, extend),
            "down" => self.move_vertical(true, 1, extend),
            "pageup" => self.move_vertical(false, self.page_lines(), extend),
            "pagedown" => self.move_vertical(true, self.page_lines(), extend),
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

    /// Whether the raw source surface is the thing on screen and holding a
    /// buffer that can be read and written. The Markdown preview mounts
    /// under the same focusable element, so a chord that acts on the source
    /// has to ask, and a file still loading has no buffer to act on at all.
    fn source_surface_ready(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
            && self.effective_mode() == MarkdownMode::Code
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

    /// Vertical movement is the only caret move that has to invent a
    /// column, because the line it lands on may be shorter than the one it
    /// left. `goal_column` is what lets the caret walk down through a short
    /// line and come back up to where it started.
    ///
    /// The document edge is not a dead end: a `down` already on the last
    /// line lands at that line's end. Deliberately its end and not
    /// `buffer.len()`, which a trailing newline puts one byte past the last
    /// row the list draws -- `render_source_line` would find no row willing
    /// to carry the caret, and it would simply stop being painted.
    fn move_vertical(&mut self, down: bool, lines: usize, extend: bool) {
        let Some(editor) = self.editor() else {
            return;
        };
        let buffer = editor.buffer().to_owned();
        let caret = self.caret.min(buffer.len());
        let column = match self.goal_column {
            Some((offset, column)) if offset == caret => column,
            _ => column_at(&buffer, caret),
        };

        let mut line_start = line_start_at(&buffer, caret);
        let mut moved = 0;
        for _ in 0..lines {
            let next = if down {
                next_line_start(&buffer, line_start)
            } else {
                previous_line_start(&buffer, line_start)
            };
            let Some(next) = next else {
                break;
            };
            line_start = next;
            moved += 1;
        }

        let position = if moved == 0 {
            // Already against the edge. The key still means something, and
            // what it means there is the very start or the very end.
            if down {
                line_end_at(&buffer, line_start)
            } else {
                0
            }
        } else {
            offset_at_column(&buffer, line_start, column)
        };

        self.move_caret(position, extend, &buffer);
        // `move_caret` just cleared the column; a vertical move is the one
        // caller that means to carry it forward.
        self.goal_column = Some((self.caret, column));
        // Non-strict, so a line already on screen scrolls nothing. Without
        // this a `pagedown` would move the caret exactly one viewport away
        // from the only place it is drawn.
        self.scroll.source.scroll_to_item(
            line_index_at(&buffer, line_start_at(&buffer, self.caret)),
            gpui::ScrollStrategy::Nearest,
        );
    }

    /// How many whole lines the source list is showing, which is what a
    /// page means to `pageup` and `pagedown`.
    ///
    /// The answer comes from the list's own last layout, and `ItemSize` is
    /// worth reading twice: despite the name, `item` is the list's padded
    /// viewport, not a row. A row's height only appears in `contents`,
    /// which gpui fills with `row_height * item_count` -- so the row is
    /// recovered by dividing by the same line count the list was given.
    /// Before the first paint there is nothing to read at all, and a page
    /// is one line rather than a guess.
    fn page_lines(&self) -> usize {
        let Some(size) = self.scroll.source.0.borrow().last_item_size else {
            return 1;
        };
        let Some(editor) = self.editor() else {
            return 1;
        };
        let line_count = line_count_of(editor.buffer());
        let row_height = size.contents.height / line_count as f32;
        if row_height <= px(0.0) {
            return 1;
        }
        ((size.item.height / row_height) as usize).max(1)
    }

    fn move_caret(&mut self, position: usize, extend: bool, buffer: &str) {
        // Every keyboard move but the vertical one abandons the remembered
        // column; `move_vertical` puts its own back afterwards.
        self.goal_column = None;
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

    fn render_state(
        &self,
        theme: Theme,
        entity: gpui::Entity<Self>,
        caret_visible: bool,
        caret_offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    let is_markdown = editor.language() == Language::Markdown;
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
                        .when(is_markdown, |this| {
                            this.child(render_markdown_bar(mode, theme, entity.clone()))
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

            let items = self.context_menu_items();
            let mut previous_group = None;
            for (index, item) in items.iter().enumerate() {
                if previous_group.is_some_and(|group| group != item.group) {
                    menu = menu.child(div().w_full().h(px(1.0)).my(px(4.0)).bg(theme.border));
                }
                previous_group = Some(item.group);

                let item_entity = entity.clone();
                let selector = format!("file-context-item-{index}");
                let debug_selector = selector.clone();
                let state = item.state.clone();
                let is_disabled = matches!(state, ItemState::Unavailable(_));
                // A redirected row acts on what is missing, not on what it
                // is labelled with; an unavailable row acts on nothing.
                let effect = match &state {
                    ItemState::Ready | ItemState::Unavailable(_) => item.action,
                    ItemState::Redirected { to, .. } => *to,
                };
                let note = match &state {
                    ItemState::Ready => None,
                    ItemState::Unavailable(reason) => Some(reason.clone()),
                    ItemState::Redirected { note, .. } => Some(note.clone()),
                };
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
                                        view.handle_context_action(effect, window, cx);
                                    });
                                })
                        })
                        .child(item.label)
                        .when_some(note, |this, reason| {
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
        // Where the answer goes. A gesture puts it back where the question
        // was asked; a message nothing pointed at settles into the corner,
        // because a stale coordinate is worse than an honest default.
        // Either way it sits *over* the file — priority 2 so it stays above
        // the menu that may have raised it.
        let message = self.message.as_ref().map(|message| {
            let card = message_card(&message.text, &message.actions, theme, entity.clone());
            match message.at {
                Some(at) => deferred(
                    anchored()
                        .position(at + point(px(8.), px(14.)))
                        .snap_to_window()
                        .child(card),
                )
                .priority(2)
                .into_any_element(),
                None => div()
                    .absolute()
                    .bottom(px(16.0))
                    .right(px(16.0))
                    .child(card)
                    .into_any_element(),
            }
        });
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .on_mouse_down(MouseButton::Right, cx.listener(Self::open_context_menu))
            .when_some(context_menu, |this, menu| this.child(menu))
            .when_some(hover_card, |this, card| this.child(card))
            .when_some(message, |this, card| this.child(card))
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
///
/// The two options are icons, not words: they sit at the right end of the
/// Markdown row, where a pair of labels would read as two more formatting
/// controls. Each one keeps its word in a tooltip, because an eye and a
/// pair of angle brackets are recognisable but not self-naming, and the
/// selector names (`file-mode-preview`, `file-mode-code`) are unchanged —
/// what the control *is* did not change, only how it is drawn.
fn render_mode_switch(
    mode: MarkdownMode,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let preview = render_mode_option(
        Icon::Eye,
        "Preview",
        "file-mode-preview",
        mode == MarkdownMode::Preview,
        MarkdownMode::Preview,
        entity.clone(),
        theme,
    );
    let code = render_mode_option(
        Icon::Code,
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
        .child(preview)
        .child(code)
}

fn render_mode_option(
    icon: Icon,
    tooltip: &'static str,
    selector: &'static str,
    active: bool,
    mode: MarkdownMode,
    entity: gpui::Entity<FileView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .px(px(6.0))
        .py(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radii.chip)
        .text_color(if active { theme.text } else { theme.text_faint })
        .when(active, |this| this.bg(theme.element_active))
        .hover(|style| style.bg(theme.element_hover))
        .tooltip(move |window, cx| Tooltip::text(tooltip, window, cx))
        .on_click(move |_, _, cx| {
            entity.update(cx, |view, cx| view.set_markdown_mode(mode, cx));
        })
        .child(IconElement::new(icon, IconSize::Small))
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

/// The Markdown row: the formatting controls (F-EDIT-02) on the left, the
/// Code/Preview icons (F-EDIT-01) at its right end.
///
/// It is drawn for **every** Markdown file, in both modes — not only while
/// editing, as the formatting toolbar alone used to be. That is load-bearing
/// now that the editor has no header: the mode icons are the only way back
/// out of Preview, so the row that carries them cannot be Code-only or
/// Preview would be a dead end.
///
/// The formatting controls themselves stay Code-only: Preview is a reading
/// surface, and there is nothing rendered there to format. The link URL is a
/// deterministic placeholder until the view has a text prompt seam of its own.
fn render_markdown_bar(
    mode: MarkdownMode,
    theme: Theme,
    file_view: gpui::Entity<FileView>,
) -> impl IntoElement {
    let formatting = file_view.clone();
    div()
        .id("file-markdown-row")
        .debug_selector(|| "file-markdown-row".into())
        .w_full()
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .flex()
        .items_center()
        .gap(theme.spacing.titlebar_control_spacing)
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface_raised)
        .when(mode == MarkdownMode::Code, move |this| {
            this.child(render_format_button(
                "B",
                "file-format-bold",
                MarkdownFormatOp::Bold,
                formatting.clone(),
                theme,
            ))
            .child(render_format_button(
                "I",
                "file-format-italic",
                MarkdownFormatOp::Italic,
                formatting.clone(),
                theme,
            ))
            .child(render_format_button(
                "H",
                "file-format-heading",
                MarkdownFormatOp::Heading,
                formatting.clone(),
                theme,
            ))
            .child(render_format_button(
                "List",
                "file-format-list",
                MarkdownFormatOp::List,
                formatting.clone(),
                theme,
            ))
            .child(render_format_button(
                "Link",
                "file-format-link",
                MarkdownFormatOp::Link {
                    url: "https://example.com".to_owned(),
                },
                formatting,
                theme,
            ))
        })
        // The spacer is what puts the icons at the *end* of the row rather
        // than beside the last formatting control — and it is why the row
        // reads the same in Preview, where there is nothing to its left.
        .child(div().flex_1())
        .child(render_mode_switch(mode, theme, file_view))
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
                let text = buffer.get(start..end).unwrap_or_default();
                let underlines = diagnostic_underlines(view.diagnostics(), text, start);
                render_source_line(
                    index,
                    text.to_owned(),
                    start,
                    end,
                    language,
                    theme,
                    row_entity.clone(),
                    selection,
                    caret_visible,
                    caret_offset,
                    &syntax_palette,
                    &underlines,
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
    let viewport_width = source_handle.bounds().size.width;
    let overflow_width = source_handle.max_offset().x;
    let horizontal_offset = source_handle.offset().x;
    let horizontal_bar = if scroll::thumb(
        viewport_width,
        overflow_width,
        horizontal_offset,
        scroll::MIN_THUMB,
    )
    .is_some()
    {
        let drag_handle = source_handle.clone();
        horizontal_scroll::bar(
            "file-horizontal-bar",
            viewport_width,
            overflow_width,
            horizontal_offset,
            &scroll.source_horizontal_bar,
            move |x, cx| {
                drag_handle.set_offset(point(x, drag_handle.offset().y));
                entity.update(cx, |_, cx| cx.notify());
            },
        )
    } else {
        let watched = source_handle.clone();
        canvas(
            move |_, window, _| {
                if scroll::thumb(
                    watched.bounds().size.width,
                    watched.max_offset().x,
                    watched.offset().x,
                    scroll::MIN_THUMB,
                )
                .is_some()
                {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    };
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
                ))
                .child(horizontal_bar),
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
    underlines: &[(Range<usize>, DiagnosticSeverity)],
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
        // The gutter is the line number alone. It used to carry a 10px
        // severity dot as well; the squiggle on the code says the same
        // thing where the problem actually is, so the column went with it.
        .child(
            div()
                .w(px(42.0))
                .flex_none()
                .text_color(theme.text_faint)
                .child(format!("{:>5} ", index + 1)),
        )
        .child(EditableLine::new(
            ("file-line-text", index),
            line,
            start,
            language,
            theme,
            entity,
            selection,
            syntax_palette,
            caret,
            underlines,
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

/// F-EDIT-07's code path stays Sirio's no-wrap editor; only token
/// classification is delegated — to `sirio_syntax`, which answers for
/// bezel's own seven grammars and for the seventeen it adds.
///
/// The tag is [`Language::fence_tag`] rather than a second table mapping
/// variants to grammar names. There used to be one, it covered eight of the
/// twenty-four variants, and the other sixteen — Java among them — reached
/// this function and left with an empty `Vec`: parsed, measured, laid out
/// and painted in one colour, with nothing anywhere reporting a gap. One
/// list is harder to leave half-finished than two.
pub(crate) fn code_spans(language: Language, line: &str) -> Vec<CodeSpan> {
    if language.is_plain_text() {
        return Vec::new();
    }
    sirio_syntax::highlight(line, language.fence_tag())
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
    /// The line's own font and size, kept so an empty selected line can ask
    /// the font for a character's advance — there is no glyph to measure.
    font: gpui::Font,
    font_size: Pixels,
    /// Set on mouse-down, consumed on mouse-up: the down position and
    /// whether the platform modifier was held, so a same-position mouse-up
    /// on a link (not a drag) can open it (F-CORE-FILE-04) while a plain
    /// click still only places the caret.
    pressed: std::rc::Rc<std::cell::Cell<Option<(usize, bool)>>>,
}

/// What one line shows of a selection, as byte offsets local to it, or
/// `None` when the selection does not reach it.
///
/// An empty line the selection passes through answers `Some(0..0)` — a
/// real answer meaning "selected, but with no text to measure". It used
/// to be indistinguishable from "not selected", so dragging across a
/// paragraph break left the blank lines looking untouched; the painter
/// gives that case a character's width instead of a rectangle of nothing.
///
/// A selection that stops exactly where an empty line begins has consumed
/// the previous line's newline and none of this one, so it does not claim
/// it.
fn selected_run_on_line(
    selection: Selection,
    line_start: usize,
    line_len: usize,
) -> Option<Range<usize>> {
    if selection.is_collapsed() {
        return None;
    }
    if line_len == 0 {
        return (selection.start <= line_start && line_start < selection.end).then_some(0..0);
    }
    let start = selection.start.max(line_start);
    let end = selection.end.min(line_start + line_len);
    (start < end).then(|| start - line_start..end - line_start)
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
        syntax_palette: &bezel::theme::SyntaxPalette,
        caret_offset: Option<usize>,
        underlines: &[(Range<usize>, DiagnosticSeverity)],
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
        let highlights = line_highlights(
            language,
            &line,
            &links,
            underlines,
            &theme,
            syntax_palette,
        );
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
            font: gpui::font(theme.typography.code_family),
            font_size: theme.typography.code_size,
            links,
            pressed: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// One character's advance in this line's font.
    ///
    /// `em_advance` rather than a shaped space: the code surface is
    /// monospace, so every glyph shares the advance, and this asks the
    /// font instead of laying out text that is not in the buffer.
    fn character_width(&self, window: &Window) -> Pixels {
        let font_id = window.text_system().resolve_font(&self.font);
        window
            .text_system()
            .em_advance(font_id, self.font_size)
            .unwrap_or(self.font_size / 2.0)
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
        let Some(run) = selected_run_on_line(selection, self.line_start, self.line_len) else {
            return;
        };
        let layout = self.text.layout();
        let line_height = layout.line_height();
        let start_position = layout.position_for_index(run.start).unwrap_or(bounds.origin);
        let end_position = if run.is_empty() {
            // An empty line has no glyph to measure, so a band drawn
            // between its own two ends is a rectangle of nothing. Give it
            // one character — the band a blank line inside a selection is
            // expected to show — rather than the row's whole width, which
            // would make the emptiest lines the loudest.
            gpui::point(start_position.x + self.character_width(window), bounds.origin.y)
        } else {
            layout
                .position_for_index(run.end)
                .unwrap_or(gpui::point(bounds.right(), bounds.origin.y))
        };
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

    /// The text node, wrapped in one that grows to fill the row.
    ///
    /// `StyledText` measures a line exactly as wide as its glyphs, and
    /// `prepaint` below hangs this line's hitbox on those bounds — so
    /// without the wrapper an empty line gets an empty hitbox,
    /// `is_hovered` is never true for it, and every mouse handler on the
    /// line is gated behind a rectangle nothing can be inside. The same
    /// arithmetic made the space to the right of any short line dead;
    /// the empty line was only the case where that space was the whole
    /// line.
    ///
    /// `flex_grow` claims the row's leftover width, never more: growth
    /// consumes free space, and an intrinsic measure — which is what
    /// `with_width_from_item` performs to size the list from its longest
    /// line — offers none, so the measured width is still the text's.
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let (text, ()) = self.text.request_layout(id, inspector_id, window, cx);
        let style = gpui::Style {
            flex_grow: 1.0,
            ..Default::default()
        };
        (window.request_layout(style, [text], cx), ())
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
                            let at = Some(event.position);
                            view.update(cx, |view, cx| {
                                view.request_definition(up_global, at, cx)
                            });
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

/// The transient-message card. `menu_surface` for the same reason the
/// context menu uses it: something an interaction puts up to be read must
/// stay opaque when the window blurs. A click anywhere on it dismisses it,
/// which is the dismissal the old full-surface notice never had.
///
/// An offer adds a row of actions under the sentence. A click on one emits
/// that action's own `FileViewEvent` — the same currency the context menu's
/// `FileContextRoute::App` rows pay in — and **does not dismiss**: an
/// install that failed puts the same button back in front of the reader,
/// and a card that vanished on the click would take the retry with it. The
/// body keeps the dismissal for both kinds.
fn message_card(
    text: &str,
    actions: &[MessageAction],
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> AnyElement {
    let mut card = div()
        .id("file-view-message")
        .debug_selector(|| "file-view-message".into())
        .max_w(px(420.0))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .bg(theme.menu_surface())
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radii.control)
        .font_family(theme.typography.ui_family)
        .text_size(theme.typography.base_size)
        .text_color(theme.text)
        .child(text.to_owned());
    if !actions.is_empty() {
        let mut row = div().flex().items_center().gap(px(6.0));
        for (index, action) in actions.iter().enumerate() {
            let selector = format!("file-view-message-action-{index}");
            let debug_selector = selector.clone();
            let event = action.event.clone();
            let action_entity = entity.clone();
            row = row.child(
                div()
                    .id(selector)
                    .debug_selector(move || debug_selector.clone())
                    .px(px(10.0))
                    .py(px(3.0))
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .hover(|style| style.bg(theme.element_hover).text_color(theme.text))
                    // The card below reads a mouse-down on itself as a
                    // click on the body and dismisses on it. The button has
                    // to stop that event before it reaches the card:
                    // stopping the click instead would be a phase too late,
                    // the dismissal having already run.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| {
                        action_entity.update(cx, |_view, cx| cx.emit(event.clone()));
                    })
                    .child(action.label.clone()),
            );
        }
        card = card.child(row);
    }
    card.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        entity.update(cx, |view, cx| view.dismiss_message(cx));
    })
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

/// Where the line holding `offset` begins: just past the newline before
/// it, or the start of the buffer.
fn line_start_at(buffer: &str, offset: usize) -> usize {
    buffer[..offset].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Where the line holding `offset` ends. The newline that closes a line is
/// not part of it, matching the ranges the rows are cut from.
fn line_end_at(buffer: &str, offset: usize) -> usize {
    buffer[offset..]
        .find('\n')
        .map_or(buffer.len(), |newline| offset + newline)
}

/// The start of the line below the one beginning at `line_start`, when
/// there is one.
///
/// A trailing newline does not open a line. The rows are cut with
/// `split_inclusive('\n')`, which ends `"alpha\ngamma\n"` with `gamma\n` as
/// a single chunk and draws two rows, not three -- so the byte past that
/// final newline belongs to no row, and a caret sent there is painted by
/// nobody.
fn next_line_start(buffer: &str, line_start: usize) -> Option<usize> {
    let line_end = line_end_at(buffer, line_start);
    (line_end + 1 < buffer.len()).then_some(line_end + 1)
}

/// The start of the line above the one beginning at `line_start`, when
/// there is one.
fn previous_line_start(buffer: &str, line_start: usize) -> Option<usize> {
    (line_start > 0).then(|| line_start_at(buffer, line_start - 1))
}

/// How many characters into its own line `offset` sits. Characters rather
/// than bytes: the surface draws a monospace grid, so a line of accented
/// text has to carry the caret exactly as far as a line of ASCII.
fn column_at(buffer: &str, offset: usize) -> usize {
    buffer[line_start_at(buffer, offset)..offset].chars().count()
}

/// The offset `column` characters into the line beginning at `line_start`,
/// clamped to that line's end when the line is too short to reach it.
fn offset_at_column(buffer: &str, line_start: usize, column: usize) -> usize {
    let line_end = line_end_at(buffer, line_start);
    buffer[line_start..line_end]
        .char_indices()
        .nth(column)
        .map_or(line_end, |(offset, _)| line_start + offset)
}

/// Which row the line beginning at `line_start` is drawn as.
fn line_index_at(buffer: &str, line_start: usize) -> usize {
    buffer[..line_start].matches('\n').count()
}

/// How many rows the list draws for this buffer -- the same count
/// `render_source` hands `uniform_list`, empty buffer and all.
fn line_count_of(buffer: &str) -> usize {
    buffer.split_inclusive('\n').count().max(1)
}

fn previous_char_boundary(buffer: &str, position: usize) -> usize {
    buffer[..position.min(buffer.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

/// The squiggles one line earns, as byte ranges **local to that line**
/// paired with the severity that gets to colour them.
///
/// Selection is by range overlap, not by `FileDiagnostic::line`: a finding
/// carries the line it *starts* on but a buffer-absolute range, so asking
/// `line == index` leaves every continuation line of a multi-line
/// diagnostic unmarked — which is what the gutter dot this replaced did.
///
/// Overlaps are flattened here, worst severity winning, and the result is
/// disjoint and sorted. That is not tidiness: `combine_highlights` merges
/// the styles of overlapping ranges by folding over a `HashSet`, so two
/// diagnostics both writing `underline` over the same bytes would pick a
/// winner in whatever order that set iterated — a colour that could differ
/// from frame to frame.
fn diagnostic_underlines(
    diagnostics: &[FileDiagnostic],
    line: &str,
    line_start: usize,
) -> Vec<(Range<usize>, DiagnosticSeverity)> {
    let line_end = line_start + line.len();
    let mut spans: Vec<(Range<usize>, DiagnosticSeverity)> = Vec::new();
    for found in diagnostics {
        let span = if found.range.is_empty() {
            // A caret-position finding — "expected `;`" and its kind. It
            // covers no bytes, so it would paint nothing at all, which
            // reads exactly like a clean line. Take the character after
            // the caret, or the one before it at end of line; an empty
            // line has neither and goes unmarked.
            if found.range.start < line_start || found.range.start > line_end {
                continue;
            }
            let at = found.range.start - line_start;
            let after = next_char_boundary(line, at);
            if after > at {
                at..after
            } else {
                previous_char_boundary(line, at)..at
            }
        } else {
            found.range.start.max(line_start) - line_start
                ..found.range.end.min(line_end).max(line_start) - line_start
        };
        if !span.is_empty() {
            spans.push((span, found.severity));
        }
    }
    flatten_worst_first(spans)
}

/// Rewrites possibly-overlapping severity spans as disjoint ones in
/// ascending order, each carrying the worst severity that covered it.
///
/// `DiagnosticSeverity` is ordered worst-first, so "worst wins" is `min`.
/// A gap no span covers stays a gap: two findings that do not touch must
/// come out as two squiggles, not one run bridging the clean text between.
fn flatten_worst_first(
    spans: Vec<(Range<usize>, DiagnosticSeverity)>,
) -> Vec<(Range<usize>, DiagnosticSeverity)> {
    let mut boundaries: Vec<usize> = spans
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .collect();
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut flattened: Vec<(Range<usize>, DiagnosticSeverity)> = Vec::new();
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let Some(severity) = spans
            .iter()
            .filter(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, severity)| *severity)
            .min()
        else {
            continue;
        };
        match flattened.last_mut() {
            // Only contiguous runs of one severity merge — a gap between
            // two findings of the same severity must stay a gap.
            Some((previous, last)) if previous.end == start && *last == severity => {
                previous.end = end;
            }
            _ => flattened.push((start..end, severity)),
        }
    }
    flattened
}

/// Every styled run one source line hands to `StyledText`: the grammar's
/// colours, the Markdown link rules, and the diagnostic squiggles on top.
///
/// The three sets are merged with [`gpui::combine_highlights`] rather than
/// sorted. `with_highlights` ends in `compute_runs`, which walks the ranges
/// assuming each begins at or after the previous one's end — true while
/// only syntax and links were in play, since `code_spans` fires for
/// non-Markdown and `links` for Markdown and the two never met. A
/// diagnostic sits *on* a coloured token by definition, so a sort is no
/// longer enough; `combine_highlights` splits the overlap into disjoint
/// runs and folds the styles. Syntax writes `color` and a diagnostic writes
/// `underline`, so the fold never has to choose between them.
fn line_highlights(
    language: Language,
    line: &str,
    links: &[(Range<usize>, String)],
    underlines: &[(Range<usize>, DiagnosticSeverity)],
    theme: &Theme,
    syntax_palette: &bezel::theme::SyntaxPalette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut painted: Vec<(Range<usize>, HighlightStyle)> = code_spans(language, line)
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
    painted.extend(links.iter().map(|(range, _)| {
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
    painted.sort_by_key(|(range, _)| range.start);
    if underlines.is_empty() {
        return painted;
    }
    gpui::combine_highlights(painted, underline_highlights(underlines, theme)).collect()
}

/// Dresses each span as the squiggle its severity earns.
///
/// Only `underline` is set. The glyphs keep whatever the grammar painted
/// them: a diagnostic says "look here", not "this is a keyword now", and
/// leaving `color` empty is also what lets `combine_highlights` merge this
/// with the syntax runs in either order and land on the same result.
fn underline_highlights(
    spans: &[(Range<usize>, DiagnosticSeverity)],
    theme: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    spans
        .iter()
        .map(|(range, severity)| {
            let color = match severity {
                DiagnosticSeverity::Error => theme.danger,
                DiagnosticSeverity::Warning => theme.warning,
                DiagnosticSeverity::Information | DiagnosticSeverity::Hint => theme.text_faint,
            };
            (
                range.clone(),
                HighlightStyle {
                    underline: Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(color.into()),
                        // Wavy, not straight: a straight rule is already
                        // the Markdown link style in this same surface.
                        wavy: true,
                    }),
                    ..Default::default()
                },
            )
        })
        .collect()
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

    /// The gap this guards: `Language` has recognised twenty-four languages
    /// since F-EDIT-07, and for most of that time eight of them had a
    /// grammar. Opening `Main.java` produced a correctly detected `Java`, a
    /// correct status line, and no colour at all — and nothing in the app
    /// could tell that apart from a file with nothing to classify.
    #[test]
    fn every_language_the_editor_recognises_has_a_grammar() {
        for language in Language::ALL {
            let supported = sirio_syntax::is_supported(language.fence_tag());
            if language.is_plain_text() {
                assert!(
                    !supported,
                    "plain text is the fallback; it must not resolve to a grammar"
                );
            } else {
                assert!(
                    supported,
                    "{} ({}) is offered in the status line with no grammar behind it",
                    language.name(),
                    language.fence_tag()
                );
            }
        }
    }

    #[test]
    fn java_reaches_the_code_surface_classified() {
        // The report was "java ho visto che non c'è": detected, named, and
        // painted like a .txt file.
        let source = "public class Main { int n = 42; } // note";
        let spans = code_spans(Language::Java, source);
        let painted = |needle: &str, kind: bezel::theme::HighlightKind| {
            let at = source.find(needle).expect("needle is in the fixture");
            spans
                .iter()
                .any(|span| span.range == (at..at + needle.len()) && span.kind == kind)
        };
        assert!(painted("public", bezel::theme::HighlightKind::Keyword), "{spans:?}");
        assert!(painted("42", bezel::theme::HighlightKind::Number), "{spans:?}");
        assert!(painted("// note", bezel::theme::HighlightKind::Comment), "{spans:?}");
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
    async fn a_file_message_sits_over_the_file_and_can_be_dismissed(
        cx: &mut gpui::TestAppContext,
    ) {
        // A failed save is the case that makes the rule obvious: the text
        // the user is trying to rescue must stay on screen while they are
        // being told it was not written. This used to replace the file with
        // the sentence, permanently — nothing in the app cleared it.
        let file = TempFile::new("message", "hello\n");

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.set_message("save failed", cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-message").is_some(),
            "a raised message is drawn in the file view"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "and the file is still drawn underneath it, not replaced by it"
        );
        assert!(
            cx.debug_bounds("file-view-message-action-0").is_none(),
            "and it carries no buttons: `set_message` is not an offer"
        );

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.dismiss_message(cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-message").is_none(),
            "dismissing the message removes it from the drawn frame"
        );
    }

    #[gpui::test]
    async fn an_offer_draws_its_actions_and_the_file_underneath(cx: &mut gpui::TestAppContext) {
        // The card that 0.18.0 banned was an *error* arriving uninvited on
        // almost every file. This is an offer, once, carrying its own way to
        // end — and, like every card since 0.17.2, it sits over the file
        // rather than in place of it.
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let probe = file.path().to_path_buf();
        view.update(&mut cx.cx, |view, cx| {
            view.offer(
                "clangd is not on PATH.",
                vec![
                    MessageAction {
                        label: "Install — 114 MB".to_owned(),
                        event: FileViewEvent::InstallLanguageServer {
                            path: probe.clone(),
                        },
                    },
                    // The design's second action, "Don't ask again": the
                    // card's contract is only that each click emits the event
                    // its own action was built with, and which event a caller
                    // puts there is the caller's business.
                    MessageAction {
                        label: "Don't ask again".to_owned(),
                        event: FileViewEvent::ViewFileHistory(probe.clone()),
                    },
                ],
                cx,
            );
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-message").is_some(),
            "the offer is drawn"
        );
        assert!(
            cx.debug_bounds("file-view-message-action-0").is_some(),
            "and so is its first action"
        );
        assert!(
            cx.debug_bounds("file-view-message-action-1").is_some(),
            "and its second"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "over the file, not instead of it"
        );

        // An action click runs the action and leaves the card up. An install
        // that failed offers the same button again, and a card that vanished
        // on the click would have taken the only retry with it.
        let first = cx
            .debug_bounds("file-view-message-action-0")
            .expect("the first action is drawn");
        cx.simulate_click(first.center(), Modifiers::none());
        assert_eq!(
            events.borrow().as_slice(),
            &[FileViewEvent::InstallLanguageServer { path: probe.clone() }],
            "the first button emits the event it was built with"
        );
        let second = cx
            .debug_bounds("file-view-message-action-1")
            .expect("the second action is drawn");
        cx.simulate_click(second.center(), Modifiers::none());
        assert_eq!(
            events.borrow().as_slice(),
            &[
                FileViewEvent::InstallLanguageServer { path: probe.clone() },
                FileViewEvent::ViewFileHistory(probe.clone()),
            ],
            "and the second emits its own, not the first's again"
        );
        assert!(
            cx.debug_bounds("file-view-message").is_some(),
            "an action is not also a dismissal"
        );

        // The body keeps the dismissal, for both kinds of card: a click that
        // is not on an action.
        let card = cx.debug_bounds("file-view-message").expect("drawn");
        let body = card.origin + point(px(2.0), px(2.0));
        for action in ["file-view-message-action-0", "file-view-message-action-1"] {
            let bounds = cx.debug_bounds(action).expect("the action is drawn");
            assert!(
                !bounds.contains(&body),
                "the click has to land on the body, not on {action}"
            );
        }
        cx.simulate_click(body, Modifiers::none());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-view-message").is_none(),
            "a click on the body still dismisses it"
        );
    }

    #[gpui::test]
    async fn a_definition_answer_is_placed_at_the_gesture_that_asked(
        cx: &mut gpui::TestAppContext,
    ) {
        // The whole point of recording the gesture: the answer appears next
        // to where the user was looking, not in the corner and not over the
        // whole file.
        let file = TempFile::new("gesture", "fn thing() -> u32 {\n    7\n}\n");

        cx.update(|cx| {
            Theme::init(cx);
            ::editor::init(cx);
        });
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let asked_at = point(px(220.), px(140.));
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| {
                view.request_definition(4, Some(asked_at), cx);
                view.answer_definition("No definition found", cx);
            });
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let bounds = cx
            .debug_bounds("file-view-message")
            .expect("the answer is drawn");
        assert!(
            (bounds.origin.x - asked_at.x).abs() < px(40.)
                && (bounds.origin.y - asked_at.y).abs() < px(40.),
            "the answer is drawn beside the gesture at {asked_at:?}, not at {:?}",
            bounds.origin
        );
        assert!(
            bounds.size.height < px(120.),
            "and it is a card, not a takeover: {:?} high",
            bounds.size.height
        );

        // The point is consumed with the answer: a later message that
        // nothing pointed at must not inherit this coordinate.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.set_message("save failed", cx));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let corner = cx
            .debug_bounds("file-view-message")
            .expect("the later message is drawn");
        assert!(
            (corner.origin.y - asked_at.y).abs() > px(40.),
            "a message nothing pointed at does not reuse the old gesture point"
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

    /// Typing is the other half of the same failure: with no hitbox there is
    /// no click, with no click the caret never arrives, and the keystroke
    /// lands wherever the caret was left.
    #[gpui::test]
    async fn an_empty_line_accepts_typed_text(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\n\nbeta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx
            .debug_bounds("file-source-line-1")
            .expect("the empty line is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_input("x");
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view
                .editor()
                .expect("loaded")
                .buffer()
                .to_owned()),
            "alpha\nx\nbeta\n",
            "the character belongs on the empty line, not wherever the caret was"
        );
    }

    /// The empty line was only the extreme case. The dead space to the right
    /// of every short line came from the same measurement, and an editor is
    /// expected to take a click there as "the end of this line".
    #[gpui::test]
    async fn a_click_past_the_end_of_a_line_lands_at_its_end(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\n\nbeta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(
            point(row.origin.x + row.size.width - px(8.), row.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            5,
            "a click in the empty space right of `alpha` belongs at its end"
        );
    }

    /// An empty line is still a line: it can be clicked into, typed on, and
    /// it carries the caret. "alpha\n\nbeta\n" puts the empty one at byte 6.
    #[gpui::test]
    async fn an_empty_line_can_be_clicked_into(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\n\nbeta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx
            .debug_bounds("file-source-line-1")
            .expect("the empty line is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            6,
            "a click on the empty line must put the caret on it"
        );
    }

    /// `up` and `down` are the two keys an editor is expected to answer and
    /// this one never did: they fall past every arm of the match, and an
    /// arrow carries no `key_char`, so the handler returns without moving
    /// anything. "alpha\nbeta\ngamma\n" puts `beta` at byte 6.
    #[gpui::test]
    async fn the_arrow_keys_walk_the_caret_between_lines(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\nbeta\ngamma\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("home down");
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            6,
            "down from the start of `alpha` belongs at the start of `beta`"
        );
    }

    /// The column the caret came from has to survive a short line, which is
    /// the whole difference between vertical movement here and in an editor
    /// that recomputes the column every press.
    /// "alphabet\nab\nalphabet\n": 0..8, 9..11, 12..20.
    #[gpui::test]
    async fn a_short_line_does_not_swallow_the_column(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alphabet\nab\nalphabet\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("end down");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            11,
            "`ab` is too short for column 8, so the caret stops at its end"
        );

        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            20,
            "the third line is long enough again, so column 8 comes back"
        );
    }

    /// The remembered column belongs to vertical movement alone. Anything
    /// else that moves the caret retires it, or the next `down` would aim at
    /// a column the caret left two presses ago.
    #[gpui::test]
    async fn a_horizontal_move_retires_the_remembered_column(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alphabet\nab\nalphabet\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("end down left down");
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.caret),
            13,
            "column 1, where `left` left the caret -- not the remembered 8"
        );
    }

    /// `down` on the last line is neither a no-op nor a leap to
    /// `buffer.len()`. The trailing newline makes that byte belong to no
    /// drawn row, and `render_source_line` paints a caret only on a row it
    /// can place it on -- so the caret would silently stop being drawn.
    #[gpui::test]
    async fn down_on_the_last_line_stops_where_the_line_does(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\nbeta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-1").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("home down");
        cx.run_until_parked();

        let (caret, len) = view.read_with(&cx.cx, |view, _| {
            (view.caret, view.editor().expect("loaded").buffer().len())
        });
        assert_eq!(len, 11, "`alpha\nbeta\n` is eleven bytes");
        assert_eq!(
            caret, 10,
            "the end of `beta`, not the byte after the newline that closes it"
        );
    }

    /// Symmetrically, `up` against the top is how the caret reaches byte 0.
    #[gpui::test]
    async fn up_on_the_first_line_reaches_the_start_of_the_document(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "alpha\nbeta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("end up");
        cx.run_until_parked();

        assert_eq!(view.read_with(&cx.cx, |view, _| view.caret), 0);
    }

    /// Shift makes the same movement extend rather than jump, which is the
    /// only reason `move_vertical` routes through `move_caret` at all.
    #[gpui::test]
    async fn shift_down_extends_the_selection_a_line_at_a_time(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "alpha\nbeta\ngamma\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("home shift-down");
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.selected_text()),
            Some("alpha\n".to_owned()),
            "shift-down takes the line and the newline that ends it"
        );
    }

    /// A page is however many lines the list is showing, so this only means
    /// anything on a file taller than the window.
    #[gpui::test]
    async fn pagedown_moves_further_than_a_single_line(cx: &mut gpui::TestAppContext) {
        let body: String = (0..400).map(|n| format!("line {n}\n")).collect();
        let file = TempFile::with_extension("rs", &body);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());

        let row = cx.debug_bounds("file-source-line-0").expect("drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("home pagedown");
        cx.run_until_parked();

        let caret = view.read_with(&cx.cx, |view, _| view.caret);
        let landed = body[..caret].matches('\n').count();
        assert!(
            landed > 1,
            "a page is more than one line; the caret landed on line {landed}"
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

        view.update(&mut cx.cx, |view, cx| view.request_definition(12, None, cx));
        cx.cx.run_until_parked();
        assert_eq!(&*events.borrow(), &[12]);
    }

    /// The squiggle replaced the gutter dot, and the invariant the dot was
    /// built around outlives it: a file that grows an error must not shift
    /// every line number sideways.
    #[gpui::test]
    async fn a_diagnostic_squiggles_the_code_and_leaves_the_gutter_alone(
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
            cx.debug_bounds("file-line-mark-1").is_none(),
            "the gutter dot is gone: the warning is on the code, not beside it"
        );
        let after = cx
            .debug_bounds("file-source-line-1")
            .expect("line 1 is still drawn");
        assert_eq!(
            before.origin.x, after.origin.x,
            "a file with an error must not shift every line number sideways"
        );
        assert_eq!(
            view.read_with(&cx.cx, |view, _| {
                let editor = view.editor().expect("loaded");
                let buffer = editor.buffer();
                // Line 1 of "fn main() {\n    let x = 1;\n}\n" is the run
                // starting at byte 12; the finding sits on `x` at 16.
                diagnostic_underlines(view.diagnostics(), &buffer[12..23], 12)
            }),
            vec![(4..5, DiagnosticSeverity::Warning)],
            "and the squiggle lands on `x` itself, not on the whole line"
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

    /// An empty publish is how a server says the errors are gone. Dropping
    /// it on the floor leaves squiggles under text that is now fine.
    #[gpui::test]
    async fn an_empty_publish_clears_the_squiggles(cx: &mut gpui::TestAppContext) {
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
        assert!(view.read_with(&cx.cx, |view, _| view.diagnostics().is_empty()));
        assert!(
            view.read_with(&cx.cx, |view, _| diagnostic_underlines(
                view.diagnostics(),
                "a",
                0
            ))
            .is_empty(),
            "and the line it marked underlines nothing"
        );
    }

    /// Tests for `selected_run_on_line` — which part of a line a selection
    /// covers, and the empty-line case that used to come out as "nothing".
    /// Vertical movement has to agree with how the rows were cut, and the
    /// rows are cut with `split_inclusive('\n')`. Disagreeing here walks the
    /// caret to a byte no row is willing to draw.
    mod line_walk {
        use super::*;

        #[test]
        fn a_trailing_newline_does_not_open_a_line() {
            let buffer = "alpha\nbeta\ngamma\n";
            assert_eq!(next_line_start(buffer, 0), Some(6), "alpha -> beta");
            assert_eq!(next_line_start(buffer, 6), Some(11), "beta -> gamma");
            assert_eq!(
                next_line_start(buffer, 11),
                None,
                "the byte past the closing newline is not a line of its own"
            );
        }

        #[test]
        fn a_buffer_that_does_not_end_in_a_newline_ends_on_its_last_line() {
            assert_eq!(next_line_start("alpha\nbeta", 6), None);
        }

        #[test]
        fn an_empty_line_is_walked_like_any_other() {
            let buffer = "alpha\n\nbeta\n";
            assert_eq!(next_line_start(buffer, 0), Some(6), "alpha -> the empty line");
            assert_eq!(next_line_start(buffer, 6), Some(7), "the empty line -> beta");
            assert_eq!(previous_line_start(buffer, 7), Some(6));
            assert_eq!(previous_line_start(buffer, 0), None, "nothing above the first");
        }

        #[test]
        fn a_line_index_counts_the_rows_above_it() {
            let buffer = "alpha\nbeta\ngamma\n";
            assert_eq!(line_index_at(buffer, 0), 0);
            assert_eq!(line_index_at(buffer, 6), 1);
            assert_eq!(line_index_at(buffer, 11), 2);
        }
    }

    /// A column is a position on a monospace grid, so it is counted in
    /// characters. Counting bytes would make an accented line move the
    /// caret twice as far as an ASCII one.
    mod columns {
        use super::*;

        // Five two-byte characters, so the first line is ten bytes wide but
        // only five columns wide. `abcde` starts at 11.
        const BUFFER: &str = "\u{e0}\u{e8}\u{ec}\u{f2}\u{f9}\nabcde\n";

        #[test]
        fn a_column_is_counted_in_characters_not_bytes() {
            assert_eq!(column_at(BUFFER, 10), 5, "five characters in, not ten");
        }

        #[test]
        fn the_same_column_lands_on_the_same_character_of_an_ascii_line() {
            assert_eq!(offset_at_column(BUFFER, 11, 2), 13, "the third character");
            assert_eq!(offset_at_column(BUFFER, 0, 2), 4, "and two characters in is four bytes in");
        }

        #[test]
        fn a_line_too_short_to_reach_the_column_clamps_to_its_end() {
            let buffer = "alphabet\nab\n";
            assert_eq!(offset_at_column(buffer, 9, 6), 11, "`ab` has no column 6");
        }
    }

    mod selection_spans {
        use super::*;

        // "alpha\n\nbeta\n": line 0 is 0..5, the empty line sits at 6, and
        // line 2 is 7..11.
        const BUFFER: &str = "alpha\n\nbeta\n";

        fn span(from: usize, to: usize) -> Selection {
            Selection::new(BUFFER, from, to).expect("valid range")
        }

        #[test]
        fn an_empty_line_the_selection_passes_through_is_selected() {
            assert_eq!(
                selected_run_on_line(span(0, 11), 6, 0),
                Some(0..0),
                "a real answer: selected, with no text to measure"
            );
        }

        #[test]
        fn an_empty_line_the_selection_never_reaches_is_not_selected() {
            assert_eq!(selected_run_on_line(span(0, 3), 6, 0), None);
        }

        /// The selection stops exactly where the empty line begins, so it
        /// has consumed the previous line's newline and nothing of this one.
        #[test]
        fn a_selection_ending_where_an_empty_line_begins_does_not_claim_it() {
            assert_eq!(selected_run_on_line(span(0, 6), 6, 0), None);
        }

        #[test]
        fn an_empty_line_that_anchors_the_selection_is_selected() {
            assert_eq!(selected_run_on_line(span(6, 11), 6, 0), Some(0..0));
        }

        #[test]
        fn a_partly_selected_line_reports_only_the_selected_run() {
            // Bytes 2..5 of "alpha" — "pha".
            assert_eq!(selected_run_on_line(span(2, 5), 0, 5), Some(2..5));
        }

        #[test]
        fn a_line_fully_inside_the_selection_reports_all_of_it() {
            assert_eq!(selected_run_on_line(span(0, 11), 0, 5), Some(0..5));
        }

        #[test]
        fn a_collapsed_selection_is_a_caret_and_selects_nothing() {
            assert_eq!(selected_run_on_line(Selection::point(6), 6, 0), None);
            assert_eq!(selected_run_on_line(Selection::point(2), 0, 5), None);
        }
    }

    /// Tests for `diagnostic_underlines` — the span arithmetic that decides
    /// what a line underlines, kept pure so it is testable without a window.
    mod underline_spans {
        use super::*;

        fn warning(range: Range<usize>) -> FileDiagnostic {
            FileDiagnostic { line: 0, range, severity: DiagnosticSeverity::Warning,
                message: "unused".into() }
        }

        fn error(range: Range<usize>) -> FileDiagnostic {
            FileDiagnostic { line: 0, range, severity: DiagnosticSeverity::Error,
                message: "boom".into() }
        }

        #[test]
        fn a_diagnostic_underlines_its_own_span_not_the_whole_line() {
            let line = "let x = 1;";
            let found = diagnostic_underlines(&[warning(4..5)], line, 0);
            assert_eq!(found, vec![(4..5, DiagnosticSeverity::Warning)]);
        }

        #[test]
        fn a_diagnostic_on_another_line_underlines_nothing_here() {
            // Line two of "a\nb\n" is the single byte at offset 2.
            let found = diagnostic_underlines(&[warning(0..1)], "b", 2);
            assert!(found.is_empty(), "a span that ends before this line starts");
        }

        /// The span arithmetic is what makes this possible at all: a
        /// diagnostic carries one `line` (its first) but a buffer-absolute
        /// range, so selecting by `line` alone leaves every continuation
        /// line unmarked — the bug the gutter dot had.
        #[test]
        fn a_diagnostic_spanning_two_lines_underlines_the_part_on_each() {
            // "let a = 1;\nlet b = 2;\n" — a span from byte 4 to byte 15
            // covers "x = 1;" on line one and "let" on line two.
            let first = diagnostic_underlines(&[error(4..15)], "let a = 1;", 0);
            let second = diagnostic_underlines(&[error(4..15)], "let b = 2;", 11);
            assert_eq!(first, vec![(4..10, DiagnosticSeverity::Error)],
                "clipped at the first line's end, not run past it");
            assert_eq!(second, vec![(0..4, DiagnosticSeverity::Error)],
                "the continuation line underlines its own leading bytes");
        }

        /// `combine_highlights` folds the styles of overlapping ranges by
        /// iterating a `HashSet`, so two diagnostics writing `underline`
        /// over the same text would pick a winner non-deterministically.
        /// Flattening here is what makes the painted colour stable.
        #[test]
        fn the_worst_severity_wins_where_two_diagnostics_overlap() {
            let line = "let x = 1;";
            let found = diagnostic_underlines(&[warning(2..8), error(0..5)], line, 0);
            assert_eq!(
                found,
                vec![(0..5, DiagnosticSeverity::Error), (5..8, DiagnosticSeverity::Warning)],
                "disjoint spans, the error keeping every byte it covers"
            );
        }

        #[test]
        fn two_diagnostics_that_do_not_touch_stay_two_spans() {
            let line = "let x = y;";
            let found = diagnostic_underlines(&[warning(4..5), error(8..9)], line, 0);
            assert_eq!(found, vec![
                (4..5, DiagnosticSeverity::Warning),
                (8..9, DiagnosticSeverity::Error),
            ]);
        }

        /// "expected `;`" arrives as an empty range. An empty range paints
        /// nothing at all, which is indistinguishable from a clean line.
        #[test]
        fn a_zero_width_diagnostic_widens_to_the_next_character() {
            let found = diagnostic_underlines(&[error(4..4)], "let x = 1;", 0);
            assert_eq!(found, vec![(4..5, DiagnosticSeverity::Error)]);
        }

        #[test]
        fn a_zero_width_diagnostic_widens_by_a_whole_character_not_a_byte() {
            // Widening by one *byte* would cut the crab in half and the
            // text system would reject the run boundary.
            let line = "let \u{1f980} = 2;";
            let found = diagnostic_underlines(&[error(4..4)], line, 0);
            assert_eq!(found, vec![(4..8, DiagnosticSeverity::Error)]);
            assert_eq!(&line[4..8], "\u{1f980}");
        }

        #[test]
        fn a_zero_width_diagnostic_at_the_end_of_a_line_widens_backwards() {
            // There is no next character to take, so the mark goes on the
            // last one rather than vanishing.
            let found = diagnostic_underlines(&[error(10..10)], "let x = 1;", 0);
            assert_eq!(found, vec![(9..10, DiagnosticSeverity::Error)]);
        }

        #[test]
        fn a_zero_width_diagnostic_on_an_empty_line_underlines_nothing() {
            // Nothing to widen onto in either direction; an empty line has
            // no glyph to carry a squiggle.
            let found = diagnostic_underlines(&[error(0..0)], "", 0);
            assert!(found.is_empty());
        }
    }

    /// Tests for `underline_highlights` — the severity-to-style mapping and
    /// its interaction with the syntax colours it is painted over.
    mod underline_styles {
        use super::*;

        #[test]
        fn each_severity_takes_its_own_theme_colour() {
            let theme = Theme::dark();
            let styled = underline_highlights(
                &[
                    (0..1, DiagnosticSeverity::Error),
                    (1..2, DiagnosticSeverity::Warning),
                    (2..3, DiagnosticSeverity::Information),
                    (3..4, DiagnosticSeverity::Hint),
                ],
                &theme,
            );
            let colours: Vec<_> = styled
                .iter()
                .map(|(_, style)| style.underline.expect("every span underlines").color)
                .collect();
            assert_eq!(colours, vec![
                Some(theme.danger.into()),
                Some(theme.warning.into()),
                Some(theme.text_faint.into()),
                Some(theme.text_faint.into()),
            ]);
        }

        #[test]
        fn an_underline_is_wavy_so_it_does_not_read_as_a_markdown_link() {
            let styled = underline_highlights(&[(0..1, DiagnosticSeverity::Warning)], &Theme::dark());
            let underline = styled[0].1.underline.expect("a span underlines");
            assert!(underline.wavy, "other IDEs squiggle; a straight rule is the link style");
        }

        #[test]
        fn an_underline_sets_no_colour_of_its_own() {
            // The glyphs keep whatever the grammar painted them; only the
            // rule underneath is the diagnostic's.
            let styled = underline_highlights(&[(0..1, DiagnosticSeverity::Error)], &Theme::dark());
            assert!(styled[0].1.color.is_none(),
                "an error must not repaint the token it sits under");
        }

        /// The whole reason `combine_highlights` replaced the old
        /// `sort_by_key`: syntax spans and diagnostics do overlap, and
        /// `compute_runs` assumes ranges that do not.
        #[test]
        fn a_syntax_coloured_token_keeps_its_colour_under_a_diagnostic() {
            let keyword = gpui::rgb(0xff0000);
            let syntax = vec![(
                0..3,
                HighlightStyle { color: Some(keyword.into()), ..Default::default() },
            )];
            let diagnostics = underline_highlights(&[(0..3, DiagnosticSeverity::Warning)], &Theme::dark());
            let combined: Vec<_> = gpui::combine_highlights(syntax, diagnostics).collect();
            assert_eq!(combined.len(), 1, "one run covering the shared span");
            let (range, style) = &combined[0];
            assert_eq!(*range, 0..3);
            assert_eq!(style.color, Some(keyword.into()), "the grammar's colour survives");
            assert!(style.underline.is_some_and(|line| line.wavy),
                "and the squiggle is added on top of it");
        }
    }

    /// Tests for `line_highlights` — the composition `EditableLine` hands
    /// to `StyledText`, where the syntax colours and the diagnostic
    /// squiggles finally meet.
    mod line_style {
        use super::*;

        fn style_at(
            highlights: &[(Range<usize>, HighlightStyle)],
            offset: usize,
        ) -> HighlightStyle {
            highlights
                .iter()
                .find(|(range, _)| range.contains(&offset))
                .unwrap_or_else(|| panic!("no run covers byte {offset}"))
                .1
        }

        #[test]
        fn a_diagnostic_squiggles_its_span_without_disturbing_the_keyword() {
            let theme = Theme::dark();
            let palette = theme.syntax_palette();
            let line = "let x = 1;";
            let highlights = line_highlights(
                Language::Rust,
                line,
                &[],
                &[(4..5, DiagnosticSeverity::Warning)],
                &theme,
                &palette,
            );

            let marked = style_at(&highlights, 4);
            let underline = marked.underline.expect("the diagnostic span underlines");
            assert!(underline.wavy);
            assert_eq!(underline.color, Some(theme.warning.into()));

            let keyword = style_at(&highlights, 0);
            assert!(
                keyword.underline.is_none(),
                "`let` carries no diagnostic, so it carries no squiggle"
            );
            assert!(
                keyword.color.is_some(),
                "and it keeps the colour the grammar gave it"
            );
        }

        /// `compute_runs` — what `with_highlights` ultimately feeds — walks
        /// the ranges assuming each starts at or after the previous one's
        /// end. A diagnostic sitting on a coloured token breaks that
        /// assumption, which is why `combine_highlights` replaced the plain
        /// sort this function used to do.
        #[test]
        fn the_runs_come_out_disjoint_and_ascending() {
            let theme = Theme::dark();
            let palette = theme.syntax_palette();
            let highlights = line_highlights(
                Language::Rust,
                "let x = 1;",
                &[],
                &[(0..6, DiagnosticSeverity::Error)],
                &theme,
                &palette,
            );
            for pair in highlights.windows(2) {
                assert!(
                    pair[0].0.end <= pair[1].0.start,
                    "overlapping runs reach compute_runs as garbage offsets: \
                     {:?} then {:?}",
                    pair[0].0,
                    pair[1].0
                );
            }
        }

        #[test]
        fn a_markdown_link_still_underlines_straight_when_nothing_is_wrong() {
            let theme = Theme::dark();
            let palette = theme.syntax_palette();
            let line = "[setup](setup.md)";
            let links = markdown_links_in_line(line)
                .into_iter()
                .map(|span| (span.label_range, span.target))
                .collect::<Vec<_>>();
            let highlights =
                line_highlights(Language::Markdown, line, &links, &[], &theme, &palette);
            let label = style_at(&highlights, 1);
            let underline = label.underline.expect("a link underlines");
            assert!(!underline.wavy, "a link is a straight rule; only a diagnostic waves");
        }
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

    /// The platform's clipboard chord: ⌘ on macOS, Ctrl everywhere else.
    /// The source surface is not a terminal, so plain Ctrl-C is copy here
    /// rather than the interrupt `TerminalView` keeps `ctrl-shift-c` clear
    /// of.
    fn clipboard_chord(key: &str) -> String {
        if cfg!(target_os = "macos") {
            format!("cmd-{key}")
        } else {
            format!("ctrl-{key}")
        }
    }

    /// The editor's clipboard chords. Copy, Cut and Paste existed only on
    /// the right-click menu: `on_editor_key` returned on every command
    /// chord, and no binding anywhere claimed the `FileEditor` context, so
    /// the keyboard route into the clipboard did not exist at all.
    #[gpui::test]
    async fn the_clipboard_chords_copy_cut_and_paste_the_source_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "alpha beta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.editor_focus.focus(window, cx));
            window.simulate_next_frame(cx);
        });

        // Copy: the selection reaches the clipboard, the buffer is untouched.
        view.update(&mut cx.cx, |view, cx| {
            view.source_selection = Some(Selection { start: 0, end: 5 });
            view.caret = 5;
            cx.notify();
        });
        cx.simulate_keystrokes(&clipboard_chord("c"));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard())
                .and_then(|item| item.text())
                .as_deref(),
            Some("alpha"),
            "the copy chord must write the selection to the clipboard"
        );
        assert_eq!(
            buffer_of(&view, &cx),
            "alpha beta\n",
            "copying must leave the buffer alone"
        );

        // Cut: the same clipboard write, and the selection leaves the buffer.
        cx.simulate_keystrokes(&clipboard_chord("x"));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard())
                .and_then(|item| item.text())
                .as_deref(),
            Some("alpha"),
            "the cut chord must write the selection to the clipboard"
        );
        assert_eq!(
            buffer_of(&view, &cx),
            " beta\n",
            "the cut chord must remove the selection from the buffer"
        );

        // Paste: the clipboard lands at the caret the cut left behind.
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("gamma".to_string()));
        });
        cx.simulate_keystrokes(&clipboard_chord("v"));
        cx.run_until_parked();
        assert_eq!(
            buffer_of(&view, &cx),
            "gamma beta\n",
            "the paste chord must insert the clipboard at the caret"
        );
    }

    /// Select-all answered `ctrl-a` alone, so on macOS — the reference
    /// release platform — ⌘A did nothing, and the select-all-then-copy
    /// flow had no keyboard route into the clipboard however well the
    /// copy chord itself worked.
    #[gpui::test]
    async fn select_all_answers_both_the_control_and_the_platform_chord(
        cx: &mut gpui::TestAppContext,
    ) {
        const SOURCE: &str = "alpha beta\n";
        let file = TempFile::with_extension("rs", SOURCE);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.editor_focus.focus(window, cx));
            window.simulate_next_frame(cx);
        });
        let whole = Some(Selection {
            start: 0,
            end: SOURCE.len(),
        });

        cx.simulate_keystrokes("cmd-a");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.source_selection),
            whole,
            "the platform chord must select the whole buffer"
        );

        // And the chord that already worked still has to.
        view.update(&mut cx.cx, |view, cx| {
            view.source_selection = None;
            view.caret = 0;
            cx.notify();
        });
        cx.simulate_keystrokes("ctrl-a");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.source_selection),
            whole,
            "the control chord must keep selecting the whole buffer"
        );
    }

    /// The Markdown preview mounts under the same focusable element as the
    /// source surface, so the paste chord reaches this view while the
    /// reader is looking at rendered text and no caret. Rewriting the
    /// buffer there is an edit nothing on screen accounts for.
    #[gpui::test]
    async fn the_paste_chord_leaves_the_buffer_alone_under_the_markdown_preview(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("md", "# Title\n\nHello.\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown_mode(MarkdownMode::Preview, cx);
                view.editor_focus.focus(window, cx);
            });
            window.simulate_next_frame(cx);
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.effective_mode()),
            MarkdownMode::Preview,
            "the fixture must actually be showing the preview"
        );

        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("intruder".to_string()));
        });
        cx.simulate_keystrokes(&clipboard_chord("v"));
        cx.run_until_parked();

        assert_eq!(
            buffer_of(&view, &cx),
            "# Title\n\nHello.\n",
            "a paste under the preview must not reach the source buffer"
        );
        assert!(
            !view.read_with(&cx.cx, |view, _| view.is_dirty()),
            "and must not leave the file dirty"
        );
    }

    /// The source buffer as the view currently holds it.
    fn buffer_of(view: &gpui::Entity<FileView>, cx: &gpui::VisualTestContext) -> String {
        view.read_with(&cx.cx, |view, _| {
            view.editor().expect("editor").buffer().to_owned()
        })
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

    /// The editor has no header bar. It used to carry the file's absolute
    /// path, a `● edited` mark and a language chip above every document —
    /// a whole row spent on three things the tab already answers
    /// (`SirioWorkspace::tab_is_dirty` draws the dirty mark, the tab label
    /// names the file). The content now starts at the top of the tab, and
    /// any bar reappearing above it pushes this origin down and fails here.
    #[gpui::test]
    async fn a_code_file_draws_no_bar_above_its_content(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        let content = cx
            .debug_bounds("file-text-scroll")
            .expect("a code file renders its source");
        assert!(
            content.origin.y < px(2.0),
            "the source surface must start at the top of the tab, got y={:?}",
            content.origin.y
        );
    }

    /// F-EDIT-01/02: one Markdown row, and the Code/Preview icons end it.
    ///
    /// Two things are load-bearing here and were not before. The row is
    /// drawn in **Preview** as well as Code — with the header gone, the
    /// icons on it are the only way out of Preview, so a Code-only row
    /// would make Preview a dead end. And the icons sit at the row's right
    /// edge rather than beside the formatting controls, so their position
    /// does not move when those controls appear and disappear with the mode.
    #[gpui::test]
    async fn the_markdown_mode_icons_end_the_row_in_both_modes(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("md", "# Title\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // Preview, the default: the row carries the icons and nothing else.
        let row = cx
            .debug_bounds("file-markdown-row")
            .expect("the Markdown row is drawn in Preview too");
        let switch = cx
            .debug_bounds("file-mode-switch")
            .expect("the mode icons are drawn in Preview");
        assert!(
            cx.debug_bounds("file-format-bold").is_none(),
            "Preview is a reading surface: it offers nothing to format"
        );
        assert!(
            row.origin.y <= switch.origin.y
                && switch.origin.y + switch.size.height <= row.origin.y + row.size.height,
            "the icons sit on the row, not above or below it: row {row:?}, icons {switch:?}"
        );
        let row_right = row.origin.x + row.size.width;
        let switch_right = switch.origin.x + switch.size.width;
        assert!(
            row_right - switch_right < px(24.0),
            "the icons end the row: it ends at {row_right:?}, they end at {switch_right:?}"
        );

        // Code: the formatting controls appear to their left, and they stay
        // where they were.
        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code icon is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let link = cx
            .debug_bounds("file-format-link")
            .expect("Code mode offers the formatting controls");
        let switch_in_code = cx
            .debug_bounds("file-mode-switch")
            .expect("the mode icons stay on the row in Code");
        assert!(
            switch_in_code.origin.x > link.origin.x + link.size.width,
            "the icons stay after the last formatting control"
        );
        assert_eq!(
            switch_in_code.origin.x, switch.origin.x,
            "and they do not move when the formatting controls appear"
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

    #[gpui::test]
    async fn file_horizontal_bar_tracks_wide_source_without_changing_vertical_scroll(
        cx: &mut gpui::TestAppContext,
    ) {
        let wide = TempFile::with_extension(
            "rs",
            &format!("{}\n{}", "w".repeat(400), "x\n".repeat(400)),
        );
        let (mut cx, view) = mounted_file_view(cx, wide.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("file-horizontal-bar-track").is_some());
        assert!(cx.debug_bounds("file-horizontal-bar-thumb").is_some());
        assert!(cx.debug_bounds("file-text-bar").is_some());

        let before_y = view.read_with(&cx.cx, |v, _| {
            v.scroll.source.0.borrow().base_handle.offset().y
        });
        let track = cx.debug_bounds("file-horizontal-bar-track").unwrap();
        let thumb = cx.debug_bounds("file-horizontal-bar-thumb").unwrap();
        let end = gpui::point(track.right() - px(2.), thumb.center().y);
        cx.simulate_mouse_move(thumb.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(thumb.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(thumb.center().x + px(8.), thumb.center().y),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        view.read_with(&cx.cx, |v, _| {
            let position = v.scroll.source.0.borrow().base_handle.offset();
            assert!(position.x < px(0.));
            assert_eq!(position.y, before_y);
        });

        let short = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _) = mounted_file_view(&mut cx.cx, short.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("file-horizontal-bar-track").is_none());

        let markdown = TempFile::with_extension("md", &format!("{}\n", "w".repeat(400)));
        let (mut cx, _) = mounted_file_view(&mut cx.cx, markdown.path().to_path_buf());
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("file-horizontal-bar-track").is_none());
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

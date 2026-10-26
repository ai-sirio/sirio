//! File-backed centre-column tabs — an editor, not a viewer (P34).
//!
//! The editor's I/O substance — load, save, conflict detection, dirty state,
//! and language detection — lives in [`crate::editor`] as pure synchronous
//! code, tested without a display. This view owns the pixels and the
//! shell-facing surface: the source, drawn and edited by Ely's `CodeEditor`
//! (hosted in `code_surface`, kept in step with the [`Editor`] model), plus
//! the rendered Markdown preview.
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
    AnyElement, App, Context, Entity, FocusHandle, HighlightStyle, KeyBinding, MouseButton,
    MouseDownEvent, Pixels, Point, Render, ScrollHandle, SharedString, Subscription, Task,
    UnderlineStyle, Window, actions, anchored, canvas, deferred, div, point, prelude::*, px,
};
use sirio_diagram::{DiagramError, DiagramKind, Palette};
use sirio_git::FileDiff;
use sirio_markdown::{Document, FileSystemEvent, FileSystemEventMonitor, parse};
use sirio_project::resolve_file_link;
use sirio_theme::Theme;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ely_gpui_component::editor::{CodeEditor, ReadOnlyBanner, SingleCursor};
use ely_gpui_component::forms::TextInput;
// `sirio` has no Ely dependency; `open_find` takes these, and the socket verbs build them.
pub use ely_gpui_component::editor::{FindOptions, LspState};
pub use status_actions::StatusItem;
pub use status_row::StatusFacts;
use crate::chat::{Chat, LinkClickOverride};
use crate::drawer::WorktreeDrawer;
use crate::editor::{
    Conflict, Editor, Language, LoadStatus, Selection,
};
use crate::file_context_menu::{self, FileContextAction, FileContextFacts, ItemState};
use crate::horizontal_scroll::{self, HorizontalBarState};
use crate::loading;
use crate::markdown_preview::{
    self, DiagramRequest, DiagramSettings, DiagramState, Diagrams, PlantUmlQueue,
};
use crate::outline::OutlineSymbol;
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use crate::text_selection::selectable_text;

mod breadcrumb;
mod code_surface;
mod find;
mod git_marks;
mod status_actions;
mod status_row;
mod sync;

use sync::minimal_edit;

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

/// Where a snapshot came from: which file, at which revision, of which
/// change request (spec §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotOrigin {
    pub reference: sirio_forge::ChangeRef,
    /// The revision shown: the head, or the base for a file the change
    /// request deletes.
    pub sha: String,
    /// Repository-relative.
    pub relative_path: PathBuf,
    /// The worktree's own copy, when it has one to offer.
    pub local_copy: Option<PathBuf>,
    /// The change request deletes this file: the snapshot shows it at the
    /// base, says so in its title, and has no local copy to offer.
    pub deleted: bool,
}

impl SnapshotOrigin {
    /// `#578 at a1b2c3d` — or `#578` alone for a refused origin whose sha
    /// was invalid and left empty.
    pub fn label(&self) -> String {
        if self.sha.is_empty() {
            return self.reference.label();
        }
        format!("{} at {}", self.reference.label(), crate::forge_source::short_sha(&self.sha))
    }

    /// What the banner over the snapshot says.
    pub fn banner_message(&self) -> String {
        format!("Read-only · {}", self.label())
    }

    /// The banner's one action, offered only when the worktree has a copy of
    /// the file to open.
    pub fn banner_action(&self) -> Option<&'static str> {
        self.local_copy.is_some().then_some("Open local file")
    }

    /// The view's `path()`. Never a real path — the workspace matches open
    /// documents by path, and a snapshot must not be mistaken for (or steal
    /// the tab of) the file it was taken from; the extension still picks the
    /// language. Built from plain name components only, so a relative path
    /// that is absolute or climbs (`..`) — one from a corrupt session row —
    /// cannot replace the prefix or leave it.
    pub fn virtual_path(&self) -> PathBuf {
        let mut path = Path::new("sirio-snapshot").join(&self.sha);
        path.extend(self.relative_path.components().filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name),
            _ => None,
        }));
        path
    }
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
    /// `Some` for a file shown at a revision rather than read from `path` —
    /// read-only, no monitor, no language server.
    snapshot: Option<SnapshotOrigin>,
    /// Why a snapshot's read failed, while it stays failed. Kept in the
    /// view rather than in a card: a card is dismissed by a click or
    /// replaced by the next message, and this sentence carries the tab's
    /// only remedy, its Retry (spec §8).
    snapshot_error: Option<String>,
    /// A message that answers something the user just did: a definition
    /// that does not exist, a save that failed, a language server that
    /// would not start. It is *about* the file, never instead of it —
    /// see `TransientMessage` for why that distinction is load-bearing.
    message: Option<TransientMessage>,
    /// What this tab has been told, oldest first, kept behind the status row's
    /// bell: the text of every `set_message` and `offer`, the newest
    /// `MAX_NOTIFICATIONS` rows of them. A message identical to the newest row
    /// is not a new row: it counts into that row (`Notification::times`).
    /// Dismissing the corner card leaves the list alone, and so does opening
    /// it; only Clear empties it. Not kept across a relaunch, and a snapshot
    /// tab has its own.
    notifications: Vec<Notification>,
    /// How many messages arrived since the list was last opened: the number on
    /// the bell. A repeat counts here, though it adds no row.
    notifications_unseen: usize,
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
    /// The file's code, drawn and edited by Ely's `CodeEditor`. Created in
    /// `render` once the file is loaded, because building one needs a window;
    /// snapshot reloads and disk adoptions reuse it through `reset_keeping_view`.
    code_editor: Option<Entity<CodeEditor>>,
    /// The code editor's text changes (kept in step with the `Editor`) and
    /// its focus (which re-checks the disk, as `focus_subscription` does for
    /// `editor_focus`).
    code_subscriptions: Vec<Subscription>,
    /// The colours of the code's lines, shared with the code editor's
    /// highlighter. Rotated once per render of this view (see `render`).
    span_cache: Rc<RefCell<code_surface::SpanCache>>,
    /// The language and span count of the first line as the highlighter last
    /// answered (`highlight_text`).
    highlight_drawn: Rc<Cell<Option<(Language, usize)>>>,
    /// Focus target for everything that is not the code: Loading, the
    /// notices and the Markdown Preview. The code editor takes the keyboard
    /// while it draws the file (see `focus_target`).
    editor_focus: FocusHandle,
    /// Re-check the disk snapshot whenever this tab's editor focus is
    /// regained after another surface owned it.
    focus_subscription: Option<Subscription>,
    /// Scroll state of the Markdown preview, so it can wear bezel's bars.
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
    /// The file's symbols from its language server, as the app last pushed
    /// them (`set_symbols`): what the breadcrumb names the caret's place by.
    symbols: Vec<OutlineSymbol>,
    /// The path the breadcrumb starts with, relative to the file's worktree
    /// (`set_location`); `None` shows the file's own name.
    location: Option<String>,
    /// The line the caret was on when the code editor last changed, which the
    /// breadcrumb names the symbols around. The code editor tells this view
    /// nothing about a caret move, so an observer of it keeps this current.
    caret_line: usize,
    /// The file's diff against HEAD, as the app last pushed it (`set_git_diff`):
    /// the gutter's marks and the hunk drawn in place. `None` for a file that is
    /// not in a repository, is clean, or is a snapshot.
    git_diff: Option<FileDiff>,
    /// The gutter line that opens the hunks drawn in place, `None` when none is
    /// drawn. It is their opening line (`git_marks::opening_line`), a line that
    /// carries one of their marks, so `diff_open` names a line the gutter does.
    open_hunk: Option<usize>,
    /// Whether a diff fetch is running (`begin_git_diff` until `land_git_diff`).
    /// Only one runs at a time: each is a whole-worktree `git status` and `diff`.
    git_diff_in_flight: bool,
    /// Whether the diff went stale while that fetch ran, so one more is due when it lands.
    git_diff_again: bool,
    /// How many times the app asked to fetch the diff (`begin_git_diff` calls, the
    /// coalesced ones too): what the editor E2E counts per click and per focus.
    git_diff_requests: u64,
    /// What the row under the file shows that only the app knows
    /// (`set_status_facts`): the branch, how far it is from its upstream, the
    /// language server.
    status_facts: StatusFacts,
    /// What the row under the file has measured of itself, which decides
    /// whether it draws every item or only the ones that matter at a glance.
    status_layout: Rc<status_row::StatusLayout>,
    /// The worktree's drawer under the code, shared by every file tab of the
    /// worktree (`set_drawer`); `None` until the workspace hands it over.
    drawer: Option<Entity<WorktreeDrawer>>,
    /// Redraws this view when the drawer changes: the corner card's offset
    /// reads the drawer's drawn height.
    _drawer_subscription: Option<Subscription>,
    /// The language this tab colours its code as, when the user chose one in
    /// the status row (`set_language_override`); `None` is the path's. Colour
    /// only, and not kept across a relaunch.
    language_override: Option<Language>,
    /// How this tab indents, `(spaces, width)`, when the user chose in the
    /// status row (`set_indent`); `None` follows the Editor setting. Not kept
    /// across a relaunch.
    indent_override: Option<(bool, usize)>,
    /// The status row's open popover; at most one.
    status_popover: Option<StatusItem>,
    /// The go-to-line field, made the first time its popover opens.
    goto_input: Option<Entity<TextInput>>,
    /// Whether the go-to-line field takes the keyboard when it is drawn.
    goto_focus_pending: bool,
    /// Whether the code takes the keyboard when the open popover closes: set by
    /// Escape and by a finished choice, cleared by the close.
    focus_code_on_close: bool,
    /// Whether the open list takes the keyboard when it is drawn, so its own
    /// keys (Up, Down, Enter, Escape) reach it without a Tab first.
    list_focus_pending: bool,
    /// A popover the verb asked for while another was open: the open one
    /// closes in the next draw and this one opens after it, so the old one's
    /// hand-back of the keyboard cannot close the new one.
    pending_status_open: Option<StatusItem>,
    /// The open find: its fields, its matches and the one that is current.
    /// `None` while find is closed.
    find: Option<find::FindState>,
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
    pending_reveal: Option<RevealTarget>,
    /// A reveal that wants the source asked for while the file was still
    /// loading: the load picks Preview for a Markdown file after the ask, so
    /// the mode set at the ask would be overwritten. Taken by the load.
    code_on_load: bool,
    /// The Preview's diagrams, by cache key (design §3).
    diagrams: Diagrams,
    /// The diagram settings the last frame saw. A change forgets every
    /// failure, so a newly installed `plantuml` or a new server is tried.
    diagram_settings: Option<DiagramSettings>,
    /// Bumped whenever the settings change. A render scheduled under older
    /// settings lands stale and is discarded, never stored.
    diagram_generation: u64,
}

/// The scroll handle of the Markdown preview, plus the bar state each bezel
/// scrollbar carries its drag in. Tracked because an untracked surface
/// scrolls just as well but reports no viewport and no overflow — a bar with
/// nothing to draw from. Held by the view, never rebuilt per frame: the
/// handle *is* the scroll position across frames. The code editor scrolls
/// itself.
struct SurfaceScroll {
    preview: ScrollHandle,
    preview_bar: ScrollbarState,
    preview_horizontal_bar: HorizontalBarState,
}

impl SurfaceScroll {
    fn new(painter: Painter) -> Self {
        Self {
            preview: ScrollHandle::new(),
            preview_bar: ScrollbarState::new(painter),
            preview_horizontal_bar: HorizontalBarState::default(),
        }
    }
}

/// What the Preview draws this frame: the document, and the directory its
/// diagrams live in. A click on one of those is not a link.
struct PreviewFrame {
    doc: bezel_markdown::Doc,
    diagram_dir: Option<PathBuf>,
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
    /// The file on disk changed, or was saved or reloaded, so its diff against
    /// HEAD (the git marks) is out of date. Emitted once per such change: after
    /// a save or a reload that succeeded, and after a check that adopted a
    /// change from disk or found one.
    GitDiffStale(PathBuf),
    /// A save succeeded: the file's symbols may have moved. Apart from
    /// `GitDiffStale`, which also fires when the editor takes focus.
    Saved(PathBuf),
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
    /// A snapshot's read failed and the Retry of its failure surface was
    /// clicked: read the revision again.
    RetrySnapshot,
    /// The snapshot banner's "Open local file": open the worktree's own,
    /// editable file.
    OpenLocalCopy(PathBuf),
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

/// Where a reveal puts the caret. Held by the view until the file has loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevealTarget {
    /// The start of a line (0-based).
    Line(usize),
    /// A line (0-based) and a column counted in UTF-16 code units, as the
    /// protocol counts: the caret goes there, past the end of a short line to
    /// its end.
    LineColumnUtf16(usize, usize),
    /// A range of bytes of the buffer, selected.
    Range(Range<usize>),
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

// `Find` opens find on the file's code, or brings the open one back; `Replace`
// opens it with the replace row. `ToggleDrawer` opens the worktree's drawer or
// closes it to its header; `SearchInWorktree` opens it on Search.
actions!(file_view, [Find, Replace, ToggleDrawer, SearchInWorktree]);

/// Installs the file tab's key bindings: Find and Replace, in the tab's
/// `FileEditor` context so they reach neither a terminal nor a chat.
pub fn init(cx: &mut App) {
    // `ctrl-h` on a platform with no `cmd-alt-f`: it is Backspace only inside
    // Ely's macOS text fields (their `ElyInput` bindings), which are not in this context.
    let replace = if cfg!(target_os = "macos") {
        "cmd-alt-f"
    } else {
        "ctrl-h"
    };
    cx.bind_keys([
        KeyBinding::new("secondary-f", Find, Some("FileEditor")),
        KeyBinding::new(replace, Replace, Some("FileEditor")),
        KeyBinding::new("secondary-j", ToggleDrawer, Some("FileEditor")),
        KeyBinding::new("secondary-shift-f", SearchInWorktree, Some("FileEditor")),
    ]);
}

/// How many notification rows a tab keeps; the oldest are dropped past it.
const MAX_NOTIFICATIONS: usize = 50;

/// One row of the bell's list: a message, and how many times it arrived in a
/// row (a failing save pressed three times is one row marked "(×3)").
struct Notification {
    text: SharedString,
    times: usize,
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
                view.markdown_mode = if std::mem::take(&mut view.code_on_load) {
                    MarkdownMode::Code
                } else {
                    Self::initial_markdown_mode(&editor)
                };
                view.state = ViewState::Ready(editor);
                view.load_task = None;
                view.apply_pending_reveal(cx);
                cx.emit(FileViewEvent::Loaded(view.path.clone()));
                cx.notify();
            });
        });
        // The debug switch the E2E uses to stand where macOS stands: no file
        // watcher, so a tab learns of a write only when something tells it to
        // look (compiled out of release builds).
        let watched =
            !(cfg!(debug_assertions) && std::env::var_os("SIRIO_NO_FILE_WATCH").is_some());
        let file_monitor = if watched { FileSystemEventMonitor::new(&path).ok() } else { None };
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
        let mut view = Self::blank(path, ViewState::Loading, cx);
        view.load_task = Some(load_task);
        view.file_monitor = file_monitor;
        view._file_monitor_task = file_monitor_task;
        view
    }

    /// Every field at rest: no load under way, no monitor, nothing the user
    /// did yet. `new` adds the disk read and the monitor; a snapshot adds
    /// neither.
    fn blank(path: PathBuf, state: ViewState, cx: &mut Context<Self>) -> Self {
        Self {
            path,
            state,
            load_task: None,
            file_monitor: None,
            _file_monitor_task: None,
            snapshot: None,
            snapshot_error: None,
            message: None,
            notifications: Vec::new(),
            notifications_unseen: 0,
            markdown_mode: MarkdownMode::Preview,
            code_editor: None,
            code_subscriptions: Vec::new(),
            span_cache: Rc::default(),
            highlight_drawn: Rc::default(),
            editor_focus: cx.focus_handle().tab_stop(true),
            focus_subscription: None,
            scroll: SurfaceScroll::new(Painter::of(cx)),
            context_menu: None,
            definition_gesture: None,
            shell_facts: FileContextFacts::default(),
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            location: None,
            caret_line: 0,
            git_diff: None,
            open_hunk: None,
            git_diff_in_flight: false,
            git_diff_again: false,
            git_diff_requests: 0,
            status_facts: StatusFacts::default(),
            status_layout: Rc::default(),
            drawer: None,
            _drawer_subscription: None,
            language_override: None,
            indent_override: None,
            status_popover: None,
            goto_input: None,
            goto_focus_pending: false,
            focus_code_on_close: false,
            list_focus_pending: false,
            pending_status_open: None,
            find: None,
            hover_offset: None,
            hover_point: point(px(0.), px(0.)),
            hover_task: None,
            hover_seq: 0,
            hover_card: None,
            pending_reveal: None,
            code_on_load: false,
            diagrams: Diagrams::default(),
            diagram_settings: None,
            diagram_generation: 0,
        }
    }

    /// A snapshot whose bytes are still being read (a restored tab).
    pub fn snapshot_pending(origin: SnapshotOrigin, cx: &mut Context<Self>) -> Self {
        let mut view = Self::blank(origin.virtual_path(), ViewState::Loading, cx);
        view.snapshot = Some(origin);
        view
    }

    /// A snapshot with its bytes at hand.
    pub fn snapshot(origin: SnapshotOrigin, bytes: Vec<u8>, cx: &mut Context<Self>) -> Self {
        let mut view = Self::snapshot_pending(origin, cx);
        let editor = Editor::from_snapshot(view.path.clone(), bytes);
        view.markdown_mode = Self::initial_markdown_mode(&editor);
        view.state = ViewState::Ready(editor);
        view
    }

    pub fn is_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    /// Gives the view the drawer of its worktree, which it draws under the code.
    pub fn set_drawer(&mut self, drawer: Entity<WorktreeDrawer>, cx: &mut Context<Self>) {
        self._drawer_subscription = Some(cx.observe(&drawer, |_, _, cx| cx.notify()));
        self.drawer = Some(drawer);
        cx.notify();
    }

    pub fn drawer(&self) -> Option<&Entity<WorktreeDrawer>> {
        self.drawer.as_ref()
    }

    pub fn snapshot_origin(&self) -> Option<&SnapshotOrigin> {
        self.snapshot.as_ref()
    }

    /// The read finished: fill the view in, or say why it could not, with a
    /// Retry the workspace answers (`FileViewEvent::RetrySnapshot`). The
    /// failure fills the surface and stays until a retry or a later read
    /// replaces it; no card can take it away.
    pub fn finish_snapshot(&mut self, result: Result<Vec<u8>, String>, cx: &mut Context<Self>) {
        let Some(origin) = self.snapshot.clone() else {
            return;
        };
        match result {
            Ok(bytes) => {
                let editor = Editor::from_snapshot(self.path.clone(), bytes);
                self.markdown_mode = Self::initial_markdown_mode(&editor);
                self.state = ViewState::Ready(editor);
                self.snapshot_error = None;
                self.message = None;
                // A retry shows new text in the same code editor.
                self.sync_code_from_editor(cx);
                // A line asked for while the read was pending (or failed
                // and retried) has its lines now.
                self.apply_pending_reveal(cx);
                cx.emit(FileViewEvent::Loaded(self.path.clone()));
            }
            Err(reason) => {
                // A snapshot whose path was refused (a saved one, or a
                // clicked one) carries no path: there is no file name to
                // put in the sentence.
                self.snapshot_error = Some(if origin.relative_path.as_os_str().is_empty() {
                    format!("Could not open the snapshot of {}: {reason}", origin.label())
                } else {
                    format!(
                        "Could not read {} at {}: {reason}",
                        origin.relative_path.display(),
                        origin.label()
                    )
                });
            }
        }
        cx.notify();
    }

    /// Back to nothing-yet, for a retry.
    pub fn restart_snapshot(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.is_some() {
            self.state = ViewState::Loading;
            self.snapshot_error = None;
            self.message = None;
            cx.notify();
        }
    }

    /// Why the snapshot's read failed, while the tab shows that failure.
    pub fn snapshot_error(&self) -> Option<&str> {
        self.snapshot_error.as_deref()
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

    /// The text of this file as the tab holds it now, LF only, for a search
    /// that must see what is on screen rather than what is on disk. `None`
    /// while the file is loading or failed to load.
    pub fn buffer_text(&self, cx: &App) -> Option<String> {
        if let Some(code) = &self.code_editor {
            if self.code_shown() {
                return Some(code.read(cx).text().to_owned());
            }
        }
        let editor = self.editor()?;
        (editor.status() == &LoadStatus::Loaded).then(|| editor.buffer().to_owned())
    }

    /// The primary selection, when it is not empty and lies on one line: what
    /// `Ctrl+Shift+F` seeds the worktree search with.
    pub fn selection_query(&self, cx: &App) -> Option<String> {
        let code = self.code_editor.as_ref().filter(|_| self.code_shown())?;
        let code = code.read(cx);
        let range = code.primary().range();
        let text = code.text().get(range)?;
        (!text.is_empty() && !text.contains('\n') && !text.contains('\r')).then(|| text.to_owned())
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
        let text = text.into();
        self.note(&text);
        self.message = Some(TransientMessage {
            text,
            at: None,
            actions: Vec::new(),
        });
        cx.notify();
    }

    /// Adds `text` to the tab's notifications (the status row's bell). Only
    /// what nothing pointed at is news: `set_message` and `offer` call it, and
    /// `answer_definition` does not, because it answers a gesture where it was
    /// made and a list of past answers would only hold the user's own
    /// questions. A message identical to the newest row adds no row: it is
    /// counted into it, so a key held down on a failing save stays one line.
    /// Past `MAX_NOTIFICATIONS` rows the oldest is dropped. A message that
    /// arrives while the list is open is seen already; otherwise it counts on
    /// the bell, repeat or not, up to what the list still holds.
    fn note(&mut self, text: &str) {
        match self.notifications.last_mut() {
            Some(newest) if newest.text.as_ref() == text => newest.times += 1,
            _ => {
                self.notifications.push(Notification {
                    text: SharedString::from(text.to_owned()),
                    times: 1,
                });
                if self.notifications.len() > MAX_NOTIFICATIONS {
                    self.notifications.remove(0);
                }
            }
        }
        if self.status_popover != Some(StatusItem::Bell) {
            let held: usize = self.notifications.iter().map(|row| row.times).sum();
            self.notifications_unseen = (self.notifications_unseen + 1).min(held);
        }
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
        let text = text.into();
        self.note(&text);
        self.message = Some(TransientMessage {
            text,
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
        self.push_diagnostics(cx);
        cx.notify();
    }

    pub fn diagnostics(&self) -> &[FileDiagnostic] {
        &self.diagnostics
    }

    /// Called from every pointer move over the code, which is to say very
    /// often, by the code editor's `Pointer` event (`code_surface`). **An
    /// unchanged offset returns immediately**, without
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
        let local = self.diagnostics_at(offset, cx);
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
        let local = self
            .hover_offset
            .map(|offset| self.diagnostics_at(offset, cx))
            .unwrap_or_default();
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

    /// Puts the caret (or a selection) where `target` names and scrolls it into
    /// view, now or as soon as the file has loaded.
    pub fn reveal(&mut self, target: RevealTarget, cx: &mut Context<Self>) {
        self.pending_reveal = Some(target);
        self.apply_pending_reveal(cx);
    }

    /// Like [`Self::reveal`], for a caller that pointed at a line: a Markdown
    /// file is shown as source, because the line only exists there. A file
    /// still loading is shown as source once it has loaded (`code_on_load`);
    /// the reveal itself waits for the code editor, which a Markdown file
    /// leaving Preview only gets on its next draw (`ensure_code_editor`
    /// applies the held target then). A file that is not Markdown is not
    /// changed by this.
    pub fn reveal_in_code(&mut self, target: RevealTarget, cx: &mut Context<Self>) {
        self.code_on_load = matches!(self.state, ViewState::Loading);
        self.set_markdown_mode(MarkdownMode::Code, cx);
        self.reveal(target, cx);
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

    /// Every message covering a byte offset, worst first. Once the code
    /// editor exists it holds the findings: Ely moves them with each edit and
    /// clears them on an undo or a reload, so the card agrees with the
    /// squiggles. Before it exists, the view's own copy is all there is.
    fn diagnostics_at(&self, offset: usize, cx: &App) -> Vec<String> {
        // Both ends count. A zero-width finding at a line's end is drawn over
        // the character before it (`marked_range` widens it to `x-1..x`), and
        // Ely answers a pointer past that character with the boundary `x`, so
        // the card must still show the finding at its end.
        let covers = |range: &Range<usize>| range.start <= offset && offset <= range.end;
        let Some(code) = &self.code_editor else {
            let mut found: Vec<_> = self
                .diagnostics
                .iter()
                .filter(|d| covers(&d.range))
                .collect();
            found.sort_by_key(|d| d.severity);
            return found.iter().map(|d| d.message.clone()).collect();
        };
        let mut found: Vec<_> = code
            .read(cx)
            .diagnostics()
            .iter()
            .filter(|d| covers(&d.range))
            .collect();
        // Worst first: the reverse of the order `ely_diagnostics` draws them in.
        found.sort_by_key(|d| std::cmp::Reverse(code_surface::severity_rank(d.severity)));
        found.iter().map(|d| d.message.to_string()).collect()
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
    pub fn context_menu_items(&self, cx: &App) -> Vec<file_context_menu::FileContextItem> {
        file_context_menu::items(&self.menu_facts(cx))
    }

    /// Whether this file is Markdown — the one question the Code/Preview
    /// pair and the context menu's Preview entry both hang off. A code file
    /// has nothing to preview, so neither is offered for one.
    fn is_markdown(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.language() == Language::Markdown)
    }

    /// Whether the file has been read: a view still loading, or one that
    /// failed, is not.
    pub fn is_loaded(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
    }

    /// Whether the Markdown row draws its formatting controls: a loaded
    /// Markdown file in Code, and not a snapshot. The render and
    /// `surface.editor.state` (`format_buttons`) both ask this, so the state
    /// reports what is drawn.
    pub fn formatting_controls_shown(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
            && self.is_markdown()
            && formatting_controls_drawn(self.effective_mode(), !self.is_snapshot())
    }

    /// The shell's facts plus the three this view answers itself.
    fn menu_facts(&self, cx: &App) -> FileContextFacts {
        let has_selection = self
            .code_selection(cx)
            .is_some_and(|selection| !selection.is_collapsed());
        FileContextFacts {
            has_selection,
            is_markdown: self.is_markdown(),
            is_snapshot: self.is_snapshot(),
            ..self.shell_facts.clone()
        }
    }

    /// Opens the menu at `at`, a window point. The code editor calls this for a
    /// right press on the code (`SecondaryClick`); the view for one anywhere
    /// else, in Preview, while loading and on the banners.
    fn open_context_menu(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_hover(cx);
        self.focus_target(cx).focus(window, cx);
        self.context_menu = Some(at);
        cx.notify();
    }

    fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// What a click on the menu's row for `action` does now. Both the row's
    /// click and the debug verb `surface.editor.context` take this path, so the
    /// verb refuses exactly where a click does. A row that is not offered has
    /// no click (`not offered`). An unavailable row refuses with its reason. A
    /// redirected row does what it redirects to, not the action it is labelled
    /// with: a missing language server is installed, not searched for.
    fn context_effect(
        &self,
        action: FileContextAction,
        cx: &App,
    ) -> Result<FileContextAction, String> {
        let items = self.context_menu_items(cx);
        let item = items
            .iter()
            .find(|item| item.action == action)
            .ok_or_else(|| "not offered".to_string())?;
        match &item.state {
            ItemState::Ready => Ok(item.action),
            ItemState::Unavailable(reason) => Err(reason.clone()),
            ItemState::Redirected { to, .. } => Ok(*to),
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
            // A snapshot's menu disables Cut and Paste (`file_context_menu::items`),
            // so neither reaches here for one: `context_effect` refuses them first.
            FileContextAction::Cut => {
                self.copy_selection(false, cx);
                self.replace_code_selection("", cx);
                cx.notify();
            }
            FileContextAction::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_code_selection(&text, cx);
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
                if let Some(lines) = self.selection_lines(cx) {
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
                let offset = self.code_caret(cx);
                self.request_definition(offset, gesture, cx);
            }
            FileContextAction::FindReferences => {
                cx.emit(FileViewEvent::FindReferences {
                    path: self.path.clone(),
                    offset: self.code_caret(cx),
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
    fn selected_text(&self, cx: &App) -> Option<String> {
        let code = self.code_editor.as_ref()?.read(cx);
        let range = code.primary().range();
        (!range.is_empty()).then(|| code.text()[range].to_owned())
    }

    fn copy_selection(&mut self, trim: bool, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text(cx) else {
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
    fn selection_lines(&self, cx: &App) -> Option<(usize, usize)> {
        let code = self.code_editor.as_ref()?.read(cx);
        Some(crate::editor::line_range_for(
            code.text(),
            self.code_selection(cx)?,
        ))
    }

    /// The payload "Add to Agent Thread" carries: where the code is, the code
    /// itself already trimmed flush left, and the fence tag to wrap it in.
    fn selection_event(&self, cx: &mut Context<Self>) -> Option<FileViewEvent> {
        let text = file_context_menu::trim_common_indent(&self.selected_text(cx)?);
        Some(FileViewEvent::SendSelectionToAgent {
            path: self.path.clone(),
            lines: self.selection_lines(cx)?,
            text,
            language: self.editor()?.language().fence_tag(),
        })
    }

    /// Re-reads the file and compares it against the last known disk state
    /// (F-EDIT-05/06). The shell calls this when a file tab is activated —
    /// "modify externally, return to Sirio" — and on window focus.
    ///
    /// Returns whether it asked for a fresh diff (`GitDiffStale`).
    pub fn check_external(&mut self, cx: &mut Context<Self>) -> bool {
        if self.snapshot.is_some() {
            return false;
        }
        let before = self.buffer_and_conflict();
        if let Some(editor) = self.editor_mut() {
            editor.check_external();
            cx.notify();
        }
        // A silent adoption of the disk's text lands in the code editor too.
        self.sync_code_from_editor(cx);
        // The git marks compare the file on disk with HEAD, so they go stale when
        // the buffer adopted a change from disk, when a conflict began or ended,
        // and while one stands, since the disk may have moved again under it. A
        // check that finds nothing new is silent: the monitor also reports Sirio's
        // own save, and that save has already asked for the diff.
        let after = self.buffer_and_conflict();
        let conflicted = after
            .as_ref()
            .is_some_and(|(_, conflict)| *conflict != Conflict::None);
        let stale = before != after || conflicted;
        if stale {
            cx.emit(FileViewEvent::GitDiffStale(self.path.clone()));
        }
        stale
    }

    /// The file or the code editor took focus: look for a change on disk, and
    /// ask for a fresh diff either way. HEAD can move with the file untouched (an
    /// agent's `git commit` in a terminal), which no disk check sees, and focus
    /// is user-paced; a second focus while the fetch runs is coalesced.
    fn gained_focus(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.is_some() {
            return;
        }
        if !self.check_external(cx) {
            cx.emit(FileViewEvent::GitDiffStale(self.path.clone()));
        }
    }

    /// The buffer's text and its conflict: what a disk check can change.
    fn buffer_and_conflict(&self) -> Option<(String, Conflict)> {
        self.editor()
            .map(|editor| (editor.buffer().to_owned(), editor.conflict()))
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

    /// Saves the buffer (F-EDIT-04/06), recreating a deleted file. A failure
    /// is returned and raised here, once, as a message of this tab (the corner
    /// card and the bell), so no caller raises it again: the Ctrl+S handler
    /// only logs it.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self.snapshot.is_some() {
            let reason = "Read-only: open the local copy to edit".to_string();
            self.set_message(reason.clone(), cx);
            return Err(reason);
        }
        let result = match self.editor_mut() {
            Some(editor) => editor.save(),
            None => Err("the file is still loading".to_string()),
        };
        match &result {
            Ok(()) => {
                cx.emit(FileViewEvent::GitDiffStale(self.path.clone()));
                cx.emit(FileViewEvent::Saved(self.path.clone()));
            }
            Err(error) => self.set_message(format!("Save failed: {error}"), cx),
        }
        cx.notify();
        result
    }

    /// F-EDIT-05 "Reload": adopt the on-disk content.
    pub fn reload(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self.snapshot.is_some() {
            return Err("a snapshot has no file to reload".to_string());
        }
        let result = match self.editor_mut() {
            Some(editor) => editor.reload(),
            None => Err("the file is still loading".to_string()),
        };
        self.sync_code_from_editor(cx);
        if result.is_ok() {
            cx.emit(FileViewEvent::GitDiffStale(self.path.clone()));
        }
        cx.notify();
        result
    }

    /// F-EDIT-05 "Keep": dismiss the banner, keep the buffer.
    pub fn keep(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.is_some() {
            return;
        }
        if let Some(editor) = self.editor_mut() {
            editor.keep();
            cx.notify();
        }
        self.sync_code_from_editor(cx);
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
        if self.snapshot.is_some() {
            return;
        }
        let Some(range) = self.code_selection(cx) else {
            return;
        };
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let Some(selection) = Selection::new(editor.buffer(), range.start, range.end) else {
            return;
        };
        let before = editor.buffer().to_owned();
        let selection = match operation {
            MarkdownFormatOp::Bold => editor.format_bold(selection),
            MarkdownFormatOp::Italic => editor.format_italic(selection),
            MarkdownFormatOp::Heading => editor.toggle_heading(selection),
            MarkdownFormatOp::List => editor.toggle_list(selection),
            MarkdownFormatOp::Link { url } => editor.make_link(selection, &url),
        };
        // The op ran on the model; the code editor takes the same change as
        // one edit, so one undo step reverts it, and then shows the result.
        let change = minimal_edit(&before, editor.buffer());
        if let Some(code) = self.code_editor.clone() {
            code.update(cx, |code, cx| {
                if let Some((range, text)) = change {
                    code.edit([(range, text)], cx);
                }
                code.select([selection.start..selection.end], cx);
            });
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
    pub fn effective_mode(&self) -> MarkdownMode {
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

    /// The Preview's document for this frame, scheduling every diagram it
    /// names that has no state yet. `None` outside Markdown Preview.
    pub(crate) fn preview_document(&mut self, cx: &mut Context<Self>) -> Option<bezel_markdown::Doc> {
        if self.effective_mode() != MarkdownMode::Preview {
            return None;
        }
        let editor = self.editor()?;
        let path = editor.path().to_path_buf();
        let document = markdown_document(&path, editor.buffer())?;
        let settings = DiagramSettings::get(cx).cloned();
        if settings != self.diagram_settings {
            self.diagrams.forget_failures();
            self.diagram_settings = settings.clone();
            self.diagram_generation += 1;
        }
        let palette = markdown_preview::palette(Theme::get(cx));
        let base = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"));
        let preview = markdown_preview::build(document, &base, &self.diagrams, &palette);
        if let Some(settings) = settings {
            for request in preview.missing {
                self.render_diagram(request, &settings, &palette, &path, cx);
            }
        }
        Some(preview.doc)
    }

    /// Renders one diagram off the UI thread: the disk cache first, then the
    /// renderer. PlantUML waits for the app-wide turn; a cache hit never does.
    fn render_diagram(
        &mut self,
        request: DiagramRequest,
        settings: &DiagramSettings,
        palette: &Palette,
        file: &Path,
        cx: &mut Context<Self>,
    ) {
        // The same diagram twice in one frame is scheduled once.
        if self.diagrams.get(&request.key).is_some() {
            return;
        }
        self.diagrams.insert(request.key.clone(), DiagramState::Pending);
        let generation = self.diagram_generation;
        let DiagramRequest { key, kind, source } = request;
        let cache_dir = settings.cache_dir.clone();
        let server = settings.plantuml_server.clone();
        let expected_settings = settings.clone();
        let palette = palette.clone();
        let file = file.to_path_buf();
        let queue = (kind == DiagramKind::PlantUml).then(|| PlantUmlQueue::get(cx));
        cx.spawn(async move |this, cx| {
            // The disk cache first: a hit is Ready without rendering (§3) and
            // never waits for the PlantUML turn.
            let hit_dir = cache_dir.clone();
            let hit_key = key.clone();
            let hit = cx
                .background_spawn(async move { sirio_diagram::cached(&hit_dir, &hit_key) })
                .await;
            if let Some((path, width)) = hit {
                this.update(cx, |view, cx| {
                    if view.diagram_generation == generation {
                        view.diagrams.insert(key.clone(), DiagramState::Ready { path, width });
                    } else {
                        view.diagrams.remove(&key);
                    }
                    cx.notify();
                })
                .ok();
                return;
            }
            let _turn = match &queue {
                Some(queue) => Some(queue.turn().await),
                None => None,
            };
            // The view closed while waiting for the turn: abandon the render.
            if this.upgrade().is_none() {
                return;
            }
            if queue.is_some()
                && this
                    .update(cx, |_, cx| DiagramSettings::get(cx).cloned())
                    .ok()
                    .flatten()
                    .as_ref()
                    != Some(&expected_settings)
            {
                this.update(cx, |view, cx| {
                    view.diagrams.remove(&key);
                    cx.notify();
                })
                .ok();
                return;
            }
            let render_key = key.clone();
            let state = cx
                .background_spawn(async move {
                    let _perf = sirio_perf::span("Preview.diagram_render", source.len() as u64);
                    let options = sirio_diagram::Options {
                        palette,
                        plantuml_server: server,
                        working_dir: file.parent().map(Path::to_path_buf).unwrap_or_default(),
                    };
                    match sirio_diagram::render(kind, &source, &options) {
                        Ok(svg) => match sirio_diagram::store(&cache_dir, &render_key, &svg) {
                            Ok(path) => DiagramState::Ready {
                                path,
                                width: svg.logical_width,
                            },
                            Err(error) => {
                                DiagramState::Failed(DiagramError::Io(error.to_string()).note(kind))
                            }
                        },
                        Err(error) => DiagramState::Failed(error.note(kind)),
                    }
                })
                .await;
            this.update(cx, |view, cx| {
                // A result from before a settings change is discarded: the
                // Pending entry goes away so the next frame requests the
                // diagram again under the new settings.
                if view.diagram_generation == generation {
                    view.diagrams.insert(key.clone(), state);
                } else {
                    view.diagrams.remove(&key);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        preview: Option<PreviewFrame>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The drawer lists this tab's file first (a snapshot's is not the worktree's).
        if let Some(drawer) = &self.drawer {
            let file = (!self.is_snapshot()).then(|| self.path());
            drawer.update(cx, |drawer, _| drawer.set_current_file(file));
        }
        match &self.state {
            ViewState::Loading if self.snapshot_error.is_some() => render_snapshot_failure(
                self.snapshot_error.as_deref().unwrap_or_default(),
                theme,
                entity,
            ),
            ViewState::Loading => div()
                .id("file-loading")
                .debug_selector(|| "file-loading".to_owned())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.card_gap)
                .text_color(theme.ely.fg_muted)
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
                    let is_markdown = self.is_markdown();
                    let formatting_shown = self.formatting_controls_shown();
                    let editor_entity = entity.clone();
                    let editor_focus = self.editor_focus.clone();
                    // The press bubbles up from the code editor after it has
                    // taken focus itself, so this hands focus to the same
                    // target the press was for, never back to `editor_focus`.
                    div()
                        .id("file-editor")
                        .key_context("FileEditor")
                        .track_focus(&editor_focus)
                        .focusable()
                        .on_action(cx.listener(|view, _: &Find, window, cx| {
                            view.find_from_key(false, window, cx);
                        }))
                        .on_action(cx.listener(|view, _: &Replace, window, cx| {
                            view.find_from_key(true, window, cx);
                        }))
                        .on_action(cx.listener(|view, _: &ToggleDrawer, _, cx| {
                            if let Some(drawer) = &view.drawer {
                                drawer.update(cx, |drawer, cx| drawer.toggle(cx));
                            }
                        }))
                        .on_action(cx.listener(
                            |view, _: &SearchInWorktree, window, cx| {
                                let seed = view.selection_query(cx);
                                if let Some(drawer) = &view.drawer {
                                    drawer.update(cx, |drawer, cx| {
                                        drawer.search_in_worktree(seed, window, cx)
                                    });
                                }
                            },
                        ))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            editor_entity.update(cx, |view, cx| {
                                view.focus_target(cx).focus(window, cx);
                            });
                        })
                        .size_full()
                        .flex()
                        .flex_col()
                        .when_some(
                            self.render_breadcrumb(theme, entity.clone()),
                            |this, row| this.child(row),
                        )
                        .when(conflict != Conflict::None, |this| {
                            this.child(render_conflict_banner(conflict, theme, entity.clone()))
                        })
                        .when(is_markdown, |this| {
                            this.child(render_markdown_bar(
                                mode,
                                formatting_shown,
                                theme,
                                entity.clone(),
                            ))
                        })
                        .child(render_content(
                            editor,
                            mode,
                            theme,
                            entity.clone(),
                            self.code_editor.as_ref(),
                            self.render_find(cx),
                            &self.scroll,
                            preview,
                        ))
                        // The drawer is the last row of the column, so the
                        // `FileEditor` context covers its fields. The canvas
                        // tells it how tall the tab is, which caps it.
                        .when_some(self.drawer.clone(), |this, drawer| {
                            let measured = drawer.clone();
                            this.relative()
                                .child(
                                    canvas(
                                        move |bounds, window, cx| {
                                            if measured.read(cx).note_tab_height(bounds.size.height) {
                                                window.request_animation_frame();
                                            }
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .size_full(),
                                )
                                .child(
                                    // Escape in any field of the drawer gives the keyboard
                                    // back to this tab's code, as it does from find's query.
                                    div()
                                        .on_key_down({
                                            let entity = entity.clone();
                                            move |event, window, cx| {
                                                if event.keystroke.key == "escape" {
                                                    cx.stop_propagation();
                                                    entity.update(cx, |view, cx| {
                                                        view.focus_target(cx).focus(window, cx);
                                                    });
                                                }
                                            }
                                        })
                                        .child(drawer),
                                )
                        })
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
        let id = cx.entity_id().as_u64();
        let _render = sirio_perf::span("FileView.render", id);
        if self.focus_subscription.is_none() {
            let editor_focus = self.editor_focus.clone();
            self.focus_subscription =
                Some(cx.on_focus(&editor_focus, window, |view, _window, cx| {
                    view.gained_focus(cx)
                }));
        }
        let theme = *Theme::get(cx);
        // Production installs bezel alongside Sirio's theme. Some isolated
        // shell fixtures set only the Sirio global, so establish the same
        // invariant before anything below reaches for a bezel component
        // (mirrors the sidebar's popup guard). The code editor reads Ely's
        // theme instead, but the bezel primitives this view renders still
        // need this one.
        if cx.try_global::<bezel_theme::Theme>().is_none() {
            theme.install_into_bezel(cx);
        }
        let entity = cx.entity();
        let preview = self.preview_document(cx).map(|doc| PreviewFrame {
            doc,
            diagram_dir: DiagramSettings::get(cx).map(|settings| settings.cache_dir.clone()),
        });
        // The code editor is made the first frame the file is loaded, then
        // kept to the app's editor settings and handed the keyboard whenever
        // the mode or its existence changes under the focus.
        // A new generation of line colours per render: the code editor's
        // minimap asks for the same rows every frame (every row of a short
        // file, one row per pixel of its height otherwise), so this frame's
        // rows come back from the previous generation and nothing a frame
        // still shows is dropped.
        self.span_cache.borrow_mut().rotate();
        let step = sirio_perf::span("FileView.ensure_code_editor", id);
        self.ensure_code_editor(window, cx);
        drop(step);
        let step = sirio_perf::span("FileView.sync_code_prefs", id);
        self.sync_code_prefs(cx);
        drop(step);
        let step = sirio_perf::span("FileView.settle_status_popover", id);
        self.settle_status_popover(window, cx);
        drop(step);
        let step = sirio_perf::span("FileView.sync_find_washes", id);
        self.sync_find_washes(cx);
        drop(step);
        let step = sirio_perf::span("FileView.hand_over_focus", id);
        self.hand_over_focus(window, cx);
        drop(step);
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
                .border_color(theme.ely.border)
                .bg(theme.menu_surface())
                .shadow_lg();

            let items = self.context_menu_items(cx);
            let mut previous_group = None;
            for (index, item) in items.iter().enumerate() {
                if previous_group.is_some_and(|group| group != item.group) {
                    menu = menu.child(div().w_full().h(px(1.0)).my(px(4.0)).bg(theme.ely.border));
                }
                previous_group = Some(item.group);

                let item_entity = entity.clone();
                let selector = format!("file-context-item-{index}");
                let debug_selector = selector.clone();
                let state = item.state.clone();
                let is_disabled = matches!(state, ItemState::Unavailable(_));
                let action = item.action;
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
                        .items_center()
                        .text_size(theme.typography.footnote)
                        .when(is_disabled, |this| {
                            this.text_color(theme.ely.fg_subtle).cursor_not_allowed()
                        })
                        .when(!is_disabled, |this| {
                            this.text_color(theme.ely.fg)
                                .hover(|style| style.bg(theme.ely.hover))
                                .on_click(move |_, window, cx| {
                                    item_entity.update(cx, |view, cx| {
                                        if let Ok(effect) = view.context_effect(action, cx) {
                                            view.handle_context_action(effect, window, cx);
                                        }
                                    });
                                })
                        })
                        // One line per row: the note waits behind the pointer.
                        .when_some(note, |this, note| {
                            this.tooltip(move |window, cx| {
                                Tooltip::text(note.clone(), window, cx)
                            })
                        })
                        .child(item.label),
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
        let step = sirio_perf::span("FileView.status_row", id);
        let status_row = self.render_status_row(theme, &entity, cx);
        drop(step);
        let drawer_height = self
            .drawer
            .as_ref()
            .map_or(px(0.0), |drawer| drawer.read(cx).drawn_height());
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
                // Above the row, and the drawer over it, when there is one;
                // without it (a file still loading) the card sits at the
                // corner itself.
                None => div()
                    .absolute()
                    .bottom(
                        px(16.0)
                            + if status_row.is_some() {
                                status_row::STATUS_ROW_HEIGHT + drawer_height
                            } else {
                                px(0.0)
                            },
                    )
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
            .bg(theme.ely.bg)
            // Escape closes what is open over the code and goes no further, so
            // the code keeps its cursors: find first, then a hunk drawn in
            // place, one per press, the way a floating box goes before what is
            // only drawn. With neither open it passes on, and Ely collapses the
            // cursors as it always has.
            .capture_action(cx.listener(|view, _: &SingleCursor, window, cx| {
                if view.find.is_some() {
                    view.close_find(window, cx);
                    cx.stop_propagation();
                } else if view.open_hunk.is_some() {
                    view.close_hunk(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.open_context_menu(event.position, window, cx);
                }),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
            .when_some(hover_card, |this, card| this.child(card))
            .when_some(
                self.snapshot
                    .as_ref()
                    .map(|origin| render_snapshot_bar(origin, cx.entity())),
                |this, bar| this.child(bar),
            )
            .child(div().flex_1().min_h(px(0.0)).child({
                let _step = sirio_perf::span("FileView.render_state", id);
                self.render_state(theme, cx.entity(), preview, window, cx)
            }))
            .when_some(status_row, |this, row| this.child(row))
            // Last, so it paints over the content and the row: an absolute child
            // earlier in the order sits under the code's own background.
            .when_some(message, |this, card| this.child(card))
    }
}

/// The banner over a snapshot: what it is, and the way to the editable copy.
/// Ely's own banner is switched off for a snapshot (its "Make editable" has
/// nothing to unlock), so this is the only one. It sits above the find bar
/// and the code, over the failure card too, and keeps `file-snapshot-bar` as
/// its selector.
fn render_snapshot_bar(
    origin: &SnapshotOrigin,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let banner = ReadOnlyBanner::new("read-only", origin.banner_message());
    let banner = match (origin.banner_action(), origin.local_copy.clone()) {
        (Some(label), Some(path)) => banner.action(label, move |_, cx| {
            let path = path.clone();
            entity.update(cx, |_, cx| cx.emit(FileViewEvent::OpenLocalCopy(path)));
        }),
        _ => banner,
    };
    div()
        .id("file-snapshot-bar")
        .debug_selector(|| "file-snapshot-bar".into())
        .w_full()
        .child(banner)
}

/// A snapshot whose read failed: the reason, and the Retry that is the
/// tab's only remedy. It fills the surface, like the Loading it replaces,
/// so nothing that dismisses a card can take it away.
fn render_snapshot_failure(
    reason: &str,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> AnyElement {
    div()
        .id("file-snapshot-failed")
        .debug_selector(|| "file-snapshot-failed".into())
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(theme.spacing.card_gap)
        .p(px(24.0))
        .text_color(theme.ely.fg_muted)
        // The reason may be a git stderr tail: bounded and wrapped, the
        // way the hover card bounds a server's answer, so it never runs
        // off the tab sideways.
        .child(
            div()
                .max_w(px(520.0))
                .w_full()
                .whitespace_normal()
                .text_center()
                .child(reason.to_owned()),
        )
        .child(
            div()
                .id("file-snapshot-retry")
                .debug_selector(|| "file-snapshot-retry".into())
                .px(px(10.0))
                .py(px(3.0))
                .rounded(theme.radii.control)
                .cursor_pointer()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .hover(|style| style.bg(theme.ely.hover).text_color(theme.ely.fg))
                .on_click(move |_, _, cx| {
                    entity.update(cx, |_, cx| cx.emit(FileViewEvent::RetrySnapshot));
                })
                .child("Retry"),
        )
        .into_any_element()
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
        .text_color(if active { theme.ely.fg } else { theme.ely.fg_subtle })
        .when(active, |this| this.bg(theme.ely.active))
        .hover(|style| style.bg(theme.ely.hover))
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
        .bg(theme.ely.danger_subtle)
        .border_b_1()
        .border_color(theme.ely.danger)
        .text_size(theme.typography.footnote)
        .text_color(theme.ely.fg)
        .child(div().flex_1().child(message))
        .when(conflict == Conflict::ChangedOnDisk, |this| {
            this.child(
                div()
                    .id("file-conflict-reload")
                    .debug_selector(|| "file-conflict-reload".into())
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.ely.hover))
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
                    .hover(|style| style.bg(theme.ely.hover))
                    .on_click(move |_, _, cx| {
                        keep_entity.update(cx, |view, cx| view.keep(cx));
                    })
                    .child("Keep"),
            )
        })
}

/// Whether the Markdown row draws the formatting controls for a file in `mode`:
/// Code only, and only when the file is editable (a snapshot is not).
fn formatting_controls_drawn(mode: MarkdownMode, editable: bool) -> bool {
    mode == MarkdownMode::Code && editable
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
/// surface, and there is nothing rendered there to format. A snapshot has none
/// either, in either mode: it refuses every edit, so a button would only lie.
/// `formatting_shown` is `FileView::formatting_controls_shown`, the same answer
/// `surface.editor.state` reports. The link URL is a deterministic placeholder
/// until the view has a text prompt seam of its own.
fn render_markdown_bar(
    mode: MarkdownMode,
    formatting_shown: bool,
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
        .border_color(theme.ely.border)
        .bg(theme.ely.surface)
        .when(formatting_shown, move |this| {
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
        .text_color(theme.ely.fg)
        .hover(|style| style.bg(theme.ely.hover))
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
    code: Option<&gpui::Entity<CodeEditor>>,
    find: Option<AnyElement>,
    scroll: &SurfaceScroll,
    preview: Option<PreviewFrame>,
) -> AnyElement {
    let is_markdown = editor.language() == Language::Markdown;
    // Preview renders the parsed document; a locked preview (large file,
    // F-EDIT-03) or Code mode renders the source. The two modes are
    // behaviour — each shows the *right* content — while the rendering
    // inside them is appearance.
    if let Some(PreviewFrame { doc, diagram_dir }) = preview {
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
                move |target, _window, cx| {
                    if markdown_preview::is_diagram_target(target, diagram_dir.as_deref()) {
                        return;
                    }
                    match resolve_file_link(target, &base) {
                        Some(resolved) => {
                            link_entity.update(cx, |_, cx| {
                                cx.emit(FileViewEvent::OpenFile(resolved.path));
                            });
                        }
                        None => cx.open_url(target),
                    }
                },
            );
        // The column is a fixed width, not a cap: a pane narrower than it
        // scrolls the page sideways rather than rewrapping the prose into a
        // strip, the way Code mode never wraps a source line.
        //
        // The surface takes what the Markdown bar above it leaves, as the
        // code editor's `flex_1().min_h(0)` container does: at `size_full` it was
        // the whole pane tall, so the bar's height of document — and both
        // scrollbars' bottom edge — hung below the pane, out of reach.
        return div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .relative()
            .child(
                div()
                    .id("file-markdown-scroll")
                    .debug_selector(|| "file-markdown-scroll".into())
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&scroll.preview)
                    .child(
                        div()
                            .debug_selector(|| "file-markdown-column".into())
                            .w(px(MARKDOWN_COLUMN_WIDTH))
                            .mx_auto()
                            .p(px(24.0))
                            .child(Chat::render_markdown_document_with_link_override(
                                doc,
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
            .child(horizontal_scrollbar(
                "file-markdown-horizontal-bar",
                &scroll.preview,
                &scroll.preview_horizontal_bar,
                entity,
            ))
            .into_any_element();
    }

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
                .bg(theme.ely.surface)
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
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
                        .hover(|style| style.bg(theme.ely.hover))
                        .on_click(move |_, _, cx| {
                            preview_entity.update(cx, |view, cx| {
                                view.set_markdown_mode(MarkdownMode::Preview, cx);
                            });
                        })
                        .child("Render preview"),
                ),
        );
    }
    // The code editor draws and scrolls the source itself; the selector is the
    // one the drawn-frame tests use to ask whether the source is on screen.
    // The find bar, when open, is the first row of this column and the code
    // takes what it leaves, so nothing is drawn over the code.
    source
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .when_some(find, |this, find| this.child(find))
                .child(
                    div()
                        .id("file-text-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .relative()
                        .debug_selector(|| "file-text-scroll".into())
                        .when_some(code.cloned(), |this, code| {
                            // A right press on the code is the code editor's own
                            // `SecondaryClick`, which opens the menu. Stopping it
                            // here keeps the view's listener above from opening it
                            // a second time. The stop is in bubble order, after
                            // the editor's listener has run.
                            this.on_mouse_down(MouseButton::Right, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .child(code)
                        }),
                ),
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
pub(crate) fn scrollbar(id: &'static str, handle: &ScrollHandle, state: &ScrollbarState) -> AnyElement {
    let has_overflow = |handle: &ScrollHandle| {
        crate::scroll::thumb(
            handle.bounds().size.height,
            handle.max_offset().y,
            handle.offset().y,
            crate::scroll::MIN_THUMB,
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

/// The horizontal counterpart of [`scrollbar`], for a surface wider than
/// its viewport. The bar overlays the surface's own box, so it spans
/// exactly the viewport it reports on and never reflows what is beneath
/// it; a drag moves only X, leaving the vertical offset where it was.
/// While there is nothing to show, the same canvas watch asks for the
/// frame that paints the bar once the content first lays out wider.
pub(crate) fn horizontal_scrollbar<V: 'static>(
    id: &'static str,
    handle: &ScrollHandle,
    state: &HorizontalBarState,
    entity: gpui::Entity<V>,
) -> AnyElement {
    let has_overflow = |handle: &ScrollHandle| {
        crate::scroll::thumb(
            handle.bounds().size.width,
            handle.max_offset().x,
            handle.offset().x,
            crate::scroll::MIN_THUMB,
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
    let drag_handle = handle.clone();
    horizontal_scroll::bar(
        id,
        handle.bounds().size.width,
        handle.max_offset().x,
        handle.offset().x,
        state,
        move |x, cx| {
            drag_handle.set_offset(point(x, drag_handle.offset().y));
            entity.update(cx, |_, cx| cx.notify());
        },
    )
}

/// One classified span of a source line, in bezel-syntax's own vocabulary.
///
/// The kind is `bezel_theme::HighlightKind` rather than a reduction of it:
/// an earlier version funnelled 24 kinds into keyword/literal/comment and
/// dropped the rest, which is how a tree-sitter parse came out looking like
/// a three-colour regex highlighter — and how a number ended up painted in
/// the string colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeSpan {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) kind: bezel_theme::HighlightKind,
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
        .border_color(theme.ely.border)
        .rounded(theme.radii.control)
        .font_family(theme.typography.code_family)
        .text_size(theme.typography.code_size)
        .text_color(theme.ely.fg)
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
        // A press on the card is the card's: without this it falls through to
        // the code under it and moves the caret as it dismisses.
        .occlude()
        .max_w(px(420.0))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .bg(theme.menu_surface())
        .border_1()
        .border_color(theme.ely.border)
        .rounded(theme.radii.control)
        .font_family(theme.typography.ui_family)
        .text_size(theme.typography.base_size)
        .text_color(theme.ely.fg)
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
                    .text_color(theme.ely.fg_muted)
                    .hover(|style| style.bg(theme.ely.hover).text_color(theme.ely.fg))
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
    let message: String = message.into();
    div()
        .id("file-view-notice")
        .debug_selector(|| "file-view-notice".into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .text_size(theme.typography.headline)
        .text_color(theme.ely.fg_muted)
        .child(selectable_text(message))
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

/// Every styled run one source line hands to the code editor: the grammar's
/// colours (`spans`, from [`code_spans`]) and the Markdown link rules. The
/// code editor draws the findings itself (`code_surface::ely_diagnostics`), so
/// the highlighter has no squiggles to add.
///
/// The runs are sorted by start, because `with_highlights` ends in
/// `compute_runs`, which walks the ranges assuming each begins at or after the
/// previous one's end.
fn line_highlights(
    spans: &[CodeSpan],
    links: &[(Range<usize>, String)],
    theme: &Theme,
    syntax_palette: &bezel_theme::SyntaxPalette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut painted: Vec<(Range<usize>, HighlightStyle)> = spans
        .iter()
        .map(|span| {
            let color = syntax_palette.color(span.kind);
            (
                span.range.clone(),
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
                color: Some(theme.ely.link),
                underline: Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(theme.ely.link),
                    wavy: false,
                }),
                ..Default::default()
            },
        )
    }));
    painted.sort_by_key(|(range, _)| range.start);
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::editor::markdown_links_in_line;
    use gpui::{Focusable, Modifiers, VisualTestContext};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Sets the code editor's selection to `range`, as a drag or a keystroke
    /// would. `mounted_file_view` has drawn two frames, so the editor exists.
    fn select_code(view: &gpui::Entity<FileView>, cx: &mut gpui::App, range: Range<usize>) {
        let code = view
            .read(cx)
            .code_editor()
            .cloned()
            .expect("the code editor is drawn once the file has loaded");
        code.update(cx, |code, cx| code.select([range], cx));
    }

    /// Gives the keyboard to the code editor, as a click on the code does.
    fn focus_code(view: &gpui::Entity<FileView>, window: &mut Window, cx: &mut gpui::App) {
        let code = view
            .read(cx)
            .code_editor()
            .cloned()
            .expect("the code editor is drawn once the file has loaded");
        code.read(cx).focus_handle(cx).focus(window, cx);
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
        let painted = |needle: &str, kind: bezel_theme::HighlightKind| {
            let at = source.find(needle).expect("needle is in the fixture");
            spans
                .iter()
                .any(|span| span.range == (at..at + needle.len()) && span.kind == kind)
        };
        assert!(painted("public", bezel_theme::HighlightKind::Keyword), "{spans:?}");
        assert!(painted("42", bezel_theme::HighlightKind::Number), "{spans:?}");
        assert!(painted("// note", bezel_theme::HighlightKind::Comment), "{spans:?}");
    }

    /// The editor must paint what tree-sitter actually classified, not a
    /// three-bucket reduction of it. bezel-syntax resolves `usize` as
    /// `TypeBuiltin` and `compute_total` as `Function`; a surface that
    /// keeps only keyword/literal/comment is the pre-tree-sitter colour
    /// scheme wearing a tree-sitter parser.
    #[test]
    fn every_kind_bezel_classifies_reaches_the_code_surface() {
        let source = "let count: usize = compute_total(&items) + 42; // note";
        let offered = bezel_syntax::highlight(source, "rust").unwrap_or_default();
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
            // Exactly what the code editor's highlighter paints with.
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
            crate::ely::init(cx);
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

    /// The keyboard reaches the code editor through the view's focus, with no
    /// click and no hand-focusing of the code editor. The view takes focus
    /// before any frame is drawn; the frame that creates the code editor hands
    /// the keyboard over to it (`hand_over_focus`), so a file that opens takes
    /// typing at once.
    #[gpui::test]
    async fn keyboard_input_edits_the_buffer_through_the_focused_file_view(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("keyboard-input", "hello\n");
        let (mut cx, view) =
            mounted_file_view_with(cx, file.path().to_path_buf(), |view, window, cx| {
                view.update(cx, |view, cx| view.editor_focus.focus(window, cx));
            });
        let code = view
            .read_with(&cx.cx, |view, _| view.code_editor().cloned())
            .expect("the frame that loaded the file drew the code editor");
        cx.update(|window, app| {
            assert!(
                code.read(app).focus_handle(app).is_focused(window),
                "the keyboard went to the code editor without any click"
            );
        });

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
        assert_eq!(
            code.read_with(&cx.cx, |code, _| code.text().to_owned()),
            buffer,
            "the code editor shows what the keys wrote"
        );
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
            crate::ely::init(cx);
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
            crate::ely::init(cx);
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
            crate::ely::init(cx);
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
            cx.debug_bounds("code-editor-line-350").is_none(),
            "line 350 starts off screen, or the test proves nothing"
        );
        view.update(&mut cx.cx, |view, cx| view.reveal(RevealTarget::Line(350), cx));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("code-editor-line-350").is_some(),
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
            crate::ely::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            let mut view = FileView::new(file.path().to_path_buf(), cx);
            // Before any load has completed.
            view.reveal(RevealTarget::Line(350), cx);
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
            cx.debug_bounds("code-editor-line-350").is_some(),
            "the reveal was held until the content existed, not dropped"
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
    async fn a_key_on_the_code_dismisses_a_resting_hover_card_and_still_reaches_the_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        // A key is not a pointer move, so the card is dismissed by the key
        // itself; the caret moving proves the key still reached Ely.
        let file = TempFile::with_extension("rs", "let x = 1;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic {
                    line: 0,
                    range: 4..5,
                    severity: DiagnosticSeverity::Error,
                    message: "boom".into(),
                }],
                cx,
            );
            view.hover_moved(4, point(px(10.), px(10.)), cx);
        });
        cx.update(|window, cx| {
            focus_code(&view, window, cx);
            window.simulate_next_frame(cx);
        });
        cx.update(|_, app| select_code(&view, app, 0..0));
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned)).as_deref(),
            Some("boom"),
            "the card is showing before the key"
        );

        cx.simulate_keystrokes("right");
        cx.run_until_parked();

        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned)),
            None,
            "a key on the code dismisses the card"
        );
        let caret = cx.update(|_, app| {
            let code = view
                .read(app)
                .code_editor()
                .cloned()
                .expect("the code editor is drawn once the file has loaded");
            code.read(app).primary().range()
        });
        assert_eq!(caret, 1..1, "the arrow key still moved the caret in the editor");
    }

    /// Tests for `line_highlights` — the composition the code editor's highlighter hands
    /// to `StyledText`, where the syntax colours and the Markdown link rules
    /// meet.
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
        fn a_markdown_link_still_underlines_straight_when_nothing_is_wrong() {
            let theme = Theme::dark();
            let palette = theme.syntax_palette();
            let line = "[setup](setup.md)";
            let links = markdown_links_in_line(line)
                .into_iter()
                .map(|span| (span.label_range, span.target))
                .collect::<Vec<_>>();
            let highlights =
                line_highlights(&code_spans(Language::Markdown, line), &links, &theme, &palette);
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
        mounted_file_view_with(cx, path, |_, _, _| {})
    }

    /// `mounted_file_view`, with `before_frames` run as soon as the window
    /// exists, before the load finishes and before any frame is drawn: the
    /// frame that creates the code editor comes after it.
    fn mounted_file_view_with(
        cx: &mut gpui::TestAppContext,
        path: PathBuf,
        before_frames: impl FnOnce(&gpui::Entity<FileView>, &mut Window, &mut gpui::App),
    ) -> (VisualTestContext, gpui::Entity<FileView>) {
        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
        let window = cx.add_window(|_window, cx| FileView::new(path, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, app| {
            let view = window.root::<FileView>().flatten().expect("file view root");
            before_frames(&view, window, app);
        });
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

    /// A diff that opens one hunk, on line 1, which adds a line after line 1.
    fn one_added_line() -> sirio_git::FileDiff {
        sirio_git::parse_diff("@@ -1,0 +2 @@\n+let z = 0;\n", Path::new("main.rs"))
    }

    // One diff fetch at a time (`begin_git_diff` / `land_git_diff`). The unit has
    // these ways to fail, each pinned below:
    //   - a `GitDiffStale` that arrives while a fetch runs starts another one, so an
    //     agent rewriting the file piles up whole-worktree scans;
    //   - the requests that arrived meanwhile are forgotten, so the marks stay on a
    //     diff older than the file;
    //   - they are remembered as one request each, so landing starts a run per
    //     request instead of one;
    //   - landing with nothing pending asks for another fetch, so a quiet file
    //     refetches forever;
    //   - landing no diff (a clean file, a git error) loses what arrived meanwhile.

    /// Stands in for the app's one path, `refresh_git_diff`: each `GitDiffStale` asks
    /// the view to begin a fetch, and a granted one is counted. The fetch runs until
    /// the test lands it.
    fn count_granted_git_diff_fetches(
        view: &gpui::Entity<FileView>,
        cx: &mut VisualTestContext,
    ) -> std::rc::Rc<std::cell::Cell<usize>> {
        let granted = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = granted.clone();
        cx.update(|_, app| {
            app.subscribe(view, move |view, event: &FileViewEvent, app| {
                if matches!(event, FileViewEvent::GitDiffStale(_))
                    && view.update(app, |view, _| view.begin_git_diff())
                {
                    counter.set(counter.get() + 1);
                }
            })
            .detach();
        });
        granted
    }

    fn git_diff_goes_stale(view: &gpui::Entity<FileView>, cx: &mut VisualTestContext) {
        view.update(&mut cx.cx, |view, cx| {
            cx.emit(FileViewEvent::GitDiffStale(view.path().to_path_buf()));
        });
    }

    fn land_git_diff(
        view: &gpui::Entity<FileView>,
        diff: Option<sirio_git::FileDiff>,
        cx: &mut VisualTestContext,
    ) {
        view.update(&mut cx.cx, |view, cx| view.land_git_diff(diff, cx));
    }

    #[gpui::test]
    async fn stale_requests_during_a_fetch_ask_for_exactly_one_more(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "let x = 1;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let granted = count_granted_git_diff_fetches(&view, &mut cx);

        git_diff_goes_stale(&view, &mut cx);
        assert_eq!(granted.get(), 1, "the first request starts a fetch");
        for _ in 0..3 {
            git_diff_goes_stale(&view, &mut cx);
        }
        assert_eq!(granted.get(), 1, "requests during the fetch start nothing");

        land_git_diff(&view, None, &mut cx);
        assert_eq!(granted.get(), 2, "landing asks once for what arrived meanwhile");

        land_git_diff(&view, None, &mut cx);
        assert_eq!(granted.get(), 2, "the re-run found nothing pending");
    }

    #[gpui::test]
    async fn landing_with_nothing_pending_asks_for_nothing_and_frees_the_next_request(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "let x = 1;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let granted = count_granted_git_diff_fetches(&view, &mut cx);

        git_diff_goes_stale(&view, &mut cx);
        land_git_diff(&view, Some(one_added_line()), &mut cx);
        assert_eq!(granted.get(), 1, "nothing was pending, so nothing is asked");
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.git_marks_text()),
            "1:added",
            "the landed diff is shown"
        );

        git_diff_goes_stale(&view, &mut cx);
        assert_eq!(granted.get(), 2, "an idle view starts a fetch at once");
    }

    #[gpui::test]
    async fn landing_no_diff_still_runs_what_arrived_meanwhile(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "let x = 1;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let granted = count_granted_git_diff_fetches(&view, &mut cx);

        git_diff_goes_stale(&view, &mut cx);
        git_diff_goes_stale(&view, &mut cx);
        assert_eq!(granted.get(), 1, "the second request waits for the fetch");
        land_git_diff(&view, None, &mut cx);
        assert_eq!(granted.get(), 2, "a clean or failed answer still re-runs");
    }

    /// How many cursors the code editor holds.
    fn cursor_count(view: &gpui::Entity<FileView>, cx: &VisualTestContext) -> usize {
        view.read_with(&cx.cx, |view, app| {
            view.code_editor()
                .expect("the code editor is drawn once the file has loaded")
                .read(app)
                .selections()
                .len()
        })
    }

    #[gpui::test]
    async fn escape_closes_find_then_a_hunk_and_leaves_the_cursors_until_none_is_open(
        cx: &mut gpui::TestAppContext,
    ) {
        // Escape is Ely's `SingleCursor`, which collapses the cursors. With find
        // open it closes find and stops, then with a hunk drawn in place it closes
        // that and stops, so both cursors stay. With neither open it reaches Ely,
        // which collapses them.
        let file = TempFile::with_extension("rs", "let x = 1;\nlet y = 2;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        view.update(&mut cx.cx, |view, cx| {
            view.set_git_diff(Some(one_added_line()), cx);
            view.toggle_hunk(1, cx);
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.open_hunk_line()),
            Some(1),
            "the press on the mark draws the hunk"
        );
        open_find(&view, "let", None, &mut cx);
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.find_matches()),
            Some(2),
            "find is open"
        );
        cx.update(|window, cx| {
            focus_code(&view, window, cx);
            window.simulate_next_frame(cx);
        });
        cx.update(|_, app| {
            let code = view
                .read(app)
                .code_editor()
                .cloned()
                .expect("the code editor is drawn once the file has loaded");
            code.update(app, |code, cx| code.select([0..0, 11..11], cx));
        });
        assert_eq!(cursor_count(&view, &cx), 2, "two cursors before the key");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.find_matches()),
            None,
            "Escape closes find"
        );
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.open_hunk_line()),
            Some(1),
            "that Escape leaves the hunk drawn"
        );
        assert_eq!(cursor_count(&view, &cx), 2, "and stops there, so the cursors stay");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.open_hunk_line()),
            None,
            "the next Escape closes the hunk drawn in place"
        );
        assert_eq!(
            cursor_count(&view, &cx),
            2,
            "that Escape stops at the hunk, so the cursors stay"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(
            cursor_count(&view, &cx),
            1,
            "with nothing open, Escape reaches Ely and collapses the cursors"
        );
    }

    /// Opens find on `query` with the replace row on `replace`, then draws a frame.
    fn open_find(
        view: &gpui::Entity<FileView>,
        query: &str,
        replace: Option<&str>,
        cx: &mut VisualTestContext,
    ) {
        let opened = cx.update(|window, app| {
            view.update(app, |view, cx| {
                view.open_find(
                    Some(query.to_owned()),
                    ely_gpui_component::editor::FindOptions::default(),
                    replace.map(str::to_owned),
                    window,
                    cx,
                )
            })
        });
        assert!(opened, "find opens on a file shown as code");
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
    }

    fn replace_matches(view: &gpui::Entity<FileView>, all: bool, cx: &mut VisualTestContext) {
        cx.update(|window, app| {
            view.update(app, |view, cx| view.replace_matches(all, window, cx))
                .expect("replacing is accepted")
        });
        cx.run_until_parked();
    }

    /// How many matches find holds and which one is current (0-based).
    fn found(
        view: &gpui::Entity<FileView>,
        cx: &VisualTestContext,
    ) -> (Option<usize>, Option<usize>) {
        view.read_with(&cx.cx, |view, _| (view.find_matches(), view.find_current()))
    }

    #[gpui::test]
    async fn replacing_one_match_moves_on_to_the_next_and_keeps_the_later_ones_right(
        cx: &mut gpui::TestAppContext,
    ) {
        // The "é" is two bytes, so an offset counted in characters is wrong from
        // the first match on. The replacement holds the query and is longer than
        // it, so every later match moves and the text just written matches again.
        let file = TempFile::with_extension("rs", "é x x x\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        open_find(&view, "x", Some("xx"), &mut cx);
        assert_eq!(found(&view, &cx), (Some(3), Some(0)), "three matches, the first current");

        replace_matches(&view, false, &mut cx);
        assert_eq!(buffer_of(&view, &cx), "é xx x x\n");
        assert_eq!(
            found(&view, &cx),
            (Some(4), Some(2)),
            "the next match is current, not the text just written"
        );

        replace_matches(&view, false, &mut cx);
        assert_eq!(
            buffer_of(&view, &cx),
            "é xx xx x\n",
            "the later match moved with the first edit"
        );
        assert_eq!(found(&view, &cx), (Some(5), Some(4)));

        replace_matches(&view, false, &mut cx);
        assert_eq!(buffer_of(&view, &cx), "é xx xx xx\n");
        assert_eq!(
            found(&view, &cx),
            (Some(6), Some(0)),
            "past the last match it wraps to the first"
        );
    }

    #[gpui::test]
    async fn replacing_every_match_is_one_undo_step(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "x x x\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        open_find(&view, "x", Some("y"), &mut cx);

        replace_matches(&view, true, &mut cx);
        assert_eq!(buffer_of(&view, &cx), "y y y\n");
        assert_eq!(found(&view, &cx), (Some(0), None), "nothing is left to find");

        cx.update(|window, app| focus_code(&view, window, app));
        cx.simulate_keystrokes(&clipboard_chord("z"));
        cx.run_until_parked();
        assert_eq!(buffer_of(&view, &cx), "x x x\n", "one undo restores every match");
    }

    #[gpui::test]
    async fn an_edit_in_the_code_recomputes_the_matches(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("rs", "x x x\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        open_find(&view, "x", None, &mut cx);
        assert_eq!(found(&view, &cx).0, Some(3));

        cx.update(|_, app| {
            let code = view.read(app).code_editor().cloned().expect("code editor");
            code.update(app, |code, cx| code.edit([(0..0, "x ".to_owned())], cx));
        });
        cx.run_until_parked();
        assert_eq!(found(&view, &cx).0, Some(4), "the typed match counts");

        cx.update(|_, app| {
            let code = view.read(app).code_editor().cloned().expect("code editor");
            code.update(app, |code, cx| code.edit([(0..4, String::new())], cx));
        });
        cx.run_until_parked();
        assert_eq!(found(&view, &cx).0, Some(2), "a deleted match no longer counts");
    }

    #[gpui::test]
    async fn the_find_key_searches_for_a_one_line_selection_and_keeps_the_query_for_any_other(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "let x = 1;\nlet y = 2;\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, app| {
            init(app);
            focus_code(&view, window, app);
            select_code(&view, app, 4..5);
        });
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(found(&view, &cx), (Some(1), Some(0)), "the selected `x` is the query");

        // A selection that crosses a line break is no query: the old one stays.
        cx.update(|_, app| select_code(&view, app, 0..15));
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(found(&view, &cx), (Some(1), Some(0)), "the query is still `x`");
    }

    #[gpui::test]
    async fn escape_in_the_query_closes_find_and_hands_the_keyboard_to_the_code(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "x x x\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        open_find(&view, "x", None, &mut cx);
        let code_focused = |cx: &mut VisualTestContext| {
            cx.update(|window, app| {
                let code = view.read(app).code_editor().cloned().expect("code editor");
                code.read(app).focus_handle(app).is_focused(window)
            })
        };
        assert!(!code_focused(&mut cx), "typing goes to the query while find is open");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(found(&view, &cx).0, None, "Escape in the query closes find");
        assert!(code_focused(&mut cx), "and the keyboard goes back to the code");
    }

    // How Find can fail around the box and the query (task 9b, from 9a's review):
    //
    // - A card already drawn for the code stays when the pointer slides from the
    //   code onto the find bar.
    // - The bar floats over the code instead of sitting above it: it hides the
    //   first lines (and the read-only banner at the top of the code), and
    //   closing it leaves the code at a different height than it had. No unit
    //   test guards it; the editor E2E's `find-open` capture and the flex column
    //   in `render_content` show the bar docked above the code.
    // - Find pressed while typing in the query takes the code's selection as
    //   the query, which after any step is the current match, and so replaces
    //   a pattern such as `fo+` with the text it matched.
    // - A selection seeds the query of a regular expression verbatim, so `f(`
    //   is an error and `a.b` finds more than was selected.

    /// The text of find's query field, `None` while find is closed.
    fn find_query(view: &gpui::Entity<FileView>, cx: &VisualTestContext) -> Option<String> {
        view.read_with(&cx.cx, |view, app| {
            view.find
                .as_ref()
                .map(|find| find.query.read(app).text().to_owned())
        })
    }

    fn query_focused(view: &gpui::Entity<FileView>, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, app| {
            let query = view
                .read(app)
                .find
                .as_ref()
                .expect("find is open")
                .query
                .clone();
            query.read(app).focus_handle(app).is_focused(window)
        })
    }

    #[gpui::test]
    async fn the_pointer_sliding_from_the_code_onto_the_find_bar_drops_the_card(
        cx: &mut gpui::TestAppContext,
    ) {
        // Long lines and one finding over all of them, so any point of the code
        // the pointer can reach is a point the card would answer.
        let line = "x".repeat(400);
        let text = format!("{line}\n{line}\n{line}\n");
        let file = TempFile::with_extension("rs", &text);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let whole = 0..text.len();
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic {
                    line: 0,
                    range: whole,
                    severity: DiagnosticSeverity::Error,
                    message: "boom".into(),
                }],
                cx,
            );
        });
        open_find(&view, "x", None, &mut cx);
        let card = |cx: &VisualTestContext| {
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned))
        };

        let line_bounds = cx
            .debug_bounds("code-editor-line-0")
            .expect("the first line is drawn");
        // Past the gutter.
        let on_code = point(line_bounds.origin.x + px(300.), line_bounds.center().y);
        cx.simulate_mouse_move(on_code, None, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            card(&cx).as_deref(),
            Some("boom"),
            "resting on the code shows the diagnostic"
        );

        let find = cx.debug_bounds("file-find").expect("the find box is drawn");
        cx.simulate_mouse_move(find.center(), None, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            card(&cx),
            None,
            "the pointer on the find bar does not keep the card of the code it left"
        );
    }

    // How a hover card can outlive the pointer's visit to the code (task 12a, from
    // 9b's review): Ely emits no `Pointer { offset: None }` when the pointer
    // leaves the code, so a card drawn for the code stays when the pointer slides
    // onto another surface of the tab. The cure is at the source (vendored Ely says
    // so when the pointer leaves the code); the find bar's test above is the third
    // of these.
    //
    // - A card stays when the pointer slides onto the conflict banner.
    // - A card stays when the pointer slides onto the Markdown row.

    /// Rests the pointer on the first line's code until its diagnostic card is
    /// drawn, slides it onto `surface`, and answers the card before and after.
    fn card_before_and_after_sliding_onto(
        extension: &str,
        surface: &'static str,
        prepare: impl FnOnce(&gpui::Entity<FileView>, &mut VisualTestContext),
        cx: &mut gpui::TestAppContext,
    ) -> (Option<String>, Option<String>) {
        let line = "x".repeat(400);
        let text = format!("{line}\n{line}\n{line}\n");
        let file = TempFile::with_extension(extension, &text);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let whole = 0..text.len();
        prepare(&view, &mut cx);
        view.update(&mut cx.cx, |view, cx| {
            view.set_diagnostics(
                vec![FileDiagnostic {
                    line: 0,
                    range: whole,
                    severity: DiagnosticSeverity::Error,
                    message: "boom".into(),
                }],
                cx,
            );
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let card = |cx: &VisualTestContext| {
            view.read_with(&cx.cx, |view, _| view.hover_card().map(str::to_owned))
        };
        let line_bounds = cx
            .debug_bounds("code-editor-line-0")
            .expect("the first line is drawn");
        let on_code = point(line_bounds.origin.x + px(300.), line_bounds.center().y);
        cx.simulate_mouse_move(on_code, None, Modifiers::none());
        cx.run_until_parked();
        let before = card(&cx);
        let target = cx.debug_bounds(surface).expect("the surface is drawn");
        cx.simulate_mouse_move(target.center(), None, Modifiers::none());
        cx.run_until_parked();
        (before, card(&cx))
    }

    #[gpui::test]
    async fn the_pointer_sliding_from_the_code_onto_the_conflict_banner_drops_the_card(
        cx: &mut gpui::TestAppContext,
    ) {
        let (before, after) = card_before_and_after_sliding_onto(
            "rs",
            "file-conflict-banner",
            |view, cx| {
                view.update(&mut cx.cx, |view, cx| {
                    let editor = view.editor_mut().expect("editor loaded");
                    editor.insert(0, "user edit\n").expect("local edit");
                    cx.notify();
                });
                std::fs::write(view.read_with(&cx.cx, |view, _| view.path().to_path_buf()), "external\n")
                    .expect("external write");
                view.update(&mut cx.cx, |view, cx| view.check_external(cx));
                cx.update(|window, cx| {
                    window.refresh();
                    window.simulate_next_frame(cx);
                    window.simulate_next_frame(cx);
                });
            },
            cx,
        );
        assert_eq!(before.as_deref(), Some("boom"), "resting on the code shows it");
        assert_eq!(after, None, "the banner does not keep the code's card");
    }

    #[gpui::test]
    async fn the_pointer_sliding_from_the_code_onto_the_markdown_row_drops_the_card(
        cx: &mut gpui::TestAppContext,
    ) {
        let (before, after) =
            card_before_and_after_sliding_onto(
                "md",
                "file-markdown-row",
                |view, cx| {
                    view.update(&mut cx.cx, |view, cx| {
                        view.set_markdown_mode(MarkdownMode::Code, cx)
                    });
                },
                cx,
            );
        assert_eq!(before.as_deref(), Some("boom"), "resting on the code shows it");
        assert_eq!(after, None, "the Markdown row does not keep the code's card");
    }

    #[gpui::test]
    async fn the_find_key_in_the_query_selects_it_and_never_reseeds_it(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "foo fooo foo\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|_, app| init(app));
        let options = ely_gpui_component::editor::FindOptions {
            regex: true,
            ..Default::default()
        };
        cx.update(|window, app| {
            view.update(app, |view, cx| {
                view.open_find(Some("fo+".into()), options, None, window, cx)
            });
        });
        cx.run_until_parked();
        assert_eq!(found(&view, &cx), (Some(3), Some(0)));
        assert!(query_focused(&view, &mut cx), "open_find puts the keyboard in the query");

        // `open_find` selected the first match in the code, `foo`: the text a
        // seed would take.
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(
            find_query(&view, &cx).as_deref(),
            Some("fo+"),
            "Find in the query field does not replace the pattern with the code's selection"
        );
        assert!(query_focused(&view, &mut cx), "and the keyboard stays in the query");
        assert_eq!(found(&view, &cx), (Some(3), Some(0)));
    }

    #[gpui::test]
    async fn a_selection_that_is_the_current_match_is_not_a_new_query(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "foo fooo foo\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|_, app| init(app));
        let options = ely_gpui_component::editor::FindOptions {
            regex: true,
            ..Default::default()
        };
        cx.update(|window, app| {
            view.update(app, |view, cx| {
                view.open_find(Some("fo+".into()), options, None, window, cx);
                view.find_step(true, window, cx);
            });
        });
        cx.run_until_parked();
        assert_eq!(found(&view, &cx), (Some(3), Some(1)), "`fooo` is current and selected");

        cx.update(|window, app| focus_code(&view, window, app));
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(
            find_query(&view, &cx).as_deref(),
            Some("fo+"),
            "the code's selection is the match the query found, not a new query"
        );
        assert_eq!(found(&view, &cx), (Some(3), Some(1)));
    }

    #[gpui::test]
    async fn a_selection_seeds_a_regular_expression_query_literally(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "f(x) a.b axb\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|_, app| init(app));
        let options = ely_gpui_component::editor::FindOptions {
            regex: true,
            ..Default::default()
        };
        cx.update(|window, app| {
            view.update(app, |view, cx| {
                view.open_find(Some("zzz".into()), options, None, window, cx)
            });
            focus_code(&view, window, app);
            select_code(&view, app, 0..2);
        });
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(
            find_query(&view, &cx).as_deref(),
            Some(r"f\("),
            "the selected `f(` is escaped for a pattern"
        );
        assert_eq!(found(&view, &cx), (Some(1), Some(0)));
        assert_eq!(view.read_with(&cx.cx, |view, _| view.find_error().map(str::to_owned)), None);

        // `a.b` selected: the escaped dot does not match `axb`.
        cx.update(|window, app| {
            focus_code(&view, window, app);
            select_code(&view, app, 5..8);
        });
        cx.simulate_keystrokes(&clipboard_chord("f"));
        cx.run_until_parked();
        assert_eq!(find_query(&view, &cx).as_deref(), Some(r"a\.b"));
        assert_eq!(found(&view, &cx), (Some(1), Some(0)), "`axb` is not a match of `a.b`");
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
            crate::ely::init(cx);
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
        cx.update(|_, app| select_code(&view, app, start..end));

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

    /// The editor's clipboard chords reach the code editor's own keymap and
    /// move the selection through the model: copy writes it, cut takes it out
    /// and paste puts the clipboard back in its place.
    #[gpui::test]
    async fn the_clipboard_chords_copy_cut_and_paste_the_source_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "alpha beta\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            focus_code(&view, window, cx);
            window.simulate_next_frame(cx);
        });

        // Copy: the selection reaches the clipboard, the buffer is untouched.
        cx.update(|_, app| select_code(&view, app, 0..5));
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

    /// Select-all is the editor's secondary chord: ⌘A on macOS, Ctrl-A
    /// elsewhere. The source surface answered Ctrl-A everywhere and ⌘A on
    /// macOS, so a macOS user's Ctrl-A stopped selecting all with the code
    /// editor, which takes Ely's keymap (spec §5.4). The Linux and Windows
    /// platform chord no longer selects all either.
    #[gpui::test]
    async fn select_all_answers_the_secondary_chord(cx: &mut gpui::TestAppContext) {
        const SOURCE: &str = "alpha beta\n";
        let file = TempFile::with_extension("rs", SOURCE);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.update(|window, cx| {
            focus_code(&view, window, cx);
            window.simulate_next_frame(cx);
        });
        let whole = Some(Selection {
            start: 0,
            end: SOURCE.len(),
        });

        cx.simulate_keystrokes(&clipboard_chord("a"));
        cx.run_until_parked();
        assert_eq!(
            view.read_with(&cx.cx, |view, app| view.code_selection(app)),
            whole,
            "the secondary chord must select the whole buffer"
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
        // The keyboard starts in the code, then the reader switches to the
        // preview. The code editor hands the keyboard back to the view, so the
        // paste below reaches neither the code editor nor the buffer.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.set_markdown_mode(MarkdownMode::Code, cx));
            window.simulate_next_frame(cx);
            focus_code(&view, window, cx);
            view.update(cx, |view, cx| view.set_markdown_mode(MarkdownMode::Preview, cx));
            window.simulate_next_frame(cx);
        });
        assert_eq!(
            view.read_with(&cx.cx, |view, _| view.effective_mode()),
            MarkdownMode::Preview,
            "the fixture must actually be showing the preview"
        );
        cx.update(|window, app| {
            assert!(
                view.read(app).editor_focus.is_focused(window),
                "the preview has the keyboard, not the hidden code editor"
            );
        });

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
        cx.update(|_, app| select_code(&view, app, start..end));

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
            focus_code(&view, window, cx);
            window.simulate_next_frame(cx);
        });
        #[cfg(target_os = "macos")]
        cx.simulate_keystrokes("cmd-down");
        #[cfg(not(target_os = "macos"))]
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
            focus_code(&view, window, app);
            window.simulate_next_frame(app);
        });
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

        let code = view
            .read_with(&cx.cx, |view, _| view.code_editor().cloned())
            .expect("the code editor is drawn in Code mode");
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
            assert_eq!(
                code.read_with(&cx.cx, |code, _| code.text().to_owned()),
                current,
                "{selector} shows its change in the code editor"
            );
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

    /// A README rendered in Preview was a picture of text: it could be read
    /// and its links followed, but no sentence of it could be copied. Drives
    /// the real view under the shell-shaped root, with a real drag.
    #[gpui::test]
    async fn text_in_the_markdown_preview_can_be_selected_and_copied(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::text_selection::testing;

        let dir = std::env::temp_dir().join(format!(
            "sirio-file-view-preview-copy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create worktree dir");
        let note = dir.join("note.md");
        std::fs::write(&note, "# Notes\n\nSelect this paragraph.\n").expect("write file");

        cx.update(crate::ely::init);
        let path = note.clone();
        let (view, _, cx) = testing::host(cx, move |_, cx| FileView::new(path, cx));
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
            if view.read_with(&cx.cx, |view, _| view.editor().is_some()) {
                break;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        cx.run_until_parked();

        let scroll = cx
            .debug_bounds("file-markdown-scroll")
            .expect("Preview renders the markdown document");
        let column_left =
            scroll.origin.x + ((scroll.size.width - px(MARKDOWN_COLUMN_WIDTH)) / 2.0).max(px(0.0));
        testing::reset_clipboard(cx);
        testing::drag(
            cx,
            gpui::point(column_left + px(25.0), scroll.origin.y + px(30.0)),
            gpui::point(scroll.right() - px(1.0), scroll.bottom() - px(1.0)),
        );
        cx.simulate_keystrokes("ctrl-c");

        assert_eq!(
            testing::clipboard(cx).as_deref(),
            Some("Notes\n\nSelect this paragraph.")
        );
        let _ = std::fs::remove_dir_all(&dir);
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

    /// Preview keeps its reading column at full width however narrow the
    /// pane gets: the page scrolls sideways instead of rewrapping into a
    /// strip, and the bar that reaches the rest moves only X. A pane wide
    /// enough for the column shows no bar and keeps the column centred.
    #[gpui::test]
    async fn a_narrow_pane_scrolls_the_markdown_preview_sideways_instead_of_rewrapping(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut source = String::from("# Title\n\n");
        for i in 0..200 {
            source.push_str(&format!(
                "Paragraph {i} runs long enough to wrap in a narrow strip of pane.\n\n"
            ));
        }
        let file = TempFile::with_extension("md", &source);
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.simulate_resize(gpui::size(px(400.), px(600.)));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        let viewport = cx
            .debug_bounds("file-markdown-scroll")
            .expect("the Markdown document is drawn in Preview");
        let column = cx
            .debug_bounds("file-markdown-column")
            .expect("the reading column is drawn");
        assert!(viewport.size.width < px(MARKDOWN_COLUMN_WIDTH));
        assert_eq!(
            column.size.width,
            px(MARKDOWN_COLUMN_WIDTH),
            "a narrow pane must not squeeze the column into a strip"
        );
        assert_eq!(
            column.origin.x, viewport.origin.x,
            "an overflowing column starts at the viewport's left edge, \
             never left of it where no scroll offset can reach"
        );

        view.read_with(&cx.cx, |v, _| {
            v.scroll.preview.set_offset(point(px(0.), px(-100.)));
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        let before_y = view.read_with(&cx.cx, |v, _| v.scroll.preview.offset().y);
        assert!(before_y < px(0.), "the document scrolled down first");

        let track = cx
            .debug_bounds("file-markdown-horizontal-bar-track")
            .expect("a column wider than the pane shows a horizontal bar");
        let thumb = cx
            .debug_bounds("file-markdown-horizontal-bar-thumb")
            .expect("the horizontal bar has a thumb");
        let pane_bottom = cx.update(|window, _| window.viewport_size().height);
        assert!(
            viewport.bottom() <= pane_bottom && track.bottom() <= pane_bottom,
            "the Markdown bar above leaves the surface less than the pane's \
             height; neither the document nor its bar may hang below it"
        );
        let end = gpui::point(track.right() + px(50.), thumb.center().y);
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
            let handle = &v.scroll.preview;
            assert_eq!(
                handle.offset().x,
                -handle.max_offset().x,
                "dragging to the end reaches the column's right edge"
            );
            assert!(handle.max_offset().x > px(0.));
            assert_eq!(handle.offset().y, before_y, "a sideways drag keeps Y");
        });

        cx.simulate_resize(gpui::size(px(1600.), px(600.)));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-horizontal-bar-track")
                .is_none(),
            "a pane wide enough for the column shows no horizontal bar"
        );
        let viewport = cx.debug_bounds("file-markdown-scroll").unwrap();
        let column = cx.debug_bounds("file-markdown-column").unwrap();
        assert_eq!(column.size.width, px(MARKDOWN_COLUMN_WIDTH));
        assert_eq!(
            column.origin.x - viewport.origin.x,
            (viewport.size.width - px(MARKDOWN_COLUMN_WIDTH)) / 2.0,
            "a wide pane centres the column, with no leftover sideways offset"
        );
    }

    use crate::markdown_preview::{DiagramSettings, PlantUmlQueue};
    use base64::Engine as _;
    use bezel_markdown::BlockKind;

    fn diagram_cache(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sirio-diagram-cache-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Drives frames and the background executor until no diagram fence is
    /// left in the Preview, and returns the last document built.
    fn settle_preview(cx: &mut VisualTestContext, view: &gpui::Entity<FileView>) -> bezel_markdown::Doc {
        let mut last = None;
        for _ in 0..200 {
            cx.run_until_parked();
            cx.update(|window, cx| window.simulate_next_frame(cx));
            let doc = cx
                .update(|_, cx| view.update(cx, |view, cx| view.preview_document(cx)))
                .expect("a Markdown file in Preview");
            let fenced = doc.blocks.iter().any(|block| {
                matches!(&block.kind, BlockKind::Code { language: Some(language), .. }
                    if sirio_diagram::DiagramKind::from_fence(language).is_some())
            });
            last = Some(doc);
            if !fenced {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        last.expect("at least one frame")
    }

    #[gpui::test]
    async fn a_mermaid_fence_renders_into_the_cache_and_the_preview(cx: &mut gpui::TestAppContext) {
        let cache = diagram_cache("one");
        cx.update(|cx| {
            DiagramSettings::set(
                DiagramSettings { cache_dir: cache.clone(), plantuml_server: None },
                cx,
            )
        });
        let file = TempFile::with_extension(
            "md",
            "# D\n\n```mermaid\nflowchart TD\n  A --> B\n```\n",
        );
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let doc = settle_preview(&mut cx, &view);
        let image = doc
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                BlockKind::Image { url, width, .. } => Some((url.clone(), *width)),
                _ => None,
            })
            .expect("the fence became an image");
        assert!(Path::new(&image.0).starts_with(&cache), "drawn from the diagram cache");
        assert!(Path::new(&image.0).exists());
        assert!(image.1.is_some_and(|width| width > 0));
        let _ = std::fs::remove_dir_all(&cache);
    }

    /// Both fences render as images; the dedupe itself is guarded by the
    /// `the_same_diagram_twice_is_requested_once` unit test in markdown_preview.
    #[gpui::test]
    async fn the_same_diagram_twice_renders_once_and_shows_twice(cx: &mut gpui::TestAppContext) {
        let cache = diagram_cache("twice");
        cx.update(|cx| {
            DiagramSettings::set(
                DiagramSettings { cache_dir: cache.clone(), plantuml_server: None },
                cx,
            )
        });
        let fence = "```mermaid\nflowchart TD\n  A --> B\n```\n";
        let file = TempFile::with_extension("md", &format!("{fence}\n{fence}"));
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let doc = settle_preview(&mut cx, &view);
        let images = doc
            .blocks
            .iter()
            .filter(|block| matches!(block.kind, BlockKind::Image { .. }))
            .count();
        assert_eq!(images, 2);
        let files = std::fs::read_dir(&cache)
            .expect("cache dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "svg"))
            .count();
        assert_eq!(files, 1, "rendered once");
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[gpui::test]
    async fn a_queued_plantuml_render_is_dropped_when_settings_change(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::io::ErrorKind;
        let old_cache = diagram_cache("queued-old");
        let new_cache = diagram_cache("queued-new");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let server = format!("http://{}", listener.local_addr().expect("addr"));
        cx.update(|app| {
            DiagramSettings::set(
                DiagramSettings {
                    cache_dir: old_cache.clone(),
                    plantuml_server: Some(server),
                },
                app,
            );
        });
        let queue = cx.update(PlantUmlQueue::get);
        let turn = queue.turn().await;
        let file = TempFile::with_extension("md", "```plantuml\nAlice -> Bob: test\n```\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.cx.run_until_parked();
        let key = cx.update(|_, app| {
            let palette = markdown_preview::palette(Theme::get(app));
            sirio_diagram::cache_key(DiagramKind::PlantUml, "Alice -> Bob: test", &palette)
        });
        cx.update(|_, app| {
            DiagramSettings::set(
                DiagramSettings {
                    cache_dir: new_cache,
                    plantuml_server: None,
                },
                app,
            );
        });
        drop(turn);
        for _ in 0..30 {
            cx.cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(10));
            match listener.accept() {
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Ok(_) => panic!("the queued render used the withdrawn PlantUML server"),
                Err(error) => panic!("listener failed: {error}"),
            }
        }
        let state = view.read_with(&cx.cx, |view, _| view.diagrams.get(&key).cloned());
        assert!(!matches!(state, Some(DiagramState::Pending)), "stale Pending state remains: {state:?}");
        let _ = std::fs::remove_dir_all(old_cache);
        let _ = std::fs::remove_dir_all(diagram_cache("queued-new"));
    }

    #[gpui::test]
    async fn without_diagram_settings_a_fence_stays_code(cx: &mut gpui::TestAppContext) {
        let file = TempFile::with_extension("md", "```mermaid\nflowchart TD\n  A --> B\n```\n");
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        cx.run_until_parked();
        let doc = cx
            .update(|_, cx| view.update(cx, |view, cx| view.preview_document(cx)))
            .expect("Preview");
        assert!(matches!(&doc.blocks[0].kind, BlockKind::Code { .. }));
    }

    /// Design §8: a drawn frame with a local image.
    #[gpui::test]
    async fn a_lone_local_image_is_a_picture_in_the_drawn_preview(cx: &mut gpui::TestAppContext) {
        let picture = std::env::temp_dir().join(format!(
            "sirio-preview-logo-{}.png",
            std::process::id()
        ));
        // A 1×1 PNG, split so no line runs past 100 columns.
        let pixel = concat!(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk",
            "YPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
        );
        std::fs::write(
            &picture,
            base64::engine::general_purpose::STANDARD
                .decode(pixel)
                .expect("a 1×1 PNG"),
        )
        .expect("write png");
        let name = picture.file_name().expect("name").to_string_lossy().into_owned();
        let file = TempFile::with_extension("md", &format!("![Logo]({name})\n"));
        let (mut cx, view) = mounted_file_view(cx, file.path().to_path_buf());
        let doc = settle_preview(&mut cx, &view);
        let is_picture = matches!(
            &doc.blocks[0].kind,
            BlockKind::Image { url, .. } if Path::new(url) == picture
        );
        let drawn = cx.debug_bounds("file-markdown-scroll").is_some();
        let _ = std::fs::remove_file(&picture);
        assert!(is_picture);
        assert!(drawn, "the Preview was drawn");
    }

    fn snapshot_origin_for_tests() -> SnapshotOrigin {
        SnapshotOrigin {
            reference: sirio_forge::ChangeRef {
                forge: sirio_forge::Forge::GitHub,
                host: "ghe.test".to_string(),
                project: "acme/widgets".to_string(),
                number: 578,
            },
            sha: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".to_string(),
            relative_path: PathBuf::from("src/lib.rs"),
            local_copy: None,
            deleted: false,
        }
    }

    /// A snapshot's revision is drawn read-only: its text is on screen, and
    /// typing, a paste, the view's own edits (a replacement, a Markdown format,
    /// a save) leave it as it was and clean. An `Unlock` the code editor
    /// announces (the one word it has for "Make editable") is taken back before
    /// the next key arrives, so a key after it changes nothing either.
    #[gpui::test]
    async fn a_snapshot_shows_the_revisions_text_and_refuses_every_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        use ely_gpui_component::editor::EditorEvent;

        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            FileView::snapshot(
                snapshot_origin_for_tests(),
                b"hello\nworld\n".to_vec(),
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let view = cx.update(|window, _| window.root::<FileView>().flatten().expect("root"));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let code = view
            .read_with(&cx.cx, |view, _| view.code_editor().cloned())
            .expect("the snapshot's code editor is drawn");
        assert!(
            cx.debug_bounds("code-editor-line-0").is_some(),
            "the revision's text is on screen"
        );
        assert!(
            code.read_with(&cx.cx, |code, _| code.is_read_only()),
            "the code editor is drawn read-only"
        );

        cx.update(|window, cx| focus_code(&view, window, cx));
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("intruder".to_string()));
        });
        cx.simulate_input("X");
        cx.simulate_keystrokes(&clipboard_chord("v"));
        cx.run_until_parked();
        assert_eq!(
            buffer_of(&view, &cx),
            "hello\nworld\n",
            "typing and pasting leave a snapshot alone"
        );
        assert_eq!(
            code.read_with(&cx.cx, |code, _| code.text().to_owned()),
            "hello\nworld\n",
            "and the code editor still shows the revision"
        );

        // What Ely's "Make editable" did (it is not drawn now): unlock, then announce it.
        code.update(&mut cx.cx, |code, cx| {
            code.set_read_only(false, cx);
            cx.emit(EditorEvent::Unlock);
        });
        cx.run_until_parked();
        assert!(
            code.read_with(&cx.cx, |code, _| code.is_read_only()),
            "a snapshot takes its read-only lock back when it is unlocked"
        );
        cx.simulate_input("Y");
        cx.simulate_keystrokes(&clipboard_chord("v"));
        cx.run_until_parked();
        assert_eq!(
            buffer_of(&view, &cx),
            "hello\nworld\n",
            "an unlock does not let a snapshot be edited"
        );

        view.update(&mut cx.cx, |view, cx| {
            assert!(view.is_snapshot());
            assert_eq!(view.editor().expect("loaded").buffer(), "hello\nworld\n");
            assert_eq!(
                view.snapshot_origin().expect("origin").label(),
                "#578 at a1b2c3d"
            );
            view.replace_code_selection("X", cx);
            view.apply_markdown_format(MarkdownFormatOp::Bold, cx);
            view.replace_code_selection("", cx);
            assert_eq!(view.editor().expect("loaded").buffer(), "hello\nworld\n", "nothing edits a snapshot");
            assert!(!view.is_dirty());
            let saved = view.save(cx);
            assert!(saved.is_err(), "a snapshot has no file to save to");
            assert!(view.message_text().is_some_and(|text| text.contains("Read-only")));
        });
    }

    #[gpui::test]
    async fn a_snapshots_path_is_never_the_local_files_path(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let origin = snapshot_origin_for_tests();
        let view = cx.new(|cx| FileView::snapshot(origin.clone(), b"x\n".to_vec(), cx));
        view.read_with(cx, |view, _| {
            assert_ne!(view.path(), origin.relative_path.as_path());
            assert!(view.path().to_string_lossy().ends_with("src/lib.rs"), "the extension still picks the language");
        });
    }

    /// The origin is rebuilt from the session store on restore, so its path
    /// and sha are shown before they are trusted: a non-ASCII sha must not
    /// panic the bar, and no relative path may turn the virtual path real.
    #[test]
    fn a_snapshots_origin_survives_a_corrupt_sha_and_never_yields_a_real_path() {
        let mut origin = snapshot_origin_for_tests();
        origin.sha = "abcdef€ghij".to_string();
        assert_eq!(origin.label(), "#578 at abcdef€");
        origin.sha = String::new();
        assert_eq!(origin.label(), "#578", "a refused sha is not shown as ` at `");

        let sha = snapshot_origin_for_tests().sha;
        for (saved, expected) in [
            ("/etc/passwd", "etc/passwd"),
            ("../x/../../y.rs", "x/y.rs"),
            ("./src/lib.rs", "src/lib.rs"),
            ("src/lib.rs", "src/lib.rs"),
        ] {
            let mut origin = snapshot_origin_for_tests();
            origin.relative_path = PathBuf::from(saved);
            let virtual_path = origin.virtual_path();
            assert!(!virtual_path.is_absolute(), "{saved}: {}", virtual_path.display());
            assert_eq!(
                virtual_path,
                Path::new("sirio-snapshot").join(&sha).join(expected),
                "{saved}"
            );
        }
    }

    /// A snapshot whose path was refused (saved, or clicked live) has no file
    /// name to say: the sentence must still read as one, and must not claim a
    /// restore that never happened.
    #[gpui::test]
    async fn a_refused_snapshot_reads_as_a_sentence_without_a_file_name(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let mut origin = snapshot_origin_for_tests();
        origin.relative_path = PathBuf::new();
        let view = cx.new(|cx| FileView::snapshot_pending(origin, cx));
        view.update(cx, |view, cx| {
            view.finish_snapshot(Err("The saved file path is not valid.".to_string()), cx);
            assert_eq!(
                view.snapshot_error(),
                Some("Could not open the snapshot of #578 at a1b2c3d: The saved file path is not valid.")
            );
        });
    }

    /// A snapshot opened with a line whose first read failed: the reveal
    /// is held through the failure and applied when the Retry succeeds.
    #[gpui::test]
    async fn a_reveal_asked_before_a_snapshots_retry_succeeded_still_happens(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            let mut view = FileView::snapshot_pending(snapshot_origin_for_tests(), cx);
            view.reveal(RevealTarget::Line(350), cx);
            view.finish_snapshot(Err("boom".to_string()), cx);
            view
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let view = cx.update(|window, _| window.root::<FileView>().flatten().expect("root"));
        let body = (0..400).map(|n| format!("line {n}\n")).collect::<String>();
        view.update(&mut cx.cx, |view, cx| {
            view.restart_snapshot(cx);
            view.finish_snapshot(Ok(body.into_bytes()), cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("code-editor-line-350").is_some(),
            "the held reveal scrolls the line into view once the retry lands"
        );
    }

    #[gpui::test]
    async fn binary_and_oversized_bytes_show_the_editors_own_state_not_a_crash(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let binary = cx.new(|cx| FileView::snapshot(snapshot_origin_for_tests(), vec![0, 1, 2, 0], cx));
        let huge = cx.new(|cx| {
            FileView::snapshot(snapshot_origin_for_tests(), vec![b'a'; crate::editor::MAX_FILE_BYTES as usize + 1], cx)
        });
        for view in [binary, huge] {
            view.read_with(cx, |view, _| {
                let editor = view.editor().expect("an editor in a refusing state");
                assert!(editor.buffer().is_empty());
                assert!(editor.load_message().is_some(), "the surface says why there is no text");
            });
        }
    }

    #[gpui::test]
    async fn a_pending_snapshot_fills_in_or_offers_a_retry(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let filled = cx.new(|cx| FileView::snapshot_pending(snapshot_origin_for_tests(), cx));
        filled.update(cx, |view, cx| {
            assert!(view.editor().is_none(), "nothing to show yet");
            view.finish_snapshot(Ok(b"late\n".to_vec()), cx);
            assert_eq!(view.editor().expect("loaded").buffer(), "late\n");
        });
        let failed = cx.new(|cx| FileView::snapshot_pending(snapshot_origin_for_tests(), cx));
        failed.update(cx, |view, cx| {
            view.finish_snapshot(Err("boom".to_string()), cx);
            assert!(view
                .snapshot_error()
                .is_some_and(|text| text.contains("boom") && text.contains("#578 at a1b2c3d")));
            view.restart_snapshot(cx);
            assert!(view.editor().is_none(), "a retry starts from nothing");
            assert!(view.snapshot_error().is_none(), "and the old failure is gone");
            view.finish_snapshot(Err("boom".to_string()), cx);
            view.finish_snapshot(Ok(b"late\n".to_vec()), cx);
            assert!(view.snapshot_error().is_none(), "a read that succeeds clears it");
        });
    }

    #[gpui::test]
    async fn a_failed_snapshot_keeps_its_retry_after_every_card_is_gone(
        cx: &mut gpui::TestAppContext,
    ) {
        // Spec §8: an unreachable tab keeps its error with Retry. The two
        // ways a transient card disappears — a click on its body, and any
        // later message such as the Read-only answer to Ctrl+S — must not
        // take the only remedy with them.
        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
        let window =
            cx.add_window(|_window, cx| FileView::snapshot_pending(snapshot_origin_for_tests(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let view = cx.update(|window, _| window.root::<FileView>().flatten().expect("root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &FileViewEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
        });

        view.update(&mut cx.cx, |view, cx| {
            view.finish_snapshot(Err("boom".to_string()), cx);
            view.dismiss_message(cx);
            let saved = view.save(cx);
            assert!(saved.is_err(), "a snapshot still refuses to save");
            view.dismiss_message(cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-snapshot-failed").is_some(),
            "the failure is still drawn"
        );
        assert!(
            cx.debug_bounds("file-loading").is_none(),
            "and the tab does not claim to be loading"
        );
        let retry = cx
            .debug_bounds("file-snapshot-retry")
            .expect("the Retry survives every card");
        cx.simulate_click(retry.center(), Modifiers::none());
        assert_eq!(
            events.borrow().as_slice(),
            &[FileViewEvent::RetrySnapshot],
            "and a click on it asks for the read again"
        );
    }
}

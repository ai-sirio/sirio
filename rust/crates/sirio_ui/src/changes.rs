//! The full-width Changes tab: git status and unified diffs.
//!
//! This surface used to live inside the right inspector, where a 405px
//! column could not show a line of code. It is now a first-class tab (a
//! peer of Chat and Terminal, per orca's placement): the diff gets the
//! width it needs, and the inspector keeps Files.
//!
//! The data model is local to this surface. Git operations are delegated to
//! `sirio_git`; the host application can later replace the refresh
//! callbacks with its project store without changing the row layout.
//!
//! Diffs are fetched with [`CHANGES_CONTEXT_LINES`] of context instead of
//! `sirio_git::DEFAULT_CONTEXT_LINES`: with 3 lines of context a collapsed
//! band can only appear between changes 4–6 lines apart and is nearly
//! invisible; with a generous window the bands — "N hidden lines", the
//! orca grammar — stop being an edge case and become the primary diff
//! structure.
//!
//! # Honesty contract (P34-routed critic findings)
//!
//! Every state this surface renders must be distinguishable from failure.
//! Three defects shipped here because they were not:
//!
//! - `load_snapshot` swallowed every git error and rendered a clean 0/0/0;
//!   now it returns `Result` and the error reaches the surface with git's
//!   own message plus a Retry that re-runs the refresh (F-CHG-09's error
//!   path was dead code). A missing `git` binary reads as a missing binary
//!   ("failed to spawn git: …"), not as an unborn-HEAD repo.
//! - a staged file in an unborn-HEAD repo showed +0 −0 while its expanded
//!   diff showed real lines — the counts now come from the same diff the
//!   expansion renders (`sirio_git::stats`), so they cannot disagree;
//! - a file whose diff cannot be fetched renders an explicit "diff
//!   unavailable" row when expanded, never empty rows next to real +/−
//!   counts, and a row whose counts are unknown shows `·` rather than a
//!   confident +0 −0.
//!
//! The one state left ambiguous on purpose: the control-socket report keeps
//! `usize` counts and defaults an uncounted file to zero — automation wants
//! numbers, and the report's `error` field covers the git-broken case. The
//! human surface is the honest one.

use gpui::{
    AnyElement, AnyView, App, AppContext, ClipboardItem, Context, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement, KeyDownEvent, ListAlignment, ListSizingBehavior, ListState, Pixels,
    HighlightStyle, Hsla, Render, SharedString, Task, Window, canvas, div, list, prelude::*, px,
};
use sirio_git::{
    DiffLine, DiffOrigin, DiffSideBySideLine, DiffSideBySideRow, DiffStat, FileDiff,
    GitDiffSideBySide, GitError, StatusEntry, StatusKind, StatusSnapshot, commit_diff_entry,
    commit_files, diff_entry, discard, range_file_diff, range_files, range_stats,
    stage, stage_all, stats, status, unstage,
};
use sirio_theme::Theme;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton, ToggleGroup, ToggleItem},
    data_display::CountBadge,
    feedback::Callout,
    menus::{ContextMenu, Menu, MenuItem},
    motion::Skeleton,
    overlays::Dialog,
    git::{DiffStat as EDiffStat, GitStatus, GitStatusBadge},
    primitives::{Icon as EIcon, IconName, Severity},
    theme::{ControlSize, IconSize as EIconSize},
};
use crate::horizontal_scroll::{self, HorizontalBarState};
use crate::diff_annotations::{Annotation, AnnotationKind, AnnotationSide, anchored_lines, band_pieces, matches_row};
use crate::text_selection::{SelectableText, selectable_text};

#[cfg(test)]
mod perf_baseline;

/// Context lines fetched for each change. Generous enough that the
/// collapsed-context bands carry real counts ("27 hidden lines"), cheap
/// enough to re-fetch on the refresh interval.
pub const CHANGES_CONTEXT_LINES: usize = 24;

/// How often the git snapshot re-polls after an external edit.
const CHANGES_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
/// How many consecutive undrawn ticks may be skipped before one refresh
/// runs anyway (#193). At the interval above this bounds staleness for a
/// live-but-undrawn surface at ten seconds, while still removing nine of
/// every ten reloads from a tab nobody is looking at.
pub(crate) const SUSPENDED_TICK_BUDGET: u32 = 10;
const TOOLBAR_HEIGHT: f32 = 34.0;
/// File headers: sidebar-family 12px UI text at the app's 30px row
/// rhythm — the rows and their action buttons read like the rest of the
/// chrome; the mono face is reserved for the diff code below.
pub(crate) const ROW_HEIGHT: f32 = 30.0;
pub(crate) const HUNK_ROW_HEIGHT: f32 = 24.0;
/// Diff code lines: gallery 12px mono on an 18px line at 20px overall.
pub(crate) const DIFF_LINE_HEIGHT: f32 = 20.0;
const BAND_ROW_HEIGHT: f32 = 24.0;
/// Five 12px Geist Mono digits need 36px; the extra room avoids clipping
/// when the resolved monospace fallback is fractionally wider.
const DIFF_NUMBER_WIDTH: f32 = 40.0;
/// Width of the gallery diff's `+` / `-` marker column.
const DIFF_SIGN_WIDTH: f32 = 12.0;
const DIFF_ROW_GAP: f32 = 8.0;
const DIFF_ROW_PADDING: f32 = 10.0;
/// A line's wash and its changed words' wash, as Ely's `DiffViewer` draws
/// them from the palette.
const LINE_WASH: f32 = 0.08;
const WORD_WASH: f32 = 0.25;

fn clamped_x(x: Pixels, content: Pixels, viewport: Pixels) -> Pixels {
    x.clamp(-(content - viewport).max(px(0.0)), px(0.0))
}
const DIFF_HUNK_GUTTER_WIDTH: f32 = DIFF_NUMBER_WIDTH * 2.0 + DIFF_SIGN_WIDTH + 16.0;
/// A run of unchanged context lines this long collapses into one labelled
/// band (orca's "18 hidden lines").
const CONTEXT_BAND_MIN: usize = 4;
/// The 1px rule between the two split columns.
///
/// It is the only geometry constant this rendering needs, and that is the
/// point. An earlier version of this file sized the split columns to the
/// *content* — reserving `chars * advance` for the widest expanded line and
/// scrolling the list horizontally to reach it. The macOS original tried
/// that too and abandoned it, and `SideBySideDiffLayout.columnWidth`
/// (`DiffContentAdapter.swift:246`) records why in its own words: "Sizing to
/// the content is what put the right-hand column past the right edge of the
/// pane: one long line in the file was enough to leave a side-by-side view
/// with only one visible side. Long lines are clipped instead."
///
/// Driven here, it was worse than that. On the 1715×972 lane, choosing Split
/// and then Expand All over a file with a 276-character line widened the
/// whole surface until the Files panel was off the right edge of the window
/// *and took the Changes toolbar's own action cluster with it*, leaving no
/// Unified segment on screen to escape with
/// (`/tmp/n1-split/10-09-split-settled.png`). Adding `min_w(px(0.0))` to the
/// scroll container — the CSS answer — got the rows laying out but did not
/// stop the surface growing (`/tmp/n1-fix/07-06-split-settled.png`).
///
/// So the columns are each exactly half the pane, `flex_1` against whatever
/// width the surface was given, and a line longer than its column ends in an
/// ellipsis — the same trade the macOS build settled on. Making a long line
/// *reachable* rather than clipped needs a per-column horizontal scroll (what
/// VS Code and GitHub do), which needs the two columns to be two stacks
/// rather than two halves of each row; that is a different shape of code,
/// not a constant.
const SPLIT_DIVIDER_WIDTH: f32 = 1.0;
/// How far past the viewport the virtualized list lays rows out, so a wheel
/// tick never scrolls into rows that have not been drawn yet. The same
/// figure the chat transcript's list uses.
const LIST_OVERDRAW: f32 = 2048.0;

fn new_list_state() -> ListState {
    ListState::new(0, ListAlignment::Top, px(LIST_OVERDRAW))
}

/// How an expanded file's diff is drawn.
///
/// What the macOS original actually did, read rather than assumed:
/// `ChangesListView` was a list of *file rows only* — it rendered no diff
/// lines at all — and its per-file `Open diff` opened a **separate tab**
/// (`App/Workspace/DiffContentAdapter.swift:52`) whose one and only
/// renderer was `SideBySideDiffView`. `GitDiffSideBySide` had exactly one
/// caller in the whole app (`DiffContentAdapter.swift:283`).
///
/// This port made two changes to that, and the second one is the bug. It
/// gave the Changes surface inline expandable diffs (orca's idea, not
/// macOS's — `04-ux-patterns-waku-does-not-cover.md`), and it routed
/// `OpenDiff` to *the same Changes surface* rather than to a diff tab
/// (`main.rs` `add_changes_tab(Some(path))`). Folding the second surface
/// into the first left the side-by-side rendering with no door anywhere in
/// the app.
///
/// So the door is a mode on the surface that swallowed it, not a
/// resurrected second surface — see [`ChangesTab::render_toolbar`] for
/// where the control lives and why.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffViewMode {
    /// One column, deletions and additions interleaved in file order.
    #[default]
    Unified,
    /// Two columns: the old file on the left, the new one on the right,
    /// context paired across both and deletion/addition runs zipped
    /// (`sirio_git::GitDiffSideBySide`).
    Split,
}

/// The app-wide diff view mode. A GPUI global rather than per-tab state on
/// purpose: `main.rs` rebuilds every `ChangesTab` when a worktree rebinds
/// (`rebind_changes_tabs`), and a per-tab field would silently snap back to
/// Unified every time — a preference that forgets itself is worse than no
/// preference. See the report for what a *durable* (across relaunch)
/// preference additionally needs, which lives outside this crate.
struct DiffViewModeSetting(DiffViewMode);

impl gpui::Global for DiffViewModeSetting {}

impl DiffViewMode {
    /// The current choice. Defaults to `Unified` when nothing has set it,
    /// so a test (or a first launch) never has to install the global.
    pub fn get(cx: &App) -> Self {
        if cx.has_global::<DiffViewModeSetting>() {
            cx.global::<DiffViewModeSetting>().0
        } else {
            Self::default()
        }
    }

    /// Records the choice for every Changes surface in the app.
    pub fn set(mode: Self, cx: &mut App) {
        cx.set_global(DiffViewModeSetting(mode));
    }

    /// The mode as the control socket names it.
    pub fn name(self) -> &'static str {
        match self {
            DiffViewMode::Unified => "unified",
            DiffViewMode::Split => "split",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "unified" => Some(DiffViewMode::Unified),
            "split" => Some(DiffViewMode::Split),
            _ => None,
        }
    }
}

/// Events emitted to the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangesTabEvent {
    /// Open a file from the diff's per-file action row in a new tab.
    OpenFile(PathBuf),
}

/// Actions that need a host-owned surface beyond the existing file-open door.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangesTabActionEvent {
    /// Ask the host to open the path in a dedicated Diff tab.
    OpenDiff(PathBuf),
    /// Ask the host to create/focus a terminal prepared for this conflict.
    ResolveInTerminal(PathBuf),
}

/// The cross-crate GPUI drag payload: repo-relative path plus unified text.
/// The terminal crate consumes this same structural payload without a
/// dependency back on the UI crate.
type DiffPayload = (PathBuf, String);

/// The file-level facts the Changes surface displays for one status bucket.
/// This is intentionally UI-owned data: the control socket reads this report
/// from the mounted `ChangesTab` rather than running a second git query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesFileReport {
    pub path: PathBuf,
    pub additions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

/// One of the three status buckets shown by the Changes surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesSectionReport {
    pub name: &'static str,
    pub count: usize,
    pub files: Vec<ChangesFileReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationReport {
    pub key: u64,
    pub path: PathBuf,
    pub placed: &'static str,
}

/// The live state currently held by one mounted Changes surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesReport {
    pub repo_root: PathBuf,
    pub sections: Vec<ChangesSectionReport>,
    pub annotations: Vec<AnnotationReport>,
    pub loading: bool,
    pub error: Option<String>,
    /// The Discard confirmation open over the surface, if any:
    /// `discard:<path>` or `discard-all`.
    pub dialog: Option<String>,
}

/// The three buckets `git status` reports, in display order. A file can
/// belong to Staged and Changed at once (staged, then modified again) —
/// porcelain's two status columns, kept apart here on purpose: collapsing
/// them into one row throws away the distinction the whole surface exists
/// to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ChangeSection {
    Staged,
    Changed,
    Untracked,
}

impl ChangeSection {
    /// Display order of the sections.
    const ORDER: [ChangeSection; 3] = [
        ChangeSection::Staged,
        ChangeSection::Changed,
        ChangeSection::Untracked,
    ];

    fn label(self) -> &'static str {
        match self {
            ChangeSection::Staged => "Staged",
            ChangeSection::Changed => "Changed",
            ChangeSection::Untracked => "Untracked",
        }
    }

    /// The heading this section wears on `source`: a change request's diff
    /// is not "staged" anything (B1 §13), so its one section reads Changes.
    /// A commit keeps Staged; §8 names only the Range.
    fn label_in(self, source: &ChangesSource) -> &'static str {
        match (self, source) {
            (ChangeSection::Staged, ChangesSource::Range { .. }) => "Changes",
            _ => self.label(),
        }
    }

    /// Lowercase label, used in element ids so a path that appears in two
    /// sections still gets unique ids.
    fn slug(self) -> &'static str {
        match self {
            ChangeSection::Staged => "staged",
            ChangeSection::Changed => "changed",
            ChangeSection::Untracked => "untracked",
        }
    }

    /// The action a row in this section offers. A Staged row acts on the
    /// index side (Unstage); Changed and Untracked rows act on the worktree
    /// side (Stage).
    fn action_label(self) -> &'static str {
        match self {
            ChangeSection::Staged => "Unstage",
            ChangeSection::Changed | ChangeSection::Untracked => "Stage",
        }
    }

    fn batch_action_label(self) -> &'static str {
        match self {
            ChangeSection::Staged => "Unstage all",
            ChangeSection::Changed | ChangeSection::Untracked => "Stage all",
        }
    }
}

/// One section of the changes list: its header metadata and the rows under
/// it. A section that is empty is omitted entirely; a collapsed one keeps
/// its header but carries no rows.
struct SectionRows {
    section: ChangeSection,
    /// Number of files in this section (a staged-and-modified file counts
    /// once per section it appears in).
    count: usize,
    collapsed: bool,
    rows: Vec<ChangeRow>,
}

/// One row of the changes list. Every variant carries the section it lives
/// in, so ids stay unique when one path appears in two sections and so the
/// Stage/Unstage action acts on the right side of the split.
#[derive(Clone, Debug)]
enum ChangeRow {
    Annotation {
        section: ChangeSection,
        path: PathBuf,
        key: u64,
        revision: u64,
    },
    File {
        section: ChangeSection,
        entry: StatusEntry,
        /// `None` when the counts are unknown; the row renders `·` instead
        /// of a confident +0 −0.
        stat: Option<DiffStat>,
        /// The parsed textual diff carried by this row's drag source.
        drag_payload: Option<DiffPayload>,
        expanded: bool,
    },
    Hunk {
        section: ChangeSection,
        path: PathBuf,
        header: String,
    },
    /// A collapsed run of unchanged context lines, labelled with its own
    /// size (orca's "18 hidden lines"): clicking expands it in place.
    ContextBand {
        section: ChangeSection,
        path: PathBuf,
        /// The run's position in the file's flattened line stream — a
        /// stable key for the expansion set within one snapshot.
        key: usize,
        count: usize,
        expanded: bool,
    },
    Line {
        section: ChangeSection,
        path: PathBuf,
        line: DiffLine,
        /// The stretch of the line that changed against its partner in the
        /// run, washed darker (`segment_words`).
        words: Option<Range<usize>>,
    },
    /// One row of the side-by-side rendering: old on the left, new on the
    /// right, either side possibly absent where a run was longer than its
    /// partner. Produced by `sirio_git::GitDiffSideBySide` — the pairing,
    /// the zipping and the padding are the model's, never this file's.
    SplitLine {
        section: ChangeSection,
        path: PathBuf,
        /// Position of this row in the file's split-row stream, for a
        /// unique element id.
        key: usize,
        row: DiffSideBySideRow,
        /// The changed stretch on the left and on the right of a zipped
        /// replacement (`changed_span`).
        words: (Option<Range<usize>>, Option<Range<usize>>),
    },
    /// A file whose diff could not be fetched. Rendered as an explicit
    /// "diff unavailable" row when expanded, so an empty expansion can
    /// never be mistaken for "no changes".
    Unavailable {
        section: ChangeSection,
        path: PathBuf,
        message: String,
        /// Git failed, so Retry can help; a binary file did not fail.
        failed: bool,
    },
}

/// One item of the virtualized list: a section header or a change row.
/// Flattened from [`SectionRows`] once per frame, because `gpui::list`
/// asks for items by index.
enum ListRow {
    Header {
        section: ChangeSection,
        count: usize,
        collapsed: bool,
    },
    Change(ChangeRow),
}

impl ListRow {
    /// Hashes what decides this row's identity and height, never its text.
    /// The list is re-spliced when the fingerprint of the whole stream
    /// changes; `ListState::splice` keeps the logical scroll top (an item
    /// index plus an offset inside it), so the reader stays put across an
    /// expand below the viewport or a refresh that re-reads the same diff.
    fn hash_identity<H: Hasher>(&self, state: &mut H) {
        match self {
            ListRow::Header {
                section, collapsed, ..
            } => {
                0u8.hash(state);
                section.hash(state);
                collapsed.hash(state);
            }
            ListRow::Change(row) => match row {
                ChangeRow::File {
                    section,
                    entry,
                    expanded,
                    ..
                } => {
                    1u8.hash(state);
                    section.hash(state);
                    entry.path.hash(state);
                    expanded.hash(state);
                }
                ChangeRow::Hunk {
                    section,
                    path,
                    header,
                } => {
                    2u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                    header.hash(state);
                }
                ChangeRow::ContextBand {
                    section,
                    path,
                    key,
                    expanded,
                    ..
                } => {
                    3u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                    key.hash(state);
                    expanded.hash(state);
                }
                ChangeRow::Line {
                    section,
                    path,
                    line,
                    ..
                } => {
                    4u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                    line.old_line_number.hash(state);
                    line.new_line_number.hash(state);
                }
                ChangeRow::SplitLine {
                    section, path, key, ..
                } => {
                    5u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                    key.hash(state);
                }
                ChangeRow::Unavailable { section, path, .. } => {
                    6u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                }
                ChangeRow::Annotation { section, path, key, revision } => {
                    7u8.hash(state);
                    section.hash(state);
                    path.hash(state);
                    key.hash(state);
                    revision.hash(state);
                }
            },
        }
    }
}

#[derive(Debug, Default)]
struct GitSnapshot {
    entries: Vec<StatusEntry>,
    diffs: HashMap<PathBuf, FileDiff>,
    stats: HashMap<PathBuf, DiffStat>,
    /// Paths whose diff could not be fetched, with git's own message. Their
    /// expanded rows say "diff unavailable" instead of rendering nothing
    /// next to real +/− counts — an empty expansion could mean "no
    /// changes", which would be a lie.
    diff_errors: HashMap<PathBuf, String>,
}

/// What a Changes surface is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ChangesSource {
    /// The working tree: `git status` plus per-entry diffs, mutable.
    WorkingTree,
    /// One commit, by sha. Immutable: stage/unstage/discard are refused.
    Commit(String),
    /// A change request's diff: `base...head`, read from objects the host
    /// made local. Immutable like a commit and never polled — a revision does
    /// not change. Files and counts load up front; one file's diff loads when
    /// its row opens, because a whole-request patch can exceed the git
    /// runner's output cap.
    Range { base: String, head: String },
}

type GitOperation = Box<dyn FnOnce(&Path) -> Result<(), GitError> + Send + 'static>;

/// Where a change request's files live on its forge, for a row's *Open on
/// the forge*. Only a Range surface has one; the change request tab sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFiles {
    pub forge: sirio_forge::Forge,
    pub web_url: String,
}

/// What an open Discard confirmation would throw away.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DiscardAsk {
    One(PathBuf),
    All(Vec<PathBuf>),
}

/// The full-width git changes surface.
pub struct ChangesTab {
    repo_root: PathBuf,
    is_git: bool,
    /// What this surface reads and whether it may mutate it.
    source: ChangesSource,
    entries: Vec<StatusEntry>,
    diffs: HashMap<PathBuf, FileDiff>,
    annotations: Vec<Annotation>,
    annotation_views: Rc<HashMap<u64, AnyView>>,
    open_threads: Rc<HashMap<PathBuf, usize>>,
    reveal_annotation: Option<u64>,
    stats: HashMap<PathBuf, DiffStat>,
    /// Expanded file rows, keyed by (section, path): a path that appears in
    /// Staged and Changed at once expands independently in each.
    expanded_changes: HashSet<(ChangeSection, PathBuf)>,
    /// Collapsed sections; a collapsed section shows only its header.
    collapsed_sections: HashSet<ChangeSection>,
    /// Expanded collapsed-context bands, keyed by (section, path, run key).
    expanded_bands: HashSet<(ChangeSection, PathBuf, usize)>,
    git_task: Option<Task<()>>,
    /// Mutations requested while a refresh or another mutation is running.
    /// They are started in click order as each task completes.
    pending_operations: VecDeque<GitOperation>,
    /// Whether a snapshot load has ever completed, successfully or not.
    ///
    /// The full-surface loader is a *first-load* treatment: once the panel has
    /// shown real content it must never blank back to an orb, and a clean repo
    /// must settle on its empty-state message rather than flashing the loader
    /// on every refresh. `git_task.is_some()` cannot express that — it is also
    /// true for the second refresh of a repo that simply has nothing to show.
    has_loaded: bool,
    git_error: Option<String>,
    /// A successful status refresh cannot clear this error: `git status` can
    /// still work while a mutation is blocked by `.git/index.lock`.
    git_error_from_mutation: bool,
    /// Paths whose diff failed to load, keyed like `diffs`. Kept separate so
    /// the expanded row can name the failure instead of showing nothing.
    diff_errors: HashMap<PathBuf, String>,
    /// One polling loop per tab, armed on first render.
    refresh_started: bool,
    /// Frames this surface has actually been drawn in (#193).
    ///
    /// The same signal, and for the same reason, as `RightPanel`'s in #189:
    /// a background tab keeps its refresh loop running, and a value polled
    /// from `render` goes stale exactly when drawing stops. A count of real
    /// draws cannot. Each tick here spawns three or four git processes --
    /// `status --untracked-files=all`, `diff --numstat HEAD`, `rev-parse
    /// --verify HEAD` -- so a Changes tab nobody is looking at was measured
    /// at 77 git processes in twenty seconds, the same as one in front of
    /// the user.
    renders: u64,
    /// The `renders` value the previous tick saw. Equal means no draw
    /// happened in between, so this tick skips the whole snapshot load.
    renders_at_last_tick: u64,
    /// Whether this surface is the right panel's Diff view rather than the
    /// Changes tab itself.
    ///
    /// Only the panel draws `Open diff`: the action reveals the Changes tab
    /// and focuses the path, which is a move from the panel and a no-op
    /// from inside the tab, where the row is already expanded by the time
    /// the control is visible at all (#217).
    embedded_in_panel: bool,
    /// Set when a tick was skipped, so the next drawn frame reloads at once
    /// instead of showing a stale diff.
    refresh_suspended: bool,
    /// Consecutive ticks skipped because nothing drew this surface.
    ///
    /// A gate that can only be released by a draw can suspend *forever*
    /// when no draw is ever coming -- an entity driven outside a window,
    /// or any future path that stops drawing without dropping the entity.
    /// That is a liveness bug, not a saving, so after
    /// `SUSPENDED_TICK_BUDGET` skipped ticks one refresh runs regardless.
    /// It bounds staleness for anything still live and, if the surface is
    /// in fact visible, the draw that follows releases the gate properly.
    suspended_ticks: u32,
    /// F-CHG-13: the `focus_path` requests that arrived before `entries` had
    /// been populated by the first `refresh()` (the common case — `new()`
    /// starts that refresh asynchronously, so a caller that opens the tab
    /// and asks it to focus a path in the same tick always races it). Every
    /// one is kept, so a caller reopening several files gets them all back.
    /// Replayed once the next snapshot lands, then cleared either way.
    pending_focus: Vec<PathBuf>,
    /// The file last opened through `focus_path` or a row expansion
    /// (`toggle_change`). Kept, unlike `pending_focus` — which is consumed
    /// once a snapshot lands — so the host can save it and replay it after
    /// a restart. Cleared when that file's last expanded row is collapsed.
    last_focus: Option<PathBuf>,
    /// A `focus_line` request not yet drawn; consumed by the frame that can
    /// scroll to it.
    reveal_line: Option<(PathBuf, AnnotationSide, Option<usize>)>,
    /// #325: the keyboard-selected file row, keyed like `expanded_changes`
    /// because one path can appear in two sections and Enter has to act on
    /// the one the user is actually on.
    selected_change: Option<(ChangeSection, PathBuf)>,
    /// The Discard confirmation open over the surface: what it would throw
    /// away. In the window, not the system's prompt (spec §8), so a test and
    /// the control socket can see it and answer it.
    discard_ask: Option<DiscardAsk>,
    /// Where a Range surface's change request lives on its forge, for each
    /// row's *Open on the forge*; `None` for every other source.
    forge_files: Option<Rc<ForgeFiles>>,
    /// Focus for the list, so `on_key_down` reaches it. Built lazily at
    /// first render, the way the Files tree's is.
    list_focus: Option<FocusHandle>,
    /// The virtualized list's state. `gpui::list` lays out only the rows
    /// in and just around the viewport, so a frame over a 15 000-line diff
    /// costs what the viewport costs, not what the file costs -- the old
    /// `overflow_y_scroll` container built every row of every expanded
    /// diff on every frame, and one wheel tick over a large diff cost
    /// seconds. Kept in step with the row stream by `sync_list_rows`.
    list_state: ListState,
    /// Fingerprint of the row stream `list_state` was last spliced to
    /// (kinds, sections, paths, keys -- never text). A refresh that
    /// re-reads an unchanged diff leaves it alone; an expand, a collapse
    /// or a file appearing re-splices.
    list_fingerprint: u64,
    /// Set by keyboard selection so the next frame scrolls the selected
    /// file row into view: a virtualized list draws nothing off-screen, so
    /// a selection that moved there would otherwise be invisible.
    reveal_selected: bool,
    unified_x: Pixels,
    unified_mode: DiffViewMode,
    unified_max_width: Pixels,
    unified_viewport: Pixels,
    unified_width_dirty: bool,
    unified_bar_state: HorizontalBarState,
    split_left_x: Pixels,
    split_right_x: Pixels,
    split_left_max_width: Pixels,
    split_right_max_width: Pixels,
    split_viewport: Pixels,
    split_left_bar_state: HorizontalBarState,
    split_right_bar_state: HorizontalBarState,
}

impl ChangesTab {
    /// Creates the tab for one checkout and starts its first refresh. The
    /// poll loop then re-checks on the interval after the first render.
    pub fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::new_with_git_capability(repo_root, true, cx)
    }

    /// Creates a working-tree surface with the project's known Git capability.
    pub fn new_with_git_capability(
        repo_root: PathBuf,
        is_git: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_source(repo_root, ChangesSource::WorkingTree, is_git, cx)
    }

    /// Creates a read-only tab showing one commit's files and diffs. The
    /// surface is immutable: every stage/unstage/discard entry point
    /// refuses to run and the mutation buttons are not drawn.
    pub fn for_commit(repo_root: PathBuf, sha: String, cx: &mut Context<Self>) -> Self {
        Self::with_source(repo_root, ChangesSource::Commit(sha), true, cx)
    }

    /// Creates a read-only surface over a change request's range: its files
    /// and counts now, each file's diff when its row opens.
    pub fn for_range(repo_root: PathBuf, base: String, head: String, cx: &mut Context<Self>) -> Self {
        Self::with_source(repo_root, ChangesSource::Range { base, head }, true, cx)
    }

    /// The `(base, head)` this surface shows, or `None` for any other source.
    pub fn range(&self) -> Option<(&str, &str)> {
        match &self.source {
            ChangesSource::Range { base, head } => Some((base, head)),
            _ => None,
        }
    }

    /// Every path with an expanded row — what a rebuilt surface reopens.
    pub fn expanded_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.expanded_changes.iter().map(|(_, path)| path.clone()).collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Whether the diff deletes `path`. Read from the index column, which is
    /// where a commit or a range surface reports every entry (each reads as
    /// staged); on the working tree a deletion not yet staged answers false.
    pub fn is_deleted(&self, path: &Path) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.path == path && entry.index_status == Some(StatusKind::Deleted))
    }

    /// Creates the surface the right panel's Diff view embeds. Identical to
    /// `new` except that it draws `Open diff`, which reveals the Changes tab
    /// — something only a host that is not that tab can ask for (#217).
    pub fn in_right_panel(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::in_right_panel_with_git_capability(repo_root, true, cx)
    }

    /// Creates the right-panel surface with the project's known Git capability.
    pub fn in_right_panel_with_git_capability(
        repo_root: PathBuf,
        is_git: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tab = Self::with_source(repo_root, ChangesSource::WorkingTree, is_git, cx);
        tab.embedded_in_panel = true;
        tab
    }

    fn with_source(
        repo_root: PathBuf,
        source: ChangesSource,
        is_git: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tab = Self {
            repo_root,
            is_git,
            source,
            entries: Vec::new(),
            diffs: HashMap::new(),
            stats: HashMap::new(),
            expanded_changes: HashSet::new(),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: false,
            git_error: None,
            git_error_from_mutation: false,
            diff_errors: HashMap::new(),
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        };
        // Menu and socket openings both construct this same surface, so the
        // first report is always produced by the surface's own refresh path.
        tab.refresh(cx);
        tab
    }

    /// Whether this surface may mutate the repository. The working tree
    /// can; a commit view is immutable by definition.
    pub fn allows_staging(&self) -> bool {
        self.is_git && matches!(self.source, ChangesSource::WorkingTree)
    }

    /// Whether this working-tree surface is backed by a known Git project.
    pub fn is_git_capable(&self) -> bool {
        self.is_git
    }

    /// Whether this surface follows the selected project's Git capability.
    pub fn uses_project_git_capability(&self) -> bool {
        matches!(self.source, ChangesSource::WorkingTree)
    }

    /// The commit this surface shows, or `None` for the working tree.
    pub fn commit(&self) -> Option<&str> {
        match &self.source {
            ChangesSource::Commit(sha) => Some(sha),
            ChangesSource::WorkingTree | ChangesSource::Range { .. } => None,
        }
    }

    /// The file last opened through [`Self::focus_path`] or a row
    /// expansion, cleared when that file's last expanded row is collapsed.
    pub fn focused_path(&self) -> Option<&Path> {
        self.last_focus.as_deref()
    }

    /// Returns the status and per-file counts currently held by this mounted
    /// surface. Empty buckets are included so automation can compare all
    /// three counts directly with `git status --porcelain`.
    pub fn report(&self) -> ChangesReport {
        let snapshot = StatusSnapshot {
            entries: self.entries.clone(),
        };
        let sections = ChangeSection::ORDER
            .into_iter()
            .map(|section| {
                let entries = match section {
                    ChangeSection::Staged => snapshot.staged(),
                    ChangeSection::Changed => snapshot.changes(),
                    ChangeSection::Untracked => snapshot.untracked(),
                };
                let files = entries
                    .into_iter()
                    .map(|entry| {
                        let stat = self.stats.get(&entry.path).copied().unwrap_or(DiffStat {
                            additions: 0,
                            deletions: 0,
                            is_binary: false,
                        });
                        ChangesFileReport {
                            path: entry.path.clone(),
                            additions: stat.additions,
                            deletions: stat.deletions,
                            is_binary: stat.is_binary,
                        }
                    })
                    .collect::<Vec<_>>();
                ChangesSectionReport {
                    name: section.label_in(&self.source),
                    count: files.len(),
                    files,
                }
            })
            .collect();
        ChangesReport {
            repo_root: self.repo_root.clone(),
            sections,
            annotations: self.annotation_reports(),
            loading: self.git_task.is_some(),
            error: self.git_error.clone(),
            dialog: self.discard_ask.as_ref().map(|ask| match ask {
                DiscardAsk::One(path) => format!("discard:{}", path.display()),
                DiscardAsk::All(_) => "discard-all".to_string(),
            }),
        }
    }

    /// The diff's owner supplies the cards; this surface owns their rows.
    pub fn set_annotations(
        &mut self,
        annotations: Vec<Annotation>,
        views: HashMap<u64, AnyView>,
        cx: &mut Context<Self>,
    ) {
        let mut open_threads = HashMap::new();
        for annotation in &annotations {
            if matches!(annotation.kind, AnnotationKind::Thread { open: true }) {
                *open_threads.entry(annotation.path.clone()).or_insert(0) += 1;
            }
        }
        self.annotations = annotations;
        self.annotation_views = Rc::new(views);
        self.open_threads = Rc::new(open_threads);
        self.unified_width_dirty = true;
        cx.notify();
    }

    fn annotation_reports(&self) -> Vec<AnnotationReport> {
        if self.annotations.is_empty() {
            return Vec::new();
        }
        let rows: Vec<ChangeRow> = self.section_rows(self.unified_mode)
            .into_iter().flat_map(|section| section.rows).collect();
        let mut reports: Vec<AnnotationReport> = self.annotations.iter().map(|annotation| {
            let drawn = rows.iter().any(|row| matches!(row,
                ChangeRow::Annotation { key, path, .. }
                    if *key == annotation.key && path == &annotation.path));
            let placed = if !drawn {
                "hidden"
            } else if matches!(annotation.kind, AnnotationKind::Outdated { .. }) {
                "section"
            } else if rows.iter().any(|row| annotation_matches_row(annotation, row)) {
                "line"
            } else {
                "file"
            };
            AnnotationReport { key: annotation.key, path: annotation.path.clone(), placed }
        }).collect();
        reports.sort_by_key(|report| rows.iter().position(|row| matches!(row,
            ChangeRow::Annotation { key, path, .. } if *key == report.key && *path == report.path,
        )).unwrap_or(usize::MAX));
        reports
    }

    fn apply_snapshot(&mut self, snapshot: GitSnapshot, cx: &mut Context<Self>) {
        // A file that leaves the status list forgets its expansion; a file
        // that merely changes section (staged → unstaged) forgets it too —
        // the old (section, path) key no longer exists.
        self.expanded_changes
            .retain(|(_, path)| snapshot.entries.iter().any(|entry| &entry.path == path));
        // A lazy refresh only re-fetches diffs for expanded rows (see
        // `load_worktree_snapshot`), so `diffs`/`diff_errors` cannot simply
        // be replaced wholesale by this snapshot's -- that would evict a
        // collapsed row's already-cached diff on every tick, for a path
        // this refresh never even asked git about. A path this refresh did
        // fetch always takes the fresh result (in whichever of the two maps
        // it landed); a path it left alone keeps whatever it had; a path
        // that dropped out of the entry list entirely (staged away,
        // reverted) is dropped from both.
        let live_paths: HashSet<&PathBuf> =
            snapshot.entries.iter().map(|entry| &entry.path).collect();
        self.diffs.retain(|path, _| live_paths.contains(path));
        self.diff_errors.retain(|path, _| live_paths.contains(path));
        for path in snapshot.diffs.keys().chain(snapshot.diff_errors.keys()) {
            self.diffs.remove(path);
            self.diff_errors.remove(path);
        }
        self.diffs.extend(snapshot.diffs);
        self.diff_errors.extend(snapshot.diff_errors);
        self.entries = snapshot.entries;
        self.stats = snapshot.stats;
        self.unified_width_dirty = true;
        // F-CHG-13: replay the focus_path requests that raced this snapshot.
        // Applied at most once — if a path still isn't present (e.g. it
        // was reverted before the snapshot came back), there is nothing
        // further to wait for.
        for path in std::mem::take(&mut self.pending_focus) {
            self.apply_focus(&path, cx);
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.is_git {
            self.has_loaded = true;
            self.git_error = None;
            self.git_error_from_mutation = false;
            self.entries.clear();
            self.diffs.clear();
            self.stats.clear();
            self.diff_errors.clear();
            cx.notify();
            return;
        }
        if self.git_task.is_some() {
            return;
        }
        let repo_root = self.repo_root.clone();
        let source = self.source.clone();
        let expanded_paths = self.expanded_paths_for_load();
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    load_snapshot(&repo_root, &source, expanded_paths.as_ref())
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                tab.git_task = None;
                tab.has_loaded = true;
                match result {
                    Ok(snapshot) => {
                        tab.apply_snapshot(snapshot, cx);
                        if !tab.git_error_from_mutation {
                            tab.git_error = None;
                        }
                    }
                    Err(error) if !tab.git_error_from_mutation => tab.git_error = Some(error),
                    Err(_) => {}
                }
                if let Some(operation) = tab.pending_operations.pop_front() {
                    tab.start_operation_boxed(operation, cx);
                }
                cx.notify();
            });
        }));
    }

    /// The diff-fetch scope for the next snapshot load: `None` (fetch every
    /// entry) before the surface has ever settled, `Some(paths)` (fetch only
    /// what's expanded) afterward. See `load_worktree_snapshot`.
    fn expanded_paths_for_load(&self) -> Option<HashSet<PathBuf>> {
        self.has_loaded.then(|| {
            self.expanded_changes
                .iter()
                .map(|(_, path)| path.clone())
                .collect()
        })
    }

    /// Arms the periodic refresh loop: once immediately, then on the
    /// interval. The timer runs on the background executor; git status never
    /// touches the render thread. A commit view never arms it: a commit's
    /// contents are fixed, so re-reading them every second would only burn
    /// git processes.
    fn ensure_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_started || !self.allows_staging() {
            return;
        }
        self.refresh_started = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(CHANGES_REFRESH_INTERVAL)
                    .await;
                let carry_on = this.update(cx, |tab, cx| {
                    // #193: no draw since the previous tick means nothing is
                    // showing this surface -- a background tab, a minimised
                    // window -- so skip the whole snapshot load rather than
                    // spawning three or four git processes for it.
                    if tab.renders == tab.renders_at_last_tick
                        && tab.suspended_ticks < SUSPENDED_TICK_BUDGET
                    {
                        tab.suspended_ticks += 1;
                        tab.refresh_suspended = true;
                        return;
                    }
                    tab.suspended_ticks = 0;
                    tab.renders_at_last_tick = tab.renders;
                    tab.refresh(cx);
                });
                if carry_on.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    fn start_operation<F>(&mut self, operation: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&Path) -> Result<(), GitError> + Send + 'static,
    {
        self.start_operation_boxed(Box::new(operation), cx);
    }

    fn start_operation_boxed(&mut self, operation: GitOperation, cx: &mut Context<Self>) {
        if self.git_task.is_some() {
            self.pending_operations.push_back(operation);
            return;
        }
        let repo_root = self.repo_root.clone();
        let source = self.source.clone();
        let expanded_paths = self.expanded_paths_for_load();
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let result = operation(&repo_root);
                    let snapshot = result
                        .as_ref()
                        .ok()
                        .map(|_| load_snapshot(&repo_root, &source, expanded_paths.as_ref()));
                    (result, snapshot)
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                tab.git_task = None;
                tab.has_loaded = true;
                let next_operation = tab.pending_operations.pop_front();
                match outcome {
                    (Ok(()), Some(Ok(snapshot))) => {
                        tab.apply_snapshot(snapshot, cx);
                        tab.git_error = None;
                        tab.git_error_from_mutation = false;
                    }
                    (Err(error), _) => {
                        tab.git_error = Some(error.to_string());
                        tab.git_error_from_mutation = true;
                    }
                    (Ok(()), Some(Err(error))) => {
                        tab.git_error = Some(format!(
                            "the change was applied, but refreshing failed: {error}"
                        ));
                        tab.git_error_from_mutation = false;
                    }
                    (Ok(()), None) => {
                        unreachable!("a successful operation always loads a snapshot")
                    }
                }
                if let Some(operation) = next_operation {
                    tab.start_operation_boxed(operation, cx);
                }
                cx.notify();
            });
        }));
    }

    fn retry_refresh(&mut self, cx: &mut Context<Self>) {
        // Retry is an explicit user action. It dismisses a previous
        // mutation error before asking git for a fresh snapshot; automatic
        // refreshes do not have that authority.
        self.git_error = None;
        self.git_error_from_mutation = false;
        self.refresh(cx);
    }

    fn stage_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        self.start_operation(move |repo| stage(repo, &path), cx);
    }

    fn unstage_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        self.start_operation(move |repo| unstage(repo, &path), cx);
    }

    fn section_action(&mut self, section: ChangeSection, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        let snapshot = StatusSnapshot {
            entries: self.entries.clone(),
        };
        let paths = match section {
            ChangeSection::Staged => snapshot
                .staged()
                .into_iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            ChangeSection::Changed => snapshot
                .changes()
                .into_iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            ChangeSection::Untracked => snapshot
                .untracked()
                .into_iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
        };
        if paths.is_empty() {
            return;
        }
        let operation: fn(&Path, &Path) -> Result<(), GitError> = match section {
            ChangeSection::Staged => unstage,
            ChangeSection::Changed | ChangeSection::Untracked => stage,
        };
        self.start_operation(
            move |repo| {
                for path in paths {
                    operation(repo, &path)?;
                }
                Ok(())
            },
            cx,
        );
    }

    /// Gives a Range surface its change request's place on the forge, for
    /// each row's *Open on the forge*.
    pub fn set_forge_files(&mut self, files: ForgeFiles) {
        self.forge_files = Some(Rc::new(files));
    }

    /// Asks, inside the window, before throwing away `path`'s worktree changes.
    pub fn ask_discard(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        self.discard_ask = Some(DiscardAsk::One(path));
        cx.notify();
    }

    /// Asks before throwing away every worktree change; nothing to ask about
    /// asks nothing.
    pub fn ask_discard_all(&mut self, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        let paths = self
            .entries
            .iter()
            .filter(|entry| entry.has_worktree_changes())
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }
        self.discard_ask = Some(DiscardAsk::All(paths));
        cx.notify();
    }

    pub fn close_discard(&mut self, cx: &mut Context<Self>) {
        if self.discard_ask.take().is_some() {
            cx.notify();
        }
    }

    /// The dialog's confirm: closes it and queues the discard behind whatever
    /// git work is in flight, the way a click always did. `false` when no
    /// dialog was open, so a second press does nothing.
    ///
    /// It throws away what the dialog listed and nothing more, and only what
    /// the surface still lists: a file an agent changed while the dialog was
    /// open was never shown, and a change that is gone (or a path the socket
    /// named that never had one) has nothing left to discard.
    pub fn confirm_discard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ask) = self.discard_ask.take() else {
            return false;
        };
        let paths: Vec<PathBuf> = match ask {
            DiscardAsk::One(path) => vec![path]
                .into_iter()
                .filter(|path| self.entries.iter().any(|entry| &entry.path == path))
                .collect(),
            DiscardAsk::All(paths) => paths
                .into_iter()
                .filter(|path| {
                    self.entries
                        .iter()
                        .any(|entry| &entry.path == path && entry.has_worktree_changes())
                })
                .collect(),
        };
        if !paths.is_empty() {
            self.start_operation(
                move |repo| paths.iter().try_for_each(|path| discard(repo, path)),
                cx,
            );
        }
        cx.notify();
        true
    }

    /// #325: every file row currently drawn, in draw order, as the pair the
    /// selection is keyed by. Section headers and diff bands are skipped —
    /// only rows Enter can act on are navigable.
    fn selectable_rows(&self, mode: DiffViewMode) -> Vec<(ChangeSection, PathBuf)> {
        self.section_rows(mode)
            .into_iter()
            .flat_map(|section| section.rows)
            .filter_map(|row| match row {
                ChangeRow::File { section, entry, .. } => Some((section, entry.path)),
                _ => None,
            })
            .collect()
    }

    fn select_change(&mut self, row: (ChangeSection, PathBuf), cx: &mut Context<Self>) {
        if self.selected_change.as_ref() != Some(&row) {
            self.selected_change = Some(row);
            self.reveal_selected = true;
            cx.notify();
        }
    }

    /// #325: up/down move the selection, Enter promotes it — the keyboard
    /// path to what "Open diff" does with the mouse. Inside the Changes tab
    /// there is nothing to promote *to* (the tab is already the destination),
    /// so Enter expands the row there instead, matching what a click does.
    fn on_change_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.selectable_rows(DiffViewMode::get(cx));
        if rows.is_empty() {
            return;
        }
        let current = self
            .selected_change
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| row == selected));
        match event.keystroke.key.as_str() {
            "down" => {
                let index = current.map_or(0, |index| (index + 1).min(rows.len() - 1));
                self.select_change(rows[index].clone(), cx);
            }
            "up" => {
                let index = current.unwrap_or(0).saturating_sub(1);
                self.select_change(rows[index].clone(), cx);
            }
            "enter" | "return" => {
                let Some(index) = current else {
                    return;
                };
                let (section, path) = rows[index].clone();
                if self.embedded_in_panel {
                    cx.emit(ChangesTabActionEvent::OpenDiff(path));
                } else {
                    self.toggle_change(section, &path, cx);
                }
            }
            _ => {}
        }
    }

    fn toggle_change(&mut self, section: ChangeSection, path: &Path, cx: &mut Context<Self>) {
        let key = (section, path.to_path_buf());
        self.unified_width_dirty = true;
        if self.expanded_changes.contains(&key) {
            self.expanded_changes.remove(&key);
            // A partially-staged file can be expanded in two sections at
            // once: only forget the file when its last expanded row closes,
            // and only when it is the saved one.
            if !self
                .expanded_changes
                .iter()
                .any(|(_, expanded)| expanded.as_path() == path)
                && self.last_focus.as_deref() == Some(path)
            {
                self.last_focus = None;
            }
        } else {
            self.expanded_changes.insert(key);
            self.last_focus = Some(path.to_path_buf());
            self.fetch_expanded_diff(path.to_path_buf(), cx);
        }
        cx.notify();
    }

    /// Fetches one file's full diff in the background and merges it into
    /// `self.diffs` once it lands, rather than leaving a just-expanded row
    /// waiting on the next periodic tick (up to `CHANGES_REFRESH_INTERVAL`
    /// away) to show anything.
    ///
    /// A no-op when the diff (or its failure) is already cached: the first
    /// load fetches every entry eagerly, so this only does real work for a
    /// row a lazy refresh had dropped, or a fresh diff a mutation-triggered
    /// refresh raced ahead of.
    /// The unavailable diff's Retry: forget the failure, then ask git again.
    fn retry_diff(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.diff_errors.remove(&path);
        self.fetch_expanded_diff(path, cx);
        cx.notify();
    }

    fn fetch_expanded_diff(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.diffs.contains_key(&path) || self.diff_errors.contains_key(&path) {
            return;
        }
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
        else {
            return;
        };
        let repo_root = self.repo_root.clone();
        let source = self.source.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_expanded_diff(&repo_root, &source, &entry) })
                .await;
            let _ = this.update(cx, |tab, cx| {
                match result {
                    Ok(diff) => {
                        tab.diffs.insert(path, diff);
                    }
                    Err(error) => {
                        tab.diff_errors.insert(path, error.to_string());
                    }
                }
                tab.unified_width_dirty = true;
                tab.resolve_reveal_line(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens `path`, opens the collapsed context band that hides new-side
    /// line `line`, and scrolls that row into view once its diff has loaded.
    /// A line that is in no hunk (an outdated comment) reveals the file's row.
    pub fn focus_line(&mut self, path: &Path, line: usize, cx: &mut Context<Self>) {
        self.reveal_annotation = None;
        self.reveal_line = Some((path.to_path_buf(), AnnotationSide::New, Some(line)));
        self.focus_path(path, cx);
        self.resolve_reveal_line(cx);
    }

    /// Opens the file and reveals its annotation, or a line on either side.
    pub fn focus_anchor(
        &mut self,
        path: &Path,
        side: AnnotationSide,
        line: Option<u32>,
        key: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        self.reveal_annotation = key;
        self.reveal_line = Some((path.to_path_buf(), side, line.map(|line| line as usize)));
        self.focus_path(path, cx);
        self.resolve_reveal_line(cx);
    }

    /// Runs when a diff lands and when a reveal is requested: opens the band
    /// that hides the pending line, so the next frame has a row to scroll to.
    fn resolve_reveal_line(&mut self, cx: &mut Context<Self>) {
        let Some((path, side, Some(line))) = self.reveal_line.clone() else {
            return;
        };
        let Some(diff) = self.diffs.get(&path) else {
            return;
        };
        let key = if self.annotations.is_empty() {
            band_key_containing(diff, side, line)
        } else {
            band_key_containing_annotations(diff, &self.annotations, side, line)
        };
        if let Some(key) = key {
            for section in ChangeSection::ORDER {
                if self.is_expanded(section, &path) {
                    self.expanded_bands.insert((section, path.clone(), key));
                }
            }
        }
        self.unified_width_dirty = true;
        cx.notify();
    }

    /// The control socket's `surface.changes.view`: the diff's layout, a
    /// file opened the way a click on its row opens it, and the toolbar's
    /// Refresh. The poll only runs once the surface is drawn, so a headless
    /// run asks for the refresh a person would get from it.
    pub fn control_view(
        &mut self,
        mode: Option<DiffViewMode>,
        expand: Option<&Path>,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(mode) = mode {
            self.set_view_mode(mode, cx);
        }
        if let Some(path) = expand {
            self.focus_path(path, cx);
        }
        if refresh {
            self.refresh(cx);
        }
    }

    /// F-CHG-13: `RightPanelActionEvent::OpenDiff(path)` and
    /// `ChangesTabActionEvent::OpenDiff(path)` both expect the receiver to
    /// do something path-specific with the diff, not just open the generic
    /// multi-file tab. Expands whichever section(s) currently carry `path`
    /// (a partially-staged file can appear in more than one) and makes sure
    /// none of them are collapsed, reusing the same `expanded_changes` /
    /// `collapsed_sections` state manual expand/collapse already drives, so
    /// the file's diff is immediately visible instead of needing a second
    /// manual expand click.
    pub fn focus_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.last_focus = Some(path.to_path_buf());
        // If entries hasn't been populated yet (the caller raced the async
        // refresh new() kicked off), there is nothing to match against yet:
        // remember the request and replay it once a snapshot lands in
        // apply_snapshot, rather than silently no-op'ing.
        if self.entries.is_empty() && self.git_task.is_some() {
            if !self.pending_focus.iter().any(|pending| pending == path) {
                self.pending_focus.push(path.to_path_buf());
            }
            return;
        }
        self.apply_focus(path, cx);
        cx.notify();
    }

    /// Expands and un-collapses every section containing `path`. Returns
    /// whether any section matched, so callers can decide whether to defer.
    fn apply_focus(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let snapshot = StatusSnapshot {
            entries: self.entries.clone(),
        };
        let mut matched = false;
        for section in ChangeSection::ORDER {
            let entries = match section {
                ChangeSection::Staged => snapshot.staged(),
                ChangeSection::Changed => snapshot.changes(),
                ChangeSection::Untracked => snapshot.untracked(),
            };
            if entries.iter().any(|entry| entry.path == path) {
                self.collapsed_sections.remove(&section);
                self.expanded_changes.insert((section, path.to_path_buf()));
                matched = true;
            }
        }
        if matched {
            self.fetch_expanded_diff(path.to_path_buf(), cx);
        }
        matched
    }

    fn is_expanded(&self, section: ChangeSection, path: &Path) -> bool {
        self.expanded_changes
            .contains(&(section, path.to_path_buf()))
    }

    fn is_band_expanded(&self, section: ChangeSection, path: &Path, key: usize) -> bool {
        self.expanded_bands
            .contains(&(section, path.to_path_buf(), key))
    }

    fn toggle_band(
        &mut self,
        section: ChangeSection,
        path: PathBuf,
        key: usize,
        cx: &mut Context<Self>,
    ) {
        self.unified_width_dirty = true;
        if !self.expanded_bands.remove(&(section, path.clone(), key)) {
            self.expanded_bands.insert((section, path, key));
        }
        cx.notify();
    }

    /// Expand All (orca's diff-header affordance, `04-ux-patterns`): every
    /// changed file opens its diff, and every collapsed-context band inside
    /// it opens too. The band keys are computed by the same walk the
    /// expansion render uses, so the two can never disagree about which
    /// runs exist.
    fn expand_all(&mut self, cx: &mut Context<Self>) {
        self.unified_width_dirty = true;
        let snapshot = StatusSnapshot {
            entries: self.entries.clone(),
        };
        for section in ChangeSection::ORDER {
            let entries = match section {
                ChangeSection::Staged => snapshot.staged(),
                ChangeSection::Changed => snapshot.changes(),
                ChangeSection::Untracked => snapshot.untracked(),
            };
            for entry in entries {
                let path = entry.path.clone();
                self.expanded_changes.insert((section, path.clone()));
                if let Some(diff) = self.diffs.get(&path) {
                    for (key, _) in context_band_keys(diff, &self.annotations) {
                        self.expanded_bands.insert((section, path.clone(), key));
                    }
                }
            }
        }
        cx.notify();
    }

    /// Collapse All: every expanded file and every expanded band closes. The
    /// choice is a view state, not a git mutation — a refresh keeps the
    /// collapsed view.
    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.unified_width_dirty = true;
        self.expanded_changes.clear();
        self.expanded_bands.clear();
        self.last_focus = None;
        cx.notify();
    }

    fn toggle_section(&mut self, section: ChangeSection, cx: &mut Context<Self>) {
        self.unified_width_dirty = true;
        if !self.collapsed_sections.remove(&section) {
            self.collapsed_sections.insert(section);
        }
        cx.notify();
    }

    /// The three sections of the changes list, in display order (Staged,
    /// Changed, Untracked), each with the rows under it. The three buckets
    /// come from the finished data layer — `StatusSnapshot::{staged,
    /// changes, untracked}` — which exists precisely because a file can
    /// hold two independent states at once. An empty section is omitted;
    /// a collapsed section keeps only its header.
    fn section_rows(&self, mode: DiffViewMode) -> Vec<SectionRows> {
        #[cfg(test)]
        perf_baseline::section_build();
        let snapshot = StatusSnapshot {
            entries: self.entries.clone(),
        };
        let mut sections = Vec::new();
        for section in ChangeSection::ORDER {
            let entries = match section {
                ChangeSection::Staged => snapshot.staged(),
                ChangeSection::Changed => snapshot.changes(),
                ChangeSection::Untracked => snapshot.untracked(),
            };
            if entries.is_empty() {
                continue;
            }
            let collapsed = self.collapsed_sections.contains(&section);
            let count = entries.len();
            let mut rows = Vec::new();
            if !collapsed {
                for entry in entries {
                    let expanded = self.is_expanded(section, &entry.path);
                    rows.push(ChangeRow::File {
                        section,
                        entry: entry.clone(),
                        // `None` when the counts are unknown (a file whose
                        // diff also failed): the row then renders `·`
                        // instead of a confident +0 −0.
                        stat: self.stats.get(&entry.path).copied(),
                        drag_payload: self.diffs.get(&entry.path).and_then(diff_payload),
                        expanded,
                    });
                    if expanded {
                        self.expand_diff(&mut rows, section, entry, mode);
                    }
                }
            }
            sections.push(SectionRows {
                section,
                count,
                collapsed,
                rows,
            });
        }
        sections
    }

    /// Appends the diff rows (hunks, collapsed context bands, lines) for one
    /// expanded file, under the section that row belongs to. The diff is the
    /// worktree-vs-HEAD combined diff — the same one the +N −M counts come
    /// from.
    fn expand_diff(
        &self,
        rows: &mut Vec<ChangeRow>,
        section: ChangeSection,
        entry: &StatusEntry,
        mode: DiffViewMode,
    ) {
        let file_start = rows.len();
        let Some(diff) = self.diffs.get(&entry.path) else {
            // The diff failed to load: say so explicitly. Rows that render
            // nothing here would make an expanded file look like "no
            // changes" next to real +/− counts — the lie this variant
            // exists to prevent.
            if let Some(error) = self.diff_errors.get(&entry.path) {
                rows.push(ChangeRow::Unavailable {
                    section,
                    path: entry.path.clone(),
                    message: format!("diff unavailable: {error}"),
                    failed: true,
                });
            }
            self.splice_annotations(rows, file_start, section, &entry.path);
            return;
        };
        // F-CHG-15: a binary file has no text diff by definition -- git
        // reports it with zero hunks, so falling through to the hunk loop
        // below renders an empty body next to sibling rows that do show
        // real diff lines, which reads as "this file has no changes" when
        // the entry is plainly dirty. Say so explicitly instead, matching
        // the Swift original's inline `ContentUnavailableView("Binary diff
        // unavailable", …)` in `ChangesListView.diffBody` -- as opposed to
        // `editor.rs`'s separate file-viewer message, which is a different
        // surface entirely.
        if diff.is_binary {
            rows.push(ChangeRow::Unavailable {
                section,
                path: entry.path.clone(),
                message: "Binary diff unavailable".to_string(),
                failed: false,
            });
            self.splice_annotations(rows, file_start, section, &entry.path);
            return;
        }
        // A run of unchanged context lines collapses into one labelled band
        // (orca's "18 hidden lines"), expanded in place on click. `key` is
        // the run's first line's position in the file's flattened line
        // stream — stable within one snapshot, and *identical in both view
        // modes*, so switching Unified↔Split never reopens a different band
        // than the one the reader opened.
        let mut line_index = 0usize;
        // Running index of the split rows this file emits, for element ids.
        let mut split_key = 0usize;
        let (old, new) = anchored_lines(&self.annotations, &entry.path);
        for hunk in &diff.hunks {
            rows.push(ChangeRow::Hunk {
                section,
                path: entry.path.clone(),
                header: hunk.header.clone(),
            });
            let mut i = 0usize;
            while i < hunk.lines.len() {
                // Walk the hunk in maximal same-kind runs. Context runs are
                // what the bands collapse; a non-context run is exactly the
                // deletion/addition stretch the side-by-side model zips, and
                // handing it over whole is what makes Split's pairing the
                // model's behaviour rather than a second implementation of
                // it. Splitting at context boundaries changes nothing:
                // `append_lines` flushes on every context line anyway.
                let is_context = hunk.lines[i].origin == DiffOrigin::Context;
                let start = i;
                while i < hunk.lines.len()
                    && (hunk.lines[i].origin == DiffOrigin::Context) == is_context
                {
                    i += 1;
                }
                let segment = &hunk.lines[start..i];
                if !is_context {
                    push_segment(rows, section, &entry.path, segment, mode, &mut split_key);
                    continue;
                }
                let count = segment.len();
                let key = line_index + start;
                let indices = anchored_indices(segment, &old, &new);
                if !indices.is_empty() {
                    for piece in band_pieces(count, &indices, CONTEXT_BAND_MIN) {
                        let piece_key = key + piece.start;
                        let expanded = self.is_band_expanded(section, &entry.path, piece_key);
                        if piece.band {
                            rows.push(ChangeRow::ContextBand {
                                section,
                                path: entry.path.clone(),
                                key: piece_key,
                                count: piece.end - piece.start,
                                expanded,
                            });
                        }
                        if !piece.band || expanded {
                            push_segment(
                                rows, section, &entry.path, &segment[piece.start..piece.end],
                                mode, &mut split_key,
                            );
                        }
                    }
                    continue;
                }
                let expanded = self.is_band_expanded(section, &entry.path, key);
                if count >= CONTEXT_BAND_MIN {
                    rows.push(ChangeRow::ContextBand {
                        section,
                        path: entry.path.clone(),
                        key,
                        count,
                        expanded,
                    });
                    if expanded {
                        push_segment(rows, section, &entry.path, segment, mode, &mut split_key);
                    }
                } else {
                    push_segment(rows, section, &entry.path, segment, mode, &mut split_key);
                }
            }
            line_index += hunk.lines.len();
        }
        self.splice_annotations(rows, file_start, section, &entry.path);
    }

    fn splice_annotations(
        &self,
        rows: &mut Vec<ChangeRow>,
        file_start: usize,
        section: ChangeSection,
        path: &Path,
    ) {
        let annotations: Vec<&Annotation> = self.annotations.iter()
            .filter(|annotation| annotation.path == path).collect();
        if annotations.is_empty() {
            return;
        }
        let file_rows = rows.split_off(file_start);
        let placed: HashSet<u64> = annotations.iter().filter(|annotation| {
            file_rows.iter().any(|row| annotation_matches_row(annotation, row))
        }).map(|annotation| annotation.key).collect();
        let annotation_row = |annotation: &Annotation| ChangeRow::Annotation {
            section,
            path: path.to_path_buf(),
            key: annotation.key,
            revision: annotation.revision,
        };
        rows.extend(annotations.iter().filter(|annotation| {
            matches!(annotation.kind, AnnotationKind::Outdated { .. })
        }).map(|annotation| annotation_row(annotation)));
        rows.extend(annotations.iter().filter(|annotation| {
            matches!(annotation.kind, AnnotationKind::Thread { .. }) && !placed.contains(&annotation.key)
        }).map(|annotation| annotation_row(annotation)));
        for row in file_rows {
            let following: Vec<ChangeRow> = annotations.iter()
                .filter(|annotation| annotation_matches_row(annotation, &row))
                .map(|annotation| annotation_row(annotation)).collect();
            rows.push(row);
            rows.extend(following);
        }
    }

    /// Flattens the sections into the list's item stream and tells the
    /// list state about it. Only a changed fingerprint splices (see
    /// [`ListRow::hash_identity`]); a pending keyboard reveal is resolved
    /// here too, because the selected row's index exists only in this
    /// stream.
    fn sync_list_rows(&mut self, sections: Vec<SectionRows>) -> Rc<Vec<ListRow>> {
        let mut rows = Vec::new();
        for section in sections {
            rows.push(ListRow::Header {
                section: section.section,
                count: section.count,
                collapsed: section.collapsed,
            });
            rows.extend(section.rows.into_iter().map(ListRow::Change));
        }
        let mut hasher = DefaultHasher::new();
        rows.len().hash(&mut hasher);
        for row in &rows {
            row.hash_identity(&mut hasher);
        }
        let fingerprint = hasher.finish();
        #[cfg(test)]
        perf_baseline::list_build(rows.len());
        if fingerprint != self.list_fingerprint {
            let old_count = self.list_state.item_count();
            self.list_state.splice(0..old_count, rows.len());
            #[cfg(test)]
            perf_baseline::splice();
            self.list_fingerprint = fingerprint;
        }
        if self.reveal_selected {
            self.reveal_selected = false;
            if let Some((selected_section, selected_path)) = &self.selected_change {
                let index = rows.iter().position(|row| {
                    matches!(
                        row,
                        ListRow::Change(ChangeRow::File { section, entry, .. })
                            if section == selected_section && &entry.path == selected_path
                    )
                });
                if let Some(index) = index {
                    self.list_state.scroll_to_reveal_item(index);
                }
            }
        }
        let reveal = self.reveal_line.clone();
        if let Some((path, side, line)) = reveal.clone() {
            // The line's row exists once the diff is here. A diff that
            // failed, or a path the surface does not list, will never bring
            // it: the reveal is spent on the file's row instead of being
            // held for a frame that never comes.
            let listed = self.entries.iter().any(|entry| entry.path == path);
            let settled = self.diffs.contains_key(&path) || self.diff_errors.contains_key(&path) || !listed;
            if settled {
                self.reveal_line = None;
                if self.reveal_annotation.is_none() {
                    let target = match (side, line) {
                        (AnnotationSide::New, Some(line)) => reveal_target(&rows, &path, line),
                        _ => reveal_anchor_target(&rows, &path, side, line),
                    };
                    if let Some(index) = target {
                        self.scroll_to_row(index);
                    }
                }
            }
        }
        if let Some(key) = self.reveal_annotation {
            let path = reveal.as_ref().map(|(path, _, _)| path).or(self.last_focus.as_ref());
            if let Some(path) = path {
                let listed = self.entries.iter().any(|entry| &entry.path == path);
                let settled = self.diffs.contains_key(path) || self.diff_errors.contains_key(path) || !listed;
                if settled {
                    self.reveal_annotation = None;
                    // A card is shown under the line it follows.
                    let target = reveal_annotation(&rows, key)
                        .map(|index| index.saturating_sub(1))
                        .or_else(|| reveal_anchor_target(&rows, path, AnnotationSide::New, None));
                    if let Some(index) = target {
                        self.scroll_to_row(index);
                    }
                }
            }
        }
        Rc::new(rows)
    }

    /// Puts row `index` at the top of the view. A reveal runs in the rebuild
    /// that spliced the list, when no row has a measured height yet, and
    /// `scroll_to_reveal_item` works from measured heights: it would read
    /// the target as already in view and leave the view where it was.
    fn scroll_to_row(&self, index: usize) {
        self.list_state.scroll_to(gpui::ListOffset { item_ix: index, offset_in_item: px(0.0) });
    }

    fn render_change_row(
        row: ChangeRow,
        unified_x: Pixels,
        split_left_x: Pixels,
        split_right_x: Pixels,
        allows_staging: bool,
        draws_open_diff: bool,
        selected: Option<&(ChangeSection, PathBuf)>,
        forge: Option<Rc<ForgeFiles>>,
        annotation_view: Option<AnyView>,
        open_threads: usize,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> AnyElement {
        match row {
            ChangeRow::File {
                section,
                entry,
                stat,
                drag_payload,
                expanded,
            } => {
                let is_selected = selected == Some(&(section, entry.path.clone()));
                let menu = file_menu(&entry.path, forge.as_deref());
                let id = format!("changes-menu-host-{}-{}", section.slug(), entry.path.display());
                ContextMenu::new(id, menu)
                    .child(Self::render_change_file(
                        section,
                        entry,
                        stat,
                        drag_payload,
                        expanded,
                        allows_staging,
                        draws_open_diff,
                        is_selected,
                        open_threads,
                        entity,
                        theme,
                    ))
                    .into_any_element()
            }
            ChangeRow::Hunk {
                section,
                path,
                header,
            } => div()
                .id(format!(
                    "hunk-{}-{}-{}",
                    section.slug(),
                    path.display(),
                    header
                ))
                // Full width in both modes: a hunk header is context for
                // the whole file, not for one of two columns
                // (F-GIT-DIFF-03's "preserves full-width hunk context").
                .debug_selector(|| "changes-hunk-row".into())
                .h(px(HUNK_ROW_HEIGHT))
                .w_full()
                .flex_none()
                .flex()
                .items_start()
                .gap(px(DIFF_ROW_GAP))
                .px(px(DIFF_ROW_PADDING))
                .py(px(1.0))
                .bg(theme.ely.fg.opacity(0.02))
                .font_family(theme.typography.code_family)
                .text_size(theme.typography.scaled(12.0))
                .line_height(px(18.0))
                .child(
                    div()
                        .w(px(DIFF_HUNK_GUTTER_WIDTH))
                        .flex_none()
                        .text_color(theme.ely.fg_subtle)
                        .child("⋯"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .debug_selector(|| "changes-hunk-header".into())
                        .truncate()
                        .text_color(theme.ely.fg_subtle)
                        .child(header),
                )
                .into_any_element(),
            ChangeRow::ContextBand {
                section,
                path,
                key,
                count,
                expanded,
            } => Self::render_context_band(section, path, key, count, expanded, entity, theme)
                .into_any_element(),
            ChangeRow::Line {
                section,
                path,
                line,
                words,
            } => Self::render_diff_line(section, path, line, words, unified_x, theme)
                .into_any_element(),
            ChangeRow::SplitLine {
                section,
                path,
                key,
                row,
                words,
            } => Self::render_split_line(
                section,
                path,
                key,
                row,
                words,
                split_left_x,
                split_right_x,
                theme,
            )
            .into_any_element(),
            ChangeRow::Unavailable {
                section,
                path,
                message,
                failed,
            } => {
                let retry_path = path.clone();
                let retry_entity = entity.clone();
                div()
                    .id(format!("diff-unavailable-{}-{}", section.slug(), path.display()))
                    .debug_selector(|| "changes-diff-unavailable".into())
                    .w_full()
                    .flex_none()
                    .px(px(DIFF_ROW_PADDING))
                    .py(px(6.0))
                    .child(
                        Callout::new(if failed { Severity::Danger } else { Severity::Info })
                            .title(if failed { "Could not load the diff" } else { "Binary file" })
                            .child(selectable_text(message))
                            .when(failed, |callout| {
                                callout.child(row_button(
                                    format!("diff-retry-{}-{}", section.slug(), path.display()),
                                    "changes-diff-retry",
                                    "Retry",
                                    move |_, cx| {
                                        retry_entity
                                            .update(cx, |tab, cx| tab.retry_diff(retry_path.clone(), cx));
                                    },
                                ))
                            }),
                    )
                    .into_any_element()
            }
            ChangeRow::Annotation { .. } => match annotation_view {
                Some(view) => div()
                    .w_full()
                    .pl(px(DIFF_ROW_PADDING))
                    .py(px(4.0))
                    .child(view)
                    .into_any_element(),
                None => div().into_any_element(),
            },
        }
    }

    /// A collapsed run of unchanged context lines, labelled with its own
    /// size (orca's "18 hidden lines"). Clicking expands it in place.
    fn render_context_band(
        section: ChangeSection,
        path: PathBuf,
        key: usize,
        count: usize,
        expanded: bool,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let band_entity = entity.clone();
        let band_path = path.clone();
        let toggle_entity = entity.clone();
        let toggle_path = path.clone();
        let label = if count == 1 {
            "1 hidden line".to_owned()
        } else {
            format!("{count} hidden lines")
        };
        let path_for_id = path.display().to_string();
        div()
            .id(format!("band-{}-{path_for_id}-{key}", section.slug()))
            .debug_selector(|| "changes-context-band".into())
            .h(px(BAND_ROW_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            // A meta row ("N context lines"), not code: sidebar face.
            .font_family(theme.typography.ui_family)
            .text_size(theme.typography.scaled(12.0))
            .text_color(theme.ely.fg_subtle)
            .bg(theme.ely.fg.opacity(0.02))
            .hover(|style| style.bg(theme.ely.hover))
            .on_click(move |_, _, cx| {
                band_entity.update(cx, |tab, cx| {
                    tab.toggle_band(section, band_path.clone(), key, cx);
                });
            })
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.ely.border))
            .child(div().text_color(theme.ely.fg_subtle).child(label))
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.ely.border))
            .child(row_icon_button(
                format!("band-toggle-{}-{path_for_id}-{key}", section.slug()),
                "changes-context-band-toggle",
                if expanded { IconName::ChevronUp } else { IconName::ChevronsUpDown },
                if expanded { "Hide lines" } else { "Show lines" },
                move |_, cx| {
                    toggle_entity.update(cx, |tab, cx| {
                        tab.toggle_band(section, toggle_path.clone(), key, cx);
                    });
                },
            ))
    }

    /// The collapsible header of one section, stating its size the way the
    /// collapsed-context bands do ("N hidden lines") rather than leaving the
    /// reader to count. Clicking collapses or re-expands the section,
    /// remembering the choice across refreshes.
    fn render_section_header(
        section: ChangeSection,
        label: &'static str,
        count: usize,
        collapsed: bool,
        allows_staging: bool,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let entity_for_toggle = entity.clone();
        let entity_for_action = entity.clone();
        let action_label = section.batch_action_label();
        let action_id = format!("changes-section-{}-all", section.slug());
        div()
            .id(format!("changes-section-{}", section.slug()))
            .debug_selector(|| "changes-section-header".into())
            .h(px(BAND_ROW_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(DIFF_ROW_GAP))
            // Section chrome ("Staged" + count + batch action), not code: the
            // sidebar face, like every other section heading in the app.
            .font_family(theme.typography.ui_family)
            .text_size(theme.typography.scaled(12.0))
            .bg(theme.ely.fg.opacity(0.02))
            .hover(|style| style.bg(theme.ely.hover))
            .on_click(move |_, _, cx| {
                entity_for_toggle.update(cx, |tab, cx| tab.toggle_section(section, cx));
            })
            .child(
                div().w(px(10.0)).flex().items_center().justify_center().child(
                    EIcon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                        .size(EIconSize::Xs)
                        .color(theme.ely.fg_muted),
                ),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.ely.fg_muted)
                    .child(label),
            )
            .child(CountBadge::new(format!("changes-section-count-{}", section.slug()), count))
            .child(div().flex_1())
            // A commit view renders no stage/unstage batch action either:
            // the header keeps its collapse toggle but not the mutation.
            .when(allows_staging, |this| {
                this.child(row_icon_button(
                    action_id,
                    match section {
                        ChangeSection::Staged => "changes-section-staged-all",
                        ChangeSection::Changed => "changes-section-changed-all",
                        ChangeSection::Untracked => "changes-section-untracked-all",
                    },
                    match section {
                        ChangeSection::Staged => IconName::Minus,
                        ChangeSection::Changed | ChangeSection::Untracked => IconName::Plus,
                    },
                    action_label,
                    move |_, cx| {
                        entity_for_action.update(cx, |tab, cx| tab.section_action(section, cx));
                    },
                ))
            })
    }

    fn render_change_file(
        section: ChangeSection,
        entry: StatusEntry,
        stat: Option<DiffStat>,
        drag_payload: Option<DiffPayload>,
        expanded: bool,
        allows_staging: bool,
        draws_open_diff: bool,
        is_selected: bool,
        open_threads: usize,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let path = entry.path.clone();
        let path_for_toggle = path.clone();
        let path_for_stage = path.clone();
        let path_for_discard = path.clone();
        // The action acts on the side of the split this row belongs to: a
        // Staged row unstages (index side), a Changed or Untracked row
        // stages (worktree side).
        let unstages = section == ChangeSection::Staged;
        let stage_label = section.action_label();
        let color = status_color(&entry, theme);
        let status = git_status(section, &entry);
        let entity_for_toggle = entity.clone();
        let entity_for_stage = entity.clone();
        let entity_for_discard = entity.clone();
        let entity_for_open = entity.clone();
        let entity_for_open_diff = entity.clone();
        let entity_for_resolve = entity.clone();
        let open_diff_path = path.clone();
        let conflict_path = path.clone();
        div()
            .id(format!("change-{}-{}", section.slug(), path.display()))
            .debug_selector(|| "changes-file-row".into())
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(DIFF_ROW_PADDING))
            .flex()
            .items_center()
            .gap(px(DIFF_ROW_GAP))
            .border_b_1()
            .border_color(theme.ely.border)
            // The row and its action buttons (Discard / Unstage / Open
            // diff) match the left sidebar's family, not the code face:
            // they are chrome, and the path is a label, not content.
            .font_family(theme.typography.ui_family)
            .text_size(theme.typography.scaled(12.0))
            // The path is neutral text — the badge and the stat carry the status.
            .text_color(theme.ely.fg)
            .hover(|style| style.bg(theme.ely.hover))
            // #325: the keyboard selection has to be visible, or up/down
            // move something the user cannot see.
            .when(is_selected, |this| this.bg(theme.ely.hover))
            .when_some(drag_payload, |this, payload| {
                this.on_drag(payload, move |_, _, _, cx| {
                    cx.new(|_| DiffDragPreview { theme })
                })
            })
            .on_click(move |_, _, cx| {
                entity_for_toggle.update(cx, |tab, cx| {
                    // Clicking a row is also how it becomes the row Enter
                    // acts on -- otherwise mouse and keyboard would track
                    // two different "current" rows.
                    tab.select_change((section, path_for_toggle.clone()), cx);
                    tab.toggle_change(section, &path_for_toggle, cx)
                });
            })
            .child(
                div().w(px(10.0)).flex().items_center().justify_center().child(
                    EIcon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                        .size(EIconSize::Xs)
                        .color(theme.ely.fg_muted),
                ),
            )
            .child(
                div()
                    .w(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(EIcon::new(IconName::File).size(EIconSize::Sm).color(color)),
            )
            .child(GitStatusBadge::new(
                format!("changes-status-{}-{}", section.slug(), path.display()),
                status,
            ))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path.to_string_lossy().to_string()),
            )
            // Known text counts are Ely's stat; unknown or binary counts stay
            // `·`, never a confident +0 −0 (the honesty contract).
            .child(match stat {
                Some(stat) if !stat.is_binary => {
                    EDiffStat::new(stat.additions, stat.deletions).into_any_element()
                }
                _ => div().text_color(theme.ely.fg_subtle).child("·").into_any_element(),
            })
            .when(open_threads > 0, |this| {
                this.child(EIcon::new(IconName::MessageSquare).size(EIconSize::Sm))
                    .child(CountBadge::new(
                        format!("changes-threads-{}", entry.path.display()),
                        open_threads,
                    ))
            })
            .when(expanded, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        // A commit view drops the Discard/Stage pair: the
                        // row keeps its diff navigation, but nothing to
                        // mutate.
                        .when(allows_staging, |this| {
                            this.child(row_button(
                                format!("discard-{}-{}", section.slug(), path.display()),
                                "changes-discard",
                                "Discard",
                                move |_, cx| {
                                    entity_for_discard.update(cx, |tab, cx| {
                                        tab.ask_discard(path_for_discard.clone(), cx);
                                    });
                                },
                            ))
                            .child(row_button(
                                format!("stage-{}-{}", section.slug(), path.display()),
                                if unstages { "changes-unstage" } else { "changes-stage" },
                                stage_label,
                                move |_, cx| {
                                    entity_for_stage.update(cx, |tab, cx| {
                                        if unstages {
                                            tab.unstage_path(path_for_stage.clone(), cx);
                                        } else {
                                            tab.stage_path(path_for_stage.clone(), cx);
                                        }
                                    });
                                },
                            ))
                        })
                        // Only where the event can act: from the tab itself
                        // it reveals the tab you are already in, and expands
                        // the row it is drawn inside (#217).
                        .when(draws_open_diff, |this| {
                            this.child(row_button(
                                format!("changes-open-diff-{}-{}", section.slug(), path.display()),
                                "changes-open-diff",
                                "Open diff",
                                move |_, cx| {
                                    entity_for_open_diff.update(cx, |_, cx| {
                                        cx.emit(ChangesTabActionEvent::OpenDiff(
                                            open_diff_path.clone(),
                                        ));
                                    });
                                },
                            ))
                        })
                        .when(entry.is_conflicted(), |this| {
                            this.child(row_button(
                                format!("resolve-{}-{}", section.slug(), path.display()),
                                "changes-resolve",
                                "Resolve in terminal",
                                move |_, cx| {
                                    entity_for_resolve.update(cx, |_, cx| {
                                        cx.emit(ChangesTabActionEvent::ResolveInTerminal(
                                            conflict_path.clone(),
                                        ));
                                    });
                                },
                            ))
                        })
                        .child(row_icon_button(
                            format!("open-{}-{}", section.slug(), path.display()),
                            "changes-open-file",
                            IconName::SquarePen,
                            "Open file",
                            move |_, cx| {
                                // `entry.path` is repo-relative, and the host
                                // opens an editor tab straight from whatever
                                // this event carries — a relative path made
                                // `FileView` resolve against the process CWD
                                // and render "This file does not exist" for a
                                // file that plainly does. Resolve against this
                                // surface's own root, the way the Files tree
                                // already emits absolute paths.
                                entity_for_open.update(cx, |tab, cx| {
                                    let absolute = tab.repo_root.join(&path);
                                    cx.emit(ChangesTabEvent::OpenFile(absolute));
                                });
                            },
                        )),
                )
            })
    }

    /// One side-by-side row. The two halves are laid out as equal flex
    /// children of the row, so every row in the surface has its columns in
    /// the same place regardless of what any single line contains — the
    /// alignment that makes a split diff readable at all. The horizontal
    /// room the longest line needs is reserved once, on the list's content
    /// wrapper (`render_body`), never per row.
    ///
    /// A half that is `None` is a genuine absence — a deletion run longer
    /// than the addition run it was zipped with — and renders as empty
    /// ground rather than as an empty *line*, so "there is nothing on this
    /// side" and "this side is a blank line" stay distinguishable.
    fn render_split_line(
        section: ChangeSection,
        path: PathBuf,
        key: usize,
        row: DiffSideBySideRow,
        words: (Option<Range<usize>>, Option<Range<usize>>),
        left_x: Pixels,
        right_x: Pixels,
        theme: Theme,
    ) -> impl IntoElement {
        let shape = split_row_shape(&row);
        div()
            .id(format!("split-{}-{}-{key}", section.slug(), path.display()))
            // The selector names the row's *shape*, so the four cases the
            // clause enumerates — paired context, a zipped replacement, a
            // deletion run with nothing opposite it, an addition run with
            // nothing opposite it — are each assertable in a drawn frame
            // rather than only in the model behind it.
            .debug_selector(move || shape.to_owned())
            .h(px(DIFF_LINE_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_stretch()
            .font_family(theme.typography.code_family)
            .text_size(theme.typography.scaled(12.0))
            .child(
                split_cell(row.left, words.0, true, left_x, theme)
                    .debug_selector(|| "changes-split-left".into()),
            )
            .child(
                div()
                    .w(px(SPLIT_DIVIDER_WIDTH))
                    .flex_none()
                    .bg(theme.ely.border),
            )
            .child(
                split_cell(row.right, words.1, false, right_x, theme)
                    .debug_selector(|| "changes-split-right".into()),
            )
    }

    fn render_diff_line(
        section: ChangeSection,
        path: PathBuf,
        line: DiffLine,
        words: Option<Range<usize>>,
        unified_x: Pixels,
        theme: Theme,
    ) -> impl IntoElement {
        let colors = &theme.ely;
        let (background, marker_color, marker, text_color, word_wash) = match line.origin {
            DiffOrigin::Context => (colors.bg, colors.fg_subtle, " ", colors.fg_muted, None),
            DiffOrigin::Addition => (
                colors.success.opacity(LINE_WASH),
                colors.success,
                "+",
                colors.fg,
                Some(colors.success.opacity(WORD_WASH)),
            ),
            DiffOrigin::Deletion => (
                colors.danger.opacity(LINE_WASH),
                colors.danger,
                "-",
                colors.fg,
                Some(colors.danger.opacity(WORD_WASH)),
            ),
        };
        div()
            .id(format!(
                "line-{}-{}-{}-{}",
                section.slug(),
                path.display(),
                line.old_line_number.unwrap_or(0),
                line.new_line_number.unwrap_or(0)
            ))
            .debug_selector(|| "changes-diff-line".into())
            .h(px(DIFF_LINE_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_start()
            .gap(px(DIFF_ROW_GAP))
            .px(px(DIFF_ROW_PADDING))
            .py(px(1.0))
            .font_family(theme.typography.code_family)
            .text_size(theme.typography.scaled(12.0))
            .line_height(px(18.0))
            .bg(background)
            .child(
                div()
                    .debug_selector(|| "changes-diff-old-number".into())
                    .w(px(DIFF_NUMBER_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_color(theme.ely.fg_subtle)
                    .child(
                        line.old_line_number
                            .map_or(String::new(), |n| n.to_string()),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "changes-diff-new-number".into())
                    .w(px(DIFF_NUMBER_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_color(theme.ely.fg_subtle)
                    .child(
                        line.new_line_number
                            .map_or(String::new(), |n| n.to_string()),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "changes-diff-marker".into())
                    .w(px(DIFF_SIGN_WIDTH))
                    .flex_none()
                    .text_color(marker_color)
                    .child(marker),
            )
            .child(
                // `h_full`: with only an absolutely positioned child this box is
                // zero tall (`items_start` stops it stretching), and its
                // `overflow_hidden` then clips the code away and leaves
                // nothing under the pointer to select. The split cells get
                // their height from `h_full` the same way.
                div().flex_1().min_w(px(0.0)).h_full().relative().overflow_hidden().child(
                    div()
                        .debug_selector(|| "changes-diff-content".into())
                        .absolute()
                        .left(unified_x)
                        .whitespace_nowrap()
                        .text_color(text_color)
                        // Only the code is selectable — not the gutter
                        // numbers or the marker — so what a reviewer copies
                        // is the line as it reads in the file. Each row has
                        // an id of its own, which is what keeps two identical
                        // lines (a closing brace) from selecting together.
                        .child(washed_text(line.content, words, word_wash)),
                ),
            )
    }

    /// Sets the app-wide diff view mode (F-GIT-DIFF-03's door). No-ops on
    /// an unchanged value so clicking the segment you are already on does
    /// not schedule a repaint.
    fn set_view_mode(&mut self, mode: DiffViewMode, cx: &mut Context<Self>) {
        if DiffViewMode::get(cx) == mode {
            return;
        }
        DiffViewMode::set(mode, cx);
        self.unified_mode = mode;
        self.unified_width_dirty = true;
        self.unified_x = px(0.0);
        self.unified_max_width = px(0.0);
        cx.notify();
    }

    fn render_toolbar(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        mode: DiffViewMode,
    ) -> impl IntoElement {
        let stage_entity = entity.clone();
        let discard_entity = entity.clone();
        let expand_entity = entity.clone();
        let collapse_entity = entity.clone();
        let refresh_entity = entity.clone();
        let mode_entity = entity.clone();
        // While git is broken the count is stale or unknown; saying so beats
        // a confident number next to an error panel.
        let title = if self.git_error.is_some() {
            "Local changes (unavailable)".to_string()
        } else {
            format!("Local changes ({})", self.entries.len())
        };
        div()
            .h(px(TOOLBAR_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            // The sidebar cannot fit the full toolbar on one line. Give its
            // title a row and let the actions wrap as the panel is resized.
            .when(self.embedded_in_panel, |bar| {
                bar.h_auto().flex_wrap().py(px(4.0)).gap(px(4.0))
            })
            .border_b_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .flex_1()
                    .when(self.embedded_in_panel, |title| title.flex_none().w_full())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(theme.typography.scaled(12.5))
                    .text_color(theme.ely.fg)
                    .child(title),
            )
            // The view mode heads the action cluster: it changes how the
            // whole surface reads, like Expand All beside it. One choice of
            // two; pressing the chosen face again empties Ely's selection,
            // which keeps the mode.
            .child(
                div()
                    .id("changes-view-mode")
                    .debug_selector(|| "changes-view-mode".into())
                    .flex_none()
                    .child(
                        ToggleGroup::new("changes-view-mode-group")
                            .size(ControlSize::Sm)
                            .item(ToggleItem::new("unified").icon(IconName::Rows2).tooltip("Unified"))
                            .item(ToggleItem::new("split").icon(IconName::Columns2).tooltip("Split"))
                            .selected([mode.name()])
                            .on_change(move |selected, _, cx| {
                                let Some(mode) = selected.first().and_then(|value| DiffViewMode::parse(value))
                                else {
                                    return;
                                };
                                mode_entity.update(cx, |tab, cx| tab.set_view_mode(mode, cx));
                            }),
                    ),
            )
            // Refresh is a button and nothing else: it never turns into a
            // spinner while a snapshot loads (a refresh runs every second),
            // and a click during one is a no-op by `refresh`'s own
            // single-flight guard.
            .child(crate::ely_ui::icon_button(
                "changes-refresh",
                IconName::RefreshCw,
                "Refresh",
                true,
                move |_, cx| refresh_entity.update(cx, |tab, cx| tab.refresh(cx)),
            ))
            .child(crate::ely_ui::icon_button(
                "changes-expand-all",
                IconName::Maximize2,
                "Expand All",
                true,
                move |_, cx| expand_entity.update(cx, |tab, cx| tab.expand_all(cx)),
            ))
            .child(crate::ely_ui::icon_button(
                "changes-collapse-all",
                IconName::Minimize2,
                "Collapse All",
                true,
                move |_, cx| collapse_entity.update(cx, |tab, cx| tab.collapse_all(cx)),
            ))
            // The git mutations only exist for a mutable checkout: a commit
            // view renders no Stage/Discard controls at all.
            .when(self.allows_staging(), |this| {
                this.child(crate::ely_ui::icon_button(
                    "changes-stage-all",
                    IconName::Plus,
                    "Stage all",
                    true,
                    move |_, cx| {
                        stage_entity.update(cx, |tab, cx| tab.start_operation(stage_all, cx));
                    },
                ))
                .child(crate::ely_ui::icon_button(
                    "changes-discard-all",
                    IconName::Undo2,
                    "Discard all",
                    true,
                    move |_, cx| {
                        discard_entity.update(cx, |tab, cx| tab.ask_discard_all(cx));
                    },
                ))
            })
    }
}

impl EventEmitter<ChangesTabEvent> for ChangesTab {}
impl EventEmitter<ChangesTabActionEvent> for ChangesTab {}

/// Whether a drawn code row carries this annotation's path and line.
fn annotation_matches_row(annotation: &Annotation, row: &ChangeRow) -> bool {
    let (path, old, new) = match row {
        ChangeRow::Line { path, line, .. } => (path, line.old_line_number, line.new_line_number),
        ChangeRow::SplitLine { path, row, .. } => (
            path,
            row.left.as_ref().and_then(|line| line.old_line_number),
            row.right.as_ref().and_then(|line| line.new_line_number),
        ),
        _ => return false,
    };
    path == &annotation.path && matches_row(
        annotation,
        old.and_then(|number| u32::try_from(number).ok()),
        new.and_then(|number| u32::try_from(number).ok()),
    )
}

fn reveal_annotation(rows: &[ListRow], key: u64) -> Option<usize> {
    rows.iter().position(|row| matches!(row,
        ListRow::Change(ChangeRow::Annotation { key: row_key, .. }) if *row_key == key))
}

/// The new-side line's row, or the file header when the line is absent.
fn reveal_target(rows: &[ListRow], path: &Path, line: usize) -> Option<usize> {
    reveal_anchor_target(rows, path, AnnotationSide::New, Some(line))
}

fn reveal_anchor_target(
    rows: &[ListRow], path: &Path, side: AnnotationSide, line: Option<usize>,
) -> Option<usize> {
    rows.iter()
        .position(|row| match row {
            ListRow::Change(ChangeRow::Line { path: row_path, line: row_line, .. }) => {
                let number = match side {
                    AnnotationSide::Old => row_line.old_line_number,
                    AnnotationSide::New => row_line.new_line_number,
                };
                row_path == path && line.is_some() && number == line
            }
            ListRow::Change(ChangeRow::SplitLine { path: row_path, row, .. }) => {
                let number = match side {
                    AnnotationSide::Old => row.left.as_ref().and_then(|line| line.old_line_number),
                    AnnotationSide::New => row.right.as_ref().and_then(|line| line.new_line_number),
                };
                row_path == path && line.is_some() && number == line
            }
            _ => false,
        })
        .or_else(|| {
            rows.iter().position(|row| {
                matches!(row, ListRow::Change(ChangeRow::File { entry, .. }) if entry.path == path)
            })
        })
}

/// Appends one contiguous run of unified diff lines in the requested view
/// mode.
///
/// Unified emits the lines as they came. Split hands the run to
/// `sirio_git::GitDiffSideBySide::rows_from_lines` — the tested model
/// behind F-GIT-DIFF-03 — which pairs context onto both sides, zips the
/// run's deletions against its additions, and pads the shorter side with
/// `None`. Metadata never reaches here: it is not a member of `DiffLine`
/// at all, so the clause's "omits metadata lines" holds by construction of
/// the parser, not by a filter here.
fn push_segment(
    rows: &mut Vec<ChangeRow>,
    section: ChangeSection,
    path: &Path,
    lines: &[DiffLine],
    mode: DiffViewMode,
    split_key: &mut usize,
) {
    match mode {
        DiffViewMode::Unified => {
            let words = segment_words(lines);
            for (line, words) in lines.iter().zip(words) {
                rows.push(ChangeRow::Line {
                    section,
                    path: path.to_path_buf(),
                    line: line.clone(),
                    words,
                });
            }
        }
        DiffViewMode::Split => {
            for row in GitDiffSideBySide::rows_from_lines(lines) {
                let words = match (&row.left, &row.right) {
                    (Some(left), Some(right))
                        if left.origin == DiffOrigin::Deletion
                            && right.origin == DiffOrigin::Addition =>
                    {
                        changed_span(&left.content, &right.content)
                            .map_or((None, None), |(old, new)| (Some(old), Some(new)))
                    }
                    _ => (None, None),
                };
                rows.push(ChangeRow::SplitLine {
                    section,
                    path: path.to_path_buf(),
                    key: *split_key,
                    row,
                    words,
                });
                *split_key += 1;
            }
        }
    }
}

/// The four shapes a side-by-side row can take, as a stable selector.
///
/// These are exactly the cases F-GIT-DIFF-03 enumerates: a context line is
/// *paired* onto both sides; a deletion run zipped against an addition run
/// makes replacement rows; and whichever run was longer leaves rows with
/// one side only — the padding that keeps the two files in step.
fn split_row_shape(row: &DiffSideBySideRow) -> &'static str {
    match (&row.left, &row.right) {
        (Some(left), Some(right))
            if left.origin == DiffOrigin::Context && right.origin == DiffOrigin::Context =>
        {
            "changes-split-pair-context"
        }
        (Some(_), Some(_)) => "changes-split-pair-replacement",
        (Some(_), None) => "changes-split-left-only",
        (None, Some(_)) => "changes-split-right-only",
        // The model never emits one: `flush` pushes a row only while at
        // least one of the two runs still has an element.
        (None, None) => "changes-split-empty",
    }
}

/// One half of a side-by-side row: the file's own line number, then the
/// line. `old` selects which of the two line numbers this side shows — the
/// left column is the old file, the right column the new one — which is
/// what makes a paired context row show *both* numbers across the row while
/// a zipped deletion/addition pair shows one on each side.
fn split_cell(
    line: Option<DiffSideBySideLine>,
    words: Option<Range<usize>>,
    old: bool,
    side_x: Pixels,
    theme: Theme,
) -> gpui::Div {
    // `relative` + `overflow_hidden` with an absolutely positioned interior is
    // the whole trick, and it is load-bearing rather than stylistic.
    //
    // A flex item's intrinsic width contribution is derived from its content,
    // and a diff line is one unbreakable token as far as layout is concerned.
    // With the line as an ordinary child, a 276-character line made this cell
    // ask for ~1900px, the row asked for two of those, and the row won: driven
    // on the 1715×972 lane the left column measured 975px inside a 980px pane
    // and the right column started at 1307 — straight over the top of the
    // Files panel, which vanished, taking the Changes toolbar's own action
    // cluster with it and leaving no Unified segment on screen to escape by
    // (/tmp/n1-fix/07-06-split-settled.png, sampled: the deletion wash runs
    // unbroken from x=330 to x=1305). Neither `min_w(px(0.0))` nor deleting
    // the width reservation altogether changed that, because both address the
    // *minimum* size and this is the *maximum* one.
    //
    // An absolutely positioned child contributes nothing to its parent's
    // intrinsic size at all. So the cell is sized purely by the flex share it
    // is given — exactly half the row, every row, down the whole diff — and
    // the line is painted inside it and clipped at its edge.
    let cell = div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .relative()
        .overflow_hidden();
    let Some(line) = line else {
        // No content on this side of the zip: the deletion run and the
        // addition run it was paired against had different lengths, and this
        // is the padding that keeps the two files in step.
        //
        // Painted with the recessed `inset` fill rather than left as the
        // surface's own ground, because "this file has no line here" and
        // "this file has a blank line here" must not look the same. A blank
        // *line* keeps its gutter number on the plain background; padding
        // has no number and a recessed ground. Leaving it unpainted made
        // the zip's own padding — half of what F-GIT-DIFF-03 asks the
        // renderer to show — invisible in a photograph.
        return cell.bg(theme.ely.sunken);
    };
    let colors = &theme.ely;
    let (background, marker_color, marker, word_wash) = match line.origin {
        DiffOrigin::Context => (colors.bg, colors.fg_subtle, " ", None),
        DiffOrigin::Addition => (
            colors.success.opacity(LINE_WASH),
            colors.success,
            "+",
            Some(colors.success.opacity(WORD_WASH)),
        ),
        DiffOrigin::Deletion => (
            colors.danger.opacity(LINE_WASH),
            colors.danger,
            "-",
            Some(colors.danger.opacity(WORD_WASH)),
        ),
    };
    let number = if old {
        line.old_line_number
    } else {
        line.new_line_number
    };
    cell.bg(background).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            .gap(px(DIFF_ROW_GAP))
            .px(px(DIFF_ROW_PADDING))
            .py(px(1.0))
            .line_height(px(18.0))
            .child(
                div()
                    .w(px(DIFF_NUMBER_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_color(theme.ely.fg_subtle)
                    .child(number.map_or(String::new(), |number| number.to_string())),
            )
            .child(
                div()
                    .w(px(DIFF_SIGN_WIDTH))
                    .flex_none()
                    .text_color(marker_color)
                    .child(marker),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .relative()
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(move || {
                                if old {
                                    "changes-split-old-content".into()
                                } else {
                                    "changes-split-new-content".into()
                                }
                            })
                            .absolute()
                            .left(side_x)
                            .whitespace_nowrap()
                            .text_color(theme.ely.fg)
                            // The two sides of a context line are the same
                            // text in the same row, so each needs its own
                            // identity or selecting one would select both.
                            .child(
                                washed_text(line.content, words, word_wash)
                                    .id(if old { "split-old" } else { "split-new" }),
                            ),
                    ),
            ),
    )
}

fn anchored_indices(lines: &[DiffLine], old: &HashSet<u32>, new: &HashSet<u32>) -> Vec<usize> {
    if old.is_empty() && new.is_empty() {
        return Vec::new();
    }
    lines.iter().enumerate().filter_map(|(index, line)| {
        let anchored = line.old_line_number
            .and_then(|number| u32::try_from(number).ok())
            .is_some_and(|number| old.contains(&number))
            || line.new_line_number
                .and_then(|number| u32::try_from(number).ok())
                .is_some_and(|number| new.contains(&number));
        anchored.then_some(index)
    }).collect()
}

/// The collapsed-context runs of one diff, as `(key, count)` pairs — the
/// same walk `expand_diff` performs when rendering, so Expand All and the
/// render can never disagree about which bands exist. `key` is the run's
/// first line's position in the file's flattened line stream.
fn context_band_keys(diff: &FileDiff, annotations: &[Annotation]) -> Vec<(usize, usize)> {
    let (old, new) = anchored_lines(annotations, &diff.path);
    let mut keys = Vec::new();
    let mut line_index = 0usize;
    for hunk in &diff.hunks {
        let mut i = 0usize;
        while i < hunk.lines.len() {
            if hunk.lines[i].origin == DiffOrigin::Context {
                let start = i;
                while i < hunk.lines.len() && hunk.lines[i].origin == DiffOrigin::Context {
                    i += 1;
                }
                let indices = anchored_indices(&hunk.lines[start..i], &old, &new);
                for piece in band_pieces(i - start, &indices, CONTEXT_BAND_MIN) {
                    if piece.band {
                        keys.push((line_index + start + piece.start, piece.end - piece.start));
                    }
                }
            } else {
                i += 1;
            }
        }
        line_index += hunk.lines.len();
    }
    keys
}

impl ChangesTab {
    /// The area below the toolbar. Three mutually exclusive states:
    ///
    /// 1. a git error — the surface shows the error with git's own message
    ///    and a Retry, replacing any (stale) list. A broken repo must look
    ///    broken, not clean (F-CHG-09);
    /// 2. the first load in flight with nothing to show yet — "Loading…";
    /// 3. the sections list.
    fn render_body(
        &mut self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        mode: DiffViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(test)]
        perf_baseline::render_body();
        if self.unified_mode != mode {
            self.unified_mode = mode;
            self.unified_width_dirty = true;
            self.unified_x = px(0.0);
            self.unified_max_width = px(0.0);
        }
        if let Some(error) = &self.git_error {
            return Self::render_error_state(error, entity, theme).into_any_element();
        }
        if !self.has_loaded && self.entries.is_empty() {
            return div()
                .id("changes-loading")
                .debug_selector(|| "changes-loading".into())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .children((0..6usize).map(|ix| {
                    div()
                        .h(px(ROW_HEIGHT))
                        .px(px(DIFF_ROW_PADDING))
                        .flex()
                        .items_center()
                        .gap(px(DIFF_ROW_GAP))
                        .border_b_1()
                        .border_color(theme.ely.border)
                        .child(Skeleton::new(("changes-skeleton-icon", ix)).size(px(14.0)))
                        .child(
                            Skeleton::new(("changes-skeleton-path", ix))
                                .h(px(10.0))
                                .w(gpui::relative(0.5)),
                        )
                        .child(div().flex_1())
                        .child(Skeleton::new(("changes-skeleton-stat", ix)).h(px(10.0)).w(px(48.0)))
                }))
                .into_any_element();
        }
        let sections = self.section_rows(mode);
        if sections.is_empty() {
            // F-CHG-02: a clean repo (or a worktree that was just closed and
            // reopened with nothing to show) fell through to an empty
            // "changes-list" with no message -- a blank panel that looks
            // broken rather than confirming there is genuinely nothing to
            // show.
            return div()
                .id("changes-empty")
                .debug_selector(|| "changes-empty".into())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.headline)
                .text_color(theme.ely.fg_muted)
                .child("No changes")
                .into_any_element();
        }
        if self.unified_width_dirty {
            self.unified_max_width = sections
                .iter()
                .flat_map(|section| section.rows.iter())
                .filter_map(|row| match row {
                    ChangeRow::Line { line, .. } => Some(&line.content),
                    _ => None,
                })
                .map(|content| {
                    window
                        .text_system()
                        .layout_line(
                            content,
                            theme.typography.scaled(12.0),
                            &[gpui::TextRun {
                                len: content.len(),
                                font: gpui::font(theme.typography.code_family),
                                color: theme.ely.fg,
                                ..Default::default()
                            }],
                            None,
                        )
                        .width
                })
                .fold(px(0.0), Pixels::max);
            if mode == DiffViewMode::Split {
                let measure_side = |left: bool| {
                    sections
                        .iter()
                        .flat_map(|section| section.rows.iter())
                        .filter_map(|row| match row {
                            ChangeRow::SplitLine { row, .. } => {
                                let line = if left { &row.left } else { &row.right };
                                line.as_ref().map(|line| &line.content)
                            }
                            _ => None,
                        })
                        .map(|content| {
                            window
                                .text_system()
                                .layout_line(
                                    content,
                                    theme.typography.scaled(12.0),
                                    &[gpui::TextRun {
                                        len: content.len(),
                                        font: gpui::font(theme.typography.code_family),
                                        color: theme.ely.fg,
                                        ..Default::default()
                                    }],
                                    None,
                                )
                                .width
                        })
                        .fold(px(0.0), Pixels::max)
                };
                self.split_left_max_width = measure_side(true);
                self.split_right_max_width = measure_side(false);
                self.split_left_x = clamped_x(
                    self.split_left_x,
                    self.split_left_max_width,
                    self.split_viewport,
                );
                self.split_right_x = clamped_x(
                    self.split_right_x,
                    self.split_right_max_width,
                    self.split_viewport,
                );
            }
            self.unified_width_dirty = false;
            self.unified_x = clamped_x(
                self.unified_x,
                self.unified_max_width,
                self.unified_viewport,
            );
        }
        let rows = self.sync_list_rows(sections);
        let row_entity = entity.clone();
        let allows_staging = self.allows_staging();
        let draws_open_diff = self.embedded_in_panel;
        let source = self.source.clone();
        let forge = self.forge_files.clone();
        let views = self.annotation_views.clone();
        let open_threads = self.open_threads.clone();
        let selected = self.selected_change.clone();
        let unified_x = self.unified_x;
        let split_left_x = self.split_left_x;
        let split_right_x = self.split_right_x;
        let unified_viewport = self.unified_viewport;
        let horizontal_bar_state = self.unified_bar_state.clone();
        let viewport_entity = entity.clone();
        let bar_entity = entity.clone();
        let list_focus = self
            .list_focus
            .as_ref()
            .expect("the changes list focus is initialized in render")
            .clone();
        // Both modes render into exactly the width the surface was given —
        // see `SPLIT_DIVIDER_WIDTH` for the two attempts at doing otherwise
        // and what each one cost. `min_w(px(0.0))` stays because a list
        // that cannot shrink below its content is a list that grows its
        // ancestors instead of scrolling, and this one holds arbitrarily
        // long file paths in its section rows. The scrolling itself is the
        // list's: `gpui::list` owns the wheel and draws only the rows in
        // and around the viewport, which is what keeps a frame over a very
        // large diff at viewport cost (see `ChangesTab::list_state`).
        div()
            .id("changes-list")
            .debug_selector(|| "changes-list".into())
            .track_focus(&list_focus)
            .on_key_down(cx.listener(Self::on_change_key))
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .relative()
            .child(
                list(
                    self.list_state.clone(),
                    move |index, _window, _cx| match rows.get(index) {
                        Some(ListRow::Header {
                            section,
                            count,
                            collapsed,
                        }) => Self::render_section_header(
                            *section,
                            section.label_in(&source),
                            *count,
                            *collapsed,
                            allows_staging,
                            row_entity.clone(),
                            theme,
                        )
                        .into_any_element(),
                        Some(ListRow::Change(row)) => Self::render_change_row(
                            {
                                #[cfg(test)]
                                perf_baseline::draw_row_clone(row);
                                row.clone()
                            },
                            unified_x,
                            split_left_x,
                            split_right_x,
                            allows_staging,
                            draws_open_diff,
                            selected.as_ref(),
                            forge.clone(),
                            match row {
                                ChangeRow::Annotation { key, .. } => views.get(key).cloned(),
                                _ => None,
                            },
                            match row {
                                ChangeRow::File { entry, .. } => open_threads.get(&entry.path).copied().unwrap_or(0),
                                _ => 0,
                            },
                            row_entity.clone(),
                            theme,
                        ),
                        None => div().into_any_element(),
                    },
                )
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .flex_1()
                .min_h(px(0.0))
                .w_full(),
            )
            .child(if mode == DiffViewMode::Unified {
                canvas(
                    move |bounds, window, cx| {
                        let viewport = bounds.size.width;
                        if (viewport - unified_viewport).abs() > px(0.5) {
                            viewport_entity.update(cx, |tab, cx| {
                                tab.unified_viewport = viewport;
                                tab.unified_x = clamped_x(
                                    tab.unified_x,
                                    tab.unified_max_width,
                                    viewport,
                                );
                                cx.notify();
                            });
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .left(px(
                    DIFF_NUMBER_WIDTH * 2.0
                        + DIFF_SIGN_WIDTH
                        + 3.0 * DIFF_ROW_GAP
                        + 2.0 * DIFF_ROW_PADDING,
                ))
                .right(px(DIFF_ROW_PADDING))
                .bottom_0()
                .h(px(10.0))
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if mode == DiffViewMode::Unified {
                let viewport = self.unified_viewport.max(px(0.0));
                let overflow = (self.unified_max_width - viewport).max(px(0.0));
                div()
                    .absolute()
                    .left(px(
                        DIFF_NUMBER_WIDTH * 2.0
                            + DIFF_SIGN_WIDTH
                            + 3.0 * DIFF_ROW_GAP
                            + 2.0 * DIFF_ROW_PADDING,
                    ))
                    .right(px(DIFF_ROW_PADDING))
                    .bottom_0()
                    .h(px(10.0))
                    .child(horizontal_scroll::bar(
                        "changes-unified-horizontal-bar",
                        viewport,
                        overflow,
                        unified_x,
                        &horizontal_bar_state,
                        move |offset, cx| {
                            bar_entity.update(cx, |tab, cx| {
                                tab.unified_x = clamped_x(
                                    offset,
                                    tab.unified_max_width,
                                    tab.unified_viewport,
                                );
                                cx.notify();
                            });
                        },
                    ))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if mode == DiffViewMode::Split {
                let viewport_entity = entity.clone();
                let previous_viewport = self.split_viewport;
                canvas(
                    move |bounds, window, cx| {
                        let gutter = DIFF_NUMBER_WIDTH
                            + DIFF_SIGN_WIDTH
                            + 2.0 * DIFF_ROW_GAP
                            + 2.0 * DIFF_ROW_PADDING;
                        let side = ((bounds.size.width - px(SPLIT_DIVIDER_WIDTH)) / 2.0
                            - px(gutter))
                            .max(px(0.0));
                        if (side - previous_viewport).abs() > px(0.5) {
                            viewport_entity.update(cx, |tab, cx| {
                                tab.split_viewport = side;
                                tab.split_left_x = clamped_x(
                                    tab.split_left_x,
                                    tab.split_left_max_width,
                                    side,
                                );
                                tab.split_right_x = clamped_x(
                                    tab.split_right_x,
                                    tab.split_right_max_width,
                                    side,
                                );
                                cx.notify();
                            });
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(10.0))
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if mode == DiffViewMode::Split {
                let side_viewport = self.split_viewport.max(px(0.0));
                let gutter = px(
                    DIFF_NUMBER_WIDTH
                        + DIFF_SIGN_WIDTH
                        + 2.0 * DIFF_ROW_GAP
                        + 2.0 * DIFF_ROW_PADDING,
                );
                let left_state = self.split_left_bar_state.clone();
                let right_state = self.split_right_bar_state.clone();
                let left_entity = entity.clone();
                let right_entity = entity.clone();
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(10.0))
                    .flex()
                    .items_start()
                    .child(div().w(gutter).flex_none())
                    .child(
                        div()
                            .w(side_viewport)
                            .h(px(10.0))
                            .relative()
                            .flex_none()
                            .child(horizontal_scroll::bar(
                                "changes-split-left-horizontal-bar",
                                side_viewport,
                                (self.split_left_max_width - side_viewport).max(px(0.0)),
                                self.split_left_x,
                                &left_state,
                                move |offset, cx| {
                                    left_entity.update(cx, |tab, cx| {
                                        tab.split_left_x = clamped_x(
                                            offset,
                                            tab.split_left_max_width,
                                            tab.split_viewport,
                                        );
                                        cx.notify();
                                    });
                                },
                            )),
                    )
                    .child(div().w(px(SPLIT_DIVIDER_WIDTH) + gutter).flex_none())
                    .child(
                        div()
                            .w(side_viewport)
                            .h(px(10.0))
                            .relative()
                            .flex_none()
                            .child(horizontal_scroll::bar(
                                "changes-split-right-horizontal-bar",
                                side_viewport,
                                (self.split_right_max_width - side_viewport).max(px(0.0)),
                                self.split_right_x,
                                &right_state,
                                move |offset, cx| {
                                    right_entity.update(cx, |tab, cx| {
                                        tab.split_right_x = clamped_x(
                                            offset,
                                            tab.split_right_max_width,
                                            tab.split_viewport,
                                        );
                                        cx.notify();
                                    });
                                },
                            )),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .into_any_element()
    }

    /// The F-CHG-09 error state: git's own message (enough detail to act
    /// on) and a Retry that re-runs the refresh. Pixel-bound like every
    /// other state here, but reachable — the refresh path sets `git_error`
    /// from a real failed git invocation.
    fn render_error_state(
        error: &str,
        entity: gpui::Entity<Self>,
        _theme: Theme,
    ) -> impl IntoElement {
        let retry = entity;
        div()
            .id("changes-error")
            .debug_selector(|| "changes-error".into())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(24.0))
            .child(
                div().w_full().max_w(px(560.0)).child(
                    Callout::new(Severity::Danger)
                        .title("Git is unavailable")
                        .child(selectable_text(error.to_string()))
                        .child(crate::ely_ui::text_button(
                            "changes-retry",
                            "Retry",
                            Some(IconName::RotateCw),
                            crate::ely_ui::ButtonState::IDLE,
                            move |_, cx| retry.update(cx, |tab, cx| tab.retry_refresh(cx)),
                        )),
                ),
            )
    }

    /// The Discard confirmation, drawn over the surface while it is asked.
    fn render_discard_dialog(&self, entity: &gpui::Entity<Self>) -> Option<AnyElement> {
        let ask = self.discard_ask.as_ref()?;
        let (title, body, label) = match ask {
            DiscardAsk::One(path) => (
                "Discard changes?",
                format!(
                    "This will throw away the worktree changes to {}. This cannot be undone.",
                    sirio_project::display_path(path)
                ),
                "Discard",
            ),
            DiscardAsk::All(paths) => (
                "Discard all changes?",
                format!(
                    "This will throw away the worktree changes to:\n{}\n\nThis cannot be undone.",
                    paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                "Discard All",
            ),
        };
        let (close, confirm) = (entity.clone(), entity.clone());
        let dialog = Dialog::new("changes-discard-dialog", title, move |_, cx| {
            close.update(cx, |tab, cx| tab.close_discard(cx))
        })
        .child(selectable_text(body))
        .action(|close| {
            dialog_button("changes-discard-cancel", "Cancel", ButtonVariant::Ghost, move |window, cx| {
                close(window, cx)
            })
        })
        .action(move |close| {
            dialog_button("changes-discard-confirm", label, ButtonVariant::Danger, move |window, cx| {
                // Confirm first: closing runs `close_discard`, which would
                // take the ask before the confirm could read it. Then close
                // like Cancel does, which hands the keyboard back.
                confirm.update(cx, |tab, cx| {
                    tab.confirm_discard(cx);
                });
                close(window, cx);
            })
        });
        Some(
            div()
                .debug_selector(|| "changes-discard-dialog".into())
                .child(dialog)
                .into_any_element(),
        )
    }
}

impl Render for ChangesTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("ChangesTab.render", cx.entity_id().as_u64());
        // Ely draws from its own theme global, which only follows Sirio's
        // appearance when something syncs it; a surface outside the chat
        // must do so itself or stay dark in Light mode.
        crate::ely::sync_theme_if_changed(cx);
        let theme = *Theme::get(cx);
        let theme = if self.embedded_in_panel {
            theme.with_sidebar_typography()
        } else {
            theme
        };
        // #193: incremented here and nowhere else -- being in a drawn frame
        // is the whole signal.
        self.renders = self.renders.wrapping_add(1);
        self.ensure_refresh(cx);
        if self.refresh_suspended {
            self.refresh_suspended = false;
            self.refresh(cx);
        }
        if self.list_focus.is_none() {
            self.list_focus = Some(cx.focus_handle().tab_stop(true));
        }
        let entity = cx.entity();
        let mode = DiffViewMode::get(cx);
        let toolbar = self
            .render_toolbar(entity.clone(), theme, mode)
            .into_any_element();
        let dialog = self.render_discard_dialog(&entity);
        let body = self.render_body(entity, theme, mode, _window, cx);
        div()
            // The surface's own extent, so a drawn test can assert that
            // nothing inside it — notably Split mode's width reservation —
            // makes it wider than the space it was given.
            .debug_selector(|| "changes-surface".into())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.ely.bg)
            .child(toolbar)
            .child(body)
            .children(dialog)
    }
}

/// The status hue for a change row's glyph — the path text stays neutral.
///
/// F-CHG-06: this used to be its own precedence chain, testing
/// `has_worktree_changes()` before `is_staged()` — backwards from Swift's
/// `GitStatusStyle.color` and from the Files tree, so a staged-then-modified
/// file rendered amber here and green there. Both views now resolve through
/// [`crate::git_status_style`]; keep it that way.
fn status_color(entry: &StatusEntry, theme: Theme) -> Hsla {
    crate::git_status_style::entry_color(entry, theme)
}

/// The letter a row's badge shows. The section decides which column of
/// porcelain's two it reads — the index in Staged, the worktree in Changed —
/// because that is the side of the split the row acts on. A commit's or a
/// range's rows carry their status in the index column.
fn git_status(section: ChangeSection, entry: &StatusEntry) -> GitStatus {
    if section == ChangeSection::Untracked || entry.is_untracked() {
        return GitStatus::Untracked;
    }
    if entry.is_conflicted() {
        return GitStatus::Conflicted;
    }
    let kind = match section {
        ChangeSection::Staged => entry.index_status,
        ChangeSection::Changed | ChangeSection::Untracked => entry.worktree_status,
    };
    match kind {
        Some(StatusKind::Added | StatusKind::Copied) => GitStatus::Added,
        Some(StatusKind::Deleted) => GitStatus::Deleted,
        Some(StatusKind::Renamed) => GitStatus::Renamed,
        Some(StatusKind::Unmerged) => GitStatus::Conflicted,
        Some(StatusKind::Untracked) => GitStatus::Untracked,
        Some(StatusKind::Modified | StatusKind::TypeChanged) | None => GitStatus::Modified,
    }
}

/// One file of a change request on its forge. GitHub anchors a file on the
/// *Files changed* page by the SHA-256 of its path; GitLab's page opens at
/// its top, because its per-file anchor is not one Sirio can check here.
fn forge_file_url(files: &ForgeFiles, path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let base = files.web_url.trim_end_matches('/');
    match files.forge {
        sirio_forge::Forge::GitHub => {
            // The forge's path, `/`-separated whatever the platform's is.
            let path = path
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let hex: String = Sha256::digest(path.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            format!("{base}/files#diff-{hex}")
        }
        sirio_forge::Forge::GitLab => format!("{base}/diffs"),
    }
}

/// A file row's right-click menu (B1 §7.1).
fn file_menu(path: &Path, forge: Option<&ForgeFiles>) -> Menu {
    let copy = path.to_string_lossy().into_owned();
    let mut menu = Menu::new().item(
        MenuItem::new("Copy path")
            .icon(IconName::Copy)
            .selectors("changes-menu-copy-path", None)
            .on_click(move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))),
    );
    if let Some(files) = forge {
        let url = forge_file_url(files, path);
        menu = menu.item(
            MenuItem::new("Open on the forge")
                .icon(IconName::ExternalLink)
                .selectors("changes-menu-open-forge", None)
                .on_click(move |_, cx| cx.open_url(&url)),
        );
    }
    menu
}

/// A button of the Discard dialog, findable by its id.
fn dialog_button(
    id: &'static str,
    label: &'static str,
    variant: ButtonVariant,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(
            Button::new(id, label)
                .variant(variant)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}

/// A row's labelled action: a small ghost button. The click stops there, so
/// the row it sits in does not also toggle.
fn row_button(
    id: String,
    selector: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.clone())
        .debug_selector(move || selector.to_owned())
        .flex_none()
        .child(
            Button::new(SharedString::from(format!("{id}-button")), label)
                .size(ControlSize::Sm)
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                }),
        )
        .into_any_element()
}

/// A row's icon action, with its tooltip. Stops the click like `row_button`.
fn row_icon_button(
    id: String,
    selector: &'static str,
    icon: IconName,
    tooltip: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.clone())
        .debug_selector(move || selector.to_owned())
        .flex_none()
        .child(
            IconButton::new(SharedString::from(format!("{id}-button")), icon)
                .size(ControlSize::Sm)
                .tooltip(tooltip)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                }),
        )
        .into_any_element()
}

/// The changed stretch of a replaced line on each side: what lies between
/// the two lines' common prefix and common suffix, widened to whole words.
/// `None` when the lines are equal or share nothing, where the line's own
/// wash already says everything.
fn changed_span(old: &str, new: &str) -> Option<(Range<usize>, Range<usize>)> {
    if old == new {
        return None;
    }
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    if prefix == 0 && suffix == 0 {
        return None;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let widen = |text: &str, start: usize, end: usize| {
        let start = start
            - text[..start]
                .chars()
                .rev()
                .take_while(|c| is_word(*c))
                .map(char::len_utf8)
                .sum::<usize>();
        let end = end
            + text[end..]
                .chars()
                .take_while(|c| is_word(*c))
                .map(char::len_utf8)
                .sum::<usize>();
        start..end
    };
    Some((
        widen(old, prefix, old.len() - suffix),
        widen(new, prefix, new.len() - suffix),
    ))
}

/// The changed words of every line of one deletion/addition run, paired the
/// way `GitDiffSideBySide` zips the run: the n-th deletion with the n-th
/// addition.
fn segment_words(lines: &[DiffLine]) -> Vec<Option<Range<usize>>> {
    let mut words = vec![None; lines.len()];
    let of = |origin| (0..lines.len()).filter(move |&ix| lines[ix].origin == origin);
    for (old, new) in of(DiffOrigin::Deletion).zip(of(DiffOrigin::Addition)) {
        if let Some((old_words, new_words)) = changed_span(&lines[old].content, &lines[new].content) {
            words[old] = Some(old_words);
            words[new] = Some(new_words);
        }
    }
    words
}

/// A line's code as selectable text, its changed words washed.
#[track_caller]
fn washed_text(text: String, words: Option<Range<usize>>, wash: Option<Hsla>) -> SelectableText {
    let text = selectable_text(text);
    match (words, wash) {
        (Some(range), Some(wash)) if !range.is_empty() => text.highlights(vec![(
            range,
            HighlightStyle {
                background_color: Some(wash),
                ..Default::default()
            },
        )]),
        _ => text,
    }
}

/// Convert one parsed file diff into the textual payload a terminal or chat
/// pane can receive. Binary files have no meaningful textual payload and do
/// not advertise a drag source.
fn diff_payload(diff: &FileDiff) -> Option<DiffPayload> {
    #[cfg(test)]
    perf_baseline::payload_call();
    if diff.is_binary {
        return None;
    }
    let mut text = String::new();
    for hunk in &diff.hunks {
        text.push_str(&hunk.header);
        text.push('\n');
        for line in &hunk.lines {
            let marker = match line.origin {
                DiffOrigin::Context => ' ',
                DiffOrigin::Addition => '+',
                DiffOrigin::Deletion => '-',
            };
            text.push(marker);
            text.push_str(&line.content);
            text.push('\n');
        }
    }
    #[cfg(test)]
    perf_baseline::payload_bytes(text.len());
    Some((diff.path.clone(), text))
}

/// Minimal drag preview required by GPUI. The typed payload remains the
/// source of truth; the preview carries no second copy of the diff text.
struct DiffDragPreview {
    theme: Theme,
}

impl Render for DiffDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(self.theme.spacing.titlebar_control_spacing)
            .py(self.theme.spacing.titlebar_control_spacing)
            .rounded(self.theme.radii.control)
            .bg(self.theme.ely.surface)
            .text_color(self.theme.ely.fg)
            .child("Diff")
    }
}

/// Loads the status, per-file counts and per-file diffs for one checkout.
/// Returns `Err` with git's own message when status or the counts fail — the
/// caller surfaces it (F-CHG-09), so a broken repo renders as an error with
/// a Retry, never as a clean 0/0/0. A per-file *diff* failure is recorded
/// per path instead of failing the whole snapshot: the row's counts may
/// still be valid, and the expanded row says "diff unavailable" rather
/// than lying.
/// Loads the snapshot this surface displays: the working tree's status, or
/// one commit's files, depending on the source.
///
/// `expanded_paths` scopes the full-diff fetch for the working tree: `None`
/// fetches every entry's diff (the first load, so the surface has something
/// to show the instant a row is expanded); `Some(paths)` fetches only the
/// diffs for rows actually expanded right now. A commit view ignores it —
/// see `load_commit_snapshot`.
fn load_snapshot(
    repo_root: &Path,
    source: &ChangesSource,
    expanded_paths: Option<&HashSet<PathBuf>>,
) -> Result<GitSnapshot, String> {
    match source {
        ChangesSource::WorkingTree => load_worktree_snapshot(repo_root, expanded_paths),
        ChangesSource::Commit(sha) => load_commit_snapshot(repo_root, sha),
        ChangesSource::Range { base, head } => load_range_snapshot(repo_root, base, head),
    }
}

/// The working-tree snapshot: `git status` entries, per-file stats always,
/// and diffs against HEAD only for `expanded_paths` (`None` means every
/// entry). Untouched by the commit view.
///
/// This is the surface's hot loop: `ensure_refresh` re-runs it every
/// `CHANGES_REFRESH_INTERVAL` for as long as the tab is visible. Fetching
/// every entry's diff unconditionally here once meant one `git diff`
/// subprocess per changed file, every tick — 50+ processes a second on a
/// large agent-driven changeset, for rows nobody had expanded. `stats`
/// stays unconditional: it is one batched call, not one process per file,
/// and a collapsed row's header still shows real +/− counts.
fn load_worktree_snapshot(
    repo_root: &Path,
    expanded_paths: Option<&HashSet<PathBuf>>,
) -> Result<GitSnapshot, String> {
    let entries = status(repo_root)
        .map_err(|error| error.to_string())?
        .entries;
    let stats = stats(repo_root, &entries).map_err(|error| error.to_string())?;
    let mut diffs = HashMap::new();
    let mut diff_errors = HashMap::new();
    for entry in &entries {
        if let Some(expanded_paths) = expanded_paths
            && !expanded_paths.contains(&entry.path)
        {
            continue;
        }
        match diff_entry(repo_root, entry, CHANGES_CONTEXT_LINES) {
            Ok(diff) => {
                diffs.insert(entry.path.clone(), diff);
            }
            Err(error) => {
                diff_errors.insert(entry.path.clone(), error.to_string());
            }
        }
    }
    Ok(GitSnapshot {
        entries,
        diffs,
        stats,
        diff_errors,
    })
}

/// Maps one `git show --name-status` letter onto the status kind the
/// surface buckets by. A letter outside porcelain's set (git's pathological
/// `X` unknown) reads as `Modified` rather than being dropped: a file git
/// itself reports on is never invisible here.
fn commit_status_kind(status: char) -> StatusKind {
    match status {
        'A' => StatusKind::Added,
        'D' => StatusKind::Deleted,
        'R' => StatusKind::Renamed,
        'C' => StatusKind::Copied,
        'T' => StatusKind::TypeChanged,
        'U' => StatusKind::Unmerged,
        _ => StatusKind::Modified,
    }
}

/// The snapshot of one commit: the files it touched, each with the unified
/// diff of that path within the commit and its +/− counts, all read through
/// `git show <sha>`. Counts come from the same `FileDiff` the expansion
/// renders, so they cannot disagree with what the rows show. Every entry
/// reads as staged content — fixed relative to the commit's parent, exactly
/// like the index is fixed relative to the worktree — which keeps the
/// section buckets working, while [`ChangesTab::allows_staging`] refuses
/// the mutations those buckets would otherwise offer.
fn load_commit_snapshot(repo_root: &Path, sha: &str) -> Result<GitSnapshot, String> {
    let files = commit_files(repo_root, sha).map_err(|error| error.to_string())?;
    let mut entries = Vec::with_capacity(files.len());
    let mut diffs = HashMap::new();
    let mut stats = HashMap::new();
    let mut diff_errors = HashMap::new();
    for (status, path) in files {
        entries.push(StatusEntry {
            path: path.clone(),
            original_path: None,
            index_status: Some(commit_status_kind(status)),
            worktree_status: None,
        });
        match commit_diff_entry(repo_root, sha, &path) {
            Ok(diff) => {
                stats.insert(
                    path.clone(),
                    DiffStat {
                        additions: diff.additions,
                        deletions: diff.deletions,
                        is_binary: diff.is_binary,
                    },
                );
                diffs.insert(path.clone(), diff);
            }
            Err(error) => {
                diff_errors.insert(path.clone(), error.to_string());
            }
        }
    }
    Ok(GitSnapshot {
        entries,
        diffs,
        stats,
        diff_errors,
    })
}

/// The snapshot of a change request's range: its files and their counts, from
/// two git processes. No diff is read here — `fetch_expanded_diff` reads one
/// when its row opens. Every entry reads as staged, like a commit's.
fn load_range_snapshot(repo_root: &Path, base: &str, head: &str) -> Result<GitSnapshot, String> {
    let files = range_files(repo_root, base, head).map_err(|error| error.to_string())?;
    let stats = range_stats(repo_root, base, head).map_err(|error| error.to_string())?;
    let entries = files
        .into_iter()
        .map(|file| StatusEntry {
            path: file.path,
            original_path: file.old_path,
            index_status: Some(commit_status_kind(file.status)),
            worktree_status: None,
        })
        .collect();
    Ok(GitSnapshot {
        entries,
        diffs: HashMap::new(),
        stats,
        diff_errors: HashMap::new(),
    })
}

/// One file's diff for whichever source the surface reads.
fn load_expanded_diff(
    repo_root: &Path,
    source: &ChangesSource,
    entry: &StatusEntry,
) -> Result<FileDiff, GitError> {
    match source {
        ChangesSource::WorkingTree => diff_entry(repo_root, entry, CHANGES_CONTEXT_LINES),
        ChangesSource::Commit(sha) => commit_diff_entry(repo_root, sha, &entry.path),
        ChangesSource::Range { base, head } => range_file_diff(
            repo_root,
            base,
            head,
            &entry.path,
            entry.original_path.as_deref(),
            CHANGES_CONTEXT_LINES,
        ),
    }
}

/// The key of the collapsed context band that hides `line` on `side` of
/// `diff`, if one does. Mirrors the walk in `expand_diff`: a run of context
/// lines of at least `CONTEXT_BAND_MIN` is one band, keyed by the position of
/// its first line in the file's flattened line stream.
fn band_key_containing(diff: &FileDiff, side: AnnotationSide, line: usize) -> Option<usize> {
    band_key_containing_annotations(diff, &[], side, line)
}

fn band_key_containing_annotations(
    diff: &FileDiff, annotations: &[Annotation], side: AnnotationSide, line: usize,
) -> Option<usize> {
    let index = diff.hunks.iter().flat_map(|hunk| &hunk.lines).position(|candidate| match side {
        AnnotationSide::Old => candidate.old_line_number == Some(line),
        AnnotationSide::New => candidate.new_line_number == Some(line),
    })?;
    context_band_keys(diff, annotations).into_iter()
        .find(|(key, count)| *key <= index && index - key < *count)
        .map(|(key, _)| key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use sirio_git::StatusKind;
    use sirio_git::{DiffLine, DiffOrigin, FileDiff, Hunk};
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    fn thread_at(key: u64, path: &str, side: AnnotationSide, line: u32) -> Annotation {
        Annotation {
            key,
            path: PathBuf::from(path),
            side,
            line: Some(line),
            start_line: None,
            kind: AnnotationKind::Thread { open: true },
            revision: 0,
        }
    }

    fn sample_tab(annotations: Vec<Annotation>) -> ChangesTab {
        let path = PathBuf::from("a.rs");
        let mut lines: Vec<DiffLine> = (10..=19).map(ctx_line).collect();
        lines.extend([del_line(20), add_line(20)]);
        lines.extend((21..=24).map(ctx_line));
        let mut diff = diff_of(vec![lines]);
        diff.path = path.clone();
        let mut tab = settled_changes_tab_without_git(std::env::temp_dir());
        tab.entries = vec![StatusEntry {
            path: path.clone(),
            original_path: None,
            index_status: None,
            worktree_status: Some(StatusKind::Modified),
        }];
        tab.diffs.insert(path.clone(), diff);
        tab.expanded_changes.insert((ChangeSection::Changed, path));
        tab.annotations = annotations;
        tab
    }

    fn annotation_diff_rows(tab: &ChangesTab, mode: DiffViewMode) -> Vec<ChangeRow> {
        let entry = &tab.entries[0];
        let mut rows = vec![ChangeRow::File {
            section: ChangeSection::Changed,
            entry: entry.clone(),
            stat: None,
            drag_payload: None,
            expanded: true,
        }];
        tab.expand_diff(&mut rows, ChangeSection::Changed, entry, mode);
        rows
    }

    fn annotation_keys(rows: &[ChangeRow]) -> Vec<(usize, u64)> {
        rows.iter().enumerate().filter_map(|(index, row)| match row {
            ChangeRow::Annotation { key, .. } => Some((index, *key)),
            _ => None,
        }).collect()
    }

    #[test]
    fn annotation_reports_follow_drawn_row_order_before_hidden_rows() {
        let outdated = Annotation {
            key: 3, kind: AnnotationKind::Outdated { count: 1 }, line: None,
            ..thread_at(3, "a.rs", AnnotationSide::New, 20)
        };
        let mut tab = sample_tab(vec![
            thread_at(2, "a.rs", AnnotationSide::New, 23),
            thread_at(1, "a.rs", AnnotationSide::New, 14),
            outdated,
            thread_at(4, "missing.rs", AnnotationSide::New, 4),
        ]);
        for mode in [DiffViewMode::Unified, DiffViewMode::Split] {
            tab.unified_mode = mode;
            let reports = tab.report().annotations;
            assert_eq!(reports.iter().map(|report| report.key).collect::<Vec<_>>(), vec![3, 1, 2, 4]);
            assert_eq!(reports[3].placed, "hidden");
        }
    }

    #[test]
    fn a_thread_follows_the_row_of_its_new_line() {
        let tab = sample_tab(vec![thread_at(1, "a.rs", AnnotationSide::New, 20)]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        let keys = annotation_keys(&rows);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].1, 1);
        assert!(matches!(&rows[keys[0].0 - 1], ChangeRow::Line { line, .. }
            if line.new_line_number == Some(20) && line.old_line_number.is_none()));
        assert_eq!(tab.report().annotations[0].placed, "line");
    }

    #[test]
    fn an_old_side_thread_follows_the_deleted_line() {
        let tab = sample_tab(vec![thread_at(1, "a.rs", AnnotationSide::Old, 20)]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        let keys = annotation_keys(&rows);
        assert_eq!(keys.len(), 1);
        assert!(matches!(&rows[keys[0].0 - 1], ChangeRow::Line { line, .. }
            if line.old_line_number == Some(20) && line.new_line_number.is_none()));
    }

    #[test]
    fn two_threads_on_one_row_both_follow_it() {
        let tab = sample_tab(vec![
            thread_at(1, "a.rs", AnnotationSide::Old, 20),
            thread_at(2, "a.rs", AnnotationSide::New, 20),
        ]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Split);
        let keys = annotation_keys(&rows);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].1, 1);
        assert_eq!(keys[1], (keys[0].0 + 1, 2));
        assert!(matches!(&rows[keys[0].0 - 1], ChangeRow::SplitLine { row, .. }
            if row.left.as_ref().and_then(|line| line.old_line_number) == Some(20)
                && row.right.as_ref().and_then(|line| line.new_line_number) == Some(20)));
    }

    #[test]
    fn a_band_splits_around_an_anchored_line() {
        let mut annotation = thread_at(1, "a.rs", AnnotationSide::New, 14);
        for mode in [DiffViewMode::Unified, DiffViewMode::Split] {
            let tab = sample_tab(vec![annotation.clone()]);
            let rows = annotation_diff_rows(&tab, mode);
            let index = annotation_keys(&rows)[0].0;
            assert!(matches!(&rows[index - 2], ChangeRow::ContextBand { key: 0, count: 4, .. }));
            assert!(matches!(&rows[index + 1], ChangeRow::ContextBand { key: 5, count: 5, .. }));
            match &rows[index - 1] {
                ChangeRow::Line { line, .. } => assert_eq!(line.new_line_number, Some(14)),
                ChangeRow::SplitLine { row, .. } => {
                    assert_eq!(row.right.as_ref().and_then(|line| line.new_line_number), Some(14));
                }
                _ => panic!("the anchored line must be visible"),
            }
        }
        annotation.start_line = Some(13);
        let tab = sample_tab(vec![annotation]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        for number in [13, 14] {
            assert!(rows.iter().any(|row| matches!(row, ChangeRow::Line { line, .. }
                if line.new_line_number == Some(number))));
        }
    }

    #[test]
    fn an_unplaced_thread_shows_at_the_top_of_its_file() {
        let mut tab = sample_tab(vec![thread_at(1, "a.rs", AnnotationSide::New, 99)]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        assert_eq!(annotation_keys(&rows), vec![(1, 1)]);
        assert!(matches!(&rows[0], ChangeRow::File { .. }));
        assert!(matches!(&rows[2], ChangeRow::Hunk { .. }));
        assert_eq!(tab.report().annotations[0].placed, "file");
        tab.expanded_changes.clear();
        assert_eq!(tab.report().annotations[0].placed, "hidden");
    }

    #[test]
    fn an_outdated_section_sits_under_the_file_header() {
        let section = Annotation {
            kind: AnnotationKind::Outdated { count: 2 },
            line: None,
            ..thread_at(9, "a.rs", AnnotationSide::New, 1)
        };
        let mut tab = sample_tab(vec![section]);
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        assert_eq!(annotation_keys(&rows), vec![(1, 9)]);
        assert!(matches!(&rows[0], ChangeRow::File { .. }));
        assert!(matches!(&rows[2], ChangeRow::Hunk { .. }));
        assert_eq!(tab.report().annotations[0].placed, "section");
        let hash = |row: ChangeRow| {
            let mut state = DefaultHasher::new();
            ListRow::Change(row).hash_identity(&mut state);
            state.finish()
        };
        let before = hash(rows[1].clone());
        let list = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(list);
        let fingerprint = tab.list_fingerprint;
        let list = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(list);
        assert_eq!(tab.list_fingerprint, fingerprint);
        tab.annotations[0].revision += 1;
        let rows = annotation_diff_rows(&tab, DiffViewMode::Unified);
        assert_ne!(hash(rows[1].clone()), before);
        let list = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(list);
        assert_ne!(tab.list_fingerprint, fingerprint);
    }

    #[gpui::test]
    async fn an_old_anchor_reveals_a_line_in_a_band_split_by_another_thread(cx: &mut TestAppContext) {
        let tab = cx.new(|_| {
            let mut tab = sample_tab(Vec::new());
            for line in &mut tab.diffs.get_mut(Path::new("a.rs")).unwrap().hunks[0].lines {
                line.old_line_number = line.old_line_number.map(|number| number + 100);
            }
            tab
        });
        tab.update(cx, |tab, cx| {
            tab.set_annotations(vec![thread_at(1, "a.rs", AnnotationSide::New, 14)], HashMap::new(), cx);
            assert_eq!(tab.open_threads.get(Path::new("a.rs")), Some(&1));
            tab.expand_all(cx);
            let rows = annotation_diff_rows(tab, DiffViewMode::Unified);
            assert!(rows.iter().all(|row| !matches!(row,
                ChangeRow::ContextBand { expanded: false, .. })),
                "Expand All must open every piece of the split bands");
            tab.expanded_bands.clear();
            tab.focus_anchor(Path::new("a.rs"), AnnotationSide::Old, Some(116), None, cx);
            for mode in [DiffViewMode::Unified, DiffViewMode::Split] {
                let rows = annotation_diff_rows(tab, mode);
                assert!(rows.iter().any(|row| match row {
                    ChangeRow::Line { line, .. } => line.old_line_number == Some(116),
                    ChangeRow::SplitLine { row, .. } => {
                        row.left.as_ref().and_then(|line| line.old_line_number) == Some(116)
                    }
                    _ => false,
                }), "the old-side reveal must open the piece containing its own line");
            }
            let rows = tab.section_rows(DiffViewMode::Split);
            tab.sync_list_rows(rows);
            assert!(tab.reveal_line.is_none());
            tab.focus_anchor(Path::new("a.rs"), AnnotationSide::New, Some(14), Some(1), cx);
            let rows = tab.section_rows(DiffViewMode::Unified);
            let rows = tab.sync_list_rows(rows);
            assert!(reveal_annotation(&rows, 1).is_some());
            assert!(tab.reveal_annotation.is_none());
        });
    }

    /// A reveal is handled by the same rebuild that splices the list, when no
    /// row has been measured yet: it must still bring the card to the top of
    /// the view, not leave the view where it was.
    #[gpui::test]
    async fn a_reveal_brings_its_card_into_view_before_any_row_is_measured(cx: &mut TestAppContext) {
        let tab = cx.new(|_| sample_tab(Vec::new()));
        tab.update(cx, |tab, cx| {
            tab.set_annotations(vec![thread_at(1, "a.rs", AnnotationSide::New, 22)], HashMap::new(), cx);
            tab.focus_anchor(Path::new("a.rs"), AnnotationSide::New, Some(22), Some(1), cx);
            let rows = tab.section_rows(DiffViewMode::Unified);
            let rows = tab.sync_list_rows(rows);
            let card = reveal_annotation(&rows, 1).expect("the card is drawn");
            let top = tab.list_state.logical_scroll_top().item_ix;
            assert!(card > 2, "the card sits below the file's first rows ({card})");
            assert!(top > 0 && top <= card, "the view starts at row {top}; the card is row {card}");
        });
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-changes-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Seeds a repository with two commits: `a.txt` ("first") then `b.txt`
    /// ("second"). A commit-mode surface reads this same shape back through
    /// `git show`.
    fn seed_two_commits(dir: &Path) {
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        std::fs::write(dir.join("a.txt"), "first\n").expect("write a.txt");
        git(dir, &["add", "a.txt"]);
        git(
            dir,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "first"],
        );
        std::fs::write(dir.join("b.txt"), "second\n").expect("write b.txt");
        git(dir, &["add", "b.txt"]);
        git(
            dir,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "second"],
        );
    }

    /// Resolves a revision to its full 40-character object name.
    fn rev_parse(dir: &Path, revision: &str) -> String {
        let output = std::process::Command::new("git")
            .args(["rev-parse", revision])
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "rev-parse {revision} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn clean_git_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        std::fs::write(dir.join("tracked.txt"), "initial\n").expect("seed tracked file");
        git(dir, &["add", "tracked.txt"]);
        git(
            dir,
            &[
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-q",
                "-m",
                "initial",
            ],
        );
    }

    /// Pumps the test executors until `condition` holds or the budget is
    /// exhausted.
    /// Pump iterations before a wait helper gives up.
    ///
    /// Each iteration sleeps 10ms of REAL time, because the conditions these
    /// helpers wait on are satisfied by real `git` subprocesses that the test
    /// executor's virtual clock cannot advance. The budget is therefore a
    /// wall-clock timeout, and it has to survive a loaded machine: under
    /// `cargo test --workspace` these tests run alongside every other crate's
    /// test binary, all competing for CPU and disk with their own `git`
    /// processes. The old 600 (6s) was enough on an idle machine and flaked
    /// under that load.
    ///
    /// Raising it costs nothing when tests pass — a satisfied condition returns
    /// on the next iteration — and only buys patience when they would otherwise
    /// fail for lack of it.
    const PUMP_BUDGET: usize = 3000;

    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..PUMP_BUDGET {
            if condition() {
                return;
            }
            // The poll loop waits on a 1s background timer; the test
            // executor runs on a virtual clock, so advance it and let the
            // git subprocess use real time.
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    /// #193: a Changes surface in a background tab kept reloading. Its
    /// tick is not cheap -- each one spawns three or four git processes
    /// (`status --untracked-files=all`, `diff --numstat HEAD`, `rev-parse
    /// --verify HEAD`) -- and measured on the running app, backgrounding
    /// the tab changed nothing at all: 71 git processes in twenty seconds
    /// with the tab in front, 77 with it behind another tab. With the gate,
    /// a backgrounded tab contributes none of them, and reselecting it
    /// resumes at once.
    ///
    /// The signal is a count of real draws, for the reason established in
    /// #189: `Window::is_window_active()` was tried there first and reads
    /// `true` for a minimised window, because `render` stops being called
    /// and the last polled value goes stale exactly when it needs to
    /// change. Both halves of the wiring are asserted -- that a draw is
    /// counted at all, and that a draw clears a suspension -- because a
    /// panel that never counted draws would suspend itself permanently,
    /// which is the failure this gate must not have.
    #[gpui::test]
    async fn a_drawn_frame_is_counted_and_resumes_a_suspended_reload(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        cx.run_until_parked();

        assert!(
            tab.read_with(&cx.cx, |tab, _| tab.renders) > 0,
            "render must count the frames this surface is drawn in -- that              count is the whole signal, and a surface that never incremented              it would suspend its reloads forever"
        );

        tab.update(&mut cx.cx, |tab, _| {
            tab.refresh_suspended = true;
        });
        let before = tab.read_with(&cx.cx, |tab, _| tab.renders);
        tab.update(&mut cx.cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| tab.renders) > before,
            "the harness really did draw another frame"
        );

        assert!(
            !tab.read_with(&cx.cx, |tab, _| tab.refresh_suspended),
            "a drawn frame must clear the suspension and reload at once, so              a reselected tab never shows the diff frozen at the moment it              was left"
        );
    }

    /// The same surface as `changes_view`, built the way the right panel's
    /// Diff view builds it — the one host `Open diff` can leave.
    fn panel_changes_view(
        cx: &mut TestAppContext,
        repo_root: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<ChangesTab>) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, cx| ChangesTab::in_right_panel(repo_root.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });
        (cx, tab)
    }

    fn changes_view(
        cx: &mut TestAppContext,
        repo_root: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<ChangesTab>) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, cx| ChangesTab::new(repo_root.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });
        (cx, tab)
    }

    /// A range surface over one file whose diff is at hand, as the tests
    /// below need it: no window, no git.
    fn range_tab_with_one_diff(expanded: bool) -> ChangesTab {
        let path = PathBuf::from("x.rs");
        let context = |n: usize| DiffLine {
            origin: DiffOrigin::Context,
            old_line_number: Some(n),
            new_line_number: Some(n),
            content: format!("line {n}"),
        };
        let diff = FileDiff {
            path: path.clone(),
            hunks: vec![sirio_git::Hunk {
                header: "@@ -1,4 +1,4 @@".to_string(),
                old_start: 1,
                old_lines: 4,
                new_start: 1,
                new_lines: 4,
                lines: vec![
                    context(1),
                    context(2),
                    DiffLine {
                        origin: DiffOrigin::Deletion,
                        old_line_number: Some(3),
                        new_line_number: None,
                        content: "old".to_string(),
                    },
                    DiffLine {
                        origin: DiffOrigin::Addition,
                        old_line_number: None,
                        new_line_number: Some(3),
                        content: "new".to_string(),
                    },
                    context(4),
                ],
            }],
            additions: 1,
            deletions: 1,
            is_binary: false,
            is_submodule: false,
        };
        let mut tab = settled_changes_tab_without_git(PathBuf::from("/tmp"));
        tab.source = ChangesSource::Range {
            base: "0".repeat(40),
            head: "1".repeat(40),
        };
        tab.entries = vec![StatusEntry {
            path: path.clone(),
            original_path: None,
            index_status: Some(StatusKind::Modified),
            worktree_status: None,
        }];
        tab.diffs = HashMap::from([(path.clone(), diff)]);
        if expanded {
            tab.expanded_changes.insert((ChangeSection::Staged, path));
        }
        tab
    }

    /// In Split mode a changed line lives in a `SplitLine` row, on its right
    /// side; a reveal must land on that row, not on the file's.
    #[test]
    fn a_revealed_line_is_found_on_the_right_side_of_a_split_row() {
        let mut tab = range_tab_with_one_diff(true);
        let path = PathBuf::from("x.rs");
        let unified = tab.section_rows(DiffViewMode::Unified);
        let unified = tab.sync_list_rows(unified);
        let file_row = unified
            .iter()
            .position(|row| matches!(row, ListRow::Change(ChangeRow::File { .. })))
            .expect("the file row");
        let in_unified = reveal_target(&unified, &path, 3).expect("line 3 is in the hunk");
        assert!(
            matches!(&unified[in_unified], ListRow::Change(ChangeRow::Line { line, .. }) if line.new_line_number == Some(3)),
            "unified: the line's own row"
        );

        let split = tab.section_rows(DiffViewMode::Split);
        let split = tab.sync_list_rows(split);
        let in_split = reveal_target(&split, &path, 3).expect("line 3 is in the hunk");
        assert_ne!(in_split, file_row, "split: not the file row");
        assert!(
            matches!(&split[in_split], ListRow::Change(ChangeRow::SplitLine { row, .. })
                if row.right.as_ref().and_then(|side| side.new_line_number) == Some(3)),
            "split: the row whose right side is new line 3"
        );
        assert_eq!(reveal_target(&split, &path, 99), Some(file_row), "a line in no hunk reveals the file");
    }

    /// A reveal for a file whose diff failed, or for a path the range does
    /// not touch, can never be satisfied: it is dropped at the file's row
    /// rather than held for a diff that will not come.
    #[test]
    fn a_reveal_that_cannot_be_satisfied_is_cleared_at_the_file_row() {
        let mut tab = range_tab_with_one_diff(true);
        tab.diffs.clear();
        tab.diff_errors.insert(PathBuf::from("x.rs"), "fatal: bad object".to_string());
        tab.reveal_line = Some((PathBuf::from("x.rs"), AnnotationSide::New, Some(3)));
        let rows = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(rows);
        assert_eq!(tab.reveal_line, None, "a failed diff will not bring the line");

        let mut tab = range_tab_with_one_diff(false);
        tab.reveal_line = Some((PathBuf::from("not-in-the-range.rs"), AnnotationSide::New, Some(1)));
        let rows = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(rows);
        assert_eq!(tab.reveal_line, None, "a path outside the range has no row to wait for");

        let mut tab = range_tab_with_one_diff(false);
        tab.diffs.clear();
        tab.reveal_line = Some((PathBuf::from("x.rs"), AnnotationSide::New, Some(3)));
        let rows = tab.section_rows(DiffViewMode::Unified);
        tab.sync_list_rows(rows);
        assert_eq!(tab.reveal_line, Some((PathBuf::from("x.rs"), AnnotationSide::New, Some(3))), "a diff still loading keeps the reveal");
    }

    fn settled_changes_tab(repo_root: PathBuf) -> ChangesTab {
        let entries = status(&repo_root)
            .expect("status for settled Changes tab")
            .entries;
        let mut tab = settled_changes_tab_without_git(repo_root);
        tab.entries = entries;
        tab
    }

    fn settled_changes_tab_without_git(repo_root: PathBuf) -> ChangesTab {
        ChangesTab {
            repo_root,
            is_git: true,
            source: ChangesSource::WorkingTree,
            entries: Vec::new(),
            diffs: HashMap::new(),
            stats: HashMap::new(),
            expanded_changes: HashSet::new(),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: true,
            git_error: None,
            git_error_from_mutation: false,
            diff_errors: HashMap::new(),
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        }
    }

    fn wait_for_tab(
        cx: &VisualTestContext,
        tab: &gpui::Entity<ChangesTab>,
        mut condition: impl FnMut(&ChangesTab) -> bool,
    ) {
        cx.cx.executor().allow_parking();
        for _ in 0..PUMP_BUDGET {
            if tab.read_with(&cx.cx, |tab, _| condition(tab)) {
                return;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        let report = tab.read_with(&cx.cx, |tab, _| tab.report());
        let git_error = tab.read_with(&cx.cx, |tab, _| tab.git_error.clone());
        panic!(
            "changes tab condition never became true within the pump budget: report={report:?}, git_error={git_error:?}"
        );
    }

    /// #325: the keyboard path to what "Open diff" does with the mouse. In
    /// the panel Enter promotes; in the tab there is nothing to promote to,
    /// so it expands the row the way a click does.
    #[gpui::test]
    async fn return_promotes_the_selected_change_row_only_from_the_panel(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        cx.update(Theme::init);

        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, cx| ChangesTab::in_right_panel(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.cx.update(|app| {
            app.subscribe(&tab, move |_, event: &ChangesTabActionEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();

        // Down selects the first row; nothing was selected before it.
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| tab.selected_change.clone()),
            None,
            "no row is selected until the keyboard picks one"
        );
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed-file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| tab
                .selected_change
                .as_ref()
                .map(|(_, path)| path.clone())),
            Some(PathBuf::from("tracked.txt")),
            "clicking a row makes it the row Enter acts on"
        );

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                ChangesTabActionEvent::OpenDiff(path) if path == &PathBuf::from("tracked.txt")
            )),
            "Return promotes the selected row through the real event path"
        );
    }

    /// #325: the same key inside the Changes tab expands rather than
    /// promoting -- the tab is already the destination `OpenDiff` reveals.
    #[gpui::test]
    async fn return_expands_the_selected_row_inside_the_changes_tab(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.cx.update(|app| {
            app.subscribe(&tab, move |_, event: &ChangesTabActionEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();

        // The click both selects and expands, which is also what focuses the
        // list -- Enter cannot reach a surface nothing put focus on.
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed-file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let expanded = |cx: &VisualTestContext| {
            tab.read_with(&cx.cx, |tab, _| {
                tab.is_expanded(ChangeSection::Changed, Path::new("tracked.txt"))
            })
        };
        assert!(expanded(&cx), "the click expanded the row");

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(
            !expanded(&cx),
            "Return toggles the selected row where there is nothing to promote to"
        );

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(expanded(&cx), "and toggles it back");
        assert!(
            events.borrow().is_empty(),
            "the tab must not emit OpenDiff at itself"
        );
    }

    fn section_count(tab: &ChangesTab, name: &str) -> usize {
        tab.report()
            .sections
            .iter()
            .find(|section| section.name == name)
            .map_or(0, |section| section.count)
    }

    #[gpui::test]
    async fn changes_refresh_automatically_after_an_external_edit(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));

        // Wait for the initial background snapshot to complete. The repo is
        // clean, so an empty entry list is the correct resting state.
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.git_task.is_none()));

        // Arm the same poll loop Render uses, then make the external edit:
        // no tab method is called afterwards.
        tab.update(cx, |tab, cx| tab.ensure_refresh(cx));
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("edit tracked file");
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });
    }

    /// The bug this fix targets, reproduced through the real refresh path
    /// rather than the bare function: a large agent-driven edit lands a new
    /// changed file while the tab stays open and nothing has been expanded
    /// for it. A periodic-style refresh (`refresh()` called again once the
    /// surface has already settled -- the shape of every tick
    /// `ensure_refresh` arms) must not spend a `git diff` process on that
    /// row just because `git status` now reports it; its cheap batched stat
    /// still arrives, same as every other row.
    #[gpui::test]
    async fn a_periodic_refresh_never_fetches_a_diff_for_a_newly_seen_collapsed_row(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("f0.txt"), "v0\n").expect("seed first changed file");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.entries.len() == 1));
        assert!(
            tab.read_with(cx, |tab, _| tab.diffs.contains_key(Path::new("f0.txt"))),
            "the first load fetches the only entry's diff up front"
        );

        // The external edit: a second changed file appears while nothing
        // new is expanded.
        std::fs::write(dir.0.join("f1.txt"), "v1\n").expect("seed second changed file");
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.entries.len() == 2));

        tab.read_with(cx, |tab, _| {
            assert!(
                !tab.diffs.contains_key(Path::new("f1.txt")),
                "a periodic-style refresh must not fetch a diff for a row nobody expanded"
            );
            assert!(
                tab.diffs.contains_key(Path::new("f0.txt")),
                "an already-cached diff for a still-live, still-collapsed row survives the refresh"
            );
            assert_eq!(
                tab.stats.len(),
                2,
                "the cheap batched stats cover the new row too"
            );
        });
    }

    /// Expanding a row whose diff isn't cached (e.g. a lazy refresh dropped
    /// it while it was collapsed) fetches it immediately instead of waiting
    /// for the next periodic tick.
    #[gpui::test]
    async fn expanding_a_row_missing_its_diff_fetches_it_at_once(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.diffs.contains_key(Path::new("tracked.txt"))
            })
        });

        // Simulate a diff that a lazy refresh dropped because the row was
        // collapsed at the time, without running a whole refresh cycle.
        tab.update(cx, |tab, _| {
            tab.diffs.remove(Path::new("tracked.txt"));
        });

        tab.update(cx, |tab, cx| {
            tab.toggle_change(ChangeSection::Changed, Path::new("tracked.txt"), cx);
        });

        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.diffs.contains_key(Path::new("tracked.txt"))
            })
        });
    }

    /// A file that is staged and then modified again appears in **both** the
    /// Staged and the Changed sections — porcelain's two status columns are
    /// kept apart, not collapsed into one row — and the per-row action is
    /// the one for the side the row sits on.
    #[gpui::test]
    async fn a_staged_and_modified_file_appears_in_both_sections(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        // One file, two independent states at once: staged (index column)
        // and further modified (worktree column).
        std::fs::write(dir.0.join("both.txt"), "v1\n").expect("seed");
        git(&dir.0, &["add", "both.txt"]);
        std::fs::write(dir.0.join("both.txt"), "v2\n").expect("modify again");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"both.txt")
            })
        });

        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let staged = sections
            .iter()
            .find(|section| section.section == ChangeSection::Staged)
            .expect("staged section must exist");
        let changed = sections
            .iter()
            .find(|section| section.section == ChangeSection::Changed)
            .expect("changed section must exist");
        assert_eq!(staged.count, 1);
        assert_eq!(changed.count, 1);
        let contains_both = |section: &SectionRows| {
            section.rows.iter().any(
                |row| matches!(row, ChangeRow::File { entry, .. } if entry.path == *"both.txt"),
            )
        };
        assert!(contains_both(staged), "the staged row must list both.txt");
        assert!(contains_both(changed), "the changed row must list both.txt");

        // F-CHG-06. Presence is not the whole contract, and asserting only
        // presence is why this test stayed green through the bug: `both.txt`
        // landed in both sections correctly while rendering the *wrong
        // colour* in the Changes list — amber, where the Files tree drew the
        // same file green. Pin the colour on this exact entry, through
        // `status_color`, the function the rows actually call.
        let entry = tab.read_with(cx, |tab, _| {
            tab.entries
                .iter()
                .find(|entry| entry.path == *"both.txt")
                .cloned()
                .expect("both.txt must be among the entries")
        });
        let theme = Theme::dark();
        assert!(
            entry.is_staged() && entry.has_worktree_changes(),
            "fixture must really be staged AND further modified, or the \
             assertion below proves nothing"
        );
        assert_eq!(
            status_color(&entry, theme),
            theme.ely.success,
            "a staged-then-modified file reads as staged, matching Swift's \
             GitStatusStyle.color and the Files tree marker"
        );
        assert_ne!(
            status_color(&entry, theme),
            theme.ely.warning,
            "the pre-fix order returned git_modified here"
        );
    }

    /// One file per bucket: the sections appear in display order, each
    /// carrying its own count.
    #[gpui::test]
    async fn section_counts_match_the_three_buckets(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("staged.txt"), "s\n").expect("seed staged");
        git(&dir.0, &["add", "staged.txt"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");
        std::fs::write(dir.0.join("untracked.txt"), "u\n").expect("untracked file");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.entries.len() == 3));

        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let kinds: Vec<ChangeSection> = sections.iter().map(|section| section.section).collect();
        assert_eq!(
            kinds,
            vec![
                ChangeSection::Staged,
                ChangeSection::Changed,
                ChangeSection::Untracked
            ],
            "sections must appear in display order"
        );
        let counts: Vec<usize> = sections.iter().map(|section| section.count).collect();
        assert_eq!(counts, vec![1, 1, 1], "each section states its own size");
    }

    /// F-CHG-13: `focus_path` expands the section(s) carrying the given
    /// path and un-collapses them, so a caller that just opened the Changes
    /// tab for one specific file sees its diff immediately.
    #[gpui::test]
    async fn focus_path_expands_and_uncollapses_the_owning_sections(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("staged.txt"), "s\n").expect("seed staged");
        git(&dir.0, &["add", "staged.txt"]);
        std::fs::write(dir.0.join("staged.txt"), "s2\n").expect("also modify after staging");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"staged.txt")
            })
        });

        tab.update(cx, |tab, cx| {
            tab.collapsed_sections.insert(ChangeSection::Staged);
            tab.collapsed_sections.insert(ChangeSection::Changed);
            tab.focus_path(Path::new("staged.txt"), cx);
        });

        tab.read_with(cx, |tab, _| {
            assert!(
                !tab.collapsed_sections.contains(&ChangeSection::Staged),
                "the Staged section (which carries this file) is un-collapsed"
            );
            assert!(
                !tab.collapsed_sections.contains(&ChangeSection::Changed),
                "the Changed section (which also carries this partially-staged file) is un-collapsed"
            );
            assert!(
                tab.is_expanded(ChangeSection::Staged, Path::new("staged.txt")),
                "the Staged row for the file is expanded"
            );
            assert!(
                tab.is_expanded(ChangeSection::Changed, Path::new("staged.txt")),
                "the Changed row for the file is expanded"
            );
        });
    }

    /// Clicking file rows inside the tab moves what a restart brings back:
    /// expanding a row saves that file, collapsing its last expanded row
    /// clears it again. `select_change` alone (keyboard/mouse highlight)
    /// does not — only opening the diff, the way `focus_path` does.
    #[gpui::test]
    async fn expanding_a_row_moves_the_saved_focus_and_collapsing_it_clears_it(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("a.txt"), "a\n").expect("seed a");
        std::fs::write(dir.0.join("b.txt"), "b\n").expect("seed b");
        git(&dir.0, &["add", "a.txt", "b.txt"]);
        git(
            &dir.0,
            &[
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-q",
                "-m",
                "seed a and b",
            ],
        );
        std::fs::write(dir.0.join("a.txt"), "a changed\n").expect("modify a");
        std::fs::write(dir.0.join("b.txt"), "b changed\n").expect("modify b");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"b.txt")
            })
        });

        tab.update(cx, |tab, cx| {
            tab.focus_path(Path::new("a.txt"), cx);
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.focused_path(),
                Some(Path::new("a.txt")),
                "focus_path saves file A"
            );
        });

        tab.update(cx, |tab, cx| {
            tab.toggle_change(ChangeSection::Changed, Path::new("b.txt"), cx);
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.focused_path(),
                Some(Path::new("b.txt")),
                "expanding B's row moves the saved focus to B"
            );
        });

        tab.update(cx, |tab, cx| {
            tab.toggle_change(ChangeSection::Changed, Path::new("b.txt"), cx);
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.focused_path(),
                None,
                "collapsing B's last expanded row clears the saved focus"
            );
        });
    }

    #[gpui::test]
    async fn collapse_all_clears_the_saved_focus(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("a.txt"), "a\n").expect("seed a");
        git(&dir.0, &["add", "a.txt"]);
        git(
            &dir.0,
            &[
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-q",
                "-m",
                "seed a",
            ],
        );
        std::fs::write(dir.0.join("a.txt"), "a changed\n").expect("modify a");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"a.txt")
            })
        });

        tab.update(cx, |tab, cx| {
            tab.focus_path(Path::new("a.txt"), cx);
        });
        tab.update(cx, |tab, cx| {
            tab.collapse_all(cx);
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.focused_path(),
                None,
                "collapse all clears the saved focus"
            );
        });
    }

    /// Regression for F-CHG-13: `add_changes_tab` calls `ChangesTab::new`
    /// and then `focus_path` in the same tick, before the `new`-triggered
    /// async refresh has populated `entries`. focus_path must not silently
    /// no-op against the still-empty entries — it must defer and replay
    /// once the first snapshot lands.
    #[gpui::test]
    async fn focus_path_called_before_the_first_refresh_lands_still_expands_once_it_does(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        // Mirrors add_changes_tab: focus_path is called synchronously right
        // after ChangesTab::new, with no pump in between, so entries is
        // still empty and the background git_task is still in flight.
        let tab = cx.new(|cx| {
            let mut tab = ChangesTab::new(dir.0.clone(), cx);
            tab.focus_path(Path::new("tracked.txt"), cx);
            tab
        });

        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });

        tab.read_with(cx, |tab, _| {
            assert!(
                tab.is_expanded(ChangeSection::Changed, Path::new("tracked.txt")),
                "the deferred focus_path request is replayed once the snapshot lands"
            );
        });
    }

    /// Every path asked for before the first snapshot lands is expanded
    /// once it does, not only the last one asked for.
    #[gpui::test]
    async fn several_focus_path_calls_before_the_first_refresh_all_expand(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");
        std::fs::write(dir.0.join("fresh.txt"), "new\n").expect("write untracked");

        let tab = cx.new(|cx| {
            let mut tab = ChangesTab::new(dir.0.clone(), cx);
            tab.focus_path(Path::new("tracked.txt"), cx);
            tab.focus_path(Path::new("fresh.txt"), cx);
            tab
        });

        pump_until(cx, || tab.read_with(cx, |tab, _| tab.entries.len() == 2));

        tab.read_with(cx, |tab, _| {
            assert!(tab.is_expanded(ChangeSection::Changed, Path::new("tracked.txt")));
            assert!(tab.is_expanded(ChangeSection::Untracked, Path::new("fresh.txt")));
        });
    }

    /// The first snapshot has no stale entries to preserve, so it gets the
    /// full-surface loading treatment while the background git task is in
    /// flight.
    #[gpui::test]
    async fn a_first_load_shows_the_generic_loader_over_an_empty_list(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        assert!(
            tab.read_with(&cx.cx, |tab, _| {
                tab.git_task.is_some() && tab.entries.is_empty()
            }),
            "the assertion must cover the empty first-load state, before git has published a snapshot"
        );
        tab.update(&mut cx.cx, |tab, cx| {
            // Hold the task open so the test cannot race a fast git snapshot.
            tab.git_task = Some(cx.spawn(async move |_this, _cx| {
                std::future::pending::<()>().await;
            }));
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("changes-loading").is_some(),
            "an empty first load draws the loading surface"
        );
    }

    /// F-CHG-02: a clean repo (or a worktree just closed and reopened with
    /// nothing to show) must draw a real "No changes" message instead of
    /// silently falling through to a blank `changes-list` with zero
    /// children — a blank panel looks broken, not confirmed-clean.
    #[gpui::test]
    async fn a_clean_repo_draws_a_no_changes_message_not_a_blank_panel(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        cx.cx
            .update(|app| tab.update(app, |tab, cx| tab.refresh(cx)));
        // `git_task.is_none()` is not the condition this test means: it is also
        // true in the window between `refresh` being called and the task being
        // spawned, so the assertions below could run against a first-load
        // loader that has not started yet. Wait for the load to have actually
        // completed.
        wait_for_tab(&cx, &tab, |tab| tab.has_loaded);
        cx.cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        assert!(
            cx.debug_bounds("changes-empty").is_some(),
            "a clean repo draws the empty-state message"
        );
        assert!(
            cx.debug_bounds("changes-list").is_none(),
            "the empty-state message replaces the (otherwise childless) list, not both"
        );
    }

    /// Collapsing a section hides its rows but keeps its header (so it can
    /// be re-opened), and the choice is remembered.
    #[gpui::test]
    async fn a_collapsed_section_hides_its_rows(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("staged.txt"), "s\n").expect("seed staged");
        git(&dir.0, &["add", "staged.txt"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");
        std::fs::write(dir.0.join("untracked.txt"), "u\n").expect("untracked file");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.entries.len() == 3));

        tab.update(cx, |tab, _| {
            tab.collapsed_sections.insert(ChangeSection::Untracked);
        });
        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let untracked = sections
            .iter()
            .find(|section| section.section == ChangeSection::Untracked)
            .expect("the header must still render");
        assert!(untracked.collapsed);
        assert_eq!(untracked.count, 1);
        assert!(
            untracked.rows.is_empty(),
            "a collapsed section must hide its file rows"
        );
        // Sibling sections are unaffected.
        let staged = sections
            .iter()
            .find(|section| section.section == ChangeSection::Staged)
            .unwrap();
        assert_eq!(staged.rows.len(), 1);

        // Re-expanding restores the rows.
        tab.update(cx, |tab, _| {
            tab.collapsed_sections.remove(&ChangeSection::Untracked);
        });
        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let untracked = sections
            .iter()
            .find(|section| section.section == ChangeSection::Untracked)
            .unwrap();
        assert!(!untracked.collapsed);
        assert_eq!(untracked.rows.len(), 1);
    }

    /// Expanding a file row is per (section, path): a file that appears in
    /// both sections expands independently, so the diff never appears under
    /// a row the user did not open.
    #[gpui::test]
    async fn expansion_is_per_section_and_path(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("both.txt"), "v1\n").expect("seed");
        git(&dir.0, &["add", "both.txt"]);
        std::fs::write(dir.0.join("both.txt"), "v2\n").expect("modify again");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"both.txt")
            })
        });

        let path = PathBuf::from("both.txt");
        tab.update(cx, |tab, cx| {
            tab.toggle_change(ChangeSection::Staged, &path, cx);
        });
        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let staged = sections
            .iter()
            .find(|section| section.section == ChangeSection::Staged)
            .unwrap();
        let changed = sections
            .iter()
            .find(|section| section.section == ChangeSection::Changed)
            .unwrap();
        assert!(
            staged.rows.iter().any(|row| matches!(
                row,
                ChangeRow::Hunk { .. } | ChangeRow::Line { .. } | ChangeRow::ContextBand { .. }
            )),
            "the expanded staged row shows the diff"
        );
        assert!(
            changed
                .rows
                .iter()
                .all(|row| matches!(row, ChangeRow::File { .. })),
            "the changed row stays collapsed"
        );
    }

    // ── Honesty: an error must look like an error, not like empty ──────

    #[gpui::test]
    async fn a_known_non_git_project_shows_the_empty_changes_state(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, cx| {
            ChangesTab::in_right_panel_with_git_capability(dir.0.clone(), false, cx)
        });
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let tab = visual.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });

        pump_until(cx, || tab.read_with(cx, |tab, _| tab.has_loaded));
        cx.run_until_parked();

        assert!(tab.read_with(cx, |tab, _| tab.git_error.is_none()));
        assert!(visual.debug_bounds("changes-error").is_none());
        assert!(visual.debug_bounds("changes-empty").is_some());
        assert!(visual.debug_bounds("changes-list").is_none());
    }

    /// The error state is actually rendered — a panel with the detail and
    /// a Retry button — and clicking Retry re-runs the refresh and returns
    /// to the list.
    #[gpui::test]
    async fn the_error_state_renders_and_retry_is_clickable(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        cx.update(Theme::init);

        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, cx| ChangesTab::new(dir.0.clone(), cx));
        let tab = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<ChangesTab>().flatten().expect("root")
            })
            .expect("window");
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });

        let mut cx = VisualTestContext::from_window(window.into(), cx);

        // Break git from outside, then let the next refresh run.
        std::fs::remove_dir_all(dir.0.join(".git")).expect("remove .git");
        cx.update(|_, app| tab.update(app, |tab, cx| tab.refresh(cx)));
        let tab_ref = tab.clone();
        for _ in 0..PUMP_BUDGET {
            if cx.read(|app| tab_ref.read_with(app, |tab, _| tab.git_error.is_some())) {
                break;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        // `debug_bounds` reads the last *drawn* frame; the full dispatcher
        // run drives the foreground effect cycle where the redraw lives
        // (VisualTestContext::run_until_parked only runs the background
        // executor, so it would leave the frame stale).
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-error").is_some(),
            "the git error renders as a panel, not as an empty list"
        );
        assert!(
            cx.debug_bounds("changes-retry").is_some(),
            "the Retry affordance is reachable (F-CHG-09)"
        );

        // Restore the repo, then click Retry: the panel goes away and the
        // list returns.
        git(&dir.0, &["init", "-q"]);
        git(&dir.0, &["config", "user.email", "t@example.invalid"]);
        git(&dir.0, &["config", "user.name", "Sirio tests"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("rewrite");
        let retry = cx
            .debug_bounds("changes-retry")
            .expect("the retry button is visible");
        cx.simulate_click(retry.center(), Modifiers::none());
        for _ in 0..PUMP_BUDGET {
            if cx.read(|app| tab_ref.read_with(app, |tab, _| tab.git_error.is_none())) {
                break;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-error").is_none(),
            "a successful Retry dismisses the error panel"
        );
        assert!(
            cx.debug_bounds("changes-list").is_some(),
            "Retry returns to the changes list"
        );
    }

    /// F-CHG-08/F-CHG-12: section and file expansion are behavior. The
    /// elements are found in the drawn frame, clicked at their laid-out
    /// bounds, and their state changes through GPUI dispatch. The diff line
    /// colors and typography are intentionally not asserted here.
    #[gpui::test]
    async fn drawn_changes_rows_expand_sections_and_context_bands(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        let original = (0..80)
            .map(|index| format!("line-{index}\n"))
            .collect::<String>();
        std::fs::write(dir.0.join("tracked.txt"), &original).expect("write long file");
        git(&dir.0, &["add", "tracked.txt"]);
        git(
            &dir.0,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "long"],
        );
        let mut changed = original.lines().map(str::to_owned).collect::<Vec<_>>();
        changed[1] = "first change".to_owned();
        changed[60] = "second change".to_owned();
        std::fs::write(dir.0.join("tracked.txt"), changed.join("\n") + "\n")
            .expect("modify long file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| {
            tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
        });
        cx.cx.run_until_parked();

        let expand_all = cx
            .debug_bounds("changes-expand-all")
            .expect("Expand All is in the drawn toolbar");
        cx.simulate_click(expand_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| {
                tab.expanded_changes
                    .contains(&(ChangeSection::Changed, PathBuf::from("tracked.txt")))
                    && !tab.expanded_bands.is_empty()
            }),
            "clicking the drawn Expand All control expands files and context bands"
        );

        let collapse_all = cx
            .debug_bounds("changes-collapse-all")
            .expect("Collapse All is in the drawn toolbar");
        cx.simulate_click(collapse_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| {
                tab.expanded_changes.is_empty() && tab.expanded_bands.is_empty()
            }),
            "clicking the drawn Collapse All control closes every expanded diff"
        );

        let section = cx
            .debug_bounds("changes-section-header")
            .expect("the section header is in the drawn frame");
        cx.simulate_click(section.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| tab
                .collapsed_sections
                .contains(&ChangeSection::Changed)),
            "clicking the drawn section header collapses it"
        );

        let section = cx
            .debug_bounds("changes-section-header")
            .expect("the collapsed section header remains clickable");
        cx.simulate_click(section.center(), Modifiers::none());
        cx.run_until_parked();
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is in the drawn frame");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| tab
                .expanded_changes
                .contains(&(ChangeSection::Changed, PathBuf::from("tracked.txt")))),
            "clicking the drawn file row expands its diff"
        );

        let band = cx
            .debug_bounds("changes-context-band")
            .expect("a collapsed context band is laid out in the drawn frame");
        cx.simulate_click(band.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| !tab.expanded_bands.is_empty()),
            "clicking the drawn context band expands the hidden lines"
        );
    }

    /// F-CHG-10: Stage and Unstage are exercised through the actual drawn
    /// row controls against a real checkout, rather than calling either
    /// operation directly.
    #[gpui::test]
    async fn a_stage_requested_during_refresh_runs_after_refresh_finishes(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let tab = cx.new(|_| settled_changes_tab(dir.0.clone()));
        assert_eq!(
            tab.read_with(cx, |tab, _| section_count(tab, "Changed")),
            1,
            "the settled test tab contains the modified file"
        );

        tab.update(cx, |tab, cx| {
            let _ = tab.git_task.take();
            tab.refresh(cx);
            assert!(tab.git_task.is_some(), "the refresh must be in flight");
            tab.stage_path(PathBuf::from("tracked.txt"), cx);
        });

        pump_until(cx, || {
            tab.read_with(cx, |tab, _| section_count(tab, "Staged") == 1)
        });
        assert_eq!(
            status(&dir.0)
                .expect("status after queued stage")
                .staged()
                .len(),
            1,
            "a Stage request made during refresh is executed after the refresh"
        );
    }

    #[gpui::test]
    async fn drawn_stage_and_unstage_buttons_mutate_the_real_checkout(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let stage = cx
            .debug_bounds("changes-stage")
            .expect("Stage is drawn after expanding the row");
        cx.simulate_click(stage.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Staged") == 1);
        assert_eq!(
            status(&dir.0).expect("status after stage").staged().len(),
            1,
            "the real Stage click updates the index"
        );

        cx.cx.run_until_parked();
        let staged_row = cx
            .debug_bounds("changes-file-row")
            .expect("the staged row is drawn after Stage");
        cx.simulate_click(staged_row.center(), Modifiers::none());
        cx.run_until_parked();
        let unstage = cx
            .debug_bounds("changes-unstage")
            .expect("Unstage is drawn after staging");
        cx.simulate_click(unstage.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        assert_eq!(
            status(&dir.0).expect("status after unstage").staged().len(),
            0,
            "the real Unstage click updates the index"
        );
    }

    /// A mutation error from the real drawn Changes tab must survive the
    /// periodic refreshes that still succeed through the stale index lock.
    #[gpui::test]
    async fn drawn_stage_error_stays_visible_across_periodic_refreshes(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        std::fs::write(dir.0.join(".git/index.lock"), b"").expect("create index lock");

        // `changes_view` is the same constructor used by + -> Changes. Its
        // first draw arms the real one-second refresh loop.
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let stage = cx
            .debug_bounds("changes-stage")
            .expect("Stage is drawn after expanding the row");
        cx.simulate_click(stage.center(), Modifiers::none());

        wait_for_tab(&cx, &tab, |tab| tab.git_error.is_some());
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-error").is_some(),
            "a failed drawn Stage renders the visible error state"
        );
        let error = tab
            .read_with(&cx.cx, |tab, _| tab.git_error.clone())
            .expect("the failed Stage keeps its error");
        assert!(
            error.contains("index.lock") || error.contains("did not finish"),
            "the visible error keeps git's lock failure detail: {error}"
        );

        // Status/snapshot refreshes continue to work with index.lock present.
        // A new untracked file makes each successful periodic snapshot
        // observable instead of merely waiting for virtual time to pass.
        for tick in 1..=3 {
            let path = format!("periodic-refresh-{tick}.txt");
            std::fs::write(dir.0.join(&path), format!("tick {tick}\n"))
                .expect("create refresh marker");
            wait_for_tab(&cx, &tab, |tab| {
                tab.entries
                    .iter()
                    .any(|entry| entry.path == Path::new(&path))
            });
            cx.cx.run_until_parked();
            assert!(
                tab.read_with(&cx.cx, |tab, _| tab.git_error.is_some()),
                "mutation error must survive successful periodic refresh {tick}"
            );
            assert!(
                cx.debug_bounds("changes-error").is_some(),
                "the visible error remains rendered after successful periodic refresh {tick}"
            );
        }

        // Retry is an explicit user action, so it may dismiss the mutation
        // error and make the refreshed Changes list usable again.
        std::fs::remove_file(dir.0.join(".git/index.lock")).expect("remove index lock");
        let retry = cx
            .debug_bounds("changes-retry")
            .expect("Retry remains visible in the mutation error state");
        cx.simulate_click(retry.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| tab.git_error.is_none());
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-error").is_none(),
            "an explicit Retry dismisses the retained mutation error"
        );
        assert!(
            cx.debug_bounds("changes-list").is_some(),
            "Retry returns the surface to the usable changes list"
        );
    }

    /// A file staged and then modified again sits in two sections, and each
    /// row names its own side of the split: what the index holds in Staged,
    /// what the worktree holds in Changed.
    #[test]
    fn a_file_in_two_sections_shows_each_sections_own_letter() {
        let entry = StatusEntry {
            path: PathBuf::from("a.rs"),
            original_path: None,
            index_status: Some(StatusKind::Added),
            worktree_status: Some(StatusKind::Modified),
        };
        assert_eq!(git_status(ChangeSection::Staged, &entry), GitStatus::Added);
        assert_eq!(git_status(ChangeSection::Changed, &entry), GitStatus::Modified);
    }

    /// A commit's or a range's rows carry their status in the index column
    /// (`commit_status_kind`), so a file the commit deletes reads D.
    #[test]
    fn a_commit_row_reads_its_status_from_the_index_column() {
        let entry = StatusEntry {
            path: PathBuf::from("gone.txt"),
            original_path: None,
            index_status: Some(commit_status_kind('D')),
            worktree_status: None,
        };
        assert_eq!(git_status(ChangeSection::Staged, &entry), GitStatus::Deleted);
    }

    /// B1 §13: a change request's diff is not "staged" anything. Its one
    /// section is headed, and reported, as Changes.
    #[test]
    fn a_range_reports_its_one_section_as_changes() {
        let report = range_tab_with_one_diff(false).report();
        let names: Vec<_> = report.sections.iter().map(|section| section.name).collect();
        assert!(names.contains(&"Changes"), "{names:?}");
        assert!(!names.contains(&"Staged"), "{names:?}");
    }

    #[test]
    fn a_changed_word_is_the_only_stretch_washed() {
        let (old, new) = changed_span("let x = 1;", "let x = 2;").expect("a stretch");
        assert_eq!((&"let x = 1;"[old], &"let x = 2;"[new]), ("1", "2"));
    }

    #[test]
    fn a_stretch_is_widened_to_whole_words() {
        let (old, new) = changed_span("let count = 10;", "let count = 12;").expect("a stretch");
        assert_eq!((&"let count = 10;"[old], &"let count = 12;"[new]), ("10", "12"));
    }

    /// Identical lines have nothing changed, and lines with nothing in common
    /// are already said by the line wash; a word wash over all of it adds noise.
    #[test]
    fn identical_or_wholly_different_lines_wash_no_words() {
        assert_eq!(changed_span("same", "same"), None);
        assert_eq!(changed_span("abc", "xyz"), None);
    }

    /// Review Focus 3: slicing a `String` off a character boundary panics.
    #[test]
    fn a_changed_stretch_falls_on_character_boundaries() {
        let (old_text, new_text) = ("città bella è", "città brutta è");
        let (old, new) = changed_span(old_text, new_text).expect("a stretch");
        assert_eq!((&old_text[old], &new_text[new]), ("bella", "brutta"));
        let (old, new) = changed_span("🙂 a", "🙃 a").expect("a stretch");
        assert_eq!((&"🙂 a"[old], &"🙃 a"[new]), ("🙂", "🙃"));
    }

    /// A run pairs its n-th deletion with its n-th addition — the side-by-side
    /// model's zip — and an addition left over has nothing to compare with.
    #[test]
    fn a_run_pairs_its_nth_deletion_with_its_nth_addition() {
        let line = |origin, content: &str| DiffLine {
            origin,
            old_line_number: None,
            new_line_number: None,
            content: content.to_string(),
        };
        let lines = [
            line(DiffOrigin::Deletion, "a = 1"),
            line(DiffOrigin::Deletion, "b = 1"),
            line(DiffOrigin::Addition, "a = 2"),
            line(DiffOrigin::Addition, "b = 2"),
            line(DiffOrigin::Addition, "c = 3"),
        ];
        let words = segment_words(&lines);
        let shown: Vec<Option<&str>> = words
            .iter()
            .zip(&lines)
            .map(|(range, line)| range.clone().map(|range| &line.content[range]))
            .collect();
        assert_eq!(shown, vec![Some("1"), Some("1"), Some("2"), Some("2"), None]);
    }

    /// An unavailable diff offers Retry, and Retry asks git again instead of
    /// keeping the old failure (`fetch_expanded_diff` skips a path that has one).
    #[gpui::test]
    async fn retry_on_an_unavailable_diff_asks_git_again(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let (base, head) = (rev_parse(&dir.0, "HEAD~1"), rev_parse(&dir.0, "HEAD"));
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_, cx| ChangesTab::for_range(dir.0.clone(), base, head, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, _| window.root::<ChangesTab>().flatten().expect("root"));
        wait_for_tab(&cx, &tab, |tab| !tab.entries.is_empty());
        let path = tab.read_with(&cx.cx, |tab, _| tab.entries[0].path.clone());
        tab.update(&mut cx.cx, |tab, cx| {
            tab.diffs.remove(&path);
            tab.diff_errors.insert(path.clone(), "fatal: transient".to_string());
            tab.expanded_changes.insert((ChangeSection::Staged, path.clone()));
            cx.notify();
        });
        cx.run_until_parked();
        let retry = cx.debug_bounds("changes-diff-retry").expect("Retry on the unavailable diff");
        cx.simulate_click(retry.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| tab.diffs.contains_key(&path) && !tab.diff_errors.contains_key(&path));
    }

    /// A confirm closes the dialog at once and queues exactly one discard, even
    /// pressed again before the refresh in flight finishes.
    #[gpui::test]
    async fn a_confirm_pressed_twice_discards_once(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "discard me\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        tab.update(&mut cx.cx, |tab, cx| {
            tab.refresh(cx);
            tab.ask_discard(PathBuf::from("tracked.txt"), cx);
            assert!(tab.confirm_discard(cx), "the first confirm runs");
            assert!(!tab.confirm_discard(cx), "the dialog is closed: a second confirm does nothing");
            assert_eq!(tab.pending_operations.len(), 1, "one discard is queued behind the refresh");
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 0);
    }

    /// GitHub anchors a file on the Files page by the SHA-256 of its path
    /// (`#diff-<hex>`); the value below is `printf 'src/login.rs' | sha256sum`.
    #[test]
    fn a_github_file_opens_at_its_anchor_on_the_files_page() {
        let files = ForgeFiles {
            forge: sirio_forge::Forge::GitHub,
            web_url: "https://ghe.test/acme/widgets/pull/101/".into(),
        };
        assert_eq!(
            forge_file_url(&files, Path::new("src/login.rs")),
            "https://ghe.test/acme/widgets/pull/101/files#diff-9aab51a5bbaf3c93f5972ab49fcaaca7a705b3258ba2dab5c387dfa8a85b7535"
        );
    }

    #[test]
    fn a_gitlab_file_opens_its_merge_requests_diffs() {
        let files = ForgeFiles {
            forge: sirio_forge::Forge::GitLab,
            web_url: "https://git.corp/acme/widgets/-/merge_requests/201".into(),
        };
        assert_eq!(
            forge_file_url(&files, Path::new("src/login.rs")),
            "https://git.corp/acme/widgets/-/merge_requests/201/diffs"
        );
    }

    #[gpui::test]
    async fn a_right_click_on_a_file_row_copies_its_path(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.run_until_parked();
        let row = cx.debug_bounds("changes-file-row").expect("the row").center();
        cx.simulate_mouse_down(row, gpui::MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(row, gpui::MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-menu-open-forge").is_none(), "a worktree row has no forge");
        let copy = cx.debug_bounds("changes-menu-copy-path").expect("Copy path").center();
        cx.simulate_click(copy, Modifiers::none());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("tracked.txt".to_string())
        );
    }

    #[gpui::test]
    async fn a_change_requests_row_offers_its_file_on_the_forge(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_, _| {
            let mut tab = range_tab_with_one_diff(false);
            tab.set_forge_files(ForgeFiles {
                forge: sirio_forge::Forge::GitHub,
                web_url: "https://ghe.test/acme/widgets/pull/101".into(),
            });
            tab
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let row = cx.debug_bounds("changes-file-row").expect("the row").center();
        cx.simulate_mouse_down(row, gpui::MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(row, gpui::MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-menu-open-forge").is_some());
    }

    /// Discard all throws away what its dialog listed, nothing more: a file an
    /// agent changed while the dialog was open was never shown, so it survives.
    #[gpui::test]
    async fn discard_all_throws_away_only_what_it_listed(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("other.txt"), "other\n").expect("seed other file");
        git(&dir.0, &["add", "other.txt"]);
        git(&dir.0, &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "other"]);
        std::fs::write(dir.0.join("tracked.txt"), "discard me\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        tab.update(&mut cx.cx, |tab, cx| tab.ask_discard_all(cx));
        std::fs::write(dir.0.join("other.txt"), "an agent's edit\n").expect("edit while the dialog is open");
        tab.update(&mut cx.cx, |tab, cx| {
            assert!(tab.confirm_discard(cx));
        });
        wait_for_tab(&cx, &tab, |tab| tab.git_task.is_none() && tab.pending_operations.is_empty());
        let paths: Vec<_> = status(&dir.0)
            .expect("status after discard all")
            .entries
            .into_iter()
            .map(|entry| entry.path)
            .collect();
        assert_eq!(paths, vec![PathBuf::from("other.txt")], "only the listed file was discarded");
    }

    /// A Discard whose path the surface no longer lists (the change is gone,
    /// or the socket named a path that never had one) discards nothing and
    /// raises no git error over the whole surface.
    #[gpui::test]
    async fn a_discard_of_a_path_the_surface_no_longer_lists_does_nothing(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        tab.update(&mut cx.cx, |tab, cx| {
            tab.ask_discard(PathBuf::from("never-was.txt"), cx);
            assert!(tab.confirm_discard(cx), "the dialog was open and is now closed");
        });
        wait_for_tab(&cx, &tab, |tab| tab.git_task.is_none() && tab.pending_operations.is_empty());
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.git_error.clone()), None);
        assert_eq!(tab.read_with(&cx.cx, |tab, _| section_count(tab, "Changed")), 1);
    }

    /// A confirm by mouse hands the keyboard back, as Cancel and Escape do:
    /// row navigation keeps working after a Discard.
    #[gpui::test]
    async fn confirming_discard_gives_the_keyboard_back(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "discard me\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.run_until_parked();
        let focus = tab.read_with(&cx.cx, |tab, _| tab.list_focus.clone().expect("list focus"));
        cx.update(|window, cx| focus.focus(window, cx));
        let row = cx.debug_bounds("changes-file-row").expect("the row");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let discard = cx.debug_bounds("changes-discard").expect("Discard");
        cx.simulate_click(discard.center(), Modifiers::none());
        cx.run_until_parked();
        let confirm = cx.debug_bounds("changes-discard-confirm").expect("the dialog's confirm");
        cx.simulate_click(confirm.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.update(|window, _| focus.is_focused(window)),
            "the list has the keyboard again"
        );
    }

    /// Ely's single-choice group empties its selection when the chosen face is
    /// pressed again; the surface must keep its mode rather than read that as
    /// "no mode".
    #[gpui::test]
    async fn pressing_the_chosen_view_again_keeps_it(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();
        let split = cx.debug_bounds("toggle split").expect("the Split face");
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| DiffViewMode::get(cx)), DiffViewMode::Split);
        let split = cx.debug_bounds("toggle split").expect("the Split face");
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| DiffViewMode::get(cx)),
            DiffViewMode::Split,
            "pressing the chosen face again keeps Split"
        );
    }

    /// F-CHG-14: the individual Discard control is laid out, opens the
    /// platform prompt, and only mutates the checkout after the real prompt
    /// answer says Discard.
    #[gpui::test]
    async fn drawn_discard_button_requires_confirmation_then_mutates_git(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "discard me\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let discard = cx
            .debug_bounds("changes-discard")
            .expect("Discard is drawn after expanding the row");
        cx.simulate_click(discard.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_some(), "Discard asks inside the window");
        assert!(!cx.cx.has_pending_prompt(), "and never through the system's own prompt");
        let cancel = cx.debug_bounds("changes-discard-cancel").expect("the dialog's Cancel");
        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_none(), "Cancel closes it");
        cx.run_until_parked();
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| section_count(tab, "Changed")),
            1,
            "Cancel leaves the real checkout changed"
        );

        let discard = cx
            .debug_bounds("changes-discard")
            .expect("Discard remains available after cancellation");
        cx.simulate_click(discard.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_some(), "Discard asks inside the window");
        assert!(!cx.cx.has_pending_prompt(), "and never through the system's own prompt");
        let confirm = cx.debug_bounds("changes-discard-confirm").expect("the dialog's confirm");
        cx.simulate_click(confirm.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 0);
        assert!(
            status(&dir.0)
                .expect("status after discard")
                .entries
                .is_empty(),
            "the confirmed Discard click restores the real checkout"
        );
    }

    /// The Refresh control never turns into a spinner. The toolbar used to
    /// swap the button for `loading::compact` for as long as `git_task` was
    /// in flight — and `ensure_refresh` puts a task in flight every second,
    /// so the icon blinked once a second for the duration of every
    /// `git status`. A refresh over a settled surface is silent: the list
    /// stays, the button stays, and the new snapshot lands in place.
    ///
    /// The in-flight state is faked with a task that never completes: a
    /// real snapshot load finishes inside `run_until_parked`, so the frame
    /// drawn afterwards would be the settled one and prove nothing.
    #[gpui::test]
    async fn a_refresh_in_flight_keeps_the_refresh_button_and_draws_no_spinner(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-refresh-spinner").is_none(),
            "a settled toolbar carries no spinner"
        );

        cx.update(|_, app| {
            tab.update(app, |tab, cx| {
                tab.git_task = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
                cx.notify();
            });
        });
        cx.run_until_parked();

        assert!(
            tab.read_with(&cx.cx, |tab, _| tab.git_task.is_some()),
            "the refresh is still in flight in the drawn frame"
        );
        assert!(
            cx.debug_bounds("changes-refresh-spinner").is_none(),
            "a refresh in flight must not draw a spinner in the toolbar"
        );
        assert!(
            cx.debug_bounds("changes-refresh").is_some(),
            "the Refresh control stays put while a refresh is in flight"
        );
        assert!(
            cx.debug_bounds("changes-file-row").is_some(),
            "the settled list stays on screen while a refresh is in flight"
        );
    }

    /// A Discard click must still open its confirmation dialog when a refresh
    /// is in flight; after confirmation, the mutation follows that refresh.
    #[gpui::test]
    async fn discard_during_refresh_keeps_confirmation_and_applies_afterward(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "discard me\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let discard = cx
            .debug_bounds("changes-discard")
            .expect("Discard is drawn after expanding the row");

        cx.update(|_, app| {
            tab.update(app, |tab, cx| tab.refresh(cx));
        });
        assert!(
            tab.read_with(&cx.cx, |tab, _| tab.git_task.is_some()),
            "the refresh must be in flight before Discard is clicked"
        );
        cx.simulate_click(discard.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_some(), "Discard still asks for confirmation during a refresh");
        assert!(!cx.cx.has_pending_prompt(), "and never through the system's own prompt");
        let confirm = cx.debug_bounds("changes-discard-confirm").expect("the dialog's confirm");
        cx.simulate_click(confirm.center(), Modifiers::none());

        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 0);
        assert!(
            status(&dir.0)
                .expect("status after deferred discard")
                .entries
                .is_empty(),
            "the confirmed Discard runs after the refresh"
        );
    }

    /// Narrow sidebars must keep the whole action cluster visible and clickable,
    /// including at the minimum width or a larger preferred interface font.
    #[gpui::test]
    async fn the_sidebar_diff_keeps_every_toolbar_action_inside_the_panel(
        cx: &mut TestAppContext,
    ) {
        struct EmbeddedDiff {
            tab: gpui::Entity<ChangesTab>,
            width: f32,
        }

        impl Render for EmbeddedDiff {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(self.width))
                    .h_full()
                    .overflow_hidden()
                    .child(self.tab.clone())
            }
        }

        cx.update(Theme::init);

        cx.update(crate::ely::init);
        for (width, font_size) in [(320.0, 13), (220.0, 13), (320.0, 18)] {
            cx.update(|cx| Theme::set_interface_font_size(font_size, cx));
            let dir = TempDir::new();
            clean_git_repo(&dir.0);
            std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
            let tab = cx.new(|cx| ChangesTab::in_right_panel(dir.0.clone(), cx));
            let tab_for_window = tab.clone();
            let window = cx.add_window(|_, _| EmbeddedDiff {
                tab: tab_for_window,
                width,
            });
            let mut view = VisualTestContext::from_window(window.into(), cx);
            wait_for_tab(&view, &tab, |tab| section_count(tab, "Changed") == 1);
            let panel = view
                .debug_bounds("changes-surface")
                .expect("sidebar diff drawn");
            let list = view.debug_bounds("changes-list").expect("diff list drawn");
            for selector in [
                "toggle unified",
                "toggle split",
                "changes-refresh",
                "changes-expand-all",
                "changes-collapse-all",
                "changes-stage-all",
                "changes-discard-all",
            ] {
                let button = view.debug_bounds(selector).expect("toolbar action drawn");
                assert!(
                    button.left() >= panel.left()
                        && button.right() <= panel.right()
                        && button.bottom() <= list.top(),
                    "{selector} must remain visible at {width}px and font {font_size}: \
                     button={button:?}, panel={panel:?}, list={list:?}"
                );
            }
            let discard = view
                .debug_bounds("changes-discard-all")
                .expect("Discard all drawn");
            view.simulate_click(discard.center(), Modifiers::none());
            view.run_until_parked();
            assert!(view.debug_bounds("changes-discard-dialog").is_some(), "the last toolbar action must take a click");
            assert!(!view.cx.has_pending_prompt(), "and never through the system's own prompt");
            let cancel = view.debug_bounds("changes-discard-cancel").expect("the dialog's Cancel");
            view.simulate_click(cancel.center(), Modifiers::none());
            view.run_until_parked();
            assert!(view.debug_bounds("changes-discard-dialog").is_none(), "Cancel closes it");
            view.run_until_parked();
        }
    }

    /// F-CHG-11: the section-level Stage all control is also a real drawn
    /// affordance, and clicking it stages every changed file.
    #[gpui::test]
    async fn drawn_stage_all_button_stages_every_changed_file(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        std::fs::write(dir.0.join("new.txt"), "new\n").expect("write untracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| {
            section_count(tab, "Changed") == 1 && section_count(tab, "Untracked") == 1
        });
        cx.cx.run_until_parked();
        let stage_all = cx
            .debug_bounds("changes-stage-all")
            .expect("Stage all is in the drawn toolbar");
        cx.simulate_click(stage_all.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Staged") == 2);
        assert_eq!(
            status(&dir.0)
                .expect("status after stage all")
                .staged()
                .len(),
            2,
            "Stage all stages every changed file"
        );
    }

    /// F-CHG-11: Discard all follows the same drawn-button and confirmation
    /// path as individual discard, but applies to the complete worktree.
    #[gpui::test]
    async fn drawn_discard_all_button_requires_confirmation_and_clears_worktree(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");
        std::fs::write(dir.0.join("new.txt"), "new\n").expect("write untracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| {
            section_count(tab, "Changed") == 1 && section_count(tab, "Untracked") == 1
        });
        cx.cx.run_until_parked();
        let discard_all = cx
            .debug_bounds("changes-discard-all")
            .expect("Discard all is in the drawn toolbar");
        cx.simulate_click(discard_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_some(), "Discard all asks for confirmation");
        assert!(!cx.cx.has_pending_prompt(), "and never through the system's own prompt");
        let cancel = cx.debug_bounds("changes-discard-cancel").expect("the dialog's Cancel");
        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_none(), "Cancel closes it");
        cx.run_until_parked();
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| {
                section_count(tab, "Changed") + section_count(tab, "Untracked")
            }),
            2,
            "Cancel leaves both worktree changes"
        );

        let discard_all = cx
            .debug_bounds("changes-discard-all")
            .expect("Discard all remains available after cancellation");
        cx.simulate_click(discard_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-discard-dialog").is_some(), "Discard asks inside the window");
        assert!(!cx.cx.has_pending_prompt(), "and never through the system's own prompt");
        let confirm = cx.debug_bounds("changes-discard-confirm").expect("the dialog's confirm");
        cx.simulate_click(confirm.center(), Modifiers::none());
        // `git restore --worktree` deliberately leaves untracked files alone
        // (`git clean` is the separate, more destructive action — see
        // `sirio_git::discard_all`). The confirmed click clears every
        // tracked worktree change and retains the untracked file; asserting
        // the retention is what makes the boundary honest instead of
        // wishing it away.
        wait_for_tab(&cx, &tab, |tab| {
            section_count(tab, "Changed") == 0 && section_count(tab, "Untracked") == 1
        });
        let after = status(&dir.0).expect("status after discard all");
        assert!(
            after.changes().is_empty() && after.staged().is_empty(),
            "confirmed Discard all clears every tracked worktree change"
        );
        assert_eq!(
            after
                .untracked()
                .iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>(),
            vec![PathBuf::from("new.txt")],
            "untracked files are deliberately retained by Discard all"
        );
    }

    /// F-CHG-11: the Staged section owns an Unstage all action, and that
    /// action uses the same free per-path `sirio_git::unstage` API as rows.
    #[gpui::test]
    async fn drawn_staged_section_unstage_all_moves_every_file_back(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("first.txt"), "first\n").expect("write first file");
        std::fs::write(dir.0.join("second.txt"), "second\n").expect("write second file");
        git(&dir.0, &["add", "-A"]);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Staged") == 2);
        cx.cx.run_until_parked();
        let unstage_all = cx
            .debug_bounds("changes-section-staged-all")
            .expect("the Staged section draws Unstage all");
        cx.simulate_click(unstage_all.center(), Modifiers::none());
        wait_for_tab(&cx, &tab, |tab| {
            section_count(tab, "Staged") == 0 && section_count(tab, "Untracked") == 2
        });

        assert!(
            status(&dir.0)
                .expect("status after Unstage all")
                .staged()
                .is_empty(),
            "Unstage all moves every staged file back to the worktree"
        );
    }

    /// A staged file in a checkout with an unborn HEAD (git init, nothing
    /// committed) shows its real line counts, not +0 −0 next to an expanded
    /// diff of real lines.
    #[gpui::test]
    async fn an_unborn_head_repo_shows_real_counts_not_zero(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        git(&dir.0, &["init", "-q"]);
        git(&dir.0, &["config", "user.email", "t@example.invalid"]);
        git(&dir.0, &["config", "user.name", "Sirio tests"]);
        std::fs::write(dir.0.join("new.txt"), "one\ntwo\nthree\nfour\n").expect("write");
        git(&dir.0, &["add", "new.txt"]);

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"new.txt")
            })
        });

        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        let staged = sections
            .iter()
            .find(|section| section.section == ChangeSection::Staged)
            .expect("the staged section must exist");
        let stat = staged
            .rows
            .iter()
            .find_map(|row| match row {
                ChangeRow::File { entry, stat, .. } if entry.path == *"new.txt" => Some(*stat),
                _ => None,
            })
            .expect("the staged row carries a stat");
        assert_eq!(
            stat,
            Some(DiffStat {
                additions: 4,
                deletions: 0,
                is_binary: false,
            }),
            "an unborn-HEAD repo must show real counts, not +0 −0"
        );
    }

    /// A missing `git` binary is reported as a missing binary (spawn
    /// failure), never classified as an unborn HEAD and rendered as empty.
    #[test]
    fn a_missing_git_reports_spawn_not_silence() {
        let missing =
            std::env::temp_dir().join(format!("sirio-changes-missing-{}", std::process::id()));
        let error = load_snapshot(&missing, &ChangesSource::WorkingTree, None)
            .expect_err("no repo, no git: the load must fail");
        assert!(
            error.contains("failed to spawn git"),
            "the missing binary is named, not hidden: {error}"
        );
        // The second half of the message is the operating system's own
        // text, and the operating system localizes it: the very same spawn
        // reads "No such file or directory (os error 2)" on Linux and
        // "Nome di directory non valido. (os error 267)" on an Italian
        // Windows. A hard-coded English fragment therefore asserts the
        // machine's locale, not the loader's behaviour, so the expected
        // text is taken from a spawn made to fail the same way here. The
        // claim is unchanged — the OS error is carried through verbatim,
        // so the user can act on it — only the way it is recognised is.
        let os_error = std::process::Command::new("git")
            .arg("--version")
            .current_dir(&missing)
            .output()
            .expect_err("spawning into the same missing directory must fail here too")
            .to_string();
        assert!(
            error.contains(&os_error),
            "the OS error is included so the user can act: {error} \
             (it must carry the spawn's own {os_error})"
        );
    }

    /// The perf bug this fix targets: `load_worktree_snapshot` used to call
    /// `diff_entry` for every changed file, every load — one `git diff`
    /// subprocess per file regardless of whether its row was expanded. With
    /// `expanded_paths` scoping the fetch, only the rows actually expanded
    /// get a diff; the rest keep their (cheap, batched) stat but no diff
    /// text, and `None` still means "fetch everything" for the first load.
    #[test]
    fn load_worktree_snapshot_only_fetches_diffs_for_expanded_paths() {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        for index in 0..5 {
            std::fs::write(dir.0.join(format!("f{index}.txt")), format!("v{index}\n"))
                .expect("seed changed file");
        }

        let expanded: HashSet<PathBuf> = [PathBuf::from("f0.txt"), PathBuf::from("f2.txt")].into();
        let snapshot =
            load_worktree_snapshot(&dir.0, Some(&expanded)).expect("snapshot load must succeed");

        assert_eq!(
            snapshot.entries.len(),
            5,
            "status still reports every changed file"
        );
        assert_eq!(
            snapshot.stats.len(),
            5,
            "the cheap batched stats still cover every file, expanded or not"
        );
        assert_eq!(
            snapshot.diffs.len(),
            2,
            "only the expanded paths get a fetched diff: {:?}",
            snapshot.diffs.keys().collect::<Vec<_>>()
        );
        assert!(snapshot.diffs.contains_key(Path::new("f0.txt")));
        assert!(snapshot.diffs.contains_key(Path::new("f2.txt")));
        assert!(!snapshot.diffs.contains_key(Path::new("f1.txt")));
        assert!(!snapshot.diffs.contains_key(Path::new("f3.txt")));
        assert!(!snapshot.diffs.contains_key(Path::new("f4.txt")));

        let eager = load_worktree_snapshot(&dir.0, None).expect("eager load must succeed");
        assert_eq!(
            eager.diffs.len(),
            5,
            "a `None` scope (the first load) still fetches every diff"
        );
    }

    /// A file whose diff could not be fetched renders an explicit
    /// "unavailable" row when expanded — never empty rows that could be
    /// mistaken for "no changes".
    #[test]
    fn an_expanded_file_whose_diff_failed_says_unavailable() {
        let tab = ChangesTab {
            repo_root: PathBuf::from("/tmp"),
            is_git: true,
            source: ChangesSource::WorkingTree,
            entries: vec![StatusEntry {
                path: PathBuf::from("x.rs"),
                original_path: None,
                index_status: Some(StatusKind::Modified),
                worktree_status: Some(StatusKind::Modified),
            }],
            diffs: HashMap::new(),
            stats: HashMap::new(),
            diff_errors: HashMap::from([(
                PathBuf::from("x.rs"),
                "git exited with status 128: fatal: no such file".to_string(),
            )]),
            expanded_changes: HashSet::new(),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: false,
            git_error: None,
            git_error_from_mutation: false,
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        };
        let entry = tab.entries[0].clone();
        let mut rows = Vec::new();
        tab.expand_diff(
            &mut rows,
            ChangeSection::Changed,
            &entry,
            DiffViewMode::Unified,
        );
        assert!(
            matches!(
                rows.as_slice(),
                [ChangeRow::Unavailable { message, .. }]
                    if message.contains("diff unavailable")
                        && message.contains("fatal: no such file")
            ),
            "the expanded row names the failure instead of rendering nothing"
        );
    }

    /// F-CHG-15: a binary file's expanded diff says so explicitly, rather
    /// than rendering an empty body next to sibling rows whose real diff
    /// lines are on the same frame — which would read as "no changes" for
    /// a file the status list plainly shows as dirty.
    #[test]
    fn an_expanded_binary_file_says_binary_diff_unavailable() {
        let tab = ChangesTab {
            repo_root: PathBuf::from("/tmp"),
            is_git: true,
            source: ChangesSource::WorkingTree,
            entries: vec![StatusEntry {
                path: PathBuf::from("image.bin"),
                original_path: None,
                index_status: None,
                worktree_status: Some(StatusKind::Modified),
            }],
            diffs: HashMap::from([(
                PathBuf::from("image.bin"),
                FileDiff {
                    path: PathBuf::from("image.bin"),
                    hunks: Vec::new(),
                    additions: 0,
                    deletions: 0,
                    is_binary: true,
                    is_submodule: false,
                },
            )]),
            stats: HashMap::new(),
            diff_errors: HashMap::new(),
            expanded_changes: HashSet::new(),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: false,
            git_error: None,
            git_error_from_mutation: false,
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        };
        let entry = tab.entries[0].clone();
        let mut rows = Vec::new();
        tab.expand_diff(
            &mut rows,
            ChangeSection::Changed,
            &entry,
            DiffViewMode::Unified,
        );
        assert!(
            matches!(
                rows.as_slice(),
                [ChangeRow::Unavailable { message, .. }] if message == "Binary diff unavailable"
            ),
            "a binary file's expansion must say so, not render nothing: {rows:?}"
        );
    }

    /// F-CHG-18: textual diffs expose the exact path and unified text that a
    /// terminal drop target receives; binary diffs deliberately do not.
    #[test]
    fn a_textual_diff_builds_the_terminal_drag_payload() {
        let diff = FileDiff {
            path: PathBuf::from("src/conflicted file.txt"),
            hunks: vec![sirio_git::Hunk {
                header: "@@ -1 +1 @@".to_string(),
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![
                    DiffLine {
                        origin: DiffOrigin::Deletion,
                        old_line_number: Some(1),
                        new_line_number: None,
                        content: "old".to_string(),
                    },
                    DiffLine {
                        origin: DiffOrigin::Addition,
                        old_line_number: None,
                        new_line_number: Some(1),
                        content: "new".to_string(),
                    },
                ],
            }],
            additions: 1,
            deletions: 1,
            is_binary: false,
            is_submodule: false,
        };

        assert_eq!(
            diff_payload(&diff),
            Some((
                PathBuf::from("src/conflicted file.txt"),
                "@@ -1 +1 @@\n-old\n+new\n".to_string(),
            ))
        );
    }

    /// A drop-target fixture standing in for a pane's real
    /// `on_drop::<(PathBuf, String)>` handler (`sirio_terminal`'s
    /// `TerminalView` owns the real one; `sirio_ui` cannot depend on
    /// `sirio_terminal`, so this in-crate stand-in receives the identical
    /// typed payload through GPUI's real drag machinery). The row's own
    /// `.on_drag(payload, ..)` in production `changes.rs` is the drag
    /// source under test here — nothing about the source half is faked.
    struct DiffDropTargetFixture {
        changes: gpui::Entity<ChangesTab>,
        received: std::rc::Rc<std::cell::RefCell<Option<(PathBuf, String)>>>,
    }

    impl gpui::Render for DiffDropTargetFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let received = self.received.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.changes.clone())
                .child(
                    div()
                        .id("pane-test-drop-target")
                        .debug_selector(|| "pane-test-drop-target".to_owned())
                        .h(px(200.0))
                        .on_drop::<(PathBuf, String)>(move |payload, _, _| {
                            *received.borrow_mut() = Some(payload.clone());
                        }),
                )
        }
    }

    /// F-EDIT-12: dragging a changed-file row out of the real Changes list
    /// and dropping it delivers the exact `(PathBuf, String)` diff payload
    /// a pane's drop target expects — driven through GPUI's real
    /// mouse-down/move/up drag path against the production
    /// `changes-file-row` drag source, not a synthetic stand-in for it.
    #[gpui::test]
    async fn a_drawn_change_row_drags_its_diff_payload_to_a_drop_target(cx: &mut TestAppContext) {
        use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point};

        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        cx.update(Theme::init);

        cx.update(crate::ely::init);
        let received = std::rc::Rc::new(std::cell::RefCell::new(None));
        let fixture_received = received.clone();
        let window = cx.add_window(|_window, cx| {
            let changes = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
            DiffDropTargetFixture {
                changes,
                received: fixture_received,
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, app| {
            window
                .root::<DiffDropTargetFixture>()
                .flatten()
                .expect("fixture root")
                .read(app)
                .changes
                .clone()
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();

        let source = cx
            .debug_bounds("changes-file-row")
            .expect("the real changed-file row is drawn");
        let target = cx
            .debug_bounds("pane-test-drop-target")
            .expect("the drop target is drawn");

        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(8.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_eq!(
            received.borrow().as_ref().map(|(path, _)| path.clone()),
            Some(PathBuf::from("tracked.txt")),
            "the drop target receives the dragged row's real path"
        );
        assert!(
            received
                .borrow()
                .as_ref()
                .is_some_and(|(_, text)| text.contains("changed")),
            "the drop target receives the row's real unified diff text"
        );
    }

    /// F-CHG-13: an expanded changed-file row exposes Open diff as a typed
    /// host action instead of silently duplicating the inline expansion.
    #[gpui::test]
    async fn clicking_a_drawn_open_diff_action_emits_the_changed_path(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify file");

        let (mut cx, tab) = panel_changes_view(cx, dir.0.clone());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&tab, move |_, event: &ChangesTabActionEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let open_diff = cx
            .debug_bounds("changes-open-diff")
            .expect("Open diff is drawn in the expanded row");
        cx.simulate_click(open_diff.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[ChangesTabActionEvent::OpenDiff(PathBuf::from(
                "tracked.txt"
            ))],
            "Open diff emits the repo-relative changed path"
        );
    }

    /// F-CHG-16: the conflict action emits the exact path that the host must
    /// pass to `TerminalView::for_conflict`.
    #[gpui::test]
    async fn clicking_a_drawn_conflict_resolve_action_emits_the_exact_path(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let path = PathBuf::from("src/conflicted file.txt");
        let window = cx.add_window(|_window, _cx| ChangesTab {
            repo_root: PathBuf::from("/repo"),
            is_git: true,
            source: ChangesSource::WorkingTree,
            entries: vec![StatusEntry {
                path: path.clone(),
                original_path: None,
                index_status: Some(StatusKind::Unmerged),
                worktree_status: Some(StatusKind::Unmerged),
            }],
            diffs: HashMap::new(),
            stats: HashMap::new(),
            expanded_changes: HashSet::from([
                (ChangeSection::Staged, path.clone()),
                (ChangeSection::Changed, path.clone()),
            ]),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: false,
            git_error: None,
            git_error_from_mutation: false,
            diff_errors: HashMap::new(),
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let tab = cx.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&tab, move |_, event: &ChangesTabActionEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        cx.run_until_parked();

        let resolve = cx
            .debug_bounds("changes-resolve")
            .expect("conflicted file draws Resolve in terminal");
        cx.simulate_click(resolve.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[ChangesTabActionEvent::ResolveInTerminal(path)],
            "the terminal seam receives the exact conflicted path"
        );
    }

    /// A repo whose one changed file carries every run shape the clause
    /// enumerates: context-only runs, a pure deletion run, an addition run
    /// longer than the deletion run it replaces (so the zip has to pad), and
    /// the paired context around them.
    fn side_by_side_fixture(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        std::fs::write(
            dir.join("sbs.txt"),
            "ctx-1\nctx-2\nctx-3\nctx-4\nctx-5\ndel-a\ndel-b\nctx-6\nctx-7\nctx-8\nctx-9\nold-1\nold-2\nctx-10\nctx-11\nctx-12\nctx-13\nctx-14\n",
        )
        .expect("seed side-by-side file");
        git(dir, &["add", "sbs.txt"]);
        git(
            dir,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"],
        );
        std::fs::write(
            dir.join("sbs.txt"),
            "ctx-1\nctx-2\nctx-3\nctx-4\nctx-5\nctx-6\nctx-7\nctx-8\nctx-9\nnew-1\nnew-2\nnew-3\nadd-only-x\nctx-10\nctx-11\nctx-12\nctx-13\nctx-14\n",
        )
        .expect("edit side-by-side file");
    }

    /// A repo whose one changed file carries a line far wider than any pane
    /// this surface is drawn in — the case that broke the layout.
    fn wide_line_fixture(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        let wide: String = (1..40).map(|i| format!("seg{i:03}-")).collect::<String>()
            + &"漢🙂界".repeat(24);
        std::fs::write(dir.join("wide.txt"), format!("head\n{wide}OLD\ntail\n"))
            .expect("seed wide file");
        git(dir, &["add", "wide.txt"]);
        git(
            dir,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"],
        );
        std::fs::write(dir.join("wide.txt"), format!("head\n{wide}NEW\ntail\n"))
            .expect("edit wide file");
    }

    #[gpui::test]
    async fn changes_unified_horizontal_bar_tracks_only_expanded_wide_code(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        wide_line_fixture(&dir.0);
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();

        let surface = cx.debug_bounds("changes-surface").expect("surface drawn");
        let track = cx
            .debug_bounds("changes-unified-horizontal-bar-track")
            .expect("expanded wide unified code draws its horizontal bar");
        let toolbar = cx
            .debug_bounds("toggle unified")
            .expect("toolbar remains drawn");
        assert!(track.left() >= surface.left() && track.right() <= surface.right());
        assert!(toolbar.left() >= surface.left() && toolbar.right() <= surface.right());
        let (max_width, viewport) = tab.read_with(&cx.cx, |tab, _| {
            (tab.unified_max_width, tab.unified_viewport)
        });
        assert!(max_width > viewport, "Unicode code width exceeds its viewport");
        let thumb = cx
            .debug_bounds("changes-unified-horizontal-bar-thumb")
            .expect("overflow has a draggable thumb");
        cx.simulate_mouse_move(thumb.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(thumb.center(), gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(thumb.center().x + px(8.0), thumb.center().y),
            gpui::MouseButton::Left,
            Modifiers::none(),
        );
        let end = gpui::point(track.right() + px(50.0), thumb.center().y);
        cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let x = tab.read_with(&cx.cx, |tab, _| tab.unified_x);
        assert_eq!(x, -(max_width - viewport));
        cx.simulate_resize(gpui::size(px(500.0), px(600.0)));
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| tab.unified_x) < px(0.0),
            "narrowing keeps X clamped inside its new overflow range"
        );
        cx.simulate_resize(gpui::size(px(10000.0), px(600.0)));
        cx.run_until_parked();
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
        });
        cx.run_until_parked();
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.unified_x), px(0.0));
        assert!(
            cx.debug_bounds("changes-unified-horizontal-bar-track").is_none(),
            "wide viewport should remove overflow: max/viewport={:?}",
            tab.read_with(&cx.cx, |tab, _| (tab.unified_max_width, tab.unified_viewport))
        );
        let split = cx.debug_bounds("toggle split").unwrap();
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        let unified = cx.debug_bounds("toggle unified").unwrap();
        cx.simulate_click(unified.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.unified_x), px(0.0));

        // Collapsing the only wide row removes its code and its bar.
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("file row remains drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-unified-horizontal-bar-track").is_none());
    }

    fn split_horizontal_fixture(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        let wide: String = (1..40).map(|i| format!("seg{i:03}-")).collect::<String>()
            + &"漢🙂界".repeat(24);
        std::fs::write(dir.join("split.txt"), format!("head\n{wide}OLD\ntail\n"))
            .expect("seed split file");
        git(dir, &["add", "split.txt"]);
        git(
            dir,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"],
        );
        std::fs::write(dir.join("split.txt"), "head\nshort replacement\ntail\n")
            .expect("write short replacement");
    }

    #[gpui::test]
    async fn changes_split_horizontal_scrolls_each_code_column_independently(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        split_horizontal_fixture(&dir.0);
        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        let row = cx.debug_bounds("changes-file-row").expect("changed row draws");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let split = cx.debug_bounds("toggle split").unwrap();
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
        });
        cx.run_until_parked();

        let left = cx.debug_bounds("changes-split-left").unwrap();
        let right = cx.debug_bounds("changes-split-right").unwrap();
        let hunk = cx.debug_bounds("changes-hunk-row").expect("full-width hunk header draws");
        assert!((f32::from(left.size.width) - f32::from(right.size.width)).abs() < 1.0);
        let track = cx
            .debug_bounds("changes-split-left-horizontal-bar-track")
            .expect("long old-side code draws a left horizontal bar");
        assert!(cx.debug_bounds("changes-split-right-horizontal-bar-track").is_none());
        assert!(cx.debug_bounds("changes-unified-horizontal-bar-track").is_none());
        let thumb = cx
            .debug_bounds("changes-split-left-horizontal-bar-thumb")
            .unwrap();
        cx.simulate_mouse_move(thumb.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(thumb.center(), gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(thumb.center().x + px(8.0), thumb.center().y),
            gpui::MouseButton::Left,
            Modifiers::none(),
        );
        let end = gpui::point(track.right() + px(50.0), thumb.center().y);
        cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let left_x = tab.read_with(&cx.cx, |tab, _| tab.split_left_x);
        assert!(left_x < px(0.0));
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.split_right_x), px(0.0));
        assert_eq!(cx.debug_bounds("changes-split-left").unwrap(), left);
        assert_eq!(cx.debug_bounds("changes-split-right").unwrap(), right);
        assert_eq!(cx.debug_bounds("changes-hunk-row").unwrap(), hunk);

        let wide: String = (1..40).map(|i| format!("seg{i:03}-")).collect::<String>()
            + &"漢🙂界".repeat(24);
        std::fs::write(
            dir.0.join("split.txt"),
            format!("head\nshort replacement\n{wide}\ntail\n"),
        )
        .expect("add long new-side code");
        tab.update(&mut cx.cx, |tab, cx| tab.refresh(cx));
        wait_for_tab(&cx, &tab, |tab| tab.split_right_max_width > tab.split_viewport);
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
        });
        cx.run_until_parked();
        let right_track = cx
            .debug_bounds("changes-split-right-horizontal-bar-track")
            .expect("long new-side code draws a right horizontal bar");
        let right_thumb = cx.debug_bounds("changes-split-right-horizontal-bar-thumb").unwrap();
        cx.simulate_mouse_move(right_thumb.center(), None, Modifiers::none());
        cx.update(|window, app| window.draw(app).clear(app));
        cx.simulate_mouse_down(right_thumb.center(), gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(right_thumb.center().x + px(8.0), right_thumb.center().y),
            gpui::MouseButton::Left,
            Modifiers::none(),
        );
        let right_end = gpui::point(right_track.right() + px(50.0), right_thumb.center().y);
        cx.simulate_mouse_move(right_end, gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(right_end, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.split_left_x), left_x);
        assert!(tab.read_with(&cx.cx, |tab, _| tab.split_right_x) < px(0.0));

        let split_positions = tab.read_with(&cx.cx, |tab, _| {
            (tab.split_left_x, tab.split_right_x)
        });
        let unified = cx.debug_bounds("toggle unified").unwrap();
        cx.simulate_click(unified.center(), Modifiers::none());
        cx.run_until_parked();
        let split = cx.debug_bounds("toggle split").unwrap();
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
        });
        cx.run_until_parked();
        let (restored, left_width, right_width, viewport) = tab.read_with(&cx.cx, |tab, _| {
            (
                (tab.split_left_x, tab.split_right_x),
                tab.split_left_max_width,
                tab.split_right_max_width,
                tab.split_viewport,
            )
        });
        assert!(
            left_width > viewport && right_width > viewport,
            "both sides still overflow"
        );
        assert_eq!(
            restored, split_positions,
            "Split → Unified → Split preserves valid X offsets"
        );
        assert_eq!(tab.read_with(&cx.cx, |tab, _| tab.unified_x), px(0.0));
        let file = cx.debug_bounds("changes-file-row").unwrap();
        cx.simulate_click(file.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("changes-split-left-horizontal-bar-track").is_none());
        assert!(cx.debug_bounds("changes-split-right-horizontal-bar-track").is_none());
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| (tab.split_left_x, tab.split_right_x)),
            (px(0.0), px(0.0)),
            "collapsing expanded split content resets offsets with no remaining overflow"
        );
    }

    /// "Long lines must scroll inside the diff, never scroll the window as a
    /// whole." Driven on the 1715×972 lane, Split mode did the opposite:
    /// choosing Split over a file with a 276-character line pushed the Files
    /// panel clean off the right edge of the window and took the Changes
    /// toolbar's own action cluster with it.
    ///
    /// The cause was that the split rendering reserved room for its widest
    /// line on a wrapper inside the scroll container, and that reservation
    /// propagated out of it into the workspace. `min_w(px(0.0))` on the
    /// container — the CSS answer — got the rows laying out and did *not*
    /// stop the surface growing (`/tmp/n1-fix/07-06-split-settled.png`), so
    /// the reservation is gone entirely and each column is now exactly half
    /// of whatever width the surface was given. See `SPLIT_DIVIDER_WIDTH`.
    #[gpui::test]
    async fn a_wide_line_scrolls_inside_the_diff_instead_of_widening_the_surface(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        wide_line_fixture(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        let surface = cx
            .debug_bounds("changes-surface")
            .expect("the Changes surface is drawn");

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let split = cx
            .debug_bounds("toggle split")
            .expect("the view-mode control draws a Split segment");
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("changes-split-pair-replacement").is_some(),
            "the wide line really is being drawn side by side"
        );
        let list = cx
            .debug_bounds("changes-list")
            .expect("the scrolling list is drawn");
        let after = cx
            .debug_bounds("changes-surface")
            .expect("the surface is still drawn");
        assert!(
            f32::from(list.size.width) <= f32::from(surface.size.width) + 1.0,
            "the diff list ({}) grew past the surface it lives in ({}) — the \
             reservation escaped the scroll container",
            f32::from(list.size.width),
            f32::from(surface.size.width)
        );
        assert!(
            (f32::from(after.size.width) - f32::from(surface.size.width)).abs() < 1.0,
            "choosing Split changed the surface's own width ({} -> {}), which is \
             how the Files panel got pushed off the window",
            f32::from(surface.size.width),
            f32::from(after.size.width)
        );
        // …and the toolbar it shares the surface with is still reachable.
        assert!(
            cx.debug_bounds("toggle unified").is_some(),
            "the Unified segment is still on screen to switch back with"
        );
    }

    /// A long trailing-context header must stay on its own fixed-height row:
    /// the following collapsed context band is the first visible row that
    /// exposed the old wrap-without-height behavior.
    #[gpui::test]
    async fn drawn_long_hunk_header_stays_inside_its_row_above_hidden_lines(
        cx: &mut TestAppContext,
    ) {
        let path = PathBuf::from("README.md");
        let diff = FileDiff {
            path: path.clone(),
            hunks: vec![sirio_git::Hunk {
                header: "@@ -1,24 +1,26 @@ # Sirio — a native app for macOS, Linux, and Windows with a deliberately long trailing context description".to_owned(),
                old_start: 1,
                old_lines: 24,
                new_start: 1,
                new_lines: 26,
                lines: (1..=24)
                    .map(|line| DiffLine {
                        origin: DiffOrigin::Context,
                        old_line_number: Some(line),
                        new_line_number: Some(line),
                        content: format!("unchanged-{line}"),
                    })
                    .chain(std::iter::once(DiffLine {
                        origin: DiffOrigin::Addition,
                        old_line_number: None,
                        new_line_number: Some(25),
                        content: "first change".to_owned(),
                    }))
                    .collect(),
            }],
            additions: 1,
            deletions: 0,
            is_binary: false,
            is_submodule: false,
        };
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.open_window(gpui::size(px(330.0), px(600.0)), move |_window, _cx| {
            ChangesTab {
                repo_root: PathBuf::from("/repo"),
                is_git: true,
                source: ChangesSource::WorkingTree,
                entries: vec![StatusEntry {
                    path: path.clone(),
                    original_path: None,
                    index_status: None,
                    worktree_status: Some(StatusKind::Modified),
                }],
                diffs: HashMap::from([(path.clone(), diff)]),
                stats: HashMap::from([(
                    path,
                    DiffStat {
                        additions: 1,
                        deletions: 0,
                        is_binary: false,
                    },
                )]),
                expanded_changes: HashSet::from([(
                    ChangeSection::Changed,
                    PathBuf::from("README.md"),
                )]),
                collapsed_sections: HashSet::new(),
                expanded_bands: HashSet::new(),
                git_task: None,
                pending_operations: VecDeque::new(),
                has_loaded: true,
                git_error: None,
                git_error_from_mutation: false,
                diff_errors: HashMap::new(),
                refresh_started: false,
                embedded_in_panel: false,
                renders: 0,
                renders_at_last_tick: 0,
                refresh_suspended: false,
                suspended_ticks: 0,
                pending_focus: Vec::new(),
                last_focus: None,
                reveal_line: None,
                selected_change: None,
                discard_ask: None,
                forge_files: None,
                list_focus: None,
                list_state: new_list_state(),
                list_fingerprint: 0,
                reveal_selected: false,
                unified_x: px(0.0),
                unified_mode: DiffViewMode::Unified,
                unified_max_width: px(0.0),
                unified_viewport: px(0.0),
                unified_width_dirty: true,
                unified_bar_state: HorizontalBarState::default(),
                split_left_x: px(0.0),
                split_right_x: px(0.0),
                split_left_max_width: px(0.0),
                split_right_max_width: px(0.0),
                split_viewport: px(0.0),
                split_left_bar_state: HorizontalBarState::default(),
                split_right_bar_state: HorizontalBarState::default(),
                annotations: Vec::new(),
                annotation_views: Rc::default(),
                open_threads: Rc::default(),
                reveal_annotation: None,
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, app| {
            window.refresh();
            window.simulate_next_frame(app);
        });

        let pane = cx
            .debug_bounds("changes-surface")
            .expect("the narrow Changes pane is drawn");
        let header = cx
            .debug_bounds("changes-hunk-header")
            .expect("the hunk header text is drawn");
        let hidden = cx
            .debug_bounds("changes-context-band")
            .expect("the collapsed hidden-lines row is drawn");
        assert!(
            !header.intersects(&hidden),
            "the hunk header must not overlap the hidden-lines row: \
             header={header:?} hidden={hidden:?}"
        );
        assert!(
            header.left() >= pane.left() && header.right() <= pane.right(),
            "the hunk header must stay inside the narrow pane: \
             pane={pane:?} header={header:?}"
        );
    }

    /// A reviewer copies a line of code out of a diff. Both view modes drew
    /// it as a plain string; this drags across the real drawn line — in a
    /// real repository, through real clicks — and reads the clipboard.
    #[gpui::test]
    async fn a_line_of_a_diff_can_be_selected_and_copied_in_unified_and_split_view(
        cx: &mut TestAppContext,
    ) {
        use crate::text_selection::testing::copy_line;

        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("a.txt"), "alpha\nbeta\n").expect("seed file");
        git(&dir.0, &["add", "a.txt"]);
        git(
            &dir.0,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"],
        );
        std::fs::write(dir.0.join("a.txt"), "alpha\nBETA changed\n").expect("edit file");

        let repo = dir.0.clone();
        let (tab, _, cx) =
            crate::text_selection::testing::host(cx, move |_, cx| ChangesTab::new(repo, cx));
        wait_for_tab(cx, &tab, |tab| {
            tab.entries.iter().any(|entry| entry.path == *"a.txt")
        });
        cx.run_until_parked();

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();

        // Unified: the last row drawn is the added line, and only its code
        // is taken — no gutter numbers, no "+".
        assert_eq!(
            copy_line(cx, "changes-diff-content").as_deref(),
            Some("BETA changed")
        );

        let split = cx
            .debug_bounds("toggle split")
            .expect("the view-mode control draws a Split segment");
        cx.simulate_click(split.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            copy_line(cx, "changes-split-new-content").as_deref(),
            Some("BETA changed")
        );
        assert_eq!(
            copy_line(cx, "changes-split-old-content").as_deref(),
            Some("beta")
        );
    }

    /// F-GIT-DIFF-03. The whole clause, in one drawn frame reached by real
    /// clicks: expand the file, click the **Split** segment of the view-mode
    /// control, and read the four row shapes plus the full-width hunk header
    /// out of the rendered frame.
    ///
    /// Every assertion here is on `debug_bounds` — the drawn frame — not on
    /// the model: the model half already had a green test in `sirio_git`
    /// and no surface, which is precisely why this row was `UNREACHABLE`.
    #[gpui::test]
    async fn clicking_split_draws_paired_context_zipped_runs_and_full_width_hunks(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        side_by_side_fixture(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-diff-line").is_some(),
            "the expanded file starts in Unified mode"
        );
        assert!(
            cx.debug_bounds("changes-split-pair-context").is_none(),
            "no side-by-side row is drawn before the mode is chosen"
        );

        let split_segment = cx
            .debug_bounds("toggle split")
            .expect("the view-mode control draws a Split segment");
        cx.simulate_click(split_segment.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-diff-line").is_none(),
            "Split replaces the unified rows rather than drawing both"
        );

        // The context runs of this fixture are all long enough to collapse
        // into "N hidden lines" bands, in either mode — the band keys are
        // computed from the same unified line stream in both, which is what
        // lets Expand All open them here at all. Opening them is what puts
        // paired context rows on screen.
        let expand_all = cx
            .debug_bounds("changes-expand-all")
            .expect("Expand All is drawn");
        cx.simulate_click(expand_all.center(), Modifiers::none());
        cx.run_until_parked();
        let paired = cx
            .debug_bounds("changes-split-pair-context")
            .expect("a context line is paired onto both sides");
        assert!(
            cx.debug_bounds("changes-split-pair-replacement").is_some(),
            "a deletion is zipped against the addition that replaced it"
        );
        assert!(
            cx.debug_bounds("changes-split-left-only").is_some(),
            "the pure deletion run leaves the right side of its rows empty"
        );
        assert!(
            cx.debug_bounds("changes-split-right-only").is_some(),
            "the additions with no deletion opposite them pad the left side"
        );
        assert!(
            cx.debug_bounds("changes-split-empty").is_none(),
            "no row is drawn with both sides absent"
        );

        let left = cx
            .debug_bounds("changes-split-left")
            .expect("the left column is drawn");
        let right = cx
            .debug_bounds("changes-split-right")
            .expect("the right column is drawn");
        assert!(
            (f32::from(left.size.width) - f32::from(right.size.width)).abs() < 1.0,
            "the two columns are the same width ({} vs {}) — they have to line up down the whole diff",
            f32::from(left.size.width),
            f32::from(right.size.width)
        );
        let hunk = cx
            .debug_bounds("changes-hunk-row")
            .expect("the hunk header is drawn in Split mode");
        assert!(
            f32::from(hunk.size.width) > f32::from(left.size.width) * 1.5,
            "the hunk header spans the full row ({}) rather than one column ({})",
            f32::from(hunk.size.width),
            f32::from(left.size.width)
        );
        assert!(
            (f32::from(hunk.size.width) - f32::from(paired.size.width)).abs() < 2.0,
            "the hunk header is exactly as wide as the paired rows under it"
        );
    }

    /// The mode is a preference, not a per-tab accident: a second Changes
    /// surface constructed after the choice opens in Split too. `main.rs`
    /// rebuilds every `ChangesTab` when a worktree rebinds, so a per-tab
    /// field would silently forget the choice.
    #[gpui::test]
    async fn the_chosen_view_mode_is_remembered_by_a_later_changes_surface(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        side_by_side_fixture(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        assert_eq!(
            cx.update(|_, app| DiffViewMode::get(app)),
            DiffViewMode::Unified,
            "the default is the unified reading"
        );

        let split_segment = cx
            .debug_bounds("toggle split")
            .expect("the view-mode control draws a Split segment");
        cx.simulate_click(split_segment.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, app| DiffViewMode::get(app)),
            DiffViewMode::Split,
            "the click records the choice app-wide"
        );

        // A brand-new surface over the same checkout — what `main.rs` does
        // on every worktree rebind.
        let second = cx.update(|_, app| app.new(|cx| ChangesTab::new(dir.0.clone(), cx)));
        let mode = second.read_with(&cx.cx, |_, cx| DiffViewMode::get(cx));
        assert_eq!(
            mode,
            DiffViewMode::Split,
            "a Changes surface built after the choice opens in the chosen mode"
        );

        let unified_segment = cx
            .debug_bounds("toggle unified")
            .expect("the view-mode control draws a Unified segment");
        cx.simulate_click(unified_segment.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, app| DiffViewMode::get(app)),
            DiffViewMode::Unified,
            "the control switches back"
        );
        assert!(
            cx.debug_bounds("changes-diff-line").is_some()
                || cx.debug_bounds("changes-file-row").is_some(),
            "the surface still draws after switching back"
        );
    }

    /// The `↗` action opened an editor tab that read "This file does not
    /// exist" for a file that plainly does: it emitted `entry.path`, which
    /// is repo-relative, and the host opens the editor on whatever it is
    /// handed. The path that crosses the seam must resolve on disk.
    #[gpui::test]
    async fn the_open_file_action_emits_a_path_that_exists_on_disk(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, app| {
            app.subscribe(&tab, move |_, event: &ChangesTabEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let open = cx
            .debug_bounds("changes-open-file")
            .expect("the expanded row draws the open-in-editor action");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();

        let emitted = events.borrow();
        let [ChangesTabEvent::OpenFile(path)] = emitted.as_slice() else {
            panic!("expected exactly one OpenFile, got {emitted:?}");
        };
        assert!(
            path.is_absolute(),
            "the host opens the editor on this path verbatim, so it must be absolute: {}",
            path.display()
        );
        assert!(
            path.exists(),
            "the emitted path does not exist on disk: {}",
            path.display()
        );
        assert_eq!(path, &dir.0.join("tracked.txt"));
    }

    /// A commit-mode surface lists exactly the files that commit touched and
    /// refuses every mutation: the commit is immutable, so there is nothing
    /// to stage, unstage or discard.
    #[gpui::test]
    async fn a_commit_view_lists_that_commit_s_files_and_forbids_staging(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        // The second commit added exactly one file; asking for it proves the
        // list comes from `git show <sha>` and not from the working tree
        // (the worktree itself is clean here).
        let sha = rev_parse(&dir.0, "HEAD");

        let tab = cx.new(|cx| ChangesTab::for_commit(dir.0.clone(), sha, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.report()
                    .sections
                    .iter()
                    .any(|section| !section.files.is_empty())
            })
        });

        tab.read_with(cx, |tab, _| {
            let report = tab.report();
            let files: Vec<_> = report
                .sections
                .iter()
                .flat_map(|section| section.files.iter())
                .collect();
            assert!(
                files.iter().any(|file| file.path.ends_with("b.txt")),
                "the commit's own files are listed, got: {:?}",
                files.iter().map(|file| &file.path).collect::<Vec<_>>()
            );
            assert!(
                files.iter().all(|file| file.path != *"a.txt"),
                "files outside the commit must not leak in from the worktree"
            );
            assert!(!tab.allows_staging(), "a commit is immutable");
        });
    }

    // ------------------------------------------------------------------
    // [PERF-diff]: the regression test for "opening a very large diff makes
    // the diff's scroll and the whole app lag". One expanded file whose
    // diff carries `lines` rows, drawn into a fixed 1200x800 window with no
    // git process anywhere (a commit-mode surface never arms the refresh
    // loop). Every number is wall-clock; run with `--nocapture` to read
    // them.
    // ------------------------------------------------------------------

    pub(super) fn synthetic_big_diff_tab(lines: usize) -> ChangesTab {
        let path = PathBuf::from("src/big_file.rs");
        let mut diff_lines = Vec::with_capacity(lines);
        let mut old_no = 1usize;
        let mut new_no = 1usize;
        let mut additions = 0usize;
        let mut deletions = 0usize;
        // 3 context / 10 deletions / 10 additions, repeated: the context
        // runs stay under CONTEXT_BAND_MIN so nothing collapses into a band
        // and the row count really is the line count.
        let mut i = 0usize;
        while diff_lines.len() < lines {
            let phase = i % 23;
            let (origin, old, new) = if phase < 3 {
                let l = (DiffOrigin::Context, Some(old_no), Some(new_no));
                old_no += 1;
                new_no += 1;
                l
            } else if phase < 13 {
                let l = (DiffOrigin::Deletion, Some(old_no), None);
                old_no += 1;
                deletions += 1;
                l
            } else {
                let l = (DiffOrigin::Addition, None, Some(new_no));
                new_no += 1;
                additions += 1;
                l
            };
            diff_lines.push(DiffLine {
                origin,
                old_line_number: old,
                new_line_number: new,
                content: format!(
                    "    let value_{i} = compute_something(argument_{i}, other_{i}); // padding"
                ),
            });
            i += 1;
        }
        let diff = FileDiff {
            path: path.clone(),
            hunks: vec![sirio_git::Hunk {
                header: format!("@@ -1,{old_no} +1,{new_no} @@"),
                old_start: 1,
                old_lines: old_no,
                new_start: 1,
                new_lines: new_no,
                lines: diff_lines,
            }],
            additions,
            deletions,
            is_binary: false,
            is_submodule: false,
        };
        let entry = StatusEntry {
            path: path.clone(),
            original_path: None,
            index_status: Some(StatusKind::Modified),
            worktree_status: None,
        };
        let mut expanded_changes = HashSet::new();
        expanded_changes.insert((ChangeSection::Staged, path.clone()));
        let mut diffs = HashMap::new();
        diffs.insert(path.clone(), diff);
        let mut stats = HashMap::new();
        stats.insert(
            path,
            DiffStat {
                additions,
                deletions,
                is_binary: false,
            },
        );
        ChangesTab {
            repo_root: PathBuf::from("."),
            is_git: true,
            source: ChangesSource::Commit("synthetic".to_owned()),
            entries: vec![entry],
            diffs,
            stats,
            expanded_changes,
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            pending_operations: VecDeque::new(),
            has_loaded: true,
            git_error: None,
            git_error_from_mutation: false,
            diff_errors: HashMap::new(),
            refresh_started: false,
            embedded_in_panel: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            pending_focus: Vec::new(),
            last_focus: None,
            reveal_line: None,
            selected_change: None,
            discard_ask: None,
            forge_files: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
            unified_x: px(0.0),
            unified_mode: DiffViewMode::Unified,
            unified_max_width: px(0.0),
            unified_viewport: px(0.0),
            unified_width_dirty: true,
            unified_bar_state: HorizontalBarState::default(),
            split_left_x: px(0.0),
            split_right_x: px(0.0),
            split_left_max_width: px(0.0),
            split_right_max_width: px(0.0),
            split_viewport: px(0.0),
            split_left_bar_state: HorizontalBarState::default(),
            split_right_bar_state: HorizontalBarState::default(),
            annotations: Vec::new(),
            annotation_views: Rc::default(),
            open_threads: Rc::default(),
            reveal_annotation: None,
        }
    }

    struct DiffFrameCost {
        rows: usize,
        section_rows_ms: f64,
        frame_ms: f64,
        scroll_dispatch_ms: f64,
        scroll_frame_ms: f64,
    }

    fn measure_big_diff(cx: &mut TestAppContext, lines: usize) -> DiffFrameCost {
        use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase, point, size};
        use std::time::Instant;
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(|_window, _cx| synthetic_big_diff_tab(lines));
        let handle: gpui::AnyWindowHandle = window.into();
        let mut cx = VisualTestContext::from_window(handle, cx);
        cx.simulate_resize(size(px(1200.0), px(800.0)));
        let tab = cx.update(|window, _| {
            window
                .root::<ChangesTab>()
                .flatten()
                .expect("changes tab root")
        });
        let frame = |cx: &mut VisualTestContext, tab: &gpui::Entity<ChangesTab>| -> f64 {
            cx.update(|window, cx| {
                tab.update(cx, |_, cx| cx.notify());
                let start = Instant::now();
                window.draw(cx).clear(cx);
                start.elapsed().as_secs_f64() * 1000.0
            })
        };
        // Warm-up: the first frame pays for text-system caches, not the bug.
        frame(&mut cx, &tab);
        let (section_rows_ms, rows) = tab.read_with(&cx.cx, |tab, _| {
            let start = Instant::now();
            let sections = tab.section_rows(DiffViewMode::Unified);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            (
                elapsed,
                sections.iter().map(|s| s.rows.len()).sum::<usize>(),
            )
        });
        // The minimum, not the median: this test runs alongside every other
        // crate's test binary under `cargo test --workspace`, and a stall
        // from a linker next door must not read as a slow frame. The bug is
        // a lower bound -- a frame that *cannot* be faster than the file --
        // and the minimum is the statistic that measures a lower bound.
        let frame_ms = (0..5)
            .map(|_| frame(&mut cx, &tab))
            .fold(f64::INFINITY, f64::min);

        let list = cx
            .debug_bounds("changes-list")
            .expect("the scrolling list is drawn");
        let mut scroll_dispatch: Vec<f64> = Vec::new();
        let mut scroll_frames: Vec<f64> = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            cx.simulate_event(ScrollWheelEvent {
                position: list.center(),
                delta: ScrollDelta::Lines(point(0.0, -3.0)),
                modifiers: Modifiers::none(),
                touch_phase: TouchPhase::Moved,
            });
            scroll_dispatch.push(start.elapsed().as_secs_f64() * 1000.0);
            scroll_frames.push(cx.update(|window, cx| {
                let start = Instant::now();
                window.draw(cx).clear(cx);
                start.elapsed().as_secs_f64() * 1000.0
            }));
        }
        DiffFrameCost {
            rows,
            section_rows_ms,
            frame_ms,
            scroll_dispatch_ms: scroll_dispatch.into_iter().fold(f64::INFINITY, f64::min),
            scroll_frame_ms: scroll_frames.into_iter().fold(f64::INFINITY, f64::min),
        }
    }

    // ---- a change request's range ----------------------------------------

    fn ctx_line(n: usize) -> DiffLine {
        DiffLine {
            origin: DiffOrigin::Context,
            old_line_number: Some(n),
            new_line_number: Some(n),
            content: format!("line {n}"),
        }
    }

    fn add_line(n: usize) -> DiffLine {
        DiffLine {
            origin: DiffOrigin::Addition,
            old_line_number: None,
            new_line_number: Some(n),
            content: format!("new {n}"),
        }
    }

    fn del_line(n: usize) -> DiffLine {
        DiffLine {
            origin: DiffOrigin::Deletion,
            old_line_number: Some(n),
            new_line_number: None,
            content: format!("old {n}"),
        }
    }

    fn diff_of(hunks: Vec<Vec<DiffLine>>) -> FileDiff {
        FileDiff {
            path: PathBuf::from("a.txt"),
            hunks: hunks
                .into_iter()
                .map(|lines| Hunk {
                    header: "@@".to_string(),
                    old_start: 1,
                    old_lines: lines.len(),
                    new_start: 1,
                    new_lines: lines.len(),
                    lines,
                })
                .collect(),
            additions: 0,
            deletions: 0,
            is_binary: false,
            is_submodule: false,
        }
    }

    #[test]
    fn a_line_inside_a_long_context_run_gives_that_bands_key() {
        // add(1), six context lines 2..=7, add(8): the run starts at index 1.
        let mut lines = vec![add_line(1)];
        lines.extend((2..=7).map(ctx_line));
        lines.push(add_line(8));
        let diff = diff_of(vec![lines]);
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 4), Some(1));
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 2), Some(1), "the run's first line");
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 7), Some(1), "and its last");
    }

    #[test]
    fn a_run_shorter_than_a_band_is_no_band_and_four_lines_is() {
        let three = diff_of(vec![[vec![add_line(1)], (2..=4).map(ctx_line).collect::<Vec<_>>(), vec![add_line(5)]].concat()]);
        assert_eq!(band_key_containing(&three, AnnotationSide::New, 3), None);
        let four = diff_of(vec![[vec![add_line(1)], (2..=5).map(ctx_line).collect::<Vec<_>>(), vec![add_line(6)]].concat()]);
        assert_eq!(band_key_containing(&four, AnnotationSide::New, 3), Some(1));
    }

    #[test]
    fn changed_deleted_and_out_of_range_lines_are_in_no_band() {
        let mut lines = vec![add_line(1), del_line(2)];
        lines.extend((2..=8).map(ctx_line));
        let diff = diff_of(vec![lines]);
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 1), None, "an added line is not context");
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 999), None, "outside every hunk");
        assert_eq!(band_key_containing(&diff_of(vec![]), AnnotationSide::New, 1), None, "no hunks at all");
    }

    #[test]
    fn a_bands_key_counts_the_lines_of_the_hunks_before_it() {
        let first: Vec<DiffLine> = (1..=5).map(add_line).collect();
        let mut second: Vec<DiffLine> = (20..=24).map(ctx_line).collect();
        second.push(add_line(25));
        let diff = diff_of(vec![first, second]);
        assert_eq!(band_key_containing(&diff, AnnotationSide::New, 22), Some(5));
    }

    /// `main` → `feat`: `a.txt` (200 lines) is edited at two lines close enough
    /// that git merges them into one hunk with an unchanged run — a context
    /// band — between; `c.txt` is added; `b.txt` is renamed to `d.txt`.
    fn seed_range(dir: &Path) -> (String, String) {
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        let lines = |edits: &[usize]| -> String {
            (1..=200)
                .map(|n| {
                    if edits.contains(&n) {
                        format!("edited {n}\n")
                    } else {
                        format!("line {n}\n")
                    }
                })
                .collect()
        };
        std::fs::write(dir.join("a.txt"), lines(&[])).expect("write a.txt");
        std::fs::write(dir.join("b.txt"), "b\n").expect("write b.txt");
        git(dir, &["add", "-A"]);
        git(dir, &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"]);
        let base = rev_parse(dir, "HEAD");
        git(dir, &["checkout", "-q", "-b", "feat"]);
        let gap = 2 * CHANGES_CONTEXT_LINES - 2;
        std::fs::write(dir.join("a.txt"), lines(&[100, 100 + gap + 1])).expect("edit a.txt");
        std::fs::write(dir.join("c.txt"), "new\n").expect("write c.txt");
        git(dir, &["mv", "b.txt", "d.txt"]);
        git(dir, &["add", "-A"]);
        git(dir, &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "change"]);
        (base, rev_parse(dir, "HEAD"))
    }

    #[gpui::test]
    async fn a_range_surface_lists_its_files_and_reads_no_diff_until_one_opens(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let dir = TempDir::new();
        let (base, head) = seed_range(&dir.0);
        let tab = cx.new(|cx| ChangesTab::for_range(dir.0.clone(), base.clone(), head.clone(), cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.report().sections.iter().any(|section| !section.files.is_empty())
            })
        });
        tab.read_with(cx, |tab, _| {
            let mut files: Vec<(String, usize, usize)> = tab
                .report()
                .sections
                .iter()
                .flat_map(|section| section.files.iter())
                .map(|file| (file.path.to_string_lossy().into_owned(), file.additions, file.deletions))
                .collect();
            files.sort();
            assert_eq!(
                files,
                vec![
                    ("a.txt".to_string(), 2, 2),
                    ("c.txt".to_string(), 1, 0),
                    ("d.txt".to_string(), 0, 0),
                ]
            );
            assert!(!tab.allows_staging(), "a change request's diff is immutable");
            assert_eq!(tab.range(), Some((base.as_str(), head.as_str())));
            assert!(tab.diffs.is_empty(), "no diff is read until a row opens");
        });

        tab.update(cx, |tab, cx| tab.focus_path(Path::new("a.txt"), cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.diffs.contains_key(Path::new("a.txt"))));
        tab.read_with(cx, |tab, _| {
            assert_eq!(tab.diffs.len(), 1, "only the opened file's diff was read");
            assert_eq!(tab.expanded_paths(), vec![PathBuf::from("a.txt")]);
        });
    }

    #[gpui::test]
    async fn focusing_a_line_opens_the_context_band_that_hides_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let dir = TempDir::new();
        let (base, head) = seed_range(&dir.0);
        let tab = cx.new(|cx| ChangesTab::for_range(dir.0.clone(), base, head, cx));
        // Inside the unchanged run between the two edits.
        let line = 101 + CHANGES_CONTEXT_LINES;
        tab.update(cx, |tab, cx| tab.focus_line(Path::new("a.txt"), line, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                let path = Path::new("a.txt");
                tab.diffs
                    .get(path)
                    .and_then(|diff| band_key_containing(diff, AnnotationSide::New, line))
                    .is_some_and(|key| {
                        ChangeSection::ORDER
                            .iter()
                            .any(|section| tab.is_band_expanded(*section, path, key))
                    })
            })
        });
    }

    #[gpui::test]
    async fn a_deleted_file_is_known_to_be_deleted(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let dir = TempDir::new();
        let (base, _) = seed_range(&dir.0);
        git(&dir.0, &["rm", "-q", "a.txt"]);
        git(&dir.0, &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "drop a"]);
        let head = rev_parse(&dir.0, "HEAD");
        let tab = cx.new(|cx| ChangesTab::for_range(dir.0.clone(), base, head, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.report().sections.iter().any(|section| !section.files.is_empty())
            })
        });
        tab.read_with(cx, |tab, _| {
            assert!(tab.is_deleted(Path::new("a.txt")));
            assert!(!tab.is_deleted(Path::new("c.txt")), "an added file is not a deleted one");
        });
    }
}

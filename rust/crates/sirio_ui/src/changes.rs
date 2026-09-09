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

use bezel::{
    theme::ink,
    ui::{icons as bezel_icons, tooltip::Tooltip},
};
use gpui::{
    AnyElement, App, AppContext, Context, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement, KeyDownEvent, ListAlignment, ListSizingBehavior, ListState, PromptLevel,
    Render, Rgba, Task, Window, div, list, prelude::*, px,
};
use sirio_git::{
    DiffLine, DiffOrigin, DiffSideBySideLine, DiffSideBySideRow, DiffStat, FileDiff,
    GitDiffSideBySide, GitError, StatusEntry, StatusKind, StatusSnapshot, commit_diff_entry,
    commit_files, diff_entry, discard, discard_all, stage, stage_all, stats, status, unstage,
};
use sirio_theme::Theme;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::controls;
use crate::loading;
use crate::sidebar::icons::{Icon, IconElement, IconSize};

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
/// File headers: gallery 12px mono text at the app's 30px row rhythm.
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
const DIFF_WASH_ALPHA: f32 = 0.10;
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
    /// Display order; index into this is the segmented control's index.
    const ORDER: [DiffViewMode; 2] = [DiffViewMode::Unified, DiffViewMode::Split];

    fn index(self) -> usize {
        match self {
            DiffViewMode::Unified => 0,
            DiffViewMode::Split => 1,
        }
    }

    fn from_index(index: usize) -> Self {
        Self::ORDER.get(index).copied().unwrap_or_default()
    }

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

/// The live state currently held by one mounted Changes surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesReport {
    pub repo_root: PathBuf,
    pub sections: Vec<ChangesSectionReport>,
    pub loading: bool,
    pub error: Option<String>,
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
    },
    /// A file whose diff could not be fetched. Rendered as an explicit
    /// "diff unavailable" row when expanded, so an empty expansion can
    /// never be mistaken for "no changes".
    Unavailable {
        section: ChangeSection,
        path: PathBuf,
        message: String,
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
}

type GitOperation = Box<dyn FnOnce(&Path) -> Result<(), GitError> + Send + 'static>;

/// The full-width git changes surface.
pub struct ChangesTab {
    repo_root: PathBuf,
    /// What this surface reads and whether it may mutate it.
    source: ChangesSource,
    entries: Vec<StatusEntry>,
    diffs: HashMap<PathBuf, FileDiff>,
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
    /// F-CHG-13: a `focus_path` request that arrived before `entries` had
    /// been populated by the first `refresh()` (the common case — `new()`
    /// starts that refresh asynchronously, so a caller that opens the tab
    /// and asks it to focus a path in the same tick always races it).
    /// Replayed once the next snapshot lands, then cleared either way.
    pending_focus: Option<PathBuf>,
    /// #325: the keyboard-selected file row, keyed like `expanded_changes`
    /// because one path can appear in two sections and Enter has to act on
    /// the one the user is actually on.
    selected_change: Option<(ChangeSection, PathBuf)>,
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
}

impl ChangesTab {
    /// Creates the tab for one checkout and starts its first refresh. The
    /// poll loop then re-checks on the interval after the first render.
    pub fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::with_source(repo_root, ChangesSource::WorkingTree, cx)
    }

    /// Creates a read-only tab showing one commit's files and diffs. The
    /// surface is immutable: every stage/unstage/discard entry point
    /// refuses to run and the mutation buttons are not drawn.
    pub fn for_commit(repo_root: PathBuf, sha: String, cx: &mut Context<Self>) -> Self {
        Self::with_source(repo_root, ChangesSource::Commit(sha), cx)
    }

    /// Creates the surface the right panel's Diff view embeds. Identical to
    /// `new` except that it draws `Open diff`, which reveals the Changes tab
    /// — something only a host that is not that tab can ask for (#217).
    pub fn in_right_panel(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut tab = Self::with_source(repo_root, ChangesSource::WorkingTree, cx);
        tab.embedded_in_panel = true;
        tab
    }

    fn with_source(repo_root: PathBuf, source: ChangesSource, cx: &mut Context<Self>) -> Self {
        let mut tab = Self {
            repo_root,
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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
        };
        // Menu and socket openings both construct this same surface, so the
        // first report is always produced by the surface's own refresh path.
        tab.refresh(cx);
        tab
    }

    /// Whether this surface may mutate the repository. The working tree
    /// can; a commit view is immutable by definition.
    pub fn allows_staging(&self) -> bool {
        matches!(self.source, ChangesSource::WorkingTree)
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
                    name: section.label(),
                    count: files.len(),
                    files,
                }
            })
            .collect();
        ChangesReport {
            repo_root: self.repo_root.clone(),
            sections,
            loading: self.git_task.is_some(),
            error: self.git_error.clone(),
        }
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
        // F-CHG-13: replay a focus_path request that raced this snapshot.
        // Applied at most once — if the path still isn't present (e.g. it
        // was reverted before the snapshot came back), there is nothing
        // further to wait for.
        if let Some(path) = self.pending_focus.take() {
            self.apply_focus(&path, cx);
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
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

    fn confirm_discard(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        let display_path = sirio_project::display_path(&path);
        let detail = format!(
            "This will throw away the worktree changes to {display_path}. This cannot be undone."
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard changes?",
            Some(&detail),
            &["Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.unwrap_or(1) != 0 {
                return;
            }
            let _ = this.update(cx, |tab, cx| {
                tab.start_operation(move |repo| discard(repo, &path), cx)
            });
        })
        .detach();
    }

    fn confirm_discard_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.allows_staging() {
            return;
        }
        let paths = self
            .entries
            .iter()
            .filter(|entry| entry.has_worktree_changes())
            .map(|entry| entry.path.display().to_string())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }
        let detail = format!(
            "This will throw away the worktree changes to:\n{}\n\nThis cannot be undone.",
            paths.join("\n")
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard all changes?",
            Some(&detail),
            &["Discard All", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.unwrap_or(1) != 0 {
                return;
            }
            let _ = this.update(cx, |tab, cx| tab.start_operation(discard_all, cx));
        })
        .detach();
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
        if self.expanded_changes.contains(&key) {
            self.expanded_changes.remove(&key);
        } else {
            self.expanded_changes.insert(key);
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
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    diff_entry(&repo_root, &entry, CHANGES_CONTEXT_LINES)
                })
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
                cx.notify();
            });
        })
        .detach();
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
        // If entries hasn't been populated yet (the caller raced the async
        // refresh new() kicked off), there is nothing to match against yet:
        // remember the request and replay it once a snapshot lands in
        // apply_snapshot, rather than silently no-op'ing.
        if self.entries.is_empty() && self.git_task.is_some() {
            self.pending_focus = Some(path.to_path_buf());
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
                    for (key, _) in context_band_keys(diff) {
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
        self.expanded_changes.clear();
        self.expanded_bands.clear();
        cx.notify();
    }

    fn toggle_section(&mut self, section: ChangeSection, cx: &mut Context<Self>) {
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
                });
            }
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
            });
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
        Rc::new(rows)
    }

    fn render_change_row(
        row: ChangeRow,
        allows_staging: bool,
        draws_open_diff: bool,
        selected: Option<&(ChangeSection, PathBuf)>,
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
                Self::render_change_file(
                    section,
                    entry,
                    stat,
                    drag_payload,
                    expanded,
                    allows_staging,
                    draws_open_diff,
                    is_selected,
                    entity,
                    theme,
                )
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
                .bg(ink(0.02))
                .font_family(theme.typography.code_family)
                .text_size(theme.typography.scaled(12.0))
                .line_height(px(18.0))
                .child(
                    div()
                        .w(px(DIFF_HUNK_GUTTER_WIDTH))
                        .flex_none()
                        .text_color(theme.text_faint)
                        .child("⋯"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .debug_selector(|| "changes-hunk-header".into())
                        .truncate()
                        .text_color(theme.text_faint)
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
            } => Self::render_diff_line(section, path, line, theme).into_any_element(),
            ChangeRow::SplitLine {
                section,
                path,
                key,
                row,
            } => Self::render_split_line(section, path, key, row, theme).into_any_element(),
            ChangeRow::Unavailable {
                section,
                path,
                message,
            } => div()
                .id(format!(
                    "diff-unavailable-{}-{}",
                    section.slug(),
                    path.display()
                ))
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
                .text_color(theme.text_faint)
                .child(div().w(px(DIFF_HUNK_GUTTER_WIDTH)).flex_none().child("⋯"))
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(message),
                )
                .into_any_element(),
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
            .font_family(theme.typography.code_family)
            .text_size(theme.typography.scaled(12.0))
            .text_color(theme.text_faint)
            .bg(ink(0.02))
            .hover(|style| style.bg(theme.element_hover))
            .on_click(move |_, _, cx| {
                band_entity.update(cx, |tab, cx| {
                    tab.toggle_band(section, band_path.clone(), key, cx);
                });
            })
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.border))
            .child(div().text_color(theme.text_faint).child(label))
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.border))
            .child(
                div()
                    .text_color(theme.text_faint)
                    .child(if expanded { "⌃" } else { "⌄" }),
            )
    }

    /// The collapsible header of one section, stating its size the way the
    /// collapsed-context bands do ("N hidden lines") rather than leaving the
    /// reader to count. Clicking collapses or re-expands the section,
    /// remembering the choice across refreshes.
    fn render_section_header(
        section: ChangeSection,
        count: usize,
        collapsed: bool,
        allows_staging: bool,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let entity_for_toggle = entity.clone();
        let entity_for_action = entity.clone();
        let action_label = section.batch_action_label();
        let action_icon = match section {
            ChangeSection::Staged => Icon::SquareMinus,
            ChangeSection::Changed | ChangeSection::Untracked => Icon::SquarePlus,
        };
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
            .font_family(theme.typography.code_family)
            .text_size(theme.typography.scaled(12.0))
            .bg(ink(0.02))
            .hover(|style| style.bg(theme.element_hover))
            .on_click(move |_, _, cx| {
                entity_for_toggle.update(cx, |tab, cx| tab.toggle_section(section, cx));
            })
            .child(
                div()
                    .w(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if collapsed {
                        IconElement::new(Icon::ChevronRight, IconSize::XSmall)
                            .text_color(theme.text_muted)
                    } else {
                        IconElement::new(Icon::ChevronDown, IconSize::XSmall)
                            .text_color(theme.text_muted)
                    }),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(section.label()),
            )
            .child(
                div()
                    .text_color(theme.text_faint)
                    .child(format!("({count})")),
            )
            .child(div().flex_1())
            // A commit view renders no stage/unstage batch action either:
            // the header keeps its collapse toggle but not the mutation.
            .when(allows_staging, |this| {
                this.child(section_action_button(
                    action_icon,
                    action_label,
                    action_id,
                    theme,
                    move |cx| {
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
        // Unknown counts render `·` (the same glyph as binary), never a
        // confident +0 −0.
        let (additions, deletions) = match stat {
            Some(stat) if stat.is_binary => ("·".to_owned(), "·".to_owned()),
            Some(stat) => (
                format!("+{}", stat.additions),
                format!("−{}", stat.deletions),
            ),
            None => ("·".to_owned(), "·".to_owned()),
        };
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
            .border_color(theme.border)
            .font_family(theme.typography.code_family)
            .text_size(theme.typography.scaled(12.0))
            // The path is neutral text — the +/− counts carry the status.
            .text_color(theme.text)
            .hover(|style| style.bg(theme.element_hover))
            // #325: the keyboard selection has to be visible, or up/down
            // move something the user cannot see.
            .when(is_selected, |this| this.bg(theme.element_hover))
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
                div()
                    .w(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if expanded {
                        IconElement::new(Icon::ChevronDown, IconSize::XSmall)
                            .text_color(theme.text_muted)
                    } else {
                        IconElement::new(Icon::ChevronRight, IconSize::XSmall)
                            .text_color(theme.text_muted)
                    }),
            )
            .child(
                div()
                    .w(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        bezel_icons::icon(bezel_icons::DOCUMENT)
                            .size(px(14.0))
                            .text_color(color),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path.to_string_lossy().to_string()),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.5))
                    .text_color(theme.diff_add)
                    .child(additions),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.5))
                    .text_color(theme.diff_del)
                    .child(deletions),
            )
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
                            this.child(destructive_action_text_button(
                                "Discard",
                                format!("discard-{}-{}", section.slug(), path.display()),
                                theme,
                                move |window, cx| {
                                    entity_for_discard.update(cx, |tab, cx| {
                                        tab.confirm_discard(path_for_discard.clone(), window, cx);
                                    });
                                },
                            ))
                            .child(action_text_button(
                                stage_label,
                                format!("stage-{}-{}", section.slug(), path.display()),
                                theme,
                                move |cx| {
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
                            this.child(action_text_button(
                                "Open diff",
                                format!("changes-open-diff-{}-{}", section.slug(), path.display()),
                                theme,
                                move |cx| {
                                    entity_for_open_diff.update(cx, |_, cx| {
                                        cx.emit(ChangesTabActionEvent::OpenDiff(
                                            open_diff_path.clone(),
                                        ));
                                    });
                                },
                            ))
                        })
                        .when(entry.is_conflicted(), |this| {
                            this.child(action_text_button(
                                "Resolve in terminal",
                                format!("resolve-{}-{}", section.slug(), path.display()),
                                theme,
                                move |cx| {
                                    entity_for_resolve.update(cx, |_, cx| {
                                        cx.emit(ChangesTabActionEvent::ResolveInTerminal(
                                            conflict_path.clone(),
                                        ));
                                    });
                                },
                            ))
                        })
                        .child(
                            div()
                                .id(format!("open-{}-{}", section.slug(), path.display()))
                                .debug_selector(|| "changes-open-file".into())
                                .text_color(theme.text_muted)
                                .hover(|style| style.text_color(theme.text))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    // `entry.path` is repo-relative, and the
                                    // host opens an editor tab straight from
                                    // whatever this event carries — a
                                    // relative path made `FileView` resolve
                                    // against the process CWD and render
                                    // "This file does not exist: <name>" for
                                    // a file that plainly does. Resolve
                                    // against this surface's own root, the
                                    // way the Files tree already emits
                                    // absolute paths.
                                    entity_for_open.update(cx, |tab, cx| {
                                        let absolute = tab.repo_root.join(&path);
                                        cx.emit(ChangesTabEvent::OpenFile(absolute));
                                    });
                                })
                                .child("↗"),
                        ),
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
            .child(split_cell(row.left, true, theme).debug_selector(|| "changes-split-left".into()))
            .child(
                div()
                    .w(px(SPLIT_DIVIDER_WIDTH))
                    .flex_none()
                    .bg(theme.border),
            )
            .child(
                split_cell(row.right, false, theme).debug_selector(|| "changes-split-right".into()),
            )
    }

    fn render_diff_line(
        section: ChangeSection,
        path: PathBuf,
        line: DiffLine,
        theme: Theme,
    ) -> impl IntoElement {
        let (background, marker_color, marker, text_color) = match line.origin {
            DiffOrigin::Context => (theme.surface, theme.text_faint, " ", theme.text_muted),
            DiffOrigin::Addition => (diff_wash(theme.diff_add), theme.diff_add, "+", theme.text),
            DiffOrigin::Deletion => (diff_wash(theme.diff_del), theme.diff_del, "-", theme.text),
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
                    .text_color(theme.text_faint)
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
                    .text_color(theme.text_faint)
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
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(text_color)
                    .child(line.content),
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
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(theme.typography.scaled(12.5))
                    .text_color(theme.text)
                    .child(title),
            )
            // The view-mode control sits at the head of the action cluster,
            // with the other two *view* controls (Expand All / Collapse
            // All) beside it and the git *mutations* (Stage all, Discard
            // all) after them. It is the same segmented primitive Settings
            // uses for System/Light/Dark, and it belongs in this header for
            // the same reason Expand All does: it changes how the whole
            // surface reads, not one file.
            //
            // The precedent taken from macOS is the *placement and
            // primitive*, not the semantics: `SideBySideDiffView.scopeBar`
            // (`DiffContentAdapter.swift:179`) is a segmented `Picker` in
            // exactly this position at the top of the diff surface — but it
            // picks the diff's *scope* (whole file vs hunks), because that
            // build had only one renderer and so never needed a view-mode
            // choice at all. Do not read this control as a port of that one.
            .child(controls::segmented_icons(
                "changes-view-mode",
                &[(Icon::DiffUnified, "Unified"), (Icon::DiffSplit, "Split")],
                mode.index(),
                theme,
                move |index, cx| {
                    mode_entity.update(cx, |tab, cx| {
                        tab.set_view_mode(DiffViewMode::from_index(index), cx);
                    });
                },
            ))
            // Refresh is a button and nothing else: it never turns into a
            // spinner while a snapshot loads. The toolbar used to swap it
            // for `loading::compact` for as long as `git_task` was in
            // flight, and `ensure_refresh` puts a task in flight every
            // second, so the icon blinked once a second for the duration
            // of every `git status`. A refresh over a settled surface is
            // silent — the same rule `render_body` applies to the list —
            // and a click during one is a no-op by `refresh`'s own
            // single-flight guard.
            .child(action_icon_button(
                Icon::RefreshCw,
                "Refresh",
                "changes-refresh",
                "refresh-changes".to_owned(),
                theme,
                move |cx| {
                    refresh_entity.update(cx, |tab, cx| tab.refresh(cx));
                },
            ))
            .child(action_icon_button(
                Icon::ExpandVertical,
                "Expand All",
                "changes-expand-all",
                "expand-all".to_owned(),
                theme,
                move |cx| {
                    expand_entity.update(cx, |tab, cx| tab.expand_all(cx));
                },
            ))
            .child(action_icon_button(
                Icon::FoldVertical,
                "Collapse All",
                "changes-collapse-all",
                "collapse-all".to_owned(),
                theme,
                move |cx| {
                    collapse_entity.update(cx, |tab, cx| tab.collapse_all(cx));
                },
            ))
            // The git mutations only exist for a mutable checkout: a commit
            // view renders no Stage/Discard controls at all.
            .when(self.allows_staging(), |this| {
                this.child(action_icon_button(
                    Icon::SquarePlus,
                    "Stage all",
                    "changes-stage-all",
                    "stage-all".to_owned(),
                    theme,
                    move |cx| {
                        stage_entity.update(cx, |tab, cx| {
                            tab.start_operation(stage_all, cx);
                        });
                    },
                ))
                .child(destructive_action_icon_button(
                    Icon::Undo,
                    "Discard all",
                    "changes-discard-all",
                    "discard-all".to_owned(),
                    theme,
                    move |window, cx| {
                        discard_entity.update(cx, |tab, cx| {
                            tab.confirm_discard_all(window, cx);
                        });
                    },
                ))
            })
    }
}

impl EventEmitter<ChangesTabEvent> for ChangesTab {}
impl EventEmitter<ChangesTabActionEvent> for ChangesTab {}

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
            for line in lines {
                rows.push(ChangeRow::Line {
                    section,
                    path: path.to_path_buf(),
                    line: line.clone(),
                });
            }
        }
        DiffViewMode::Split => {
            for row in GitDiffSideBySide::rows_from_lines(lines) {
                rows.push(ChangeRow::SplitLine {
                    section,
                    path: path.to_path_buf(),
                    key: *split_key,
                    row,
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
fn split_cell(line: Option<DiffSideBySideLine>, old: bool, theme: Theme) -> gpui::Div {
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
        return cell.bg(theme.input_bg);
    };
    let (background, marker_color, marker) = match line.origin {
        DiffOrigin::Context => (theme.surface, theme.text_faint, " "),
        DiffOrigin::Addition => (diff_wash(theme.diff_add), theme.diff_add, "+"),
        DiffOrigin::Deletion => (diff_wash(theme.diff_del), theme.diff_del, "-"),
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
                    .text_color(theme.text_faint)
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
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(theme.text)
                    .child(line.content),
            ),
    )
}

/// The collapsed-context runs of one diff, as `(key, count)` pairs — the
/// same walk `expand_diff` performs when rendering, so Expand All and the
/// render can never disagree about which bands exist. `key` is the run's
/// first line's position in the file's flattened line stream.
fn context_band_keys(diff: &FileDiff) -> Vec<(usize, usize)> {
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
                let count = i - start;
                if count >= CONTEXT_BAND_MIN {
                    keys.push((line_index + start, count));
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
                .items_center()
                .justify_center()
                .gap(theme.spacing.card_gap)
                .text_size(theme.typography.headline)
                .text_color(theme.text_muted)
                .child(loading::indeterminate(
                    "changes-loading-orb",
                    loading::GENERIC_ORB,
                    &theme,
                    window,
                    cx,
                ))
                .child("Loading changes…")
                .child(loading::skeleton_rows(
                    "changes-skeleton",
                    loading::SKELETON_ROWS,
                    &theme,
                    window,
                    cx,
                ))
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
                .text_color(theme.text_muted)
                .child("No changes")
                .into_any_element();
        }
        let rows = self.sync_list_rows(sections);
        let row_entity = entity;
        let allows_staging = self.allows_staging();
        let draws_open_diff = self.embedded_in_panel;
        let selected = self.selected_change.clone();
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
                            allows_staging,
                            draws_open_diff,
                            selected.as_ref(),
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
            .into_any_element()
    }

    /// The F-CHG-09 error state: git's own message (enough detail to act
    /// on) and a Retry that re-runs the refresh. Pixel-bound like every
    /// other state here, but reachable — the refresh path sets `git_error`
    /// from a real failed git invocation.
    fn render_error_state(
        error: &str,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let retry_entity = entity;
        div()
            .id("changes-error")
            .debug_selector(|| "changes-error".into())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(24.0))
            .child(
                div()
                    .max_w(px(560.0))
                    .text_size(theme.typography.headline)
                    .text_color(theme.danger)
                    .child(format!("Git is unavailable: {error}")),
            )
            .child(action_text_button(
                "Retry",
                "changes-retry".to_owned(),
                theme,
                move |cx| retry_entity.update(cx, |tab, cx| tab.retry_refresh(cx)),
            ))
    }
}

impl Render for ChangesTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
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
        let body = self.render_body(entity, theme, mode, _window, cx);
        div()
            // The surface's own extent, so a drawn test can assert that
            // nothing inside it — notably Split mode's width reservation —
            // makes it wider than the space it was given.
            .debug_selector(|| "changes-surface".into())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .child(toolbar)
            .child(body)
    }
}

/// The status hue for a change row's glyph — the path text stays neutral.
///
/// F-CHG-06: this used to be its own precedence chain, testing
/// `has_worktree_changes()` before `is_staged()` — backwards from Swift's
/// `GitStatusStyle.color` and from the Files tree, so a staged-then-modified
/// file rendered amber here and green there. Both views now resolve through
/// [`crate::git_status_style`]; keep it that way.
fn status_color(entry: &StatusEntry, theme: Theme) -> Rgba {
    crate::git_status_style::entry_color(entry, theme)
}

/// The gallery derives row washes from the semantic ink rather than keeping
/// a second palette entry for the same meaning.
fn diff_wash(color: Rgba) -> Rgba {
    Rgba {
        a: DIFF_WASH_ALPHA,
        ..color
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
            .bg(self.theme.surface_raised)
            .text_color(self.theme.text)
            .child("Diff")
    }
}

fn action_text_button(
    label: &'static str,
    id: String,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.clone())
        // The id drives click dispatch; the selector is what tests can
        // find in the drawn frame's debug-bounds map.
        .debug_selector(move || match label {
            "Stage" => "changes-stage".to_owned(),
            "Unstage" => "changes-unstage".to_owned(),
            "Retry" => "changes-retry".to_owned(),
            "Stage all" => "changes-stage-all".to_owned(),
            "Expand All" => "changes-expand-all".to_owned(),
            "Collapse All" => "changes-collapse-all".to_owned(),
            "Open diff" => "changes-open-diff".to_owned(),
            "Resolve in terminal" => "changes-resolve".to_owned(),
            _ => format!("changes-action-{label}"),
        })
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .rounded(theme.radii.control)
        .text_size(theme.typography.caption2)
        .text_color(theme.text)
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
}

fn action_icon_button(
    icon: Icon,
    tooltip: &'static str,
    selector: &'static str,
    id: String,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || selector.to_owned())
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .rounded(theme.radii.control)
        .text_size(theme.typography.caption2)
        .text_color(theme.text)
        .hover(|style| style.bg(theme.element_hover))
        .tooltip(move |window, cx| Tooltip::text(tooltip, window, cx))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(IconElement::new(icon, IconSize::Small))
}

fn section_action_button(
    icon: Icon,
    label: &'static str,
    id: String,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.clone())
        .debug_selector(move || id.clone())
        // The token layer has no compact-section-action padding yet; use its
        // titlebar spacing as the nearest COSMIC control rhythm.
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .rounded(theme.radii.control)
        .text_size(theme.typography.caption2)
        .text_color(theme.text_muted)
        .hover(|style| style.bg(theme.element_hover).text_color(theme.text))
        .tooltip(move |window, cx| Tooltip::text(label, window, cx))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(IconElement::new(icon, IconSize::XSmall))
}

fn destructive_action_icon_button<F>(
    icon: Icon,
    tooltip: &'static str,
    selector: &'static str,
    id: String,
    theme: Theme,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .debug_selector(move || selector.to_owned())
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .text_size(theme.typography.scaled(12.5))
        .text_color(theme.text_muted)
        .hover(|style| style.text_color(theme.danger))
        .tooltip(move |window, cx| Tooltip::text(tooltip, window, cx))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(IconElement::new(icon, IconSize::Small))
}

fn destructive_action_text_button<F>(
    label: &'static str,
    id: String,
    theme: Theme,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .debug_selector(move || match label {
            "Discard" => "changes-discard".to_owned(),
            "Discard all" => "changes-discard-all".to_owned(),
            _ => format!("changes-destructive-{label}"),
        })
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .text_size(theme.typography.scaled(12.5))
        .text_color(theme.text_muted)
        .hover(|style| style.text_color(theme.danger))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(label)
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
        if let Some(expanded_paths) = expanded_paths {
            if !expanded_paths.contains(&entry.path) {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use sirio_git::StatusKind;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

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

    fn settled_changes_tab(repo_root: PathBuf) -> ChangesTab {
        let entries = status(&repo_root)
            .expect("status for settled Changes tab")
            .entries;
        ChangesTab {
            repo_root,
            source: ChangesSource::WorkingTree,
            entries,
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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
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
            theme.success,
            "a staged-then-modified file reads as staged, matching Swift's \
             GitStatusStyle.color and the Files tree marker"
        );
        assert_ne!(
            status_color(&entry, theme),
            theme.warning,
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

    /// An empty bucket gets no section at all — the panel never renders a
    /// header that says zero.
    #[gpui::test]
    async fn an_empty_section_is_omitted(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("staged.txt"), "s\n").expect("seed staged");
        git(&dir.0, &["add", "staged.txt"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });

        let sections = tab.read_with(cx, |tab, _| tab.section_rows(DiffViewMode::Unified));
        assert_eq!(sections.len(), 2, "only the non-empty sections render");
        assert!(
            sections
                .iter()
                .all(|section| section.section != ChangeSection::Untracked),
            "an empty Untracked bucket must not render a section"
        );
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

    /// A refresh after a snapshot has landed keeps the last known rows on
    /// screen. The compact refresh indicator belongs inside that stale list;
    /// the full-surface first-load state must not flash over it.
    #[gpui::test]
    async fn a_refresh_keeps_the_settled_list_visible(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| {
            tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
        });
        cx.cx.run_until_parked();

        tab.update(&mut cx.cx, |tab, cx| tab.refresh(cx));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("changes-list").is_some(),
            "a refresh preserves the stale changes list"
        );
        assert!(
            cx.debug_bounds("changes-loading").is_none(),
            "a refresh with stale entries does not replace the list with a full-surface loader"
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

    /// The per-row action acts on the side of the split the row sits on:
    /// Unstage for a Staged row, Stage for Changed and Untracked rows.
    #[test]
    fn row_actions_follow_the_section_not_the_entry() {
        assert_eq!(ChangeSection::Staged.action_label(), "Unstage");
        assert_eq!(ChangeSection::Changed.action_label(), "Stage");
        assert_eq!(ChangeSection::Untracked.action_label(), "Stage");
    }

    // ── Honesty: an error must look like an error, not like empty ──────

    /// F-CHG-09: a repository whose `.git` is removed must surface git's
    /// own message — never a clean 0/0/0 with `error=''` — and Retry must
    /// be reachable and recover.
    #[gpui::test]
    async fn a_broken_repo_surfaces_the_error_and_retry_recovers(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });
        assert!(tab.read_with(cx, |tab, _| tab.git_error.is_none()));

        // Break git from outside: the repository metadata disappears.
        std::fs::remove_dir_all(dir.0.join(".git")).expect("remove .git");
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.git_error.is_some()));
        let error = tab
            .read_with(cx, |tab, _| tab.git_error.clone())
            .expect("the error reached the surface");
        // git's own fatal message is shown, not an invented empty state.
        // Only the prefix is locale-independent (git's stderr can be any
        // language); that it is present proves the detail arrived.
        assert!(
            error.starts_with("git exited with status 128")
                && error.len() > "git exited with status 128".len(),
            "git's own fatal message is shown, not an invented empty state: {error}"
        );

        // Restore the repository and retry: the error clears and the
        // change returns.
        git(&dir.0, &["init", "-q"]);
        git(&dir.0, &["config", "user.email", "t@example.invalid"]);
        git(&dir.0, &["config", "user.name", "Sirio tests"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("rewrite");
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.git_error.is_none()
                    && tab.entries.iter().any(|entry| entry.path == *"tracked.txt")
            })
        });
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

    /// A failed mutation must reach the same visible error state as a failed
    /// refresh, survive a successful refresh, and clear after a successful
    /// retry of the mutation.
    #[gpui::test]
    async fn a_failed_stage_survives_refresh_and_recovers_after_retry(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked");

        let tab = cx.new(|_| settled_changes_tab(dir.0.clone()));
        assert_eq!(
            tab.read_with(cx, |tab, _| section_count(tab, "Changed")),
            1,
            "the settled test tab contains the modified file"
        );

        std::fs::write(dir.0.join(".git/index.lock"), b"").expect("create index lock");
        tab.update(cx, |tab, cx| {
            tab.refresh(cx);
            assert!(tab.git_task.is_some(), "the refresh must be in flight");
            tab.stage_path(PathBuf::from("tracked.txt"), cx);
        });
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.git_error.is_some()));

        let error = tab
            .read_with(cx, |tab, _| tab.git_error.clone())
            .expect("the failed stage reaches the Changes surface");
        assert!(
            error.contains("index.lock") || error.contains("did not finish"),
            "the visible error keeps git's actionable failure detail: {error}"
        );
        std::fs::remove_file(dir.0.join(".git/index.lock")).expect("remove index lock");
        std::fs::write(dir.0.join("refresh-marker.txt"), "refreshed\n")
            .expect("create refresh marker");
        tab.update(cx, |tab, cx| {
            tab.refresh(cx);
        });
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries
                    .iter()
                    .any(|entry| entry.path == Path::new("refresh-marker.txt"))
            })
        });
        assert!(
            tab.read_with(cx, |tab, _| tab.git_error.is_some()),
            "a successful refresh must not clear a mutation error"
        );

        tab.update(cx, |tab, cx| {
            tab.stage_path(PathBuf::from("tracked.txt"), cx);
        });
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.git_error.is_none() && section_count(tab, "Staged") == 1
            })
        });
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
        assert!(cx.cx.has_pending_prompt(), "Discard asks for confirmation");
        cx.cx.simulate_prompt_answer("Cancel");
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
        assert!(cx.cx.has_pending_prompt());
        cx.cx.simulate_prompt_answer("Discard");
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
        assert!(
            cx.cx.has_pending_prompt(),
            "Discard still asks for confirmation during a refresh"
        );
        cx.cx.simulate_prompt_answer("Discard");

        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 0);
        assert!(
            status(&dir.0)
                .expect("status after deferred discard")
                .entries
                .is_empty(),
            "the confirmed Discard runs after the refresh"
        );
    }

    /// The Changes toolbar keeps all six controls reachable by their stable
    /// selectors after replacing their text chrome with icons. GPUI's visual
    /// test context exposes bounds but not rendered text content, so the
    /// permitted fallback is used here; the action controls' compact bounds
    /// also distinguish them from the replaced labels.
    #[gpui::test]
    async fn drawn_changes_toolbar_uses_icon_controls(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify tracked file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        cx.cx.run_until_parked();

        for selector in [
            "changes-view-mode-0",
            "changes-view-mode-1",
            "changes-expand-all",
            "changes-collapse-all",
            "changes-stage-all",
            "changes-discard-all",
        ] {
            let bounds = cx.debug_bounds(selector).expect("toolbar control is drawn");
            assert!(
                bounds.size.width > px(0.0) && bounds.size.height > px(0.0),
                "{selector} has a non-empty clickable bounds"
            );
        }

        for selector in [
            "changes-expand-all",
            "changes-collapse-all",
            "changes-stage-all",
            "changes-discard-all",
        ] {
            let width = cx
                .debug_bounds(selector)
                .expect("action is drawn")
                .size
                .width;
            assert!(
                f32::from(width) < 40.0,
                "{selector} keeps icon-sized chrome, got {width}"
            );
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
        assert!(
            cx.cx.has_pending_prompt(),
            "Discard all asks for confirmation"
        );
        cx.cx.simulate_prompt_answer("Cancel");
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
        assert!(cx.cx.has_pending_prompt());
        cx.cx.simulate_prompt_answer("Discard All");
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

    /// Expand All / Collapse All (orca's diff-header affordance): both are
    /// drawn toolbar buttons that drive the whole list — every file row
    /// opens its diff (including its collapsed-context bands) and then
    /// closes again. The drawn frame proves the buttons reach the model;
    /// the model assertions prove the choice is view state, not a git
    /// mutation.
    #[gpui::test]
    async fn drawn_expand_all_and_collapse_all_drive_the_whole_list(cx: &mut TestAppContext) {
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

        // Nothing is expanded yet: no row actions, no diff bands.
        assert!(
            cx.debug_bounds("changes-stage").is_none()
                && cx.debug_bounds("changes-context-band").is_none(),
            "a freshly loaded list is fully collapsed"
        );

        let expand_all = cx
            .debug_bounds("changes-expand-all")
            .expect("Expand All is in the drawn toolbar");
        cx.simulate_click(expand_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tab.read_with(&cx.cx, |tab, _| !tab.expanded_changes.is_empty()),
            "Expand All opens every file row"
        );
        assert!(
            tab.read_with(&cx.cx, |tab, _| !tab.expanded_bands.is_empty()),
            "Expand All opens the collapsed-context bands too"
        );
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("changes-stage").is_some(),
            "an expanded file renders its row actions"
        );
        assert!(
            cx.debug_bounds("changes-context-band").is_some(),
            "an expanded diff renders its context bands"
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
            "Collapse All closes every row and band"
        );
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("changes-stage").is_none()
                && cx.debug_bounds("changes-context-band").is_none(),
            "after Collapse All the list is back to its headers only"
        );
        // The collapse is view state: the change is still there underneath.
        assert_eq!(
            tab.read_with(&cx.cx, |tab, _| section_count(tab, "Changed")),
            1,
            "collapsing does not touch the git state"
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
        assert!(
            error.contains("No such file"),
            "the OS error is included so the user can act: {error}"
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

        let expanded: HashSet<PathBuf> =
            [PathBuf::from("f0.txt"), PathBuf::from("f2.txt")].into();
        let snapshot = load_worktree_snapshot(&dir.0, Some(&expanded))
            .expect("snapshot load must succeed");

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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
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

    /// F-CHG-13: and it is drawn *only* where that event can do something.
    /// Inside the Changes tab, `OpenDiff` reveals the Changes tab and
    /// expands a row that — being the row the control is drawn in — is
    /// already expanded, so the control had no effect it could still have
    /// (#217).
    #[gpui::test]
    async fn the_changes_tab_draws_no_open_diff_action_of_its_own(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("modify file");

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);

        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("changes-stage").is_some(),
            "the expanded row still draws the actions that do act on it"
        );
        assert!(
            cx.debug_bounds("changes-open-diff").is_none(),
            "Open diff is not drawn in the surface it cannot leave"
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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
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
        let wide: String = (1..40).map(|i| format!("seg{i:03}-")).collect();
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
            .debug_bounds("changes-view-mode-1")
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
            cx.debug_bounds("changes-view-mode-0").is_some(),
            "the Unified segment is still on screen to switch back with"
        );
    }

    /// The copied bezel diff pattern is a visual contract too: the two
    /// five-digit gutters plus the change marker keep fixed widths so code
    /// never shifts as line numbers grow.
    #[gpui::test]
    async fn drawn_diff_rows_follow_the_gallery_gutter_rhythm(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        side_by_side_fixture(&dir.0);

        let (mut cx, tab) = changes_view(cx, dir.0.clone());
        wait_for_tab(&cx, &tab, |tab| section_count(tab, "Changed") == 1);
        let row = cx
            .debug_bounds("changes-file-row")
            .expect("the changed file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.debug_bounds("changes-hunk-row").is_some());
        assert!(cx.debug_bounds("changes-diff-line").is_some());

        let old = cx
            .debug_bounds("changes-diff-old-number")
            .expect("the old line-number gutter is drawn");
        let new = cx
            .debug_bounds("changes-diff-new-number")
            .expect("the new line-number gutter is drawn");
        let marker = cx
            .debug_bounds("changes-diff-marker")
            .expect("the change marker column is drawn");
        assert!(
            f32::from(old.size.width) >= 36.0,
            "the old gutter must fit five 12px monospace digits"
        );
        assert!(
            f32::from(new.size.width) >= 36.0,
            "the new gutter must fit five 12px monospace digits"
        );
        assert_eq!(f32::from(marker.size.width), 12.0);
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
        let window = cx.open_window(gpui::size(px(330.0), px(600.0)), move |_window, _cx| {
            ChangesTab {
                repo_root: PathBuf::from("/repo"),
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
                pending_focus: None,
                selected_change: None,
                list_focus: None,
                list_state: new_list_state(),
                list_fingerprint: 0,
                reveal_selected: false,
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
            .debug_bounds("changes-view-mode-1")
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
            .debug_bounds("changes-view-mode-1")
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
            .debug_bounds("changes-view-mode-0")
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
            pending_focus: None,
            selected_change: None,
            list_focus: None,
            list_state: new_list_state(),
            list_fingerprint: 0,
            reveal_selected: false,
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

    /// The report: opening a very large diff makes the diff's own scroll and
    /// the whole app lag. The loop's claim is that a frame over an expanded
    /// diff should cost what the *viewport* costs, not what the *file*
    /// costs: a 50x larger diff must not make every frame ~50x slower.
    #[gpui::test]
    async fn perf_a_large_expanded_diff_costs_a_frame_proportional_to_the_viewport(
        cx: &mut TestAppContext,
    ) {
        let small = measure_big_diff(cx, 300);
        let large = measure_big_diff(cx, 5_000);
        for (label, cost) in [("small", &small), ("large", &large)] {
            eprintln!(
                "[PERF-diff] {label}: rows={} section_rows={:.2}ms frame={:.2}ms scroll_dispatch={:.2}ms scroll_frame={:.2}ms",
                cost.rows,
                cost.section_rows_ms,
                cost.frame_ms,
                cost.scroll_dispatch_ms,
                cost.scroll_frame_ms
            );
        }
        let ratio = large.frame_ms / small.frame_ms.max(0.01);
        eprintln!("[PERF-diff] frame ratio large/small = {ratio:.1}x");
        assert!(
            ratio < 4.0,
            "a frame over a {}-row diff costs {:.1}ms, {ratio:.1}x the {:.1}ms of a {}-row one: \
             the whole file is being laid out every frame, not the viewport",
            large.rows,
            large.frame_ms,
            small.frame_ms,
            small.rows
        );
    }
}

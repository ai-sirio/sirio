//! The full-width Changes tab: git status and unified diffs.
//!
//! This surface used to live inside the right inspector, where a 405px
//! column could not show a line of code. It is now a first-class tab (a
//! peer of Chat and Terminal, per orca's placement): the diff gets the
//! width it needs, and the inspector keeps Files.
//!
//! The data model is local to this surface. Git operations are delegated to
//! `tiller_git`; the host application can later replace the refresh
//! callbacks with its project store without changing the row layout.
//!
//! Diffs are fetched with [`CHANGES_CONTEXT_LINES`] of context instead of
//! `tiller_git::DEFAULT_CONTEXT_LINES`: with 3 lines of context a collapsed
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
//!   expansion renders (`tiller_git::stats`), so they cannot disagree;
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
    AnyElement, App, Context, EventEmitter, FontWeight, InteractiveElement, PromptLevel, Render,
    Rgba, Task, Window, div, prelude::*, px,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tiller_git::{
    DiffLine, DiffOrigin, DiffStat, FileDiff, GitError, StatusEntry, StatusSnapshot, diff_entry,
    discard, discard_all, stage, stage_all, stats, status, unstage,
};
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement};

/// Context lines fetched for each change. Generous enough that the
/// collapsed-context bands carry real counts ("27 hidden lines"), cheap
/// enough to re-fetch on the refresh interval.
pub const CHANGES_CONTEXT_LINES: usize = 24;

/// How often the git snapshot re-polls after an external edit.
const CHANGES_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const TOOLBAR_HEIGHT: f32 = 34.0;
/// File rows: 12.5px text at 30px, the app's single-line row rhythm.
pub(crate) const ROW_HEIGHT: f32 = 30.0;
pub(crate) const HUNK_ROW_HEIGHT: f32 = 24.0;
/// Diff code lines: 11.5px mono at 20px.
pub(crate) const DIFF_LINE_HEIGHT: f32 = 20.0;
const BAND_ROW_HEIGHT: f32 = 24.0;
/// A run of unchanged context lines this long collapses into one labelled
/// band (orca's "18 hidden lines").
const CONTEXT_BAND_MIN: usize = 4;

/// Events emitted to the shell.
#[derive(Clone, Debug)]
pub enum ChangesTabEvent {
    /// Open a file from the diff's per-file action row in a new tab.
    OpenFile(PathBuf),
}

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
    /// A file whose diff could not be fetched. Rendered as an explicit
    /// "diff unavailable" row when expanded, so an empty expansion can
    /// never be mistaken for "no changes".
    Unavailable {
        section: ChangeSection,
        path: PathBuf,
        message: String,
    },
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

/// The full-width git changes surface.
pub struct ChangesTab {
    repo_root: PathBuf,
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
    git_error: Option<String>,
    /// Paths whose diff failed to load, keyed like `diffs`. Kept separate so
    /// the expanded row can name the failure instead of showing nothing.
    diff_errors: HashMap<PathBuf, String>,
    /// One polling loop per tab, armed on first render.
    refresh_started: bool,
}

impl ChangesTab {
    /// Creates the tab for one checkout and starts its first refresh. The
    /// poll loop then re-checks on the interval after the first render.
    pub fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut tab = Self {
            repo_root,
            entries: Vec::new(),
            diffs: HashMap::new(),
            stats: HashMap::new(),
            expanded_changes: HashSet::new(),
            collapsed_sections: HashSet::new(),
            expanded_bands: HashSet::new(),
            git_task: None,
            git_error: None,
            diff_errors: HashMap::new(),
            refresh_started: false,
        };
        // Menu and socket openings both construct this same surface, so the
        // first report is always produced by the surface's own refresh path.
        tab.refresh(cx);
        tab
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

    fn apply_snapshot(&mut self, snapshot: GitSnapshot) {
        // A file that leaves the status list forgets its expansion; a file
        // that merely changes section (staged → unstaged) forgets it too —
        // the old (section, path) key no longer exists.
        self.expanded_changes
            .retain(|(_, path)| snapshot.entries.iter().any(|entry| &entry.path == path));
        self.entries = snapshot.entries;
        self.stats = snapshot.stats;
        self.diffs = snapshot.diffs;
        self.diff_errors = snapshot.diff_errors;
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.git_task.is_some() {
            return;
        }
        let repo_root = self.repo_root.clone();
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_snapshot(&repo_root) })
                .await;
            let _ = this.update(cx, |tab, cx| {
                tab.git_task = None;
                match result {
                    Ok(snapshot) => {
                        tab.apply_snapshot(snapshot);
                        tab.git_error = None;
                    }
                    Err(error) => tab.git_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    /// Arms the periodic refresh loop: once immediately, then on the
    /// interval. The timer runs on the background executor; git status never
    /// touches the render thread.
    fn ensure_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_started {
            return;
        }
        self.refresh_started = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(CHANGES_REFRESH_INTERVAL)
                    .await;
                if this.update(cx, |tab, cx| tab.refresh(cx)).is_err() {
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
        if self.git_task.is_some() {
            return;
        }
        let repo_root = self.repo_root.clone();
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let result = operation(&repo_root);
                    let snapshot = result.as_ref().ok().map(|_| load_snapshot(&repo_root));
                    (result, snapshot)
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                tab.git_task = None;
                match outcome {
                    (Ok(()), Some(Ok(snapshot))) => tab.apply_snapshot(snapshot),
                    (Err(error), _) => tab.git_error = Some(error.to_string()),
                    (Ok(()), Some(Err(error))) => {
                        tab.git_error = Some(format!(
                            "the change was applied, but refreshing failed: {error}"
                        ))
                    }
                    (Ok(()), None) => {
                        unreachable!("a successful operation always loads a snapshot")
                    }
                }
                cx.notify();
            });
        }));
    }

    fn stage_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.start_operation(move |repo| stage(repo, &path), cx);
    }

    fn unstage_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.start_operation(move |repo| unstage(repo, &path), cx);
    }

    fn confirm_discard(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.git_task.is_some() {
            return;
        }
        let display_path = path.display().to_string();
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
        if self.git_task.is_some() {
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

    fn toggle_change(&mut self, section: ChangeSection, path: &Path, cx: &mut Context<Self>) {
        let key = (section, path.to_path_buf());
        if self.expanded_changes.contains(&key) {
            self.expanded_changes.remove(&key);
        } else {
            self.expanded_changes.insert(key);
        }
        cx.notify();
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
    fn section_rows(&self) -> Vec<SectionRows> {
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
                        expanded,
                    });
                    if expanded {
                        self.expand_diff(&mut rows, section, entry);
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
    fn expand_diff(&self, rows: &mut Vec<ChangeRow>, section: ChangeSection, entry: &StatusEntry) {
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
        // A run of unchanged context lines collapses into one labelled band
        // (orca's "18 hidden lines"), expanded in place on click. `key` is
        // the run's first line's position in the file's flattened line
        // stream — stable within one snapshot.
        let mut line_index = 0usize;
        for hunk in &diff.hunks {
            rows.push(ChangeRow::Hunk {
                section,
                path: entry.path.clone(),
                header: hunk.header.clone(),
            });
            let mut i = 0usize;
            while i < hunk.lines.len() {
                if hunk.lines[i].origin == DiffOrigin::Context {
                    let start = i;
                    while i < hunk.lines.len() && hunk.lines[i].origin == DiffOrigin::Context {
                        i += 1;
                    }
                    let count = i - start;
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
                            for line in &hunk.lines[start..i] {
                                rows.push(ChangeRow::Line {
                                    section,
                                    path: entry.path.clone(),
                                    line: line.clone(),
                                });
                            }
                        }
                    } else {
                        for line in &hunk.lines[start..i] {
                            rows.push(ChangeRow::Line {
                                section,
                                path: entry.path.clone(),
                                line: line.clone(),
                            });
                        }
                    }
                } else {
                    rows.push(ChangeRow::Line {
                        section,
                        path: entry.path.clone(),
                        line: hunk.lines[i].clone(),
                    });
                    i += 1;
                }
            }
            line_index += hunk.lines.len();
        }
    }

    fn render_change_row(row: ChangeRow, entity: gpui::Entity<Self>, theme: Theme) -> AnyElement {
        match row {
            ChangeRow::File {
                section,
                entry,
                stat,
                expanded,
            } => Self::render_change_file(section, entry, stat, expanded, entity, theme)
                .into_any_element(),
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
                .h(px(HUNK_ROW_HEIGHT))
                .w_full()
                .flex_none()
                .px(px(10.0))
                .flex()
                .items_center()
                .text_size(px(11.5))
                .text_color(theme.subtitle)
                .bg(theme.diff_hunk_background)
                .child(header)
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
                .px(px(6.0))
                .flex()
                .items_center()
                .text_size(px(11.5))
                .text_color(theme.meta)
                .child(
                    div()
                        .flex_1()
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
            .text_size(px(11.5))
            .text_color(theme.meta)
            .bg(theme.diff_hunk_background)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                band_entity.update(cx, |tab, cx| {
                    tab.toggle_band(section, band_path.clone(), key, cx);
                });
            })
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.hairline))
            .child(div().text_color(theme.meta).child(label))
            .child(div().h(px(1.0)).w(px(24.0)).bg(theme.hairline))
            .child(
                div()
                    .text_color(theme.meta)
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
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let entity_for_toggle = entity.clone();
        div()
            .id(format!("changes-section-{}", section.slug()))
            .debug_selector(|| "changes-section-header".into())
            .h(px(BAND_ROW_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.5))
            .bg(theme.diff_hunk_background)
            .hover(|style| style.bg(theme.row_hover))
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
                        IconElement::new(Icon::ChevronRight, px(10.0)).text_color(theme.subtitle)
                    } else {
                        IconElement::new(Icon::ChevronDown, px(10.0)).text_color(theme.subtitle)
                    }),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.subtitle)
                    .child(section.label()),
            )
            .child(div().text_color(theme.meta).child(format!("({count})")))
    }

    fn render_change_file(
        section: ChangeSection,
        entry: StatusEntry,
        stat: Option<DiffStat>,
        expanded: bool,
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
        let glyph = file_glyph(&path);
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
        div()
            .id(format!("change-{}-{}", section.slug(), path.display()))
            .debug_selector(|| "changes-file-row".into())
            .h(px(ROW_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.5))
            // The path is neutral text — the +/− counts carry the status.
            .text_color(theme.title)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                entity_for_toggle.update(cx, |tab, cx| {
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
                        IconElement::new(Icon::ChevronDown, px(10.0)).text_color(theme.subtitle)
                    } else {
                        IconElement::new(Icon::ChevronRight, px(10.0)).text_color(theme.subtitle)
                    }),
            )
            .child(
                div()
                    .w(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph.map_or_else(
                        || {
                            IconElement::new(Icon::File, px(12.0))
                                .text_color(color)
                                .into_any_element()
                        },
                        |glyph| div().text_color(color).child(glyph).into_any_element(),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path.to_string_lossy().to_string()),
            )
            .child(div().text_color(theme.diff_deletion).child(deletions))
            .child(div().text_color(theme.diff_addition).child(additions))
            .when(expanded, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .child(destructive_action_text_button(
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
                        .child(
                            div()
                                .id(format!("open-{}-{}", section.slug(), path.display()))
                                .text_color(theme.subtitle)
                                .hover(|style| style.text_color(theme.title))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    entity_for_open.update(cx, |_, cx| {
                                        cx.emit(ChangesTabEvent::OpenFile(path.clone()));
                                    });
                                })
                                .child("↗"),
                        ),
                )
            })
    }

    fn render_diff_line(
        section: ChangeSection,
        path: PathBuf,
        line: DiffLine,
        theme: Theme,
    ) -> impl IntoElement {
        let (background, marker_color, marker) = match line.origin {
            DiffOrigin::Context => (theme.background, theme.meta, " "),
            DiffOrigin::Addition => (theme.diff_addition_background, theme.diff_addition, "+"),
            DiffOrigin::Deletion => (theme.diff_deletion_background, theme.diff_deletion, "−"),
        };
        div()
            .id(format!(
                "line-{}-{}-{}-{}",
                section.slug(),
                path.display(),
                line.old_line_number.unwrap_or(0),
                line.new_line_number.unwrap_or(0)
            ))
            .h(px(DIFF_LINE_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(6.0))
            .flex()
            .items_center()
            .font_family(theme.typography.code_family)
            .text_size(px(11.5))
            .text_color(theme.title)
            .bg(background)
            .child(
                div().w(px(22.0)).text_color(theme.meta).child(
                    line.old_line_number
                        .map_or(String::new(), |n| n.to_string()),
                ),
            )
            .child(
                div().w(px(22.0)).text_color(theme.meta).child(
                    line.new_line_number
                        .map_or(String::new(), |n| n.to_string()),
                ),
            )
            .child(div().w(px(14.0)).text_color(marker_color).child(marker))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(line.content),
            )
    }

    fn render_toolbar(&self, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let stage_entity = entity.clone();
        let discard_entity = entity.clone();
        let expand_entity = entity.clone();
        let collapse_entity = entity.clone();
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
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.5))
                    .text_color(theme.title)
                    .child(title),
            )
            .child(action_text_button(
                "Stage all",
                "stage-all".to_owned(),
                theme,
                move |cx| {
                    stage_entity.update(cx, |tab, cx| {
                        tab.start_operation(stage_all, cx);
                    });
                },
            ))
            .child(action_text_button(
                "Expand All",
                "expand-all".to_owned(),
                theme,
                move |cx| {
                    expand_entity.update(cx, |tab, cx| tab.expand_all(cx));
                },
            ))
            .child(action_text_button(
                "Collapse All",
                "collapse-all".to_owned(),
                theme,
                move |cx| {
                    collapse_entity.update(cx, |tab, cx| tab.collapse_all(cx));
                },
            ))
            .child(destructive_action_text_button(
                "Discard all",
                "discard-all".to_owned(),
                theme,
                move |window, cx| {
                    discard_entity.update(cx, |tab, cx| {
                        tab.confirm_discard_all(window, cx);
                    });
                },
            ))
    }
}

impl EventEmitter<ChangesTabEvent> for ChangesTab {}

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
    fn render_body(&self, entity: gpui::Entity<Self>, theme: Theme) -> AnyElement {
        if let Some(error) = &self.git_error {
            return Self::render_error_state(error, entity, theme).into_any_element();
        }
        if self.git_task.is_some() && self.entries.is_empty() {
            return div()
                .id("changes-loading")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.headline)
                .text_color(theme.subtitle)
                .child("Loading changes…")
                .into_any_element();
        }
        let sections = self.section_rows();
        let row_entity = entity;
        div()
            .id("changes-list")
            .debug_selector(|| "changes-list".into())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .children(sections.into_iter().flat_map(move |section| {
                let mut elements: Vec<AnyElement> = vec![
                    Self::render_section_header(
                        section.section,
                        section.count,
                        section.collapsed,
                        row_entity.clone(),
                        theme,
                    )
                    .into_any_element(),
                ];
                for row in section.rows {
                    elements.push(Self::render_change_row(row, row_entity.clone(), theme));
                }
                elements
            }))
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
                    .text_color(theme.git_conflict)
                    .child(format!("Git is unavailable: {error}")),
            )
            .child(action_text_button(
                "Retry",
                "changes-retry".to_owned(),
                theme,
                move |cx| retry_entity.update(cx, |tab, cx| tab.refresh(cx)),
            ))
    }
}

impl Render for ChangesTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        self.ensure_refresh(cx);
        let entity = cx.entity();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(self.render_toolbar(entity.clone(), theme))
            .child(self.render_body(entity, theme))
    }
}

/// The status hue for a change row's glyph — the path text stays neutral.
fn status_color(entry: &StatusEntry, theme: Theme) -> Rgba {
    if entry.is_conflicted() {
        theme.git_conflict
    } else if entry.is_untracked() {
        theme.git_untracked
    } else if entry.has_worktree_changes() {
        theme.git_modified
    } else if entry.is_staged() {
        theme.git_staged
    } else {
        theme.title
    }
}

/// A per-type glyph for known file kinds; the generic fallback is the file
/// icon.
fn file_glyph(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => Some("🦀"),
        Some("swift") => Some("🕊"),
        Some("md") => Some("📝"),
        Some("toml") | Some("yaml") | Some("yml") | Some("json") => Some("⚙"),
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") => Some("🖼"),
        _ => None,
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
            _ => format!("changes-action-{label}"),
        })
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .text_size(px(11.5))
        .text_color(theme.title)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
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
        .text_size(px(11.5))
        .text_color(theme.subtitle)
        .hover(|style| style.text_color(theme.git_conflict))
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
fn load_snapshot(repo_root: &Path) -> Result<GitSnapshot, String> {
    let entries = status(repo_root)
        .map_err(|error| error.to_string())?
        .entries;
    let stats = stats(repo_root, &entries).map_err(|error| error.to_string())?;
    let mut diffs = HashMap::new();
    let mut diff_errors = HashMap::new();
    for entry in &entries {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use tiller_git::StatusKind;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-changes-test-{}-{unique}",
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

    fn clean_git_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Tiller tests"]);
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
    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..600 {
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

    fn wait_for_tab(
        cx: &VisualTestContext,
        tab: &gpui::Entity<ChangesTab>,
        mut condition: impl FnMut(&ChangesTab) -> bool,
    ) {
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
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

        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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

        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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

        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
        assert_eq!(sections.len(), 2, "only the non-empty sections render");
        assert!(
            sections
                .iter()
                .all(|section| section.section != ChangeSection::Untracked),
            "an empty Untracked bucket must not render a section"
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
        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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
        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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
        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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
        git(&dir.0, &["config", "user.name", "Tiller tests"]);
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
        for _ in 0..600 {
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
        git(&dir.0, &["config", "user.name", "Tiller tests"]);
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("rewrite");
        let retry = cx
            .debug_bounds("changes-retry")
            .expect("the retry button is visible");
        cx.simulate_click(retry.center(), Modifiers::none());
        for _ in 0..600 {
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
        // `tiller_git::discard_all`). The confirmed click clears every
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
        git(&dir.0, &["config", "user.name", "Tiller tests"]);
        std::fs::write(dir.0.join("new.txt"), "one\ntwo\nthree\nfour\n").expect("write");
        git(&dir.0, &["add", "new.txt"]);

        let tab = cx.new(|cx| ChangesTab::new(dir.0.clone(), cx));
        tab.update(cx, |tab, cx| tab.refresh(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| {
                tab.entries.iter().any(|entry| entry.path == *"new.txt")
            })
        });

        let sections = tab.read_with(cx, |tab, _| tab.section_rows());
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
            std::env::temp_dir().join(format!("tiller-changes-missing-{}", std::process::id()));
        let error = load_snapshot(&missing).expect_err("no repo, no git: the load must fail");
        assert!(
            error.contains("failed to spawn git"),
            "the missing binary is named, not hidden: {error}"
        );
        assert!(
            error.contains("No such file"),
            "the OS error is included so the user can act: {error}"
        );
    }

    /// A file whose diff could not be fetched renders an explicit
    /// "unavailable" row when expanded — never empty rows that could be
    /// mistaken for "no changes".
    #[test]
    fn an_expanded_file_whose_diff_failed_says_unavailable() {
        let tab = ChangesTab {
            repo_root: PathBuf::from("/tmp"),
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
            git_error: None,
            refresh_started: false,
        };
        let entry = tab.entries[0].clone();
        let mut rows = Vec::new();
        tab.expand_diff(&mut rows, ChangeSection::Changed, &entry);
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
}

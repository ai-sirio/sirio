//! The checkout right panel: filesystem browsing, git changes, and activity.
//!
//! The data model deliberately stays local to this panel. Git operations are
//! delegated to `tiller_git`; the host application can later replace the
//! refresh callbacks with its project store without changing the row layout.

use gpui::{
    AnyElement, App, Context, EventEmitter, FontWeight, MouseButton, PromptLevel, Render, Rgba,
    Task, Window, div, prelude::*, px, uniform_list,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tiller_git::{
    DiffLine, DiffOrigin, DiffStat, FileDiff, GitError, StatusEntry, diff_entry, discard,
    discard_all, stage, stage_all, stats, status, unstage,
};
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement};

const PANEL_WIDTH: f32 = 405.0;
const HEADER_HEIGHT: f32 = 40.0;
const TOOLBAR_HEIGHT: f32 = 34.0;
const ROW_HEIGHT: f32 = 24.0;
const ACTIVITY_HEADER_HEIGHT: f32 = 27.0;
const ACTIVITY_ROW_HEIGHT: f32 = 43.0;
/// How long a Changes view may remain stale after an external edit.
///
/// The poll is deliberately serialized with manual git work below: a slow
/// status call cannot cause another call to pile up on the background
/// executor, and a single panel owns only one long-lived timer.
const CHANGES_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Which segment is visible in the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelTab {
    /// The on-disk directory tree.
    #[default]
    Files,
    /// The checkout's changed paths and hunks.
    Changes,
}

/// Status shown at the trailing edge of an activity row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStatus {
    /// The surface is waiting for input.
    Idle,
    /// The surface is currently running.
    Running,
    /// The surface completed successfully.
    Done,
    /// The surface reported an error.
    Error,
}

/// User actions originating from an activity row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPanelEvent {
    /// Select the open tab at this activity index.
    SelectActivity(usize),
    /// Close the open tab at this activity index.
    CloseActivity(usize),
    /// Open a file from the Files tree in the host application's tab strip.
    OpenFile(PathBuf),
}

/// A surface shown in the Activity section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySurface {
    /// The typed surface icon from the shared embedded icon set.
    pub icon: Icon,
    /// Surface title.
    pub title: String,
    /// Project/worktree subtitle.
    pub location: String,
    /// Current surface status.
    pub status: ActivityStatus,
}

impl ActivitySurface {
    /// Build one activity row.
    #[must_use]
    pub fn new(
        icon: Icon,
        title: impl Into<String>,
        location: impl Into<String>,
        status: ActivityStatus,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            location: location.into(),
            status,
        }
    }
}

#[derive(Clone, Debug)]
struct FileNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    modified: bool,
    expanded: bool,
    /// Why the directory could not be read (permissions, a vanished
    /// mount). Rendered on the row — an unreadable directory must not
    /// vanish silently and must not panic.
    read_error: Option<String>,
    children: Vec<FileNode>,
}

#[derive(Clone, Debug)]
struct FileRow {
    node: FileNode,
    depth: usize,
}

#[derive(Clone, Debug)]
enum ChangeRow {
    File {
        entry: StatusEntry,
        stat: DiffStat,
        expanded: bool,
    },
    Hunk {
        path: PathBuf,
        header: String,
    },
    Line {
        path: PathBuf,
        line: DiffLine,
    },
}

#[derive(Default)]
struct GitSnapshot {
    entries: Vec<StatusEntry>,
    diffs: HashMap<PathBuf, FileDiff>,
    stats: HashMap<PathBuf, DiffStat>,
    /// The on-disk directory tree, built off the UI thread together with
    /// the git work so the refresh path never touches the filesystem on
    /// the render thread.
    file_tree: Vec<FileNode>,
}

/// The GPUI right panel for one checkout.
pub struct RightPanel {
    repo_root: PathBuf,
    tab: PanelTab,
    activity_expanded: bool,
    activity: Vec<ActivitySurface>,
    file_tree: Vec<FileNode>,
    changed_paths: HashSet<PathBuf>,
    entries: Vec<StatusEntry>,
    diffs: HashMap<PathBuf, FileDiff>,
    stats: HashMap<PathBuf, DiffStat>,
    expanded_changes: HashSet<PathBuf>,
    git_task: Option<Task<()>>,
    git_error: Option<String>,
    /// One polling loop per panel, armed on first render. The detached loop
    /// holds only a weak entity handle and exits as soon as the panel is gone.
    changes_refresh_task_started: bool,
    /// The in-flight folder-expansion walk, if any. Replaced (never
    /// queued) on every new expansion request.
    walk_task: Option<Task<()>>,
    /// Bumped on every walk request (and on collapse): a walk that
    /// completes after a newer one was requested must not apply its
    /// result late.
    walk_generation: u64,
}

impl RightPanel {
    /// Create a panel rooted at a real checkout path.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self::with_activity(repo_root, Vec::new())
    }

    /// Create a panel with the host application's current open surfaces.
    pub fn with_activity(repo_root: impl Into<PathBuf>, activity: Vec<ActivitySurface>) -> Self {
        let panel = Self {
            repo_root: repo_root.into(),
            tab: PanelTab::Files,
            activity_expanded: true,
            activity,
            file_tree: Vec::new(),
            changed_paths: HashSet::new(),
            entries: Vec::new(),
            diffs: HashMap::new(),
            stats: HashMap::new(),
            expanded_changes: HashSet::new(),
            git_task: None,
            git_error: None,
            changes_refresh_task_started: false,
            walk_task: None,
            walk_generation: 0,
        };
        panel
    }

    /// Set the visible Files/Changes segment.
    pub fn set_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    /// Return the current segment, useful to the host application's routing.
    #[must_use]
    pub fn tab(&self) -> PanelTab {
        self.tab
    }

    /// Replace the host-provided activity rows. The host (`main.rs`) calls
    /// this every render to stay live with `Chat`'s own state, same as it
    /// re-fetches `Theme::get(cx)` every render rather than caching a slice
    /// of it — so this must no-op on an unchanged value or every render
    /// would `notify()` and trigger another render, forever.
    pub fn set_activity(&mut self, activity: Vec<ActivitySurface>, cx: &mut Context<Self>) {
        if self.activity == activity {
            return;
        }
        self.activity = activity;
        cx.notify();
    }

    fn apply_snapshot(&mut self, snapshot: GitSnapshot) {
        self.expanded_changes
            .retain(|path| snapshot.entries.iter().any(|entry| &entry.path == path));
        self.entries = snapshot.entries;
        self.changed_paths = self
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        self.stats = snapshot.stats;
        self.diffs = snapshot.diffs;
        self.file_tree = snapshot.file_tree;
    }

    fn apply_changes_snapshot(&mut self, snapshot: GitSnapshot) {
        self.expanded_changes
            .retain(|path| snapshot.entries.iter().any(|entry| &entry.path == path));
        self.entries = snapshot.entries;
        self.changed_paths = self
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        self.stats = snapshot.stats;
        self.diffs = snapshot.diffs;
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.git_task.is_some() {
            return;
        }

        let repo_root = self.repo_root.clone();
        self.git_error = None;
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_spawn(async move { load_snapshot(&repo_root) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.apply_snapshot(snapshot);
                panel.git_task = None;
                cx.notify();
            });
        }));
    }

    /// Arms the single automatic Changes refresh loop. The timer itself runs
    /// on GPUI's background executor; each refresh then uses the existing
    /// background snapshot path, so neither the timer nor git status blocks
    /// rendering. The loop exits when its entity disappears, and the held
    /// task is cancelled when the panel is dropped.
    fn ensure_changes_refresh(&mut self, cx: &mut Context<Self>) {
        if self.changes_refresh_task_started {
            return;
        }

        self.changes_refresh_task_started = true;
        cx.spawn(async move |this, cx| {
            let mut initial_refresh = true;
            loop {
                if this
                    .update(cx, |panel, cx| {
                        if initial_refresh {
                            panel.refresh(cx);
                        } else {
                            panel.refresh_changes(cx);
                        }
                    })
                    .is_err()
                {
                    return;
                }
                initial_refresh = false;
                cx.background_executor()
                    .timer(CHANGES_REFRESH_INTERVAL)
                    .await;
            }
        })
        .detach();
    }

    fn refresh_changes(&mut self, cx: &mut Context<Self>) {
        if self.git_task.is_some() {
            return;
        }

        let repo_root = self.repo_root.clone();
        self.git_error = None;
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_spawn(async move { load_changes_snapshot(&repo_root) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.apply_changes_snapshot(snapshot);
                panel.git_task = None;
                cx.notify();
            });
        }));
    }

    fn start_operation<F>(&mut self, operation: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&Path) -> Result<(), GitError> + Send + 'static,
    {
        if self.git_task.is_some() {
            return;
        }

        let repo_root = self.repo_root.clone();
        self.git_error = None;
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let result = operation(&repo_root);
                    let snapshot = result.is_ok().then(|| load_snapshot(&repo_root));
                    (result, snapshot)
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.git_task = None;
                match outcome {
                    (Ok(()), Some(snapshot)) => panel.apply_snapshot(snapshot),
                    (Err(error), _) => panel.git_error = Some(error.to_string()),
                    (Ok(()), None) => {
                        panel.git_error = Some("git operation did not return a snapshot".into())
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
            let _ = this.update(cx, |panel, cx| {
                panel.start_operation(move |repo| discard(repo, &path), cx)
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
            let _ = this.update(cx, |panel, cx| {
                panel.start_operation(|repo| discard_all(repo), cx)
            });
        })
        .detach();
    }

    fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        self.activity_expanded = !self.activity_expanded;
        cx.notify();
    }

    fn toggle_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let (expanded, needs_walk) = {
            let Some(node) = find_node_mut(&mut self.file_tree, path) else {
                cx.notify();
                return;
            };
            node.expanded = !node.expanded;
            node.read_error = None;
            (node.expanded, node.expanded && node.children.is_empty())
        };
        if needs_walk {
            self.start_walk(path.to_path_buf(), cx);
        } else if !expanded {
            // The collapse supersedes any in-flight expansion of this (or
            // any) folder: its result must not land later.
            self.walk_task = None;
            self.walk_generation += 1;
        }
        cx.notify();
    }

    /// Starts an off-thread directory walk for `path`. A newer expansion
    /// request supersedes the in-flight one: the task slot is replaced
    /// (never queued, so fast clicking cannot pile up walks) and the
    /// generation guard drops any walk that completes after a newer one
    /// was requested.
    fn start_walk(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.walk_task = None;
        self.walk_generation += 1;
        let generation = self.walk_generation;
        let repo_root = self.repo_root.clone();
        let changed_paths = self.changed_paths.clone();
        // The walk consumes `path`; the update needs its own copy to find
        // the node again.
        let apply_path = path.clone();
        self.walk_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { read_tree(&repo_root, &path, &changed_paths) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                if generation != panel.walk_generation {
                    // Superseded by a newer walk (or a collapse): drop the
                    // result instead of applying it late.
                    return;
                }
                panel.walk_task = None;
                if let Some(node) = find_node_mut(&mut panel.file_tree, &apply_path) {
                    match result {
                        Ok(children) => node.children = children,
                        Err(error) => node.read_error = Some(error),
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.emit(RightPanelEvent::OpenFile(path));
    }

    fn toggle_change(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.entries.iter().any(|entry| entry.path == path) {
            if self
                .diffs
                .get(path)
                .is_some_and(|diff| !diff.hunks.is_empty())
            {
                if self.expanded_changes.contains(path) {
                    self.expanded_changes.remove(path);
                } else {
                    self.expanded_changes.insert(path.to_path_buf());
                }
            }
        }
        cx.notify();
    }

    fn is_expanded(&self, path: &Path) -> bool {
        self.expanded_changes.contains(path)
    }

    fn change_rows(&self) -> Vec<ChangeRow> {
        let mut rows = Vec::new();
        for entry in &self.entries {
            let expanded = self.is_expanded(&entry.path);
            rows.push(ChangeRow::File {
                entry: entry.clone(),
                stat: self.stats.get(&entry.path).copied().unwrap_or(DiffStat {
                    additions: 0,
                    deletions: 0,
                    is_binary: false,
                }),
                expanded,
            });
            if !expanded {
                continue;
            }
            if let Some(diff) = self.diffs.get(&entry.path) {
                for hunk in &diff.hunks {
                    rows.push(ChangeRow::Hunk {
                        path: entry.path.clone(),
                        header: hunk.header.clone(),
                    });
                    rows.extend(hunk.lines.iter().cloned().map(|line| ChangeRow::Line {
                        path: entry.path.clone(),
                        line,
                    }));
                }
            }
        }
        rows
    }

    fn file_rows(&self) -> Vec<FileRow> {
        let mut rows = Vec::new();
        flatten_files(&self.file_tree, 0, &mut rows);
        rows
    }

    fn render_file_row(row: FileRow, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let path = row.node.path.clone();
        let is_dir = row.node.is_dir;
        let name = row.node.name.clone();
        let modified = row.node.modified;
        let read_error = row.node.read_error.clone();
        let disclosure = if is_dir {
            if row.node.expanded {
                Some(IconElement::new(Icon::ChevronDown, px(10.0)).text_color(theme.subtitle))
            } else {
                Some(IconElement::new(Icon::ChevronRight, px(10.0)).text_color(theme.subtitle))
            }
        } else {
            None
        };
        let indent = row.depth as f32 * 16.0;
        let row_id = format!("file-{}", path.display());
        let click_path = path.clone();
        div()
            .id(row_id)
            .h(px(ROW_HEIGHT))
            .w_full()
            .pl(px(8.0 + indent))
            .pr(px(12.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(if read_error.is_some() {
                // An unreadable directory renders dimmed with a warning —
                // it must not vanish silently, and it must not panic.
                theme.subtitle
            } else if modified {
                theme.git_modified
            } else {
                theme.title
            })
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                if is_dir {
                    entity.update(cx, |panel, cx| panel.toggle_file(&click_path, cx));
                } else {
                    entity.update(cx, |panel, cx| panel.open_file(click_path.clone(), cx));
                }
            })
            .child(
                div()
                    .w(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match disclosure {
                        Some(element) => element.into_any_element(),
                        None => div().into_any_element(),
                    }),
            )
            .child(div().flex_1().overflow_hidden().text_ellipsis().child(name))
            .when(read_error.is_some(), |this| {
                this.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.git_conflict)
                        .child("⚠"),
                )
            })
            .when(modified, |this| {
                this.child(
                    div()
                        .w(px(5.0))
                        .h(px(5.0))
                        .rounded(px(3.0))
                        .bg(theme.git_modified),
                )
            })
    }

    fn render_change_row(row: ChangeRow, entity: gpui::Entity<Self>, theme: Theme) -> AnyElement {
        match row {
            ChangeRow::File {
                entry,
                stat,
                expanded,
            } => Self::render_change_file(entry, stat, expanded, entity, theme).into_any_element(),
            ChangeRow::Hunk { path, header } => div()
                .id(format!("hunk-{}-{}", path.display(), header))
                .h(px(ROW_HEIGHT))
                .w_full()
                .px(px(10.0))
                .flex()
                .items_center()
                .text_size(px(10.5))
                .text_color(theme.file_link)
                .bg(theme.diff_hunk_background)
                .child(header)
                .into_any_element(),
            ChangeRow::Line { path, line } => {
                Self::render_diff_line(path, line, theme).into_any_element()
            }
        }
    }

    fn render_change_file(
        entry: StatusEntry,
        stat: DiffStat,
        expanded: bool,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let path = entry.path.clone();
        let path_for_toggle = path.clone();
        let path_for_stage = path.clone();
        let path_for_discard = path.clone();
        let path_for_trash = path.clone();
        let is_staged = entry.is_staged();
        let stage_label = if is_staged { "Unstage" } else { "Stage" };
        let color = status_color(&entry, theme);
        let glyph = file_glyph(&path);
        let additions = if stat.is_binary {
            "·".to_owned()
        } else {
            format!("+{}", stat.additions)
        };
        let deletions = if stat.is_binary {
            "·".to_owned()
        } else {
            format!("−{}", stat.deletions)
        };
        let entity_for_toggle = entity.clone();
        let entity_for_stage = entity.clone();
        let entity_for_discard = entity.clone();
        let entity_for_trash = entity.clone();
        div()
            .id(format!("change-{}", path.display()))
            .h(px(ROW_HEIGHT))
            .w_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.5))
            .text_color(color)
            .hover(|style| style.bg(theme.row_hover))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                entity_for_toggle.update(cx, |panel, cx| panel.toggle_change(&path_for_toggle, cx));
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
            .child(div().w(px(14.0)).flex().items_center().justify_center().child(
                glyph.map_or_else(
                    || IconElement::new(Icon::File, px(12.0)).text_color(color).into_any_element(),
                    |glyph| div().text_color(color).child(glyph).into_any_element(),
                ),
            ))
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
                            format!("discard-{}", path.display()),
                            theme,
                            move |window, cx| {
                                entity_for_discard.update(cx, |panel, cx| {
                                    panel.confirm_discard(path_for_discard.clone(), window, cx);
                                });
                            },
                        ))
                        .child(action_text_button(
                            stage_label,
                            format!("stage-{}", path.display()),
                            theme,
                            move |cx| {
                                entity_for_stage.update(cx, |panel, cx| {
                                    if is_staged {
                                        panel.unstage_path(path_for_stage.clone(), cx);
                                    } else {
                                        panel.stage_path(path_for_stage.clone(), cx);
                                    }
                                });
                            },
                        ))
                        .child(div().text_color(theme.subtitle).child("↗"))
                        .child(destructive_action_text_button(
                            "⌫",
                            format!("discard-icon-{}", path.display()),
                            theme,
                            move |window, cx| {
                                entity_for_trash.update(cx, |panel, cx| {
                                    panel.confirm_discard(path_for_trash.clone(), window, cx);
                                });
                            },
                        )),
                )
            })
    }

    fn render_diff_line(path: PathBuf, line: DiffLine, theme: Theme) -> impl IntoElement {
        let (background, marker_color, marker) = match line.origin {
            DiffOrigin::Context => (theme.background, theme.meta, " "),
            DiffOrigin::Addition => (theme.diff_addition_background, theme.diff_addition, "+"),
            DiffOrigin::Deletion => (theme.diff_deletion_background, theme.diff_deletion, "−"),
        };
        div()
            .id(format!(
                "line-{}-{}-{}",
                path.display(),
                line.old_line_number.unwrap_or(0),
                line.new_line_number.unwrap_or(0)
            ))
            .h(px(ROW_HEIGHT))
            .w_full()
            .px(px(6.0))
            .flex()
            .items_center()
            .text_size(px(10.5))
            .text_color(theme.title)
            .bg(background)
            .child(
                div().w(px(30.0)).text_color(theme.meta).child(
                    line.old_line_number
                        .map_or(String::new(), |n| n.to_string()),
                ),
            )
            .child(
                div().w(px(30.0)).text_color(theme.meta).child(
                    line.new_line_number
                        .map_or(String::new(), |n| n.to_string()),
                ),
            )
            .child(div().w(px(12.0)).text_color(marker_color).child(marker))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(line.content),
            )
    }

    fn render_header(&self, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let files_entity = entity.clone();
        let changes_entity = entity.clone();
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .border_b_1()
            .border_color(theme.hairline)
            .child(segment_button(
                "Files",
                self.tab == PanelTab::Files,
                theme,
                move |cx| files_entity.update(cx, |panel, cx| panel.set_tab(PanelTab::Files, cx)),
            ))
            .child(segment_button(
                "Changes",
                self.tab == PanelTab::Changes,
                theme,
                move |cx| {
                    changes_entity.update(cx, |panel, cx| panel.set_tab(PanelTab::Changes, cx))
                },
            ))
            .child(
                div()
                    .id("close-right-panel")
                    .ml(px(7.0))
                    .w(px(20.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(16.0))
                    .text_color(theme.subtitle)
                    .hover(|style| style.bg(theme.row_hover).rounded(px(4.0)))
                    .child(IconElement::new(Icon::Close, px(13.0)).text_color(theme.subtitle)),
            )
    }

    fn render_files(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refresh_entity = entity.clone();
        let rows = self.file_rows();
        let row_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(
                div()
                    .h(px(TOOLBAR_HEIGHT))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.hairline)
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(10.5))
                            .text_color(theme.meta)
                            .child(self.repo_root.to_string_lossy().to_string()),
                    )
                    .child(icon_button(Icon::RefreshCw, "refresh-files", theme, move |cx| {
                        refresh_entity.update(cx, |panel, cx| panel.refresh(cx));
                    })),
            )
            .child(
                uniform_list(
                    "right-panel-files",
                    rows.len(),
                    cx.processor(move |_panel, range: Range<usize>, _window, _cx| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                Self::render_file_row(row.clone(), row_entity.clone(), theme)
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h(px(0.0)),
            )
            .into_any_element()
    }

    fn render_changes(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refresh_entity = entity.clone();
        let stage_entity = entity.clone();
        let discard_entity = entity.clone();
        let rows = self.change_rows();
        let row_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(
                div()
                    .h(px(TOOLBAR_HEIGHT))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.hairline)
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(12.0))
                            .text_color(theme.title)
                            .child("Local changes"),
                    )
                    .child(icon_button(Icon::RefreshCw, "refresh-changes", theme, move |cx| {
                        refresh_entity.update(cx, |panel, cx| panel.refresh(cx));
                    })),
            )
            .child(
                div()
                    .h(px(31.0))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .text_size(px(11.0))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.title)
                            .child(format!("Changes ({})", self.entries.len())),
                    )
                    .child(action_text_button(
                        "Stage all",
                        "stage-all".to_owned(),
                        theme,
                        move |cx| {
                            stage_entity.update(cx, |panel, cx| {
                                panel.start_operation(|repo| stage_all(repo), cx);
                            });
                        },
                    ))
                    .child(destructive_action_text_button(
                        "Discard all",
                        "discard-all".to_owned(),
                        theme,
                        move |window, cx| {
                            discard_entity.update(cx, |panel, cx| {
                                panel.confirm_discard_all(window, cx);
                            });
                        },
                    )),
            )
            .child(
                uniform_list(
                    "right-panel-changes",
                    rows.len(),
                    cx.processor(move |_panel, range: Range<usize>, _window, _cx| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                Self::render_change_row(row.clone(), row_entity.clone(), theme)
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h(px(0.0)),
            )
            .into_any_element()
    }

    fn render_activity(&self, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let toggle_entity = entity.clone();
        let mut section = div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(ACTIVITY_HEADER_HEIGHT
                + if self.activity_expanded {
                    self.activity.len() as f32 * ACTIVITY_ROW_HEIGHT
                } else {
                    0.0
                }))
            .flex()
            .flex_col()
            .w_full()
            .flex_none()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .id("activity-header")
                    .h(px(ACTIVITY_HEADER_HEIGHT))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        toggle_entity.update(cx, |panel, cx| panel.toggle_activity(cx));
                    })
                    .child(if self.activity_expanded {
                        IconElement::new(Icon::ChevronDown, px(12.0)).text_color(theme.meta)
                    } else {
                        IconElement::new(Icon::ChevronRight, px(12.0)).text_color(theme.meta)
                    })
                    .child("Activity"),
            );
        if self.activity_expanded {
            for (index, surface) in self.activity.iter().cloned().enumerate() {
                section = section.child(Self::render_activity_row(
                    surface,
                    index,
                    entity.clone(),
                    theme,
                ));
            }
        }
        section
    }

    fn render_activity_row(
        surface: ActivitySurface,
        index: usize,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let status = activity_status(surface.status, theme);
        let select_entity = entity.clone();
        let close_entity = entity;
        div()
            .id(format!("activity-{index}"))
            .h(px(ACTIVITY_ROW_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                select_entity.update(cx, |_, cx| {
                    cx.emit(RightPanelEvent::SelectActivity(index));
                });
            })
            .child(
                div()
                    .w(px(15.0))
                    .text_color(theme.tab_focus_accent)
                    .child(IconElement::new(surface.icon, px(14.0))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.title)
                            .child(surface.title),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.meta)
                            .child(surface.location),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(status)
                    .child(activity_status_glyph(surface.status)),
            )
            .child(
                div()
                    .id(format!("activity-close-{index}"))
                    .text_size(px(16.0))
                    .text_color(theme.subtitle)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        close_entity.update(cx, |_, cx| {
                            cx.emit(RightPanelEvent::CloseActivity(index));
                        });
                    })
                    .child(IconElement::new(Icon::Close, px(13.0)).text_color(theme.subtitle)),
            )
    }
}

impl EventEmitter<RightPanelEvent> for RightPanel {}

impl Render for RightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        self.ensure_changes_refresh(cx);
        let entity = cx.entity();
        let content = match self.tab {
            PanelTab::Files => self.render_files(entity.clone(), theme, cx),
            PanelTab::Changes => self.render_changes(entity.clone(), theme, cx),
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .w(px(PANEL_WIDTH))
            .h_full()
            .overflow_hidden()
            .bg(theme.background)
            .child(self.render_header(entity.clone(), theme))
            .child(content)
            .child(self.render_activity(entity, theme))
    }
}

fn segment_button(
    label: &'static str,
    selected: bool,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(format!("segment-{label}"))
        .h(px(24.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if selected {
            theme.title_selected
        } else {
            theme.title
        })
        .when(selected, |this| this.bg(theme.file_link))
        .hover(|style| style.bg(theme.file_link))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
}

fn icon_button(
    icon: Icon,
    id: &'static str,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme.subtitle)
        .hover(|style| style.bg(theme.row_hover).rounded(px(4.0)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(IconElement::new(icon, px(14.0)).text_color(theme.subtitle))
}

fn action_text_button(
    label: &'static str,
    id: String,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .text_size(px(10.5))
        .text_color(theme.subtitle)
        .hover(|style| style.text_color(theme.title))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
}

fn destructive_action_text_button(
    label: &'static str,
    id: String,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .text_size(px(10.5))
        .text_color(theme.subtitle)
        .hover(|style| style.text_color(theme.git_conflict))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(label)
}

fn load_snapshot(repo_root: &Path) -> GitSnapshot {
    let mut snapshot = load_changes_snapshot(repo_root);
    let changed_paths = snapshot
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    // The directory walk runs here, next to the git calls — both are
    // executed by the caller on the background executor, so the refresh
    // path never touches the filesystem on the UI thread.
    snapshot.file_tree = read_tree(repo_root, repo_root, &changed_paths).unwrap_or_default();
    snapshot
}

fn load_changes_snapshot(repo_root: &Path) -> GitSnapshot {
    let entries = status(repo_root)
        .unwrap_or_else(|_| tiller_git::StatusSnapshot::empty())
        .entries;
    let stats = stats(repo_root, &entries).unwrap_or_default();
    let diffs = entries
        .iter()
        .filter_map(|entry| {
            diff_entry(repo_root, entry, tiller_git::DEFAULT_CONTEXT_LINES)
                .ok()
                .map(|diff| (entry.path.clone(), diff))
        })
        .collect();
    GitSnapshot {
        entries,
        diffs,
        stats,
        file_tree: Vec::new(),
    }
}

fn read_tree(
    root: &Path,
    directory: &Path,
    changed_paths: &HashSet<PathBuf>,
) -> Result<Vec<FileNode>, String> {
    let entries = std::fs::read_dir(directory).map_err(|error| error.to_string())?;
    let mut nodes = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".git") {
                return None;
            }
            let is_dir = entry.file_type().ok()?.is_dir();
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let modified = changed_paths
                .iter()
                .any(|changed| changed == &relative || (is_dir && changed.starts_with(&relative)));
            Some(FileNode {
                name: path.file_name()?.to_string_lossy().to_string(),
                path,
                is_dir,
                modified,
                expanded: false,
                read_error: None,
                children: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(nodes)
}

fn flatten_files(nodes: &[FileNode], depth: usize, rows: &mut Vec<FileRow>) {
    for node in nodes {
        rows.push(FileRow {
            node: node.clone(),
            depth,
        });
        if node.is_dir && node.expanded {
            flatten_files(&node.children, depth + 1, rows);
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [FileNode], path: &Path) -> Option<&'a mut FileNode> {
    for node in nodes {
        if node.path == path {
            return Some(node);
        }
        if node.is_dir
            && let Some(found) = find_node_mut(&mut node.children, path)
        {
            return Some(found);
        }
    }
    None
}

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

/// A per-type glyph for known file kinds (kept for the type-identity they
/// carry, matching the reference's coloured file markers); the generic
/// fallback is the Lucide file icon.
fn file_glyph(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => Some("◈"),
        Some("swift") => Some("◉"),
        Some("json") | Some("toml") | Some("yaml") | Some("yml") => Some("◇"),
        Some("md") => Some("▤"),
        _ => None,
    }
}

fn activity_status(status: ActivityStatus, theme: Theme) -> Rgba {
    match status {
        ActivityStatus::Idle => theme.meta,
        ActivityStatus::Running => theme.tab_needs_input,
        ActivityStatus::Done => theme.tab_done,
        ActivityStatus::Error => theme.tab_error,
    }
}

fn activity_status_glyph(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Idle | ActivityStatus::Running => "○",
        ActivityStatus::Done => "✓",
        ActivityStatus::Error => "!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-right-panel-test-{}-{unique}",
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

    fn find_node<'a>(nodes: &'a [FileNode], path: &Path) -> Option<&'a FileNode> {
        for node in nodes {
            if node.path == path {
                return Some(node);
            }
            if let Some(found) = find_node(&node.children, path) {
                return Some(found);
            }
        }
        None
    }

    fn seed_dir_with_files(dir: &Path, count: usize) {
        std::fs::create_dir_all(dir).expect("create dir");
        for i in 0..count {
            std::fs::write(dir.join(format!("file-{i:05}.txt")), "x").expect("write file");
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
    /// exhausted (a real filesystem walk completes on a real thread).
    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..200 {
            if condition() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    #[gpui::test]
    async fn expanding_a_folder_is_asynchronous(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let subdir = dir.0.join("subdir");
        seed_dir_with_files(&subdir, 2000);

        let panel = cx.new(|cx| RightPanel::new(dir.0.clone()));
        panel.update(cx, |panel, cx| panel.refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &subdir).is_some()
            })
        });

        panel.update(cx, |panel, cx| panel.toggle_file(&subdir, cx));

        // The observable consequence of the fix: the call returns before
        // the tree has been populated — the expansion must not perform the
        // read synchronously on the calling thread.
        panel.read_with(cx, |panel, _| {
            let node = find_node(&panel.file_tree, &subdir).expect("subdir node");
            assert!(node.expanded, "the folder toggles open immediately");
            assert!(
                node.children.is_empty(),
                "children must arrive asynchronously, not during the click handler"
            );
        });

        // …and the tree arrives afterwards, once the walk completes.
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &subdir)
                    .map(|node| node.children.len())
                    .unwrap_or(0)
                    == 2000
            })
        });
    }

    #[gpui::test]
    async fn a_superseded_walk_is_dropped(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let first = dir.0.join("first");
        let second = dir.0.join("second");
        seed_dir_with_files(&first, 100);
        seed_dir_with_files(&second, 100);

        let panel = cx.new(|cx| RightPanel::new(dir.0.clone()));
        panel.update(cx, |panel, cx| panel.refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &first).is_some()
                    && find_node(&panel.file_tree, &second).is_some()
            })
        });

        // Expand the first folder, then immediately the second: the first
        // walk is superseded and its result must not land late.
        panel.update(cx, |panel, cx| panel.toggle_file(&first, cx));
        panel.update(cx, |panel, cx| panel.toggle_file(&second, cx));

        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &second)
                    .map(|node| node.children.len())
                    .unwrap_or(0)
                    == 100
            })
        });

        panel.read_with(cx, |panel, _| {
            let first_node = find_node(&panel.file_tree, &first).expect("first node");
            let second_node = find_node(&panel.file_tree, &second).expect("second node");
            assert_eq!(
                first_node.children.len(),
                0,
                "the superseded walk's result must not be applied over the newer one"
            );
            assert_eq!(second_node.children.len(), 100);
        });
    }

    #[gpui::test]
    async fn changes_refresh_automatically_after_an_external_edit(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        clean_git_repo(&dir.0);
        let panel = cx.new(|cx| RightPanel::new(dir.0.clone()));
        panel.update(cx, |panel, cx| panel.refresh(cx));

        // Wait until the initial background snapshot completes, then arm the
        // same task used by Render. The edit below is observed by polling,
        // without relying on a manual refresh click.
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                panel.git_task.is_none()
                    && find_node(&panel.file_tree, &dir.0.join("tracked.txt")).is_some()
            })
        });
        panel.update(cx, |panel, cx| panel.ensure_changes_refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                panel.git_task.is_none()
                    && find_node(&panel.file_tree, &dir.0.join("tracked.txt")).is_some()
            })
        });

        // This is the external edit: no panel method is called afterwards.
        std::fs::write(dir.0.join("tracked.txt"), "changed\n").expect("edit tracked file");
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                panel
                    .entries
                    .iter()
                    .any(|entry| entry.path == PathBuf::from("tracked.txt"))
            })
        });
    }

    #[gpui::test]
    async fn an_unreadable_directory_renders_an_error(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let locked = dir.0.join("locked");
        std::fs::create_dir_all(&locked).expect("create dir");
        std::fs::write(locked.join("secret.txt"), "x").expect("write file");

        let panel = cx.new(|cx| RightPanel::new(dir.0.clone()));
        panel.update(cx, |panel, cx| panel.refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &locked).is_some()
            })
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
                .expect("lock the directory");
        }

        panel.update(cx, |panel, cx| panel.toggle_file(&locked, cx));

        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &locked)
                    .map(|node| node.read_error.is_some())
                    .unwrap_or(false)
            })
        });

        // The row keeps its name and shows the error instead of vanishing.
        panel.read_with(cx, |panel, _| {
            let node = find_node(&panel.file_tree, &locked).expect("locked node");
            assert!(
                node.read_error.is_some(),
                "an unreadable directory must report why, not vanish silently"
            );
            assert_eq!(node.children.len(), 0);
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
                .expect("restore permissions for cleanup");
        }
    }
}

//! The checkout right panel: filesystem browsing, git changes, and activity.
//!
//! The data model deliberately stays local to this panel. Git operations are
//! delegated to `tiller_git`; the host application can later replace the
//! refresh callbacks with its project store without changing the row layout.

use gpui::{
    AnyElement, App, ClipboardItem, Context, EventEmitter, FocusHandle, FontWeight, KeyDownEvent,
    MouseButton, Pixels, Point, Render, Rgba, Task, Window, anchored, div, prelude::*, px,
    uniform_list,
};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tiller_git::status;
use tiller_theme::Theme;

use crate::editor::fs_actions;
use crate::sidebar::icons::{Icon, IconElement};

const PANEL_WIDTH: f32 = 405.0;
const HEADER_HEIGHT: f32 = 40.0;
/// File-tree rows: 12.5px text at 30px, the app's single-line row rhythm.
const TOOLBAR_HEIGHT: f32 = 34.0;
/// File-tree rows: 12.5px text at 30px, the app's single-line row rhythm.
pub(crate) const ROW_HEIGHT: f32 = 30.0;
const ACTIVITY_HEADER_HEIGHT: f32 = 27.0;
/// Two-line activity row: 5 + 18 + 2 + 15 + 5, waku's card math.
const ACTIVITY_ROW_HEIGHT: f32 = 48.0;
/// Status shown at the trailing edge of an activity row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStatus {
    /// The surface is waiting for input.
    Idle,
    /// The surface is currently running.
    Running,
    /// The agent is blocked on a user decision or answer.
    NeedsInput,
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

/// Actions that need a host-owned surface beyond the existing file-open door.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPanelActionEvent {
    /// Open a modified file in the host application's Diff tab.
    OpenDiff(PathBuf),
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
struct FileContextMenu {
    path: PathBuf,
    position: Point<Pixels>,
}

/// The GPUI right panel: the filesystem tree and the activity section.
pub struct RightPanel {
    repo_root: PathBuf,
    /// A panel is constructed for a selected checkout. Closing that checkout
    /// must explicitly revoke the binding; retaining its path would make the
    /// Files/Changes surface look current while serving stale data.
    worktree_selected: bool,
    file_tree: Vec<FileNode>,
    changed_paths: HashSet<PathBuf>,
    activity_expanded: bool,
    activity: Vec<ActivitySurface>,
    /// The in-flight folder-expansion walk, if any. Replaced (never
    /// queued) on every new expansion request.
    walk_task: Option<Task<()>>,
    /// Whether the top-level tree refresh is currently in flight. This is
    /// both the single-flight guard and the loading state rendered to users.
    refresh_started: bool,
    refresh_error: Option<String>,
    selected_path: Option<PathBuf>,
    file_focus: Option<FocusHandle>,
    /// Bumped on every walk request (and on collapse): a walk that
    /// completes after a newer one was requested must not apply its
    /// result late.
    walk_generation: u64,
    /// One polling loop per panel, armed on first render.
    refresh_loop_started: bool,
    file_context_menu: Option<FileContextMenu>,
}

impl RightPanel {
    /// Creates the panel for one checkout.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            worktree_selected: true,
            file_tree: Vec::new(),
            changed_paths: HashSet::new(),
            activity_expanded: false,
            activity: Vec::new(),
            walk_task: None,
            refresh_started: false,
            refresh_error: None,
            selected_path: None,
            file_focus: None,
            walk_generation: 0,
            refresh_loop_started: false,
            file_context_menu: None,
        }
    }

    /// Creates the panel with an initial activity section.
    pub fn with_activity(repo_root: impl Into<PathBuf>, activity: Vec<ActivitySurface>) -> Self {
        Self {
            activity,
            ..Self::new(repo_root)
        }
    }

    /// Replace the host-provided activity rows. The host (`main.rs`) calls
    /// this every render to stay live with `Chat`'s own state; it no-ops on
    /// an unchanged value to avoid notifying every render.
    pub fn set_activity(&mut self, activity: Vec<ActivitySurface>, cx: &mut Context<Self>) {
        if self.activity == activity {
            return;
        }
        self.activity = activity;
        cx.notify();
    }

    /// Remove the panel's checkout binding after its selected worktree
    /// closes. This clears both already drawn rows and any in-flight result's
    /// visible destination; a later worktree selection replaces the panel
    /// with a new bound instance.
    pub fn clear_worktree(&mut self, cx: &mut Context<Self>) {
        if !self.worktree_selected {
            return;
        }
        self.worktree_selected = false;
        self.file_tree.clear();
        self.changed_paths.clear();
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        cx.notify();
    }

    /// Refreshes the changed-paths set and the directory tree off the
    /// render thread. Single-flight on the walk task.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_started || self.walk_task.is_some() {
            return;
        }
        self.refresh_started = true;
        self.refresh_error = None;
        let repo_root = self.repo_root.clone();
        self.walk_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    // Files can browse a plain directory too; an absent Git
                    // repository means no status dots, not an unreadable
                    // filesystem. Root traversal remains the error boundary.
                    let changed = status(&repo_root)
                        .map(|snapshot| {
                            snapshot
                                .entries
                                .iter()
                                .map(|entry| entry.path.clone())
                                .collect::<HashSet<_>>()
                        })
                        .unwrap_or_default();
                    let tree = read_tree(&repo_root, &repo_root, &changed)
                        .map_err(|error| error.to_string())?;
                    Ok::<_, String>((changed, tree))
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.walk_task = None;
                panel.refresh_started = false;
                match result {
                    Ok((changed, tree)) => {
                        panel.changed_paths = changed;
                        panel.file_tree = tree;
                        panel.refresh_error = None;
                    }
                    Err(error) => panel.refresh_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    /// Arms the periodic tree refresh: once immediately, then on a fixed
    /// interval so external edits show up in the tree.
    fn ensure_tree_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_loop_started {
            return;
        }
        self.refresh_loop_started = true;
        self.refresh(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                // F-CHG-03: a failed refresh must self-heal once the
                // underlying condition (e.g. permissions) clears, without
                // requiring a manual Retry click. Keep retrying on the same
                // 1s cadence even while `refresh_error` is set — `refresh`
                // itself is still single-flight, so this costs nothing
                // beyond the one background walk it already runs each tick.
                if this
                    .update(cx, |panel, cx| {
                        panel.refresh(cx);
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
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

    fn open_file_context_menu(
        &mut self,
        path: PathBuf,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.selected_path = Some(path.clone());
        self.file_context_menu = Some(FileContextMenu { path, position });
        cx.notify();
    }

    fn close_file_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.file_context_menu.take().is_some() {
            cx.notify();
        }
    }

    fn render_file_context_menu(
        menu: FileContextMenu,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let path = menu.path;
        let position = menu.position;
        let mut view = div()
            .id("file-context-menu")
            .debug_selector(|| "file-context-menu".to_owned())
            .w(theme.spacing.menu_width)
            .p(theme.spacing.titlebar_control_spacing)
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .shadow_lg();

        for (label, selector) in [
            ("Open", "file-context-open"),
            ("Reveal in File Manager", "file-context-reveal"),
            ("Copy Path", "file-context-copy-path"),
        ] {
            let action_entity = entity.clone();
            let action_path = path.clone();
            let mut row = div()
                .id(selector)
                .debug_selector(move || selector.to_owned())
                .w_full()
                .min_h(theme.spacing.titlebar_control_frame.height)
                .px(theme.spacing.titlebar_control_spacing)
                .py(theme.spacing.titlebar_control_spacing)
                .rounded(theme.radii.control)
                .flex()
                .items_center()
                .text_size(theme.typography.footnote)
                .text_color(theme.title)
                .hover(|style| style.bg(theme.row_hover));

            row = match selector {
                "file-context-open" => row.on_click(move |_, _, cx| {
                    action_entity.update(cx, |panel, cx| {
                        panel.open_file(action_path.clone(), cx);
                        panel.close_file_context_menu(cx);
                    });
                }),
                "file-context-reveal" => row.on_click(move |_, _, cx| {
                    if let Some(mut command) = fs_actions::reveal_command(&action_path) {
                        command
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null());
                        let _ = command.spawn();
                    }
                    action_entity.update(cx, |panel, cx| panel.close_file_context_menu(cx));
                }),
                "file-context-copy-path" => row.on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(fs_actions::copy_path_text(
                        &action_path,
                    )));
                    action_entity.update(cx, |panel, cx| panel.close_file_context_menu(cx));
                }),
                _ => row,
            };
            view = view.child(row.child(label));
        }

        anchored()
            .position(position)
            .snap_to_window()
            .child(view.on_mouse_down_out(move |_, _, cx| {
                entity.update(cx, |panel, cx| panel.close_file_context_menu(cx));
            }))
    }

    fn open_diff(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let relative_path = path
            .strip_prefix(&self.repo_root)
            .map(Path::to_path_buf)
            .unwrap_or(path);
        cx.emit(RightPanelActionEvent::OpenDiff(relative_path));
    }

    fn file_rows(&self) -> Vec<FileRow> {
        let mut rows = Vec::new();
        flatten_files(&self.file_tree, 0, &mut rows);
        rows
    }

    fn render_file_row(
        row: FileRow,
        entity: gpui::Entity<Self>,
        theme: Theme,
        selected: bool,
        file_focus: FocusHandle,
    ) -> impl IntoElement {
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
        let context_path = path.clone();
        let context_entity = entity.clone();
        let diff_path = path.clone();
        let diff_entity = entity.clone();
        let glyph = file_glyph(&path, is_dir);
        div()
            .id(row_id)
            .debug_selector(move || {
                if is_dir {
                    "file-directory-row".to_owned()
                } else {
                    "file-row".to_owned()
                }
            })
            .h(px(ROW_HEIGHT))
            .w_full()
            .pl(px(8.0 + indent))
            .pr(px(12.0))
            .flex()
            .items_center()
            .gap(theme.spacing.titlebar_control_spacing)
            .text_size(px(12.5))
            // Names are neutral text; the status dot carries "modified",
            // and unreadable directories dim rather than shout.
            .text_color(if read_error.is_some() {
                theme.subtitle
            } else {
                theme.title
            })
            .when(selected, |this| this.bg(theme.selected_fill))
            .hover(|style| style.bg(theme.row_hover))
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                file_focus.focus(window, cx);
                entity.update(cx, |panel, cx| {
                    panel.select_file(click_path.clone(), cx);
                    if is_dir {
                        panel.toggle_file(&click_path, cx);
                    } else if event.click_count >= 2 {
                        // Files follow the inventory's double-click contract;
                        // a single click only gives the row focus/hover.
                        panel.open_file(click_path.clone(), cx);
                    }
                });
            })
            .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                cx.stop_propagation();
                context_entity.update(cx, |panel, cx| {
                    if !is_dir {
                        panel.open_file_context_menu(context_path.clone(), event.position, cx);
                    }
                });
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
            // The per-type file glyph — the visible subject of the "File
            // icons" setting. It is resolved from the embedded set (the
            // Material set, the only one this platform offers; the settings
            // screen is gated to match, P19) and tinted like the Swift
            // explorer's subtitle-tinted FileTypeIcon.
            .child(
                div()
                    .w(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(IconElement::new(glyph, px(14.0)).text_color(theme.subtitle)),
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
            .when(modified && !is_dir, |this| {
                this.child(files_action_button(
                    "Diff",
                    "file-open-diff",
                    theme,
                    move |cx| {
                        diff_entity.update(cx, |panel, cx| {
                            panel.open_diff(diff_path.clone(), cx);
                        });
                    },
                ))
            })
    }

    fn select_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.selected_path != Some(path.clone()) {
            self.selected_path = Some(path);
            cx.notify();
        }
    }

    fn on_file_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.file_rows();
        if rows.is_empty() {
            return;
        }
        let current = self
            .selected_path
            .as_ref()
            .and_then(|path| rows.iter().position(|row| &row.node.path == path));
        match event.keystroke.key.as_str() {
            "down" => {
                let index = current.map_or(0, |index| (index + 1).min(rows.len() - 1));
                self.select_file(rows[index].node.path.clone(), cx);
            }
            "up" => {
                let index = current.unwrap_or(0).saturating_sub(1);
                self.select_file(rows[index].node.path.clone(), cx);
            }
            "space" => {
                if let Some(index) = current {
                    let row = &rows[index].node;
                    if row.is_dir {
                        self.toggle_file(&row.path.clone(), cx);
                    }
                }
            }
            "enter" | "return" => {
                if let Some(index) = current {
                    let row = &rows[index].node;
                    if row.is_dir {
                        self.toggle_file(&row.path.clone(), cx);
                    } else {
                        self.open_file(row.path.clone(), cx);
                    }
                }
            }
            _ => return,
        }
    }

    fn render_header(&self, theme: Theme) -> impl IntoElement {
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.5))
                    .text_color(theme.title)
                    .child("Files"),
            )
            .child(
                div()
                    .id("close-right-panel")
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
        let rows = self.file_rows();
        let row_entity = entity.clone();
        let refresh_entity = entity.clone();
        let file_focus = self
            .file_focus
            .as_ref()
            .expect("Files focus is initialized")
            .clone();
        let selected_path = self.selected_path.clone();
        let processor_focus = file_focus.clone();
        let toolbar = div()
            .h(px(TOOLBAR_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(11.5))
                    .text_color(theme.meta)
                    .child(self.repo_root.to_string_lossy().to_string()),
            )
            .child(files_action_button(
                "Refresh",
                "files-refresh",
                theme,
                move |cx| refresh_entity.update(cx, |panel, cx| panel.refresh(cx)),
            ));
        let body = if self.refresh_started {
            div()
                .id("files-loading")
                .debug_selector(|| "files-loading".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.headline)
                .text_color(theme.subtitle)
                .child("Loading files…")
                .into_any_element()
        } else if let Some(error) = &self.refresh_error {
            let retry_entity = entity.clone();
            div()
                .id("files-error")
                .debug_selector(|| "files-error".to_owned())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.titlebar_control_spacing)
                .p(theme.spacing.card_gap)
                .child(
                    div()
                        .text_size(theme.typography.headline)
                        .text_color(theme.git_conflict)
                        .child(format!("Files unavailable: {error}")),
                )
                .child(files_action_button(
                    "Retry",
                    "files-retry",
                    theme,
                    move |cx| retry_entity.update(cx, |panel, cx| panel.refresh(cx)),
                ))
                .into_any_element()
        } else {
            let list = uniform_list(
                "right-panel-files",
                rows.len(),
                cx.processor(move |_panel, range: Range<usize>, _window, _cx| {
                    range
                        .filter_map(|index| rows.get(index))
                        .map(|row| {
                            Self::render_file_row(
                                row.clone(),
                                row_entity.clone(),
                                theme,
                                selected_path.as_ref() == Some(&row.node.path),
                                processor_focus.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .debug_selector(|| "right-panel-files".to_owned())
            .track_focus(&file_focus)
            .on_key_down(cx.listener(Self::on_file_key))
            .flex_1()
            .min_h(px(0.0));
            list.into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(toolbar)
            .child(body)
            .into_any_element()
    }
}

impl RightPanel {
    fn toggle_activity(&mut self, cx: &mut Context<Self>) {
        self.activity_expanded = !self.activity_expanded;
        cx.notify();
    }

    fn render_activity(&self, entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        let toggle_entity = entity.clone();
        let running_count = self
            .activity
            .iter()
            .filter(|surface| surface.status == ActivityStatus::Running)
            .count();
        let mut section = div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(ACTIVITY_HEADER_HEIGHT
                + if self.activity_expanded {
                    // F-CHG-20: even with zero rows, the expanded section
                    // still renders one "No activity" placeholder row, so
                    // the height must reserve space for at least one row —
                    // otherwise that row is squeezed into near-zero visible
                    // height and its text renders as illegible specks.
                    self.activity.len().max(1) as f32 * ACTIVITY_ROW_HEIGHT
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
                    .debug_selector(|| "activity-header".into())
                    .h(px(ACTIVITY_HEADER_HEIGHT))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.footnote)
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
                    .child("Activity")
                    // F-CHG-20: the running count. A quiet meta label beside
                    // the header, present only while something is running,
                    // so a live worktree is visible from the collapsed
                    // header alone.
                    .when(running_count > 0, |this| {
                        this.child(
                            div()
                                .id("activity-running-count")
                                .debug_selector(|| "activity-running-count".into())
                                .text_size(theme.typography.caption2)
                                .text_color(theme.meta)
                                .child(format!("{running_count} running")),
                        )
                    }),
            );
        if self.activity_expanded {
            if self.activity.is_empty() {
                // F-CHG-20: the no-activity empty state, only visible while
                // the section is expanded — the collapsed header stays
                // silent instead of shouting about nothing.
                section = section.child(
                    div()
                        .id("activity-empty")
                        .debug_selector(|| "activity-empty".into())
                        .h(px(ACTIVITY_ROW_HEIGHT))
                        .w_full()
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .text_size(theme.typography.footnote)
                        .text_color(theme.meta)
                        .child("No activity"),
                );
            } else {
                for (index, surface) in self.activity.iter().cloned().enumerate() {
                    section = section.child(Self::render_activity_row(
                        surface,
                        index,
                        entity.clone(),
                        theme,
                    ));
                }
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
        let status_name = match surface.status {
            ActivityStatus::Idle => "idle",
            ActivityStatus::Running => "running",
            ActivityStatus::NeedsInput => "needs-input",
            ActivityStatus::Done => "done",
            ActivityStatus::Error => "error",
        };
        let status_id = format!("activity-status-{status_name}-{index}");
        let select_entity = entity.clone();
        let close_entity = entity;
        div()
            .id(format!("activity-{index}"))
            .debug_selector(move || format!("activity-{index}"))
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
                            .text_size(theme.typography.headline)
                            .text_color(theme.title)
                            .child(surface.title),
                    )
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.meta)
                            .child(surface.location),
                    ),
            )
            .child(
                div()
                    .id(status_id.clone())
                    .debug_selector(move || status_id.clone())
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
impl EventEmitter<RightPanelActionEvent> for RightPanel {}

impl Render for RightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        if self.worktree_selected {
            self.ensure_tree_refresh(cx);
        }
        if self.worktree_selected && self.file_focus.is_none() {
            self.file_focus = Some(cx.focus_handle().tab_stop(true));
        }
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .flex_col()
            .w(px(PANEL_WIDTH))
            .h_full()
            .overflow_hidden()
            .bg(theme.background)
            .child(self.render_header(theme))
            .child(if self.worktree_selected {
                self.render_files(entity.clone(), theme, cx).into_any_element()
            } else {
                div()
                    .id("right-panel-no-worktree")
                    .debug_selector(|| "right-panel-no-worktree".to_owned())
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(theme.spacing.card_gap)
                    .p(theme.spacing.card_gap)
                    .text_size(theme.typography.headline)
                    .text_color(theme.meta)
                    .child(IconElement::new(Icon::PanelRight, px(24.0)).text_color(theme.meta))
                    .child("No worktree selected")
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.meta)
                            .child("Select a worktree to inspect its files and changes."),
                    )
                    .into_any_element()
            })
            .when(self.worktree_selected, |this| {
                this.when_some(self.file_context_menu.clone(), |this, menu| {
                this.child(Self::render_file_context_menu(menu, entity.clone(), theme))
                })
            })
            .child(self.render_activity(entity, theme))
    }
}

fn files_action_button(
    label: &'static str,
    id: &'static str,
    theme: Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        // The token layer has no compact-action padding yet; use its
        // titlebar spacing as the nearest COSMIC control rhythm.
        .px(theme.spacing.titlebar_control_spacing)
        .py(theme.spacing.titlebar_control_spacing)
        .rounded(theme.radii.control)
        .text_size(theme.typography.caption2)
        .text_color(theme.title)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
}

/// The per-type file glyph for the Files tree, resolved from the embedded
/// icon set — the Material set, the only one this platform offers (the
/// settings screen is gated to match: SF Symbols is macOS-only, P19).
/// Directories get the folder mark; a handful of well-known kinds get their
/// own glyph, mirroring the Swift `FileIconTheme` intent (shell → terminal,
/// git → branch, env/settings → gear); everything else shares the generic
/// file mark.
fn file_glyph(path: &Path, is_dir: bool) -> Icon {
    if is_dir {
        return Icon::FolderFill;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(extension.as_str(), "sh" | "bash" | "zsh" | "fish")
        || lower.starts_with(".bash")
        || lower.starts_with(".zsh")
        || lower == ".profile"
    {
        return Icon::SquareTerminal;
    }
    if lower.starts_with(".git")
        || matches!(lower.as_str(), "gitignore" | "gitattributes" | "gitmodules")
    {
        return Icon::GitBranch;
    }
    if lower.starts_with(".env")
        || matches!(lower.as_str(), ".editorconfig" | ".gitconfig" | ".npmrc")
    {
        return Icon::Settings;
    }
    Icon::File
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

fn activity_status(status: ActivityStatus, theme: Theme) -> Rgba {
    match status {
        ActivityStatus::Idle => theme.meta,
        ActivityStatus::Running => theme.accent,
        ActivityStatus::NeedsInput => theme.tab_needs_input,
        ActivityStatus::Done => theme.tab_done,
        ActivityStatus::Error => theme.tab_error,
    }
}

fn activity_status_glyph(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Idle => "○",
        ActivityStatus::Running => "●",
        ActivityStatus::NeedsInput => "?",
        ActivityStatus::Done => "✓",
        ActivityStatus::Error => "!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext,
        VisualTestContext,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
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

    /// Pumps the test executors until `condition` holds or the budget is
    /// exhausted (a real filesystem walk completes on a real thread).
    /// Pumps the test executors until `condition` holds or the budget is
    /// exhausted (a real filesystem walk completes on a real thread). The
    /// budget mirrors the F-CHG-09 drawn-test pattern: generous enough that
    /// a state landing late under parallel CI load still passes, and a
    /// failure is a real failure, never a timing-luck win.
    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..600 {
            if condition() {
                return;
            }
            // Advance the virtual clock so the panel's own 1s refresh timer
            // fires; the walk itself completes on a real thread.
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    #[test]
    fn file_glyph_resolves_known_kinds_from_the_embedded_set() {
        // The Files tree's per-type glyphs come from the embedded set — the
        // Material set, the only one this platform can render. The mapping is
        // extension-driven so the glyph follows the file, not its folder.
        assert_eq!(
            file_glyph(Path::new("/repo/src/main.rs"), false),
            Icon::File,
            "unknown kinds share the generic file mark"
        );
        assert_eq!(
            file_glyph(Path::new("/repo/deploy.sh"), false),
            Icon::SquareTerminal
        );
        assert_eq!(
            file_glyph(Path::new("/repo/.bashrc"), false),
            Icon::SquareTerminal
        );
        assert_eq!(
            file_glyph(Path::new("/repo/.gitignore"), false),
            Icon::GitBranch
        );
        assert_eq!(
            file_glyph(Path::new("/repo/gitmodules"), false),
            Icon::GitBranch
        );
        assert_eq!(
            file_glyph(Path::new("/repo/.env.local"), false),
            Icon::Settings
        );
        assert_eq!(
            file_glyph(Path::new("/repo/.editorconfig"), false),
            Icon::Settings
        );
        assert_eq!(file_glyph(Path::new("/repo/Cargo.toml"), false), Icon::File);
        assert_eq!(
            file_glyph(Path::new("/repo/src"), true),
            Icon::FolderFill,
            "directories keep the folder mark"
        );
    }

    #[gpui::test]
    async fn expanding_a_folder_is_asynchronous(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let subdir = dir.0.join("subdir");
        seed_dir_with_files(&subdir, 2000);

        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));
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

        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));
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
    async fn an_unreadable_directory_renders_an_error(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let locked = dir.0.join("locked");
        std::fs::create_dir_all(&locked).expect("create dir");
        std::fs::write(locked.join("secret.txt"), "x").expect("write file");

        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));
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

    /// F-CHG-03: the same refresh guard that prevents a second refresh from
    /// starting must be visible as a drawn loading state, with a Refresh
    /// affordance available in the toolbar.
    #[gpui::test]
    async fn a_refresh_in_flight_draws_loading_and_refresh(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("visible.txt"), "x").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| {
            let mut panel = RightPanel::new(dir.0.clone());
            panel.refresh_started = true;
            panel
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("files-loading").is_some(),
            "an in-flight refresh reaches a loading pixel"
        );
        assert!(
            cx.debug_bounds("files-refresh").is_some(),
            "the Files toolbar keeps a Refresh action visible"
        );
    }

    /// F-CHG-03: a failed root refresh renders Retry and a subsequent click
    /// re-fetches the tree after the repository becomes readable again.
    #[gpui::test]
    async fn a_failed_refresh_draws_retry_and_recovers(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("visible.txt"), "x").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("window");
        std::fs::remove_dir_all(&dir.0).expect("break root refresh");
        cx.update(|app| panel.update(app, |panel, cx| panel.refresh(cx)));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| panel.refresh_error.is_some())
        });
        cx.cx.run_until_parked();

        assert!(
            cx.debug_bounds("files-error").is_some(),
            "a failed refresh is drawn as an error"
        );
        let retry = cx
            .debug_bounds("files-retry")
            .expect("Retry is drawn for a failed refresh");

        std::fs::create_dir_all(&dir.0).expect("restore root");
        std::fs::write(dir.0.join("visible.txt"), "x").expect("restore file");
        cx.simulate_click(retry.center(), Modifiers::none());
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel.refresh_error.is_none()
                    && find_node(&panel.file_tree, &dir.0.join("visible.txt")).is_some()
            })
        });
    }

    /// F-EDIT-09 is an interaction contract, not a pixel contract: the real
    /// file row is laid out, receives a double-click, and emits the path the
    /// shell uses to open the editor. The row's typography and icon remain a
    /// separate visual debt.
    #[gpui::test]
    async fn double_clicking_a_drawn_file_row_emits_open_file(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("opened.md"), "# opened\n").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel = cx.update(|window, _| {
            window
                .root::<RightPanel>()
                .flatten()
                .expect("right panel root")
        });
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&panel, move |_, event: &RightPanelEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
            panel.update(cx, |panel, cx| panel.refresh(cx));
        });

        // The tree walk runs on a real background thread; the hardened pump
        // gives it real time plus the virtual-clock ticks the panel's own
        // refresh timer needs, so the test cannot pass on timing luck.
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel
                    .file_tree
                    .iter()
                    .any(|node| node.path == dir.0.join("opened.md"))
            })
        });
        cx.cx.run_until_parked();
        cx.update(|_, app| panel.update(app, |_, cx| cx.notify()));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let row = cx
            .debug_bounds("file-row")
            .expect("the file row is present in the drawn frame");
        let position = row.center();
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted.iter().any(|event| matches!(
                event,
                RightPanelEvent::OpenFile(path) if *path == dir.0.join("opened.md")
            )),
            "the real double-click must emit the file path, got {emitted:?}"
        );
    }

    #[gpui::test]
    async fn right_clicking_a_file_row_draws_and_dispatches_file_actions(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let path = dir.0.join("opened.md");
        std::fs::write(&path, "# opened\n").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("window");
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|app| {
            app.subscribe(&panel, move |_, event: &RightPanelEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
            panel.update(app, |panel, cx| panel.refresh(cx));
        });

        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &path).is_some()
            })
        });
        cx.cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let row = cx.debug_bounds("file-row").expect("the file row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let menu = cx
            .debug_bounds("file-context-menu")
            .expect("the file context menu is drawn");
        assert!(
            (menu.origin.x.as_f32() - row.center().x.as_f32()).abs() <= 1.0,
            "the file context menu starts at the pointer's x coordinate: menu={:?}, pointer={:?}",
            menu.origin.x,
            row.center().x
        );
        assert!(
            (menu.origin.y.as_f32() - row.center().y.as_f32()).abs() <= 1.0,
            "the file context menu starts at the pointer's y coordinate: menu={:?}, pointer={:?}",
            menu.origin.y,
            row.center().y
        );
        for selector in [
            "file-context-open",
            "file-context-reveal",
            "file-context-copy-path",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "file context menu draws {selector}"
            );
        }

        let open = cx
            .debug_bounds("file-context-open")
            .expect("Open action is drawn");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                RightPanelEvent::OpenFile(opened) if opened == &path
            )),
            "Open dispatches the existing file-open event"
        );

        // The copy action is a second real menu trial, and the clipboard
        // value is the observable result rather than just a click counter.
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let row = cx
            .debug_bounds("file-row")
            .expect("the file row remains drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        let copy = cx
            .debug_bounds("file-context-copy-path")
            .expect("Copy Path action is drawn");
        cx.simulate_click(copy.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.cx.read_from_clipboard().and_then(|item| item.text()),
            Some(path.to_string_lossy().into_owned()),
            "Copy Path writes the absolute file path"
        );

        // Reveal is the Linux adaptation of Show in Finder. Exercise its
        // drawn action too; the command derivation has a headless assertion
        // in editor::tests::reveal_command_opens_the_containing_directory_with_xdg_open.
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        let row = cx
            .debug_bounds("file-row")
            .expect("the file row remains drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        let reveal = cx
            .debug_bounds("file-context-reveal")
            .expect("Reveal action is drawn");
        cx.simulate_click(reveal.center(), Modifiers::none());
        cx.run_until_parked();
    }

    /// F-CHG-05: once a Files row has focus and selection, Return follows the
    /// same open-file path as a double click.
    #[gpui::test]
    async fn return_opens_the_selected_file_row(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let path = dir.0.join("opened.md");
        std::fs::write(&path, "# opened\n").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("window");
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|app| {
            app.subscribe(&panel, move |_, event: &RightPanelEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &path).is_some()
            })
        });
        cx.cx.run_until_parked();
        let row = cx.debug_bounds("file-row").expect("the file row is drawn");
        cx.simulate_click(row.center(), Modifiers::none());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                RightPanelEvent::OpenFile(opened) if opened == &path
            )),
            "Return opens the selected file through the real event path"
        );
    }

    /// F-CHG-05: the Files focus path supports arrow selection and Space
    /// toggles the selected directory without needing a mouse click.
    #[gpui::test]
    async fn arrow_keys_select_and_space_expands_the_files_tree(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let folder = dir.0.join("src");
        std::fs::create_dir_all(&folder).expect("create folder");
        std::fs::write(folder.join("main.rs"), "fn main() {}\n").expect("write child");
        std::fs::write(dir.0.join("z.txt"), "z\n").expect("write sibling");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("window");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &folder).is_some()
            })
        });
        let focus = panel.read_with(&cx.cx, |panel, _| {
            panel
                .file_focus
                .clone()
                .expect("Files focus is initialized")
        });
        cx.update(|window, app| focus.focus(window, app));
        cx.simulate_keystrokes("down down up space");
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel.selected_path.as_deref() == Some(folder.as_path())
                    && find_node(&panel.file_tree, &folder)
                        .is_some_and(|node| node.expanded && node.children.len() == 1)
            })
        });
    }

    /// F-CHG-04 is the interaction half of the explorer contract: the drawn
    /// directory row toggles and its asynchronously loaded child becomes
    /// part of the next drawn frame. File selection visuals remain a
    /// separate appearance debt.
    #[gpui::test]
    async fn clicking_a_drawn_directory_row_expands_and_collapses_it(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let subdir = dir.0.join("src");
        std::fs::create_dir_all(&subdir).expect("create directory");
        std::fs::write(subdir.join("main.rs"), "fn main() {}\n").expect("write file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel = cx.update(|window, _| {
            window
                .root::<RightPanel>()
                .flatten()
                .expect("right panel root")
        });
        cx.update(|_, cx| panel.update(cx, |panel, cx| panel.refresh(cx)));
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &subdir).is_some()
            })
        });
        cx.update(|_, app| panel.update(app, |_, cx| cx.notify()));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });

        let folder = cx
            .debug_bounds("file-directory-row")
            .expect("the directory row is present in the drawn frame");
        cx.simulate_click(folder.center(), Modifiers::none());
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &subdir)
                    .is_some_and(|node| node.expanded && node.children.len() == 1)
            })
        });
        cx.update(|_, app| panel.update(app, |_, cx| cx.notify()));
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-row").is_some(),
            "the expanded directory's child is laid out in the drawn frame"
        );

        let folder = cx
            .debug_bounds("file-directory-row")
            .expect("the expanded directory row remains in the drawn frame");
        cx.simulate_click(folder.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            panel.read_with(&cx.cx, |panel, _| {
                find_node(&panel.file_tree, &subdir).is_some_and(|node| !node.expanded)
            }),
            "clicking the drawn directory row again collapses it"
        );
    }

    // ── F-EDIT-12 harness capability: payload drags ─────────────────────
    //
    // The inventory's drag entry needs a drop target in a *pane* — that
    // lives in the shell (`tiller/src/main.rs`), so the product half is
    // routed to codex12 (a file row becomes the drag source, a pane the
    // drop target). What this crate owns is the proof that the harness can
    // express a payload drag at all: a source element with `on_drag`, a
    // target with `on_drop`, and the real mouse-down / move / up sequence
    // between them. The shell's F-EDIT-12 test then uses exactly this
    // recipe against its workspace.

    /// The drag preview view GPUI requires from `on_drag`.
    struct EmptyDragPreview;

    impl Render for EmptyDragPreview {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct DragFixture {
        dropped: Rc<RefCell<Vec<PathBuf>>>,
    }

    impl DragFixture {
        fn new() -> Self {
            Self {
                dropped: Default::default(),
            }
        }
    }

    impl Render for DragFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let dropped = self.dropped.clone();
            div()
                .size_full()
                .flex()
                .child(
                    div()
                        .id("drag-source")
                        .w(px(100.0))
                        .h(px(100.0))
                        .debug_selector(|| "drag-source".into())
                        .on_drag(PathBuf::from("/repo/file.txt"), |_, _, _, cx| {
                            cx.new(|_| EmptyDragPreview)
                        }),
                )
                .child(
                    div()
                        .id("drag-target")
                        .w(px(100.0))
                        .h(px(100.0))
                        .debug_selector(|| "drag-target".into())
                        .on_drag_move(|_event: &gpui::DragMoveEvent<PathBuf>, _, _| {})
                        .on_drop(move |path: &PathBuf, _, _| {
                            dropped.borrow_mut().push(path.clone());
                        }),
                )
        }
    }

    #[gpui::test]
    async fn a_payload_drag_reaches_the_drop_target_through_real_mouse_events(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| DragFixture::new());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let fixture = cx.update(|window, _| {
            window
                .root::<DragFixture>()
                .flatten()
                .expect("fixture root")
        });

        let source = cx
            .debug_bounds("drag-source")
            .expect("the drag source is in the drawn frame");
        let target = cx
            .debug_bounds("drag-target")
            .expect("the drop target is in the drawn frame");

        // The real gesture: press on the source, move past the 2px drag
        // threshold (which starts the payload drag), move over the target,
        // release. No drag convenience method exists — this is the mouse
        // sequence GPUI itself uses, dispatched through the same window
        // event path as production input.
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: gpui::point(source.center().x + px(30.0), source.center().y),
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

        let dropped = fixture.read_with(&cx.cx, |fixture, _| fixture.dropped.borrow().clone());
        assert_eq!(
            dropped,
            vec![PathBuf::from("/repo/file.txt")],
            "the drop target receives the drag payload through the real event path"
        );
    }

    /// F-CHG-20 (empty half): with no activity rows, the expanded section
    /// states No activity instead of showing nothing, and no running count
    /// is offered.
    #[gpui::test]
    async fn activity_section_states_no_activity_when_empty(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx
            .add_window(|_window, _cx| RightPanel::with_activity(std::env::temp_dir(), Vec::new()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-empty").is_some(),
            "an expanded section with no rows states No activity (F-CHG-20)"
        );
        assert!(
            cx.debug_bounds("activity-running-count").is_none(),
            "nothing runs, so no running count is offered"
        );
    }

    /// F-CHG-20 (running-count half): with a running row in the list, the
    /// header states the count while rows render below it.
    #[gpui::test]
    async fn activity_section_states_the_running_count(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| {
            RightPanel::with_activity(
                std::env::temp_dir(),
                vec![
                    ActivitySurface::new(
                        Icon::MessageSquare,
                        "Chat",
                        "/repo",
                        ActivityStatus::Running,
                    ),
                    ActivitySurface::new(Icon::File, "Changes", "/repo", ActivityStatus::Done),
                ],
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The count is visible from the collapsed header alone.
        assert!(
            cx.debug_bounds("activity-running-count").is_some(),
            "the running count is stated beside the header"
        );

        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-0").is_some() && cx.debug_bounds("activity-1").is_some(),
            "the activity rows render under the expanded header"
        );
        assert!(
            cx.debug_bounds("activity-empty").is_none(),
            "rows exist, so the empty state is not shown"
        );
        assert!(
            cx.debug_bounds("activity-running-count").is_some(),
            "the running count stays visible with rows present"
        );
    }

    /// F-CHG-22: NeedsInput has its own drawn status marker and is not the
    /// same visual state as Idle.
    #[gpui::test]
    async fn activity_section_draws_needs_input_as_distinct_from_idle(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| {
            RightPanel::with_activity(
                std::env::temp_dir(),
                vec![
                    ActivitySurface::new(
                        Icon::MessageSquare,
                        "Waiting agent",
                        "/repo",
                        ActivityStatus::NeedsInput,
                    ),
                    ActivitySurface::new(Icon::File, "Idle surface", "/repo", ActivityStatus::Idle),
                ],
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("activity-status-needs-input-0").is_some(),
            "NeedsInput reaches a dedicated status marker in the drawn panel"
        );
        assert_ne!(
            activity_status_glyph(ActivityStatus::NeedsInput),
            activity_status_glyph(ActivityStatus::Idle),
            "NeedsInput is not rendered with Idle's glyph"
        );
    }

    /// F-CHG-13: a modified file row exposes an Open diff action and sends
    /// the exact repo-relative path to the shell boundary.
    #[gpui::test]
    async fn clicking_a_drawn_modified_file_open_diff_emits_its_path(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let file = dir.0.join("changed.md");
        std::fs::write(&file, "before\n").expect("seed file");
        git(&dir.0, &["init", "-q"]);
        git(&dir.0, &["config", "user.email", "tests@example.invalid"]);
        git(&dir.0, &["config", "user.name", "Tiller tests"]);
        git(&dir.0, &["add", "changed.md"]);
        git(
            &dir.0,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "seed"],
        );
        std::fs::write(&file, "after\n").expect("modify file");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("right panel entity");
        let events = Rc::new(RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|app| {
            app.subscribe(&panel, move |_, event: &RightPanelActionEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
            panel.update(app, |panel, cx| panel.refresh(cx));
        });

        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel.file_tree.iter().any(|node| node.path == file)
            })
        });
        cx.cx.run_until_parked();

        let diff = cx
            .debug_bounds("file-open-diff")
            .expect("modified file draws Open diff");
        cx.simulate_click(diff.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            &[RightPanelActionEvent::OpenDiff(PathBuf::from("changed.md"))],
            "Open diff crosses the panel seam with a repo-relative path"
        );
    }
}

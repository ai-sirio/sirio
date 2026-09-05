//! The Files surface of the right panel: the filesystem tree walk, its
//! git markers, and the file-row interactions (open, diff, context menu,
//! drag).

use super::*;
use bezel::motion::{Fade, Painter};
use bezel::ui::popover;
use gpui::{
    AnyElement, App, ClipboardItem, KeyDownEvent, MouseButton, Pixels, Point, Rgba, uniform_list,
};
use sirio_git::{DirectoryGitStatus, directory_statuses, status};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::editor::fs_actions;
use crate::loading;
use crate::sidebar::icons::file_glyph;

/// File-tree rows: 12.5px text at 26px, the app's single-line row rhythm.
pub(crate) const ROW_HEIGHT: f32 = 26.0;

/// The git state the Files tree paints, for both halves of the tree.
///
/// `files` holds one entry per changed path, resolved with
/// [`DirectoryGitStatus::for_file`] — the full four-way vocabulary
/// (conflicted > untracked > staged > modified) a single file needs, and
/// that the Changes list already uses. `directories` is
/// [`sirio_git::directory_statuses`]'s aggregate: every non-root ancestor
/// of every changed path, resolved with [`DirectoryGitStatus::for_entry`]'s
/// narrower conflicted > changed > untracked (staged collapses into
/// changed there, on purpose — a directory roll-up, matching the Swift
/// original), and with a rename contributing both its destination *and*
/// its original ancestors (F-GIT-STATUS-02). Both are keyed by
/// worktree-relative path. The two halves deliberately use two different
/// precedence orders on the same `DirectoryGitStatus` type — see F-CHG-06.
///
/// This replaced a `HashSet<PathBuf>` of changed paths plus a
/// `changed.starts_with(relative)` roll-up done in `read_tree`: that
/// produced a *boolean* for a directory, so a conflicted, a changed and an
/// untracked ancestor were byte-identical on screen, and it discarded
/// `StatusEntry::original_path` entirely, so the source directories of a
/// `git mv` were marked as nothing at all.
#[derive(Clone, Debug, Default)]
pub(super) struct GitMarkers {
    files: HashMap<PathBuf, DirectoryGitStatus>,
    directories: HashMap<PathBuf, DirectoryGitStatus>,
}

impl GitMarkers {
    /// The marker for one worktree-relative path, taken from the half of
    /// the model that owns it.
    fn get(&self, relative: &Path, is_dir: bool) -> Option<DirectoryGitStatus> {
        if is_dir {
            self.directories.get(relative).copied()
        } else {
            self.files.get(relative).copied()
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    /// The git marker this row carries: its own status for a file, the
    /// aggregate of everything beneath it for a directory. `None` is a
    /// clean row.
    git_status: Option<DirectoryGitStatus>,
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
pub(super) struct FileContextMenu {
    path: PathBuf,
    /// Directories get the same menu as files minus `Open`, which
    /// would otherwise emit an `OpenFile` for a path no editor opens.
    is_dir: bool,
    position: Point<Pixels>,
    painter: Painter,
}

impl RightPanel {
    /// Refreshes the changed-paths set and the directory tree off the
    /// render thread. Single-flight on the walk task.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_started || self.walk_task.is_some() {
            return;
        }
        if !self
            .allowed_roots
            .iter()
            .any(|root| normalize_path(root) == normalize_path(&self.repo_root))
        {
            self.settled = true;
            self.refresh_error = Some("worktree is outside the project roots".to_string());
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
                    let markers = status(&repo_root)
                        .map(|snapshot| GitMarkers {
                            files: snapshot
                                .entries
                                .iter()
                                .map(|entry| {
                                    (entry.path.clone(), DirectoryGitStatus::for_file(entry))
                                })
                                .collect(),
                            // F-GIT-STATUS-02: the tested aggregate, not a
                            // second implementation of it. It is what
                            // resolves conflicted > changed > untracked on a
                            // shared ancestor and what marks *both* sides of
                            // a rename.
                            directories: directory_statuses(&snapshot.entries),
                        })
                        .unwrap_or_default();
                    let tree = read_tree(&repo_root, &repo_root, &markers)
                        .map_err(|error| error.to_string())?;
                    Ok::<_, String>((markers, tree))
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.walk_task = None;
                panel.refresh_started = false;
                // The walk finished. Whichever way it went, the panel now
                // has something truthful to show — a tree, an empty tree, or
                // an error — and must never fall back to "Loading files…".
                panel.settled = true;
                match result {
                    Ok((markers, tree)) => {
                        panel.git_markers = markers;
                        // F-TAB-01: the periodic 1s refresh loop
                        // (`ensure_tree_refresh`) walks only the root
                        // listing and used to replace `file_tree` wholesale,
                        // so any folder a user had expanded (and its
                        // already-loaded children) silently collapsed back
                        // the instant the next tick landed — visible as a
                        // "Loading files…" flash followed by the tree
                        // reverting to fully collapsed. Preserve prior
                        // expansion/children for any node the fresh walk
                        // still reports at the same path.
                        panel.file_tree = preserve_expansion(&panel.file_tree, tree);
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
    pub(super) fn ensure_tree_refresh(&mut self, cx: &mut Context<Self>) {
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
                        // #189: no draw since the previous tick means
                        // nothing is showing this panel -- minimised,
                        // collapsed, whatever the reason -- so skip both
                        // repository-sized walks.
                        if panel.renders == panel.renders_at_last_tick
                            && panel.suspended_ticks < crate::changes::SUSPENDED_TICK_BUDGET
                        {
                            panel.suspended_ticks += 1;
                            panel.refresh_suspended = true;
                            return;
                        }
                        // #191: the panel can be drawn and still not be
                        // showing this tree. Measured with the panel on
                        // History, 12 of 17 git processes in twenty seconds
                        // were this walk's own
                        // `status --untracked-files=all` -- a command the
                        // commit graph has no use for.
                        if super::PanelView::get(cx) != super::PanelView::Files {
                            panel.refresh_suspended = true;
                            return;
                        }
                        panel.suspended_ticks = 0;
                        panel.renders_at_last_tick = panel.renders;
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
        let markers = self.git_markers.clone();
        // The walk consumes `path`; the update needs its own copy to find
        // the node again.
        let apply_path = path.clone();
        self.walk_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { read_tree(&repo_root, &path, &markers) })
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
        is_dir: bool,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.selected_path = Some(path.clone());
        self.file_context_menu = Some(FileContextMenu {
            path,
            is_dir,
            position,
            painter: Painter::of(cx),
        });
        cx.notify();
    }

    fn close_file_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.file_context_menu.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn render_file_context_menu(
        menu: FileContextMenu,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let path = menu.path;
        let is_dir = menu.is_dir;
        let position = menu.position;
        let painter = menu.painter;
        let bezel_theme = theme.to_bezel_theme();
        let mut view = popover::popover_card(&bezel_theme)
            .id("file-context-menu")
            .debug_selector(|| "file-context-menu".to_owned())
            .w(theme.spacing.menu_width);

        // `Open` is the one entry a directory must not offer: it emits
        // `OpenFile`, and a directory is not a document. Everything
        // else — reveal, copy, and the tree-wide Refresh — reads the
        // same for both kinds of row.
        let mut entries: Vec<(&'static str, &'static str)> = Vec::new();
        if !is_dir {
            entries.push(("Open", "file-context-open"));
        }
        entries.push(("Reveal in File Manager", "file-context-reveal"));
        entries.push(("Copy Path", "file-context-copy-path"));
        entries.push(("Refresh", "file-context-refresh"));

        for (label, selector) in entries {
            let action_entity = entity.clone();
            let action_path = path.clone();
            let mut row = popover::menu_row(
                &bezel_theme,
                false,
                Fade::new(painter, selector),
            )
                .id(selector)
                .debug_selector(move || selector.to_owned())
                .w_full()
                .min_h(theme.spacing.titlebar_control_frame.height)
                .text_color(bezel_theme.text);

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
                // The Files view's only Refresh affordance — the
                // toolbar that used to carry one is gone. It is a
                // manual nudge, not the sole route back to a current
                // tree: `ensure_tree_refresh` re-walks every second on
                // its own, which is also what covers the states with no
                // row to right-click.
                "file-context-refresh" => row.on_click(move |_, _, cx| {
                    action_entity.update(cx, |panel, cx| {
                        panel.refresh(cx);
                        panel.close_file_context_menu(cx);
                    });
                }),
                _ => row,
            };
            view = view.child(row.child(label));
        }

        popover::menu_at(
            "file-context-menu-layer",
            position,
            view.on_mouse_down_out(move |_, _, cx| {
                entity.update(cx, |panel, cx| panel.close_file_context_menu(cx));
            })
            .into_any_element(),
            None,
        )
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
        repo_root: PathBuf,
    ) -> impl IntoElement {
        let path = row.node.path.clone();
        let is_dir = row.node.is_dir;
        let name = row.node.name.clone();
        let git_status = row.node.git_status;
        let read_error = row.node.read_error.clone();
        // The marker's selector names both the row and the status it
        // resolved to, so a drawn test can assert that a directory holding a
        // conflict *and* a modification marks conflicted — the precedence is
        // otherwise unobservable from a frame.
        let marker_selector = git_status.map(|status| {
            let relative = path
                .strip_prefix(&repo_root)
                .unwrap_or(&path)
                .display()
                .to_string();
            format!("file-status-{}-{relative}", status.slug())
        });
        let disclosure = if is_dir {
            if row.node.expanded {
                Some(
                    IconElement::new(Icon::ChevronDown, IconSize::XSmall)
                        .text_color(theme.text_muted),
                )
            } else {
                Some(
                    IconElement::new(Icon::ChevronRight, IconSize::XSmall)
                        .text_color(theme.text_muted),
                )
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
        let glyph = file_row_glyph(&path, is_dir);
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
            .text_size(theme.typography.scaled(13.5))
            // Names are neutral text; the status dot carries the git state
            // (three distinguishable colours, not one "modified" amber),
            // and unreadable directories dim rather than shout.
            .text_color(if read_error.is_some() {
                theme.text_muted
            } else {
                theme.text
            })
            .when(selected, |this| this.bg(theme.element_active))
            .hover(|style| style.bg(theme.element_hover))
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
                    panel.open_file_context_menu(context_path.clone(), is_dir, event.position, cx);
                });
            })
            // F-CORE-FILE-03: give every non-directory row a real drag source
            // so a file can be dropped onto a terminal pane. The target side
            // already exists (`sirio_terminal`'s `on_drop::<PathBuf>` calls
            // straight into `sirio_project::terminal_file_drop`); this was
            // the only missing half. `path` is worktree-relative, so the
            // payload joins it against `repo_root` to hand the drop target an
            // absolute path.
            .when(!is_dir, |this| {
                let drag_path = repo_root.join(&path);
                this.on_drag(drag_path, |_, _, _, cx| cx.new(|_| gpui::Empty))
            })
            .child(
                div()
                    .debug_selector(move || {
                        if is_dir {
                            "file-directory-disclosure".to_owned()
                        } else {
                            "file-file-disclosure".to_owned()
                        }
                    })
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
            // icons" setting. Directory rows intentionally have no glyph or
            // icon slot so their names follow the disclosure control directly.
            .when_some(glyph, |this, glyph| {
                this.child(
                    div()
                        .debug_selector(|| "file-file-icon".to_owned())
                        .w(px(16.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            IconElement::new(glyph, IconSize::Medium).text_color(theme.text_muted),
                        ),
                )
            })
            .child(
                div()
                    .debug_selector(move || {
                        if is_dir {
                            "file-directory-name".to_owned()
                        } else {
                            "file-file-name".to_owned()
                        }
                    })
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            )
            .when(read_error.is_some(), |this| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.danger)
                        .child("⚠"),
                )
            })
            .when_some(marker_selector, |this, selector| {
                let status = git_status.expect("a marker selector implies a status");
                this.child(
                    div()
                        .debug_selector(move || selector.clone())
                        .w(px(5.0))
                        .h(px(5.0))
                        .rounded(px(3.0))
                        .bg(git_status_color(status, theme)),
                )
            })
            .when(git_status.is_some() && !is_dir, |this| {
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
        if event.keystroke.key == "escape" && self.file_context_menu.is_some() {
            self.close_file_context_menu(cx);
            return;
        }
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
            _ => {}
        }
    }
}

impl RightPanel {
    pub(super) fn render_files(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.file_rows();
        let row_entity = entity.clone();
        let file_focus = self
            .file_focus
            .as_ref()
            .expect("Files focus is initialized")
            .clone();
        let selected_path = self.selected_path.clone();
        let processor_focus = file_focus.clone();
        let repo_root = self.repo_root.clone();
        // The Files view has no toolbar of its own. The row it used to
        // hold showed the checkout path — which the sidebar already
        // names — and its bottom border stacked a second hairline
        // directly under the view rail's, reading as a doubled rule.
        // Refresh moved into the file/folder context menu; the error
        // branch below keeps its own Retry, which is what covers the
        // one state that has no row to right-click.
        // "Loading files…" is the *first-load* state, not the refresh state
        // — see the `settled` field. A refresh over a settled panel happens
        // in place: whatever the panel was truthfully showing stays on
        // screen until the new walk lands, which is the same rule
        // `preserve_expansion` above already applies to expansion state and
        // the same rule `ChangesTab::render_body` applies to its own 1 s
        // poll.
        let body = if !self.settled {
            div()
                .id("files-loading")
                .debug_selector(|| "files-loading".to_owned())
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
                    "files-loading-orb",
                    loading::GENERIC_ORB,
                    &theme,
                    window,
                    cx,
                ))
                .child("Loading files…")
                .child(loading::skeleton_rows(
                    "files-skeleton",
                    loading::SKELETON_ROWS,
                    &theme,
                    window,
                    cx,
                ))
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
                        .text_color(theme.danger)
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
                                repo_root.clone(),
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
            .child(body)
            .into_any_element()
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
        .text_color(theme.text)
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on_click(cx);
        })
        .child(label)
}

fn file_row_glyph(path: &Path, is_dir: bool) -> Option<Icon> {
    if is_dir {
        return None;
    }
    Some(file_glyph(path, false))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn path_is_within(root: &Path, path: &Path) -> bool {
    normalize_path(path).starts_with(normalize_path(root))
}

fn read_tree(root: &Path, directory: &Path, markers: &GitMarkers) -> Result<Vec<FileNode>, String> {
    if !path_is_within(root, directory) {
        return Err(format!(
            "refusing to read {} outside file-tree root {}",
            directory.display(),
            root.display()
        ));
    }
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
            let git_status = markers.get(&relative, is_dir);
            Some(FileNode {
                name: path.file_name()?.to_string_lossy().to_string(),
                path,
                is_dir,
                git_status,
                expanded: false,
                read_error: None,
                children: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    // F-CORE-FILE-01. This is the comparator the Files panel a user
    // actually sees goes through — `sirio_project::file::read_directory`
    // has its own copy of this call, but nothing in the UI reaches it, so
    // porting natural sort there alone left the panel sorting
    // `file10.txt` before `file2.txt`. A fresh critic caught exactly that:
    // "the fix exists, was never wired to the UI".
    nodes.sort_by(|left, right| {
        sirio_project::compare_file_tree_names(left.is_dir, &left.name, right.is_dir, &right.name)
    });
    Ok(nodes)
}

/// Carries a matching old node's `expanded` flag and already-loaded
/// `children` forward onto a freshly-walked node at the same path. The
/// periodic root-level refresh only re-lists the immediate directory, so it
/// has no fresher data for a subdirectory's own contents; without this, the
/// blind replacement in `refresh()` silently collapsed every expanded
/// folder on the next 1s tick.
fn preserve_expansion(old: &[FileNode], new: Vec<FileNode>) -> Vec<FileNode> {
    new.into_iter()
        .map(|mut node| {
            if let Some(old_node) = old
                .iter()
                .find(|candidate| candidate.path == node.path && candidate.is_dir == node.is_dir)
                && old_node.expanded
            {
                node.expanded = true;
                node.children = old_node.children.clone();
            }
            node
        })
        .collect()
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

/// The Files tree's marker hue. The four cases are the four the model
/// distinguishes (a directory row never actually carries `Staged` — see
/// [`DirectoryGitStatus::for_entry`] — but a file row can), and they must
/// stay four *different* colours: rendering them all as `git_modified` is
/// what made F-GIT-STATUS-02's precedence ordering unobservable in the
/// frame, and rendering `Staged` as `git_modified` is F-CHG-06.
/// F-CHG-06: delegates to [`crate::git_status_style`], the crate's single
/// status -> colour mapping, so this and the Changes list cannot drift apart
/// on precedence again. Kept as a named local so the call sites below read
/// unchanged.
fn git_status_color(status: DirectoryGitStatus, theme: Theme) -> Rgba {
    crate::git_status_style::status_color(status, theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
        TestAppContext, VisualTestContext,
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
                "sirio-right-panel-test-{}-{unique}",
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
        // The Files tree's per-type glyphs are driven by `FileIconKey`
        // (F-CORE-FILE-08), ported from the original's `FileIconKey.swift`.
        // The approved Zed subset intentionally maps fewer shapes than the
        // original icon theme, so most kinds share the generic file mark —
        // see `file_glyph`'s doc comment for which few don't.
        let cases: &[(&str, bool, Icon)] = &[
            ("/repo/src/main.rs", false, Icon::file_type("rust")),
            ("/repo/deploy.sh", false, Icon::SquareTerminal),
            (".gitignore", false, Icon::GitBranch),
            ("Dockerfile", false, Icon::file_type("docker")),
            ("Cargo.lock", false, Icon::file_type("lock")),
            ("release.zip", false, Icon::file_type("zip")),
            (".env", false, Icon::Settings),
            ("service.env", false, Icon::Settings),
            // Not in the original's tables — `pathExtension` treats a name
            // that starts with `.` and has no *other* `.` as extension-less,
            // and neither is an exact-name entry, so both fall back to the
            // generic file mark rather than a heuristic Terminal/Settings
            // guess. See `FileIconKey`'s own tests for the full table.
            (".bashrc", false, Icon::File),
            (".editorconfig", false, Icon::File),
            (".env.local", false, Icon::File),
            ("gitmodules", false, Icon::File), // no leading dot: not the exact-name key
            ("Cargo.toml", false, Icon::file_type("toml")),
        ];
        for (path, is_dir, expected) in cases {
            assert_eq!(
                file_glyph(Path::new(path), *is_dir),
                *expected,
                "file_glyph({path:?}, {is_dir})"
            );
        }
    }

    #[test]
    fn file_row_glyph_keeps_file_icons_but_omits_directory_icons() {
        assert_eq!(file_row_glyph(Path::new("/repo/src"), true), None);
        assert_eq!(
            file_row_glyph(Path::new("/repo/main.rs"), false),
            Some(Icon::file_type("rust"))
        );
    }

    #[test]
    fn file_glyph_resolves_directory_kinds_and_falls_back_to_the_default_folder() {
        let cases: &[(&str, Icon)] = &[
            ("/repo/src", Icon::FolderFill),
            ("/repo/.git", Icon::GitBranch),
            ("/repo/node_modules", Icon::FolderFill),
            ("/repo/random-name", Icon::FolderFill),
        ];
        for (path, expected) in cases {
            assert_eq!(
                file_glyph(Path::new(path), true),
                *expected,
                "file_glyph({path:?}, true)"
            );
        }
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
    async fn a_periodic_refresh_does_not_collapse_an_expanded_folder(cx: &mut TestAppContext) {
        // F-TAB-01: `ensure_tree_refresh` reruns `refresh()` on every tick.
        // Before the fix, that walk replaced `file_tree` wholesale, so a
        // folder the user had just expanded (and its already-loaded
        // children) silently reverted to collapsed the instant the next
        // periodic refresh landed.
        let dir = TempDir::new();
        let subdir = dir.0.join("subdir");
        seed_dir_with_files(&subdir, 5);

        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));
        panel.update(cx, |panel, cx| panel.refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &subdir).is_some()
            })
        });

        panel.update(cx, |panel, cx| panel.toggle_file(&subdir, cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| {
                find_node(&panel.file_tree, &subdir)
                    .map(|node| node.expanded && node.children.len() == 5)
                    .unwrap_or(false)
            })
        });

        // Simulate the periodic loop's next tick: a fresh top-level refresh
        // while the folder is still expanded.
        panel.update(cx, |panel, cx| panel.refresh(cx));
        pump_until(cx, || {
            panel.read_with(cx, |panel, _| !panel.refresh_started)
        });

        panel.read_with(cx, |panel, _| {
            let node = find_node(&panel.file_tree, &subdir).expect("subdir node");
            assert!(
                node.expanded,
                "a periodic refresh must not collapse a folder the user expanded"
            );
            assert_eq!(
                node.children.len(),
                5,
                "the previously-loaded children must survive a periodic refresh"
            );
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

    /// F-CHG-03: the same refresh guard that prevents a second refresh
    /// from starting must be visible as a drawn loading state.
    ///
    /// This also asserted a Refresh control in the Files toolbar. That
    /// toolbar is gone and Refresh lives in the file/folder context
    /// menu, which needs a row to right-click — and a panel on its
    /// first load has none. Nothing is left uncovered by dropping the
    /// assertion: what carries this window is `ensure_tree_refresh`'s
    /// 1 s loop, which keeps walking with no gesture at all.
    #[gpui::test]
    async fn a_refresh_in_flight_draws_the_loading_state(cx: &mut TestAppContext) {
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
        right_click(&mut cx, row.center());
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
        // Bezel's menu entrance begins 2px above its settled anchor and eases
        // into place; keep guarding the pointer anchor without rejecting that
        // deliberate first-frame transform.
        assert!(
            (menu.origin.y.as_f32() - row.center().y.as_f32()).abs() <= 2.0,
            "the file context menu starts at the pointer's y coordinate (within bezel's 2px entrance): menu={:?}, pointer={:?}",
            menu.origin.y,
            row.center().y
        );
        for selector in [
            "file-context-open",
            "file-context-reveal",
            "file-context-copy-path",
            "file-context-refresh",
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

    struct FileDropTargetFixture {
        panel: gpui::Entity<RightPanel>,
        received: Rc<RefCell<Option<PathBuf>>>,
    }

    impl gpui::Render for FileDropTargetFixture {
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
                .child(self.panel.clone())
                .child(
                    div()
                        .id("pane-test-drop-target")
                        .debug_selector(|| "pane-test-drop-target".to_owned())
                        .h(px(200.0))
                        .on_drop::<PathBuf>(move |path, _, _| {
                            *received.borrow_mut() = Some(path.clone());
                        }),
                )
        }
    }

    /// F-CORE-FILE-03: dragging a real, drawn `file-row` out of the Files
    /// panel and dropping it delivers the absolute path a terminal pane's
    /// `on_drop::<PathBuf>` target expects — the same mouse-down/move/up
    /// drag path `changes-file-row` already proves for diff payloads.
    #[gpui::test]
    async fn a_drawn_file_row_drags_its_absolute_path_to_a_drop_target(cx: &mut TestAppContext) {
        use gpui::point;

        let dir = TempDir::new();
        let path = dir.0.join("dragged.md");
        std::fs::write(&path, "# dragged\n").expect("write file");

        cx.update(Theme::init);
        let received = Rc::new(RefCell::new(None));
        let fixture_received = received.clone();
        let window = cx.add_window(|_window, cx| {
            let panel = cx.new(|_| RightPanel::new(dir.0.clone()));
            FileDropTargetFixture {
                panel,
                received: fixture_received,
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel = cx.update(|window, app| {
            window
                .root::<FileDropTargetFixture>()
                .flatten()
                .expect("fixture root")
                .read(app)
                .panel
                .clone()
        });
        cx.update(|_, app| panel.update(app, |panel, cx| panel.refresh(cx)));
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

        let source = cx
            .debug_bounds("file-row")
            .expect("the real file row is drawn");
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
            received.borrow().as_ref(),
            Some(&path),
            "the drop target receives the dragged row's absolute path"
        );
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

    /// F-CORE-FILE-01: the Files panel's own comparator, not
    /// `sirio_project::file::read_directory`'s. A critic pass found the
    /// natural-sort port landed in the latter while the panel a user sees
    /// goes through `read_tree` — so this test drives `read_tree` directly
    /// rather than the already-covered domain function, because that is
    /// where the defect was.
    #[test]
    fn the_files_tree_sorts_names_in_natural_numeric_order() {
        let dir = std::env::temp_dir().join(format!("sirio-files-natsort-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture");
        for name in ["file10.txt", "file9.txt", "file1.txt", "file2.txt"] {
            std::fs::write(dir.join(name), b"x").expect("write fixture file");
        }
        std::fs::create_dir_all(dir.join("zdir")).expect("create fixture dir");

        let nodes = read_tree(&dir, &dir, &GitMarkers::default()).expect("read the fixture tree");
        let order: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();

        assert_eq!(
            order,
            vec!["zdir", "file1.txt", "file2.txt", "file9.txt", "file10.txt"],
            "directories lead, then digit runs compare numerically — a plain \
             lowercase sort puts file10 before file2"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_files_tree_does_not_read_a_sibling_outside_its_root() {
        let fixture = std::env::temp_dir().join(format!(
            "sirio-files-roots-{}",
            std::process::id()
        ));
        let root = fixture.join("project");
        let sibling = fixture.join("sibling");
        let _ = std::fs::remove_dir_all(&fixture);
        std::fs::create_dir_all(&root).expect("create project root");
        std::fs::create_dir_all(&sibling).expect("create sibling");
        std::fs::write(sibling.join("private.txt"), b"must not be visited")
            .expect("write sibling fixture");

        let escaped_sibling = root.join("..").join("sibling");
        for directory in [&sibling, &escaped_sibling] {
            let error = read_tree(&root, directory, &GitMarkers::default())
                .expect_err("a tree walk must reject a directory outside its root");
            assert!(error.contains("outside"), "unexpected error: {error}");
        }
        let _ = std::fs::remove_dir_all(&fixture);
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
        let directory_disclosure = cx
            .debug_bounds("file-directory-disclosure")
            .expect("the directory disclosure slot is drawn");
        let directory_name = cx
            .debug_bounds("file-directory-name")
            .expect("the directory name is drawn");
        assert!(
            cx.debug_bounds("file-directory-icon").is_none(),
            "directory rows do not draw a file icon or reserve its slot"
        );

        let file_icon = cx
            .debug_bounds("file-file-icon")
            .expect("the file icon slot is drawn");
        let file_name = cx
            .debug_bounds("file-file-name")
            .expect("the file name is drawn");
        let gap = file_name.origin.x.as_f32()
            - (file_icon.origin.x.as_f32() + file_icon.size.width.as_f32());
        let expected_directory_name_x =
            directory_disclosure.origin.x.as_f32() + directory_disclosure.size.width.as_f32() + gap;
        assert!(
            (directory_name.origin.x.as_f32() - expected_directory_name_x).abs() <= 0.5,
            "directory name follows the disclosure slot directly: disclosure={directory_disclosure:?}, name={directory_name:?}, file_gap={gap}"
        );
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

    /// F-CHG-13: a modified file row exposes an Open diff action and sends
    /// the exact repo-relative path to the shell boundary.
    #[gpui::test]
    async fn clicking_a_drawn_modified_file_open_diff_emits_its_path(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let file = dir.0.join("changed.md");
        std::fs::write(&file, "before\n").expect("seed file");
        git(&dir.0, &["init", "-q"]);
        git(&dir.0, &["config", "user.email", "tests@example.invalid"]);
        git(&dir.0, &["config", "user.name", "Sirio tests"]);
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

    /// Runs git and returns whether it succeeded — for the one command in
    /// these fixtures that is *meant* to fail (`git merge` stopping on a
    /// conflict).
    fn git_may_fail(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git")
            .status
            .success()
    }

    /// A real repository carrying, at once: a real unresolved merge conflict
    /// beside a plain modification under the same directory, a modification
    /// beside an untracked file under another, an untracked-only directory,
    /// and a real `git mv` whose source directory still exists on disk.
    fn ancestor_status_fixture(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        git(dir, &["config", "commit.gpgSign", "false"]);
        for (relative, contents) in [
            ("src/app/deep/mod.rs", "mod deep;\n"),
            ("docs/guide/readme.md", "guide\n"),
            ("conflict/dir/file.txt", "base\n"),
            ("conflict/also_mod.txt", "base\n"),
            ("move/from/orig.txt", "moving\n"),
        ] {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().expect("nested fixture path"))
                .expect("create fixture dir");
            std::fs::write(&path, contents).expect("write fixture file");
        }
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "base tree"]);

        // A genuine conflict, produced by a genuine merge.
        git(dir, &["checkout", "-q", "-b", "left"]);
        std::fs::write(dir.join("conflict/dir/file.txt"), "left\n").expect("left edit");
        git(dir, &["commit", "-qam", "left"]);
        git(dir, &["checkout", "-q", "-"]);
        std::fs::write(dir.join("conflict/dir/file.txt"), "right\n").expect("right edit");
        git(dir, &["commit", "-qam", "right"]);
        assert!(
            !git_may_fail(dir, &["merge", "left"]),
            "the merge has to stop on the conflict for this fixture to mean anything"
        );

        std::fs::write(dir.join("src/app/deep/mod.rs"), "mod deep; // edited\n")
            .expect("edit nested file");
        std::fs::write(dir.join("src/zz_new.txt"), "untracked\n").expect("untracked sibling");
        std::fs::write(dir.join("docs/guide/notes.txt"), "untracked only\n")
            .expect("untracked-only dir");
        std::fs::write(
            dir.join("conflict/also_mod.txt"),
            "modified beside a conflict\n",
        )
        .expect("modification beside a conflict");
        std::fs::create_dir_all(dir.join("moved_dest")).expect("rename destination");
        git(dir, &["mv", "move/from/orig.txt", "moved_dest/renamed.txt"]);
    }

    /// F-GIT-STATUS-02, in the drawn frame: every ancestor of a changed
    /// path carries a marker, the marker resolves conflicted > changed >
    /// untracked, and a rename marks the directories it came *from* as well
    /// as the ones it went to.
    ///
    /// Each assertion is paired with its own negative: a directory that
    /// resolved to conflicted must draw no changed marker, and vice versa.
    /// Without that pair the test would pass on the old boolean roll-up,
    /// which drew one identical amber dot for all three states.
    #[gpui::test]
    async fn the_files_tree_marks_every_ancestor_with_conflict_over_change_over_untracked(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        ancestor_status_fixture(&dir.0);

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("right panel entity");
        cx.update(|app| panel.update(app, |panel, cx| panel.refresh(cx)));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel.file_tree.iter().any(|node| node.name == "moved_dest")
            })
        });
        cx.cx.run_until_parked();

        // Conflict beats the modification that shares its parent.
        assert!(
            cx.debug_bounds("file-status-conflicted-conflict").is_some(),
            "the directory holding the conflict is marked conflicted"
        );
        assert!(
            cx.debug_bounds("file-status-changed-conflict").is_none(),
            "…and not merely changed, though it also holds a plain modification"
        );
        // Changed beats the untracked file that shares its parent.
        assert!(
            cx.debug_bounds("file-status-changed-src").is_some(),
            "a directory holding a modification and an untracked file is changed"
        );
        assert!(
            cx.debug_bounds("file-status-untracked-src").is_none(),
            "…the untracked sibling cannot demote it"
        );
        // Untracked alone still marks its ancestors.
        assert!(
            cx.debug_bounds("file-status-untracked-docs").is_some(),
            "a directory whose only change is an untracked file is marked untracked"
        );
        assert!(
            cx.debug_bounds("file-status-changed-docs").is_none(),
            "…and is not indistinguishable from a modified one"
        );
        // Both sides of the rename.
        assert!(
            cx.debug_bounds("file-status-changed-moved_dest").is_some(),
            "the rename destination's directory is marked"
        );
        assert!(
            cx.debug_bounds("file-status-changed-move").is_some(),
            "the rename *source* directory is marked too — it was not, before"
        );
        // A root-level changed file is a file marker, not a directory one.
        assert!(
            cx.debug_bounds("file-status-changed-src/app").is_none(),
            "only the rows the root listing drew are on screen yet"
        );

        // Depth: expand the rename source and find its own child directory
        // marked. Clicking the marker clicks the row it sits in.
        let source = cx
            .debug_bounds("file-status-changed-move")
            .expect("the rename source directory is drawn");
        cx.simulate_click(source.center(), Modifiers::none());
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel
                    .file_rows()
                    .iter()
                    .any(|row| row.node.name == "from" && row.depth == 1)
            })
        });
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("file-status-changed-move/from").is_some(),
            "the rename source's own directory, two levels down, is marked as well"
        );

        // The clause says *every* ancestor, so walk one chain to its end
        // rather than stopping at the first level. `src/app/deep/mod.rs` is
        // three directories deep, and each of the three must carry the
        // marker its descendant earned. (`debug_bounds` takes a `'static`
        // selector, so these are spelled out rather than formatted.)
        for (marker, opened) in [
            ("file-status-changed-src", "src/app"),
            ("file-status-changed-src/app", "src/app/deep"),
        ] {
            let row = cx
                .debug_bounds(marker)
                .unwrap_or_else(|| panic!("{marker} is drawn"));
            cx.simulate_click(row.center(), Modifiers::none());
            pump_until(&cx.cx, || {
                panel.read_with(&cx.cx, |panel, _| {
                    let wanted = panel.repo_root.join(opened);
                    panel.file_rows().iter().any(|row| row.node.path == wanted)
                })
            });
            cx.cx.run_until_parked();
        }
        assert!(
            cx.debug_bounds("file-status-changed-src/app").is_some(),
            "src/app inherits the marker from the file two levels below it"
        );
        assert!(
            cx.debug_bounds("file-status-changed-src/app/deep")
                .is_some(),
            "…and so does src/app/deep, the directory that actually holds it"
        );

        // …and with `src` open, the untracked sibling that could not demote
        // it is on screen at the same time, drawn as its own untracked file
        // marker. Two different statuses, in one frame, in one subtree —
        // which the old boolean roll-up could not express at all.
        assert!(
            cx.debug_bounds("file-status-untracked-src/zz_new.txt")
                .is_some(),
            "the untracked file itself is marked untracked, beside its changed parent"
        );

        // The sharpest case: a conflicted directory whose own child file is
        // merely changed. Precedence made the parent red; the child stays
        // amber.
        let conflict = cx
            .debug_bounds("file-status-conflicted-conflict")
            .expect("the conflicted directory is drawn");
        cx.simulate_click(conflict.center(), Modifiers::none());
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel
                    .file_rows()
                    .iter()
                    .any(|row| row.node.name == "also_mod.txt")
            })
        });
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("file-status-conflicted-conflict/dir")
                .is_some(),
            "the directory that actually holds the unmerged file is conflicted too"
        );
        assert!(
            cx.debug_bounds("file-status-changed-conflict/also_mod.txt")
                .is_some(),
            "…while the plain modification beside it stays a changed file"
        );
        assert!(
            cx.debug_bounds("file-status-conflicted-conflict/also_mod.txt")
                .is_none(),
            "…and does not inherit its parent's conflict"
        );
    }

    /// A real repository carrying, at once: a purely staged add, a purely
    /// staged rename (zero unstaged component on either), a file that is
    /// staged *and* further modified in the worktree, a purely unstaged
    /// modification, and a genuinely untracked file. All five sit at the
    /// repo root so every marker renders on the very first frame, with no
    /// expansion needed.
    fn staged_marker_fixture(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "tests@example.invalid"]);
        git(dir, &["config", "user.name", "Sirio tests"]);
        git(dir, &["config", "commit.gpgSign", "false"]);
        for (relative, contents) in [
            ("modified-only.txt", "base\n"),
            ("staged-and-modified.txt", "base\n"),
            ("rename-source.txt", "rename me\n"),
        ] {
            std::fs::write(dir.join(relative), contents).expect("write fixture file");
        }
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "base tree"]);

        // Unstaged modification only — zero staged component.
        std::fs::write(dir.join("modified-only.txt"), "base\nedited\n")
            .expect("edit modified-only");

        // Staged, then edited again in the worktree: index and worktree
        // both carry a change on the same path at once.
        std::fs::write(dir.join("staged-and-modified.txt"), "base\nstaged\n")
            .expect("stage-then-edit: first edit");
        git(dir, &["add", "staged-and-modified.txt"]);
        std::fs::write(
            dir.join("staged-and-modified.txt"),
            "base\nstaged\nedited again\n",
        )
        .expect("stage-then-edit: second edit");

        // A pure staged rename — zero unstaged component.
        git(dir, &["mv", "rename-source.txt", "staged-rename.txt"]);

        // A pure staged add — brand new file, fully staged, zero unstaged
        // component.
        std::fs::write(dir.join("staged-add.txt"), "new + staged\n").expect("write staged-add");
        git(dir, &["add", "staged-add.txt"]);

        // A genuinely untracked file, for contrast.
        std::fs::write(dir.join("untracked-only.txt"), "untracked\n")
            .expect("write untracked-only");
    }

    /// F-CHG-06: the Files-tree marker must be able to show "staged" as
    /// distinct from "changed", exactly like the Changes list already can.
    /// Pixel-sampling on the live app found a staged rename, a staged add
    /// and a plain unstaged modification all rendering the identical amber
    /// dot — this pins the fix with the actual rendered marker, not just
    /// the pure classification function.
    ///
    /// The Swift original's contract (`GitStatusStyle.color`,
    /// `GitPanelTypes.swift`) resolves a file's own marker as conflicted >
    /// untracked > staged > modified — notably, staged wins even when the
    /// same file *also* carries unstaged changes. That combination is the
    /// one a two-state (or even a naively-ordered four-state) marker gets
    /// wrong invisibly: `staged-and-modified.txt` below must still read as
    /// staged, not modified.
    #[gpui::test]
    async fn the_files_tree_marks_a_staged_file_distinctly_from_a_merely_changed_one(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        staged_marker_fixture(&dir.0);

        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("right panel entity");
        cx.update(|app| panel.update(app, |panel, cx| panel.refresh(cx)));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || {
            panel.read_with(&cx.cx, |panel, _| {
                panel
                    .file_tree
                    .iter()
                    .any(|node| node.name == "untracked-only.txt")
            })
        });
        cx.cx.run_until_parked();

        // A pure staged add: zero unstaged component, must not fall back
        // to "changed" for lack of a staged colour.
        assert!(
            cx.debug_bounds("file-status-staged-staged-add.txt")
                .is_some(),
            "a fully-staged new file is marked staged"
        );
        assert!(
            cx.debug_bounds("file-status-changed-staged-add.txt")
                .is_none(),
            "…and not merely changed"
        );

        // A pure staged rename: same shape.
        assert!(
            cx.debug_bounds("file-status-staged-staged-rename.txt")
                .is_some(),
            "a staged rename's destination is marked staged"
        );
        assert!(
            cx.debug_bounds("file-status-changed-staged-rename.txt")
                .is_none(),
            "…and not merely changed"
        );

        // The awkward one: staged AND further modified in the worktree.
        // The Swift contract has staged win here too.
        assert!(
            cx.debug_bounds("file-status-staged-staged-and-modified.txt")
                .is_some(),
            "a file that is both staged and further modified still reads as staged"
        );
        assert!(
            cx.debug_bounds("file-status-changed-staged-and-modified.txt")
                .is_none(),
            "…the unstaged component must not steal the marker back to changed"
        );

        // A plain unstaged modification stays changed.
        assert!(
            cx.debug_bounds("file-status-changed-modified-only.txt")
                .is_some(),
            "a purely unstaged modification is still changed"
        );
        assert!(
            cx.debug_bounds("file-status-staged-modified-only.txt")
                .is_none(),
            "…it never reads as staged"
        );

        // Untracked stays untracked, distinct from staged.
        assert!(
            cx.debug_bounds("file-status-untracked-untracked-only.txt")
                .is_some(),
            "an untracked file keeps its own colour, distinct from staged"
        );
    }

    /// Builds a panel over `root`, drives one refresh, and hands back the
    /// entity plus a visual context — the setup every placeholder case below
    /// repeats.
    fn settled_panel(
        cx: &mut TestAppContext,
        root: PathBuf,
    ) -> (VisualTestContext, Entity<RightPanel>) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| RightPanel::new(root));
        let panel = cx
            .update_window(window.into(), |_, window, _| {
                window.root::<RightPanel>().flatten().expect("panel root")
            })
            .expect("right panel entity");
        let cx = VisualTestContext::from_window(window.into(), cx);
        pump_until(&cx.cx, || panel.read_with(&cx.cx, |panel, _| panel.settled));
        cx.cx.run_until_parked();
        (cx, panel)
    }

    /// The two events a real secondary click delivers, in order — gpui
    /// does not synthesize the up from the down.
    fn right_click(cx: &mut VisualTestContext, position: Point<Pixels>) {
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
    }

    /// A folder must open the same menu a file does: it is now the only
    /// place Refresh lives, and the old `!is_dir` gate left every folder
    /// row with no menu at all. `Open` is the one entry a directory must
    /// not carry — it emits `OpenFile` for a path no editor opens.
    #[gpui::test]
    async fn right_clicking_a_folder_offers_refresh_but_not_open(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::create_dir(dir.0.join("nested")).expect("create folder");

        let (mut cx, _panel) = settled_panel(cx, dir.0.clone());
        let row = cx
            .debug_bounds("file-directory-row")
            .expect("the folder row is drawn");
        right_click(&mut cx, row.center());
        cx.cx.run_until_parked();

        assert!(
            cx.debug_bounds("file-context-menu").is_some(),
            "a folder opens the context menu, not just a file"
        );
        assert!(
            cx.debug_bounds("file-context-open").is_none(),
            "a directory offers no Open — it would emit OpenFile for a \
             path no editor can open"
        );
        let refresh = cx
            .debug_bounds("file-context-refresh")
            .expect("the folder menu offers Refresh");

        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("file-context-menu").is_none(),
            "acting on the menu closes it"
        );
        assert!(
            cx.debug_bounds("file-directory-row").is_some(),
            "the tree survives the refresh the menu triggered"
        );
    }

    /// #189: `ensure_tree_refresh`'s 1 s tick runs two walks that both
    /// scale with the repository -- git's `--untracked-files=all` scan and
    /// `read_tree`'s recursive descent -- and nothing gated them on the
    /// panel being drawn at all. Measured against this repo on Windows,
    /// minimising the window changed nothing: 0.65 git processes a second
    /// while minimised, against 0.45 while visible. With the gate, a
    /// minimised window runs **zero** in twenty seconds and returns to
    /// 0.75/s the moment it is restored.
    ///
    /// The signal is a count of real draws, and deliberately not
    /// `Window::is_window_active()`: that was tried first and reads `true`
    /// for a minimised window, because `render` stops being called and the
    /// last polled value simply goes stale. Instrumenting the running app
    /// showed the draw count freezing (11, 11, 11, …) while the flag stayed
    /// `true` throughout.
    ///
    /// Both halves of the wiring are asserted here: that a draw is counted
    /// at all, and that a draw clears a suspension. A panel that never
    /// counted draws leaves the first at zero; a resume path that was never
    /// wired leaves the second suspended.
    #[gpui::test]
    async fn a_drawn_frame_is_counted_and_resumes_a_suspended_walk(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("visible.txt"), "x").expect("seed file");

        let (mut cx, panel) = settled_panel(cx, dir.0.clone());
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.renders) > 0,
            "render must count the frames this panel is drawn in -- that              count is the whole signal, and a panel that never incremented              it would suspend its walks forever"
        );

        panel.update(&mut cx.cx, |panel, _| {
            panel.refresh_suspended = true;
        });
        // Force a genuinely new frame: `debug_bounds` reports on the last
        // one drawn, it does not draw another.
        let before = panel.read_with(&cx.cx, |panel, _| panel.renders);
        panel.update(&mut cx.cx, |_, cx| cx.notify());
        cx.cx.run_until_parked();
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.renders) > before,
            "the harness really did draw another frame"
        );

        assert!(
            !panel.read_with(&cx.cx, |panel, _| panel.refresh_suspended),
            "a drawn frame must clear the suspension and refresh at once,              so a restored window never shows a tree frozen at the moment it              was hidden"
        );
    }

    /// #191: being drawn is not the same as being *shown*. #189 stopped the
    /// tree walk when nothing was drawing the panel at all; a panel showing
    /// History is still a panel being drawn, so the walk carried on behind
    /// the commit graph. Measured on the running app with the panel
    /// switched to History, 12 of the 17 git processes in twenty seconds
    /// were this walk's own `status --untracked-files=all` -- a command the
    /// commit graph has no use for. With the gate: zero.
    ///
    /// The resume clause in `render` is the half that is easy to get wrong,
    /// and it is what this asserts. `render` clears a suspension on any
    /// drawn frame; without the `PanelView::Files` condition on that clause
    /// the gate below would be undone on the very next frame, because the
    /// History panel keeps drawing.
    #[gpui::test]
    async fn a_panel_showing_another_view_does_not_resume_the_tree_walk(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("visible.txt"), "x").expect("seed file");

        let (mut cx, panel) = settled_panel(cx, dir.0.clone());

        cx.update(|_, app| PanelView::set(PanelView::History, app));
        panel.update(&mut cx.cx, |panel, _| {
            panel.refresh_suspended = true;
        });
        let before = panel.read_with(&cx.cx, |panel, _| panel.renders);
        panel.update(&mut cx.cx, |_, cx| cx.notify());
        cx.cx.run_until_parked();
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.renders) > before,
            "the harness really did draw another frame -- the assertion              below is about a drawn frame declining to resume, not about no              frame happening"
        );
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.refresh_suspended),
            "a frame drawn while the panel shows History must leave the walk              suspended"
        );

        // And switching back to Files does resume it, on the next frame.
        cx.update(|_, app| PanelView::set(PanelView::Files, app));
        panel.update(&mut cx.cx, |_, cx| cx.notify());
        cx.cx.run_until_parked();
        assert!(
            !panel.read_with(&cx.cx, |panel, _| panel.refresh_suspended),
            "returning to Files must resume the walk at once, or the tree              stays frozen at whatever it looked like when the user left it"
        );
    }

    /// The Files panel spent most of its time showing "Loading files…": the
    /// 1 s `ensure_tree_refresh` tick calls `refresh()`, which sets
    /// `refresh_started`, and the body swapped the whole tree for a
    /// full-panel placeholder on that flag alone.
    ///
    /// Two halves, and this test proves both. The *gesture* half: the drawn
    /// Refresh control really reaches `refresh()`, and the rows survive it.
    /// The *frame* half: with a walk genuinely in flight over a panel that
    /// has already settled, the drawn frame is the tree — while a panel that
    /// has never finished a walk still draws the first-load placeholder, so
    /// the fix narrowed the state rather than deleting it.
    #[gpui::test]
    async fn a_refresh_over_a_loaded_tree_keeps_the_rows_instead_of_flashing_loading(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new();
        std::fs::write(dir.0.join("visible.txt"), "x").expect("seed file");

        let (mut cx, panel) = settled_panel(cx, dir.0.clone());
        assert!(
            cx.debug_bounds("file-row").is_some(),
            "the tree is on screen before anything is refreshed"
        );

        // Gesture half: the drawn control reaches the refresh path.
        // Refresh now lives in the row context menu, so the gesture is
        // the real two-step one a user performs — right-click a row,
        // then click the entry.
        let row = cx
            .debug_bounds("file-row")
            .expect("a row to open the context menu on");
        right_click(&mut cx, row.center());
        cx.cx.run_until_parked();
        let refresh = cx
            .debug_bounds("file-context-refresh")
            .expect("the context menu offers Refresh");
        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("file-row").is_some(),
            "the rows survive a refresh triggered by the real control"
        );

        // Frame half, in two steps, because a frame can only be asserted on
        // once it has actually been drawn. First: `refresh()` is genuinely
        // what raises the in-flight flag.
        let in_flight = cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.refresh(cx);
                panel.refresh_started
            })
        });
        assert!(
            in_flight,
            "refresh() marks the walk in flight — the state the frame below holds"
        );
        cx.cx.run_until_parked();

        // Second: hold exactly that flag and force the repaint. Nothing is
        // walking now, so the flag survives the draw and the frame is the
        // one a user saw once a second — which must be the tree, not the
        // placeholder.
        hold_in_flight(&mut cx, &panel);
        assert!(
            cx.debug_bounds("files-loading").is_none(),
            "a refresh over an existing tree must not replace it with a placeholder"
        );
        assert!(
            cx.debug_bounds("file-row").is_some(),
            "the last good tree stays drawn while the refresh runs"
        );

        // The first-load placeholder still exists — the fix narrowed the
        // state, it did not delete it.
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.settled = false;
                cx.notify();
            })
        });
        cx.cx.run_until_parked();
        assert!(
            cx.debug_bounds("files-loading").is_some(),
            "the very first load, with nothing to show yet, still says so"
        );
    }

    /// Puts the panel in the state the 1 s tick puts it in — a walk in
    /// flight — and forces the repaint, without starting a walk that would
    /// clear the flag again before the frame could be asserted on.
    fn hold_in_flight(cx: &mut VisualTestContext, panel: &Entity<RightPanel>) {
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.refresh_started = true;
                cx.notify();
            })
        });
        cx.cx.run_until_parked();
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.refresh_started),
            "the drawn frame really was drawn with a walk in flight"
        );
    }

    /// The two cases a `file_tree.is_empty()` test of the loading state gets
    /// wrong, and the reason the flag is "has a walk ever finished" instead.
    ///
    /// A genuinely empty directory has an empty tree forever, so it would
    /// claim to be loading forever; and a root that cannot be read has an
    /// empty tree *and* an error to show, so the 1 s tick would cover its
    /// Retry button with the placeholder for the whole in-flight window —
    /// hiding the one control that recovers the panel.
    #[gpui::test]
    async fn an_empty_directory_and_an_unreadable_root_never_claim_to_be_loading(
        cx: &mut TestAppContext,
    ) {
        let empty = TempDir::new();
        let (mut cx, panel) = settled_panel(cx, empty.0.clone());
        assert!(
            panel.read_with(&cx.cx, |panel, _| panel.file_tree.is_empty()),
            "the fixture directory really is empty"
        );
        assert!(
            cx.debug_bounds("files-loading").is_none(),
            "an empty directory has finished loading — it is empty, not pending"
        );
        // The next 1 s tick, held over the drawn frame, must not say
        // otherwise. Under the old `refresh_started && file_tree.is_empty()`
        // rule this frame was the placeholder, forever, once a second.
        hold_in_flight(&mut cx, &panel);
        assert!(
            cx.debug_bounds("files-loading").is_none(),
            "…and the next 1 s tick over it must not say otherwise either"
        );

        let broken = TempDir::new();
        let root = broken.0.clone();
        std::fs::remove_dir_all(&root).expect("break the root before the first walk");
        let (mut cx, panel) = settled_panel(&mut cx.cx, root);
        assert!(
            cx.debug_bounds("files-error").is_some(),
            "an unreadable root draws its error"
        );
        let retry = cx
            .debug_bounds("files-retry")
            .expect("…and the Retry that recovers it");
        // The 1 s retry tick fires while the user is reaching for Retry.
        hold_in_flight(&mut cx, &panel);
        assert!(
            cx.debug_bounds("files-loading").is_none(),
            "the retry loop must not paint over the error it is retrying"
        );
        assert_eq!(
            cx.debug_bounds("files-retry").map(|bounds| bounds.origin),
            Some(retry.origin),
            "Retry stays exactly where the user was about to click it"
        );
    }
}

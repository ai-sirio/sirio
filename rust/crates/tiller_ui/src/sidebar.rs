//! The project/worktree/tab sidebar.
//!
//! This first Rust implementation deliberately owns a small fixture model. The
//! real project store will be connected by the integrator; keeping the view
//! model flat here makes filtering and variable-height rendering deterministic.
//!
//! Worktree creation and removal are real: the "+ New Worktree..." row opens
//! a branch-name prompt, creates the worktree with `tiller_git` on the
//! background executor (never the render thread), and inserts the new row
//! without a refresh; a hover "×" on a worktree row removes it. The fixture
//! project "tiller" is backed by the repository given in
//! `TILLER_SIDEBAR_REPO`; projects without a repository path are not offered
//! a New Worktree row, exactly like non-git projects.

use std::path::PathBuf;

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, PathPromptOptions, PromptLevel, Render, Rgba, Window, div, prelude::*, px, rgb,
};
use tiller_git::{
    create_worktree, derive_worktree_path, remove_worktree, resolve_parent_directory,
};
use tiller_project::TabKind;
use tiller_theme::Theme;

use crate::tab_bar::NewTabAction;

#[path = "icons.rs"]
pub mod icons;

use self::icons::{Icon, IconElement};
use crate::right_panel::ActivityStatus;

/// `SidebarRow::id` for a tab row built from real, host-owned tab data is
/// this offset plus the tab's own id. Real tab ids and the fixture/catalog's
/// hand- and index-assigned ids both start low, so without an offset a tab
/// row could collide with — and be toggled by clicking — an unrelated row.
/// The offset is far past anything a session will reach.
const TAB_ROW_ID_OFFSET: usize = 1_000_000;

/// One open tab, as the host (`main.rs`) knows it. This is the same fact the
/// tab bar and the Activity panel already render — the sidebar's tab rows
/// are a third view of it, not a second copy.
pub struct SidebarTab {
    /// The tab's real id (`OpenTab::id`).
    pub id: usize,
    /// The tab's title, e.g. "Chat" or "Terminal".
    pub title: String,
    /// Whether this is the workspace's active tab.
    pub selected: bool,
    /// The tab's semantic kind. Icons are derived from this value rather
    /// than guessed from the user-visible title.
    pub kind: TabKind,
    /// The running agent's brand mark, when this tab belongs to an agent.
    /// A plain terminal or chat keeps the surface icon instead.
    pub agent_icon: Option<Icon>,
}

const SIDEBAR_WIDTH: f32 = 325.0;
const FILTER_LEFT_INSET: f32 = 20.0;
const ROW_LEFT_INSET: f32 = 27.0;
const ROW_RIGHT_INSET: f32 = 7.0;
const TAB_INDENT: f32 = 35.0;
pub(crate) const ROW_HEIGHT: f32 = 32.0;
/// Single-line row title line height (13.5px at waku's row ratio).
pub(crate) const ROW_TITLE_LINE_HEIGHT: f32 = 18.0;
/// Two-line card context line height (11.5px).
pub(crate) const ROW_SUB_LINE_HEIGHT: f32 = 15.0;
/// Vertical padding of a row (waku's `py(7)`).
pub(crate) const ROW_V_PADDING: f32 = 7.0;
/// Gap between a card's title and context lines.
pub(crate) const ROW_GAP: f32 = 4.0;
/// Two-line card height: 7 + 18 + 4 + 15 + 7 — waku's session-card math.
pub(crate) const CARD_TWO_LINE_HEIGHT: f32 = 51.0;
const GUIDE_LEFT: f32 = 20.0;
const GUIDE_WIDTH: f32 = 2.0;
const PROJECT_TITLE_CHARS_PER_LINE: usize = 30;

// TODO(theme): tree_guide is translucent white and does not match the measured opaque guide.
const INDENT_GUIDE_FILL: Rgba = Rgba {
    r: 40.0 / 255.0,
    g: 40.0 / 255.0,
    b: 43.0 / 255.0,
    a: 1.0,
};

/// A visible row in the flattened sidebar tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    /// Stable fixture identity used by click handlers.
    pub id: usize,
    /// The row's semantic kind.
    pub kind: RowKind,
    /// Tree depth, where projects are depth zero.
    pub depth: usize,
    /// Display title.
    pub title: String,
    /// Whether this row is selected.
    pub selected: bool,
    /// Whether this project row is expanded.
    pub expanded: bool,
    /// The application-level primary marker for a worktree row.
    pub is_primary: bool,
    /// The worktree's live agent status, driving the small status dot next
    /// to its glyph. Only meaningful for `RowKind::Worktree`; host-pushed,
    /// the same way `RightPanel::set_activity` is — this crate never
    /// resolves status itself.
    pub agent_status: Option<ActivityStatus>,
    /// Whether this project row is a git repository. Only meaningful for
    /// `RowKind::Project`; the New Worktree row is offered only for git
    /// projects with a repository path.
    pub is_git: bool,
    /// The repository root for a project row, or the checkout path for a
    /// worktree row — the path git operations run against.
    pub path: Option<PathBuf>,
    /// The real `OpenTab::id` this row was built from, for a `RowKind::Tab`
    /// row sourced from [`Sidebar::set_worktree_tabs`]. `None` for every
    /// other row, including the decorative fixture rows: those keep the old
    /// local-selection click behaviour, this doesn't.
    pub tab_id: Option<usize>,
    /// The semantic kind for a real tab row. `None` for non-tab rows and
    /// legacy fixture rows that do not represent a host-owned tab.
    pub tab_kind: Option<TabKind>,
    /// The running agent's brand mark for a real tab row, if any.
    pub agent_icon: Option<Icon>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarProject {
    pub id: String,
    pub name: String,
    /// Whether the project is a git repository — a non-git project has no
    /// worktrees and is not offered the New Worktree row.
    pub is_git: bool,
    /// The project's root on disk, needed to create a worktree from its row.
    pub root_path: PathBuf,
    pub worktrees: Vec<SidebarWorktree>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarWorktree {
    pub branch: String,
    pub path: PathBuf,
    pub is_primary: bool,
}

/// Stable target carried by a sidebar context action. Paths and catalog ids
/// are the host's source of truth; row numbers are only drawing identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarContextTarget {
    Project {
        id: String,
        path: PathBuf,
        is_git: bool,
    },
    Worktree {
        path: PathBuf,
        is_primary: bool,
    },
}

/// The transitions exposed by a project/worktree context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarContextAction {
    ProjectSettings,
    InitializeGit,
    RevealInFileManager,
    RemoveProject,
    SetPrimary,
    UnsetPrimary,
    NewTab(NewTabAction),
}

/// Typed explanation for an unavailable context-menu command. The reason is
/// rendered beside the disabled item instead of leaving a grey mystery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarDisabledReason {
    AlreadyGitProject,
}

impl std::fmt::Display for SidebarDisabledReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyGitProject => formatter.write_str("Git is already initialized"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarContextItem {
    pub label: &'static str,
    pub action: SidebarContextAction,
    pub enabled: bool,
    pub disabled_reason: Option<SidebarDisabledReason>,
}

#[derive(Clone)]
struct OpenContextMenu {
    target: SidebarContextTarget,
}

#[derive(Clone)]
struct ProjectSettingsCard {
    id: String,
    name: String,
    path: PathBuf,
    is_git: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    AddProject(PathBuf),
    RemoveProject(String),
    /// Select the open tab with this id (an `OpenTab::id`, not a row id).
    SelectTab(usize),
    /// The user clicked a worktree row. The host owns which worktree is
    /// selected — the sidebar only reports the click, carrying the
    /// worktree's checkout path — and answers with
    /// [`Sidebar::set_selected_worktree`] so the highlight follows the
    /// host's decision, not the other way around.
    SelectWorktree(PathBuf),
    /// Close the open tab with this id.
    CloseTab(usize),
    /// Open the project settings sheet for a catalog project.
    OpenProjectSettings(String),
    /// A typed project/worktree context-menu transition for the shell.
    ContextAction {
        target: SidebarContextTarget,
        action: SidebarContextAction,
    },
}

/// The kinds of rows rendered by [`Sidebar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A top-level project.
    Project,
    /// A git worktree or plain folder.
    Worktree,
    /// An agent or terminal tab.
    Tab,
    /// The action row below an expanded project.
    NewWorktree,
}

/// The open branch-name prompt for creating a worktree.
#[derive(Clone)]
struct WorktreePrompt {
    /// The project row the new worktree belongs to.
    project_row_id: usize,
    /// The project's display name, used in the derived checkout folder.
    project_name: String,
    /// The repository root to create the worktree in.
    repo_root: PathBuf,
    /// The branch name being typed.
    draft: String,
    /// A creation error to show under the field, if the last attempt failed.
    error: Option<String>,
    /// Focus for the prompt's text field.
    focus: FocusHandle,
}

/// A fixture-backed project sidebar.
pub struct Sidebar {
    rows: Vec<SidebarRow>,
    project_ids: std::collections::HashMap<usize, String>,
    filter: String,
    filter_focus: FocusHandle,
    /// The open worktree-creation prompt, if any.
    prompt: Option<WorktreePrompt>,
    /// A transient error message (failed creation/removal) shown at the
    /// bottom of the sidebar.
    notice: Option<String>,
    context_menu: Option<OpenContextMenu>,
    project_settings: Option<ProjectSettingsCard>,
}

impl Sidebar {
    /// Creates the expanded fixture shown by the reference sidebar capture.
    pub fn new_for_demo(cx: &mut Context<Self>) -> Self {
        // The fixture's "tiller" project is backed by the repository the app
        // runs from when provided; without one, no New Worktree row is
        // offered (a project with no repository path behaves like a non-git
        // project).
        let tiller_repo = std::env::var_os("TILLER_SIDEBAR_REPO").map(PathBuf::from);
        Self::new_with_repo(cx, tiller_repo)
    }

    /// The fixture with an explicit repository path for the "tiller"
    /// project — the test entry point (tests pass their scratch repo
    /// directly instead of racing a process-global env var).
    fn new_with_repo(cx: &mut Context<Self>, tiller_repo: Option<PathBuf>) -> Self {
        fn row(
            id: usize,
            kind: RowKind,
            depth: usize,
            title: &str,
            selected: bool,
            expanded: bool,
            path: Option<PathBuf>,
        ) -> SidebarRow {
            SidebarRow {
                id,
                kind,
                depth,
                title: title.to_string(),
                selected,
                expanded,
                is_primary: false,
                agent_status: None,
                is_git: true,
                path,
                tab_id: None,
                tab_kind: None,
                agent_icon: None,
            }
        }

        let worktree_path = tiller_repo.clone();
        let mut rows = vec![
            row(0, RowKind::Project, 0, "tiller", true, true, tiller_repo),
            row(1, RowKind::Worktree, 1, "main", true, false, worktree_path),
            row(2, RowKind::Tab, 2, "Chat", true, false, None),
            row(
                3,
                RowKind::NewWorktree,
                1,
                "New Worktree...",
                false,
                false,
                None,
            ),
            row(
                4,
                RowKind::Project,
                0,
                "cricchetto-firma-e-digitalizzazione-bff-app",
                false,
                false,
                None,
            ),
            row(5, RowKind::Worktree, 1, "main", false, false, None),
            row(6, RowKind::Tab, 2, "Terminal", false, false, None),
            row(
                7,
                RowKind::Project,
                0,
                "Project-Tracker",
                false,
                false,
                None,
            ),
            row(8, RowKind::Project, 0, "source", false, false, None),
        ];
        rows[2].tab_kind = Some(TabKind::AgentChat);
        rows[6].tab_kind = Some(TabKind::Terminal);

        Self {
            rows,
            project_ids: std::collections::HashMap::new(),
            filter: String::new(),
            filter_focus: cx.focus_handle().tab_stop(true),
            prompt: None,
            notice: None,
            context_menu: None,
            project_settings: None,
        }
    }

    pub fn from_projects(projects: Vec<SidebarProject>, cx: &mut Context<Self>) -> Self {
        let mut rows = Vec::new();
        let mut project_ids = std::collections::HashMap::new();
        for (project_index, project) in projects.into_iter().enumerate() {
            let project_row_id = project_index * 1000;
            project_ids.insert(project_row_id, project.id);
            let project_is_git = project.is_git;
            let project_path = project.root_path.clone();
            rows.push(SidebarRow {
                id: project_row_id,
                kind: RowKind::Project,
                depth: 0,
                title: project.name,
                // Only the first worktree of the first project starts
                // selected: the selected row is one fact, and a project row
                // is a container, not a selection.
                selected: false,
                expanded: true,
                is_primary: false,
                agent_status: None,
                is_git: project_is_git,
                path: Some(project_path),
                tab_id: None,
                tab_kind: None,
                agent_icon: None,
            });
            let worktree_count = project.worktrees.len();
            for (worktree_index, worktree) in project.worktrees.into_iter().enumerate() {
                rows.push(SidebarRow {
                    id: project_row_id + worktree_index + 1,
                    kind: RowKind::Worktree,
                    depth: 1,
                    title: worktree.branch,
                    selected: project_index == 0 && worktree_index == 0,
                    expanded: false,
                    is_primary: worktree.is_primary,
                    agent_status: None,
                    // Worktree rows inherit the project's repo-ness; the
                    // worktree actions hang off the project row.
                    is_git: project_is_git,
                    path: Some(worktree.path),
                    tab_id: None,
                    tab_kind: None,
                    agent_icon: None,
                });
            }
            if project_is_git {
                rows.push(SidebarRow {
                    id: project_row_id + worktree_count + 1,
                    kind: RowKind::NewWorktree,
                    depth: 1,
                    title: "New Worktree...".to_string(),
                    selected: false,
                    expanded: false,
                    is_primary: false,
                    agent_status: None,
                    is_git: true,
                    path: None,
                    tab_id: None,
                    tab_kind: None,
                    agent_icon: None,
                });
            }
        }
        Self {
            rows,
            project_ids,
            filter: String::new(),
            filter_focus: cx.focus_handle().tab_stop(true),
            prompt: None,
            notice: None,
            context_menu: None,
            project_settings: None,
        }
    }

    pub fn set_projects(&mut self, projects: Vec<SidebarProject>, cx: &mut Context<Self>) {
        let filter = std::mem::take(&mut self.filter);
        let replacement = Self::from_projects(projects, cx);
        self.rows = replacement.rows;
        self.project_ids = replacement.project_ids;
        self.filter = filter;
        cx.notify();
    }

    /// Returns the complete context menu contract for a project or worktree.
    /// Disabled rows stay visible with their typed reason so the user can
    /// distinguish an unavailable transition from a missing affordance.
    pub fn context_menu_items(target: &SidebarContextTarget) -> Vec<SidebarContextItem> {
        let item = |label, action, enabled, disabled_reason| SidebarContextItem {
            label,
            action,
            enabled,
            disabled_reason,
        };
        match target {
            SidebarContextTarget::Project { is_git, .. } => vec![
                item(
                    "Project Settings",
                    SidebarContextAction::ProjectSettings,
                    true,
                    None,
                ),
                item(
                    "Initialize Git repository",
                    SidebarContextAction::InitializeGit,
                    !is_git,
                    is_git.then_some(SidebarDisabledReason::AlreadyGitProject),
                ),
                item(
                    "Show in File Manager",
                    SidebarContextAction::RevealInFileManager,
                    true,
                    None,
                ),
                item(
                    "Remove Project",
                    SidebarContextAction::RemoveProject,
                    true,
                    None,
                ),
            ],
            SidebarContextTarget::Worktree { is_primary, .. } => {
                let mut items = vec![item(
                    if *is_primary {
                        "Unset Primary"
                    } else {
                        "Set Primary"
                    },
                    if *is_primary {
                        SidebarContextAction::UnsetPrimary
                    } else {
                        SidebarContextAction::SetPrimary
                    },
                    true,
                    None,
                )];
                items.extend([
                    item(
                        "New Terminal",
                        SidebarContextAction::NewTab(NewTabAction::NewTerminal),
                        true,
                        None,
                    ),
                    item(
                        "Claude Code",
                        SidebarContextAction::NewTab(NewTabAction::ClaudeCode),
                        true,
                        None,
                    ),
                    item(
                        "Codex",
                        SidebarContextAction::NewTab(NewTabAction::Codex),
                        true,
                        None,
                    ),
                    item(
                        "OpenCode",
                        SidebarContextAction::NewTab(NewTabAction::OpenCode),
                        true,
                        None,
                    ),
                    item(
                        "Pi",
                        SidebarContextAction::NewTab(NewTabAction::Pi),
                        true,
                        None,
                    ),
                    item(
                        "Oh-My-Pi",
                        SidebarContextAction::NewTab(NewTabAction::OhMyPi),
                        true,
                        None,
                    ),
                    item(
                        "New Chat",
                        SidebarContextAction::NewTab(NewTabAction::NewChat),
                        true,
                        None,
                    ),
                ]);
                items
            }
        }
    }

    fn context_target(&self, row_id: usize) -> Option<SidebarContextTarget> {
        let row = self.rows.iter().find(|row| row.id == row_id)?;
        match row.kind {
            RowKind::Project => Some(SidebarContextTarget::Project {
                id: self.project_ids.get(&row_id)?.clone(),
                path: row.path.clone()?,
                is_git: row.is_git,
            }),
            RowKind::Worktree => Some(SidebarContextTarget::Worktree {
                path: row.path.clone()?,
                is_primary: row.is_primary,
            }),
            RowKind::Tab | RowKind::NewWorktree => None,
        }
    }

    fn open_context_menu(&mut self, row_id: usize, cx: &mut Context<Self>) {
        if let Some(target) = self.context_target(row_id) {
            self.context_menu = Some(OpenContextMenu { target });
            self.project_settings = None;
            cx.notify();
        }
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Host entry point for the gear affordance. The sheet is intentionally
    /// read-only until project-setting mutations have a persistence contract.
    pub fn open_project_settings(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(row_id) = self
            .project_ids
            .iter()
            .find_map(|(row_id, id)| (id == project_id).then_some(*row_id))
        else {
            return;
        };
        let Some(row) = self.rows.iter().find(|row| row.id == row_id) else {
            return;
        };
        let Some(path) = row.path.clone() else {
            return;
        };
        self.context_menu = None;
        self.project_settings = Some(ProjectSettingsCard {
            id: project_id.to_string(),
            name: row.title.clone(),
            path,
            is_git: row.is_git,
        });
        cx.notify();
    }

    pub fn set_notice(&mut self, notice: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(notice.into());
        cx.notify();
    }

    fn dispatch_context_action(
        &mut self,
        target: SidebarContextTarget,
        action: SidebarContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        if action == SidebarContextAction::RemoveProject {
            if let SidebarContextTarget::Project { id, .. } = target {
                self.request_remove_project(id, window, cx);
            }
            return;
        }
        cx.emit(SidebarEvent::ContextAction { target, action });
    }

    fn start_add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI's platform path prompt is the one mechanism this codebase
        // opens a chooser with: it routes to the XDG portal on Linux and
        // the system open-panel on macOS. The sidebar only turns the picked
        // path into an event; the shell owns what happens next.
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        cx.spawn_in(window, async move |sidebar, cx| {
            let outcome = receiver.await;
            let _ = sidebar.update(cx, |sidebar, cx| match outcome {
                // A real selection: report the picked directory.
                Ok(Ok(Some(mut paths))) => {
                    if let Some(path) = paths.pop() {
                        cx.emit(SidebarEvent::AddProject(path));
                    }
                }
                // Cancelled (or an empty selection): nothing, silently.
                Ok(Ok(None)) => {}
                // The platform could not open a chooser at all (no XDG
                // portal on this session, for example). A silent no-op
                // here is the exact "drawn but does nothing" defect — say
                // why instead of pretending nothing happened.
                Ok(Err(error)) => {
                    sidebar.notice = Some(format!("could not open the folder picker: {error}"));
                    cx.notify();
                }
                // The prompt was dropped with the window.
                Err(_) => {}
            });
        })
        .detach();
    }

    fn request_remove_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = window.prompt(
            PromptLevel::Warning,
            "Remove project from Tiller?",
            Some("This only removes the project from Tiller's sidebar. Files on disk will not be deleted."),
            &["Remove from Tiller", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |sidebar, cx| {
            if receiver.await.unwrap_or(1) == 0 {
                let _ =
                    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::RemoveProject(project_id)));
            }
        })
        .detach();
    }

    /// Mirrors SidebarView.swift: an explicit project colour wins when present;
    /// the fixture has no persisted override, so its stable fallback is the
    /// DJB2 hash of the project name modulo the source palette.
    fn project_color(name: &str) -> Rgba {
        let mut hash = 5381_i64;
        for byte in name.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(i64::from(byte));
        }
        // TODO(theme): expose the Swift project's semantic accent palette from
        // tiller_theme. These are the source-defined system colours used by
        // SidebarView's stable fallback.
        match hash.unsigned_abs() as usize % 8 {
            0 => rgb(0x007AFF), // blue
            1 => rgb(0xFF9500), // orange
            2 => rgb(0x34C759), // green
            3 => rgb(0xAF52DE), // purple
            4 => rgb(0xFF2D55), // pink
            5 => rgb(0x30B0C7), // teal
            6 => rgb(0x5856D6), // indigo
            _ => rgb(0xFFCC00), // yellow
        }
    }

    /// waku's row rhythm: a single-line row is 32px (13.5px title at an
    /// 18px line height plus 7px of vertical padding — the action-row
    /// math); a card with a context line is 51px (7 + 18 + 4 + 15 + 7 —
    /// the session-card math). Long titles add one 18px line each.
    fn row_height(row: &SidebarRow) -> f32 {
        let lines = row
            .title
            .chars()
            .count()
            .div_ceil(PROJECT_TITLE_CHARS_PER_LINE)
            .max(1);
        let extra_lines = lines.saturating_sub(1) as f32;
        let is_card =
            row.path.is_some() && matches!(row.kind, RowKind::Project | RowKind::Worktree);
        if is_card {
            CARD_TWO_LINE_HEIGHT + extra_lines * ROW_TITLE_LINE_HEIGHT
        } else {
            ROW_HEIGHT + extra_lines * ROW_TITLE_LINE_HEIGHT
        }
    }

    fn on_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.filter.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.filter.push_str(character);
        }
        cx.notify();
    }

    fn select_row(&mut self, id: usize, cx: &mut Context<Self>) {
        for row in &mut self.rows {
            row.selected = row.id == id;
        }
        cx.notify();
    }

    /// Host-driven selection: the shell answers a [`SidebarEvent::SelectWorktree`]
    /// by calling this, so the highlight always agrees with what the shell
    /// actually treats as the current worktree.
    pub fn set_selected_worktree(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let mut changed = false;
        for row in &mut self.rows {
            let selected = row.kind == RowKind::Worktree && row.path.as_deref() == Some(path);
            if row.selected != selected {
                row.selected = selected;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Sets the live agent status shown on a worktree row's status dot. The
    /// host resolves the status (from `tiller_activity`, ultimately); this
    /// crate only renders whatever it is handed.
    pub fn set_worktree_status(
        &mut self,
        id: usize,
        status: Option<ActivityStatus>,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.kind == RowKind::Worktree)
        {
            if row.agent_status == status {
                return;
            }
            row.agent_status = status;
            cx.notify();
        }
    }

    /// Replaces the tab rows under a worktree with the host's real, current
    /// tab list — the same list the tab bar and the Activity panel already
    /// render. Called every render (see `TillerWorkspace::sync_activity`),
    /// so this must diff before touching `self.rows` or it would `notify()`
    /// every frame forever.
    ///
    /// A row built here carries `tab_id: Some(tab.id)` and, unlike a fixture
    /// or catalog row, is never locally toggled by a click: `selected`
    /// always comes from the host's `tabs` argument, because the host's
    /// `active_tab` is the one fact "which tab is selected" is allowed to
    /// have.
    pub fn set_worktree_tabs(
        &mut self,
        worktree_id: usize,
        tabs: Vec<SidebarTab>,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree_index) = self
            .rows
            .iter()
            .position(|row| row.id == worktree_id && row.kind == RowKind::Worktree)
        else {
            return;
        };
        let insert_at = worktree_index + 1;
        let existing_end = insert_at
            + self.rows[insert_at..]
                .iter()
                .take_while(|row| row.kind == RowKind::Tab && row.tab_id.is_some())
                .count();

        let unchanged = self.rows[insert_at..existing_end]
            .iter()
            .map(|row| {
                (
                    row.tab_id,
                    row.tab_kind,
                    row.agent_icon,
                    row.title.as_str(),
                    row.selected,
                )
            })
            .eq(tabs.iter().map(|tab| {
                (
                    Some(tab.id),
                    Some(tab.kind),
                    tab.agent_icon,
                    tab.title.as_str(),
                    tab.selected,
                )
            }));
        if unchanged {
            return;
        }

        let depth = self.rows[worktree_index].depth + 1;
        let new_rows = tabs.into_iter().map(|tab| SidebarRow {
            id: TAB_ROW_ID_OFFSET + tab.id,
            kind: RowKind::Tab,
            depth,
            title: tab.title,
            selected: tab.selected,
            expanded: false,
            is_primary: false,
            agent_status: None,
            is_git: false,
            path: None,
            tab_id: Some(tab.id),
            tab_kind: Some(tab.kind),
            agent_icon: tab.agent_icon,
        });
        self.rows.splice(insert_at..existing_end, new_rows);
        cx.notify();
    }

    fn toggle_project(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(project) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.kind == RowKind::Project)
        {
            project.expanded = !project.expanded;
        }
        self.select_row(id, cx);
    }

    // ------------------------------------------------------------------
    // Worktree creation and removal
    // ------------------------------------------------------------------

    /// The index of the project row enclosing `row_id`, if any.
    fn enclosing_project_index(&self, row_id: usize) -> Option<usize> {
        let row_index = self.rows.iter().position(|row| row.id == row_id)?;
        self.rows[..=row_index]
            .iter()
            .rposition(|row| row.kind == RowKind::Project)
    }

    /// The repository root of the project enclosing `row_id`, if it has one.
    fn project_root(&self, row_id: usize) -> Option<PathBuf> {
        let project_index = self.enclosing_project_index(row_id)?;
        let project = &self.rows[project_index];
        if project.is_git {
            project.path.clone()
        } else {
            None
        }
    }

    /// Opens the branch-name prompt for the project whose New Worktree row
    /// was clicked.
    fn begin_worktree_prompt(
        &mut self,
        row_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_index) = self.enclosing_project_index(row_id) else {
            return;
        };
        let project = &self.rows[project_index];
        let Some(repo_root) = project.path.clone() else {
            // A project with no repository path (like a non-git project) is
            // never offered the row in the first place.
            return;
        };
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        self.prompt = Some(WorktreePrompt {
            project_row_id: project.id,
            project_name: project.title.clone(),
            repo_root,
            draft: String::new(),
            error: None,
            focus,
        });
        cx.notify();
    }

    /// Cancels the open prompt, if any.
    fn cancel_worktree_prompt(&mut self, cx: &mut Context<Self>) {
        if self.prompt.take().is_some() {
            cx.notify();
        }
    }

    /// Confirms the open prompt: creates the worktree on the background
    /// executor and inserts its row on success.
    fn confirm_worktree_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let branch = prompt.draft.trim().to_string();
        if branch.is_empty() {
            // Re-open with the reason; the draft is kept for editing.
            self.prompt = Some(WorktreePrompt {
                error: Some("enter a branch name".to_string()),
                ..prompt
            });
            cx.notify();
            return;
        }

        let repo_root = prompt.repo_root.clone();
        let project_name = prompt.project_name.clone();
        let project_row_id = prompt.project_row_id;
        // The worktree lives next to the project (or at an explicit
        // override), named `{project}-{branch}` — mirroring the Swift app.
        let path = derive_worktree_path(
            &resolve_parent_directory(&repo_root, None),
            &project_name,
            &branch,
        );

        let repo_root_for_task = repo_root.clone();
        let branch_for_task = branch.clone();
        let path_for_task = path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    create_worktree(&repo_root_for_task, &branch_for_task, &path_for_task, None)
                })
                .await;
            this.update(cx, |sidebar, cx| match result {
                Ok(()) => {
                    sidebar.insert_worktree_row(project_row_id, &branch, path, cx);
                }
                Err(error) => {
                    sidebar.notice = Some(error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Inserts a worktree row for `branch` under the project, before its
    /// New Worktree row, and selects it.
    fn insert_worktree_row(
        &mut self,
        project_row_id: usize,
        branch: &str,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let project_index = self
            .rows
            .iter()
            .position(|row| row.id == project_row_id)
            .expect("project row still present");
        let insert_at = self.rows[project_index + 1..]
            .iter()
            .position(|row| row.kind == RowKind::NewWorktree)
            .map_or(self.rows.len(), |offset| project_index + 1 + offset);
        // Excludes tab rows: their ids live at `TAB_ROW_ID_OFFSET` and up
        // (see that constant's doc comment), a separate namespace from
        // hand- and index-assigned rows. Taking the max across both would
        // hand a brand-new worktree row an id inside the tab-row range as
        // soon as any tab was open.
        let id = self
            .rows
            .iter()
            .filter(|row| row.tab_id.is_none())
            .map(|row| row.id)
            .max()
            .unwrap_or(0)
            + 1;

        self.rows.insert(
            insert_at,
            SidebarRow {
                id,
                kind: RowKind::Worktree,
                depth: 1,
                title: branch.to_string(),
                selected: false,
                expanded: false,
                is_primary: false,
                agent_status: None,
                is_git: true,
                path: Some(path),
                tab_id: None,
                tab_kind: None,
                agent_icon: None,
            },
        );
        self.select_row(id, cx);
        self.notice = None;
    }

    /// Removes a worktree on the background executor and drops its rows.
    fn remove_worktree_row(&mut self, row_id: usize, cx: &mut Context<Self>) {
        let Some(repo_root) = self.project_root(row_id) else {
            return;
        };
        let Some(worktree_path) = self
            .rows
            .iter()
            .find(|row| row.id == row_id)
            .and_then(|row| row.path.clone())
        else {
            return;
        };

        let repo_root_for_task = repo_root.clone();
        let worktree_path_for_task = worktree_path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { remove_worktree(&repo_root_for_task, &worktree_path_for_task) })
                .await;
            this.update(cx, |sidebar, cx| match result {
                Ok(()) => {
                    sidebar.drop_worktree_rows(row_id);
                    cx.notify();
                }
                Err(error) => {
                    // git refused (typically uncommitted changes); surface
                    // its reason instead of forcing through.
                    sidebar.notice = Some(error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Drops a worktree row and its tab rows from the tree.
    fn drop_worktree_rows(&mut self, row_id: usize) {
        let Some(index) = self.rows.iter().position(|row| row.id == row_id) else {
            return;
        };
        let end = self.rows[index + 1..]
            .iter()
            .position(|row| row.kind != RowKind::Tab)
            .map_or(self.rows.len(), |offset| index + 1 + offset);
        self.rows.drain(index..end);
        self.notice = None;
    }

    // ------------------------------------------------------------------
    // The branch-name prompt
    // ------------------------------------------------------------------

    fn on_prompt_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" | "return" => self.confirm_worktree_prompt(cx),
            "escape" => self.cancel_worktree_prompt(cx),
            "backspace" | "delete" => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.draft.pop();
                }
                cx.notify();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.draft.push_str(character);
                    }
                    cx.notify();
                }
            }
        }
    }

    fn visible_rows(&self) -> Vec<SidebarRow> {
        let query = self.filter.trim().to_lowercase();
        if query.is_empty() {
            return self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    if row.kind == RowKind::Project {
                        Some((index, row))
                    } else {
                        None
                    }
                })
                .flat_map(|(project_index, project)| {
                    let next_project = self.rows[project_index + 1..]
                        .iter()
                        .position(|row| row.kind == RowKind::Project)
                        .map_or(self.rows.len(), |offset| project_index + 1 + offset);
                    let mut section = vec![project.clone()];
                    if project.expanded {
                        section.extend(
                            self.rows[project_index + 1..next_project]
                                .iter()
                                .filter(|row| {
                                    // The New Worktree row is only offered
                                    // for git projects with a repository
                                    // path.
                                    row.kind != RowKind::NewWorktree
                                        || (project.is_git && project.path.is_some())
                                })
                                .cloned(),
                        );
                    }
                    section
                })
                .collect();
        }

        let mut filtered = Vec::new();
        let mut project_index = 0;
        while project_index < self.rows.len() {
            let project = &self.rows[project_index];
            if project.kind != RowKind::Project {
                project_index += 1;
                continue;
            }
            let next_project = self.rows[project_index + 1..]
                .iter()
                .position(|row| row.kind == RowKind::Project)
                .map_or(self.rows.len(), |offset| project_index + 1 + offset);
            let section = &self.rows[project_index + 1..next_project];
            let project_matches = project.title.to_lowercase().contains(&query);
            let section_matches = section
                .iter()
                .any(|row| row.title.to_lowercase().contains(&query));

            if project_matches || section_matches {
                filtered.push(project.clone());
                if project.expanded || section_matches {
                    let mut worktree: Option<SidebarRow> = None;
                    let mut tabs = Vec::new();
                    let append_worktree =
                        |filtered: &mut Vec<SidebarRow>,
                         worktree: &mut Option<SidebarRow>,
                         tabs: &mut Vec<SidebarRow>| {
                            let Some(worktree_row) = worktree.take() else {
                                return;
                            };
                            let worktree_matches =
                                worktree_row.title.to_lowercase().contains(&query);
                            let tab_matches = tabs
                                .iter()
                                .any(|tab: &SidebarRow| tab.title.to_lowercase().contains(&query));
                            if worktree_matches || tab_matches {
                                filtered.push(worktree_row);
                                if worktree_matches {
                                    filtered.append(tabs);
                                } else {
                                    filtered.extend(
                                        tabs.drain(..).filter(|tab| {
                                            tab.title.to_lowercase().contains(&query)
                                        }),
                                    );
                                }
                            } else {
                                tabs.clear();
                            }
                        };
                    for row in section {
                        match row.kind {
                            RowKind::Worktree => {
                                append_worktree(&mut filtered, &mut worktree, &mut tabs);
                                worktree = Some(row.clone());
                            }
                            RowKind::Tab => tabs.push(row.clone()),
                            RowKind::NewWorktree => {
                                append_worktree(&mut filtered, &mut worktree, &mut tabs);
                                if project_matches
                                    && project.expanded
                                    && project.is_git
                                    && project.path.is_some()
                                {
                                    filtered.push(row.clone());
                                }
                            }
                            RowKind::Project => {}
                        }
                    }
                    append_worktree(&mut filtered, &mut worktree, &mut tabs);
                }
            }
            project_index = next_project;
        }
        filtered
    }

    fn row_icon(row: &SidebarRow) -> Icon {
        match row.kind {
            RowKind::Project => Icon::FolderFill,
            RowKind::Worktree => Icon::GitBranch,
            RowKind::Tab => row.agent_icon.unwrap_or(match row.tab_kind {
                Some(TabKind::Terminal) => Icon::SquareTerminal,
                Some(TabKind::Editor | TabKind::Diff) => Icon::File,
                _ => Icon::MessageSquare,
            }),
            RowKind::NewWorktree => Icon::Plus,
        }
    }

    fn context_action_selector(action: SidebarContextAction) -> &'static str {
        match action {
            SidebarContextAction::ProjectSettings => "project-settings",
            SidebarContextAction::InitializeGit => "initialize-git",
            SidebarContextAction::RevealInFileManager => "reveal-in-file-manager",
            SidebarContextAction::RemoveProject => "remove-project",
            SidebarContextAction::SetPrimary => "set-primary",
            SidebarContextAction::UnsetPrimary => "unset-primary",
            SidebarContextAction::NewTab(NewTabAction::NewTerminal) => "new-terminal",
            SidebarContextAction::NewTab(NewTabAction::ClaudeCode) => "claude-code",
            SidebarContextAction::NewTab(NewTabAction::Codex) => "codex",
            SidebarContextAction::NewTab(NewTabAction::OpenCode) => "opencode",
            SidebarContextAction::NewTab(NewTabAction::Pi) => "pi",
            SidebarContextAction::NewTab(NewTabAction::OhMyPi) => "oh-my-pi",
            SidebarContextAction::NewTab(NewTabAction::NewChat) => "new-chat",
            SidebarContextAction::NewTab(NewTabAction::NewChanges)
            | SidebarContextAction::NewTab(NewTabAction::NewBrowser)
            | SidebarContextAction::NewTab(NewTabAction::SplitClaudeCode) => "unsupported",
        }
    }

    fn render_context_menu(
        menu: OpenContextMenu,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let target = menu.target;
        let mut view = div()
            .id("sidebar-context-menu")
            .debug_selector(|| "sidebar-context-menu".to_owned())
            .absolute()
            .left(px(18.0))
            .top(px(54.0))
            .w(px(240.0))
            .p(px(6.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .shadow_lg();

        for item in Self::context_menu_items(&target) {
            let selector = format!(
                "sidebar-context-item-{}",
                Self::context_action_selector(item.action)
            );
            let enabled = item.enabled;
            let action = item.action;
            let item_target = target.clone();
            let item_entity = entity.clone();
            let mut row = div()
                .id(selector.clone())
                .debug_selector(move || selector.clone())
                .w_full()
                .min_h(px(29.0))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(theme.radii.control)
                .flex()
                .items_center()
                .justify_between()
                .text_size(theme.typography.footnote)
                .text_color(if enabled { theme.title } else { theme.meta })
                .when(enabled, |this| {
                    this.hover(|style| style.bg(theme.row_hover))
                });
            if enabled {
                row = row.on_click(move |_, window, cx| {
                    item_entity.update(cx, |sidebar, cx| {
                        sidebar.dispatch_context_action(item_target.clone(), action, window, cx)
                    });
                });
            }
            row = row.child(item.label);
            if let Some(reason) = item.disabled_reason {
                row = row.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.meta)
                        .child(reason.to_string()),
                );
            }
            view = view.child(row);
        }
        view.on_mouse_down_out(move |_, _, cx| {
            entity.update(cx, |sidebar, cx| sidebar.close_context_menu(cx));
        })
    }

    fn render_project_settings(
        card: ProjectSettingsCard,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let close_entity = entity.clone();
        div()
            .id("project-settings-sheet")
            .debug_selector(|| "project-settings-sheet".to_owned())
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(0.0))
            .bottom(px(0.0))
            .p(px(16.0))
            .bg(theme.sidebar)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
                    .child(format!("Project Settings · {}", card.name)),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.meta)
                    .child(card.path.display().to_string()),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.title)
                    .child(if card.is_git {
                        "Repository: Git"
                    } else {
                        "Repository: Folder"
                    }),
            )
            .child(
                div()
                    .id("close-project-settings")
                    .debug_selector(|| "close-project-settings".to_owned())
                    .mt(px(4.0))
                    .w(px(80.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(theme.radii.control)
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        close_entity.update(cx, |sidebar, cx| {
                            sidebar.project_settings = None;
                            cx.notify();
                        });
                    })
                    .child("Close"),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.meta)
                    .child(card.id),
            )
    }

    fn render_row(
        row: SidebarRow,
        project_id: Option<String>,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let row_id = row.id;
        let selected = row.selected;
        let kind = row.kind;
        let title = row.title.clone();
        let path = row.path.clone();
        // The click closure reports the checkout path for worktree rows.
        let worktree_path = path.clone();
        let is_project = kind == RowKind::Project;
        let is_worktree = kind == RowKind::Worktree;
        let guide = matches!(kind, RowKind::Worktree | RowKind::Tab);
        // waku's card rhythm: projects and worktrees are two-line cards
        // (13.5px title over an 11.5px context line); leaf rows are
        // single-line at the 32px action-row height.
        let is_card = path.is_some() && matches!(kind, RowKind::Project | RowKind::Worktree);
        let row_height = Self::row_height(&row);
        let row_left_inset = ROW_LEFT_INSET + row.depth.saturating_sub(1) as f32 * TAB_INDENT;
        let row_width = SIDEBAR_WIDTH - row_left_inset - ROW_RIGHT_INSET;
        let title_width = row_width - 16.0 - 12.0 - 16.0 - 21.0 - 16.0;
        let disclosure = match (kind, row.expanded) {
            (RowKind::Project, true) => Some(Icon::ChevronDown),
            (RowKind::Project, false) => Some(Icon::ChevronRight),
            _ => None,
        };
        // A dot only appears for a notable status — matching the reference.
        let status_dot_color = (kind == RowKind::Worktree)
            .then_some(row.agent_status)
            .flatten()
            .and_then(|status| match status {
                ActivityStatus::Done => Some(theme.tab_done),
                ActivityStatus::Error => Some(theme.tab_error),
                ActivityStatus::Idle => Some(theme.tab_needs_input),
                ActivityStatus::Running => None,
            });
        let glyph = Self::row_icon(&row);
        let glyph_color = match kind {
            RowKind::Project => Self::project_color(&title),
            RowKind::Tab => theme.tab_needs_input,
            RowKind::Worktree | RowKind::NewWorktree => theme.meta,
        };
        let text_color = if selected {
            theme.title_selected
        } else {
            theme.title
        };
        let entity = entity.clone();
        let remove_entity = entity.clone();
        let click_entity = entity.clone();
        let tab_close_entity = entity.clone();
        let context_entity = entity.clone();
        let hover_group = format!("sidebar-project-{row_id}");
        let tab_id = row.tab_id;

        let row_debug_selector = if kind == RowKind::NewWorktree {
            "new-worktree-row".to_string()
        } else {
            format!("sidebar-row-{row_id}")
        };
        let mut row_view = div()
            .id(row_id)
            .debug_selector(move || row_debug_selector)
            .group(hover_group.clone())
            .relative()
            .h(px(row_height))
            .w(px(row_width))
            .ml(px(row_left_inset))
            .mr(px(ROW_RIGHT_INSET))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(ROW_GAP))
            .px(px(8.0))
            .py(px(ROW_V_PADDING))
            .rounded(theme.radii.control)
            // Rows are clickable, never draggable: project order belongs to
            // the host's catalog, there is no reorder gesture to install,
            // and a pointer cursor would invite a drag the program cannot
            // perform (P19). Pin the default arrow — waku's convention.
            .cursor_default()
            .text_size(px(13.5))
            .text_color(text_color)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, window, cx| {
                click_entity.update(cx, |sidebar, cx| {
                    if let Some(tab_id) = tab_id {
                        // A host-driven row: the host owns which tab is
                        // selected, so report the click rather than
                        // flipping `selected` locally.
                        cx.emit(SidebarEvent::SelectTab(tab_id));
                        return;
                    }
                    match kind {
                        RowKind::Project => sidebar.toggle_project(row_id, cx),
                        RowKind::NewWorktree => {
                            sidebar.begin_worktree_prompt(row_id, window, cx);
                        }
                        RowKind::Tab => {}
                        RowKind::Worktree => {
                            // Report the click to the host; the host decides
                            // what actually becomes selected and confirms by
                            // calling back `set_selected_worktree`.
                            if let Some(path) = worktree_path.as_ref() {
                                cx.emit(SidebarEvent::SelectWorktree(path.clone()));
                            }
                            sidebar.select_row(row_id, cx);
                        }
                    }
                });
            });

        row_view = row_view.on_mouse_down(MouseButton::Right, move |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            context_entity.update(cx, |sidebar, cx| sidebar.open_context_menu(row_id, cx));
        });

        if selected {
            row_view = row_view.bg(theme.selected_fill);
        }

        // The card's main line. The disclosure chevron and every per-row
        // control are hover-revealed, waku's way of keeping a resting row
        // free of chrome.
        let main_line = div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .min_h(px(ROW_TITLE_LINE_HEIGHT))
            .child(
                div()
                    .w(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(theme.meta)
                    .child(match status_dot_color {
                        Some(color) => div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(color)
                            .into_any_element(),
                        None => match disclosure {
                            Some(icon) => IconElement::new(icon, px(11.0))
                                .text_color(theme.meta)
                                .invisible()
                                .group_hover(hover_group.clone(), |icon| icon.visible())
                                .into_any_element(),
                            None => div().into_any_element(),
                        },
                    }),
            )
            .child(
                div()
                    .w(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        IconElement::new(glyph, px(if is_project { 14.0 } else { 13.0 }))
                            .text_color(glyph_color),
                    ),
            )
            .child(
                div()
                    .w(px(title_width))
                    .flex_none()
                    .whitespace_normal()
                    .line_height(px(ROW_TITLE_LINE_HEIGHT))
                    .font_weight(if is_project {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(text_color)
                    .child(title),
            )
            .when(is_project, |this| {
                this.child(
                    div()
                        .id(("project-settings", row_id))
                        .debug_selector(move || format!("project-settings-{row_id}"))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(theme.meta)
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .child(IconElement::new(Icon::Settings, px(12.0)).text_color(theme.meta))
                        .on_click(move |_, _window, cx| {
                            if let Some(project_id) = project_id.clone() {
                                remove_entity.update(cx, |_, cx| {
                                    cx.emit(SidebarEvent::OpenProjectSettings(project_id));
                                });
                            }
                        }),
                )
            })
            .when(is_worktree, |this| {
                let remove_entity = entity.clone();
                this.child(
                    div()
                        .id(("remove-worktree", row_id))
                        .debug_selector(move || format!("remove-worktree-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(11.0))
                        .text_color(theme.meta)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.row_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            remove_entity.update(cx, |sidebar, cx| {
                                sidebar.remove_worktree_row(row_id, cx);
                            });
                        })
                        .child(IconElement::new(Icon::Close, px(13.0)).text_color(theme.meta)),
                )
            })
            .when_some(tab_id, |this, tab_id| {
                this.child(
                    div()
                        .id(("sidebar-tab-close", row_id))
                        .debug_selector(move || format!("sidebar-tab-close-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(theme.subtitle)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.row_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            tab_close_entity.update(cx, |_, cx| {
                                cx.emit(SidebarEvent::CloseTab(tab_id));
                            });
                        })
                        .child(IconElement::new(Icon::Close, px(13.0)).text_color(theme.subtitle)),
                )
            });

        let row_view = row_view.child(main_line).when(is_card, |this| {
            let sub = path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            this.child(
                div()
                    // Aligned under the title: 12px leading slot + 7px gap
                    // + 16px glyph + 7px gap.
                    .pl(px(42.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(11.5))
                    .line_height(px(ROW_SUB_LINE_HEIGHT))
                    .text_color(theme.meta)
                    .child(div().min_w_0().truncate().child(sub))
                    .when(row.is_primary, |this| {
                        this.child(
                            div()
                                .id(("sidebar-primary-pill", row_id))
                                .debug_selector(move || format!("sidebar-primary-pill-{row_id}"))
                                .px(px(5.0))
                                .rounded(theme.radii.chip)
                                .bg(theme.primary_pill_bg)
                                .text_color(theme.title)
                                .text_size(px(10.0))
                                .child("Primary"),
                        )
                    }),
            )
        });

        let mut container = div().relative().w_full().h(px(row_height));
        if guide {
            container = container.child(
                div()
                    .absolute()
                    .left(px(GUIDE_LEFT))
                    .top(px(0.0))
                    .w(px(GUIDE_WIDTH))
                    .h_full()
                    .bg(INDENT_GUIDE_FILL),
            );
        }
        container.child(row_view)
    }
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.filter_focus.clone()
    }
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let rows = self.visible_rows();
        let entity = cx.entity();
        // The row list consumes one; the worktree prompt below needs another.
        let prompt_owner = entity.clone();
        let project_ids = self.project_ids.clone();
        let filter_focus = self.filter_focus.clone();
        let filter_is_focused = filter_focus.is_focused(window);
        let filter_text = self.filter.clone();
        let prompt = self.prompt.clone();
        let notice = self.notice.clone();
        let context_menu = self.context_menu.clone();
        let project_settings = self.project_settings.clone();
        div()
            .relative()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .overflow_hidden()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .pt(px(8.0))
            .child(
                div()
                    .h(px(20.0))
                    .w_full()
                    .px(px(FILTER_LEFT_INSET))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.meta)
                    .child("Projects")
                    .child(
                        div()
                            .id("add-project")
                            .debug_selector(|| "add-project".to_string())
                            .w(px(20.0))
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(16.0))
                            .text_color(theme.meta)
                            .hover(|style| style.bg(theme.row_hover).rounded(theme.radii.control))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_add_project(window, cx);
                            }))
                            .child("+"),
                    ),
            )
            .child(
                div()
                    .id("filter-field")
                    .track_focus(&filter_focus)
                    .relative()
                    .ml(px(FILTER_LEFT_INSET))
                    .mt(px(6.0))
                    .w(px(SIDEBAR_WIDTH - FILTER_LEFT_INSET - ROW_RIGHT_INSET))
                    .h(px(28.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded(theme.radii.control)
                    .bg(theme.filter_field_bg)
                    .border_1()
                    .border_color(if filter_is_focused {
                        theme.selection_ring
                    } else {
                        theme.hairline
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.filter_focus.focus(window, cx);
                        }),
                    )
                    .on_key_down(cx.listener(Self::on_filter_key))
                    .child(div().text_size(px(11.5)).text_color(theme.meta).child("⌕"))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .text_color(if filter_text.is_empty() {
                                theme.meta
                            } else {
                                theme.title
                            })
                            .child(if filter_text.is_empty() {
                                "Filter".to_owned()
                            } else {
                                filter_text
                            }),
                    ),
            )
            .child(
                div()
                    .id("sidebar-tree")
                    .mt(px(11.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(rows.into_iter().map({
                        let entity = entity.clone();
                        move |row| {
                            let project_id = project_ids.get(&row.id).cloned();
                            Self::render_row(row, project_id, entity.clone(), theme)
                        }
                    })),
            )
            .when(notice.is_some(), |this| {
                this.child(
                    div()
                        .id("sidebar-notice")
                        .w_full()
                        .px(px(FILTER_LEFT_INSET))
                        .py(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.diff_deletion)
                        .child(notice.unwrap_or_default()),
                )
            })
            .when(prompt.is_some(), |this| {
                let prompt = prompt.expect("checked above");
                let prompt_entity = prompt_owner.clone();
                this.child(
                    div()
                        .id("worktree-prompt")
                        .absolute()
                        .left(px(0.0))
                        .right(px(0.0))
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x000000).alpha(0.35))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                if let Some(prompt) = &this.prompt {
                                    prompt.focus.focus(window, cx);
                                }
                            }),
                        )
                        .child(
                            div()
                                .id("worktree-prompt-card")
                                .debug_selector(|| "worktree-prompt".to_string())
                                .track_focus(&prompt.focus)
                                .w(px(260.0))
                                .rounded(theme.radii.toast)
                                .bg(theme.chat_surface)
                                .border_1()
                                .border_color(theme.hairline)
                                .px(px(14.0))
                                .py(px(12.0))
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .on_key_down(cx.listener(Self::on_prompt_key))
                                .on_click(move |_, window, cx| {
                                    // Keep the card focused when clicked.
                                    let focus = prompt_entity
                                        .read(cx)
                                        .prompt
                                        .as_ref()
                                        .map(|prompt| prompt.focus.clone());
                                    if let Some(focus) = focus {
                                        focus.focus(window, cx);
                                    }
                                })
                                .child(
                                    div()
                                        .text_size(theme.typography.headline)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.title)
                                        .child(format!("New worktree in {}", prompt.project_name)),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(26.0))
                                        .px(px(8.0))
                                        .flex()
                                        .items_center()
                                        .rounded(theme.radii.control)
                                        .bg(theme.filter_field_bg)
                                        .border_1()
                                        .border_color(theme.selection_ring)
                                        .text_size(theme.typography.footnote)
                                        .text_color(if prompt.draft.is_empty() {
                                            theme.meta
                                        } else {
                                            theme.title
                                        })
                                        .child(if prompt.draft.is_empty() {
                                            "branch name".to_owned()
                                        } else {
                                            prompt.draft.clone()
                                        }),
                                )
                                .when(prompt.error.is_some(), |this| {
                                    this.child(
                                        div()
                                            .text_size(theme.typography.footnote)
                                            .text_color(theme.diff_deletion)
                                            .child(prompt.error.clone().unwrap_or_default()),
                                    )
                                })
                                .child(
                                    div()
                                        .text_size(theme.typography.caption2)
                                        .text_color(theme.meta)
                                        .child("Enter to create · Esc to cancel"),
                                ),
                        ),
                )
            })
            .when_some(context_menu, |this, menu| {
                this.child(Self::render_context_menu(menu, entity.clone(), theme))
            })
            .when_some(project_settings, |this, card| {
                this.child(Self::render_project_settings(card, entity.clone(), theme))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, VisualTestContext, point};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Creates a throwaway git repo with one commit.
    fn scratch_repo(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "tiller-sidebar-test-{tag}-{}-{unique}",
            std::process::id()
        ));
        let repo = parent.join("repo");
        std::fs::create_dir_all(&repo).expect("create repo dir");
        // Canonicalize so derived paths match what porcelain reports
        // (macOS /var is a symlink to /private/var).
        let repo = std::fs::canonicalize(&repo).expect("canonicalize repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@tiller.dev"],
            vec!["config", "user.name", "Tiller Test"],
        ] {
            assert!(
                Command::new("git")
                    .args(&args)
                    .current_dir(&repo)
                    .status()
                    .expect("git")
                    .success()
            );
        }
        std::fs::write(repo.join("file.txt"), "one\ntwo\nthree\n").expect("write");
        assert!(
            Command::new("git")
                .args(["add", "-A"])
                .current_dir(&repo)
                .status()
                .expect("git")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "root"])
                .current_dir(&repo)
                .status()
                .expect("git")
                .success()
        );
        repo
    }

    fn porcelain(repo: &std::path::Path) -> String {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo)
            .output()
            .expect("git");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn terminal_tab_icon_ignores_title() {
        let row = SidebarRow {
            id: 1,
            kind: RowKind::Tab,
            depth: 2,
            title: "foo".to_string(),
            selected: false,
            expanded: false,
            agent_status: None,
            is_primary: false,
            is_git: false,
            path: None,
            tab_id: Some(1),
            tab_kind: Some(TabKind::Terminal),
            agent_icon: None,
        };

        assert_eq!(Sidebar::row_icon(&row), Icon::SquareTerminal);
    }

    #[test]
    fn agent_tab_icon_ignores_title() {
        let row = SidebarRow {
            id: 2,
            kind: RowKind::Tab,
            depth: 2,
            title: "renamed agent".to_string(),
            selected: false,
            expanded: false,
            agent_status: None,
            is_primary: false,
            is_git: false,
            path: None,
            tab_id: Some(2),
            tab_kind: Some(TabKind::Terminal),
            agent_icon: Some(Icon::ClaudeCode),
        };

        assert_eq!(Sidebar::row_icon(&row), Icon::ClaudeCode);
    }

    #[test]
    fn sidebar_context_items_explain_git_eligibility_and_list_every_new_surface() {
        let git_project = SidebarContextTarget::Project {
            id: "git".to_string(),
            path: PathBuf::from("/tmp/git"),
            is_git: true,
        };
        let items = Sidebar::context_menu_items(&git_project);
        let initialize = items
            .iter()
            .find(|item| item.action == SidebarContextAction::InitializeGit)
            .expect("Git initialization remains visible as a disabled command");
        assert!(!initialize.enabled);
        assert_eq!(
            initialize.disabled_reason,
            Some(SidebarDisabledReason::AlreadyGitProject)
        );

        let worktree = SidebarContextTarget::Worktree {
            path: PathBuf::from("/tmp/git-main"),
            is_primary: false,
        };
        let worktree_items = Sidebar::context_menu_items(&worktree);
        assert!(
            worktree_items
                .iter()
                .any(|item| { item.action == SidebarContextAction::SetPrimary && item.enabled })
        );
        for action in [
            NewTabAction::NewTerminal,
            NewTabAction::ClaudeCode,
            NewTabAction::Codex,
            NewTabAction::OpenCode,
            NewTabAction::Pi,
            NewTabAction::OhMyPi,
            NewTabAction::NewChat,
        ] {
            assert!(
                worktree_items
                    .iter()
                    .any(|item| item.action == SidebarContextAction::NewTab(action)),
                "context menu lists {action:?}"
            );
        }
    }

    #[gpui::test]
    async fn right_click_context_menu_dispatches_a_typed_worktree_action(
        cx: &mut gpui::TestAppContext,
    ) {
        let repo = scratch_repo("context-menu");
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let row = cx
            .debug_bounds("sidebar-row-1")
            .expect("the worktree row is drawn");
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
        assert!(
            cx.debug_bounds("sidebar-context-menu").is_some(),
            "right-click draws the context menu"
        );

        let terminal = cx
            .debug_bounds("sidebar-context-item-new-terminal")
            .expect("the context menu exposes New Terminal");
        cx.simulate_click(terminal.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(events.borrow().iter().any(|event| matches!(
            event,
            SidebarEvent::ContextAction {
                target: SidebarContextTarget::Worktree { .. },
                action: SidebarContextAction::NewTab(NewTabAction::NewTerminal),
            }
        )));
    }

    #[gpui::test]
    async fn new_worktree_prompt_creates_a_real_worktree(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("create");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The New Worktree row is offered for the git project.
        let row_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        // The branch-name prompt opens.
        assert!(
            cx.debug_bounds("worktree-prompt").is_some(),
            "clicking New Worktree opens the prompt"
        );

        // Type the branch (a slash, exercising the nested-directory case)
        // and confirm with Enter.
        cx.simulate_input("feature/login");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The worktree row appears in the tree without a refresh.
        let created = cx
            .update(|window, cx| {
                let sidebar = window
                    .root::<Sidebar>()
                    .flatten()
                    .expect("sidebar root")
                    .read(cx);
                sidebar
                    .rows
                    .iter()
                    .find(|row| row.kind == RowKind::Worktree && row.title == "feature/login")
                    .cloned()
            })
            .expect("the new worktree row was inserted");
        assert!(created.selected, "the new row is selected");

        // And the real repository agrees: porcelain reports the worktree on
        // the new branch, at the derived path.
        let derived = derive_worktree_path(
            &resolve_parent_directory(&repo, None),
            "tiller",
            "feature/login",
        );
        let porcelain = porcelain(&repo);
        assert!(
            porcelain.contains(&format!("worktree {}", derived.display())),
            "porcelain reports the created worktree at the derived path:\n{porcelain}"
        );
        assert!(
            porcelain.contains("branch refs/heads/feature/login"),
            "porcelain reports the new branch:\n{porcelain}"
        );
    }

    #[gpui::test]
    async fn remove_button_removes_the_worktree(cx: &mut gpui::TestAppContext) {
        // The sidebar's create/remove go through `tiller_git`, whose runner
        // bounds every git invocation with a 10 s deadline so a hung git can
        // never freeze the UI. On a loaded machine (parallel compiles, high
        // load average) a single `git worktree` call on a fixture repo can
        // legitimately exceed that budget, which turns a healthy test into a
        // flake. Widen the budget for this process — the production default
        // is untouched; the override only applies while this variable is set.
        // SAFETY: test process; the only reader is the crate's per-call
        // `TILLER_GIT_TIMEOUT_MS` lookup, and a wider budget can only turn a
        // would-be timeout into a pass.
        unsafe { std::env::set_var("TILLER_GIT_TIMEOUT_MS", "120000") };
        let repo = scratch_repo("remove");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Create the worktree through the prompt so its row exists.
        let row_bounds = cx.debug_bounds("new-worktree-row").expect("row rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("to-remove");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The remove button sits at the row's right edge, on the card's
        // title line. The new worktree row is inserted directly above the
        // New Worktree row (whose bounds are known statically): the card is
        // 51px tall with 7px top padding and an 18px title line, so the ×
        // rides 16px below the card's top edge — i.e. 35px above the next
        // row's top.
        let new_worktree_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row's bounds are known");
        // The × is 16px wide, inset 8px from the row's right edge.
        let remove_button = point(
            new_worktree_bounds.origin.x + new_worktree_bounds.size.width - px(16.0),
            new_worktree_bounds.origin.y - px(35.0),
        );
        // The remove button is hover-revealed: move the mouse over the row
        // first so the × is visible and clickable.
        cx.simulate_mouse_move(remove_button, None, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(remove_button, Modifiers::none());

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.condition(&sidebar_entity, |sidebar, _cx| {
            !sidebar
                .rows
                .iter()
                .any(|row| row.kind == RowKind::Worktree && row.title == "to-remove")
        })
        .await;

        // The row is gone from the tree and the repository agrees.
        let remaining = cx.update(|window, cx| {
            let sidebar = window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx);
            sidebar
                .rows
                .iter()
                .filter(|row| row.kind == RowKind::Worktree)
                .map(|row| row.title.clone())
                .collect::<Vec<_>>()
        });
        assert!(
            !remaining.iter().any(|title| title == "to-remove"),
            "the removed worktree row is gone, remaining: {remaining:?}"
        );
        let porcelain = porcelain(&repo);
        assert!(
            !porcelain.contains("to-remove"),
            "porcelain no longer reports the removed worktree:\n{porcelain}"
        );
    }

    #[gpui::test]
    async fn clicking_a_worktree_row_reports_selection_to_the_host(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("select");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // The fixture's worktree rows carry no path; give the first one a
        // real checkout path so the click has something to report.
        let worktree_path = repo.join("tiller-main");
        cx.update(|_, cx| {
            sidebar_entity.update(cx, |sidebar, cx| {
                if let Some(row) = sidebar
                    .rows
                    .iter_mut()
                    .find(|row| row.kind == RowKind::Worktree)
                {
                    row.path = Some(worktree_path.clone());
                }
                cx.notify();
            })
        });
        let row_bounds = cx
            .debug_bounds("sidebar-row-1")
            .expect("the first worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectWorktree(path) if *path == worktree_path)),
            "clicking a worktree row must emit SelectWorktree with its path, got {emitted:?}"
        );

        // The host answers by confirming the selection; the highlight agrees.
        sidebar_entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_selected_worktree(&worktree_path, cx);
        });
        let selected = cx.read(|cx| {
            sidebar_entity
                .read(cx)
                .rows
                .iter()
                .filter(|r| r.selected)
                .count()
        });
        assert_eq!(selected, 1, "exactly one row stays selected");
    }

    #[gpui::test]
    async fn add_project_picker_reports_the_chosen_directory(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let picked = std::env::temp_dir().join("tiller-picked-project");
        let plus_bounds = cx
            .debug_bounds("add-project")
            .expect("the + add-project control is rendered");
        cx.simulate_click(plus_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.did_prompt_for_paths(),
            "clicking + must open the platform folder picker"
        );

        // The picker is a directory chooser, not a file chooser.
        cx.simulate_path_prompt_response(|options| {
            assert!(
                options.directories && !options.files && !options.multiple,
                "the add-project picker asks for one directory"
            );
            Some(vec![picked.clone()])
        });
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted.iter().any(|event| matches!(
                event,
                SidebarEvent::AddProject(path) if *path == picked
            )),
            "the chosen directory must be emitted as AddProject, got {emitted:?}"
        );
    }

    #[gpui::test]
    async fn add_project_picker_cancel_is_silent(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let plus_bounds = cx
            .debug_bounds("add-project")
            .expect("the + add-project control is rendered");
        cx.simulate_click(plus_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.did_prompt_for_paths(),
            "clicking + opens the platform folder picker"
        );

        // The user cancels: the platform answers None and nothing happens.
        cx.simulate_path_prompt_response(|_options| None);
        cx.run_until_parked();

        assert!(
            events.borrow().is_empty(),
            "cancelling the picker must emit nothing, got {:?}",
            events.borrow()
        );
    }

    #[gpui::test]
    async fn dragging_across_rows_does_not_reorder_the_sidebar(cx: &mut gpui::TestAppContext) {
        // P19: rows used to invite drag-to-reorder without any handler — an
        // affordance that promises something the program cannot do. There is
        // no reorder (project order belongs to the host's catalog and its
        // persistence; the reference app and waku have no drag-reorder
        // either), so the affordance is removed: rows install no drag
        // gesture, and a drag-shaped interaction must leave the row order
        // untouched.
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let row_ids = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                window
                    .root::<Sidebar>()
                    .flatten()
                    .expect("sidebar root")
                    .read(cx)
                    .rows
                    .iter()
                    .map(|row| row.id)
                    .collect::<Vec<_>>()
            })
        };
        let before = row_ids(&mut cx);

        // A drag-shaped motion: sweep from the first row to the last and
        // release there. Without a drag handler this is an ordinary hover
        // plus a click — the order must not change and nothing may panic.
        let first = cx.debug_bounds("sidebar-row-0").expect("first row");
        let last = cx.debug_bounds("sidebar-row-7").expect("last row");
        cx.simulate_mouse_move(first.center(), None, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(last.center(), Modifiers::none());
        cx.run_until_parked();

        let after = row_ids(&mut cx);
        assert_eq!(
            before, after,
            "a drag-shaped interaction must not reorder sidebar rows"
        );
    }

    /// F-TAB-15: a host-owned tab row's close control is drawn, hover-
    /// revealed, and clicking it reports CloseTab with the real tab id —
    /// the tab strip's ✕, exercised from the sidebar's view of the same
    /// tabs the strip renders.
    #[gpui::test]
    async fn the_drawn_tab_close_control_reports_closeta_tab(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("tab-close");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // The host pushes one open tab under the worktree row (id 1).
        let tab_id = 42usize;
        sidebar_entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                1,
                vec![SidebarTab {
                    id: tab_id,
                    title: "Chat".into(),
                    selected: true,
                    kind: TabKind::AgentChat,
                    agent_icon: None,
                }],
                cx,
            );
        });
        cx.run_until_parked();
        let row_id = TAB_ROW_ID_OFFSET + tab_id;

        // The close control is hover-revealed: move over the row, then the
        // ✕ is visible and clickable at its own drawn bounds.
        // `debug_bounds` takes a static selector; the row ids are dynamic,
        // so leak one string per lookup — a bounded, test-only cost.
        let row_selector: &'static str =
            Box::leak(format!("sidebar-row-{row_id}").into_boxed_str());
        let close_selector: &'static str =
            Box::leak(format!("sidebar-tab-close-{row_id}").into_boxed_str());
        let row_bounds = cx
            .debug_bounds(row_selector)
            .expect("the host-driven tab row is drawn");
        cx.simulate_mouse_move(row_bounds.center(), None, Modifiers::none());
        cx.run_until_parked();
        let close = cx
            .debug_bounds(close_selector)
            .expect("the tab close control is drawn after hovering the row");
        cx.simulate_click(close.center(), Modifiers::none());
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::CloseTab(id) if *id == tab_id)),
            "clicking the drawn ✕ must emit CloseTab for the real tab id, got {emitted:?}"
        );
        assert!(
            !emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectTab(_))),
            "the close control must not also select the tab"
        );
    }
}

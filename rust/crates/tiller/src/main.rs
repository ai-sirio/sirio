use gpui::{
    AnyElement, App, Bounds, Context, DefiniteLength, DragMoveEvent, Entity, FontWeight,
    Focusable, InteractiveElement, KeyBinding, MouseButton, Render, StatefulInteractiveElement,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div, point, prelude::*, px,
    size,
};
use gpui_platform::application;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiller_activity::{AgentActivityModel, AgentStatus};
use tiller_agents::ALL as AGENT_CATALOG;
use tiller_project::{TabKind, current_branch, is_git_repository};
use tiller_terminal::{TerminalShell, TerminalView};
use tiller_theme::Theme;
use tiller_ui::{
    chat::Chat,
    file_view::FileView,
    right_panel::{ActivityStatus, ActivitySurface, RightPanel, RightPanelEvent},
    sidebar::{
        Sidebar, SidebarEvent, SidebarProject, SidebarTab, SidebarWorktree,
        icons::{Icon, IconElement},
    },
    settings::Settings,
    status_bar::{StatusBar, UsageBarData},
    tab_bar::{NewTabAction, TabBar},
    titlebar::Titlebar,
};

mod session;
mod panes;

use panes::{PaneContent as TabContent, PaneNode, SplitDirection};
use session::{ProjectCatalog, RestoredSession, SessionLayout, SessionStore, SessionTab};

/// Column geometry, measured off the frozen reference shots — see
/// `reference/MEASURED.md`. The seam between columns is 1pt of pure black; at
/// 2pt, or tinted, or offset by a point, it is visible.
const SIDEBAR_WIDTH: f32 = 325.;
const RIGHT_PANEL_WIDTH: f32 = 405.;
const SEAM_WIDTH: f32 = 1.;
const TITLE_BAR_HEIGHT: f32 = 32.;
const STATUS_BAR_HEIGHT: f32 = 40.;
const TAB_BAR_HEIGHT: f32 = 34.;
const CHAT_TAB_WIDTH: f32 = 108.;
const TERMINAL_TAB_WIDTH: f32 = 132.;
/// The sidebar row id for the one worktree this workspace owns tabs for
/// (the project catalog puts the running worktree's checkout at index 0 of
/// its first project, unless a restored catalog already had other projects
/// first — see `main()`'s fallback). Matches the existing
/// `set_worktree_status(1, ...)` call this constant now names.
const MAIN_WORKTREE_SIDEBAR_ID: usize = 1;

struct OpenTab {
    id: usize,
    title: String,
    kind: TabKind,
    /// The agent brand shown for an agent-backed terminal tab. `None` means
    /// the surface kind decides the icon (chat, terminal, or file).
    agent_icon: Option<Icon>,
    panes: PaneNode<TabContent>,
    focused_pane: usize,
}

enum WorkspaceAction {
    NewTab(NewTabAction),
    OpenSettings,
    CloseSettings,
}

fn tab_has_file(tab: &OpenTab) -> bool {
    let mut has_file = false;
    tab.panes.for_each(&mut |_, content| {
        if matches!(content, TabContent::File { .. }) {
            has_file = true;
        }
    });
    has_file
}

fn tab_has_terminal(tab: &OpenTab) -> bool {
    let mut has_terminal = false;
    tab.panes.for_each(&mut |_, content| {
        if matches!(content, TabContent::Terminal { .. }) {
            has_terminal = true;
        }
    });
    has_terminal
}

fn tab_icon(kind: TabKind, has_file: bool, agent_icon: Option<Icon>) -> Icon {
    if has_file {
        return Icon::File;
    }
    if let Some(agent_icon) = agent_icon {
        return agent_icon;
    }
    match kind {
        TabKind::AgentChat => Icon::MessageSquare,
        TabKind::Terminal => Icon::SquareTerminal,
        TabKind::Editor => Icon::File,
        TabKind::Browser => unreachable!("Browser surfaces are external to the shell"),
        TabKind::Diff => unreachable!("Diff surfaces are inline in the Changes panel"),
    }
}

fn agent_id_for_action(action: NewTabAction) -> Option<&'static str> {
    match action {
        NewTabAction::ClaudeCode => Some("claude"),
        NewTabAction::Codex => Some("codex"),
        NewTabAction::OpenCode => Some("opencode"),
        NewTabAction::Pi => Some("pi"),
        NewTabAction::OhMyPi => Some("omp"),
        NewTabAction::SplitClaudeCode => Some("claude"),
        NewTabAction::NewTerminal | NewTabAction::NewBrowser | NewTabAction::NewChat => None,
    }
}

fn agent_icon_for_action(action: NewTabAction) -> Option<Icon> {
    agent_id_for_action(action).and_then(Icon::for_agent_id)
}

#[derive(Clone, Debug)]
struct WorktreeContext {
    branch: String,
    path: String,
    activity_label: String,
    terminal_breadcrumb: String,
}

/// Resolves the labels shown by the running shell from its selected checkout,
/// not from the fixture values used by the standalone UI demos.
fn worktree_context(catalog: &ProjectCatalog, working_directory: &Path) -> WorktreeContext {
    let catalog_entry = catalog.projects().iter().find_map(|project| {
        project
            .worktrees
            .iter()
            .find(|worktree| worktree.path == working_directory)
            .map(|worktree| {
                (
                    project.name.clone(),
                    worktree.branch.clone(),
                    worktree.is_primary,
                )
            })
    });
    let fallback_project = working_directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| working_directory.to_string_lossy().into_owned());
    let (project, catalog_branch, is_primary) = catalog_entry.unwrap_or((
        fallback_project,
        String::new(),
        false,
    ));
    let branch = current_branch(working_directory)
        .ok()
        .flatten()
        .or_else(|| {
            (is_primary && !catalog_branch.is_empty()).then_some(catalog_branch.clone())
        })
        .or_else(|| short_head(working_directory))
        .or_else(|| (!catalog_branch.is_empty()).then_some(catalog_branch))
        .unwrap_or_else(|| if is_primary { "main" } else { "HEAD" }.to_string());

    WorktreeContext {
        branch: branch.clone(),
        path: display_path(working_directory),
        activity_label: format!("{project}/{branch}"),
        terminal_breadcrumb: shell_breadcrumb(),
    }
}

/// Keeps the sidebar's detached-HEAD convention when the session catalog is
/// unavailable or stale: a short commit, then the historical primary fallback.
fn short_head(path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|commit| !commit.is_empty())
}

fn display_path(path: &Path) -> String {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return path.to_string_lossy().into_owned();
    };
    path.strip_prefix(&home)
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", relative.to_string_lossy())
            }
        })
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn shell_breadcrumb() -> String {
    let shell = std::env::var("SHELL")
        .ok()
        .and_then(|path| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "zsh".to_string());
    let time = Command::new("date")
        .arg("+%H:%M:%S")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|time| !time.is_empty())
        .unwrap_or_else(|| "--:--:--".to_string());
    format!("in {shell} at {time}")
}

#[derive(Clone, Debug)]
struct DraggedPaneDivider {
    tab_index: usize,
    path: Vec<bool>,
    direction: SplitDirection,
}

actions!(pane_navigation, [
    FocusPaneLeft,
    FocusPaneRight,
    FocusPaneAbove,
    FocusPaneBelow,
    SplitPaneRight,
    SplitPaneDown,
    ClosePane,
]);

/// The shell-owned tab model. Content entities live in this vector for the
/// lifetime of the workspace, so switching tabs only changes which entity is
/// mounted in the centre column; it never reconstructs a PTY or transcript.
struct TillerWorkspace {
    titlebar: Entity<Titlebar>,
    sidebar: Entity<Sidebar>,
    tab_bar: Entity<TabBar>,
    status_bar: Entity<StatusBar>,
    settings: Entity<Settings>,
    right_panel: Entity<RightPanel>,
    show_settings: bool,
    tabs: Vec<OpenTab>,
    active_tab: usize,
    next_tab_id: usize,
    next_pane_id: usize,
    working_directory: PathBuf,
    session: SessionStore,
    project_catalog: ProjectCatalog,
    worktree_label: String,
    terminal_breadcrumb: String,
    /// The single source of truth for agent lifecycle status. Every one of
    /// the sidebar dot, the tab checkmark, and the Activity row reads
    /// through this (or, for a chat pane, through `Chat`'s own state) —
    /// never a second, independently-tracked flag.
    activity: AgentActivityModel,
}

impl TillerWorkspace {
    fn new(
        titlebar: Entity<Titlebar>,
        sidebar: Entity<Sidebar>,
        tab_bar: Entity<TabBar>,
        status_bar: Entity<StatusBar>,
        settings: Entity<Settings>,
        right_panel: Entity<RightPanel>,
        tabs: Vec<OpenTab>,
        active_tab: usize,
        working_directory: PathBuf,
        actions: Arc<Mutex<Vec<WorkspaceAction>>>,
        session: SessionStore,
        project_catalog: ProjectCatalog,
        worktree_label: String,
        terminal_breadcrumb: String,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::bind_pane_keys(cx);
        // These UI callbacks predate an App-aware callback API. Polling this
        // small queue lets tab actions and shell navigation update the
        // workspace without changing the UI callback contracts.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let pending = {
                    let Ok(mut actions) = actions.lock() else {
                        return;
                    };
                    std::mem::take(&mut *actions)
                };
                if pending.is_empty() {
                    continue;
                }
                if this
                    .update(cx, |workspace, cx| {
                        for action in pending {
                            match action {
                                WorkspaceAction::NewTab(action) => workspace.open_action(action, cx),
                                WorkspaceAction::OpenSettings => {
                                    workspace.show_settings = true;
                                    cx.notify();
                                }
                                WorkspaceAction::CloseSettings => {
                                    workspace.show_settings = false;
                                    cx.notify();
                                }
                            }
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();

        cx.subscribe(
            &right_panel,
            |workspace, _, event: &RightPanelEvent, cx| match event {
                RightPanelEvent::SelectActivity(index) => workspace.select_activity(*index, cx),
                RightPanelEvent::CloseActivity(index) => workspace.close_tab(*index, cx),
                RightPanelEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            },
        )
        .detach();

        cx.subscribe(&sidebar, |workspace, _, event: &SidebarEvent, cx| {
            match event {
                SidebarEvent::AddProject(path) => workspace.add_project(path.clone(), cx),
                SidebarEvent::RemoveProject(id) => workspace.remove_project(id, cx),
                SidebarEvent::SelectTab(id) => workspace.select_tab(*id, cx),
                SidebarEvent::CloseTab(id) => workspace.close_tab_by_id(*id, cx),
            }
        })
        .detach();

        let tabs_len = tabs.len();
        let workspace = Self {
            titlebar,
            sidebar,
            tab_bar,
            status_bar,
            settings,
            right_panel,
            tabs,
            active_tab,
            next_tab_id: tabs_len,
            next_pane_id: tabs_len,
            working_directory,
            session,
            project_catalog,
            worktree_label,
            terminal_breadcrumb,
            activity: AgentActivityModel::new(),
            show_settings: false,
        };
        workspace.schedule_save();
        workspace.sync_activity(cx);
        workspace
    }

    /// The shell's current layout, in the shape persistence understands.
    fn layout(&self) -> SessionLayout {
        SessionLayout {
            working_directory: self.working_directory.clone(),
            branch: current_branch(&self.working_directory)
                .ok()
                .flatten()
                .unwrap_or_else(|| "main".to_string()),
            tabs: self
                .tabs
                .iter()
                .enumerate()
                .map(|(index, tab)| SessionTab {
                    title: tab.title.clone(),
                    kind: match tab.kind {
                        TabKind::Editor => "file",
                        TabKind::AgentChat => "chat",
                        TabKind::Terminal => "terminal",
                        TabKind::Browser | TabKind::Diff => {
                            unreachable!("unsupported tab kind in shell persistence")
                        }
                    }
                    .to_string(),
                    active: index == self.active_tab,
                })
                .collect(),
        }
    }

    /// Records the current layout; the session store's debounce collapses a
    /// burst of changes into one database write.
    fn schedule_save(&self) {
        self.session.schedule(self.layout());
    }

    fn sidebar_projects(&self) -> Vec<SidebarProject> {
        self.project_catalog
            .projects()
            .iter()
            .map(|project| SidebarProject {
                id: project.id.clone(),
                name: project.name.clone(),
                is_git: project.is_git,
                root_path: project.root_path.clone(),
                worktrees: project
                    .worktrees
                    .iter()
                    .map(|worktree| SidebarWorktree {
                        branch: worktree.branch.clone(),
                        path: worktree.path.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    fn refresh_sidebar(&self, cx: &mut Context<Self>) {
        let projects = self.sidebar_projects();
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_projects(projects, cx);
        });
    }

    fn add_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.project_catalog.add(&path) {
            Ok(true) => {
                self.session.schedule_catalog(&self.project_catalog);
                self.refresh_sidebar(cx);
            }
            Ok(false) => eprintln!("[projects] already tracked or nested: {}", path.display()),
            Err(error) => eprintln!("[projects] {error}"),
        }
    }

    fn remove_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.project_catalog.remove(id) {
            self.session.schedule_catalog(&self.project_catalog);
            self.refresh_sidebar(cx);
        }
    }

    /// One tab's real, live status — `None` when there is nothing yet to
    /// report (a fresh chat with no turn sent, a plain shell with no agent).
    /// A chat pane's own state is authoritative for itself; a terminal
    /// pane's comes from `self.activity`, the one `AgentActivityModel` this
    /// workspace owns. For a split tab, the highest-priority pane status wins:
    /// errors first, then any running pane, then needs-input, then done. That
    /// keeps a busy pane visible instead of letting an idle sibling hide it.
    fn tab_status(&self, tab: &OpenTab, cx: &App) -> Option<ActivityStatus> {
        let mut status = None;
        tab.panes.for_each(&mut |pane_id, content| {
            let candidate = match content {
                TabContent::Chat(chat) => {
                    let chat = chat.read(cx);
                    if chat.is_streaming() {
                        Some(ActivityStatus::Running)
                    } else if chat.has_completed_turn() {
                        Some(ActivityStatus::Done)
                    } else {
                        None
                    }
                }
                TabContent::Terminal { .. } => {
                    let pane_status = self
                        .activity
                        .status(&format!("pane-{pane_id}"))
                        .or_else(|| self.activity.status(&format!("tab-{}", tab.id)));
                    match pane_status {
                        Some(AgentStatus::Running) => Some(ActivityStatus::Running),
                        Some(AgentStatus::NeedsInput) => Some(ActivityStatus::Idle),
                        Some(AgentStatus::Done) => Some(ActivityStatus::Done),
                        Some(AgentStatus::Error) => Some(ActivityStatus::Error),
                        None => None,
                    }
                }
                TabContent::File { .. } => None,
            };
            if candidate.is_some_and(|candidate| {
                status.map_or(true, |current| Self::status_priority(candidate) < Self::status_priority(current))
            }) {
                status = candidate;
            }
        });
        status
    }

    fn status_priority(status: ActivityStatus) -> u8 {
        match status {
            ActivityStatus::Error => 0,
            ActivityStatus::Running => 1,
            ActivityStatus::Idle => 2,
            ActivityStatus::Done => 3,
        }
    }

    /// The highest-priority status among the open tabs (error > running
    /// > needs-input > done), for the worktree row's single dot — one aggregate
    /// of the same per-tab facts the tab checkmarks and Activity rows show,
    /// not an independently-decided fourth state.
    fn worktree_status(&self, cx: &App) -> Option<ActivityStatus> {
        self.tabs
            .iter()
            .filter_map(|tab| self.tab_status(tab, cx))
            .min_by_key(|status| Self::status_priority(*status))
    }

    fn activity_surfaces(&self, cx: &App) -> Vec<ActivitySurface> {
        self.tabs
            .iter()
            .map(|tab| {
                let icon = tab_icon(tab.kind, tab_has_file(tab), tab.agent_icon);
                let status = self.tab_status(tab, cx).unwrap_or(ActivityStatus::Idle);
                ActivitySurface::new(icon, tab.title.clone(), self.worktree_label.clone(), status)
            })
            .collect()
    }

    fn sync_activity(&self, cx: &mut Context<Self>) {
        let activity = self.activity_surfaces(cx);
        self.right_panel
            .update(cx, |panel, cx| panel.set_activity(activity, cx));

        let worktree_status = self.worktree_status(cx);
        // The sidebar's tab rows under this worktree are the same `tabs`
        // the tab bar and the Activity panel just rendered from above —
        // one collection, three views, so they cannot disagree the way the
        // sidebar's own fixture rows used to.
        let sidebar_tabs: Vec<SidebarTab> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| SidebarTab {
                id: tab.id,
                title: tab.title.clone(),
                selected: index == self.active_tab,
                kind: tab.kind,
                agent_icon: tab.agent_icon,
            })
            .collect();
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_worktree_status(MAIN_WORKTREE_SIDEBAR_ID, worktree_status, cx);
            sidebar.set_worktree_tabs(MAIN_WORKTREE_SIDEBAR_ID, sidebar_tabs, cx);
        });
    }

    fn seam(&self) -> impl IntoElement {
        div().w(px(SEAM_WIDTH)).h_full().bg(gpui::black())
    }

    fn tab_width(kind: TabKind) -> f32 {
        match kind {
            TabKind::AgentChat | TabKind::Editor => CHAT_TAB_WIDTH,
            TabKind::Terminal => TERMINAL_TAB_WIDTH,
            TabKind::Browser => unreachable!("Browser surfaces are external to the shell"),
            TabKind::Diff => unreachable!("Diff surfaces are inline in the Changes panel"),
        }
    }

    fn select_tab(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.active_tab = index;
            self.schedule_save();
            cx.notify();
        }
    }

    fn select_activity(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.schedule_save();
            cx.notify();
        }
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        let mut terminals = Vec::new();
        self.tabs[index].panes.for_each(&mut |_, content| {
            if let Some(terminal) = content.terminal() {
                terminals.push(terminal);
            }
        });
        for terminal in terminals {
            // TerminalView exposes PTY input but not a public shutdown method.
            // Interrupt first, then send EOF so the login shell exits and the
            // alacritty event loop observes child exit and reaps the PTY.
            terminal.update(cx, |terminal, _| terminal.input([3, 4]));
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active_tab = 0;
        } else if self.active_tab > index {
            self.active_tab -= 1;
        } else if self.active_tab == index {
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        }
        self.schedule_save();
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_chat_tab(&mut self, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let chat = cx.new(Chat::launch);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            title,
            kind: TabKind::AgentChat,
            agent_icon: None,
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Chat(chat)),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.schedule_save();
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_terminal_tab(&mut self, title: impl Into<String>, cx: &mut Context<Self>) {
        self.add_terminal_tab_with_shell(title, TerminalShell::System, None, cx);
    }

    fn add_terminal_tab_with_shell(
        &mut self,
        title: impl Into<String>,
        shell: TerminalShell,
        agent_icon: Option<Icon>,
        cx: &mut Context<Self>,
    ) {
        let working_directory = self.working_directory.clone();
        let terminal = cx.new(|cx| match TerminalView::with_shell(&working_directory, shell, cx)
        {
            Ok(view) => view,
            // A PTY can fail to fork for ordinary reasons (fd exhaustion, a
            // deleted directory, a sandbox denial): the pane shows the
            // failure and a retry button instead of aborting the app.
            Err(error) => TerminalView::failed(
                &working_directory,
                TerminalShell::System,
                format!("{error:#}"),
                cx,
            ),
        });
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            title: title.into(),
            kind: TabKind::Terminal,
            agent_icon,
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Terminal { view: terminal }),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.schedule_save();
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_file_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let title = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let view = cx.new(|cx| FileView::new(path, cx));
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            title,
            // The existing UI tab model has only chat/terminal kinds. File
            // identity stays in TabContent; the shell overlay adjusts its
            // glyph and width below without changing the menu component.
            kind: TabKind::Editor,
            agent_icon: None,
            panes: PaneNode::leaf(self.next_pane_id, TabContent::File { view }),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.schedule_save();
        self.sync_activity(cx);
        cx.notify();
    }

    /// Opens a tab running `adapter`'s agent CLI. Calls `prepare` first —
    /// that is what writes the worktree-local hook config the agent needs —
    /// then runs the resolved command through the user's login shell (`-lc`,
    /// not `-il`: one command and exit, not an interactive session), so the
    /// PTY's child is the shell running the command rather than the agent
    /// binary directly, since the command is shell syntax (`tiller_agents`
    /// embeds quoted `-c key=value` overrides) and must be parsed as such.
    fn add_agent_tab(
        &mut self,
        adapter: &dyn tiller_agents::AgentAdapter,
        agent_icon: Icon,
        cx: &mut Context<Self>,
    ) {
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        let pane_id = format!("pane-{}", self.next_pane_id);

        if let Err(error) = adapter.prepare(&worktree_path, &pane_id, "tillerctl") {
            eprintln!(
                "failed to prepare {} in {worktree_path}: {error}",
                adapter.display_name()
            );
        }

        let command = adapter.command(&worktree_path, &pane_id, "tillerctl");
        let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let shell = TerminalShell::WithArguments {
            program: shell_program,
            args: vec!["-lc".to_string(), command],
        };
        // Registers with the one AgentActivityModel this workspace owns,
        // under the same pane key `tab_status` looks up by — this is Layer
        // A's entry point (a later `tillerctl notify` push updates it).
        self.activity
            .agent_spawned(&pane_id, adapter.id(), Instant::now());
        self.add_terminal_tab_with_shell(
            adapter.display_name(),
            shell,
            Some(agent_icon),
            cx,
        );
    }

    fn open_action(&mut self, action: NewTabAction, cx: &mut Context<Self>) {
        match action {
            NewTabAction::NewChat => self.add_chat_tab("Chat", cx),
            NewTabAction::NewTerminal => self.add_terminal_tab("Terminal", cx),
            NewTabAction::ClaudeCode
            | NewTabAction::Codex
            | NewTabAction::OpenCode
            | NewTabAction::Pi
            | NewTabAction::OhMyPi => {
                let id = agent_id_for_action(action).expect("agent action has an id");
                let Some(adapter) = AGENT_CATALOG.iter().find(|adapter| adapter.id() == id)
                else {
                    eprintln!("[agent] no adapter for action id '{id}'");
                    return;
                };
                let Some(agent_icon) = agent_icon_for_action(action) else {
                    eprintln!("[agent] no icon for action id '{id}'");
                    return;
                };
                self.add_agent_tab(*adapter, agent_icon, cx);
            }
            NewTabAction::SplitClaudeCode => {
                self.split_focused_agent("claude", SplitDirection::Horizontal, None, cx);
            }
            // Browser is not part of this shell's content set yet. Keeping the
            // action typed and ignored is preferable to opening a fake pane.
            NewTabAction::NewBrowser => {}
        }
    }

    fn active_tab_mut(&mut self) -> Option<&mut OpenTab> {
        self.tabs.get_mut(self.active_tab)
    }

    fn select_pane(&mut self, pane_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut() {
            if tab.panes.contains(pane_id) {
                tab.focused_pane = pane_id;
                let mut focused_content = None;
                tab.panes.for_each(&mut |id, content| {
                    if id == pane_id {
                        focused_content = match content {
                            TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                            TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                            TabContent::File { .. } => None,
                        };
                    }
                });
                if let Some(focus_handle) = focused_content {
                    window.focus(&focus_handle, cx);
                }
                cx.notify();
            }
        }
    }

    fn focus_neighbor(
        &mut self,
        direction: SplitDirection,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.panes.neighbor(tab.focused_pane, direction, forward));
        if let Some(next) = next {
            self.select_pane(next, window, cx);
        }
    }

    fn split_focused_terminal(
        &mut self,
        direction: SplitDirection,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let pane_id = self.next_pane_id;
        let working_directory = self.working_directory.clone();
        let terminal = cx.new(|cx| {
            TerminalView::with_shell(&working_directory, TerminalShell::System, cx)
                .expect("start split terminal")
        });
        let split = {
            let Some(tab) = self.active_tab_mut() else { return };
            let split = tab.panes.split_focused(
                tab.focused_pane,
                pane_id,
                direction,
                TabContent::Terminal { view: terminal },
            );
            if split {
                tab.focused_pane = pane_id;
            }
            split
        };
        if split {
            self.next_pane_id += 1;
            if let Some(window) = window {
                self.select_pane(pane_id, window, cx);
            }
            cx.notify();
        }
    }

    fn split_focused_agent(
        &mut self,
        adapter_id: &str,
        direction: SplitDirection,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(adapter) = AGENT_CATALOG.iter().find(|adapter| adapter.id() == adapter_id) else {
            return;
        };
        let pane_id = self.next_pane_id;
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        let pane_name = format!("pane-{pane_id}");
        if let Err(error) = adapter.prepare(&worktree_path, &pane_name, "tillerctl") {
            eprintln!("failed to prepare {} in {worktree_path}: {error}", adapter.display_name());
        }
        let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let shell = TerminalShell::WithArguments {
            program: shell_program,
            args: vec!["-lc".to_string(), adapter.command(&worktree_path, &pane_name, "tillerctl")],
        };
        let terminal = cx.new(|cx| {
            TerminalView::with_shell(&worktree_path, shell, cx).expect("start split agent")
        });
        let split = {
            let Some(tab) = self.active_tab_mut() else { return };
            let split = tab.panes.split_focused(
                tab.focused_pane,
                pane_id,
                direction,
                TabContent::Terminal { view: terminal },
            );
            if split {
                tab.agent_icon = Some(
                    Icon::for_agent_id(adapter.id())
                        .expect("every catalog agent must have a brand icon"),
                );
                tab.focused_pane = pane_id;
            }
            split
        };
        if split {
            self.activity.agent_spawned(&pane_name, adapter.id(), Instant::now());
            self.next_pane_id += 1;
            if let Some(window) = window {
                self.select_pane(pane_id, window, cx);
            }
            self.sync_activity(cx);
            cx.notify();
        }
    }

    fn close_focused_pane(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab_mut() else { return };
        if tab.panes.leaf_ids().len() <= 1 {
            return;
        }
        let focused = tab.focused_pane;
        let removed = tab.panes.remove(focused);
        if let Some(removed) = removed {
            if let Some(terminal) = removed.terminal() {
                terminal.update(cx, |terminal, _| terminal.input([3, 4]));
            }
            let replacement = tab.panes.first_id().unwrap_or(focused);
            tab.focused_pane = replacement;
            if let Some(window) = window {
                let mut replacement_content = None;
                tab.panes.for_each(&mut |id, content| {
                    if id == replacement {
                        replacement_content = match content {
                            TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                            TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                            TabContent::File { .. } => None,
                        };
                    }
                });
                if let Some(focus_handle) = replacement_content {
                    window.focus(&focus_handle, cx);
                }
            }
            cx.notify();
        }
    }

    fn update_divider(
        &mut self,
        drag: &DraggedPaneDivider,
        event: &DragMoveEvent<DraggedPaneDivider>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(drag.tab_index) else { return };
        let bounds = event.bounds;
        let ratio = match drag.direction {
            SplitDirection::Horizontal => {
                (event.event.position.x - bounds.left()) / (bounds.right() - bounds.left())
            }
            SplitDirection::Vertical => {
                (event.event.position.y - bounds.top()) / (bounds.bottom() - bounds.top())
            }
        };
        if ratio.is_finite() && tab.panes.set_ratio(&drag.path, ratio) {
            cx.notify();
        }
    }

    fn render_pane_tree(
        &self,
        node: &PaneNode<TabContent>,
        tab_index: usize,
        entity: Entity<Self>,
        path: Vec<bool>,
    ) -> AnyElement {
        match node {
            PaneNode::Leaf { id, content } => {
                let Some(content) = content else {
                    return div().size_full().into_any_element();
                };
                let pane_id = *id;
                let entity_for_click = entity.clone();
                let surface = match content {
                    TabContent::Chat(chat) => div().size_full().child(chat.clone()).into_any_element(),
                    TabContent::Terminal { view } => div().size_full().child(view.clone()).into_any_element(),
                    TabContent::File { view } => div().size_full().child(view.clone()).into_any_element(),
                };
                div()
                    .id(format!("pane-{pane_id}"))
                    .relative()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        entity_for_click.update(cx, |workspace, cx| workspace.select_pane(pane_id, window, cx));
                    })
                    .child(surface)
                    .into_any_element()
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_path = {
                    let mut path = path.clone();
                    path.push(false);
                    path
                };
                let second_path = {
                    let mut path = path.clone();
                    path.push(true);
                    path
                };
                let first_element = self.render_pane_tree(first, tab_index, entity.clone(), first_path);
                let second_element = self.render_pane_tree(second, tab_index, entity.clone(), second_path);
                let first_style = |element| {
                    div()
                        .flex_shrink_1()
                        .min_w_0()
                        .min_h_0()
                        .flex_basis(DefiniteLength::Fraction(*ratio))
                        .child(element)
                };
                let second_style = |element| {
                    div()
                        .flex_shrink_1()
                        .min_w_0()
                        .min_h_0()
                        .flex_basis(DefiniteLength::Fraction(1. - *ratio))
                        .child(element)
                };
                let drag_entity = entity.clone();
                let divider_drag = DraggedPaneDivider {
                    tab_index,
                    path: path.clone(),
                    direction: *direction,
                };
                let drag = div()
                    .id(format!("pane-divider-{path:?}"))
                    .flex_shrink_0()
                    .relative()
                    .bg(gpui::black())
                    .when(*direction == SplitDirection::Horizontal, |this| {
                        this.w(px(1.)).h_full().child(
                            div()
                                .id(format!("pane-divider-h-handle-{path:?}"))
                                .absolute()
                                .left(px(-4.))
                                .w(px(9.))
                                .h_full()
                                .cursor_col_resize()
                                .on_drag(divider_drag.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty)),
                        )
                    })
                    .when(*direction == SplitDirection::Vertical, |this| {
                        this.h(px(1.)).w_full().child(
                            div()
                                .id(format!("pane-divider-v-handle-{path:?}"))
                                .absolute()
                                .top(px(-4.))
                                .h(px(9.))
                                .w_full()
                                .cursor_row_resize()
                                .on_drag(divider_drag, |_, _, _, cx| cx.new(|_| gpui::Empty)),
                        )
                    });
                let drag_entity_move = drag_entity.clone();
                let drag_entity_drop = drag_entity;
                div()
                    .id(format!("pane-split-{path:?}"))
                    .size_full()
                    .when(*direction == SplitDirection::Horizontal, |this| this.flex().flex_row())
                    .when(*direction == SplitDirection::Vertical, |this| this.flex().flex_col())
                    .on_drag_move::<DraggedPaneDivider>(move |event, _, cx| {
                        let drag = event.drag(cx).clone();
                        drag_entity_move.update(cx, |workspace, cx| workspace.update_divider(&drag, event, cx));
                    })
                    .on_drop::<DraggedPaneDivider>(move |_, _, cx| {
                        drag_entity_drop.update(cx, |_, cx| cx.notify());
                    })
                    .child(first_style(first_element))
                    .child(drag)
                    .child(second_style(second_element))
                    .into_any_element()
            }
        }
    }

    fn bind_pane_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-alt-left", FocusPaneLeft, None),
            KeyBinding::new("cmd-alt-right", FocusPaneRight, None),
            KeyBinding::new("cmd-alt-up", FocusPaneAbove, None),
            KeyBinding::new("cmd-alt-down", FocusPaneBelow, None),
            KeyBinding::new("cmd-alt-shift-right", SplitPaneRight, None),
            KeyBinding::new("cmd-alt-shift-down", SplitPaneDown, None),
            KeyBinding::new("cmd-alt-w", ClosePane, None),
        ]);
    }

    fn render_open_tab(
        tab: &OpenTab,
        active: bool,
        status: Option<ActivityStatus>,
        entity: Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let id = tab.id;
        let is_file = tab_has_file(tab);
        let icon = tab_icon(tab.kind, is_file, tab.agent_icon);
        // Agent marks follow the primary text colour (the Swift app renders
        // them with `.primary`); chromatic marks ignore this entirely and
        // keep their brand colours. Everything else keeps the reference
        // tinting.
        let glyph_color = if icon.is_agent_mark() {
            theme.title
        } else if tab.kind == TabKind::AgentChat {
            if is_file { theme.file_link } else { gpui::rgb(0xca7250) }
        } else {
            theme.meta
        };
        let width = if is_file { 180.0 } else { Self::tab_width(tab.kind) };
        let close_entity = entity.clone();
        div()
            .id(format!("workspace-tab-{id}"))
            .relative()
            .mt(px(4.0))
            .h(px(30.0))
            .w(px(width))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .rounded_t(px(6.0))
            .text_size(px(12.0))
            .text_color(if active {
                theme.title_selected
            } else {
                theme.subtitle
            })
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.select_tab(id, cx)))
            .child(
                div()
                    .w(px(14.0))
                    .text_color(glyph_color)
                    .child(IconElement::new(icon, px(14.0))),
            )
            .child(
                div()
                    .font_weight(FontWeight::NORMAL)
                    .child(tab.title.clone()),
            )
            .child(div().w(px(16.0)).when(
                status == Some(ActivityStatus::Done),
                |this| this.text_color(theme.tab_done).child("✓"),
            ))
            .when(active, |this| {
                this.child(
                    div()
                        .id(format!("workspace-tab-close-{id}"))
                        .w(px(14.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.0))
                        .text_color(theme.subtitle)
                        .hover(|style| style.bg(theme.row_hover).rounded(px(4.0)))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            close_entity.update(cx, |this, cx| this.close_tab_by_id(id, cx));
                        })
                        .child(IconElement::new(Icon::Close, px(12.0)).text_color(theme.subtitle)),
                )
            })
            .when(active, |this| {
                this.bg(theme.selection_fill).child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left_0()
                        .right_0()
                        .h(px(2.0))
                        .bg(theme.tab_focus_accent),
                )
            })
    }

    fn close_tab_by_id(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.close_tab(index, cx);
        }
    }

    /// This shell overlay owns the visible tabs. The UI crate's TabBar remains
    /// underneath only for its typed + menu implementation; covering the full
    /// tab area prevents its fixture rows from leaking through after a close.
    fn render_open_tabs(&self, theme: Theme, entity: Entity<Self>, cx: &App) -> impl IntoElement {
        let mut tabs = div()
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .right(px(26.0))
            .pl(px(5.0))
            .flex()
            .items_start()
            .gap(px(1.0))
            .bg(theme.background);
        for (index, tab) in self.tabs.iter().enumerate() {
            tabs = tabs.child(Self::render_open_tab(
                tab,
                index == self.active_tab,
                self.tab_status(tab, cx),
                entity.clone(),
                theme,
            ));
        }
        tabs
    }

    /// The three columns. Content entities are mounted selectively, while
    /// their owning entities remain in `tabs` above.
    fn columns(&self, theme: &Theme, entity: Entity<Self>, cx: &App) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .child(
                div()
                    .w(px(SIDEBAR_WIDTH))
                    .h_full()
                    .bg(theme.canvas)
                    .child(self.sidebar.clone()),
            )
            .child(self.seam())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .h_full()
                    .bg(theme.background)
                    .child(
                        div()
                            .relative()
                            .h(px(TAB_BAR_HEIGHT))
                            .w_full()
                            .child(self.tab_bar.clone())
                            .child(self.render_open_tabs(*theme, entity.clone(), cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .w_full()
                            .overflow_hidden()
                            .when_some(self.tabs.get(self.active_tab), |this, tab| {
                                let has_terminal = tab_has_terminal(tab);
                                this.child(self.render_pane_tree(
                                    &tab.panes,
                                    self.active_tab,
                                    entity,
                                    Vec::new(),
                                ))
                                .when(has_terminal, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .top(px(0.0))
                                            .right(px(10.0))
                                            .h(px(24.0))
                                            .px(px(8.0))
                                            .flex()
                                            .items_center()
                                            .bg(theme.background)
                                            .text_size(px(13.0))
                                            .text_color(theme.title)
                                            .child(self.terminal_breadcrumb.clone()),
                                    )
                                })
                            }),
                    ),
            )
            .child(self.seam())
            .child(
                div()
                    .w(px(RIGHT_PANEL_WIDTH))
                    .h_full()
                    .bg(theme.background)
                    .child(self.right_panel.clone()),
            )
    }
}

impl TillerWorkspace {
    fn handle_focus_pane_left(&mut self, _: &FocusPaneLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_neighbor(SplitDirection::Horizontal, false, window, cx);
    }

    fn handle_focus_pane_right(&mut self, _: &FocusPaneRight, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_neighbor(SplitDirection::Horizontal, true, window, cx);
    }

    fn handle_focus_pane_above(&mut self, _: &FocusPaneAbove, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_neighbor(SplitDirection::Vertical, false, window, cx);
    }

    fn handle_focus_pane_below(&mut self, _: &FocusPaneBelow, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_neighbor(SplitDirection::Vertical, true, window, cx);
    }

    fn handle_split_pane_right(&mut self, _: &SplitPaneRight, window: &mut Window, cx: &mut Context<Self>) {
        self.split_focused_terminal(SplitDirection::Horizontal, Some(window), cx);
    }

    fn handle_split_pane_down(&mut self, _: &SplitPaneDown, window: &mut Window, cx: &mut Context<Self>) {
        self.split_focused_terminal(SplitDirection::Vertical, Some(window), cx);
    }

    fn handle_close_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
        self.close_focused_pane(Some(window), cx);
    }
}

impl Render for TillerWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Fetched fresh every frame from the global, so a change of appearance
        // is picked up without the workspace holding a stale copy.
        let theme = *Theme::get(cx);

        // `Chat`'s streaming/completed state changes on its own schedule (an
        // ACP event arriving), not through any of this workspace's own
        // mutation methods, so the sidebar dot and Activity row would go
        // stale after a turn finished unless this re-derives them every
        // render, same as the tab checkmark already does inline in
        // `render_open_tab`. `set_activity`/`set_worktree_status` both no-op
        // on an unchanged value, so this doesn't loop.
        self.sync_activity(cx);

        if self.show_settings {
            return div()
                .flex()
                .flex_col()
                .size_full()
                .bg(theme.canvas)
                .child(
                    div()
                        .h(px(TITLE_BAR_HEIGHT))
                        .w_full()
                        .child(self.titlebar.clone()),
                )
                .child(div().flex_1().w_full().child(self.settings.clone()));
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .on_action(cx.listener(Self::handle_focus_pane_left))
            .on_action(cx.listener(Self::handle_focus_pane_right))
            .on_action(cx.listener(Self::handle_focus_pane_above))
            .on_action(cx.listener(Self::handle_focus_pane_below))
            .on_action(cx.listener(Self::handle_split_pane_right))
            .on_action(cx.listener(Self::handle_split_pane_down))
            .on_action(cx.listener(Self::handle_close_pane))
            .child(
                div()
                    .h(px(TITLE_BAR_HEIGHT))
                    .w_full()
                    .child(self.titlebar.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .child(self.columns(&theme, cx.entity(), cx)),
            )
            .child(
                div()
                    .h(px(STATUS_BAR_HEIGHT))
                    .w_full()
                    .child(self.status_bar.clone()),
            )
    }
}

fn sidebar_projects(catalog: &ProjectCatalog) -> Vec<SidebarProject> {
    catalog
        .projects()
        .iter()
        .map(|project| SidebarProject {
            id: project.id.clone(),
            name: project.name.clone(),
            is_git: project.is_git,
            root_path: project.root_path.clone(),
            worktrees: project
                .worktrees
                .iter()
                .map(|worktree| SidebarWorktree {
                    branch: worktree.branch.clone(),
                    path: worktree.path.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Starts in the nearest repository when launched from one of its subdirectories.
fn initial_working_directory() -> PathBuf {
    let current = std::env::current_dir().unwrap_or_else(|error| {
        eprintln!("[tiller] cannot read the current directory: {error}");
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    current
        .ancestors()
        .find(|path| is_git_repository(path))
        .map(PathBuf::from)
        .unwrap_or(current)
}

/// Rebuilds the shell's tabs from a restored session: chat tabs get a fresh
/// `Chat` entity (identity, not transcript), terminal tabs get a fresh
/// terminal in the restored worktree directory (scrollback is expected to be
/// gone). Returns the tabs and the active index.
fn restore_tabs(
    restored: &RestoredSession,
    working_directory: &std::path::Path,
    cx: &mut App,
) -> (Vec<OpenTab>, usize) {
    let mut tabs = Vec::new();
    let mut active = 0usize;
    for tab in &restored.tabs {
        let id = tabs.len();
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(Chat::launch)),
            "terminal" => {
                let cwd = working_directory.to_path_buf();
                let view = cx.new(|cx| match TerminalView::new(&cwd, cx) {
                    Ok(view) => view,
                    Err(error) => TerminalView::failed(
                        &cwd,
                        TerminalShell::System,
                        format!("{error:#}"),
                        cx,
                    ),
                });
                TabContent::Terminal { view }
            }
            "file" => {
                // File tabs are not restored yet: their source may disappear
                // between launches. Session restoration skips them rather
                // than opening a stale or missing document.
                continue;
            }
            // restore() only returns chat and terminal tabs.
            _ => unreachable!("unexpected restored tab kind {}", tab.kind),
        };
        let pane_id = id;
        tabs.push(OpenTab {
            id,
            title: tab.title.clone(),
            kind: match tab.kind.as_str() {
                "chat" => TabKind::AgentChat,
                _ => TabKind::Terminal,
            },
            agent_icon: None,
            panes: PaneNode::leaf(pane_id, content),
            focused_pane: pane_id,
        });
        if tab.active {
            active = id;
        }
    }
    (tabs, active)
}

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);

        // Restore the stored layout; a missing, corrupt or newer-schema
        // database logs and falls back to the default layout — the app must
        // never refuse to open because of its own state file.
        let database_path = session::database_path();
        // The working directory can vanish between launches (a terminal
        // whose cwd was deleted): fall back rather than abort before the
        // window exists.
        let fallback_directory = initial_working_directory();
        let restored = session::restore(&database_path, &fallback_directory);
        for diagnostic in &restored.diagnostics {
            eprintln!("[session] {diagnostic}");
        }
        let working_directory = restored.working_directory.clone();
        let restored_catalog = session::restore_catalog(&database_path);
        for diagnostic in &restored_catalog.diagnostics {
            eprintln!("[session] {diagnostic}");
        }
        let mut project_catalog = ProjectCatalog::from_projects(restored_catalog.projects);
        if project_catalog.projects().is_empty() {
            let _ = project_catalog.add(&working_directory);
        }
        let session_store = SessionStore::open(&database_path);
        let context = worktree_context(&project_catalog, &working_directory);
        let status_data = UsageBarData {
            branch: context.branch.clone(),
            path: context.path.clone(),
        };
        let activity_label = context.activity_label.clone();
        let terminal_breadcrumb = context.terminal_breadcrumb.clone();

        let bounds = Bounds::centered(None, size(px(1470.), px(833.)), cx);
        let pending_actions = Arc::new(Mutex::new(Vec::<WorkspaceAction>::new()));
        let pending_for_tab_bar = pending_actions.clone();
        let pending_for_status_bar = pending_actions.clone();
        let pending_for_settings = pending_actions.clone();
        let session_store_for_window = session_store.clone();

        let window_result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(12.))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| {
                let (tabs, active_tab) = restore_tabs(&restored, &working_directory, cx);
                let activity = tabs
                    .iter()
                    .map(|tab| {
                        let icon = tab_icon(tab.kind, tab_has_file(tab), tab.agent_icon);
                        ActivitySurface::new(
                            icon,
                            tab.title.clone(),
                            activity_label.clone(),
                            ActivityStatus::Idle,
                        )
                    })
                    .collect();
                let catalog_for_sidebar = sidebar_projects(&project_catalog);
                let tab_bar = cx.new(|cx| {
                    TabBar::new(cx).on_new_tab(move |action| {
                        if let Ok(mut actions) = pending_for_tab_bar.lock() {
                            actions.push(WorkspaceAction::NewTab(action));
                        }
                    })
                });
                let status_bar = cx.new(|_| {
                    StatusBar::new(status_data.clone()).on_settings(move || {
                        if let Ok(mut actions) = pending_for_status_bar.lock() {
                            actions.push(WorkspaceAction::OpenSettings);
                        }
                    })
                });
                let settings = cx.new(|cx| {
                    Settings::new(cx).on_back(move || {
                        if let Ok(mut actions) = pending_for_settings.lock() {
                            actions.push(WorkspaceAction::CloseSettings);
                        }
                    })
                });
                cx.new(|cx| {
                    TillerWorkspace::new(
                        cx.new(Titlebar::new),
                        cx.new(|cx| Sidebar::from_projects(catalog_for_sidebar, cx)),
                        tab_bar,
                        status_bar,
                        settings,
                        cx.new(|_| {
                            RightPanel::with_activity(
                                working_directory.to_string_lossy().into_owned(),
                                activity,
                            )
                        }),
                        tabs,
                        active_tab,
                        working_directory.clone(),
                        pending_actions.clone(),
                        session_store_for_window,
                        project_catalog,
                        activity_label.clone(),
                        terminal_breadcrumb.clone(),
                        cx,
                    )
                })
            },
        );
        if let Err(error) = window_result {
            eprintln!("[tiller] failed to open the main window: {error:#}");
            std::process::exit(1);
        }

        // Flush the pending layout when the window closes, so quitting never
        // loses the last change to the debounce window.
        let session_store_for_close = session_store.clone();
        cx.on_window_closed(move |_, _| session_store_for_close.flush_now())
            .detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn agent_tab_created_from_id_keeps_the_catalog_brand_icon() {
        let expected = [
            ("claude", Icon::ClaudeCode),
            ("codex", Icon::Codex),
            ("opencode", Icon::OpenCode),
            ("pi", Icon::Pi),
            ("omp", Icon::OhMyPi),
        ];

        for (agent_id, icon) in expected {
            assert_eq!(
                Icon::for_agent_id(agent_id),
                Some(icon),
                "agent tab id {agent_id} must resolve to its brand icon"
            );
            assert_eq!(
                tab_icon(TabKind::Terminal, false, Some(icon)),
                icon,
                "a terminal tab created for {agent_id} must retain its brand icon"
            );
        }

        assert_eq!(agent_icon_for_action(NewTabAction::ClaudeCode), Some(Icon::ClaudeCode));
        assert_eq!(agent_icon_for_action(NewTabAction::Codex), Some(Icon::Codex));
        assert_eq!(agent_icon_for_action(NewTabAction::OpenCode), Some(Icon::OpenCode));
        assert_eq!(agent_icon_for_action(NewTabAction::Pi), Some(Icon::Pi));
        assert_eq!(agent_icon_for_action(NewTabAction::OhMyPi), Some(Icon::OhMyPi));
        assert_eq!(agent_icon_for_action(NewTabAction::NewTerminal), None);
        assert_eq!(agent_icon_for_action(NewTabAction::NewChat), None);
    }

    fn missing_directory(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tiller-restore-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    /// Restoring a session whose terminal tab points at a directory that no
    /// longer exists must degrade per item: the workspace still opens with
    /// every tab present, and the bad terminal renders as a failed pane —
    /// never a process abort. This is the launch path that runs BEFORE the
    /// window shows, so a panic here would brick the app for good.
    #[gpui::test]
    async fn restore_survives_a_terminal_tab_in_a_missing_directory(
        cx: &mut TestAppContext,
    ) {
        // A session saved while a worktree existed; the worktree is gone now.
        let working_directory = missing_directory("layout");
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![
                session::SessionTab {
                    title: "Chat".into(),
                    kind: "chat".into(),
                    active: false,
                },
                session::SessionTab {
                    title: "Terminal".into(),
                    kind: "terminal".into(),
                    active: true,
                },
            ],
            diagnostics: vec![],
        };

        let (tabs, active) = cx.update(|cx| restore_tabs(&restored, &working_directory, cx));

        assert_eq!(tabs.len(), 2, "the chat tab AND the bad terminal tab are both present");
        assert_eq!(active, 1, "the terminal tab keeps its active slot");
        let (failed, live) = cx.update(|cx| {
            let mut failed = 0usize;
            let mut live = 0usize;
            for tab in &tabs {
                // A tab now holds a tree of panes rather than one surface, so
                // count every leaf: a split tab has more than one terminal.
                tab.panes.for_each(&mut |_, content| match content {
                    TabContent::Chat(_) => {}
                    TabContent::Terminal { view } => {
                        if view.read(cx).is_failed() {
                            failed += 1;
                        } else {
                            live += 1;
                        }
                    }
                    TabContent::File { .. } => {}
                });
            }
            (failed, live)
        });
        assert_eq!(failed, 1, "the terminal pane is a visible failed pane");
        assert_eq!(live, 0);
        // (Rendering the failed pane headlessly is covered by the terminal
        // crate's own gpui test: `failed_pane_renders_and_retry_recovers`.)
    }
}

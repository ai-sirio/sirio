use gpui::{
    AnyElement, App, Bounds, Context, DefiniteLength, DragMoveEvent, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement, KeyBinding, KeyDownEvent, MouseButton,
    PathPromptOptions, PromptLevel, Render, StatefulInteractiveElement, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, point, prelude::*, px, size,
};
use gpui_platform::application;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiller_acp::{AgentCommand, ChatSession, ChatSessionConfig, ChatSnapshot};
use tiller_activity::{
    AgentActivityModel, AgentStatus, NotificationPayload, NotificationPolicy, Transition,
};
use tiller_agents::ALL as AGENT_CATALOG;
use tiller_control::{
    ControlHandler, ControlRequest, ControlResponse, ControlServer, PaneError, PaneExitStatus,
    PaneInfo, PaneRegistry, PaneStateSnapshot, base64_encode,
};
use tiller_git::{GitError, discard, discard_all, init_repository, stage, stage_all, unstage};
use tiller_persistence::{
    AppDatabase, AppSettings, AppearanceMode, ChatEntry, ChatPermissionOutcome, FileIconTheme,
    TabRecord,
};
use tiller_project::{TabKind, current_branch, is_git_repository};
use tiller_terminal::{
    TerminalActivityEvent, TerminalContextAction, TerminalContextEvent, TerminalExitStatus,
    TerminalIdentity, TerminalLinkEvent, TerminalShell, TerminalStateSnapshot, TerminalView,
};
use tiller_theme::{Theme, ThemeMode};
use tiller_ui::{
    browser::{BrowserEvent, BrowserSurface},
    changes::{ChangesReport, ChangesTab, ChangesTabActionEvent, ChangesTabEvent},
    chat::{Chat, acp_agent_command},
    file_view::FileView,
    right_panel::{
        ActivityStatus, ActivitySurface, RightPanel, RightPanelActionEvent, RightPanelEvent,
    },
    row_reorder::{ReorderScope, RowDrag},
    settings::{Settings, SettingsCategory, SettingsReport, SettingsSnapshot},
    sidebar::{
        Sidebar, SidebarContextAction, SidebarContextTarget, SidebarEvent, SidebarProject,
        SidebarTab, SidebarWorktree, TAB_ROW_ID_OFFSET,
        icons::{Icon, IconElement},
    },
    status_bar::{StatusBar, UsageBarData},
    tab_bar::{NewTabAction, TabBar, TabContextAction, TabContextItem, render_tab_context_menu},
    titlebar::{Titlebar, TitlebarEvent},
};

mod command_palette;
mod panes;
mod session;
mod tab_machinery;

use command_palette::{
    EMPTY_RESULT_LABEL, PaletteCommand, PaletteContext, SidebarPaletteAction, SidebarPaletteTarget,
    TabCommand, entries as palette_entries, filter_entries as filter_palette_entries,
};
use panes::{
    CloseOtherTabs, ClosePane, CloseTab, CloseTabsToRight, CycleTabBackward, CycleTabForward,
    FocusPaneAbove, FocusPaneBelow, FocusPaneLeft, FocusPaneRight, JumpToTab1, JumpToTab2,
    JumpToTab3, JumpToTab4, JumpToTab5, JumpToTab6, JumpToTab7, JumpToTab8, JumpToTab9,
    MoveTabEarlier, MoveTabLater, MoveTabToCurrentPane, MoveTabToOtherPane, OpenAllTabs,
    OpenTabMenu, PaneContent as TabContent, PaneNode, ResumeChat, SplitDirection, SplitPaneDown,
    SplitPaneRight, SplitPlacement, TabSelection,
};
use session::{
    PaneEvent, ProjectCatalog, RestoredSession, SessionLayout, SessionStore, SessionTab,
    SessionTabState,
};
use tab_machinery::{MoveDirection, MoveTarget, TabGroup, TabMachinery, visible_tab_count};

actions!(
    window_commands,
    [
        NewTerminalTab,
        OpenFile,
        SaveFile,
        ToggleSidebar,
        ToggleRightPanel,
    ]
);

// P58, F-SET-02: the shell's Escape closes the settings surface. The
// binding is global (no key context), so it fires regardless of what holds
// focus — including the stale focus left behind when the settings surface
// replaced the main frame. Scoped bindings (the summarizer picker menu, the
// chat composer) resolve first when their context is focused, so those keep
// consuming Escape before the shell ever sees it.
actions!(shell_settings, [CloseSettingsSurface, OpenSettingsShortcut]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowCommand {
    NewTerminalTab,
    OpenFile,
    SaveFile,
    ToggleSidebar,
    ToggleRightPanel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowCommandDisabledReason {
    NoActiveFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowCommandAvailability {
    Enabled,
    Disabled(WindowCommandDisabledReason),
}

fn linux_window_shortcuts() -> [(WindowCommand, &'static str); 5] {
    [
        (WindowCommand::NewTerminalTab, "ctrl-t"),
        (WindowCommand::OpenFile, "ctrl-o"),
        (WindowCommand::SaveFile, "ctrl-s"),
        (WindowCommand::ToggleSidebar, "ctrl-shift-s"),
        (WindowCommand::ToggleRightPanel, "ctrl-shift-i"),
    ]
}

fn window_command_availability(
    command: WindowCommand,
    active_tab_kind: Option<TabKind>,
) -> WindowCommandAvailability {
    match command {
        WindowCommand::SaveFile if active_tab_kind != Some(TabKind::Editor) => {
            WindowCommandAvailability::Disabled(WindowCommandDisabledReason::NoActiveFile)
        }
        WindowCommand::NewTerminalTab
        | WindowCommand::OpenFile
        | WindowCommand::SaveFile
        | WindowCommand::ToggleSidebar
        | WindowCommand::ToggleRightPanel => WindowCommandAvailability::Enabled,
    }
}

fn bind_window_keys(cx: &mut App) {
    cx.bind_keys(
        linux_window_shortcuts()
            .into_iter()
            .map(|(command, shortcut)| match command {
                WindowCommand::NewTerminalTab => KeyBinding::new(shortcut, NewTerminalTab, None),
                WindowCommand::OpenFile => KeyBinding::new(shortcut, OpenFile, None),
                WindowCommand::SaveFile => KeyBinding::new(shortcut, SaveFile, None),
                WindowCommand::ToggleSidebar => KeyBinding::new(shortcut, ToggleSidebar, None),
                WindowCommand::ToggleRightPanel => {
                    KeyBinding::new(shortcut, ToggleRightPanel, None)
                }
            })
            // F-SET-02: Escape closes the settings surface. Global (no key
            // context) on purpose — it must fire even when the surface
            // replaced the previously focused element; scoped bindings
            // (composer, picker menus) resolve first when focused.
            .chain(std::iter::once(KeyBinding::new(
                "escape",
                CloseSettingsSurface,
                None,
            )))
            .chain(std::iter::once(KeyBinding::new(
                "ctrl-,",
                OpenSettingsShortcut,
                None,
            )))
            .collect::<Vec<_>>(),
    );
}

/// Column geometry, measured off the frozen reference shots — see
/// `reference/MEASURED.md`. The seam between columns is 1pt of pure black; at
/// 2pt, or tinted, or offset by a point, it is visible.
const SIDEBAR_WIDTH: f32 = 325.;
const RIGHT_PANEL_WIDTH: f32 = 405.;
const SEAM_WIDTH: f32 = 6.;
const TITLE_BAR_HEIGHT: f32 = 32.;
const STATUS_BAR_HEIGHT: f32 = 40.;
const TAB_BAR_HEIGHT: f32 = 34.;
const CHAT_TAB_WIDTH: f32 = 108.;
const TERMINAL_TAB_WIDTH: f32 = 132.;

const CONTROL_ACTION_TIMEOUT: Duration = Duration::from_secs(5);
const BROWSER_METHODS: [&str; 10] = [
    "browser.open",
    "browser.navigate",
    "browser.get",
    "browser.screenshot",
    "browser.snapshot",
    "browser.act",
    "browser.wait",
    "browser.eval",
    "browser.console",
    "browser.errors",
];
const BROWSER_CAPABILITIES: [&str; 3] = ["browser.open", "browser.navigate", "browser.act"];

type ControlReply = Sender<Result<Vec<(String, String)>, String>>;

fn browser_request_error(method: &str, params: &BTreeMap<String, String>) -> Option<String> {
    if !BROWSER_CAPABILITIES.contains(&method) {
        return Some(format!(
            "{method} is unsupported on Linux: browser automation is not implemented"
        ));
    }

    match method {
        "browser.open"
            if params
                .get("url")
                .or_else(|| params.get("address"))
                .is_none_or(|url| url.trim().is_empty()) =>
        {
            Some("browser.open requires a non-empty url".to_string())
        }
        "browser.navigate" if params.contains_key("action") => Some(format!(
            "{method} action is unsupported on Linux: only URL navigation is implemented"
        )),
        "browser.navigate"
            if params
                .get("url")
                .or_else(|| params.get("address"))
                .or_else(|| params.get("href"))
                .is_none_or(|url| url.trim().is_empty()) =>
        {
            Some("browser.navigate requires a non-empty url".to_string())
        }
        "browser.act"
            if !params.contains_key("driving") && !params.contains_key("agentDriving") =>
        {
            Some(
                "browser.act is unsupported on Linux: only the driving flag is implemented"
                    .to_string(),
            )
        }
        _ => None,
    }
}

enum PaneQuery {
    State,
    Scrollback(Option<usize>),
}

enum ControlAction {
    Quit {
        reply: ControlReply,
    },
    Notify {
        pane_id: String,
        status: AgentStatus,
    },
    SelectWorktree {
        selector: String,
        reply: ControlReply,
    },
    AddProject {
        path: PathBuf,
        reply: ControlReply,
    },
    CreateWorkspace {
        project: String,
        branch: Option<String>,
        reply: ControlReply,
    },
    CloseWorkspace {
        selector: String,
        reply: ControlReply,
    },
    RestoreSession {
        reply: ControlReply,
    },
    OpenChanges {
        worktree: Option<String>,
        reply: ControlReply,
    },
    ReadChanges {
        reply: ControlReply,
    },
    OpenSettings {
        section: Option<SettingsCategory>,
        reply: ControlReply,
    },
    SelectSettings {
        section: SettingsCategory,
        reply: ControlReply,
    },
    ReadSettings {
        reply: ControlReply,
    },
    ReadPane {
        id: String,
        query: PaneQuery,
        reply: ControlReply,
    },
    FocusPane {
        direction: SplitDirection,
        forward: bool,
        reply: ControlReply,
    },
    SplitPane {
        direction: SplitDirection,
        reply: ControlReply,
    },
    ClosePane {
        reply: ControlReply,
    },
    CycleTab {
        forward: bool,
        reply: ControlReply,
    },
    SelectTab {
        position: usize,
        reply: ControlReply,
    },
    Browser {
        method: String,
        params: BTreeMap<String, String>,
        reply: ControlReply,
    },
}
struct OpenTab {
    id: usize,
    group_id: usize,
    title: String,
    kind: TabKind,
    /// The agent brand shown for an agent-backed terminal tab. `None` means
    /// the surface kind decides the icon (chat, terminal, or file).
    agent_icon: Option<Icon>,
    /// Stable adapter identity for an agent-backed terminal.
    agent_id: Option<String>,
    session_state: SessionTabState,
    panes: PaneNode<TabContent>,
    focused_pane: usize,
}

struct TabRename {
    tab_id: usize,
    draft: String,
    focus: FocusHandle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedChat {
    id: usize,
    title: String,
    transcript: String,
    agent_id: Option<String>,
}

enum WorkspaceAction {
    NewTab(NewTabAction),
    NewChatAgent(&'static str),
    InstallSkill(tiller_project::SkillInstallCommand),
    OpenSettings,
    CloseSettings,
}

#[derive(Clone)]
struct ControlWorkspace {
    id: String,
    project: String,
    branch: String,
    path: String,
    selected: bool,
    mounted: bool,
    /// Runtime annotation from `worktree.set`; intentionally not persisted.
    comment: String,
    /// Runtime pane/session association from `worktree.set`; intentionally
    /// not persisted and rebuilt empty when the app restarts.
    session: Option<String>,
}

#[derive(Clone)]
struct ControlState {
    projects: Vec<session::CatalogProject>,
    workspaces: Vec<ControlWorkspace>,
    current: Option<usize>,
}

impl ControlState {
    fn from_catalog(catalog: &ProjectCatalog, working_directory: &Path) -> Self {
        let working_directory = working_directory
            .canonicalize()
            .unwrap_or_else(|_| working_directory.to_path_buf());
        let mut workspaces = Vec::new();
        let mut selected_path_claimed = false;
        for project in catalog.projects() {
            for (index, worktree) in project.worktrees.iter().enumerate() {
                let path = worktree.path.to_string_lossy().into_owned();
                let selected = !selected_path_claimed && worktree.path == working_directory;
                selected_path_claimed |= selected;
                workspaces.push(ControlWorkspace {
                    id: format!("{}-wt-{index}", project.id),
                    project: project.name.clone(),
                    branch: worktree.branch.clone(),
                    selected,
                    path,
                    mounted: true,
                    comment: String::new(),
                    session: None,
                });
            }
        }
        let current = workspaces.iter().position(|worktree| worktree.selected);
        Self {
            projects: catalog.projects().to_vec(),
            workspaces,
            current,
        }
    }

    fn project_rows(&self) -> Vec<BTreeMap<String, String>> {
        self.projects
            .iter()
            .map(|project| {
                let worktrees: Vec<BTreeMap<String, String>> = project
                    .worktrees
                    .iter()
                    .enumerate()
                    .map(|(index, worktree)| {
                        BTreeMap::from([
                            ("id".to_string(), format!("{}-wt-{index}", project.id)),
                            ("branch".to_string(), worktree.branch.clone()),
                            (
                                "path".to_string(),
                                worktree.path.to_string_lossy().into_owned(),
                            ),
                            ("primary".to_string(), worktree.is_primary.to_string()),
                        ])
                    })
                    .collect();
                BTreeMap::from([
                    ("id".to_string(), project.id.clone()),
                    ("name".to_string(), project.name.clone()),
                    (
                        "path".to_string(),
                        project.root_path.to_string_lossy().into_owned(),
                    ),
                    ("isGit".to_string(), project.is_git.to_string()),
                    (
                        "worktreeCount".to_string(),
                        project.worktrees.len().to_string(),
                    ),
                    (
                        "empty".to_string(),
                        project.worktrees.is_empty().to_string(),
                    ),
                    (
                        "worktrees".to_string(),
                        tiller_control::protocol::rows::encode(&worktrees),
                    ),
                ])
            })
            .collect()
    }

    fn workspace_rows(&self) -> Vec<BTreeMap<String, String>> {
        self.workspaces
            .iter()
            .map(|workspace| {
                BTreeMap::from([
                    ("id".to_string(), workspace.id.clone()),
                    ("project".to_string(), workspace.project.clone()),
                    ("branch".to_string(), workspace.branch.clone()),
                    ("path".to_string(), workspace.path.clone()),
                    ("selected".to_string(), workspace.selected.to_string()),
                    ("mounted".to_string(), workspace.mounted.to_string()),
                    ("comment".to_string(), workspace.comment.clone()),
                ])
            })
            .collect()
    }

    fn current_workspace(&self) -> Option<&ControlWorkspace> {
        self.current.and_then(|index| self.workspaces.get(index))
    }

    fn path_for_selector(&self, selector: &str) -> Option<PathBuf> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == selector || workspace.path == selector)
            .map(|workspace| PathBuf::from(&workspace.path))
    }

    fn select_worktree(&mut self, path: &Path) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|workspace| Path::new(&workspace.path) == path)
        else {
            return false;
        };

        self.current = Some(index);
        for (workspace_index, workspace) in self.workspaces.iter_mut().enumerate() {
            workspace.selected = workspace_index == index;
        }
        self.workspaces[index].mounted = true;
        true
    }

    fn close_worktree(&mut self, path: &Path) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|workspace| Path::new(&workspace.path) == path)
        else {
            return false;
        };

        self.workspaces[index].mounted = false;
        self.workspaces[index].selected = false;
        if self.current == Some(index) {
            self.current = None;
        }
        true
    }

    fn set_worktree(
        &mut self,
        selector: &str,
        comment: Option<&str>,
        session: Option<&str>,
    ) -> Option<ControlWorkspace> {
        let index = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == selector || workspace.path == selector)?;
        let workspace = &mut self.workspaces[index];
        if let Some(comment) = comment {
            workspace.comment = comment.to_string();
        }
        if let Some(session) = session {
            workspace.session = Some(session.to_string());
        }
        Some(workspace.clone())
    }

    fn working_directory(&self, selector: Option<&str>) -> Option<PathBuf> {
        match selector {
            Some(selector) => self
                .workspaces
                .iter()
                .find(|workspace| {
                    workspace.mounted && (workspace.id == selector || workspace.path == selector)
                })
                .map(|workspace| PathBuf::from(&workspace.path)),
            None => self
                .current_workspace()
                .map(|workspace| PathBuf::from(&workspace.path)),
        }
    }
}

#[derive(Clone)]
struct ControlNotification {
    date: String,
    title: String,
    subtitle: String,
    body: String,
}

#[derive(Clone)]
struct ControlSocketInfo {
    path: PathBuf,
    enabled: Arc<AtomicBool>,
}

impl ControlSocketInfo {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
}

struct ControlSocketController {
    info: ControlSocketInfo,
    handler: Arc<AppControlHandler>,
    server: Mutex<Option<ControlServer>>,
}

impl ControlSocketController {
    fn new(info: ControlSocketInfo, handler: Arc<AppControlHandler>) -> Self {
        Self {
            info,
            handler,
            server: Mutex::new(None),
        }
    }

    fn set_enabled(&self, enabled: bool) {
        let Ok(mut server) = self.server.lock() else {
            eprintln!("[control] socket state unavailable");
            return;
        };
        if enabled {
            if server.is_some() {
                self.info.enabled.store(true, Ordering::SeqCst);
                return;
            }
            let control_server = ControlServer::new(&self.info.path, self.handler.clone());
            match control_server.start() {
                Ok(()) => {
                    eprintln!("[control] listening on {}", self.info.path.display());
                    *server = Some(control_server);
                    self.info.enabled.store(true, Ordering::SeqCst);
                }
                Err(error) => {
                    eprintln!("[control] failed to start: {error}");
                    self.info.enabled.store(false, Ordering::SeqCst);
                }
            }
        } else {
            if let Some(control_server) = server.take() {
                control_server.stop();
            }
            self.info.enabled.store(false, Ordering::SeqCst);
            eprintln!("[control] disabled");
        }
    }
}

/// The app-side implementation of the transport crate's synchronous handler.
/// Read-only queries use the shared live workspace state; status notifications
/// are queued for the GPUI thread. Shell mutations use the same workspace
/// transition methods as keyboard actions, so the socket cannot grow a
/// second, divergent pane state machine.
struct AppControlHandler {
    state: Arc<Mutex<ControlState>>,
    control_actions: Arc<Mutex<Vec<ControlAction>>>,
    panes: Arc<PaneRegistry>,
    notifications: Arc<Mutex<Vec<ControlNotification>>>,
    session_refs: Arc<Mutex<BTreeMap<String, String>>>,
    session_store: Option<SessionStore>,
    socket_info: ControlSocketInfo,
    chat_sessions: Arc<Mutex<BTreeMap<String, ChatSession>>>,
    chat_database_path: PathBuf,
    chat_command: AgentCommand,
}

impl AppControlHandler {
    fn new(
        state: Arc<Mutex<ControlState>>,
        control_actions: Arc<Mutex<Vec<ControlAction>>>,
        panes: Arc<PaneRegistry>,
        notifications: Arc<Mutex<Vec<ControlNotification>>>,
        session_refs: Arc<Mutex<BTreeMap<String, String>>>,
        session_store: Option<SessionStore>,
        socket_info: ControlSocketInfo,
    ) -> Self {
        Self::new_with_chat_config(
            state,
            control_actions,
            panes,
            notifications,
            session_refs,
            session_store,
            socket_info,
            session::database_path(),
            default_chat_command(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_chat_config(
        state: Arc<Mutex<ControlState>>,
        control_actions: Arc<Mutex<Vec<ControlAction>>>,
        panes: Arc<PaneRegistry>,
        notifications: Arc<Mutex<Vec<ControlNotification>>>,
        session_refs: Arc<Mutex<BTreeMap<String, String>>>,
        session_store: Option<SessionStore>,
        socket_info: ControlSocketInfo,
        chat_database_path: PathBuf,
        chat_command: AgentCommand,
    ) -> Self {
        Self {
            state,
            control_actions,
            panes,
            notifications,
            session_refs,
            session_store,
            socket_info,
            chat_sessions: Arc::new(Mutex::new(BTreeMap::new())),
            chat_database_path,
            chat_command,
        }
    }

    fn success(id: &str, pairs: impl IntoIterator<Item = (String, String)>) -> ControlResponse {
        ControlResponse::success(id, pairs.into_iter().collect())
    }

    fn pane_error(request: &ControlRequest, error: PaneError) -> ControlResponse {
        ControlResponse::failure(&request.id, error.to_string())
    }

    fn record_notification(
        &self,
        request: &ControlRequest,
        title: &str,
        body: &str,
    ) -> ControlResponse {
        let date = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let Ok(mut notifications) = self.notifications.lock() else {
            return ControlResponse::failure(&request.id, "notification store unavailable");
        };
        notifications.push(ControlNotification {
            date,
            title: title.to_string(),
            subtitle: request.params.get("subtitle").cloned().unwrap_or_default(),
            body: body.to_string(),
        });
        Self::success(&request.id, [])
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ControlState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn queue_action(
        &self,
        request: &ControlRequest,
        action: impl FnOnce(ControlReply) -> ControlAction,
    ) -> ControlResponse {
        let (reply, result) = mpsc::channel();
        let Ok(mut actions) = self.control_actions.lock() else {
            return ControlResponse::failure(&request.id, "control action queue unavailable");
        };
        actions.push(action(reply));
        drop(actions);

        match result.recv_timeout(CONTROL_ACTION_TIMEOUT) {
            Ok(Ok(pairs)) => Self::success(&request.id, pairs),
            Ok(Err(error)) => ControlResponse::failure(&request.id, error),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                ControlResponse::failure(&request.id, "control action timed out")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                ControlResponse::failure(&request.id, "control action worker stopped")
            }
        }
    }

    fn changes_worktree(&self, request: &ControlRequest) -> Result<PathBuf, String> {
        self.state()
            .working_directory(request.params.get("worktree").map(String::as_str))
            .ok_or_else(|| "unknown worktree".to_string())
    }

    fn run_changes_path_action(
        &self,
        request: &ControlRequest,
        action: fn(&Path, &Path) -> Result<(), GitError>,
    ) -> ControlResponse {
        let Some(path) = request.params.get("path") else {
            return ControlResponse::failure(&request.id, "changes mutation requires path");
        };
        let repo = match self.changes_worktree(request) {
            Ok(repo) => repo,
            Err(error) => return ControlResponse::failure(&request.id, error),
        };
        let path = Path::new(path);
        match action(&repo, path) {
            Ok(()) => Self::success(
                &request.id,
                [
                    ("worktree".to_string(), repo.to_string_lossy().into_owned()),
                    ("path".to_string(), path.to_string_lossy().into_owned()),
                ],
            ),
            Err(error) => ControlResponse::failure(&request.id, error.to_string()),
        }
    }

    fn run_changes_all_action(
        &self,
        request: &ControlRequest,
        action: fn(&Path) -> Result<(), GitError>,
    ) -> ControlResponse {
        let repo = match self.changes_worktree(request) {
            Ok(repo) => repo,
            Err(error) => return ControlResponse::failure(&request.id, error),
        };
        match action(&repo) {
            Ok(()) => Self::success(
                &request.id,
                [("worktree".to_string(), repo.to_string_lossy().into_owned())],
            ),
            Err(error) => ControlResponse::failure(&request.id, error.to_string()),
        }
    }

    fn chat_workspace(&self, selector: Option<&str>) -> Result<ControlWorkspace, String> {
        let state = self.state();
        match selector {
            Some(selector) => state
                .workspaces
                .iter()
                .find(|workspace| {
                    workspace.mounted && (workspace.id == selector || workspace.path == selector)
                })
                .cloned()
                .ok_or_else(|| format!("unknown worktree: {selector}")),
            None => state
                .current_workspace()
                .filter(|workspace| workspace.mounted)
                .cloned()
                .ok_or_else(|| "no current workspace".to_string()),
        }
    }

    fn ensure_chat_tab(&self, workspace: &ControlWorkspace) -> Result<String, String> {
        let database = AppDatabase::open(&self.chat_database_path)
            .map_err(|error| format!("chat persistence unavailable: {error}"))?;
        let tabs = database
            .tabs_of_worktree(&workspace.id)
            .map_err(|error| format!("chat tabs unavailable: {error}"))?;
        if let Some(tab) = tabs.iter().find(|tab| tab.kind == "chat") {
            return Ok(tab.id.clone());
        }

        let order = tabs.iter().map(|tab| tab.order_idx).max().unwrap_or(-1) + 1;
        let id = format!("{}-tab-{order}", workspace.id);
        let mut tab = TabRecord::new(&id, &workspace.id, "Chat", "chat");
        tab.order_idx = order;
        database
            .save_tab(&tab)
            .map_err(|error| format!("chat tab unavailable: {error}"))?;
        Ok(id)
    }

    fn persisted_chat_surface(&self, surface_id: &str) -> Result<(String, PathBuf), String> {
        let database = AppDatabase::open(&self.chat_database_path)
            .map_err(|error| format!("chat persistence unavailable: {error}"))?;
        let tab = database
            .tabs()
            .map_err(|error| format!("chat tabs unavailable: {error}"))?
            .into_iter()
            .find(|tab| tab.id == surface_id && tab.kind == "chat")
            .ok_or_else(|| format!("unknown chat surface: {surface_id}"))?;
        let worktree = database
            .worktrees()
            .map_err(|error| format!("chat worktrees unavailable: {error}"))?
            .into_iter()
            .find(|worktree| worktree.id == tab.worktree_id)
            .ok_or_else(|| format!("unknown chat worktree: {}", tab.worktree_id))?;
        Ok((worktree.id, PathBuf::from(worktree.path)))
    }

    fn chat_snapshot(&self, surface_id: &str) -> Result<ChatSnapshot, String> {
        let mut sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;
        if !sessions.contains_key(surface_id) {
            let (worktree_id, _) = self.persisted_chat_surface(surface_id)?;
            let session = ChatSession::restore(&self.chat_database_path, surface_id, worktree_id)
                .map_err(|error| format!("chat restore failed: {error}"))?;
            sessions.insert(surface_id.to_string(), session);
        }
        sessions
            .get(surface_id)
            .ok_or_else(|| format!("unknown chat surface: {surface_id}"))?
            .read()
            .map_err(|error| format!("chat read failed: {error}"))
    }

    fn open_chat(&self, request: &ControlRequest) -> Result<ChatSnapshot, String> {
        let workspace = self.chat_workspace(request.params.get("worktree").map(String::as_str))?;
        let surface_id = self.ensure_chat_tab(&workspace)?;
        let mut sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;

        if let Some(session) = sessions.get(&surface_id)
            && session
                .read()
                .map_err(|error| format!("chat read failed: {error}"))?
                .agent_session_id
                .is_some()
        {
            return session
                .read()
                .map_err(|error| format!("chat read failed: {error}"));
        }
        sessions.remove(&surface_id);

        let session = ChatSession::launch(ChatSessionConfig::new(
            &surface_id,
            &workspace.id,
            self.chat_command.clone(),
            &workspace.path,
            &self.chat_database_path,
        ))
        .map_err(|error| format!("chat launch failed: {error}"))?;
        let snapshot = session
            .read()
            .map_err(|error| format!("chat read failed: {error}"))?;
        sessions.insert(surface_id, session);
        Ok(snapshot)
    }

    fn chat_send(&self, request: &ControlRequest) -> Result<ChatSnapshot, String> {
        let surface_id = request
            .params
            .get("surfaceId")
            .ok_or_else(|| "surface.chat.send requires surfaceId".to_string())?;
        let text = request
            .params
            .get("text")
            .ok_or_else(|| "surface.chat.send requires text".to_string())?;
        let sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;
        let session = sessions
            .get(surface_id)
            .ok_or_else(|| format!("chat surface is not open: {surface_id}"))?;
        session
            .send(text)
            .map_err(|error| format!("chat send failed: {error}"))?;
        session
            .read()
            .map_err(|error| format!("chat read failed: {error}"))
    }

    fn chat_compose(&self, request: &ControlRequest) -> Result<ChatSnapshot, String> {
        let surface_id = request
            .params
            .get("surfaceId")
            .ok_or_else(|| "surface.chat.compose requires surfaceId".to_string())?;
        let text = request
            .params
            .get("text")
            .ok_or_else(|| "surface.chat.compose requires text".to_string())?;
        let sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;
        let session = sessions
            .get(surface_id)
            .ok_or_else(|| format!("chat surface is not open: {surface_id}"))?;
        session
            .compose(text)
            .map_err(|error| format!("chat compose failed: {error}"))?;
        session
            .read()
            .map_err(|error| format!("chat read failed: {error}"))
    }

    fn chat_permission(&self, request: &ControlRequest) -> Result<ChatSnapshot, String> {
        let surface_id = request
            .params
            .get("surfaceId")
            .ok_or_else(|| "surface.chat.permission requires surfaceId".to_string())?;
        let request_id = request
            .params
            .get("requestId")
            .ok_or_else(|| "surface.chat.permission requires requestId".to_string())?
            .parse::<u64>()
            .map_err(|error| format!("invalid requestId: {error}"))?;
        let option_id = request
            .params
            .get("optionId")
            .ok_or_else(|| "surface.chat.permission requires optionId".to_string())?;
        let sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;
        let session = sessions
            .get(surface_id)
            .ok_or_else(|| format!("chat surface is not open: {surface_id}"))?;
        session
            .respond_permission(request_id, option_id)
            .map_err(|error| format!("chat permission failed: {error}"))?;
        session
            .read()
            .map_err(|error| format!("chat read failed: {error}"))
    }

    fn chat_stop(&self, request: &ControlRequest) -> Result<ChatSnapshot, String> {
        let surface_id = request
            .params
            .get("surfaceId")
            .ok_or_else(|| "surface.chat.stop requires surfaceId".to_string())?;
        let sessions = self
            .chat_sessions
            .lock()
            .map_err(|_| "chat session store unavailable".to_string())?;
        let session = sessions
            .get(surface_id)
            .ok_or_else(|| format!("chat surface is not open: {surface_id}"))?;
        session
            .stop()
            .map_err(|error| format!("chat stop failed: {error}"))?;
        session
            .read()
            .map_err(|error| format!("chat read failed: {error}"))
    }
}

impl ControlHandler for AppControlHandler {
    fn handle(&self, request: &ControlRequest) -> ControlResponse {
        match request.method.as_str() {
            "system.ping" => Self::success(&request.id, [("pong".to_string(), "true".to_string())]),
            "system.capabilities" => {
                let mut methods = vec![
                    "system.ping",
                    "system.capabilities",
                    "system.identify",
                    "system.quit",
                    "project.list",
                    "project.add",
                    "workspace.list",
                    "workspace.create",
                    "workspace.select",
                    "workspace.current",
                    "workspace.close",
                    "worktree.set",
                    "notify",
                    "panel.create",
                    "panel.split",
                    "panel.list",
                    "panel.write",
                    "panel.key",
                    "panel.read",
                    "panel.state",
                    "panel.scrollback",
                    "panel.wait",
                    "panel.focus",
                    "panel.close",
                    "pane.split",
                    "pane.focus",
                    "pane.close",
                    "tab.cycle",
                    "tab.select",
                    "notification.create",
                    "notification.list",
                    "notification.clear",
                    "session.ref",
                    "session.restore",
                    "surface.changes.open",
                    "surface.changes.read",
                    "surface.changes.stage",
                    "surface.changes.unstage",
                    "surface.changes.discard",
                    "surface.changes.stage_all",
                    "surface.changes.discard_all",
                    "surface.settings.open",
                    "surface.settings.select",
                    "surface.settings.read",
                    "surface.chat.open",
                    "surface.chat.send",
                    "surface.chat.compose",
                    "surface.chat.permission",
                    "surface.chat.stop",
                    "surface.chat.read",
                ];
                methods.extend(BROWSER_CAPABILITIES);
                let rows: Vec<BTreeMap<String, String>> = methods
                    .iter()
                    .map(|method| BTreeMap::from([("method".to_string(), (*method).to_string())]))
                    .collect();
                Self::success(
                    &request.id,
                    [
                        (
                            "methods".to_string(),
                            tiller_control::protocol::rows::encode(&rows),
                        ),
                        (
                            "socketEnabled".to_string(),
                            self.socket_info.enabled().to_string(),
                        ),
                        (
                            "enabled".to_string(),
                            self.socket_info.enabled().to_string(),
                        ),
                        (
                            "socketPath".to_string(),
                            self.socket_info.path.to_string_lossy().into_owned(),
                        ),
                    ],
                )
            }
            "system.identify" => {
                let state = self.state();
                let requested_workspace = request.params.get("worktree");
                let pane = request.params.get("pane");
                let workspace = if let Some(selector) = requested_workspace {
                    state.workspaces.iter().find(|workspace| {
                        workspace.id == selector.as_str() || workspace.path == selector.as_str()
                    })
                } else {
                    pane.and_then(|pane| {
                        state
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.session.as_deref() == Some(pane.as_str()))
                    })
                    .or_else(|| state.current_workspace())
                }
                .cloned();
                drop(state);
                let Some(workspace) = workspace else {
                    return ControlResponse::failure(&request.id, "no current workspace");
                };
                let mut result = vec![
                    ("project".to_string(), workspace.project),
                    ("branch".to_string(), workspace.branch),
                    ("path".to_string(), workspace.path),
                    ("workspaceId".to_string(), workspace.id),
                    (
                        "surfaceId".to_string(),
                        request.params.get("pane").cloned().unwrap_or_default(),
                    ),
                ];
                if let Some(pane) = request.params.get("pane")
                    && let Ok(references) = self.session_refs.lock()
                    && let Some(reference) = references.get(pane)
                {
                    result.push(("sessionRef".to_string(), reference.clone()));
                }
                Self::success(&request.id, result)
            }
            "system.quit" => self.queue_action(request, |reply| ControlAction::Quit { reply }),
            "workspace.list" => Self::success(
                &request.id,
                [(
                    "workspaces".to_string(),
                    tiller_control::protocol::rows::encode(&self.state().workspace_rows()),
                )],
            ),
            "project.list" => Self::success(
                &request.id,
                [(
                    "projects".to_string(),
                    tiller_control::protocol::rows::encode(&self.state().project_rows()),
                )],
            ),
            "project.add" => {
                let Some(path) = request.params.get("path") else {
                    return ControlResponse::failure(&request.id, "project.add requires path");
                };
                let path = PathBuf::from(path);
                self.queue_action(request, move |reply| ControlAction::AddProject {
                    path,
                    reply,
                })
            }
            "workspace.current" => {
                let state = self.state();
                let Some(workspace) = state.current_workspace() else {
                    return ControlResponse::failure(&request.id, "no current workspace");
                };
                Self::success(
                    &request.id,
                    [
                        ("id".to_string(), workspace.id.clone()),
                        ("project".to_string(), workspace.project.clone()),
                        ("branch".to_string(), workspace.branch.clone()),
                        ("path".to_string(), workspace.path.clone()),
                    ],
                )
            }
            "surface.changes.open" => {
                let worktree = request.params.get("worktree").cloned();
                self.queue_action(request, move |reply| ControlAction::OpenChanges {
                    worktree,
                    reply,
                })
            }
            "surface.changes.read" => {
                self.queue_action(request, |reply| ControlAction::ReadChanges { reply })
            }
            "surface.changes.stage" => self.run_changes_path_action(request, stage),
            "surface.changes.unstage" => self.run_changes_path_action(request, unstage),
            "surface.changes.discard" => self.run_changes_path_action(request, discard),
            "surface.changes.stage_all" => self.run_changes_all_action(request, stage_all),
            "surface.changes.discard_all" => self.run_changes_all_action(request, discard_all),
            "surface.settings.open" => {
                let section = match request.params.get("section") {
                    Some(value) => match parse_settings_category(value) {
                        Ok(section) => Some(section),
                        Err(error) => return ControlResponse::failure(&request.id, error),
                    },
                    None => None,
                };
                self.queue_action(request, move |reply| ControlAction::OpenSettings {
                    section,
                    reply,
                })
            }
            "surface.settings.select" => {
                let Some(value) = request.params.get("section") else {
                    return ControlResponse::failure(
                        &request.id,
                        "surface.settings.select requires section",
                    );
                };
                let section = match parse_settings_category(value) {
                    Ok(section) => section,
                    Err(error) => return ControlResponse::failure(&request.id, error),
                };
                self.queue_action(request, move |reply| ControlAction::SelectSettings {
                    section,
                    reply,
                })
            }
            "surface.settings.read" => {
                self.queue_action(request, |reply| ControlAction::ReadSettings { reply })
            }
            "surface.chat.open" => self
                .open_chat(request)
                .map(|snapshot| snapshot_result(&snapshot))
                .map_or_else(
                    |error| ControlResponse::failure(&request.id, error),
                    |result| Self::success(&request.id, result),
                ),
            "surface.chat.send" => self
                .chat_send(request)
                .map(|snapshot| snapshot_result(&snapshot))
                .map_or_else(
                    |error| ControlResponse::failure(&request.id, error),
                    |result| Self::success(&request.id, result),
                ),
            "surface.chat.compose" => self
                .chat_compose(request)
                .map(|snapshot| snapshot_result(&snapshot))
                .map_or_else(
                    |error| ControlResponse::failure(&request.id, error),
                    |result| Self::success(&request.id, result),
                ),
            "surface.chat.permission" => self
                .chat_permission(request)
                .map(|snapshot| snapshot_result(&snapshot))
                .map_or_else(
                    |error| ControlResponse::failure(&request.id, error),
                    |result| Self::success(&request.id, result),
                ),
            "surface.chat.stop" => self
                .chat_stop(request)
                .map(|snapshot| snapshot_result(&snapshot))
                .map_or_else(
                    |error| ControlResponse::failure(&request.id, error),
                    |result| Self::success(&request.id, result),
                ),
            "surface.chat.read" => {
                let Some(surface_id) = request.params.get("surfaceId") else {
                    return ControlResponse::failure(
                        &request.id,
                        "surface.chat.read requires surfaceId",
                    );
                };
                self.chat_snapshot(surface_id)
                    .map(|snapshot| snapshot_result(&snapshot))
                    .map_or_else(
                        |error| ControlResponse::failure(&request.id, error),
                        |result| Self::success(&request.id, result),
                    )
            }
            "panel.create" => {
                let Some(working_directory) = self
                    .state()
                    .working_directory(request.params.get("worktree").map(String::as_str))
                else {
                    return ControlResponse::failure(&request.id, "unknown worktree");
                };
                let command = request.params.get("cmd").map(String::as_str);
                let title = command
                    .and_then(|value| value.split_whitespace().next())
                    .unwrap_or("Panel");
                match self.panes.create(working_directory, command, title) {
                    Ok(pane) => Self::success(&request.id, [("id".to_string(), pane.id)]),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.split" => {
                let Some(source) = request.params.get("from") else {
                    return ControlResponse::failure(&request.id, "panel.split requires from");
                };
                let Some(direction) = request.params.get("direction") else {
                    return ControlResponse::failure(&request.id, "panel.split requires direction");
                };
                match self.panes.split(
                    source,
                    direction,
                    request.params.get("cmd").map(String::as_str),
                ) {
                    Ok(pane) => Self::success(&request.id, [("id".to_string(), pane.id)]),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.list" => {
                let working_directory = if let Some(working_directory) = self
                    .state()
                    .working_directory(request.params.get("worktree").map(String::as_str))
                {
                    working_directory
                } else {
                    return ControlResponse::failure(&request.id, "unknown worktree");
                };
                match self.panes.list_for(&working_directory) {
                    Ok(panes) => {
                        let rows: Vec<BTreeMap<String, String>> = panes
                            .into_iter()
                            .map(|pane| {
                                BTreeMap::from([
                                    ("id".to_string(), pane.id),
                                    ("tab".to_string(), pane.tab),
                                    ("title".to_string(), pane.title),
                                    ("agent".to_string(), pane.agent),
                                    ("active".to_string(), pane.active.to_string()),
                                ])
                            })
                            .collect();
                        Self::success(
                            &request.id,
                            [(
                                "panels".to_string(),
                                tiller_control::protocol::rows::encode(&rows),
                            )],
                        )
                    }
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.write" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.write requires id");
                };
                let Some(input) = request.params.get("input") else {
                    return ControlResponse::failure(&request.id, "panel.write requires input");
                };
                match self.panes.write(id, input.as_bytes()) {
                    Ok(()) => Self::success(&request.id, []),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.key" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.key requires id");
                };
                let Some(key) = request.params.get("key") else {
                    return ControlResponse::failure(&request.id, "panel.key requires key");
                };
                match self.panes.key(id, key) {
                    Ok(()) => Self::success(&request.id, []),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.read" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.read requires id");
                };
                match self.panes.read(id) {
                    Ok(output) => {
                        Self::success(&request.id, [("data".to_string(), base64_encode(&output))])
                    }
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.state" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.state requires id");
                };
                let id = id.clone();
                self.queue_action(request, move |reply| ControlAction::ReadPane {
                    id,
                    query: PaneQuery::State,
                    reply,
                })
            }
            "panel.scrollback" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.scrollback requires id");
                };
                let max_bytes = match request.params.get("maxBytes") {
                    Some(value) => match value.parse::<usize>() {
                        Ok(value) => Some(value),
                        Err(_) => {
                            return ControlResponse::failure(
                                &request.id,
                                "maxBytes must be a nonnegative integer",
                            );
                        }
                    },
                    None => None,
                };
                let id = id.clone();
                self.queue_action(request, move |reply| ControlAction::ReadPane {
                    id,
                    query: PaneQuery::Scrollback(max_bytes),
                    reply,
                })
            }
            "panel.wait" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.wait requires id");
                };
                let timeout = match request.params.get("timeoutMs") {
                    Some(value) => match value.parse::<u64>() {
                        Ok(milliseconds) => Some(Duration::from_millis(milliseconds)),
                        Err(_) => {
                            return ControlResponse::failure(
                                &request.id,
                                "timeoutMs must be a nonnegative integer",
                            );
                        }
                    },
                    None => None,
                };
                match self.panes.wait(id, timeout) {
                    Ok(exit_code) => Self::success(
                        &request.id,
                        [("exitCode".to_string(), exit_code.to_string())],
                    ),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.close" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.close requires id");
                };
                match self.panes.close(id) {
                    Ok(()) => Self::success(&request.id, []),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "panel.focus" => {
                let Some(id) = request.params.get("id") else {
                    return ControlResponse::failure(&request.id, "panel.focus requires id");
                };
                match self.panes.focus(id) {
                    Ok(()) => Self::success(&request.id, []),
                    Err(error) => Self::pane_error(request, error),
                }
            }
            "pane.split" => {
                let Some(direction) = request.params.get("direction") else {
                    return ControlResponse::failure(&request.id, "pane.split requires direction");
                };
                let direction = match direction.as_str() {
                    "right" | "left" => SplitDirection::Horizontal,
                    "down" | "up" => SplitDirection::Vertical,
                    _ => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("unknown pane split direction: {direction}"),
                        );
                    }
                };
                self.queue_action(request, move |reply| ControlAction::SplitPane {
                    direction,
                    reply,
                })
            }
            "pane.focus" => {
                let Some(direction) = request.params.get("direction") else {
                    return ControlResponse::failure(&request.id, "pane.focus requires direction");
                };
                let (direction, forward) = match direction.as_str() {
                    "left" => (SplitDirection::Horizontal, false),
                    "right" => (SplitDirection::Horizontal, true),
                    "up" | "above" => (SplitDirection::Vertical, false),
                    "down" | "below" => (SplitDirection::Vertical, true),
                    _ => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("unknown pane focus direction: {direction}"),
                        );
                    }
                };
                self.queue_action(request, move |reply| ControlAction::FocusPane {
                    direction,
                    forward,
                    reply,
                })
            }
            "pane.close" => self.queue_action(request, |reply| ControlAction::ClosePane { reply }),
            "tab.cycle" => {
                let Some(direction) = request.params.get("direction") else {
                    return ControlResponse::failure(&request.id, "tab.cycle requires direction");
                };
                let forward = match direction.as_str() {
                    "forward" | "next" => true,
                    "backward" | "previous" => false,
                    _ => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("unknown tab cycle direction: {direction}"),
                        );
                    }
                };
                self.queue_action(request, move |reply| ControlAction::CycleTab {
                    forward,
                    reply,
                })
            }
            "tab.select" => {
                let Some(position) = request
                    .params
                    .get("index")
                    .or_else(|| request.params.get("position"))
                else {
                    return ControlResponse::failure(&request.id, "tab.select requires index");
                };
                let Ok(position) = position.parse::<usize>() else {
                    return ControlResponse::failure(
                        &request.id,
                        "tab.select index must be a positive integer",
                    );
                };
                if position == 0 {
                    return ControlResponse::failure(
                        &request.id,
                        "tab.select index must be a positive integer",
                    );
                }
                self.queue_action(request, move |reply| ControlAction::SelectTab {
                    position,
                    reply,
                })
            }
            "notification.create" => {
                let Some(title) = request.params.get("title") else {
                    return ControlResponse::failure(&request.id, "notification requires title");
                };
                let Some(body) = request.params.get("body") else {
                    return ControlResponse::failure(&request.id, "notification requires body");
                };
                self.record_notification(request, title, body)
            }
            "notification.list" => {
                let Ok(notifications) = self.notifications.lock() else {
                    return ControlResponse::failure(&request.id, "notification store unavailable");
                };
                let rows: Vec<BTreeMap<String, String>> = notifications
                    .iter()
                    .map(|notification| {
                        BTreeMap::from([
                            ("date".to_string(), notification.date.clone()),
                            ("title".to_string(), notification.title.clone()),
                            ("subtitle".to_string(), notification.subtitle.clone()),
                            ("body".to_string(), notification.body.clone()),
                        ])
                    })
                    .collect();
                Self::success(
                    &request.id,
                    [(
                        "notifications".to_string(),
                        tiller_control::protocol::rows::encode(&rows),
                    )],
                )
            }
            "notification.clear" => {
                let Ok(mut notifications) = self.notifications.lock() else {
                    return ControlResponse::failure(&request.id, "notification store unavailable");
                };
                notifications.clear();
                Self::success(&request.id, [])
            }
            "session.ref" => {
                let Some(session) = request.params.get("session") else {
                    return ControlResponse::failure(&request.id, "session.ref requires session");
                };
                let Some(reference) = request.params.get("ref") else {
                    return ControlResponse::failure(&request.id, "session.ref requires ref");
                };
                let Ok(mut references) = self.session_refs.lock() else {
                    return ControlResponse::failure(&request.id, "session store unavailable");
                };
                references.insert(session.clone(), reference.clone());
                if let Some(store) = &self.session_store {
                    store.save_session_ref(session, reference);
                }
                Self::success(
                    &request.id,
                    [
                        ("session".to_string(), session.clone()),
                        ("ref".to_string(), reference.clone()),
                    ],
                )
            }
            "worktree.set" => {
                let Some(selector) = request.params.get("worktree") else {
                    return ControlResponse::failure(&request.id, "worktree.set requires worktree");
                };
                let comment = request.params.get("comment").map(String::as_str);
                let session = request.params.get("session").map(String::as_str);
                if comment.is_none() && session.is_none() {
                    return ControlResponse::failure(
                        &request.id,
                        "worktree.set requires comment or session",
                    );
                }
                let Some(workspace) = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| state.set_worktree(selector, comment, session))
                else {
                    return ControlResponse::failure(&request.id, "unknown worktree");
                };
                let mut result = vec![
                    ("id".to_string(), workspace.id),
                    ("path".to_string(), workspace.path),
                    ("comment".to_string(), workspace.comment),
                ];
                if let Some(session) = workspace.session {
                    result.push(("session".to_string(), session));
                }
                Self::success(&request.id, result)
            }
            "notify" => {
                if let Some(title) = request.params.get("title") {
                    let Some(body) = request.params.get("body") else {
                        return ControlResponse::failure(&request.id, "notify requires body");
                    };
                    return self.record_notification(request, title, body);
                }
                let Some(pane_id) = request.params.get("session").cloned() else {
                    return ControlResponse::failure(&request.id, "notify requires session");
                };
                let Some(status) =
                    request
                        .params
                        .get("status")
                        .and_then(|status| match status.as_str() {
                            "running" => Some(AgentStatus::Running),
                            "needs-input" | "needs_input" => Some(AgentStatus::NeedsInput),
                            "done" | "finished" => Some(AgentStatus::Done),
                            "error" | "failed" => Some(AgentStatus::Error),
                            _ => None,
                        })
                else {
                    return ControlResponse::failure(&request.id, "notify has an unknown status");
                };
                if let Some(agent_session) = request.params.get("agentSession") {
                    let Ok(mut references) = self.session_refs.lock() else {
                        return ControlResponse::failure(&request.id, "session store unavailable");
                    };
                    references.insert(pane_id.clone(), agent_session.clone());
                    if let Some(store) = &self.session_store {
                        store.save_session_ref(&pane_id, agent_session);
                    }
                }
                let Ok(mut actions) = self.control_actions.lock() else {
                    return ControlResponse::failure(
                        &request.id,
                        "control action queue unavailable",
                    );
                };
                actions.push(ControlAction::Notify { pane_id, status });
                Self::success(&request.id, [("queued".to_string(), "true".to_string())])
            }
            "workspace.select" => {
                let Some(selector) = request.params.get("workspace").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "workspace.select requires workspace",
                    );
                };
                self.queue_action(request, move |reply| ControlAction::SelectWorktree {
                    selector,
                    reply,
                })
            }
            "workspace.create" | "workspace.new" => {
                let Some(project) = request.params.get("project").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "workspace.create requires project",
                    );
                };
                let branch = request.params.get("branch").cloned();
                self.queue_action(request, move |reply| ControlAction::CreateWorkspace {
                    project,
                    branch,
                    reply,
                })
            }
            "workspace.close" => {
                let Some(selector) = request.params.get("workspace").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "workspace.close requires workspace",
                    );
                };
                self.queue_action(request, move |reply| ControlAction::CloseWorkspace {
                    selector,
                    reply,
                })
            }
            "session.restore" => {
                self.queue_action(request, |reply| ControlAction::RestoreSession { reply })
            }
            method if BROWSER_METHODS.contains(&method) => {
                if let Some(error) = browser_request_error(method, &request.params) {
                    return ControlResponse::failure(&request.id, error);
                }
                let method = method.to_string();
                let params = request.params.clone();
                self.queue_action(request, move |reply| ControlAction::Browser {
                    method,
                    params,
                    reply,
                })
            }
            _ => ControlResponse::failure(
                &request.id,
                format!("unknown control method: {}", request.method),
            ),
        }
    }
}

fn parse_settings_category(value: &str) -> Result<SettingsCategory, String> {
    let category = SettingsCategory::from_title(value)
        .ok_or_else(|| format!("unknown settings section: {value}"))?;
    if !SettingsCategory::all().contains(&category) {
        return Err(format!(
            "settings section '{}' is unavailable on this platform",
            category.title()
        ));
    }
    Ok(category)
}

fn default_chat_command() -> AgentCommand {
    std::env::var_os("TILLER_ACP_PROGRAM")
        .map(PathBuf::from)
        .map(AgentCommand::new)
        .unwrap_or_else(|| {
            AgentCommand::new("npx").args(["-y", "@agentclientprotocol/claude-agent-acp@latest"])
        })
}

fn skill_install_shell(command: tiller_project::SkillInstallCommand) -> TerminalShell {
    TerminalShell::WithArguments {
        program: command.program,
        args: command.args,
    }
}

fn terminal_link_url_for_pane<'a>(event: &'a TerminalLinkEvent, pane_id: &str) -> Option<&'a str> {
    (event.target.pane_id() == pane_id).then_some(event.url.as_str())
}

fn post_desktop_notification(payload: &NotificationPayload) {
    if let Err(error) = Command::new("notify-send")
        .arg("--app-name=Tiller")
        .arg(&payload.title)
        .arg(&payload.body)
        .spawn()
    {
        eprintln!("[notifications] could not deliver desktop notification: {error}");
    }
}

/// Resolves persisted chat identity into the command and tab metadata that
/// can actually be restored. Legacy rows and adapters without an ACP server
/// use the default chat command but do not retain a misleading agent id.
fn restored_chat_spec(agent_id: Option<&str>) -> (AgentCommand, Option<Icon>, Option<String>) {
    let Some(adapter) = agent_id.and_then(|id| {
        AGENT_CATALOG
            .iter()
            .find(|adapter| adapter.id() == id)
            .copied()
    }) else {
        return (default_chat_command(), None, None);
    };
    let Some(program) = adapter.acp_program() else {
        return (default_chat_command(), None, None);
    };
    (
        acp_agent_command(program),
        Icon::for_agent_id(adapter.id()),
        Some(adapter.id().to_string()),
    )
}

fn snapshot_result(snapshot: &ChatSnapshot) -> BTreeMap<String, String> {
    let mut result = BTreeMap::from([
        ("surfaceId".to_string(), snapshot.tab_id.clone()),
        ("status".to_string(), snapshot.status.as_str().to_string()),
        ("composerText".to_string(), snapshot.composer_text.clone()),
        ("queuedText".to_string(), snapshot.queued_text.clone()),
        (
            "transcript".to_string(),
            tiller_control::protocol::rows::encode(
                &snapshot
                    .transcript
                    .turns
                    .iter()
                    .flat_map(|turn| turn.entries.iter().map(chat_entry_row))
                    .collect::<Vec<_>>(),
            ),
        ),
    ]);
    if let Some(agent_session_id) = &snapshot.agent_session_id {
        result.insert("agentSessionId".to_string(), agent_session_id.clone());
    }
    if let Some(error) = &snapshot.error {
        result.insert("error".to_string(), error.clone());
    }
    result
}

fn chat_entry_row(entry: &ChatEntry) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    match entry {
        ChatEntry::UserMessage { text } => {
            row.insert("kind".to_string(), "user".to_string());
            row.insert("text".to_string(), text.clone());
        }
        ChatEntry::AssistantMessage { text } => {
            row.insert("kind".to_string(), "assistant".to_string());
            row.insert("text".to_string(), text.clone());
        }
        ChatEntry::Thought { text } => {
            row.insert("kind".to_string(), "thought".to_string());
            row.insert("text".to_string(), text.clone());
        }
        ChatEntry::ToolCall { id, title, status } => {
            row.insert("kind".to_string(), "tool".to_string());
            row.insert("id".to_string(), id.clone());
            row.insert("text".to_string(), title.clone());
            row.insert("status".to_string(), status.clone());
        }
        ChatEntry::Permission {
            request_id,
            outcome,
            ..
        } => {
            row.insert("kind".to_string(), "permission".to_string());
            row.insert("id".to_string(), request_id.to_string());
            match outcome {
                ChatPermissionOutcome::Pending => {
                    row.insert("status".to_string(), "pending".to_string());
                }
                ChatPermissionOutcome::Selected { option_id, .. } => {
                    row.insert("status".to_string(), "selected".to_string());
                    row.insert("optionId".to_string(), option_id.clone());
                }
                ChatPermissionOutcome::Cancelled => {
                    row.insert("status".to_string(), "cancelled".to_string());
                }
                ChatPermissionOutcome::TimedOut => {
                    row.insert("status".to_string(), "timed_out".to_string());
                }
                ChatPermissionOutcome::Expired => {
                    row.insert("status".to_string(), "expired".to_string());
                }
            }
        }
        ChatEntry::Plan { entries } => {
            row.insert("kind".to_string(), "plan".to_string());
            row.insert(
                "text".to_string(),
                entries
                    .iter()
                    .map(|entry| format!("{} · {}", entry.status, entry.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        ChatEntry::TurnFooter { text } => {
            row.insert("kind".to_string(), "turn".to_string());
            row.insert("text".to_string(), text.clone());
        }
        ChatEntry::Error { message, .. } => {
            row.insert("kind".to_string(), "error".to_string());
            row.insert("text".to_string(), message.clone());
        }
    }
    row
}

fn panel_state_pairs(snapshot: &PaneStateSnapshot) -> Vec<(String, String)> {
    let mut pairs = vec![
        (
            "workingDirectory".to_string(),
            snapshot.working_directory.to_string_lossy().into_owned(),
        ),
        (
            "scrollback".to_string(),
            base64_encode(&snapshot.scrollback),
        ),
        (
            "scrollbackBytes".to_string(),
            snapshot.scrollback.len().to_string(),
        ),
    ];
    if let Some(status) = snapshot.exit_status {
        let (label, code) = match status {
            PaneExitStatus::Success => ("success".to_string(), Some(0)),
            PaneExitStatus::Code(code) => (format!("code:{code}"), Some(code)),
            PaneExitStatus::Signal(signal) => (format!("signal:{signal}"), None),
            PaneExitStatus::Unknown => ("unknown".to_string(), None),
        };
        pairs.push(("exitStatus".to_string(), label));
        if let Some(code) = code {
            pairs.push(("exitCode".to_string(), code.to_string()));
        }
    } else {
        pairs.push(("exitStatus".to_string(), "running".to_string()));
    }
    pairs
}

fn panel_state_from_terminal(snapshot: TerminalStateSnapshot) -> PaneStateSnapshot {
    PaneStateSnapshot {
        working_directory: snapshot.working_directory,
        scrollback: snapshot.scrollback,
        exit_status: snapshot.exit_status.map(|status| match status {
            TerminalExitStatus::Success => PaneExitStatus::Success,
            TerminalExitStatus::Code(code) => PaneExitStatus::Code(code),
            TerminalExitStatus::Signal(signal) => PaneExitStatus::Signal(signal),
            TerminalExitStatus::Unknown => PaneExitStatus::Unknown,
        }),
    }
}

fn changes_report_pairs(
    tab_id: usize,
    report: &ChangesReport,
) -> Result<Vec<(String, String)>, String> {
    let section_rows = report
        .sections
        .iter()
        .map(|section| {
            let files = section
                .files
                .iter()
                .map(|file| {
                    BTreeMap::from([
                        ("path".to_string(), file.path.to_string_lossy().into_owned()),
                        ("additions".to_string(), file.additions.to_string()),
                        ("deletions".to_string(), file.deletions.to_string()),
                        ("binary".to_string(), file.is_binary.to_string()),
                    ])
                })
                .collect::<Vec<_>>();
            BTreeMap::from([
                ("section".to_string(), section.name.to_string()),
                ("count".to_string(), section.count.to_string()),
                (
                    "files".to_string(),
                    tiller_control::protocol::rows::encode(&files),
                ),
            ])
        })
        .collect::<Vec<_>>();

    let section = |name: &str| {
        report
            .sections
            .iter()
            .find(|section| section.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Changes report is missing {name} section"))
    };
    let staged = section("Staged")?;
    let changed = section("Changed")?;
    let untracked = section("Untracked")?;

    Ok(vec![
        ("surfaceId".to_string(), "changes".to_string()),
        ("tabId".to_string(), tab_id.to_string()),
        (
            "worktree".to_string(),
            report.repo_root.to_string_lossy().into_owned(),
        ),
        ("loading".to_string(), report.loading.to_string()),
        ("ready".to_string(), (!report.loading).to_string()),
        (
            "error".to_string(),
            report.error.clone().unwrap_or_default(),
        ),
        ("stagedCount".to_string(), staged.count.to_string()),
        ("changedCount".to_string(), changed.count.to_string()),
        ("untrackedCount".to_string(), untracked.count.to_string()),
        (
            "staged".to_string(),
            tiller_control::protocol::rows::encode(
                &staged
                    .files
                    .iter()
                    .map(|file| {
                        BTreeMap::from([
                            ("path".to_string(), file.path.to_string_lossy().into_owned()),
                            ("additions".to_string(), file.additions.to_string()),
                            ("deletions".to_string(), file.deletions.to_string()),
                            ("binary".to_string(), file.is_binary.to_string()),
                        ])
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "changed".to_string(),
            tiller_control::protocol::rows::encode(
                &changed
                    .files
                    .iter()
                    .map(|file| {
                        BTreeMap::from([
                            ("path".to_string(), file.path.to_string_lossy().into_owned()),
                            ("additions".to_string(), file.additions.to_string()),
                            ("deletions".to_string(), file.deletions.to_string()),
                            ("binary".to_string(), file.is_binary.to_string()),
                        ])
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "untracked".to_string(),
            tiller_control::protocol::rows::encode(
                &untracked
                    .files
                    .iter()
                    .map(|file| {
                        BTreeMap::from([
                            ("path".to_string(), file.path.to_string_lossy().into_owned()),
                            ("additions".to_string(), file.additions.to_string()),
                            ("deletions".to_string(), file.deletions.to_string()),
                            ("binary".to_string(), file.is_binary.to_string()),
                        ])
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "sections".to_string(),
            tiller_control::protocol::rows::encode(&section_rows),
        ),
    ])
}

fn settings_report_pairs(report: &SettingsReport) -> Result<Vec<(String, String)>, String> {
    let section_rows = SettingsCategory::all()
        .into_iter()
        .map(|section| {
            BTreeMap::from([
                ("id".to_string(), settings_category_id(section)),
                ("title".to_string(), section.title().to_string()),
            ])
        })
        .collect::<Vec<_>>();
    let provider_rows = report
        .provider_availability
        .iter()
        .map(|provider| {
            BTreeMap::from([
                ("id".to_string(), provider.id.to_string()),
                ("name".to_string(), provider.display_name.to_string()),
                ("available".to_string(), provider.is_available().to_string()),
                ("status".to_string(), provider.status_label().to_string()),
                (
                    "path".to_string(),
                    provider
                        .executable
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let snapshot = &report.snapshot;
    let values = BTreeMap::from([
        (
            "theme".to_string(),
            theme_mode_name(snapshot.theme).to_string(),
        ),
        (
            "interfaceFontSize".to_string(),
            snapshot.interface_font_size.to_string(),
        ),
        (
            "terminalFontSize".to_string(),
            snapshot.terminal_font_size.to_string(),
        ),
        (
            "fileIcons".to_string(),
            snapshot.file_icons.title().to_string(),
        ),
        (
            "controlSocketEnabled".to_string(),
            snapshot.control_socket_enabled.to_string(),
        ),
        ("socketPath".to_string(), snapshot.socket_path.clone()),
        (
            "resumeAgentSessions".to_string(),
            report.resume_agent_sessions.to_string(),
        ),
        ("autoNaming".to_string(), report.auto_naming.to_string()),
        (
            "limitChatHistory".to_string(),
            report.limit_chat_history.to_string(),
        ),
        (
            "chatRetention".to_string(),
            report.chat_retention.to_string(),
        ),
        (
            "limitMountedWorktrees".to_string(),
            report.limit_mounted_worktrees.to_string(),
        ),
        (
            "mountedWorktrees".to_string(),
            report.mounted_worktrees.to_string(),
        ),
        (
            "claudeShowInBar".to_string(),
            report.claude_show_in_bar.to_string(),
        ),
        (
            "codexShowInBar".to_string(),
            report.codex_show_in_bar.to_string(),
        ),
        (
            "opencodeShowInBar".to_string(),
            report.opencode_show_in_bar.to_string(),
        ),
        (
            "refreshInterval".to_string(),
            report.refresh_interval.to_string(),
        ),
    ]);
    let mut result = vec![
        ("surfaceId".to_string(), "settings".to_string()),
        ("section".to_string(), report.category.title().to_string()),
        (
            "sectionId".to_string(),
            settings_category_id(report.category),
        ),
        (
            "availableSections".to_string(),
            tiller_control::protocol::rows::encode(&section_rows),
        ),
        (
            "providers".to_string(),
            tiller_control::protocol::rows::encode(&provider_rows),
        ),
        (
            "values".to_string(),
            tiller_control::protocol::rows::encode(std::slice::from_ref(&values)),
        ),
    ];
    result.extend(values);
    Ok(result)
}

fn settings_category_id(category: SettingsCategory) -> String {
    category.title().to_ascii_lowercase().replace(' ', "-")
}

fn theme_mode_name(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "system",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    }
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

fn file_path_is_already_open(open_paths: &[PathBuf], path: &Path) -> bool {
    open_paths.iter().any(|open_path| open_path == path)
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
        TabKind::Browser => Icon::Globe,
        TabKind::Diff => Icon::File,
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
        NewTabAction::NewTerminal
        | NewTabAction::NewChanges
        | NewTabAction::NewBrowser
        | NewTabAction::NewChat => None,
    }
}

fn agent_icon_for_action(action: NewTabAction) -> Option<Icon> {
    agent_id_for_action(action).and_then(Icon::for_agent_id)
}

fn activity_status_for_agent(status: AgentStatus) -> ActivityStatus {
    match status {
        AgentStatus::Running => ActivityStatus::Running,
        AgentStatus::NeedsInput => ActivityStatus::NeedsInput,
        AgentStatus::Done => ActivityStatus::Done,
        AgentStatus::Error => ActivityStatus::Error,
    }
}

fn tab_status_color(status: ActivityStatus, theme: Theme) -> gpui::Rgba {
    match status {
        ActivityStatus::Idle => theme.meta,
        ActivityStatus::Running => theme.tab_focus_accent,
        ActivityStatus::NeedsInput => theme.tab_needs_input,
        ActivityStatus::Done => theme.tab_done,
        ActivityStatus::Error => theme.tab_error,
    }
}

fn tab_status_glyph(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Idle => "○",
        ActivityStatus::Running => "●",
        ActivityStatus::NeedsInput => "?",
        ActivityStatus::Done => "✓",
        ActivityStatus::Error => "!",
    }
}

fn tab_status_name(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::Idle => "idle",
        ActivityStatus::Running => "running",
        ActivityStatus::NeedsInput => "needs-input",
        ActivityStatus::Done => "done",
        ActivityStatus::Error => "error",
    }
}

fn chat_tab_identity(
    adapter: Option<&dyn tiller_agents::AgentAdapter>,
) -> (String, Option<Icon>, Option<String>) {
    (
        adapter.map_or_else(
            || "Chat".to_string(),
            |adapter| adapter.display_name().to_string(),
        ),
        adapter.and_then(|adapter| Icon::for_agent_id(adapter.id())),
        adapter.map(|adapter| adapter.id().to_string()),
    )
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
    let (project, catalog_branch, is_primary) =
        catalog_entry.unwrap_or((fallback_project, String::new(), false));
    let branch = current_branch(working_directory)
        .ok()
        .flatten()
        .or_else(|| (is_primary && !catalog_branch.is_empty()).then_some(catalog_branch.clone()))
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

const SPLIT_DIVIDER_SIZE: f32 = 6.0;
const MIN_SPLIT_PANE_SIZE: f32 = 160.0;

fn split_direction_name(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Horizontal => "horizontal",
        SplitDirection::Vertical => "vertical",
    }
}

fn split_event_name(direction: SplitDirection, placement: SplitPlacement) -> String {
    let suffix = match placement {
        SplitPlacement::Before => "-before",
        SplitPlacement::After => "",
    };
    format!("{}{suffix}", split_direction_name(direction))
}

fn parse_split_direction(direction: &str) -> Option<SplitDirection> {
    match direction {
        "horizontal" => Some(SplitDirection::Horizontal),
        "vertical" => Some(SplitDirection::Vertical),
        _ => None,
    }
}

fn parse_split_event(direction: &str) -> Option<(SplitDirection, SplitPlacement)> {
    if let Some(axis) = direction.strip_suffix("-before") {
        return parse_split_direction(axis).map(|direction| (direction, SplitPlacement::Before));
    }
    parse_split_direction(direction).map(|direction| (direction, SplitPlacement::After))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalContextCommand {
    SetTitle,
    Split {
        direction: SplitDirection,
        placement: SplitPlacement,
    },
    Close,
}

fn delegated_terminal_context_action(
    action: TerminalContextAction,
) -> Option<TerminalContextCommand> {
    match action {
        TerminalContextAction::SetTitle => Some(TerminalContextCommand::SetTitle),
        TerminalContextAction::SplitLeft => Some(TerminalContextCommand::Split {
            direction: SplitDirection::Horizontal,
            placement: SplitPlacement::Before,
        }),
        TerminalContextAction::SplitRight => Some(TerminalContextCommand::Split {
            direction: SplitDirection::Horizontal,
            placement: SplitPlacement::After,
        }),
        TerminalContextAction::SplitAbove => Some(TerminalContextCommand::Split {
            direction: SplitDirection::Vertical,
            placement: SplitPlacement::Before,
        }),
        TerminalContextAction::SplitDown => Some(TerminalContextCommand::Split {
            direction: SplitDirection::Vertical,
            placement: SplitPlacement::After,
        }),
        TerminalContextAction::CloseTerminal => Some(TerminalContextCommand::Close),
        TerminalContextAction::Copy
        | TerminalContextAction::Paste
        | TerminalContextAction::CopyContext
        | TerminalContextAction::CopyPaneId
        | TerminalContextAction::CopyTerminalId
        | TerminalContextAction::ClearTerminal => None,
    }
}

fn next_pane_id(tabs: &[OpenTab]) -> usize {
    tabs.iter()
        .flat_map(|tab| tab.panes.leaf_ids())
        .max()
        .map_or(0, |id| id.saturating_add(1))
}

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
    sidebar_visible: bool,
    right_panel_visible: bool,
    panes: Arc<PaneRegistry>,
    control_state: Arc<Mutex<ControlState>>,
    pending_actions: Arc<Mutex<Vec<WorkspaceAction>>>,
    show_settings: bool,
    /// Set when settings closes and the main surface must take focus back
    /// (the settings surface held it while open; a stale focus would leave
    /// the shell's key handling dead until the user clicks something).
    restore_focus_pending: bool,
    tabs: Vec<OpenTab>,
    active_tab: usize,
    next_tab_id: usize,
    next_retained_chat_id: usize,
    retained_chats: Vec<RetainedChat>,
    next_pane_id: usize,
    working_directory: PathBuf,
    session: SessionStore,
    project_catalog: ProjectCatalog,
    worktree_label: String,
    terminal_breadcrumb: String,
    launch_snapshot: RestoredSession,
    tab_machinery: TabMachinery,
    overflow_menu_open: bool,
    tab_menu_open: bool,
    tab_menu_tab: Option<usize>,
    tab_rename: Option<TabRename>,
    palette_open: bool,
    palette_query: String,
    palette_selected: usize,
    palette_focus: FocusHandle,
    palette_previous_focus: Option<FocusHandle>,
    /// The single source of truth for agent lifecycle status. Every one of
    /// the sidebar dot, the tab checkmark, and the Activity row reads
    /// through this (or, for a chat pane, through `Chat`'s own state) —
    /// never a second, independently-tracked flag.
    activity: AgentActivityModel,
    /// Prevents scheduling restored scrollback more than once before the
    /// first frame mounts the terminal entities.
    restored_scrollback_scheduled: bool,
    browser_origins: BTreeSet<String>,
}

impl TillerWorkspace {
    #[allow(clippy::too_many_arguments)]
    fn new(
        titlebar: Entity<Titlebar>,
        sidebar: Entity<Sidebar>,
        tab_bar: Entity<TabBar>,
        status_bar: Entity<StatusBar>,
        settings: Entity<Settings>,
        right_panel: Entity<RightPanel>,
        panes: Arc<PaneRegistry>,
        control_state: Arc<Mutex<ControlState>>,
        tabs: Vec<OpenTab>,
        active_tab: usize,
        working_directory: PathBuf,
        actions: Arc<Mutex<Vec<WorkspaceAction>>>,
        pending_actions: Arc<Mutex<Vec<WorkspaceAction>>>,
        control_actions: Arc<Mutex<Vec<ControlAction>>>,
        session: SessionStore,
        project_catalog: ProjectCatalog,
        worktree_label: String,
        terminal_breadcrumb: String,
        launch_snapshot: RestoredSession,
        cx: &mut Context<Self>,
    ) -> Self {
        panes::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseTab, None),
            KeyBinding::new("ctrl-w", CloseTab, None),
        ]);
        bind_window_keys(cx);
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
                let pending_control = {
                    let Ok(mut actions) = control_actions.lock() else {
                        return;
                    };
                    std::mem::take(&mut *actions)
                };
                if pending.is_empty() && pending_control.is_empty() {
                    continue;
                }
                if this
                    .update_in(cx, |workspace, window, cx| {
                        for action in pending {
                            match action {
                                WorkspaceAction::NewTab(action) => {
                                    workspace.open_action(action, window, cx)
                                }
                                WorkspaceAction::NewChatAgent(id) => {
                                    workspace.open_chat_agent(id, window, cx);
                                }
                                WorkspaceAction::InstallSkill(command) => {
                                    workspace.add_terminal_tab_with_shell(
                                        "Install Skill",
                                        skill_install_shell(command),
                                        None,
                                        cx,
                                    );
                                }
                                WorkspaceAction::OpenSettings => {
                                    workspace.open_settings(None, cx);
                                }
                                WorkspaceAction::CloseSettings => {
                                    workspace.show_settings = false;
                                    workspace.restore_focus_pending = true;
                                    cx.notify();
                                }
                            }
                        }
                        for action in pending_control {
                            match action {
                                ControlAction::Quit { reply } => {
                                    let _ = reply.send(Ok(Vec::new()));
                                    cx.quit();
                                }
                                ControlAction::Notify { pane_id, status } => {
                                    let transition =
                                        workspace.activity.notify(&pane_id, status, Instant::now());
                                    workspace.post_activity_notification(&transition);
                                    workspace.sync_activity(cx);
                                }
                                ControlAction::SelectWorktree { selector, reply } => {
                                    let result = workspace.control_select_worktree(&selector, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::AddProject { path, reply } => {
                                    let result = workspace.control_add_project(&path, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::CreateWorkspace {
                                    project,
                                    branch,
                                    reply,
                                } => {
                                    let result =
                                        workspace.create_workspace(&project, branch.as_deref(), cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::CloseWorkspace { selector, reply } => {
                                    let result = workspace.close_workspace(&selector, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::RestoreSession { reply } => {
                                    let result = workspace.restore_launch_snapshot(window, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::OpenChanges { worktree, reply } => {
                                    let result =
                                        workspace.control_open_changes(worktree.as_deref(), cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::ReadChanges { reply } => {
                                    let result = workspace.control_read_changes(cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::OpenSettings { section, reply } => {
                                    let result = workspace.control_open_settings(section, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::SelectSettings { section, reply } => {
                                    let result = workspace.control_select_settings(section, cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::ReadSettings { reply } => {
                                    let result = workspace.control_read_settings(cx);
                                    let _ = reply.send(result);
                                }
                                ControlAction::ReadPane { id, query, reply } => {
                                    workspace.sync_control_panes(cx);
                                    let result = match query {
                                        PaneQuery::State => workspace
                                            .panes
                                            .state(&id)
                                            .map(|snapshot| panel_state_pairs(&snapshot))
                                            .map_err(|error| error.to_string()),
                                        PaneQuery::Scrollback(max_bytes) => workspace
                                            .panes
                                            .scrollback(&id, max_bytes)
                                            .map(|output| {
                                                vec![
                                                    ("data".to_string(), base64_encode(&output)),
                                                    ("bytes".to_string(), output.len().to_string()),
                                                ]
                                            })
                                            .map_err(|error| error.to_string()),
                                    };
                                    let _ = reply.send(result);
                                }
                                ControlAction::FocusPane {
                                    direction,
                                    forward,
                                    reply,
                                } => {
                                    workspace.focus_neighbor(direction, forward, None, cx);
                                    let _ = reply.send(Ok(Vec::new()));
                                }
                                ControlAction::SplitPane { direction, reply } => {
                                    workspace.split_focused_terminal(direction, None, cx);
                                    let _ = reply.send(Ok(Vec::new()));
                                }
                                ControlAction::ClosePane { reply } => {
                                    workspace.close_focused_pane(None, cx);
                                    let _ = reply.send(Ok(Vec::new()));
                                }
                                ControlAction::CycleTab { forward, reply } => {
                                    workspace.cycle_tab(forward, cx);
                                    let _ = reply.send(Ok(Vec::new()));
                                }
                                ControlAction::SelectTab { position, reply } => {
                                    workspace.select_tab_position(position, cx);
                                    let _ = reply.send(Ok(Vec::new()));
                                }
                                ControlAction::Browser {
                                    method,
                                    params,
                                    reply,
                                } => {
                                    let result = workspace
                                        .handle_browser_action(&method, &params, window, cx);
                                    let _ = reply.send(result);
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

        Self::subscribe_right_panel(&right_panel, cx);
        Self::bind_terminal_tabs(&tabs, cx);
        for tab in &tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Changes(changes) = content {
                    Self::subscribe_changes_tab(changes, cx);
                }
            });
        }

        cx.subscribe(
            &sidebar,
            |workspace, _, event: &SidebarEvent, cx| match event {
                SidebarEvent::AddProject(path) => workspace.add_project(path.clone(), cx),
                SidebarEvent::RemoveProject(id) => workspace.remove_project(id, cx),
                SidebarEvent::SelectTab(id) => workspace.select_tab(*id, cx),
                SidebarEvent::SelectWorktree(path) => {
                    let _ = workspace.select_worktree(path.clone(), cx);
                }
                SidebarEvent::CloseTab(id) => workspace.close_tab_by_id(*id, cx),
                SidebarEvent::OpenProjectSettings(id) => {
                    workspace
                        .sidebar
                        .update(cx, |sidebar, cx| sidebar.open_project_settings(id, cx));
                }
                SidebarEvent::Reorder {
                    drag,
                    target_id,
                    before,
                } => workspace.reorder_sidebar(*drag, *target_id, *before, cx),
                SidebarEvent::ContextAction { target, action } => {
                    workspace.handle_sidebar_context_action(target, *action, cx)
                }
            },
        )
        .detach();

        cx.subscribe(
            &titlebar,
            |workspace, _, event: &TitlebarEvent, cx| match event {
                TitlebarEvent::ToggleSidebar => workspace.toggle_sidebar(cx),
                TitlebarEvent::ToggleRightPanel => workspace.toggle_right_panel(cx),
            },
        )
        .detach();

        let tabs_len = tabs.len();
        let next_pane_id = next_pane_id(&tabs);
        let active_tab_id = tabs.get(active_tab).map(|tab| tab.id);
        let tab_machinery = TabMachinery::new(
            vec![TabGroup::new(
                0,
                tabs.iter().map(|tab| tab.id).collect(),
                active_tab_id,
            )],
            0,
        )
        .expect("restored tabs form one valid pane group");
        let browser_origins = session.load_browser_origin_grants().into_iter().collect();
        // P58, F-SET-10: the usage bar consumes the settings surface's
        // visibility toggles and refresh interval. Observing the settings
        // entity applies every change to the bar live, so a toggle in
        // settings takes effect without a relaunch.
        cx.observe(&settings, |workspace, _, cx| {
            let snapshot = workspace.settings.read(cx).snapshot();
            let prefs = tiller_ui::status_bar::UsageBarPrefs::from_snapshot(&snapshot);
            workspace
                .status_bar
                .update(cx, |bar, cx| bar.apply_preferences(prefs, cx));
        })
        .detach();
        let workspace = Self {
            titlebar,
            sidebar,
            tab_bar,
            status_bar,
            settings,
            right_panel,
            sidebar_visible: true,
            right_panel_visible: true,
            panes,
            control_state,
            pending_actions,
            tabs,
            active_tab,
            next_tab_id: tabs_len,
            next_retained_chat_id: 0,
            retained_chats: Vec::new(),
            next_pane_id,
            working_directory,
            session,
            project_catalog,
            worktree_label,
            terminal_breadcrumb,
            launch_snapshot,
            tab_machinery,
            overflow_menu_open: false,
            tab_menu_open: false,
            tab_menu_tab: None,
            tab_rename: None,
            palette_open: false,
            palette_query: String::new(),
            palette_selected: 0,
            palette_focus: cx.focus_handle(),
            palette_previous_focus: None,
            activity: AgentActivityModel::new(),
            restored_scrollback_scheduled: false,
            browser_origins,
            show_settings: false,
            restore_focus_pending: false,
        };
        // ctrl-shift-p is universal, including while the terminal owns focus.
        // An element-level listener is too late for embedded terminal input,
        // so intercept this one chord before GPUI dispatches to the focused
        // surface. ctrl-k intentionally remains in the shell's capture path
        // so readline keeps precedence in terminals.
        let workspace_ref = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, app| {
            let stroke = &event.keystroke;
            if stroke.key == "p" && stroke.modifiers.control && stroke.modifiers.shift {
                let _ = workspace_ref.update(app, |workspace, cx| {
                    workspace.open_command_palette(window, cx);
                    cx.stop_propagation();
                });
            }
        })
        .detach();
        workspace.seed_browser_origins(cx);
        workspace.schedule_save(cx);
        workspace.sync_activity(cx);
        workspace
    }

    /// The shell's current layout, in the shape persistence understands.
    fn layout(&self, cx: &App) -> SessionLayout {
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
                        TabKind::Browser => "browser",
                        TabKind::Diff => "diff",
                    }
                    .to_string(),
                    agent_id: tab.agent_id.clone(),
                    active: index == self.active_tab,
                })
                .collect(),
            tab_states: self
                .tabs
                .iter()
                .map(|tab| {
                    let mut state = tab.session_state.clone();
                    state.scrollback.clear();
                    tab.panes.for_each(&mut |pane_id, content| {
                        if let TabContent::Terminal { view } = content {
                            state
                                .scrollback
                                .insert(pane_id, view.read(cx).capture_scrollback());
                        }
                    });
                    state
                })
                .collect(),
        }
    }

    /// Records the current layout; the session store's debounce collapses a
    /// burst of changes into one database write.
    fn schedule_save(&self, cx: &App) {
        self.session.schedule(self.layout(cx));
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
                        is_primary: worktree.is_primary,
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

    fn sync_control_state(&self) {
        let state_path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_workspace()
            .map(|workspace| PathBuf::from(&workspace.path))
            .unwrap_or_else(|| self.working_directory.clone());
        *self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            ControlState::from_catalog(&self.project_catalog, &state_path);
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        self.right_panel_visible = !self.right_panel_visible;
        cx.notify();
    }

    fn subscribe_right_panel(right_panel: &Entity<RightPanel>, cx: &mut Context<Self>) {
        cx.subscribe(
            right_panel,
            |workspace, _, event: &RightPanelEvent, cx| match event {
                RightPanelEvent::SelectActivity(index) => workspace.select_activity(*index, cx),
                RightPanelEvent::CloseActivity(index) => workspace.close_tab(*index, cx),
                RightPanelEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            },
        )
        .detach();
        cx.subscribe(
            right_panel,
            |workspace, _, event: &RightPanelActionEvent, cx| match event {
                RightPanelActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
            },
        )
        .detach();
    }

    fn bind_terminal(
        terminal: &Entity<TerminalView>,
        tab_id: usize,
        pane_id: usize,
        cx: &mut Context<Self>,
    ) {
        terminal.update(cx, |terminal, _| {
            terminal.set_identity(TerminalIdentity::new(
                format!("pane-{pane_id}"),
                format!("terminal-{pane_id}"),
            ));
        });
        Self::subscribe_terminal(terminal, tab_id, pane_id, cx);
        Self::subscribe_terminal_link(terminal, pane_id, cx);
        Self::subscribe_terminal_activity(terminal, pane_id, cx);
        Self::start_process_signal_refresh(terminal, tab_id, pane_id, cx);
    }

    fn bind_terminal_tabs(tabs: &[OpenTab], cx: &mut Context<Self>) {
        for tab in tabs {
            let tab_id = tab.id;
            tab.panes.for_each(&mut |pane_id, content| {
                if let TabContent::Terminal { view } = content {
                    Self::bind_terminal(view, tab_id, pane_id, cx);
                }
            });
        }
    }

    fn subscribe_terminal(
        terminal: &Entity<TerminalView>,
        tab_id: usize,
        pane_id: usize,
        cx: &mut Context<Self>,
    ) {
        let expected_pane_id = format!("pane-{pane_id}");
        cx.subscribe(
            terminal,
            move |workspace, _, event: &TerminalContextEvent, cx| {
                if event.target.pane_id() != expected_pane_id {
                    return;
                }
                match delegated_terminal_context_action(event.action) {
                    Some(TerminalContextCommand::SetTitle) => workspace.set_terminal_title(
                        tab_id,
                        pane_id,
                        event.target.terminal_id(),
                        cx,
                    ),
                    Some(TerminalContextCommand::Split {
                        direction,
                        placement,
                    }) => {
                        workspace.split_terminal_at_with_placement(
                            tab_id, pane_id, direction, placement, None, cx,
                        );
                    }
                    Some(TerminalContextCommand::Close) => {
                        workspace.close_terminal_at(tab_id, pane_id, None, cx);
                    }
                    None => {}
                }
            },
        )
        .detach();
    }

    fn subscribe_terminal_link(
        terminal: &Entity<TerminalView>,
        pane_id: usize,
        cx: &mut Context<Self>,
    ) {
        let expected_pane_id = format!("pane-{pane_id}");
        cx.subscribe(
            terminal,
            move |_, _, event: &TerminalLinkEvent, cx| {
                if let Some(url) = terminal_link_url_for_pane(event, &expected_pane_id) {
                    cx.open_url(url);
                }
            },
        )
        .detach();
    }

    fn subscribe_terminal_activity(
        terminal: &Entity<TerminalView>,
        pane_id: usize,
        cx: &mut Context<Self>,
    ) {
        let activity_pane_id = format!("pane-{pane_id}");
        cx.subscribe(
            terminal,
            move |workspace, _, event: &TerminalActivityEvent, cx| {
                let transition = panes::apply_terminal_activity_event(
                    &mut workspace.activity,
                    &activity_pane_id,
                    event,
                    Instant::now(),
                );
                if let Some(transition) = transition {
                    workspace.post_activity_notification(&transition);
                }
                // Terminal exit status is stored on TerminalView even when
                // no agent activity transition exists. Repaint the shell so
                // the tab status cell can show the concrete exit/signal.
                workspace.sync_activity(cx);
                cx.notify();
            },
        )
        .detach();
    }

    fn start_process_signal_refresh(
        terminal: &Entity<TerminalView>,
        tab_id: usize,
        pane_id: usize,
        cx: &mut Context<Self>,
    ) {
        let terminal = terminal.clone();
        let workspace = cx.weak_entity();
        cx.spawn(async move |_this, cx| {
            loop {
                cx.background_executor()
                    .timer(panes::PROCESS_SIGNAL_INTERVAL)
                    .await;

                let shell_pid = terminal.read_with(cx, |terminal, _| terminal.shell_pid());
                let terminal_id = terminal.entity_id();
                let keep_running = match workspace.update(cx, |workspace, cx| {
                    let Some(bound_terminal) = workspace.terminal_for_pane(tab_id, pane_id) else {
                        return false;
                    };
                    if bound_terminal.entity_id() != terminal_id {
                        return false;
                    }
                    let Some(shell_pid) = shell_pid else {
                        return true;
                    };
                    match panes::refresh_process_signal(
                        &mut workspace.activity,
                        &format!("pane-{pane_id}"),
                        shell_pid,
                    ) {
                        Ok(Some(transition)) => {
                            workspace.post_activity_notification(&transition);
                            workspace.sync_activity(cx);
                        }
                        Ok(None) => workspace.sync_activity(cx),
                        Err(error) => eprintln!(
                            "[activity] process refresh failed for pane-{pane_id}: {error}"
                        ),
                    }
                    true
                }) {
                    Ok(keep_running) => keep_running,
                    Err(_) => return,
                };
                if !keep_running {
                    return;
                }
            }
        })
        .detach();
    }

    fn terminal_for_pane(&self, tab_id: usize, pane_id: usize) -> Option<Entity<TerminalView>> {
        let tab = self.tabs.iter().find(|tab| tab.id == tab_id)?;
        let mut terminal = None;
        tab.panes.for_each(&mut |candidate_pane_id, content| {
            if candidate_pane_id == pane_id {
                terminal = content.terminal();
            }
        });
        terminal
    }

    fn set_terminal_title(
        &mut self,
        tab_id: usize,
        pane_id: usize,
        terminal_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return;
        };
        if !tab.panes.contains(pane_id) {
            return;
        }
        // The Linux context event carries identity, not text. Use that stable
        // identity as the app-owned title until a text-entry prompt is added.
        tab.title = format!("Terminal {terminal_id}");
        self.sync_activity(cx);
        self.schedule_save(cx);
        cx.notify();
    }

    fn add_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.project_catalog.add(&path) {
            Ok(true) => {
                self.session.schedule_catalog(&self.project_catalog);
                self.sync_control_state();
                self.refresh_sidebar(cx);
            }
            Ok(false) => {
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_notice(format!("already tracked or nested: {}", path.display()), cx)
                });
                cx.notify();
            }
            Err(error) => {
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_notice(error, cx));
                cx.notify();
            }
        }
    }

    fn remove_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.project_catalog.remove(id) {
            self.session.schedule_catalog(&self.project_catalog);
            self.sync_control_state();
            self.refresh_sidebar(cx);
        }
    }

    fn reorder_sidebar(
        &mut self,
        drag: RowDrag,
        target_id: usize,
        before: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = match drag.scope {
            ReorderScope::Projects => {
                self.project_catalog
                    .reorder_projects(drag.id / 1000, target_id / 1000, before)
            }
            ReorderScope::Worktrees => {
                let Some(project_row_id) = drag.group else {
                    return;
                };
                let Some(target_project_row_id) = target_id.checked_div(1000) else {
                    return;
                };
                if project_row_id / 1000 != target_project_row_id {
                    return;
                }
                let Some(from) = drag.id.checked_sub(project_row_id + 1) else {
                    return;
                };
                let Some(target) = target_id.checked_sub(project_row_id + 1) else {
                    return;
                };
                self.project_catalog
                    .reorder_worktrees(project_row_id / 1000, from, target, before)
            }
            ReorderScope::Tabs => {
                let Some(from_id) = drag.id.checked_sub(TAB_ROW_ID_OFFSET) else {
                    return;
                };
                let Some(target_id) = target_id.checked_sub(TAB_ROW_ID_OFFSET) else {
                    return;
                };
                self.reorder_tabs_by_id(from_id, target_id, before, None)
            }
        };
        if !changed {
            return;
        }
        match drag.scope {
            ReorderScope::Projects | ReorderScope::Worktrees => {
                self.session.schedule_catalog(&self.project_catalog);
                self.sync_control_state();
                self.refresh_sidebar(cx);
            }
            ReorderScope::Tabs => {
                self.schedule_save(cx);
                self.sync_activity(cx);
                cx.notify();
            }
        }
    }

    fn reorder_tabs_by_id(
        &mut self,
        from_id: usize,
        target_id: usize,
        before: bool,
        group: Option<usize>,
    ) -> bool {
        let Some(from) = self.tabs.iter().position(|tab| tab.id == from_id) else {
            return false;
        };
        let Some(target) = self.tabs.iter().position(|tab| tab.id == target_id) else {
            return false;
        };
        if from == target
            || group.is_some_and(|group| {
                self.tabs[from].group_id != group || self.tabs[target].group_id != group
            })
        {
            return false;
        }
        let active_id = self.tabs.get(self.active_tab).map(|tab| tab.id);
        let tab = self.tabs.remove(from);
        let target_after_remove = if from < target { target - 1 } else { target };
        let insert_at = (target_after_remove + usize::from(!before)).min(self.tabs.len());
        self.tabs.insert(insert_at, tab);
        self.active_tab = active_id
            .and_then(|id| self.tabs.iter().position(|tab| tab.id == id))
            .unwrap_or(self.active_tab.min(self.tabs.len().saturating_sub(1)));
        self.rebuild_tab_machinery();
        true
    }

    fn preview_tab_reorder(
        &mut self,
        drag: RowDrag,
        target_id: usize,
        before: bool,
        cx: &mut Context<Self>,
    ) {
        if drag.scope != ReorderScope::Tabs {
            return;
        }
        if self.reorder_tabs_by_id(drag.id, target_id, before, drag.group) {
            self.schedule_save(cx);
            self.sync_activity(cx);
            cx.notify();
        }
    }

    fn control_add_project(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let added = self.project_catalog.add(path)?;
        if added {
            self.session.schedule_catalog(&self.project_catalog);
            self.sync_control_state();
            self.refresh_sidebar(cx);
        }
        let state = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let project = path.canonicalize().ok().and_then(|path| {
            state.projects.iter().find(|project| {
                project.root_path == path
                    || project
                        .worktrees
                        .iter()
                        .any(|worktree| worktree.path == path)
            })
        });
        let mut result = vec![("added".to_string(), added.to_string())];
        if let Some(project) = project {
            result.extend([
                ("projectId".to_string(), project.id.clone()),
                (
                    "worktreeCount".to_string(),
                    project.worktrees.len().to_string(),
                ),
            ]);
        }
        Ok(result)
    }

    fn handle_sidebar_context_action(
        &mut self,
        target: &SidebarContextTarget,
        action: SidebarContextAction,
        cx: &mut Context<Self>,
    ) {
        match (target, action) {
            (SidebarContextTarget::Project { id, .. }, SidebarContextAction::ProjectSettings) => {
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.open_project_settings(id, cx));
            }
            (
                SidebarContextTarget::Project { id, path, is_git },
                SidebarContextAction::InitializeGit,
            ) => {
                if *is_git {
                    return;
                }
                let project_id = id.clone();
                let path = path.clone();
                let sidebar = self.sidebar.clone();
                let session = self.session.clone();
                cx.spawn(async move |this, cx| {
                    let result =
                        cx.background_executor()
                            .spawn(async move {
                                init_repository(&path).map_err(|error| error.to_string())
                            })
                            .await;
                    let _ = this.update(cx, |workspace, cx| match result {
                        Ok(()) => {
                            if let Err(error) =
                                workspace.project_catalog.refresh_project(&project_id)
                            {
                                sidebar.update(cx, |sidebar, cx| sidebar.set_notice(error, cx));
                                return;
                            }
                            session.schedule_catalog(&workspace.project_catalog);
                            workspace.sync_control_state();
                            workspace.refresh_sidebar(cx);
                        }
                        Err(error) => {
                            sidebar.update(cx, |sidebar, cx| sidebar.set_notice(error, cx));
                        }
                    });
                })
                .detach();
            }
            (
                SidebarContextTarget::Project { path, .. },
                SidebarContextAction::RevealInFileManager,
            ) => {
                if let Err(error) = Command::new("xdg-open").arg(path).spawn() {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_notice(format!("could not open file manager: {error}"), cx)
                    });
                }
            }
            (SidebarContextTarget::Worktree { path, .. }, SidebarContextAction::SetPrimary) => {
                self.set_worktree_primary(path, true, cx)
            }
            (SidebarContextTarget::Worktree { path, .. }, SidebarContextAction::UnsetPrimary) => {
                self.set_worktree_primary(path, false, cx)
            }
            (SidebarContextTarget::Worktree { path, .. }, SidebarContextAction::NewTab(action)) => {
                if *path != self.working_directory
                    && self.select_worktree(path.clone(), cx).is_err()
                {
                    return;
                }
                if let Ok(mut actions) = self.pending_actions.lock() {
                    actions.push(WorkspaceAction::NewTab(action));
                }
            }
            (_, SidebarContextAction::RemoveProject) => {}
            (_, SidebarContextAction::SetPrimary | SidebarContextAction::UnsetPrimary) => {}
            (_, SidebarContextAction::NewTab(_)) => {}
            (
                _,
                SidebarContextAction::ProjectSettings
                | SidebarContextAction::InitializeGit
                | SidebarContextAction::RevealInFileManager,
            ) => {}
        }
    }

    fn set_worktree_primary(&mut self, path: &Path, primary: bool, cx: &mut Context<Self>) {
        match self.project_catalog.set_primary(path, primary) {
            Ok(()) => {
                self.session.schedule_catalog(&self.project_catalog);
                self.sync_control_state();
                self.refresh_sidebar(cx);
            }
            Err(error) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.set_notice(error, cx)),
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
                        Some(ActivityStatus::Idle)
                    }
                }
                TabContent::Terminal { view } => {
                    let terminal = view.read(cx);
                    if terminal.is_failed() {
                        Some(ActivityStatus::Error)
                    } else if let Some(exit_status) = terminal.exit_status() {
                        Some(match exit_status {
                            TerminalExitStatus::Success => ActivityStatus::Done,
                            TerminalExitStatus::Code(_)
                            | TerminalExitStatus::Signal(_)
                            | TerminalExitStatus::Unknown => ActivityStatus::Error,
                        })
                    } else {
                        let pane_status = self
                            .activity
                            .status(&format!("pane-{pane_id}"))
                            .or_else(|| self.activity.status(&format!("tab-{}", tab.id)));
                        Some(pane_status.map_or(ActivityStatus::Idle, activity_status_for_agent))
                    }
                }
                TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => None,
            };
            if candidate.is_some_and(|candidate| {
                status.is_none_or(|current| {
                    Self::status_priority(candidate) < Self::status_priority(current)
                })
            }) {
                status = candidate;
            }
        });
        status
    }

    fn terminal_exit_label(tab: &OpenTab, cx: &App) -> Option<String> {
        let mut label = None;
        tab.panes.for_each(&mut |_, content| {
            if let TabContent::Terminal { view } = content
                && let Some(status) = view.read(cx).exit_status()
            {
                label = Some(match status {
                    TerminalExitStatus::Success => "exit 0".to_string(),
                    TerminalExitStatus::Code(code) => format!("exit {code}"),
                    TerminalExitStatus::Signal(signal) => format!("signal {signal}"),
                    TerminalExitStatus::Unknown => "exit unknown".to_string(),
                });
            }
        });
        label
    }

    fn status_priority(status: ActivityStatus) -> u8 {
        match status {
            ActivityStatus::Error => 0,
            ActivityStatus::NeedsInput => 1,
            ActivityStatus::Running => 1,
            ActivityStatus::Idle => 2,
            ActivityStatus::Done => 3,
        }
    }

    /// The highest-priority status among the open tabs (error > running
    /// > needs-input > done), for the worktree row's single dot — one aggregate
    /// > of the same per-tab facts the tab checkmarks and Activity rows show,
    /// > not an independently-decided fourth state.
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

    fn sync_control_panes(&self, cx: &App) {
        let mut panes = Vec::new();
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            tab.panes.for_each(&mut |pane_id, content| {
                let (title, agent, state) = match content {
                    TabContent::Chat(_) => (
                        "Chat".to_string(),
                        String::new(),
                        PaneStateSnapshot {
                            working_directory: self.working_directory.clone(),
                            scrollback: Vec::new(),
                            exit_status: None,
                        },
                    ),
                    TabContent::Terminal { view } => {
                        let agent = tab.agent_id.clone().unwrap_or_default();
                        let state = panel_state_from_terminal(view.read(cx).snapshot());
                        (tab.title.clone(), agent, state)
                    }
                    TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => (
                        tab.title.clone(),
                        String::new(),
                        PaneStateSnapshot {
                            working_directory: self.working_directory.clone(),
                            scrollback: Vec::new(),
                            exit_status: None,
                        },
                    ),
                };
                panes.push((
                    PaneInfo {
                        id: format!("pane-{pane_id}"),
                        tab: tab.title.clone(),
                        title,
                        agent,
                        active: tab_index == self.active_tab && pane_id == tab.focused_pane,
                    },
                    state,
                ));
            });
        }
        if let Err(error) = self
            .panes
            .set_external_state(&self.working_directory, panes)
        {
            eprintln!("[control] failed to snapshot application panes: {error}");
        }
    }

    fn sidebar_worktree_id(&self, path: &Path) -> Option<usize> {
        self.project_catalog
            .projects()
            .iter()
            .enumerate()
            .find_map(|(project_index, project)| {
                project
                    .worktrees
                    .iter()
                    .enumerate()
                    .find(|(_, worktree)| worktree.path == path)
                    .map(|(worktree_index, _)| project_index * 1000 + worktree_index + 1)
            })
    }

    fn select_worktree(
        &mut self,
        requested_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(selected_path) = self
            .project_catalog
            .projects()
            .iter()
            .flat_map(|project| project.worktrees.iter())
            .find(|worktree| worktree.path == requested_path)
            .map(|worktree| worktree.path.clone())
        else {
            return Err(format!("unknown worktree: {}", requested_path.display()));
        };

        let old_path = self.working_directory.clone();
        let old_sidebar_id = self.sidebar_worktree_id(&old_path);
        let new_sidebar_id = self.sidebar_worktree_id(&selected_path);
        if !self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .select_worktree(&selected_path)
        {
            return Err(format!("unknown worktree: {}", selected_path.display()));
        }

        let context = worktree_context(&self.project_catalog, &selected_path);
        self.working_directory = selected_path.clone();
        self.worktree_label = context.activity_label;
        self.terminal_breadcrumb = context.terminal_breadcrumb;
        self.rebind_changes_tabs(cx);
        if let Err(error) = self.panes.set_external(&old_path, Vec::new()) {
            eprintln!("[control] failed to clear old worktree panes: {error}");
        }

        let pending_actions = self.pending_actions.clone();
        let status_data = UsageBarData {
            branch: context.branch,
            path: context.path,
        };
        // Rebuilt bars start from the settings surface's current values
        // (P58, F-SET-10): visibility and interval must survive a worktree
        // switch, not reset to the defaults.
        let bar_prefs =
            tiller_ui::status_bar::UsageBarPrefs::from_snapshot(&self.settings.read(cx).snapshot());
        self.status_bar = cx.new(|_| {
            StatusBar::new(status_data)
                .with_preferences(bar_prefs)
                .on_settings(move || {
                    if let Ok(mut actions) = pending_actions.lock() {
                        actions.push(WorkspaceAction::OpenSettings);
                    }
                })
        });

        let activity = self.activity_surfaces(cx);
        let selected_path_for_panel = selected_path.clone();
        self.right_panel = cx.new(|_| RightPanel::with_activity(selected_path_for_panel, activity));
        Self::subscribe_right_panel(&self.right_panel, cx);

        if old_sidebar_id != new_sidebar_id
            && let Some(old_sidebar_id) = old_sidebar_id
        {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_worktree_status(old_sidebar_id, None, cx);
                sidebar.set_worktree_tabs(old_sidebar_id, Vec::new(), cx);
            });
        }
        self.sync_activity(cx);
        self.sidebar.update(cx, |sidebar, cx| {
            // The host has updated its current path, control snapshot, status
            // bar and tab home before answering the click. The row highlight
            // is the final confirmation of that state transition.
            sidebar.set_selected_worktree(&selected_path, cx);
        });
        self.schedule_save(cx);
        cx.notify();
        Ok(())
    }

    fn control_select_worktree(
        &mut self,
        selector: &str,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .path_for_selector(selector)
            .ok_or_else(|| format!("unknown worktree: {selector}"))?;
        self.select_worktree(path, cx)?;
        let state = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let workspace = state
            .current_workspace()
            .ok_or_else(|| "worktree selection did not produce a current workspace".to_string())?;
        Ok(vec![
            ("id".to_string(), workspace.id.clone()),
            ("project".to_string(), workspace.project.clone()),
            ("branch".to_string(), workspace.branch.clone()),
            ("path".to_string(), workspace.path.clone()),
        ])
    }

    fn create_workspace(
        &mut self,
        project_selector: &str,
        requested_branch: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let project_index = self
            .project_catalog
            .projects()
            .iter()
            .position(|project| {
                project.id == project_selector
                    || project.name == project_selector
                    || project.root_path.to_string_lossy() == project_selector
            })
            .ok_or_else(|| format!("unknown project: {project_selector}"))?;
        let project = &self.project_catalog.projects()[project_index];
        if !project.is_git {
            return Err(format!("project {} is not a Git repository", project.name));
        }
        let branch = requested_branch
            .filter(|branch| !branch.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(generated_worktree_branch);
        let path = new_worktree_path(&project.name, &branch);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create worktree parent: {error}"))?;
        }
        let output = Command::new("git")
            .args(["worktree", "add", "-b", &branch])
            .arg(&path)
            .current_dir(&project.root_path)
            .output()
            .map_err(|error| format!("cannot run git worktree add: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let mut projects = self.project_catalog.projects().to_vec();
        let project_id = projects[project_index].id.clone();
        projects[project_index]
            .worktrees
            .push(session::CatalogWorktree {
                branch: branch.clone(),
                path: path.clone(),
                is_primary: false,
            });
        self.project_catalog = ProjectCatalog::from_projects(projects);
        self.session.schedule_catalog(&self.project_catalog);

        let current_path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_workspace()
            .map(|workspace| PathBuf::from(&workspace.path));
        let state_path = current_path.as_deref().unwrap_or_else(|| Path::new(""));
        *self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            ControlState::from_catalog(&self.project_catalog, state_path);
        self.refresh_sidebar(cx);
        cx.notify();

        let worktree_index = self.project_catalog.projects()[project_index]
            .worktrees
            .len()
            - 1;
        Ok(vec![
            (
                "id".to_string(),
                format!("{}-wt-{worktree_index}", project_id),
            ),
            ("branch".to_string(), branch),
            ("path".to_string(), path.to_string_lossy().into_owned()),
        ])
    }

    fn close_workspace(
        &mut self,
        selector: &str,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .path_for_selector(selector)
            .ok_or_else(|| format!("unknown worktree: {selector}"))?;
        let was_current = self.working_directory == path
            && self
                .control_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .current_workspace()
                .is_some();
        if let Err(error) = self.panes.shutdown_for(&path) {
            eprintln!(
                "[control] failed to terminate panes for closed worktree {}: {error}",
                path.display()
            );
        }
        if was_current {
            for index in (0..self.tabs.len()).rev() {
                self.close_tab(index, cx);
            }
            if let Err(error) = self.panes.set_external(&path, Vec::new()) {
                eprintln!("[control] failed to clear closed worktree panes: {error}");
            }
        }
        if !self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .close_worktree(&path)
        {
            return Err(format!("unknown worktree: {selector}"));
        }
        if was_current {
            self.sync_activity(cx);
            self.schedule_save(cx);
            cx.notify();
        }
        Ok(vec![
            ("closed".to_string(), "true".to_string()),
            ("path".to_string(), path.to_string_lossy().into_owned()),
        ])
    }

    fn restore_launch_snapshot(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let snapshot = self.launch_snapshot.clone();
        self.select_worktree(snapshot.working_directory.clone(), cx)?;

        let current = self.layout(cx).tabs;
        let merged = merge_launch_snapshot_tabs(&snapshot.tabs, &current);
        let missing = merged.into_iter().skip(current.len()).collect::<Vec<_>>();
        let restored_count = missing.len();
        if !missing.is_empty() {
            let restored = RestoredSession {
                working_directory: self.working_directory.clone(),
                tabs: missing,
                tab_states: Vec::new(),
                diagnostics: Vec::new(),
            };
            let (tabs, _) = restore_tabs_in_workspace(
                &restored,
                &self.working_directory,
                self.next_tab_id,
                self.next_pane_id,
                window,
                cx,
            );
            Self::bind_terminal_tabs(&tabs, cx);
            self.next_tab_id += tabs.len();
            self.tabs.extend(tabs);
            self.next_pane_id = next_pane_id(&self.tabs);
            self.rebuild_tab_machinery();
        }
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
        Ok(vec![
            ("restoredCount".to_string(), restored_count.to_string()),
            (
                "path".to_string(),
                snapshot.working_directory.to_string_lossy().into_owned(),
            ),
        ])
    }

    fn sync_activity(&self, cx: &mut Context<Self>) {
        self.sync_control_panes(cx);
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
        if let Some(worktree_id) = self.sidebar_worktree_id(&self.working_directory) {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_worktree_status(worktree_id, worktree_status, cx);
                sidebar.set_worktree_tabs(worktree_id, sidebar_tabs, cx);
            });
        }
    }

    fn post_activity_notification(&self, transition: &Transition) {
        let Some(agent_id) = self.activity.agent_id(&transition.pane_id) else {
            return;
        };
        let visible = self.tabs.get(self.active_tab).is_some_and(|tab| {
            let pane_id = transition.pane_id.strip_prefix("pane-");
            pane_id.is_some_and(|pane_id| {
                pane_id
                    .parse::<usize>()
                    .is_ok_and(|pane_id| tab.panes.contains(pane_id))
            })
        });
        if !NotificationPolicy::should_notify(transition.old, transition.new, true, visible) {
            return;
        }
        let agent_display_name = AGENT_CATALOG
            .iter()
            .find(|adapter| adapter.id() == agent_id)
            .map_or(agent_id, |adapter| adapter.display_name());
        let context = worktree_context(&self.project_catalog, &self.working_directory);
        let worktree_id = self
            .sidebar_worktree_id(&self.working_directory)
            .map_or_else(
                || self.working_directory.to_string_lossy().into_owned(),
                |id| id.to_string(),
            );
        let project_name = self
            .project_catalog
            .projects()
            .iter()
            .find(|project| {
                project
                    .worktrees
                    .iter()
                    .any(|worktree| worktree.path == self.working_directory)
            })
            .map(|project| project.name.as_str());
        let Some(payload) = self.activity.build_payload(
            &transition.pane_id,
            transition.new,
            agent_display_name,
            &worktree_id,
            &context.branch,
            project_name,
            None,
        ) else {
            return;
        };
        post_desktop_notification(&payload);
    }

    fn seam(&self) -> impl IntoElement {
        div().w(px(SEAM_WIDTH)).h_full().bg(gpui::black())
    }

    fn tab_width(kind: TabKind) -> f32 {
        match kind {
            TabKind::AgentChat | TabKind::Editor | TabKind::Diff => CHAT_TAB_WIDTH,
            TabKind::Terminal => TERMINAL_TAB_WIDTH,
            TabKind::Browser => CHAT_TAB_WIDTH,
        }
    }

    fn tab_render_width(tab: &OpenTab) -> f32 {
        if tab_has_file(tab) {
            180.0
        } else {
            Self::tab_width(tab.kind)
        }
    }

    fn select_tab(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
            self.active_tab = index;
            let group_id = self.tabs[index].group_id;
            self.tab_machinery.select_tab(group_id, id);
            self.schedule_save(cx);
            self.sync_activity(cx);
            cx.notify();
        }
    }

    /// Rebuilds the pure placement model after a tab has been created or a
    /// restored snapshot has been merged. Existing group ids and selections
    /// survive; a newly-created tab joins the current group.
    fn rebuild_tab_machinery(&mut self) {
        let old_active_group = self.tab_machinery.active_group();
        let old_active_tabs = self
            .tab_machinery
            .groups()
            .iter()
            .map(|group| (group.id, group.active_tab))
            .collect::<std::collections::BTreeMap<_, _>>();
        let desired_active = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.group_id, tab.id));
        let mut group_ids = self
            .tab_machinery
            .groups()
            .iter()
            .map(|group| group.id)
            .collect::<Vec<_>>();
        for tab in &self.tabs {
            if !group_ids.contains(&tab.group_id) {
                group_ids.push(tab.group_id);
            }
        }
        if group_ids.is_empty() {
            group_ids.push(0);
        }

        let groups = group_ids
            .into_iter()
            .map(|group_id| {
                let tabs = self
                    .tabs
                    .iter()
                    .filter(|tab| tab.group_id == group_id)
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                let active_tab = desired_active
                    .filter(|(active_group, active)| {
                        *active_group == group_id && tabs.contains(active)
                    })
                    .map(|(_, active)| active)
                    .or_else(|| {
                        old_active_tabs
                            .get(&group_id)
                            .copied()
                            .flatten()
                            .filter(|active| tabs.contains(active))
                    })
                    .or_else(|| tabs.first().copied());
                TabGroup::new(group_id, tabs, active_tab)
            })
            .collect::<Vec<_>>();
        let active_group = if groups.iter().any(|group| group.id == old_active_group) {
            old_active_group
        } else {
            self.tabs
                .get(self.active_tab)
                .map(|tab| tab.group_id)
                .unwrap_or(groups[0].id)
        };
        self.tab_machinery = TabMachinery::new(groups, active_group)
            .expect("workspace tabs must form a valid tab placement model");
    }

    /// Applies a placement transition to the live tab entities, preserving
    /// the pure model's order and making the moved tab active.
    fn apply_tab_machinery(&mut self, machinery: TabMachinery) {
        let mut remaining = std::mem::take(&mut self.tabs);
        let mut ordered = Vec::with_capacity(remaining.len());
        for group in machinery.groups() {
            for tab_id in &group.tabs {
                if let Some(index) = remaining.iter().position(|tab| tab.id == *tab_id) {
                    let mut tab = remaining.remove(index);
                    tab.group_id = group.id;
                    ordered.push(tab);
                }
            }
        }
        ordered.extend(remaining);
        self.tabs = ordered;
        self.tab_machinery = machinery;
        if let Some(active_id) = self.tab_machinery.active_tab()
            && let Some(index) = self.tabs.iter().position(|tab| tab.id == active_id)
        {
            self.active_tab = index;
        } else if self.tabs.is_empty() {
            self.active_tab = 0;
        } else {
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        }
    }

    fn subscribe_changes_tab(tab: &Entity<ChangesTab>, cx: &mut Context<Self>) {
        cx.subscribe(
            tab,
            |workspace, _, event: &ChangesTabEvent, cx| match event {
                ChangesTabEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            },
        )
        .detach();
        cx.subscribe(
            tab,
            |workspace, _, event: &ChangesTabActionEvent, cx| match event {
                ChangesTabActionEvent::OpenDiff(_path) => workspace.add_changes_tab(cx),
                ChangesTabActionEvent::ResolveInTerminal(path) => {
                    workspace.add_conflict_terminal_tab(path.clone(), cx)
                }
            },
        )
        .detach();
    }

    fn rebind_changes_tabs(&mut self, cx: &mut Context<Self>) {
        let working_directory = self.working_directory.clone();
        for tab in &mut self.tabs {
            if tab.kind != TabKind::Diff {
                continue;
            }
            let pane_id = tab.focused_pane;
            let changes = cx.new(|cx| ChangesTab::new(working_directory.clone(), cx));
            Self::subscribe_changes_tab(&changes, cx);
            tab.panes = PaneNode::leaf(pane_id, TabContent::Changes(changes));
        }
    }

    fn select_activity(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab = index;
            let tab = &self.tabs[index];
            self.tab_machinery.select_tab(tab.group_id, tab.id);
            self.schedule_save(cx);
            self.sync_activity(cx);
            cx.notify();
        }
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        let tab_id = self.tabs[index].id;
        let mut machinery = self.tab_machinery.clone();
        machinery.remove_tab(tab_id);

        let retained_chat = {
            let tab = &self.tabs[index];
            let mut retained = None;
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Chat(chat) = content {
                    retained = Some(RetainedChat {
                        id: self.next_retained_chat_id,
                        title: tab.title.clone(),
                        transcript: chat.read(cx).transcript_for_resume(),
                        agent_id: tab.agent_id.clone(),
                    });
                }
            });
            retained
        };

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
        if self.tab_menu_tab == Some(tab_id) {
            self.tab_menu_tab = None;
            self.tab_menu_open = false;
        }
        if let Some(retained_chat) = retained_chat {
            self.next_retained_chat_id += 1;
            self.retained_chats.push(retained_chat);
        }
        self.apply_tab_machinery(machinery);
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn request_close_other_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(removed) = self.tab_command_machinery().close_others() else {
            return;
        };
        self.request_close_ids(removed, window, cx);
    }

    fn request_close_tabs_to_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(removed) = self.tab_command_machinery().close_tabs_to_right() else {
            return;
        };
        self.request_close_ids(removed, window, cx);
    }

    fn request_close_ids(&mut self, ids: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) {
        if ids.is_empty() {
            return;
        }
        let dirty = ids.iter().any(|id| {
            self.tabs
                .iter()
                .find(|tab| tab.id == *id)
                .is_some_and(|tab| self.tab_is_dirty(tab, cx))
        });
        if !dirty {
            for tab_id in ids {
                if let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) {
                    self.close_tab(index, cx);
                }
            }
            return;
        }
        let detail = format!(
            "Discard unsaved work in {}?",
            ids.iter()
                .filter_map(|id| self.tabs.iter().find(|tab| tab.id == *id))
                .map(|tab| tab.title.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            "Close dirty tabs?",
            Some(&detail),
            &["Close", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.unwrap_or(1) != 0 {
                return;
            }
            let _ = this.update(cx, |workspace, cx| {
                for tab_id in ids {
                    workspace.close_tab_by_id(tab_id, cx);
                }
            });
        })
        .detach();
    }

    fn move_active_tab(&mut self, direction: MoveDirection, cx: &mut Context<Self>) {
        let mut machinery = self.tab_command_machinery();
        if !machinery.move_active_tab(direction) {
            return;
        }
        self.apply_tab_machinery(machinery);
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    /// Context-menu commands operate on the tab that opened the menu, which
    /// may not be the workspace's currently active tab. The pure model still
    /// owns the transition; this helper only projects that menu selection into
    /// a clone before asking the model to apply it.
    fn tab_command_machinery(&self) -> TabMachinery {
        let mut machinery = self.tab_machinery.clone();
        let Some(tab_id) = self.tab_menu_tab else {
            return machinery;
        };
        if let Some(group_id) = machinery
            .groups()
            .iter()
            .find(|group| group.tabs.contains(&tab_id))
            .map(|group| group.id)
        {
            let _ = machinery.select_tab(group_id, tab_id);
        }
        machinery
    }

    fn move_selected_tab(&mut self, target: MoveTarget, cx: &mut Context<Self>) {
        let Some(tab_id) = self
            .tab_menu_tab
            .or_else(|| self.tabs.get(self.active_tab).map(|tab| tab.id))
        else {
            return;
        };
        let mut machinery = self.tab_machinery.clone();
        if machinery.move_tab(tab_id, target).is_err() {
            return;
        }
        self.move_selected_tab_with_machinery(target, machinery, cx);
    }

    fn move_selected_tab_with_machinery(
        &mut self,
        _target: MoveTarget,
        machinery: TabMachinery,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self
            .tab_menu_tab
            .or_else(|| self.tabs.get(self.active_tab).map(|tab| tab.id))
        else {
            return;
        };
        if machinery
            .groups()
            .iter()
            .all(|group| !group.tabs.contains(&tab_id))
        {
            return;
        }
        self.apply_tab_machinery(machinery);
        self.tab_menu_open = false;
        self.tab_menu_tab = None;
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn shutdown_terminals(&mut self, cx: &mut Context<Self>) {
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let Some(terminal) = content.terminal() {
                    terminal.update(cx, |terminal, _| terminal.shutdown());
                }
            });
        }
    }

    fn add_chat_tab(
        &mut self,
        window: &mut Window,
        adapter: Option<&dyn tiller_agents::AgentAdapter>,
        cx: &mut Context<Self>,
    ) {
        let (title, agent_icon, agent_id) = chat_tab_identity(adapter);
        let chat = match adapter {
            Some(adapter) => {
                let Some(program) = adapter.acp_program() else {
                    eprintln!(
                        "[chat] {} has no ACP server; refusing a silent fallback",
                        adapter.display_name()
                    );
                    return;
                };
                let command = acp_agent_command(program);
                let cwd = self.working_directory.clone();
                cx.new(|cx| Chat::launch_with_command(command, cwd, cx))
            }
            None => cx.new(Chat::launch),
        };
        let composer_focus = chat.focus_handle(cx);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon,
            agent_id,
            session_state: SessionTabState::with_root(self.next_pane_id),
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Chat(chat)),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        window.focus(&composer_focus, cx);
        window.on_next_frame(move |window, cx| window.focus(&composer_focus, cx));
        cx.notify();
    }

    fn open_chat_agent(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(adapter) = AGENT_CATALOG.iter().find(|adapter| adapter.id() == id) else {
            eprintln!("[chat] no adapter for action id '{id}'");
            return;
        };
        self.add_chat_tab(window, Some(*adapter), cx);
    }

    fn resume_chat(&mut self, retained_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .retained_chats
            .iter()
            .position(|chat| chat.id == retained_id)
        else {
            return;
        };
        let retained = self.retained_chats.remove(index);
        let title = retained.title.clone();
        let transcript = retained.transcript;
        let (command, agent_icon, agent_id) = restored_chat_spec(retained.agent_id.as_deref());
        let cwd = self.working_directory.clone();
        let pane_id = self.next_pane_id;
        let chat = cx.new(|cx| {
            let mut chat = Chat::launch_with_command(command, cwd, cx);
            chat.restore_transcript(&transcript, cx);
            chat
        });
        let composer_focus = chat.focus_handle(cx);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon,
            agent_id,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Chat(chat)),
            focused_pane: pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        window.focus(&composer_focus, cx);
        window.on_next_frame(move |window, cx| window.focus(&composer_focus, cx));
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
        let terminal = cx.new(
            |cx| match TerminalView::with_shell(&working_directory, shell, cx) {
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
            },
        );
        self.insert_terminal_tab(title, terminal, agent_icon, cx);
    }

    fn insert_terminal_tab(
        &mut self,
        title: impl Into<String>,
        terminal: Entity<TerminalView>,
        agent_icon: Option<Icon>,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.next_tab_id;
        let pane_id = self.next_pane_id;
        Self::bind_terminal(&terminal, tab_id, pane_id, cx);
        self.tabs.push(OpenTab {
            id: tab_id,
            group_id: self.tab_machinery.active_group(),
            title: title.into(),
            kind: TabKind::Terminal,
            agent_icon,
            agent_id: None,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Terminal { view: terminal }),
            focused_pane: pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_conflict_terminal_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let working_directory = self.working_directory.clone();
        let title = format!("Resolve {}", path.display());
        let terminal = cx.new(|cx| {
            TerminalView::for_conflict(&working_directory, &path, cx).unwrap_or_else(|error| {
                TerminalView::failed(
                    &working_directory,
                    TerminalShell::System,
                    format!("{error:#}"),
                    cx,
                )
            })
        });
        self.insert_terminal_tab(title, terminal, None, cx);
    }

    fn add_file_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let open_paths = self
            .tabs
            .iter()
            .flat_map(|tab| {
                let mut paths = Vec::new();
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::File { view } = content {
                        paths.push(view.read(cx).path().to_path_buf());
                    }
                });
                paths
            })
            .collect::<Vec<_>>();
        if file_path_is_already_open(&open_paths, &path)
            && let Some(index) = self.tabs.iter().position(|tab| {
                let mut matches_path = false;
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::File { view } = content {
                        matches_path |= view.read(cx).path() == path.as_path();
                    }
                });
                matches_path
            })
        {
            self.active_tab = index;
            let tab = &self.tabs[index];
            self.tab_machinery.select_tab(tab.group_id, tab.id);
            self.schedule_save(cx);
            self.sync_activity(cx);
            cx.notify();
            return;
        }
        let title = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let view = cx.new(|cx| FileView::new(path, cx));
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title,
            // The existing UI tab model has only chat/terminal kinds. File
            // identity stays in TabContent; the shell overlay adjusts its
            // glyph and width below without changing the menu component.
            kind: TabKind::Editor,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(self.next_pane_id),
            panes: PaneNode::leaf(self.next_pane_id, TabContent::File { view }),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_changes_tab(&mut self, cx: &mut Context<Self>) {
        let changes = cx.new(|cx| ChangesTab::new(self.working_directory.clone(), cx));
        Self::subscribe_changes_tab(&changes, cx);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title: "Changes".to_string(),
            kind: TabKind::Diff,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(self.next_pane_id),
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Changes(changes)),
            focused_pane: self.next_pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_browser_tab(
        &mut self,
        initial_url: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (String, Entity<BrowserSurface>) {
        let initial_url = initial_url.into();
        let pane_id = self.next_pane_id;
        let surface_id = format!("surface:{pane_id}");
        let browser = cx.new(|cx| BrowserSurface::new(&initial_url, window, cx));
        let origins = self.browser_origins.iter().cloned().collect::<Vec<_>>();
        browser.update(cx, |surface, _| surface.set_allowed_origins(origins));
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title: "Browser".to_string(),
            kind: TabKind::Browser,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Browser(browser.clone())),
            focused_pane: pane_id,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
        (surface_id, browser)
    }

    fn browser_surface(&self) -> Option<Entity<BrowserSurface>> {
        let mut browser = None;
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Browser(surface) = content {
                    browser = Some(surface.clone());
                }
            });
            if browser.is_some() {
                break;
            }
        }
        browser
    }

    fn handle_browser_action(
        &mut self,
        method: &str,
        params: &BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        if method == "browser.open" {
            let initial_url = params
                .get("url")
                .or_else(|| params.get("address"))
                .filter(|url| !url.trim().is_empty())
                .ok_or_else(|| "browser.open requires a non-empty url".to_string())?;
            let (surface_id, browser) = self.add_browser_tab(initial_url, window, cx);
            return Ok(browser.update(cx, |surface, _| {
                let state = surface.state();
                vec![
                    ("surface".to_string(), surface_id),
                    ("url".to_string(), state.address().to_string()),
                    ("title".to_string(), state.page_title().to_string()),
                ]
            }));
        }

        let browser = self
            .browser_surface()
            .ok_or_else(|| format!("{method} failed: no browser surface"))?;
        browser.update(cx, |surface, _| match method {
            "browser.navigate" => {
                let address = params
                    .get("url")
                    .or_else(|| params.get("address"))
                    .or_else(|| params.get("href"))
                    .ok_or_else(|| "browser.navigate requires a non-empty url".to_string())?;
                surface
                    .submit_address(address)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                let state = surface.state();
                Ok(vec![
                    ("url".to_string(), state.address().to_string()),
                    ("title".to_string(), state.page_title().to_string()),
                ])
            }
            "browser.act" => {
                let driving = params
                    .get("driving")
                    .or_else(|| params.get("agentDriving"))
                    .ok_or_else(|| {
                        "browser.act is unsupported on Linux: only the driving flag is implemented"
                            .to_string()
                    })?;
                let driving = matches!(driving.as_str(), "1" | "true" | "yes");
                surface.set_agent_driving(driving);
                Ok(vec![("driving".to_string(), driving.to_string())])
            }
            _ => Err(format!(
                "{method} is unsupported on Linux: browser automation is not implemented"
            )),
        })
    }

    fn seed_browser_origins(&self, cx: &mut Context<Self>) {
        let origins = self.browser_origins.iter().cloned().collect::<Vec<_>>();
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Browser(browser) = content {
                    let origins = origins.clone();
                    browser.update(cx, |surface, _| surface.set_allowed_origins(origins));
                }
            });
        }
    }

    fn drain_browser_events(&mut self, cx: &mut Context<Self>) {
        let settings_origins = self
            .settings
            .read(cx)
            .browser_origins()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        if settings_origins != self.browser_origins {
            self.browser_origins = settings_origins;
            self.seed_browser_origins(cx);
        }
        let mut newly_allowed = Vec::new();
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Browser(browser) = content {
                    browser.update(cx, |surface, _| {
                        newly_allowed.extend(
                            surface
                                .allowed_origins()
                                .map(str::to_owned)
                                .filter(|origin| !self.browser_origins.contains(origin)),
                        );
                        for event in surface.take_events() {
                            if let BrowserEvent::OpenExternal(url) = event
                                && let Err(error) = Command::new("xdg-open").arg(url).spawn()
                            {
                                eprintln!("[browser] could not open external link: {error}");
                            }
                        }
                    });
                }
            });
        }
        let browser_origins_changed = !newly_allowed.is_empty();
        for origin in newly_allowed {
            if self.browser_origins.insert(origin.clone()) {
                self.session.save_browser_origin_grant(&origin);
            }
        }
        if browser_origins_changed {
            let origins = self.browser_origins.iter().cloned().collect::<Vec<_>>();
            self.settings
                .update(cx, |settings, cx| settings.set_browser_origins(origins, cx));
        }
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
        let tillerctl_path = match resolve_tillerctl_for_process() {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(error) => {
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_notice(format!("[agent] {}: {error}", adapter.display_name()), cx)
                });
                cx.notify();
                return;
            }
        };
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        let pane_id = format!("pane-{}", self.next_pane_id);

        if let Err(error) = adapter.prepare(&worktree_path, &pane_id, &tillerctl_path) {
            eprintln!(
                "failed to prepare {} in {worktree_path}: {error}",
                adapter.display_name()
            );
        }

        let command = adapter.command(&worktree_path, &pane_id, &tillerctl_path);
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
        self.add_terminal_tab_with_shell(adapter.display_name(), shell, Some(agent_icon), cx);
        if let Some(tab) = self.tabs.last_mut() {
            tab.agent_id = Some(adapter.id().to_string());
        }
        self.sync_activity(cx);
    }

    fn open_action(&mut self, action: NewTabAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            NewTabAction::NewChat => self.add_chat_tab(window, None, cx),
            NewTabAction::NewTerminal => self.add_terminal_tab("Terminal", cx),
            NewTabAction::NewChanges => self.add_changes_tab(cx),
            NewTabAction::ClaudeCode
            | NewTabAction::Codex
            | NewTabAction::OpenCode
            | NewTabAction::Pi
            | NewTabAction::OhMyPi => {
                let id = agent_id_for_action(action).expect("agent action has an id");
                let Some(adapter) = AGENT_CATALOG.iter().find(|adapter| adapter.id() == id) else {
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
            NewTabAction::NewBrowser => {
                self.add_browser_tab("https://example.com", window, cx);
            }
        }
    }

    /// The full-window Settings route used by the status-bar affordance and
    /// by the control socket. Selecting a section is done on the Settings
    /// entity itself, so both doors render the same selected detail.
    fn open_settings(&mut self, section: Option<SettingsCategory>, cx: &mut Context<Self>) {
        if let Some(section) = section {
            self.settings
                .update(cx, |settings, cx| settings.select_category(section, cx));
        }
        self.show_settings = true;
        // F-SET-02: the Escape handler lives on this workspace's root, which
        // GPUI only reaches through the focused element's dispatch path. The
        // settings surface must hold focus while it is open; the request is
        // fulfilled on the surface's next frame.
        self.settings
            .update(cx, |settings, _| settings.request_surface_focus());
        cx.notify();
    }

    fn active_changes_view(&self) -> Option<(&OpenTab, Entity<ChangesTab>)> {
        let tab = self.tabs.get(self.active_tab)?;
        let mut view = None;
        tab.panes.for_each(&mut |_, content| {
            if let TabContent::Changes(changes) = content {
                view = Some(changes.clone());
            }
        });
        view.map(|view| (tab, view))
    }

    fn control_open_changes(
        &mut self,
        worktree: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        if let Some(selector) = worktree {
            let path = self
                .control_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .path_for_selector(selector)
                .ok_or_else(|| format!("unknown worktree: {selector}"))?;
            if path != self.working_directory {
                self.select_worktree(path, cx)?;
            }
        }
        // This is the same typed action used by the New-tab UI callback.
        self.add_changes_tab(cx);
        self.control_read_changes(cx)
    }

    fn control_read_changes(&self, cx: &Context<Self>) -> Result<Vec<(String, String)>, String> {
        let Some((tab, view)) = self.active_changes_view() else {
            return Err("Changes surface is not open".to_string());
        };
        changes_report_pairs(tab.id, &view.read(cx).report())
    }

    fn control_open_settings(
        &mut self,
        section: Option<SettingsCategory>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        self.open_settings(section, cx);
        self.control_read_settings(cx)
    }

    fn control_select_settings(
        &mut self,
        section: SettingsCategory,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        // Selecting over the socket also opens the full-window route, just as
        // a user choosing a category can only do from Settings.
        self.open_settings(Some(section), cx);
        self.control_read_settings(cx)
    }

    fn control_read_settings(&self, cx: &Context<Self>) -> Result<Vec<(String, String)>, String> {
        if !self.show_settings {
            return Err("Settings surface is not open".to_string());
        }
        settings_report_pairs(&self.settings.read(cx).report())
    }

    fn active_tab_mut(&mut self) -> Option<&mut OpenTab> {
        self.tabs.get_mut(self.active_tab)
    }

    fn select_pane(&mut self, pane_id: usize, window: Option<&mut Window>, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut()
            && tab.panes.contains(pane_id)
        {
            tab.focused_pane = pane_id;
            let mut focused_content = None;
            tab.panes.for_each(&mut |id, content| {
                if id == pane_id {
                    focused_content = match content {
                        TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                        TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                        TabContent::File { .. }
                        | TabContent::Changes(_)
                        | TabContent::Browser(_) => None,
                    };
                }
            });
            if let (Some(window), Some(focus_handle)) = (window, focused_content) {
                window.focus(&focus_handle, cx);
            }
            self.sync_control_panes(cx);
            cx.notify();
        }
    }

    fn focus_neighbor(
        &mut self,
        direction: SplitDirection,
        forward: bool,
        window: Option<&mut Window>,
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

    fn cycle_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        let ids = self
            .tab_machinery
            .group_tabs(self.tab_machinery.active_group())
            .unwrap_or(&[]);
        let Some(active) = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| ids.iter().position(|id| *id == tab.id))
        else {
            return;
        };
        let Some(selection) = TabSelection::new(ids.len(), active) else {
            return;
        };
        let next = selection.cycle(forward).active();
        let id = ids[next];
        self.select_tab(id, cx);
    }

    fn select_tab_position(&mut self, position: usize, cx: &mut Context<Self>) {
        let ids = self
            .tab_machinery
            .group_tabs(self.tab_machinery.active_group())
            .unwrap_or(&[]);
        let active = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| ids.iter().position(|id| *id == tab.id))
            .unwrap_or(0);
        let Some(selection) = TabSelection::new(ids.len(), active) else {
            return;
        };
        let index = selection.jump(position).active();
        let id = ids[index];
        self.select_tab(id, cx);
    }

    fn split_focused_terminal(
        &mut self,
        direction: SplitDirection,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some((tab_id, focused_pane)) = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.id, tab.focused_pane))
        else {
            return;
        };
        self.split_terminal_at(tab_id, focused_pane, direction, window, cx);
    }

    fn split_terminal_at(
        &mut self,
        tab_id: usize,
        focused_pane: usize,
        direction: SplitDirection,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.split_terminal_at_with_placement(
            tab_id,
            focused_pane,
            direction,
            SplitPlacement::After,
            window,
            cx,
        );
    }

    fn split_terminal_at_with_placement(
        &mut self,
        tab_id: usize,
        focused_pane: usize,
        direction: SplitDirection,
        placement: SplitPlacement,
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
            let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
                return;
            };
            let split = tab.panes.split_focused_with_placement(
                focused_pane,
                pane_id,
                direction,
                placement,
                TabContent::Terminal {
                    view: terminal.clone(),
                },
            );
            if split {
                tab.focused_pane = pane_id;
            }
            split
        };
        if split {
            Self::bind_terminal(&terminal, tab_id, pane_id, cx);
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                tab.session_state.pane_events.push(PaneEvent::Split {
                    focused: focused_pane,
                    new_id: pane_id,
                    direction: split_event_name(direction, placement),
                });
            }
            self.next_pane_id += 1;
            if let Some(window) = window {
                self.select_pane(pane_id, Some(window), cx);
            }
            self.sync_control_panes(cx);
            self.schedule_save(cx);
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
        let Some(adapter) = AGENT_CATALOG
            .iter()
            .find(|adapter| adapter.id() == adapter_id)
        else {
            return;
        };
        let pane_id = self.next_pane_id;
        let focused_pane = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.focused_pane)
            .unwrap_or(pane_id);
        let tillerctl_path = match resolve_tillerctl_for_process() {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(error) => {
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_notice(format!("[agent] {}: {error}", adapter.display_name()), cx)
                });
                cx.notify();
                return;
            }
        };
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        let pane_name = format!("pane-{pane_id}");
        if let Err(error) = adapter.prepare(&worktree_path, &pane_name, &tillerctl_path) {
            eprintln!(
                "failed to prepare {} in {worktree_path}: {error}",
                adapter.display_name()
            );
        }
        let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let shell = TerminalShell::WithArguments {
            program: shell_program,
            args: vec![
                "-lc".to_string(),
                adapter.command(&worktree_path, &pane_name, &tillerctl_path),
            ],
        };
        let terminal = cx.new(|cx| {
            TerminalView::with_shell(&worktree_path, shell, cx).expect("start split agent")
        });
        let tab_id = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.id)
            .unwrap_or_default();
        let split = {
            let Some(tab) = self.active_tab_mut() else {
                return;
            };
            let split = tab.panes.split_focused(
                tab.focused_pane,
                pane_id,
                direction,
                TabContent::Terminal {
                    view: terminal.clone(),
                },
            );
            if split {
                tab.agent_icon = Some(
                    Icon::for_agent_id(adapter.id())
                        .expect("every catalog agent must have a brand icon"),
                );
                tab.agent_id = Some(adapter.id().to_string());
                tab.focused_pane = pane_id;
            }
            split
        };
        if split {
            Self::bind_terminal(&terminal, tab_id, pane_id, cx);
            if let Some(tab) = self.active_tab_mut() {
                tab.session_state.pane_events.push(PaneEvent::Split {
                    focused: focused_pane,
                    new_id: pane_id,
                    direction: split_direction_name(direction).to_string(),
                });
            }
            self.activity
                .agent_spawned(&pane_name, adapter.id(), Instant::now());
            self.next_pane_id += 1;
            if let Some(window) = window {
                self.select_pane(pane_id, Some(window), cx);
            }
            self.sync_activity(cx);
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn close_focused_pane(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let Some((tab_id, focused_pane)) = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.id, tab.focused_pane))
        else {
            return;
        };
        self.close_terminal_at(tab_id, focused_pane, window, cx);
    }

    fn close_terminal_at(
        &mut self,
        tab_id: usize,
        focused_pane: usize,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return;
        };
        if tab.panes.leaf_ids().len() <= 1 || !tab.panes.contains(focused_pane) {
            return;
        }
        let removed = tab.panes.remove(focused_pane);
        if let Some(removed) = removed {
            tab.session_state
                .pane_events
                .push(PaneEvent::Close { id: focused_pane });
            if let Some(terminal) = removed.terminal() {
                terminal.update(cx, |terminal, _| terminal.input([3, 4]));
            }
            let replacement = tab.panes.first_id().unwrap_or(focused_pane);
            tab.focused_pane = replacement;
            if let Some(window) = window {
                let mut replacement_content = None;
                tab.panes.for_each(&mut |id, content| {
                    if id == replacement {
                        replacement_content = match content {
                            TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                            TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                            TabContent::File { .. }
                            | TabContent::Changes(_)
                            | TabContent::Browser(_) => None,
                        };
                    }
                });
                if let Some(focus_handle) = replacement_content {
                    window.focus(&focus_handle, cx);
                }
            }
            self.sync_control_panes(cx);
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn update_divider(
        &mut self,
        drag: &DraggedPaneDivider,
        event: &DragMoveEvent<DraggedPaneDivider>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(drag.tab_index) else {
            return;
        };
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
            let ratio_millis = (ratio * 1000.0).round().clamp(100.0, 900.0) as u16;
            tab.session_state.pane_events.push(PaneEvent::SetRatio {
                path: drag.path.clone(),
                ratio_millis,
            });
            self.schedule_save(cx);
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
                let tab_id = self.tabs[tab_index].id;
                let entity_for_click = entity.clone();
                let entity_for_close = entity.clone();
                let surface = match content {
                    TabContent::Chat(chat) => {
                        div().size_full().child(chat.clone()).into_any_element()
                    }
                    TabContent::Terminal { view } => {
                        div().size_full().child(view.clone()).into_any_element()
                    }
                    TabContent::File { view } => {
                        div().size_full().child(view.clone()).into_any_element()
                    }
                    TabContent::Changes(view) => {
                        div().size_full().child(view.clone()).into_any_element()
                    }
                    TabContent::Browser(view) => {
                        div().size_full().child(view.clone()).into_any_element()
                    }
                };
                div()
                    .id(format!("pane-{pane_id}"))
                    .relative()
                    .size_full()
                    .min_w(px(MIN_SPLIT_PANE_SIZE))
                    .min_h(px(MIN_SPLIT_PANE_SIZE))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        entity_for_click.update(cx, |workspace, cx| {
                            workspace.select_pane(pane_id, Some(window), cx)
                        });
                    })
                    .capture_key_down(move |event, window, cx| {
                        if event.keystroke.key == "w" && event.keystroke.modifiers.platform {
                            cx.stop_propagation();
                            entity_for_close.update(cx, |workspace, cx| {
                                workspace.request_close_tab_by_id(tab_id, window, cx)
                            });
                        }
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
                let first_element =
                    self.render_pane_tree(first, tab_index, entity.clone(), first_path);
                let second_element =
                    self.render_pane_tree(second, tab_index, entity.clone(), second_path);
                let first_style = |element| {
                    div()
                        .flex_shrink_1()
                        .min_w(px(MIN_SPLIT_PANE_SIZE))
                        .min_h(px(MIN_SPLIT_PANE_SIZE))
                        .flex_basis(DefiniteLength::Fraction(*ratio))
                        .child(element)
                };
                let second_style = |element| {
                    div()
                        .flex_shrink_1()
                        .min_w(px(MIN_SPLIT_PANE_SIZE))
                        .min_h(px(MIN_SPLIT_PANE_SIZE))
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
                        this.w(px(SPLIT_DIVIDER_SIZE)).h_full().child(
                            div()
                                .id(format!("pane-divider-h-handle-{path:?}"))
                                .absolute()
                                .left(px(-1.5))
                                .w(px(9.))
                                .h_full()
                                .cursor_col_resize()
                                .on_drag(divider_drag.clone(), |_, _, _, cx| {
                                    cx.new(|_| gpui::Empty)
                                }),
                        )
                    })
                    .when(*direction == SplitDirection::Vertical, |this| {
                        this.h(px(SPLIT_DIVIDER_SIZE)).w_full().child(
                            div()
                                .id(format!("pane-divider-v-handle-{path:?}"))
                                .absolute()
                                .top(px(-1.5))
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
                    .when(*direction == SplitDirection::Horizontal, |this| {
                        this.flex().flex_row()
                    })
                    .when(*direction == SplitDirection::Vertical, |this| {
                        this.flex().flex_col()
                    })
                    .on_drag_move::<DraggedPaneDivider>(move |event, _, cx| {
                        let drag = event.drag(cx).clone();
                        drag_entity_move.update(cx, |workspace, cx| {
                            workspace.update_divider(&drag, event, cx)
                        });
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

    /// Renders one active tab surface per pane group. A group may be empty
    /// after its last tab was moved away; keeping that surface visible is
    /// deliberate because it gives Move Existing Tab a real destination and
    /// makes the F-TAB-13 empty state observable instead of silently deleting
    /// the pane.
    fn render_group_surfaces(&self, theme: Theme, entity: Entity<Self>) -> AnyElement {
        let groups = self.tab_machinery.groups();
        let mut surfaces = div().flex().flex_row().size_full().bg(theme.background);
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                surfaces = surfaces.child(div().w(px(1.0)).h_full().bg(gpui::black()));
            }
            let surface = group
                .active_tab
                .and_then(|tab_id| self.tabs.iter().position(|tab| tab.id == tab_id))
                .map(|tab_index| {
                    div()
                        .id(format!("pane-group-surface-{}", group.id))
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(self.render_pane_tree(
                            &self.tabs[tab_index].panes,
                            tab_index,
                            entity.clone(),
                            Vec::new(),
                        ))
                        .into_any_element()
                })
                .unwrap_or_else(|| {
                    div()
                        .id(format!("pane-group-empty-{}", group.id))
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.meta)
                        .child("No tabs in this pane")
                        .into_any_element()
                });
            surfaces = surfaces.child(surface);
        }
        surfaces.into_any_element()
    }

    fn render_open_tab(
        tab: &OpenTab,
        active: bool,
        status: Option<ActivityStatus>,
        exit_label: Option<String>,
        dirty: bool,
        renaming: bool,
        rename_draft: Option<&str>,
        rename_focus: Option<FocusHandle>,
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
            if is_file {
                theme.file_link
            } else {
                theme.tab_focus_accent
            }
        } else {
            theme.meta
        };
        let width = if is_file {
            180.0
        } else {
            Self::tab_width(tab.kind)
        };
        let close_entity = entity.clone();
        let menu_entity = entity.clone();
        let rename_entity = entity.clone();
        let drag_entity = entity.clone();
        let tab_drag = RowDrag {
            scope: ReorderScope::Tabs,
            id,
            group: Some(tab.group_id),
        };
        div()
            .id(format!("workspace-tab-{id}"))
            .debug_selector(move || format!("workspace-tab-{id}"))
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
            .on_drag(tab_drag, |_, _, _, cx| cx.new(|_| gpui::Empty))
            .on_drag_move::<RowDrag>(move |event, _, cx| {
                let drag = *event.drag(cx);
                let before = event.event.position.x < event.bounds.center().x;
                drag_entity.update(cx, |workspace, cx| {
                    workspace.preview_tab_reorder(drag, id, before, cx);
                });
            })
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                cx.stop_propagation();
                menu_entity.update(cx, |this, cx| this.open_tab_menu(id, cx));
            })
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.select_tab(id, cx)))
            .child(
                div()
                    .w(px(14.0))
                    .text_color(glyph_color)
                    .child(IconElement::new(icon, px(14.0))),
            )
            .when(!renaming, |this| {
                this.child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .child(tab.title.clone()),
                )
            })
            .when(renaming, |this| {
                let focus = rename_focus.expect("a renaming tab has a focus handle");
                let focus_for_click = focus.clone();
                let draft = rename_draft.unwrap_or_default().to_owned();
                this.child(
                    div()
                        .id("tab-rename-field")
                        .debug_selector(|| "tab-rename-field".to_owned())
                        .track_focus(&focus)
                        .flex_1()
                        .min_w_0()
                        .px(theme.spacing.titlebar_control_spacing)
                        .rounded(theme.radii.control)
                        .bg(theme.filter_field_bg)
                        .text_color(theme.title)
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            focus_for_click.focus(window, cx);
                        })
                        .on_key_down(move |event, window, cx| {
                            cx.stop_propagation();
                            rename_entity.update(cx, |workspace, cx| {
                                workspace.handle_tab_rename_key(event, window, cx)
                            });
                        })
                        .child(draft),
                )
            })
            .child(
                div()
                    .id(format!("workspace-tab-status-{id}"))
                    .debug_selector(move || format!("workspace-tab-status-{id}"))
                    .min_w(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .when_some(status, |this, status| {
                        let status_name = tab_status_name(status);
                        this.child(
                            div()
                                .id(format!("workspace-tab-status-glyph-{id}"))
                                .debug_selector(move || {
                                    format!("workspace-tab-status-{status_name}-{id}")
                                })
                                .text_color(tab_status_color(status, theme))
                                .child(tab_status_glyph(status)),
                        )
                    })
                    .when_some(exit_label, |this, label| {
                        this.child(
                            div()
                                .id(format!("workspace-tab-exit-{id}"))
                                .debug_selector(move || format!("workspace-tab-exit-{id}"))
                                .text_size(px(9.0))
                                .text_color(theme.meta)
                                .child(label),
                        )
                    }),
            )
            .when(active, |this| {
                this.child(
                    div()
                        .id(format!("workspace-tab-close-{id}"))
                        .debug_selector(move || format!("workspace-tab-close-{id}"))
                        .w(px(14.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.0))
                        .text_color(theme.subtitle)
                        .hover(|style| style.bg(theme.row_hover).rounded(px(4.0)))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            close_entity.update(cx, |this, cx| {
                                this.request_close_tab_by_id(id, window, cx)
                            });
                        })
                        .child(IconElement::new(Icon::Close, px(12.0)).text_color(theme.subtitle)),
                )
            })
            .when(dirty, |this| {
                this.child(
                    div()
                        .id(format!("workspace-tab-dirty-{id}"))
                        .debug_selector(move || format!("workspace-tab-dirty-{id}"))
                        .w(theme.spacing.titlebar_control_spacing)
                        .h(theme.spacing.titlebar_control_spacing)
                        .rounded(theme.radii.control)
                        .bg(theme.tab_focus_accent),
                )
            })
            .when(active, |this| {
                this.bg(theme.selected_fill).child(
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

    fn tab_is_dirty(&self, tab: &OpenTab, cx: &App) -> bool {
        // One predicate feeds both the strip indicator and every close door:
        // live terminals, streaming chats, and unsaved editors are dirty.
        // Chat exposes streaming state but no public draft getter, so an
        // unsent draft remains clean until the chat surface publishes one.
        let mut dirty = false;
        tab.panes.for_each(&mut |_, content| {
            dirty |= match content {
                TabContent::File { view } => view.read(cx).is_dirty(),
                // A live terminal owns a process whose input/output would be
                // lost on close. Failed and already-exited panes are clean.
                TabContent::Terminal { view } => {
                    !view.read(cx).is_failed() && view.read(cx).exit_status().is_none()
                }
                TabContent::Chat(chat) => chat.read(cx).is_streaming(),
                TabContent::Changes(_) | TabContent::Browser(_) => false,
            };
        });
        dirty
    }

    fn request_close_tab_by_id(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let dirty = self.tab_is_dirty(&self.tabs[index], cx);
        if !dirty {
            self.close_tab(index, cx);
            return;
        }

        let title = self.tabs[index].title.clone();
        let answer = window.prompt(
            PromptLevel::Warning,
            "Close dirty tab?",
            Some(&format!("Discard unsaved work in {title}?")),
            &["Close", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.unwrap_or(1) != 0 {
                return;
            }
            let _ = this.update(cx, |workspace, cx| {
                workspace.close_tab_by_id(id, cx);
            });
        })
        .detach();
    }

    fn open_tab_menu(&mut self, id: usize, cx: &mut Context<Self>) {
        self.tab_menu_tab = Some(id);
        self.tab_menu_open = true;
        self.overflow_menu_open = false;
        cx.notify();
    }

    fn tab_context_items(&self) -> Vec<TabContextItem> {
        let Some(tab_id) = self.tab_menu_tab else {
            return Vec::new();
        };
        let machinery = self.tab_command_machinery();
        let Some(group) = machinery
            .groups()
            .iter()
            .find(|group| group.tabs.contains(&tab_id))
        else {
            return Vec::new();
        };
        let Some(position) = group.tabs.iter().position(|id| *id == tab_id) else {
            return Vec::new();
        };

        let mut items = vec![
            TabContextItem::enabled("Open File", "open-file", TabContextAction::OpenFile),
            TabContextItem::separator(),
            TabContextItem::enabled("Rename", "rename", TabContextAction::Rename),
            TabContextItem::separator(),
            TabContextItem::enabled("Close", "close", TabContextAction::Close),
            if group.tabs.len() > 1 {
                TabContextItem::enabled(
                    "Close Others",
                    "close-others",
                    TabContextAction::CloseOthers,
                )
            } else {
                TabContextItem::disabled(
                    "Close Others",
                    "close-others",
                    TabContextAction::CloseOthers,
                    "no other tab is available",
                )
            },
            if position + 1 < group.tabs.len() {
                TabContextItem::enabled(
                    "Close Tabs to the Right",
                    "close-right",
                    TabContextAction::CloseTabsToRight,
                )
            } else {
                TabContextItem::disabled(
                    "Close Tabs to the Right",
                    "close-right",
                    TabContextAction::CloseTabsToRight,
                    "already the last tab",
                )
            },
            TabContextItem::separator(),
            if position > 0 {
                TabContextItem::enabled(
                    "Move Earlier",
                    "move-earlier",
                    TabContextAction::MoveEarlier,
                )
            } else {
                TabContextItem::disabled(
                    "Move Earlier",
                    "move-earlier",
                    TabContextAction::MoveEarlier,
                    "already the first tab",
                )
            },
            if position + 1 < group.tabs.len() {
                TabContextItem::enabled("Move Later", "move-later", TabContextAction::MoveLater)
            } else {
                TabContextItem::disabled(
                    "Move Later",
                    "move-later",
                    TabContextAction::MoveLater,
                    "already the last tab",
                )
            },
        ];

        let other_groups = machinery
            .groups()
            .iter()
            .filter(|candidate| candidate.id != group.id)
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        items.push(TabContextItem::separator());
        items.push(TabContextItem::disabled(
            "Move to This Pane",
            "move-to-current-pane",
            TabContextAction::MoveToCurrentPane,
            "no other tab is available",
        ));
        if other_groups.is_empty() {
            items.push(TabContextItem::disabled(
                "Move to Other Pane",
                "move-to-other-pane",
                TabContextAction::MoveToPane(usize::MAX),
                "no other pane is available",
            ));
        } else {
            for group_id in other_groups {
                items.push(TabContextItem::enabled(
                    format!("Move to Pane {group_id}"),
                    format!("move-to-pane-{group_id}"),
                    TabContextAction::MoveToPane(group_id),
                ));
            }
        }
        items.push(TabContextItem::separator());
        if self.retained_chats.is_empty() {
            items.push(TabContextItem::disabled(
                "Resume Chat",
                "resume-chat",
                TabContextAction::ResumeChat,
                "no retained chat is available",
            ));
        } else {
            items.push(TabContextItem::enabled(
                "Resume Chat",
                "resume-chat",
                TabContextAction::ResumeChat,
            ));
        }
        items
    }

    fn tab_context_menu_left(&self) -> f32 {
        let Some(tab_id) = self.tab_menu_tab else {
            return 0.0;
        };
        let active_group = self.tab_machinery.active_group();
        let mut left = 5.0;
        for tab in self.tabs.iter().filter(|tab| tab.group_id == active_group) {
            if tab.id == tab_id {
                break;
            }
            left += Self::tab_width(tab.kind) + 1.0;
        }
        left
    }

    fn render_tab_context_menu(&self, theme: Theme, entity: Entity<Self>) -> impl IntoElement {
        let action_entity = entity.clone();
        let on_action = Rc::new(move |action, window: &mut Window, cx: &mut App| {
            action_entity.update(cx, |workspace, cx| {
                workspace.handle_tab_context_action(action, window, cx)
            });
        });
        let dismiss_entity = entity;
        let menu_tab_id = self.tab_menu_tab.unwrap_or_default();
        div()
            .id(format!("workspace-tab-menu-{menu_tab_id}"))
            .debug_selector(move || format!("workspace-tab-menu-{menu_tab_id}"))
            .absolute()
            .top(px(TAB_BAR_HEIGHT))
            .left(px(self.tab_context_menu_left()))
            .on_mouse_down_out(move |_, _, cx| {
                dismiss_entity.update(cx, |workspace, cx| workspace.dismiss_tab_menu(cx));
            })
            .child(render_tab_context_menu(
                self.tab_context_items(),
                on_action,
                theme,
            ))
    }

    fn dismiss_tab_menu(&mut self, cx: &mut Context<Self>) {
        self.tab_menu_open = false;
        self.tab_menu_tab = None;
        cx.notify();
    }

    fn handle_tab_context_action(
        &mut self,
        action: TabContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            TabContextAction::Dismiss => self.dismiss_tab_menu(cx),
            TabContextAction::OpenFile => {
                self.handle_open_file(&OpenFile, window, cx);
                self.dismiss_tab_menu(cx);
            }
            TabContextAction::ResumeChat => {
                self.handle_resume_chat(&ResumeChat, window, cx);
                self.dismiss_tab_menu(cx);
            }
            TabContextAction::Rename => {
                if let Some(tab_id) = self.tab_menu_tab {
                    self.begin_tab_rename(tab_id, window, cx);
                }
            }
            TabContextAction::Close => {
                self.handle_close_tab(&CloseTab, window, cx);
                self.dismiss_tab_menu(cx);
            }
            TabContextAction::CloseOthers => {
                self.request_close_other_tabs(window, cx);
                self.dismiss_tab_menu(cx);
            }
            TabContextAction::CloseTabsToRight => {
                self.request_close_tabs_to_right(window, cx);
                self.dismiss_tab_menu(cx);
            }
            TabContextAction::MoveEarlier => {
                self.move_selected_tab_direction(MoveDirection::Earlier, cx)
            }
            TabContextAction::MoveLater => {
                self.move_selected_tab_direction(MoveDirection::Later, cx)
            }
            TabContextAction::MoveToCurrentPane => {
                self.move_selected_tab(MoveTarget::CurrentPane, cx)
            }
            TabContextAction::MoveToPane(group_id) if group_id != usize::MAX => {
                self.move_selected_tab(MoveTarget::Group(group_id), cx)
            }
            TabContextAction::MoveToPane(_) => {}
        }
    }

    fn move_selected_tab_direction(&mut self, direction: MoveDirection, cx: &mut Context<Self>) {
        let mut machinery = self.tab_command_machinery();
        if !machinery.move_active_tab(direction) {
            return;
        }
        self.apply_tab_machinery(machinery);
        self.dismiss_tab_menu(cx);
        self.schedule_save(cx);
        self.sync_activity(cx);
    }

    fn begin_tab_rename(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        self.tab_rename = Some(TabRename {
            tab_id: id,
            draft: tab.title.clone(),
            focus,
        });
        self.dismiss_tab_menu(cx);
    }

    fn commit_tab_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.tab_rename.take() else {
            return;
        };
        let title = rename.draft.trim();
        if !title.is_empty()
            && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == rename.tab_id)
        {
            tab.title = title.to_owned();
            self.schedule_save(cx);
            self.sync_activity(cx);
        }
        cx.notify();
    }

    fn handle_tab_rename_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "enter" | "return" => self.commit_tab_rename(cx),
            "escape" => {
                self.tab_rename = None;
                cx.notify();
            }
            "backspace" | "delete" => {
                if let Some(rename) = self.tab_rename.as_mut() {
                    rename.draft.pop();
                }
                cx.notify();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                    && let Some(rename) = self.tab_rename.as_mut()
                {
                    rename.draft.push_str(character);
                    cx.notify();
                }
            }
        }
    }

    /// This shell overlay owns the visible tabs. The UI crate's TabBar remains
    /// underneath only for its typed + menu implementation; covering the full
    /// tab area prevents its fixture rows from leaking through after a close.
    fn tab_strip_available_width(&self, window: &Window, theme: Theme) -> f32 {
        let mut width = f32::from(window.bounds().size.width);
        if self.sidebar_visible {
            width -= SIDEBAR_WIDTH + SEAM_WIDTH;
        }
        if self.right_panel_visible {
            width -= RIGHT_PANEL_WIDTH + SEAM_WIDTH;
        }
        width - f32::from(theme.spacing.titlebar_control_frame.width)
    }

    fn render_overflow_menu(
        &self,
        tabs: &[&OpenTab],
        active_tab_id: Option<usize>,
        entity: Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let dismiss_entity = entity.clone();
        let mut menu = div()
            .id("tab-overflow-menu")
            .debug_selector(|| "tab-overflow-menu".to_owned())
            .absolute()
            .right_0()
            .top(theme.spacing.titlebar_control_frame.height)
            .w(theme.spacing.menu_width)
            .p(theme.spacing.titlebar_control_spacing)
            .flex()
            .flex_col()
            .gap(theme.spacing.titlebar_control_spacing)
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .shadow_lg()
            .on_mouse_down_out(move |_, _, cx| {
                dismiss_entity.update(cx, |workspace, cx| {
                    workspace.overflow_menu_open = false;
                    cx.notify();
                });
            });

        for tab in tabs {
            let id = tab.id;
            let active = active_tab_id == Some(id);
            let select_entity = entity.clone();
            let selector = format!("tab-overflow-item-{id}");
            let selector_for_debug = selector.clone();
            let selected_selector = format!("tab-overflow-selected-{id}");
            let row = div()
                .id(selector)
                .debug_selector(move || selector_for_debug.clone())
                .w_full()
                .min_h(theme.typography.ui_line_height)
                .px(theme.spacing.card_gap)
                .py(theme.spacing.titlebar_control_spacing)
                .flex()
                .items_center()
                .justify_between()
                .gap(theme.spacing.titlebar_control_spacing)
                .rounded(theme.radii.control)
                .text_size(theme.typography.footnote)
                .text_color(if active { theme.title } else { theme.subtitle })
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| {
                    select_entity.update(cx, |workspace, cx| {
                        workspace.select_tab(id, cx);
                        workspace.overflow_menu_open = false;
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(tab.title.clone()),
                )
                .when(active, |this| {
                    this.child(
                        div()
                            .id(selected_selector.clone())
                            .debug_selector(move || selected_selector.clone())
                            .text_color(theme.tab_focus_accent)
                            .child("✓"),
                    )
                });
            menu = menu.child(row);
        }
        menu
    }

    fn render_open_tabs(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        window: &Window,
        cx: &App,
    ) -> impl IntoElement {
        let active_group = self.tab_command_machinery().active_group();
        let group_tabs = self
            .tabs
            .iter()
            .filter(|tab| tab.group_id == active_group)
            .collect::<Vec<_>>();
        let tab_widths = group_tabs
            .iter()
            .map(|tab| Self::tab_render_width(tab))
            .collect::<Vec<_>>();
        let overflow_width = f32::from(theme.spacing.titlebar_control_frame.width);
        let available_width = self.tab_strip_available_width(window, theme);
        let visible_count = visible_tab_count(&tab_widths, available_width, overflow_width);
        let has_overflow = visible_count < group_tabs.len();
        let active_tab_id = self.tabs.get(self.active_tab).map(|tab| tab.id);
        let mut tabs = div()
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .right(theme.spacing.titlebar_control_frame.width)
            .pl(px(5.0))
            .flex()
            .items_start()
            .gap(px(1.0))
            .bg(theme.background);
        if has_overflow {
            tabs = tabs.pr(theme.spacing.titlebar_control_frame.width);
        }
        for (index, tab) in group_tabs.iter().take(visible_count).enumerate() {
            let renaming = self
                .tab_rename
                .as_ref()
                .is_some_and(|rename| rename.tab_id == tab.id);
            let rename_draft = self
                .tab_rename
                .as_ref()
                .filter(|rename| rename.tab_id == tab.id)
                .map(|rename| rename.draft.as_str());
            let rename_focus = self
                .tab_rename
                .as_ref()
                .filter(|rename| rename.tab_id == tab.id)
                .map(|rename| rename.focus.clone());
            tabs = tabs.child(Self::render_open_tab(
                tab,
                index == self.active_tab,
                self.tab_status(tab, cx),
                Self::terminal_exit_label(tab, cx),
                self.tab_is_dirty(tab, cx),
                renaming,
                rename_draft,
                rename_focus,
                entity.clone(),
                theme,
            ));
        }
        if has_overflow {
            let overflow_entity = entity.clone();
            let menu_tabs = group_tabs.clone();
            let overflow_button = div()
                .id("tab-overflow-button")
                .debug_selector(|| "tab-overflow-button".to_owned())
                .absolute()
                .right_0()
                .top(theme.spacing.titlebar_control_spacing)
                .w(theme.spacing.titlebar_control_frame.width)
                .h(theme.spacing.titlebar_control_frame.height)
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| {
                    overflow_entity.update(cx, |workspace, cx| {
                        workspace.overflow_menu_open = !workspace.overflow_menu_open;
                        workspace.tab_menu_open = false;
                        cx.notify();
                    });
                })
                .child(
                    IconElement::new(Icon::ChevronDown, theme.spacing.title_strip_icon_size)
                        .text_color(theme.meta),
                );
            tabs = tabs.child(overflow_button);
            if self.overflow_menu_open {
                tabs = tabs.child(self.render_overflow_menu(
                    &menu_tabs,
                    active_tab_id,
                    entity.clone(),
                    theme,
                ));
            }
        }
        tabs
    }

    /// The three columns. Content entities are mounted selectively, while
    /// their owning entities remain in `tabs` above.
    fn columns(
        &self,
        theme: &Theme,
        entity: Entity<Self>,
        cx: &App,
        window: &Window,
    ) -> impl IntoElement {
        let mut columns = div().flex().flex_row().size_full();

        if self.sidebar_visible {
            columns = columns
                .child(
                    div()
                        .w(px(SIDEBAR_WIDTH))
                        .h_full()
                        .bg(theme.canvas)
                        .child(self.sidebar.clone()),
                )
                .child(self.seam());
        }

        columns = columns.child(
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
                        .child(self.render_open_tabs(*theme, entity.clone(), window, cx))
                        .when(self.tab_menu_open, |this| {
                            this.child(self.render_tab_context_menu(*theme, entity.clone()))
                        }),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .w_full()
                        .overflow_hidden()
                        .child(self.render_group_surfaces(*theme, entity))
                        .when_some(self.tabs.get(self.active_tab), |this, tab| {
                            this.when(tab_has_terminal(tab), |this| {
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
        );

        if self.right_panel_visible {
            columns = columns.child(self.seam()).child(
                div()
                    .w(px(RIGHT_PANEL_WIDTH))
                    .h_full()
                    .bg(theme.background)
                    .child(self.right_panel.clone()),
            );
        }

        columns
    }
}

impl TillerWorkspace {
    fn handle_new_terminal_tab(
        &mut self,
        _: &NewTerminalTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_action(NewTabAction::NewTerminal, window, cx);
    }

    fn handle_open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open File".into()),
        });
        cx.spawn_in(window, async move |workspace, cx| {
            let outcome = receiver.await;
            let _ = workspace.update(cx, |workspace, cx| match outcome {
                Ok(Ok(Some(mut paths))) => {
                    if let Some(path) = paths.pop() {
                        workspace.add_file_tab(path, cx);
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    workspace.sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_notice(
                            format!("[files] could not open the file picker: {error}"),
                            cx,
                        )
                    });
                }
                Err(_) => {}
            });
        })
        .detach();
    }

    fn handle_save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        let active_tab_kind = self.tabs.get(self.active_tab).map(|tab| tab.kind);
        if !matches!(
            window_command_availability(WindowCommand::SaveFile, active_tab_kind),
            WindowCommandAvailability::Enabled
        ) {
            return;
        }
        let views = self
            .tabs
            .get(self.active_tab)
            .map(|tab| {
                let mut views = Vec::new();
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::File { view } = content {
                        views.push(view.clone());
                    }
                });
                views
            })
            .unwrap_or_default();
        for view in views {
            if let Err(error) = view.update(cx, |view, cx| view.save(cx)) {
                view.update(cx, |view, cx| {
                    view.set_notice(format!("[files] save failed: {error}"), cx)
                });
            }
        }
    }

    fn handle_focus_pane_left(
        &mut self,
        _: &FocusPaneLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_neighbor(SplitDirection::Horizontal, false, Some(window), cx);
    }

    fn handle_focus_pane_right(
        &mut self,
        _: &FocusPaneRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_neighbor(SplitDirection::Horizontal, true, Some(window), cx);
    }

    fn handle_focus_pane_above(
        &mut self,
        _: &FocusPaneAbove,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_neighbor(SplitDirection::Vertical, false, Some(window), cx);
    }

    fn handle_focus_pane_below(
        &mut self,
        _: &FocusPaneBelow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_neighbor(SplitDirection::Vertical, true, Some(window), cx);
    }

    fn handle_split_pane_right(
        &mut self,
        _: &SplitPaneRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_focused_terminal(SplitDirection::Horizontal, Some(window), cx);
    }

    fn handle_split_pane_down(
        &mut self,
        _: &SplitPaneDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_focused_terminal(SplitDirection::Vertical, Some(window), cx);
    }

    fn handle_close_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
        self.close_focused_pane(Some(window), cx);
    }

    fn handle_cycle_tab_forward(
        &mut self,
        _: &CycleTabForward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_tab(true, cx);
    }

    fn handle_cycle_tab_backward(
        &mut self,
        _: &CycleTabBackward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_tab(false, cx);
    }

    fn handle_jump_to_tab(&mut self, position: usize, _: &mut Window, cx: &mut Context<Self>) {
        self.select_tab_position(position, cx);
    }

    fn handle_jump_to_tab_1(
        &mut self,
        _: &JumpToTab1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(1, window, cx);
    }

    fn handle_jump_to_tab_2(
        &mut self,
        _: &JumpToTab2,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(2, window, cx);
    }

    fn handle_jump_to_tab_3(
        &mut self,
        _: &JumpToTab3,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(3, window, cx);
    }

    fn handle_jump_to_tab_4(
        &mut self,
        _: &JumpToTab4,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(4, window, cx);
    }

    fn handle_jump_to_tab_5(
        &mut self,
        _: &JumpToTab5,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(5, window, cx);
    }

    fn handle_jump_to_tab_6(
        &mut self,
        _: &JumpToTab6,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(6, window, cx);
    }

    fn handle_jump_to_tab_7(
        &mut self,
        _: &JumpToTab7,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(7, window, cx);
    }

    fn handle_jump_to_tab_8(
        &mut self,
        _: &JumpToTab8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(8, window, cx);
    }

    fn handle_jump_to_tab_9(
        &mut self,
        _: &JumpToTab9,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_jump_to_tab(9, window, cx);
    }

    fn handle_open_all_tabs(&mut self, _: &OpenAllTabs, _: &mut Window, cx: &mut Context<Self>) {
        self.overflow_menu_open = !self.overflow_menu_open;
        self.tab_menu_open = false;
        cx.notify();
    }

    fn handle_open_tab_menu(&mut self, _: &OpenTabMenu, _: &mut Window, cx: &mut Context<Self>) {
        self.tab_menu_tab = self.tabs.get(self.active_tab).map(|tab| tab.id);
        self.tab_menu_open = self.tab_menu_tab.is_some();
        self.overflow_menu_open = false;
        cx.notify();
    }

    fn handle_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab_id) = self
            .tab_menu_tab
            .or_else(|| self.tabs.get(self.active_tab).map(|tab| tab.id))
        else {
            return;
        };
        self.request_close_tab_by_id(tab_id, window, cx);
    }

    fn handle_close_other_tabs(
        &mut self,
        _: &CloseOtherTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close_other_tabs(window, cx);
    }

    fn handle_close_tabs_to_right(
        &mut self,
        _: &CloseTabsToRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close_tabs_to_right(window, cx);
    }

    fn handle_move_tab_earlier(
        &mut self,
        _: &MoveTabEarlier,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_active_tab(MoveDirection::Earlier, cx);
    }

    fn handle_move_tab_later(&mut self, _: &MoveTabLater, _: &mut Window, cx: &mut Context<Self>) {
        self.move_active_tab(MoveDirection::Later, cx);
    }

    fn handle_move_tab_to_current_pane(
        &mut self,
        _: &MoveTabToCurrentPane,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selected_tab(MoveTarget::CurrentPane, cx);
    }

    fn handle_move_tab_to_other_pane(
        &mut self,
        _: &MoveTabToOtherPane,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_group = self.tab_machinery.active_group();
        let Some(target) = self
            .tab_machinery
            .groups()
            .iter()
            .find(|group| group.id != active_group)
            .map(|group| group.id)
        else {
            return;
        };
        self.move_selected_tab(MoveTarget::Group(target), cx);
    }

    fn handle_resume_chat(&mut self, _: &ResumeChat, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.retained_chats.first().map(|chat| chat.id) {
            self.resume_chat(id, window, cx);
        }
    }

    fn palette_context(&self) -> PaletteContext {
        let sidebar_target = self.project_catalog.projects().iter().find_map(|project| {
            project
                .worktrees
                .iter()
                .find(|worktree| worktree.path == self.working_directory)
                .map(|worktree| SidebarPaletteTarget {
                    project_id: project.id.clone(),
                    project_path: project.root_path.clone(),
                    project_is_git: project.is_git,
                    worktree_path: worktree.path.clone(),
                    worktree_is_primary: worktree.is_primary,
                })
                .or_else(|| {
                    (project.root_path == self.working_directory).then(|| SidebarPaletteTarget {
                        project_id: project.id.clone(),
                        project_path: project.root_path.clone(),
                        project_is_git: project.is_git,
                        worktree_path: self.working_directory.clone(),
                        worktree_is_primary: true,
                    })
                })
        });
        PaletteContext {
            active_tab_kind: self.tabs.get(self.active_tab).map(|tab| tab.kind),
            has_retained_chat: !self.retained_chats.is_empty(),
            has_other_pane: self.tab_machinery.groups().len() > 1,
            sidebar_target,
        }
    }

    fn focused_terminal(&self, window: &Window, cx: &App) -> bool {
        let Some(focused) = window.focused(cx) else {
            return false;
        };
        let mut terminal_focused = false;
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Terminal { view } = content {
                    terminal_focused |= view.focus_handle(cx) == focused;
                }
            });
            if terminal_focused {
                break;
            }
        }
        terminal_focused
    }

    fn handle_root_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette_open {
            self.handle_palette_key(event, window, cx);
            cx.stop_propagation();
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        // GPUI's platform modifier is the cross-platform spelling of ⌘ on
        // macOS and the Super key on Linux. The universal palette chord is
        // intercepted at the app boundary; this capture-phase handler keeps
        // the fallback for surfaces whose focus is still inside the shell.
        let close_tab_chord =
            modifiers.platform || (cfg!(target_os = "linux") && modifiers.control);
        if !self.show_settings && key == "w" && close_tab_chord {
            self.handle_close_tab(&CloseTab, window, cx);
            return;
        }
        let universal = key == "p" && modifiers.control && modifiers.shift;
        let readline_safe = key == "k" && modifiers.control && !modifiers.shift;
        if universal {
            self.open_command_palette(window, cx);
            cx.stop_propagation();
        } else if readline_safe && !self.focused_terminal(window, cx) {
            self.open_command_palette(window, cx);
            cx.stop_propagation();
        }
    }

    /// F-SET-02: Escape closes the settings surface, the same way Back
    /// does. The global binding dispatches here regardless of what holds
    /// focus; Back routes through the CloseSettings action. Focus returns
    /// to the sidebar immediately (it is rendered again on the next frame),
    /// so the shell's ctrl-k handling keeps working without a click.
    fn handle_close_settings_surface(
        &mut self,
        _: &CloseSettingsSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.show_settings {
            return;
        }
        self.show_settings = false;
        if self.sidebar_visible {
            let focus = self.sidebar.focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn handle_open_settings_shortcut(
        &mut self,
        _: &OpenSettingsShortcut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(None, cx);
    }

    fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette_open {
            return;
        }
        self.palette_previous_focus = window.focused(cx);
        self.palette_query.clear();
        self.palette_selected = 0;
        self.palette_open = true;
        let focus = self.palette_focus.clone();
        window.focus(&focus, cx);
        window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.palette_open {
            return;
        }
        self.palette_open = false;
        let previous_focus = self.palette_previous_focus.take();
        if let Some(previous_focus) = previous_focus {
            window.focus(&previous_focus, cx);
            window.on_next_frame(move |window, cx| window.focus(&previous_focus, cx));
        }
        cx.notify();
    }

    fn handle_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_command_palette(window, cx);
            return;
        }
        let context = self.palette_context();
        let entries = palette_entries(&context);
        let filtered = filter_palette_entries(&entries, &self.palette_query);
        match key {
            "backspace" | "delete" => {
                self.palette_query.pop();
                self.palette_selected = 0;
                cx.notify();
            }
            "up" => {
                self.palette_selected = self.palette_selected.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                if !filtered.is_empty() {
                    self.palette_selected = (self.palette_selected + 1).min(filtered.len() - 1);
                    cx.notify();
                }
            }
            "enter" | "return" => {
                if let Some(entry) = filtered.get(self.palette_selected).copied()
                    && entry.is_enabled()
                {
                    self.dispatch_palette_command(entry.command, window, cx);
                }
            }
            _ if !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && event
                    .keystroke
                    .key_char
                    .as_deref()
                    .is_some_and(|text| !text.is_empty()) =>
            {
                self.palette_query
                    .push_str(event.keystroke.key_char.as_deref().unwrap_or_default());
                self.palette_selected = 0;
                cx.notify();
            }
            _ => {}
        }
    }

    fn dispatch_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::Window(command) => match command {
                WindowCommand::NewTerminalTab => {
                    window.dispatch_action(Box::new(NewTerminalTab), cx)
                }
                WindowCommand::OpenFile => window.dispatch_action(Box::new(OpenFile), cx),
                WindowCommand::SaveFile => window.dispatch_action(Box::new(SaveFile), cx),
                WindowCommand::ToggleSidebar => window.dispatch_action(Box::new(ToggleSidebar), cx),
                WindowCommand::ToggleRightPanel => {
                    window.dispatch_action(Box::new(ToggleRightPanel), cx)
                }
            },
            PaletteCommand::Tab(command) => match command {
                TabCommand::FocusPane(direction, forward) => match (direction, forward) {
                    (SplitDirection::Horizontal, false) => {
                        window.dispatch_action(Box::new(FocusPaneLeft), cx)
                    }
                    (SplitDirection::Horizontal, true) => {
                        window.dispatch_action(Box::new(FocusPaneRight), cx)
                    }
                    (SplitDirection::Vertical, false) => {
                        window.dispatch_action(Box::new(FocusPaneAbove), cx)
                    }
                    (SplitDirection::Vertical, true) => {
                        window.dispatch_action(Box::new(FocusPaneBelow), cx)
                    }
                },
                TabCommand::SplitPane(SplitDirection::Horizontal) => {
                    window.dispatch_action(Box::new(SplitPaneRight), cx)
                }
                TabCommand::SplitPane(SplitDirection::Vertical) => {
                    window.dispatch_action(Box::new(SplitPaneDown), cx)
                }
                TabCommand::ClosePane => window.dispatch_action(Box::new(ClosePane), cx),
                TabCommand::CycleTab(true) => window.dispatch_action(Box::new(CycleTabForward), cx),
                TabCommand::CycleTab(false) => {
                    window.dispatch_action(Box::new(CycleTabBackward), cx)
                }
                TabCommand::JumpToTab(position) => match position {
                    1 => window.dispatch_action(Box::new(JumpToTab1), cx),
                    2 => window.dispatch_action(Box::new(JumpToTab2), cx),
                    3 => window.dispatch_action(Box::new(JumpToTab3), cx),
                    4 => window.dispatch_action(Box::new(JumpToTab4), cx),
                    5 => window.dispatch_action(Box::new(JumpToTab5), cx),
                    6 => window.dispatch_action(Box::new(JumpToTab6), cx),
                    7 => window.dispatch_action(Box::new(JumpToTab7), cx),
                    8 => window.dispatch_action(Box::new(JumpToTab8), cx),
                    9 => window.dispatch_action(Box::new(JumpToTab9), cx),
                    _ => {}
                },
                TabCommand::OpenAllTabs => window.dispatch_action(Box::new(OpenAllTabs), cx),
                TabCommand::OpenTabMenu => window.dispatch_action(Box::new(OpenTabMenu), cx),
                TabCommand::CloseTab => window.dispatch_action(Box::new(CloseTab), cx),
                TabCommand::CloseOtherTabs => window.dispatch_action(Box::new(CloseOtherTabs), cx),
                TabCommand::CloseTabsToRight => {
                    window.dispatch_action(Box::new(CloseTabsToRight), cx)
                }
                TabCommand::MoveTabEarlier => window.dispatch_action(Box::new(MoveTabEarlier), cx),
                TabCommand::MoveTabLater => window.dispatch_action(Box::new(MoveTabLater), cx),
                TabCommand::MoveTabToCurrentPane => {
                    window.dispatch_action(Box::new(MoveTabToCurrentPane), cx)
                }
                TabCommand::MoveTabToOtherPane => {
                    window.dispatch_action(Box::new(MoveTabToOtherPane), cx)
                }
                TabCommand::ResumeChat => window.dispatch_action(Box::new(ResumeChat), cx),
            },
            PaletteCommand::NewTab(action) => {
                if let Ok(mut actions) = self.pending_actions.lock() {
                    actions.push(WorkspaceAction::NewTab(action));
                }
            }
            PaletteCommand::Sidebar(action) => self.dispatch_sidebar_palette_action(action, cx),
        }
        self.close_command_palette(window, cx);
    }

    fn dispatch_sidebar_palette_action(
        &mut self,
        action: SidebarPaletteAction,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.palette_context().sidebar_target else {
            return;
        };
        match action {
            SidebarPaletteAction::ProjectSettings => self.sidebar.update(cx, |_, cx| {
                cx.emit(SidebarEvent::OpenProjectSettings(target.project_id.clone()))
            }),
            SidebarPaletteAction::InitializeGit
            | SidebarPaletteAction::RevealInFileManager
            | SidebarPaletteAction::RemoveProject => {
                let action = match action {
                    SidebarPaletteAction::InitializeGit => SidebarContextAction::InitializeGit,
                    SidebarPaletteAction::RevealInFileManager => {
                        SidebarContextAction::RevealInFileManager
                    }
                    SidebarPaletteAction::RemoveProject => SidebarContextAction::RemoveProject,
                    _ => unreachable!(),
                };
                self.sidebar.update(cx, |_, cx| {
                    cx.emit(SidebarEvent::ContextAction {
                        target: SidebarContextTarget::Project {
                            id: target.project_id.clone(),
                            path: target.project_path.clone(),
                            is_git: target.project_is_git,
                        },
                        action,
                    })
                });
            }
            SidebarPaletteAction::SetPrimary
            | SidebarPaletteAction::UnsetPrimary
            | SidebarPaletteAction::NewTab(_) => {
                let action = match action {
                    SidebarPaletteAction::SetPrimary => SidebarContextAction::SetPrimary,
                    SidebarPaletteAction::UnsetPrimary => SidebarContextAction::UnsetPrimary,
                    SidebarPaletteAction::NewTab(action) => SidebarContextAction::NewTab(action),
                    _ => unreachable!(),
                };
                self.sidebar.update(cx, |_, cx| {
                    cx.emit(SidebarEvent::ContextAction {
                        target: SidebarContextTarget::Worktree {
                            path: target.worktree_path.clone(),
                            is_primary: target.worktree_is_primary,
                        },
                        action,
                    })
                });
            }
        }
    }

    fn palette_selector(label: &str) -> String {
        format!(
            "command-palette-row-{}",
            label
                .to_ascii_lowercase()
                .chars()
                .map(|character| if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                })
                .collect::<String>()
        )
    }

    fn render_command_palette(&self, theme: Theme, entity: Entity<Self>) -> AnyElement {
        let context = self.palette_context();
        let entries = palette_entries(&context);
        let filtered = filter_palette_entries(&entries, &self.palette_query);
        let selected = self.palette_selected.min(filtered.len().saturating_sub(1));
        let query = self.palette_query.clone();
        let focus = self.palette_focus.clone();
        let focus_entity = entity.clone();
        let key_entity = entity.clone();
        let mut rows = div()
            .id("command-palette-rows")
            .flex()
            .flex_col()
            .gap(px(1.0));

        for (index, entry) in filtered.iter().copied().enumerate() {
            let active = index == selected;
            let entity = entity.clone();
            let selector = Self::palette_selector(entry.label);
            rows = rows.child(
                div()
                    .id(selector.clone())
                    .debug_selector(move || selector.clone())
                    .h(px(31.0))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(if entry.is_enabled() {
                        if active {
                            theme.title_selected
                        } else {
                            theme.title
                        }
                    } else {
                        theme.meta
                    })
                    .when(active && entry.is_enabled(), |this| {
                        this.bg(theme.selected_fill)
                    })
                    .when(entry.is_enabled(), move |this| {
                        this.hover(|style| style.bg(theme.row_hover)).on_click(
                            move |_, window, cx| {
                                entity.update(cx, |workspace, cx| {
                                    workspace.dispatch_palette_command(entry.command, window, cx)
                                });
                            },
                        )
                    })
                    .child(entry.label)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(theme.meta)
                            .child(entry.shortcut.unwrap_or(""))
                            .when_some(entry.disabled_reason, |this, reason| {
                                this.child(
                                    div()
                                        .debug_selector(move || {
                                            format!(
                                                "command-palette-disabled-{}",
                                                reason
                                                    .label()
                                                    .to_ascii_lowercase()
                                                    .replace(' ', "-")
                                            )
                                        })
                                        .child(format!("({})", reason.label())),
                                )
                            }),
                    ),
            );
        }

        let body = if filtered.is_empty() {
            div()
                .id("command-palette-empty")
                .debug_selector(|| "command-palette-empty".to_owned())
                .h(px(31.0))
                .w_full()
                .px(px(10.0))
                .flex()
                .items_center()
                .text_size(theme.typography.footnote)
                .text_color(theme.meta)
                .child(EMPTY_RESULT_LABEL)
                .into_any_element()
        } else {
            rows.into_any_element()
        };

        div()
            .id("command-palette")
            .debug_selector(|| "command-palette".to_owned())
            .key_context("CommandPalette")
            .track_focus(&focus)
            .capture_key_down(move |event, window, cx| {
                key_entity.update(cx, |workspace, cx| {
                    if workspace.palette_open {
                        workspace.handle_palette_key(event, window, cx);
                        cx.stop_propagation();
                    }
                });
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_entity.update(cx, |workspace, cx| {
                    workspace.palette_focus.focus(window, cx);
                });
            })
            .absolute()
            .top(px(56.0))
            .left(px(220.0))
            .w(px(620.0))
            .h(px(470.0))
            .p(px(8.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .shadow_lg()
            .child(
                div()
                    .id("command-palette-filter")
                    .debug_selector(|| "command-palette-filter".to_owned())
                    .h(px(34.0))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.filter_field_bg)
                    .border_1()
                    .border_color(theme.selection_ring)
                    .text_size(theme.typography.headline)
                    .text_color(if query.is_empty() {
                        theme.meta
                    } else {
                        theme.title
                    })
                    .child(if query.is_empty() {
                        "Type to filter commands".to_owned()
                    } else {
                        query
                    }),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .mb(px(5.0))
                    .px(px(10.0))
                    .text_size(theme.typography.caption2)
                    .text_color(theme.meta)
                    .child("Commands · substring filter"),
            )
            .child(div().flex_1().child(body))
            .into_any_element()
    }
}

impl TillerWorkspace {
    fn schedule_restored_scrollback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restored_scrollback_scheduled
            || !self
                .tabs
                .iter()
                .any(|tab| !tab.session_state.scrollback.is_empty())
        {
            return;
        }
        self.restored_scrollback_scheduled = true;
        cx.defer_in(window, |workspace, _window, cx| {
            replay_persisted_terminal_scrollback(&mut workspace.tabs, cx);
            cx.notify();
        });
    }
}

fn replay_persisted_terminal_scrollback(tabs: &mut [OpenTab], cx: &mut App) {
    for tab in tabs {
        let persisted = std::mem::take(&mut tab.session_state.scrollback);
        if persisted.is_empty() {
            continue;
        }
        tab.panes.for_each(&mut |pane_id, content| {
            if let TabContent::Terminal { view } = content
                && let Some(bytes) = persisted.get(&pane_id)
            {
                view.update(cx, |terminal, _| terminal.replay_scrollback(bytes));
            }
        });
    }
}

impl Render for TillerWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        self.drain_browser_events(cx);
        self.sync_activity(cx);

        if self.show_settings {
            return div()
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .bg(theme.canvas)
                .capture_key_down(cx.listener(Self::handle_root_key_down))
                .on_action(cx.listener(Self::handle_close_settings_surface))
                .child(
                    div()
                        .h(px(TITLE_BAR_HEIGHT))
                        .w_full()
                        .child(self.titlebar.clone()),
                )
                .child(div().flex_1().w_full().child(self.settings.clone()))
                .when(self.palette_open, |this| {
                    this.child(self.render_command_palette(theme, cx.entity()))
                });
        }

        self.schedule_restored_scrollback(window, cx);

        // Settings closed: hand focus back to a surface that is actually in
        // this frame, so the shell's key handling (ctrl-k, Escape…) keeps
        // working without an extra click.
        if self.restore_focus_pending {
            self.restore_focus_pending = false;
            if self.sidebar_visible {
                let focus = self.sidebar.focus_handle(cx);
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            }
        }

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .capture_key_down(cx.listener(Self::handle_root_key_down))
            .on_action(cx.listener(Self::handle_new_terminal_tab))
            .on_action(cx.listener(Self::handle_open_file))
            .on_action(cx.listener(Self::handle_save_file))
            .on_action(cx.listener(Self::handle_open_settings_shortcut))
            .on_action(cx.listener(|workspace, _: &ToggleSidebar, _, cx| {
                workspace.toggle_sidebar(cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleRightPanel, _, cx| {
                workspace.toggle_right_panel(cx);
            }))
            .on_action(cx.listener(Self::handle_focus_pane_left))
            .on_action(cx.listener(Self::handle_focus_pane_right))
            .on_action(cx.listener(Self::handle_focus_pane_above))
            .on_action(cx.listener(Self::handle_focus_pane_below))
            .on_action(cx.listener(Self::handle_split_pane_right))
            .on_action(cx.listener(Self::handle_split_pane_down))
            .on_action(cx.listener(Self::handle_close_pane))
            .on_action(cx.listener(Self::handle_cycle_tab_forward))
            .on_action(cx.listener(Self::handle_cycle_tab_backward))
            .on_action(cx.listener(Self::handle_jump_to_tab_1))
            .on_action(cx.listener(Self::handle_jump_to_tab_2))
            .on_action(cx.listener(Self::handle_jump_to_tab_3))
            .on_action(cx.listener(Self::handle_jump_to_tab_4))
            .on_action(cx.listener(Self::handle_jump_to_tab_5))
            .on_action(cx.listener(Self::handle_jump_to_tab_6))
            .on_action(cx.listener(Self::handle_jump_to_tab_7))
            .on_action(cx.listener(Self::handle_jump_to_tab_8))
            .on_action(cx.listener(Self::handle_jump_to_tab_9))
            .on_action(cx.listener(Self::handle_open_all_tabs))
            .on_action(cx.listener(Self::handle_open_tab_menu))
            .on_action(cx.listener(Self::handle_close_tab))
            .on_action(cx.listener(Self::handle_close_other_tabs))
            .on_action(cx.listener(Self::handle_close_tabs_to_right))
            .on_action(cx.listener(Self::handle_move_tab_earlier))
            .on_action(cx.listener(Self::handle_move_tab_later))
            .on_action(cx.listener(Self::handle_move_tab_to_current_pane))
            .on_action(cx.listener(Self::handle_move_tab_to_other_pane))
            .on_action(cx.listener(Self::handle_resume_chat))
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
                    .child(self.columns(&theme, cx.entity(), cx, window)),
            )
            .child(
                div()
                    .h(px(STATUS_BAR_HEIGHT))
                    .w_full()
                    .child(self.status_bar.clone()),
            )
            .when(self.palette_open, |this| {
                this.child(self.render_command_palette(theme, cx.entity()))
            })
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
                    is_primary: worktree.is_primary,
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

/// Replays a persisted pane event history through `PaneNode`'s public
/// operations. This keeps the tree implementation owned by `panes.rs` while
/// still allowing the session host to round-trip the user's layout.
fn replay_pane_events<T>(
    root_id: usize,
    root_content: T,
    events: &[PaneEvent],
    mut new_content: impl FnMut(usize) -> T,
) -> PaneNode<T> {
    let mut tree = PaneNode::leaf(root_id, root_content);
    for event in events {
        match event {
            PaneEvent::Split {
                focused,
                new_id,
                direction,
            } => {
                let Some((direction, placement)) = parse_split_event(direction) else {
                    continue;
                };
                if tree.contains(*focused) {
                    let _ = tree.split_focused_with_placement(
                        *focused,
                        *new_id,
                        direction,
                        placement,
                        new_content(*new_id),
                    );
                }
            }
            PaneEvent::SetRatio { path, ratio_millis } => {
                let _ = tree.set_ratio(path, f32::from(*ratio_millis) / 1000.0);
            }
            PaneEvent::Close { id } => {
                let _ = tree.remove(*id);
            }
        }
    }
    tree
}

/// Rebuilds the shell's tabs from a restored session: chat tabs get a fresh
/// `Chat` entity (identity, not transcript), terminal tabs get a fresh
/// terminal in the restored worktree directory, and persisted pane events
/// rebuild split trees. Returns the tabs and the active index.
fn restore_tabs(
    restored: &RestoredSession,
    working_directory: &std::path::Path,
    mut window: Option<&mut Window>,
    cx: &mut App,
) -> (Vec<OpenTab>, usize) {
    let mut tabs = Vec::new();
    let mut active = 0usize;
    for (tab_index, tab) in restored.tabs.iter().enumerate() {
        let id = tabs.len();
        let tab_state = restored
            .tab_states
            .get(tab_index)
            .cloned()
            .unwrap_or_default();
        let pane_id = tab_state.root_id.unwrap_or(id);
        let (command, agent_icon, agent_id) = if tab.kind == "chat" {
            let (command, icon, agent_id) = restored_chat_spec(tab.agent_id.as_deref());
            (Some(command), icon, agent_id)
        } else {
            (
                None,
                tab.agent_id.as_deref().and_then(Icon::for_agent_id),
                tab.agent_id.clone(),
            )
        };
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(|cx| {
                Chat::launch_with_command(
                    command.expect("chat restoration always has a fallback command"),
                    working_directory.to_path_buf(),
                    cx,
                )
            })),
            "terminal" => {
                let cwd = working_directory.to_path_buf();
                let view = cx.new(|cx| match TerminalView::new(&cwd, cx) {
                    Ok(view) => view,
                    Err(error) => {
                        TerminalView::failed(&cwd, TerminalShell::System, format!("{error:#}"), cx)
                    }
                });
                TabContent::Terminal { view }
            }
            "diff" => TabContent::Changes(
                cx.new(|cx| ChangesTab::new(working_directory.to_path_buf(), cx)),
            ),
            "browser" => {
                let Some(window) = window.as_deref_mut() else {
                    continue;
                };
                let browser = cx.new(|cx| BrowserSurface::new("https://example.com", window, cx));
                TabContent::Browser(browser)
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
        let panes = match content {
            TabContent::Chat(chat) => replay_pane_events(
                pane_id,
                TabContent::Chat(chat),
                &tab_state.pane_events,
                |_| TabContent::Terminal {
                    view: cx.new(|cx| {
                        TerminalView::new(working_directory, cx).unwrap_or_else(|error| {
                            TerminalView::failed(
                                working_directory,
                                TerminalShell::System,
                                format!("{error:#}"),
                                cx,
                            )
                        })
                    }),
                },
            ),
            content => replay_pane_events(pane_id, content, &tab_state.pane_events, |_| {
                let cwd = working_directory.to_path_buf();
                TabContent::Terminal {
                    view: cx.new(|cx| match TerminalView::new(&cwd, cx) {
                        Ok(view) => view,
                        Err(error) => TerminalView::failed(
                            &cwd,
                            TerminalShell::System,
                            format!("{error:#}"),
                            cx,
                        ),
                    }),
                }
            }),
        };
        tabs.push(OpenTab {
            id,
            group_id: 0,
            title: tab.title.clone(),
            kind: match tab.kind.as_str() {
                "chat" => TabKind::AgentChat,
                "diff" => TabKind::Diff,
                "browser" => TabKind::Browser,
                _ => TabKind::Terminal,
            },
            agent_icon,
            agent_id,
            session_state: tab_state,
            panes,
            focused_pane: pane_id,
        });
        if tab.active {
            active = id;
        }
    }
    (tabs, active)
}

fn restore_tabs_in_workspace(
    restored: &RestoredSession,
    working_directory: &std::path::Path,
    tab_id_start: usize,
    pane_id_start: usize,
    window: &mut Window,
    cx: &mut Context<TillerWorkspace>,
) -> (Vec<OpenTab>, usize) {
    let mut tabs = Vec::new();
    let mut active = 0usize;
    for (tab_index, tab) in restored.tabs.iter().enumerate() {
        let id = tab_id_start + tabs.len();
        let tab_state = restored
            .tab_states
            .get(tab_index)
            .cloned()
            .unwrap_or_default();
        let pane_id = tab_state
            .root_id
            .unwrap_or_else(|| pane_id_start + tabs.len());
        let (command, agent_icon, agent_id) = if tab.kind == "chat" {
            let (command, icon, agent_id) = restored_chat_spec(tab.agent_id.as_deref());
            (Some(command), icon, agent_id)
        } else {
            (
                None,
                tab.agent_id.as_deref().and_then(Icon::for_agent_id),
                tab.agent_id.clone(),
            )
        };
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(|cx| {
                Chat::launch_with_command(
                    command.expect("chat restoration always has a fallback command"),
                    working_directory.to_path_buf(),
                    cx,
                )
            })),
            "terminal" => {
                let cwd = working_directory.to_path_buf();
                let view = cx.new(|cx| match TerminalView::new(&cwd, cx) {
                    Ok(view) => view,
                    Err(error) => {
                        TerminalView::failed(&cwd, TerminalShell::System, format!("{error:#}"), cx)
                    }
                });
                TabContent::Terminal { view }
            }
            "diff" => TabContent::Changes(
                cx.new(|cx| ChangesTab::new(working_directory.to_path_buf(), cx)),
            ),
            "browser" => TabContent::Browser(
                cx.new(|cx| BrowserSurface::new("https://example.com", window, cx)),
            ),
            _ => continue,
        };
        let panes = replay_pane_events(pane_id, content, &tab_state.pane_events, |_| {
            let cwd = working_directory.to_path_buf();
            TabContent::Terminal {
                view: cx.new(|cx| match TerminalView::new(&cwd, cx) {
                    Ok(view) => view,
                    Err(error) => {
                        TerminalView::failed(&cwd, TerminalShell::System, format!("{error:#}"), cx)
                    }
                }),
            }
        });
        tabs.push(OpenTab {
            id,
            group_id: 0,
            title: tab.title.clone(),
            kind: if tab.kind == "chat" {
                TabKind::AgentChat
            } else if tab.kind == "diff" {
                TabKind::Diff
            } else if tab.kind == "browser" {
                TabKind::Browser
            } else {
                TabKind::Terminal
            },
            agent_icon,
            agent_id,
            session_state: tab_state,
            panes,
            focused_pane: pane_id,
        });
        if tab.active {
            active = id;
        }
    }
    (tabs, active)
}

/// Returns the current tabs followed by launch-snapshot tabs that are no
/// longer present. A restore is an additive recovery action: it must not
/// destroy a tab the user opened after launch, and the current active tab
/// remains active while recovered tabs are appended in snapshot order.
fn merge_launch_snapshot_tabs(snapshot: &[SessionTab], current: &[SessionTab]) -> Vec<SessionTab> {
    let mut merged = current.to_vec();
    for tab in snapshot {
        if current
            .iter()
            .any(|current| current.title == tab.title && current.kind == tab.kind)
        {
            continue;
        }
        let mut restored = tab.clone();
        restored.active = false;
        merged.push(restored);
    }
    merged
}

fn generated_worktree_branch() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("wt-{seconds}")
}

fn new_worktree_path(project: &str, branch: &str) -> PathBuf {
    let project = project
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let branch = branch
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    std::env::temp_dir()
        .join("tiller-worktrees")
        .join(format!("{project}-{branch}-{}", std::process::id()))
}

const TILLERCTL_INSTALL_SUBPATH: &str = "TillerRust/bin/tillerctl";

/// Resolve the control CLI used by worktree-local agent hooks.
///
/// The installed path is deliberately app-owned and XDG-correct rather than
/// relying on the login shell's PATH. A development build supplies the
/// sibling binary from `target/debug`; an installed build may supply an
/// already-installed copy or a PATH entry. In all cases hooks receive the
/// absolute app-owned path, so the shell that an agent starts cannot lose the
/// control CLI merely because it has a different PATH.
fn resolve_tillerctl_path(
    current_exe: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let destination = xdg_data_home_for(environment).join(TILLERCTL_INSTALL_SUBPATH);
    if is_executable_file(&destination) {
        return Ok(destination);
    }

    let mut candidates = Vec::new();
    if let Some(parent) = current_exe.parent() {
        candidates.push(parent.join("tillerctl"));
    }
    if let Some(path) = environment.get("PATH")
        && let Some(path) =
            tiller_agents::find_executable_in_path("tillerctl", std::ffi::OsStr::new(path))
    {
        candidates.push(path);
    }

    let source = candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .map(|candidate| {
            std::fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf())
        })
        .ok_or_else(|| {
            format!(
                "tillerctl is unavailable: checked {} and PATH; build or install the control CLI before launching an agent",
                current_exe
                    .parent()
                    .map(|path| path.join("tillerctl").display().to_string())
                    .unwrap_or_else(|| "the app executable directory".to_string())
            )
        })?;

    install_tillerctl(&source, &destination).map(|()| destination)
}

fn resolve_tillerctl_for_process() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().unwrap_or_default();
    let environment: BTreeMap<String, String> = std::env::vars().collect();
    resolve_tillerctl_path(&current_exe, &environment)
}

fn xdg_data_home_for(environment: &BTreeMap<String, String>) -> PathBuf {
    environment
        .get("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            environment
                .get("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join(".local/share"))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn install_tillerctl(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().ok_or_else(|| {
        format!(
            "tillerctl install path has no parent: {}",
            destination.display()
        )
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;

    if is_executable_file(destination) {
        return Ok(());
    }
    if std::fs::symlink_metadata(destination).is_ok() {
        return Err(format!(
            "cannot install tillerctl at {}: a non-executable file already exists",
            destination.display()
        ));
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination).map_err(|error| {
        format!(
            "could not install tillerctl at {}: {error}",
            destination.display()
        )
    })?;

    #[cfg(not(unix))]
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| {
            format!(
                "could not install tillerctl at {}: {error}",
                destination.display()
            )
        })?;

    if is_executable_file(destination) {
        Ok(())
    } else {
        Err(format!(
            "installed tillerctl at {} is not executable",
            destination.display()
        ))
    }
}

fn settings_snapshot_from_app_settings(settings: AppSettings) -> SettingsSnapshot {
    let summarizer_agent =
        match tiller_ui::settings::SummarizerChoice::parse(&settings.summarizer_agent) {
            Some(choice) => choice,
            None => {
                eprintln!(
                    "[settings] unknown summarizer agent '{}'; using Claude",
                    settings.summarizer_agent
                );
                tiller_ui::settings::SummarizerChoice::Claude
            }
        };

    SettingsSnapshot {
        theme: match settings.appearance {
            AppearanceMode::System => ThemeMode::System,
            AppearanceMode::Light => ThemeMode::Light,
            AppearanceMode::Dark => ThemeMode::Dark,
        },
        interface_font_size: settings.ui_font_size.clamp(10, 20) as i32,
        terminal_font_size: settings.terminal_font_size.clamp(9, 24) as i32,
        file_icons: match settings.file_icon_theme {
            FileIconTheme::SfSymbols => tiller_ui::settings::FileIconChoice::SfSymbols,
            FileIconTheme::Material => tiller_ui::settings::FileIconChoice::Material,
        },
        control_socket_enabled: settings.control_socket_enabled,
        // The live socket path is supplied by the host after this conversion;
        // it is runtime state, not an AppSettings field.
        socket_path: String::new(),
        resume_agent_sessions: settings.resume_agent_sessions,
        auto_naming: settings.auto_naming,
        limit_chat_history: settings.limit_chat_history,
        chat_retention: settings.chat_retention.clamp(5, 500) as i32,
        limit_mounted_worktrees: settings.limit_mounted_worktrees,
        mounted_worktrees: settings.mounted_worktrees.clamp(2, 50) as i32,
        summarizer_agent,
        claude_show_in_bar: settings.claude_show_in_bar,
        codex_show_in_bar: settings.codex_show_in_bar,
        opencode_show_in_bar: settings.opencode_show_in_bar,
        refresh_interval: settings.refresh_interval_min.clamp(1, 60) as i32,
        // F-SET-22 has no AppSettings field yet; do not pretend this UI-only
        // picker is persisted until its schema follow-up lands.
        agent_colors: SettingsSnapshot::default().agent_colors,
    }
}

fn app_settings_from_snapshot(snapshot: SettingsSnapshot) -> AppSettings {
    AppSettings {
        appearance: match snapshot.theme {
            ThemeMode::System => AppearanceMode::System,
            ThemeMode::Light => AppearanceMode::Light,
            ThemeMode::Dark => AppearanceMode::Dark,
        },
        ui_font_size: i64::from(snapshot.interface_font_size.clamp(10, 20)),
        terminal_font_size: i64::from(snapshot.terminal_font_size.clamp(9, 24)),
        file_icon_theme: match snapshot.file_icons {
            tiller_ui::settings::FileIconChoice::SfSymbols => FileIconTheme::SfSymbols,
            tiller_ui::settings::FileIconChoice::Material => FileIconTheme::Material,
        },
        control_socket_enabled: snapshot.control_socket_enabled,
        resume_agent_sessions: snapshot.resume_agent_sessions,
        auto_naming: snapshot.auto_naming,
        limit_chat_history: snapshot.limit_chat_history,
        chat_retention: i64::from(snapshot.chat_retention.clamp(5, 500)),
        limit_mounted_worktrees: snapshot.limit_mounted_worktrees,
        mounted_worktrees: i64::from(snapshot.mounted_worktrees.clamp(2, 50)),
        summarizer_agent: snapshot.summarizer_agent.id().to_owned(),
        claude_show_in_bar: snapshot.claude_show_in_bar,
        codex_show_in_bar: snapshot.codex_show_in_bar,
        opencode_show_in_bar: snapshot.opencode_show_in_bar,
        refresh_interval_min: i64::from(snapshot.refresh_interval.clamp(1, 60)),
    }
}

/// Applies the deployment override to the persisted settings exactly once,
/// before the boot controller decides whether to bind the control socket.
fn app_settings_with_environment_override(mut settings: AppSettings) -> AppSettings {
    let policy = tiller_project::SettingsPolicy {
        control_socket_enabled: settings.control_socket_enabled,
        ..Default::default()
    }
    .with_environment_override();
    settings.control_socket_enabled = policy.control_socket_enabled;
    settings
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
        let project_catalog = ProjectCatalog::from_projects(restored_catalog.projects);
        let session_store = SessionStore::open(&database_path);
        // Restore is tolerant of old path-based project ids; rewrite the
        // canonical catalog immediately so every later layout save sees one
        // project/worktree id convention.
        session_store.schedule_catalog(&project_catalog);
        let saved_settings = app_settings_with_environment_override(session_store.load_settings());
        Theme::set_mode(
            settings_snapshot_from_app_settings(saved_settings.clone()).theme,
            cx,
        );
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
        let control_actions = Arc::new(Mutex::new(Vec::<ControlAction>::new()));
        let control_state = Arc::new(Mutex::new(ControlState::from_catalog(
            &project_catalog,
            &working_directory,
        )));
        let panes = Arc::new(PaneRegistry::new());
        let notifications = Arc::new(Mutex::new(Vec::<ControlNotification>::new()));
        let session_refs = Arc::new(Mutex::new(session_store.load_session_refs()));
        let control_environment: BTreeMap<String, String> = std::env::vars().collect();
        let socket_path = PathBuf::from(tiller_control::default_socket_path(&control_environment));
        let socket_info = ControlSocketInfo::new(socket_path);
        let control_handler = Arc::new(AppControlHandler::new(
            control_state.clone(),
            control_actions.clone(),
            panes.clone(),
            notifications,
            session_refs,
            Some(session_store.clone()),
            socket_info.clone(),
        ));
        let control_socket = Arc::new(ControlSocketController::new(
            socket_info.clone(),
            control_handler,
        ));
        control_socket.set_enabled(saved_settings.control_socket_enabled);
        let session_store_for_window = session_store.clone();
        let session_store_for_settings = session_store.clone();
        let session_store_for_browser_revoke = session_store.clone();
        let control_socket_for_settings = control_socket.clone();
        let browser_origins_for_settings = session_store.load_browser_origin_grants();
        // Route the resolved socket path into the settings snapshot so the
        // General screen can display the path the live socket listens on
        // (P23: the socket row must show the real path, not a template).
        let settings_snapshot = {
            let mut snapshot = settings_snapshot_from_app_settings(saved_settings);
            snapshot.socket_path = socket_info.path.to_string_lossy().into_owned();
            snapshot
        };
        let panes_for_window = panes.clone();
        let workspace_for_quit = Arc::new(Mutex::new(None::<Entity<TillerWorkspace>>));
        let workspace_slot = workspace_for_quit.clone();

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
            move |window, cx| {
                let (tabs, active_tab) =
                    restore_tabs(&restored, &working_directory, Some(window), cx);
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
                let pending_for_chat_agent = pending_for_tab_bar.clone();
                let tab_bar = cx.new(|cx| {
                    TabBar::new(cx)
                        .on_new_tab(move |action| {
                            if let Ok(mut actions) = pending_for_tab_bar.lock() {
                                actions.push(WorkspaceAction::NewTab(action));
                            }
                        })
                        .on_chat_agent(move |id| {
                            if let Ok(mut actions) = pending_for_chat_agent.lock() {
                                actions.push(WorkspaceAction::NewChatAgent(id));
                            }
                        })
                });
                let status_bar = cx.new(|_| {
                    StatusBar::new(status_data.clone())
                        .with_preferences(tiller_ui::status_bar::UsageBarPrefs::from_snapshot(
                            &settings_snapshot,
                        ))
                        .on_settings(move || {
                            if let Ok(mut actions) = pending_for_status_bar.lock() {
                                actions.push(WorkspaceAction::OpenSettings);
                            }
                        })
                });
                let settings = cx.new(|cx| {
                    Settings::with_snapshot(cx, settings_snapshot)
                        .with_browser_origins(browser_origins_for_settings.clone())
                        .on_install_skill({
                            let pending_actions = pending_for_settings.clone();
                            move |command| {
                                if let Ok(mut actions) = pending_actions.lock() {
                                    actions.push(WorkspaceAction::InstallSkill(command));
                                }
                            }
                        })
                        .on_change(move |snapshot| {
                            control_socket_for_settings
                                .set_enabled(snapshot.control_socket_enabled);
                            session_store_for_settings
                                .save_settings(&app_settings_from_snapshot(snapshot));
                        })
                        .on_revoke_browser_origin({
                            let session_store = session_store_for_browser_revoke.clone();
                            move |origin| session_store.revoke_browser_origin(&origin)
                        })
                        .on_revoke_all_browser_origins({
                            let session_store = session_store_for_browser_revoke.clone();
                            move || session_store.revoke_all_browser_origins()
                        })
                        .on_back(move || {
                            if let Ok(mut actions) = pending_for_settings.lock() {
                                actions.push(WorkspaceAction::CloseSettings);
                            }
                        })
                });
                let workspace = cx.new(|cx| {
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
                        panes_for_window.clone(),
                        control_state.clone(),
                        tabs,
                        active_tab,
                        working_directory.clone(),
                        pending_actions.clone(),
                        pending_actions.clone(),
                        control_actions.clone(),
                        session_store_for_window,
                        project_catalog,
                        activity_label.clone(),
                        terminal_breadcrumb.clone(),
                        restored.clone(),
                        cx,
                    )
                });
                *workspace_slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(workspace.clone());
                workspace
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

        // App quit runs before GPUI clears its windows. Flush the latest
        // layout and explicitly terminate control-owned PTYs here; relying
        // only on an eventual Arc drop can leave the socket handler holding
        // a live child past the application shutdown boundary.
        let session_store_for_quit = session_store.clone();
        let panes_for_quit = panes.clone();
        cx.on_app_quit(move |cx| {
            if let Some(workspace) = workspace_for_quit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .cloned()
            {
                workspace.update(cx, |workspace, cx| {
                    workspace.schedule_save(cx);
                    workspace
                        .session
                        .schedule_catalog(&workspace.project_catalog);
                    workspace.shutdown_terminals(cx);
                });
            }
            session_store_for_quit.flush_now();
            panes_for_quit.shutdown();
            async {}
        })
        .detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        FocusHandle, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render,
        TestAppContext, VisualTestContext,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use tiller_persistence::{AppSettings, AppearanceMode, FileIconTheme};

    static TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(0);

    struct TerminalReplayFixture {
        terminal: Entity<TerminalView>,
    }

    impl Render for TerminalReplayFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.terminal.clone())
        }
    }

    struct WindowCommandFixture {
        fired: Rc<RefCell<Vec<WindowCommand>>>,
        focus_handle: FocusHandle,
    }

    impl WindowCommandFixture {
        fn new(fired: Rc<RefCell<Vec<WindowCommand>>>, cx: &mut Context<Self>) -> Self {
            bind_window_keys(cx);
            Self {
                fired,
                focus_handle: cx.focus_handle(),
            }
        }
    }

    impl Render for WindowCommandFixture {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let new_terminal = self.fired.clone();
            let open_file = self.fired.clone();
            let save_file = self.fired.clone();
            let toggle_sidebar = self.fired.clone();
            let toggle_right_panel = self.fired.clone();
            div()
                .key_context("WindowCommandFixture")
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(move |_, _: &NewTerminalTab, _, _| {
                    new_terminal
                        .borrow_mut()
                        .push(WindowCommand::NewTerminalTab);
                }))
                .on_action(cx.listener(move |_, _: &OpenFile, _, _| {
                    open_file.borrow_mut().push(WindowCommand::OpenFile);
                }))
                .on_action(cx.listener(move |_, _: &SaveFile, _, _| {
                    save_file.borrow_mut().push(WindowCommand::SaveFile);
                }))
                .on_action(cx.listener(move |_, _: &ToggleSidebar, _, _| {
                    toggle_sidebar
                        .borrow_mut()
                        .push(WindowCommand::ToggleSidebar);
                }))
                .on_action(cx.listener(move |_, _: &ToggleRightPanel, _, _| {
                    toggle_right_panel
                        .borrow_mut()
                        .push(WindowCommand::ToggleRightPanel);
                }))
                .child("window command fixture")
        }
    }

    #[test]
    fn skill_install_command_is_preserved_when_opened_in_a_terminal() {
        let command = tiller_project::agent_skill_install_command();
        let TerminalShell::WithArguments { program, args } = skill_install_shell(command) else {
            panic!("skill installation must run as a terminal command");
        };
        assert_eq!(program, "npx");
        assert_eq!(
            args,
            vec![
                "skills",
                "add",
                "e-palmisano/tiller",
                "--skill",
                "tiller",
                "-a",
                "claude-code,codex,opencode,pi",
                "-y",
            ]
        );
    }

    #[test]
    fn terminal_links_are_only_routed_to_their_owning_pane() {
        let event = TerminalLinkEvent {
            target: TerminalIdentity::new("pane-2", "terminal-2"),
            url: "https://example.test/pane-2".to_string(),
        };
        assert_eq!(
            terminal_link_url_for_pane(&event, "pane-2"),
            Some("https://example.test/pane-2")
        );
        assert_eq!(terminal_link_url_for_pane(&event, "pane-1"), None);
    }

    fn palette_test_workspace(cx: &mut Context<TillerWorkspace>) -> TillerWorkspace {
        palette_test_workspace_with_tab_count(cx, 1)
    }

    fn test_workspace_for_repo(
        cx: &mut Context<TillerWorkspace>,
        repo: PathBuf,
        with_changes_tab: bool,
    ) -> TillerWorkspace {
        let mut workspace = palette_test_workspace(cx);
        workspace.working_directory = repo.clone();
        workspace.right_panel = cx.new(|_| RightPanel::new(repo));
        let right_panel = workspace.right_panel.clone();
        TillerWorkspace::subscribe_right_panel(&right_panel, cx);
        if with_changes_tab {
            workspace.add_changes_tab(cx);
        }
        workspace
    }

    fn git_test(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git fixture command");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn test_repo(tag: &str) -> PathBuf {
        let repo = std::env::temp_dir().join(format!("tiller-p67-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("create P67 git fixture");
        git_test(&repo, &["init", "-q", "-b", "main"]);
        git_test(&repo, &["config", "user.email", "tests@example.invalid"]);
        git_test(&repo, &["config", "user.name", "Tiller tests"]);
        repo
    }

    fn changed_test_repo(tag: &str) -> PathBuf {
        let repo = test_repo(tag);
        std::fs::write(repo.join("changed.md"), "before\n").expect("seed changed fixture");
        git_test(&repo, &["add", "changed.md"]);
        git_test(
            &repo,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "seed"],
        );
        std::fs::write(repo.join("changed.md"), "after\n").expect("modify changed fixture");
        repo
    }

    fn conflicted_test_repo(tag: &str) -> PathBuf {
        let repo = test_repo(tag);
        std::fs::write(repo.join("conflicted.txt"), "base\n").expect("seed conflict fixture");
        git_test(&repo, &["add", "conflicted.txt"]);
        git_test(
            &repo,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "base"],
        );
        git_test(&repo, &["checkout", "-q", "-b", "theirs"]);
        std::fs::write(repo.join("conflicted.txt"), "theirs\n").expect("write theirs fixture");
        git_test(&repo, &["add", "conflicted.txt"]);
        git_test(
            &repo,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "theirs"],
        );
        git_test(&repo, &["checkout", "-q", "main"]);
        std::fs::write(repo.join("conflicted.txt"), "ours\n").expect("write ours fixture");
        git_test(&repo, &["add", "conflicted.txt"]);
        git_test(
            &repo,
            &["-c", "commit.gpgSign=false", "commit", "-q", "-m", "ours"],
        );
        let output = Command::new("git")
            .args(["merge", "theirs"])
            .current_dir(&repo)
            .output()
            .expect("run conflict merge");
        assert!(!output.status.success(), "the fixture merge must conflict");
        repo
    }

    fn wait_for_drawn(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<gpui::Pixels> {
        for _ in 0..200 {
            cx.run_until_parked();
            if let Some(bounds) = cx.debug_bounds(selector) {
                return bounds;
            }
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
        }
        panic!("{selector} was not drawn");
    }

    fn palette_test_workspace_with_tab_count(
        cx: &mut Context<TillerWorkspace>,
        tab_count: usize,
    ) -> TillerWorkspace {
        let unique = TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed);
        let scratch_root = std::env::temp_dir().join(format!(
            "tiller-command-palette-{}-{unique}",
            std::process::id()
        ));
        let working_directory = scratch_root.join("worktree");
        std::fs::create_dir_all(&working_directory).expect("create palette test worktree");
        let project_catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "palette-project".into(),
            name: "Palette Project".into(),
            root_path: working_directory.clone(),
            is_git: true,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: working_directory.clone(),
                is_primary: true,
            }],
        }]);
        let terminal = cx.new(|cx| {
            TerminalView::failed(
                &working_directory,
                TerminalShell::System,
                "headless palette test terminal",
                cx,
            )
        });
        let tabs = (0..tab_count)
            .map(|id| OpenTab {
                id,
                group_id: 0,
                title: if id == 0 {
                    "Terminal".into()
                } else {
                    format!("Terminal {id}")
                },
                kind: TabKind::Terminal,
                agent_icon: None,
                agent_id: None,
                session_state: SessionTabState::with_root(id),
                panes: PaneNode::leaf(
                    id,
                    TabContent::Terminal {
                        view: if id == 0 {
                            terminal.clone()
                        } else {
                            cx.new(|cx| {
                                TerminalView::failed(
                                    &working_directory,
                                    TerminalShell::System,
                                    "headless palette test terminal",
                                    cx,
                                )
                            })
                        },
                    },
                ),
                focused_pane: id,
            })
            .collect();
        let pending_actions = Arc::new(Mutex::new(Vec::new()));
        let control_actions = Arc::new(Mutex::new(Vec::new()));
        let panes = Arc::new(PaneRegistry::new());
        let state = Arc::new(Mutex::new(ControlState::from_catalog(
            &project_catalog,
            &working_directory,
        )));
        // Each GPUI test owns a database namespace. This includes SQLite's
        // `-wal` sidecar because both files live below the same private root;
        // no test can read or lock a sibling's session state.
        let session_path = scratch_root.join("tiller.sqlite");
        let session = SessionStore::open(&session_path);
        let titlebar = cx.new(Titlebar::new);
        let sidebar = cx.new(|cx| Sidebar::from_projects(sidebar_projects(&project_catalog), cx));
        let tab_bar = cx.new(|cx| TabBar::new(cx));
        let status_bar = cx.new(|_| {
            StatusBar::new(UsageBarData {
                branch: "main".into(),
                path: working_directory.to_string_lossy().into_owned(),
            })
        });
        let settings = cx.new(|cx| Settings::new(cx));
        let right_panel = cx.new(|_| RightPanel::new(working_directory.clone()));
        TillerWorkspace::new(
            titlebar,
            sidebar,
            tab_bar,
            status_bar,
            settings,
            right_panel,
            panes,
            state,
            tabs,
            0,
            working_directory.clone(),
            pending_actions.clone(),
            pending_actions,
            control_actions,
            session,
            project_catalog,
            "Palette Project/main".into(),
            "in test shell".into(),
            RestoredSession {
                working_directory,
                tabs: Vec::new(),
                tab_states: Vec::new(),
                diagnostics: Vec::new(),
            },
            cx,
        )
    }

    fn palette_test_terminal_focus(
        workspace: &Entity<TillerWorkspace>,
        cx: &VisualTestContext,
    ) -> FocusHandle {
        workspace.read_with(&cx.cx, |workspace, app| {
            let mut focus = None;
            workspace.tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Terminal { view } = content {
                    focus = Some(view.focus_handle(app));
                }
            });
            focus.expect("palette test has a terminal focus handle")
        })
    }

    fn activity_test_workspace(
        terminal: Entity<TerminalView>,
        working_directory: PathBuf,
        cx: &mut Context<TillerWorkspace>,
    ) -> TillerWorkspace {
        let project_catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "activity-project".into(),
            name: "Activity Project".into(),
            root_path: working_directory.clone(),
            is_git: false,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: working_directory.clone(),
                is_primary: true,
            }],
        }]);
        let tabs = vec![OpenTab {
            id: 0,
            group_id: 0,
            title: "Terminal".into(),
            kind: TabKind::Terminal,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(0),
            panes: PaneNode::leaf(0, TabContent::Terminal { view: terminal }),
            focused_pane: 0,
        }];
        let pending_actions = Arc::new(Mutex::new(Vec::new()));
        let control_actions = Arc::new(Mutex::new(Vec::new()));
        let panes = Arc::new(PaneRegistry::new());
        let state = Arc::new(Mutex::new(ControlState::from_catalog(
            &project_catalog,
            &working_directory,
        )));
        let session_path =
            std::env::temp_dir().join(format!("tiller-activity-wiring-{}.db", std::process::id()));
        let session = SessionStore::open(&session_path);
        TillerWorkspace::new(
            cx.new(Titlebar::new),
            cx.new(|cx| Sidebar::from_projects(sidebar_projects(&project_catalog), cx)),
            cx.new(TabBar::new),
            cx.new(|_| {
                StatusBar::new(UsageBarData {
                    branch: "main".into(),
                    path: working_directory.to_string_lossy().into_owned(),
                })
            }),
            cx.new(Settings::new),
            cx.new(|_| RightPanel::new(working_directory.clone())),
            panes,
            state,
            tabs,
            0,
            working_directory.clone(),
            pending_actions.clone(),
            pending_actions,
            control_actions,
            session,
            project_catalog,
            "Activity Project/main".into(),
            "in test shell".into(),
            RestoredSession {
                working_directory,
                tabs: Vec::new(),
                tab_states: Vec::new(),
                diagnostics: Vec::new(),
            },
            cx,
        )
    }

    fn activity_status(
        workspace: &Entity<TillerWorkspace>,
        cx: &VisualTestContext,
    ) -> Option<AgentStatus> {
        workspace.read_with(&cx.cx, |workspace, _| workspace.activity.status("pane-0"))
    }

    #[gpui::test]
    async fn workspace_wires_real_osc_title_into_activity_model(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("tiller-activity-wiring-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create activity test directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf '\\033]0;. working\\007'; exec sleep 1".into(),
            ],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx)
                .expect("spawn activity test terminal")
        });
        let workspace = cx.update(|_, app| {
            app.new(|cx| activity_test_workspace(terminal.clone(), working_directory.clone(), cx))
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if activity_status(&workspace, &cx) == Some(AgentStatus::Running) {
                terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
                cx.run_until_parked();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "the app-level terminal subscription never applied the real OSC title; status was {:?}",
            activity_status(&workspace, &cx)
        );
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    async fn workspace_wires_real_process_signal_without_title_clobber(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::temp_dir().join(format!(
            "tiller-activity-process-wiring-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&working_directory).expect("create process test directory");
        let agent = working_directory.join("codex");
        std::fs::copy("/bin/sleep", &agent).expect("create matching agent binary");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("printf '\\033]0;zsh\\007'; {} 30", agent.to_string_lossy()),
            ],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx)
                .expect("spawn process activity test terminal")
        });
        let workspace = cx.update(|_, app| {
            app.new(|cx| activity_test_workspace(terminal.clone(), working_directory.clone(), cx))
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if activity_status(&workspace, &cx) == Some(AgentStatus::Running) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            activity_status(&workspace, &cx),
            Some(AgentStatus::Running),
            "the app-level process refresh must identify the real codex child"
        );
        assert!(workspace.read_with(&cx.cx, |workspace, _| {
            workspace.activity.is_process_owned("pane-0")
        }));

        terminal.update(&mut cx.cx, |_, cx| {
            cx.emit(TerminalActivityEvent::OscTitle("zsh".into()));
        });
        cx.run_until_parked();
        assert_eq!(
            activity_status(&workspace, &cx),
            Some(AgentStatus::Running),
            "an unrelated title must not clear process-owned activity"
        );

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    fn palette_test_sidebar_focus(
        workspace: &Entity<TillerWorkspace>,
        cx: &VisualTestContext,
    ) -> FocusHandle {
        workspace.read_with(&cx.cx, |workspace, app| workspace.sidebar.focus_handle(app))
    }

    fn open_palette_for_test(cx: &mut VisualTestContext, workspace: &Entity<TillerWorkspace>) {
        let focus = palette_test_terminal_focus(workspace, cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.run_until_parked();
        assert!(cx.debug_bounds("command-palette").is_some());
    }

    #[gpui::test]
    async fn drawn_palette_filters_and_dispatches_sidebar_action_through_shell_route(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });

        open_palette_for_test(&mut cx, &workspace);
        cx.simulate_input("toggle side");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("command-palette-row-toggle-sidebar")
                .is_some()
        );
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert!(!workspace.read_with(&cx.cx, |workspace, _| workspace.sidebar_visible));
        assert!(cx.debug_bounds("command-palette").is_none());
    }

    #[gpui::test]
    async fn drawn_palette_filters_and_dispatches_tab_action_through_dirty_close_route(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });

        open_palette_for_test(&mut cx, &workspace);
        cx.simulate_input("close tab");
        cx.run_until_parked();
        assert!(cx.debug_bounds("command-palette-row-close-tab").is_some());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert!(workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.is_empty()));
        assert!(cx.debug_bounds("command-palette").is_none());
    }

    fn right_click_tab(cx: &mut VisualTestContext, tab_id: usize) {
        let selector = match tab_id {
            0 => "workspace-tab-0",
            1 => "workspace-tab-1",
            2 => "workspace-tab-2",
            _ => panic!("test tab selector is not defined"),
        };
        let bounds = cx.debug_bounds(selector).expect("the tab is drawn");
        cx.simulate_event(MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: bounds.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("tab-context-menu").is_some());
    }

    #[gpui::test]
    async fn drawn_tab_context_menu_invokes_close_other_and_close_right_routes(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 3));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        assert!(cx.debug_bounds("workspace-tab-0").is_some());
        assert!(cx.debug_bounds("workspace-tab-1").is_some());
        assert!(cx.debug_bounds("workspace-tab-2").is_some());

        right_click_tab(&mut cx, 1);
        let close_right = cx
            .debug_bounds("tab-command-close-right")
            .expect("close-right is reachable from the tab menu");
        cx.simulate_click(close_right.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>()
            }),
            vec![0, 1]
        );

        right_click_tab(&mut cx, 1);
        let close_others = cx
            .debug_bounds("tab-command-close-others")
            .expect("close-others is reachable from the tab menu");
        cx.simulate_click(close_others.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len() == 1));
    }

    #[gpui::test]
    async fn drawn_tab_context_menu_moves_and_renames_the_selected_tab(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 3));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        right_click_tab(&mut cx, 1);
        let move_earlier = cx
            .debug_bounds("tab-command-move-earlier")
            .expect("move earlier is reachable from the tab menu");
        cx.simulate_click(move_earlier.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>()
            }),
            vec![1, 0, 2]
        );

        right_click_tab(&mut cx, 1);
        let rename = cx
            .debug_bounds("tab-command-rename")
            .expect("rename is reachable from the tab menu");
        cx.simulate_click(rename.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("tab-rename-field").is_some());
        cx.simulate_input(" renamed");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs[0].title.clone()),
            "Terminal 1 renamed"
        );
    }

    #[gpui::test]
    async fn drawn_tab_drag_reorders_the_live_tab_strip(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 3));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("tab workspace root")
        });
        let source = cx
            .debug_bounds("workspace-tab-0")
            .expect("source tab is drawn");
        let target = cx
            .debug_bounds("workspace-tab-2")
            .expect("target tab is drawn");

        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(30.0), source.center().y),
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
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>()
            }),
            vec![1, 2, 0],
            "the drawn tab drag must update the live strip order"
        );
    }

    #[gpui::test]
    async fn drawn_sidebar_drag_persists_worktree_order_in_the_catalog(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("sidebar workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            let mut projects = workspace.project_catalog.projects().to_vec();
            projects[0].worktrees.extend([
                session::CatalogWorktree {
                    branch: "branch-1".into(),
                    path: PathBuf::from("/tmp/tiller-command-palette-branch-1"),
                    is_primary: false,
                },
                session::CatalogWorktree {
                    branch: "branch-2".into(),
                    path: PathBuf::from("/tmp/tiller-command-palette-branch-2"),
                    is_primary: false,
                },
            ]);
            workspace.project_catalog = ProjectCatalog::from_projects(projects);
            workspace.refresh_sidebar(cx);
        });
        cx.run_until_parked();

        let source = cx
            .debug_bounds("sidebar-row-1")
            .expect("first worktree row");
        let target = cx
            .debug_bounds("sidebar-row-3")
            .expect("third worktree row");
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(30.0), source.center().y),
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
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.project_catalog.projects()[0]
                    .worktrees
                    .iter()
                    .map(|worktree| worktree.branch.clone())
                    .collect::<Vec<_>>()
            }),
            vec!["branch-1", "branch-2", "main"]
        );
    }

    #[gpui::test]
    async fn drawn_tab_status_cell_renders_idle_and_running_states(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("tiller-tab-status-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create status test directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 30".into()],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx)
                .expect("spawn status test terminal")
        });
        let (workspace, cx) = cx.add_window_view(|_, cx| {
            activity_test_workspace(terminal.clone(), working_directory.clone(), cx)
        });
        let initial_status = workspace.read_with(&cx.cx, |workspace, app| {
            workspace.tab_status(&workspace.tabs[0], app)
        });
        assert_eq!(initial_status, Some(ActivityStatus::Idle));
        assert!(cx.debug_bounds("workspace-tab-0").is_some());
        assert!(cx.debug_bounds("workspace-tab-status-0").is_some());
        assert!(cx.debug_bounds("workspace-tab-status-idle-0").is_some());

        workspace.update(cx, |workspace, cx| {
            workspace
                .activity
                .agent_spawned("pane-0", "codex", Instant::now());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("workspace-tab-status-running-0").is_some());

        terminal.update(cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn drawn_tab_status_cell_labels_a_nonzero_terminal_exit(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("tiller-tab-exit-status-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create exit test directory");
        let shell = TerminalShell::WithArguments {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "exit 3".into()],
        };
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx)
                .expect("spawn exit status test terminal")
        });
        let (workspace, cx) = cx.add_window_view(|_, cx| {
            activity_test_workspace(terminal.clone(), working_directory.clone(), cx)
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            if cx.debug_bounds("workspace-tab-exit-0").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs[0].panes.contains(0)
                    && workspace.tabs[0].panes.leaf_ids().contains(&0)
            }),
            "the exit fixture keeps its terminal tab mounted"
        );
        let status_cell = cx.debug_bounds("workspace-tab-status-0").is_some();
        let error_marker = cx.debug_bounds("workspace-tab-status-error-0").is_some();
        let exit_marker = cx.debug_bounds("workspace-tab-exit-0").is_some();
        assert!(
            status_cell && error_marker && exit_marker,
            "a nonzero exit is rendered as an error status; status={:?}, exit={:?}, cell={status_cell}, error={error_marker}, exit_label={exit_marker}",
            workspace.read_with(&cx.cx, |workspace, app| {
                workspace.tab_status(&workspace.tabs[0], app)
            }),
            terminal.read_with(&cx.cx, |terminal, _| terminal.exit_status())
        );
        assert!(
            cx.debug_bounds("workspace-tab-exit-0").is_some(),
            "the tab exposes the concrete exit status"
        );
    }

    #[gpui::test]
    async fn drawn_tab_context_menu_moves_a_tab_to_another_pane_group(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 3));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs[2].group_id = 1;
            workspace.tab_machinery = TabMachinery::new(
                vec![
                    TabGroup::new(0, vec![0, 1], Some(0)),
                    TabGroup::new(1, vec![2], Some(2)),
                ],
                0,
            )
            .expect("test groups are valid");
            cx.notify();
        });
        cx.run_until_parked();

        right_click_tab(&mut cx, 1);
        let move_to_pane = cx
            .debug_bounds("tab-command-move-to-pane-1")
            .expect("the other pane destination is drawn");
        cx.simulate_click(move_to_pane.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(workspace.read_with(&cx.cx, |workspace, _| {
            workspace
                .tabs
                .iter()
                .find(|tab| tab.id == 1)
                .is_some_and(|tab| tab.group_id == 1)
        }));
    }

    #[gpui::test]
    async fn ctrl_w_uses_the_same_dirty_close_door_as_the_tab_menu(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let focus = palette_test_sidebar_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-w");
        cx.run_until_parked();
        assert!(workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.is_empty()));
    }

    #[gpui::test]
    async fn ctrl_comma_opens_the_settings_surface(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let focus = palette_test_sidebar_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-,");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-category-General").is_some(),
            "Linux ctrl-, must open the settings surface"
        );
    }

    #[gpui::test]
    async fn drawn_all_tabs_overflow_lists_every_hidden_tab_and_marks_the_active_one(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 10));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(900.0), px(600.0)));
        cx.run_until_parked();

        let overflow = cx
            .debug_bounds("tab-overflow-button")
            .expect("the strip exposes overflow when tabs do not fit");
        assert!(
            cx.debug_bounds("workspace-tab-9").is_none(),
            "tabs beyond the visible budget must be hidden from the strip"
        );
        cx.simulate_click(overflow.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.debug_bounds("tab-overflow-menu").is_some());
        assert!(cx.debug_bounds("tab-overflow-item-0").is_some());
        assert!(cx.debug_bounds("tab-overflow-item-9").is_some());
        assert!(cx.debug_bounds("tab-overflow-selected-0").is_some());
    }

    #[gpui::test]
    async fn drawn_tab_context_open_file_uses_the_picker_and_adds_an_editor_tab(
        cx: &mut TestAppContext,
    ) {
        let path =
            std::env::temp_dir().join(format!("tiller-open-file-menu-{}.md", std::process::id()));
        std::fs::write(&path, "# opened from tab menu\n").expect("write picker fixture");
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        right_click_tab(&mut cx, 0);
        let open_file = cx
            .debug_bounds("tab-command-open-file")
            .expect("the tab menu exposes Open File");
        cx.simulate_click(open_file.center(), Modifiers::none());
        cx.run_until_parked();
        cx.cx
            .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();

        let opened = workspace.read_with(&cx.cx, |workspace, app| {
            workspace.tabs.iter().any(|tab| {
                if tab.kind != TabKind::Editor {
                    return false;
                }
                let mut found = false;
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::File { view } = content {
                        found |= view.read(app).path() == path.as_path();
                    }
                });
                found
            })
        });
        assert!(opened, "the picker result must create a file-backed tab");
        let _ = std::fs::remove_file(path);
    }

    #[gpui::test]
    async fn drawn_tab_context_resume_chat_reopens_the_retained_session(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // ACP owns an OS worker, so this test must allow its event channel to
        // wake the deterministic executor while the fixture is being torn
        // down. The worker is still explicitly closed below.
        cx.cx.executor().allow_parking();
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let chat = cx.update(|_, app| {
            app.new(|cx| {
                Chat::launch_with_command(
                    AgentCommand::new("/bin/false"),
                    PathBuf::from("/tmp"),
                    cx,
                )
            })
        });
        let chat_for_tab = chat.clone();
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs.push(OpenTab {
                id: 1,
                group_id: 0,
                title: "Resumed chat".into(),
                kind: TabKind::AgentChat,
                agent_icon: Some(Icon::Codex),
                agent_id: Some("codex".into()),
                session_state: SessionTabState::with_root(1),
                panes: PaneNode::leaf(1, TabContent::Chat(chat_for_tab)),
                focused_pane: 1,
            });
            workspace.active_tab = 1;
            workspace.next_tab_id = 2;
            workspace.next_pane_id = 2;
            workspace.rebuild_tab_machinery();
            workspace.close_tab(1, cx);
            workspace.tab_menu_tab = Some(0);
            workspace.tab_menu_open = true;
            cx.notify();
        });
        // The local handle keeps the pre-close Chat entity alive after the
        // tab has retained its transcript. Drop it before launching the
        // resumed session so its ACP worker cannot overlap the next phase.
        drop(chat);
        cx.run_until_parked();

        let resume = cx
            .debug_bounds("tab-command-resume-chat")
            .expect("the tab menu exposes Resume Chat when a session is retained");
        cx.simulate_click(resume.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(workspace.read_with(&cx.cx, |workspace, _| {
            workspace.tabs.iter().any(|tab| {
                tab.title == "Resumed chat"
                    && tab.kind == TabKind::AgentChat
                    && tab.agent_id.as_deref() == Some("codex")
                    && tab.agent_icon == Some(Icon::Codex)
            })
        }));

        let resumed_chat = workspace.read_with(&cx.cx, |workspace, _| {
            workspace.tabs.iter().find_map(|tab| {
                (tab.title == "Resumed chat").then(|| {
                    let mut chat = None;
                    tab.panes.for_each(&mut |_, content| {
                        if let TabContent::Chat(candidate) = content {
                            chat = Some(candidate.clone());
                        }
                    });
                    chat.expect("resumed tab owns its Chat entity")
                })
            })
        });
        workspace.update(&mut cx, |workspace, cx| {
            if let Some(index) = workspace
                .tabs
                .iter()
                .position(|tab| tab.title == "Resumed chat")
            {
                workspace.close_tab(index, cx);
            }
        });
        drop(resumed_chat);
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn drawn_add_project_duplicate_shows_sidebar_notice(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let path = workspace.read_with(&cx.cx, |workspace, _| workspace.working_directory.clone());
        let project_count = workspace.read_with(&cx.cx, |workspace, _| {
            workspace.project_catalog.projects().len()
        });

        workspace.update(&mut cx, |workspace, cx| {
            workspace.add_project(path.clone(), cx);
        });
        cx.run_until_parked();
        let notice = workspace.read_with(&cx.cx, |workspace, cx| {
            workspace.sidebar.read(cx).notice().map(str::to_owned)
        });
        assert!(
            notice.is_some(),
            "duplicate project insertion must update the rendered sidebar state"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.project_catalog.projects().len()
            }),
            project_count,
            "a duplicate insertion must not add a second project"
        );
    }

    #[gpui::test]
    async fn drawn_save_failure_surfaces_file_notice(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let path = PathBuf::from("/proc/self/status");
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let file_view = cx.update(|_, cx| cx.new(|cx| FileView::new(path.clone(), cx)));
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs[0].title = "status".into();
            workspace.tabs[0].kind = TabKind::Editor;
            workspace.tabs[0].panes = PaneNode::leaf(
                0,
                TabContent::File {
                    view: file_view.clone(),
                },
            );
            workspace.sync_activity(cx);
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            file_view.read_with(&cx.cx, |view, _| view.editor().is_some()),
            "the procfs fixture must load as an editor before the save attempt"
        );
        cx.update(|window, app| {
            workspace.update(app, |workspace, cx| {
                workspace.handle_save_file(&SaveFile, window, cx);
            });
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("file-view-notice").is_some(),
            "a real read-only procfs save failure must be visible in the file view"
        );
    }

    #[gpui::test]
    async fn drawn_dirty_file_tab_uses_the_shared_dirty_predicate_for_its_indicator(
        cx: &mut TestAppContext,
    ) {
        let path =
            std::env::temp_dir().join(format!("tiller-dirty-tab-{}.txt", std::process::id()));
        std::fs::write(&path, "clean\n").expect("write dirty-tab fixture");
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let file_view = cx.update(|_, app| app.new(|cx| FileView::new(path.clone(), cx)));
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs[0].title = "Note".into();
            workspace.tabs[0].kind = TabKind::Editor;
            workspace.tabs[0].panes = PaneNode::leaf(
                0,
                TabContent::File {
                    view: file_view.clone(),
                },
            );
            workspace.sync_activity(cx);
            cx.notify();
        });
        for _ in 0..20 {
            cx.run_until_parked();
            if file_view.read_with(&cx.cx, |view, _| view.editor().is_some()) {
                break;
            }
        }
        file_view.update(&mut cx, |view, cx| {
            let editor = view.editor_mut().expect("file editor loaded");
            let end = editor.buffer().len();
            editor.insert(end, "dirty").expect("edit fixture buffer");
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("workspace-tab-dirty-0").is_some());
        let _ = std::fs::remove_file(path);
    }

    #[gpui::test]
    async fn ctrl_k_in_a_focused_terminal_does_not_open_the_palette(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });
        let focus = palette_test_terminal_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-k");
        cx.run_until_parked();

        assert!(cx.debug_bounds("command-palette").is_none());
        assert!(cx.update(|window, _| focus.is_focused(window)));
    }

    #[gpui::test]
    async fn ctrl_k_opens_the_palette_when_a_non_terminal_surface_is_focused(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });
        let focus = palette_test_sidebar_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-k");
        cx.run_until_parked();

        assert!(cx.debug_bounds("command-palette").is_some());
    }

    #[gpui::test]
    async fn escape_closes_the_palette_and_returns_focus_to_the_terminal(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });
        let focus = palette_test_terminal_focus(&workspace, &cx);

        open_palette_for_test(&mut cx, &workspace);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(cx.debug_bounds("command-palette").is_none());
        assert!(cx.update(|window, _| focus.is_focused(window)));
    }

    /// P58, F-SET-02: Escape closes the settings surface, the same way Back
    /// does. The shell's root key handler is attached to the settings
    /// branch, so the binding needs no new machinery — this test proves the
    /// full drawn round trip: open via the workspace action, close with the
    /// key.
    #[gpui::test]
    async fn escape_closes_the_settings_surface(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });

        workspace.update(&mut cx, |workspace, cx| workspace.open_settings(None, cx));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("settings-category-General").is_some(),
            "settings opens"
        );
        // The surface takes focus on the next frame after opening (focus
        // cannot move during render); the shell's Escape handler sits on
        // the focused surface's dispatch path, so the focus must land
        // first.
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("settings-category-General").is_none(),
            "Escape returns to the workspace, exactly like Back"
        );
        let focus = palette_test_sidebar_focus(&workspace, &cx);
        assert!(
            cx.update(|window, _| focus.is_focused(window)),
            "closing returns focus to a surface in the main frame, so ctrl-k keeps working"
        );
    }

    #[gpui::test]
    async fn settings_visibility_toggles_reach_the_usage_bar(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });

        assert!(
            cx.debug_bounds("Claude-usage-text").is_some(),
            "the bar starts with Claude visible per the contract defaults"
        );
        assert!(
            cx.debug_bounds("OpenCode Go-usage-text").is_none(),
            "the bar starts with OpenCode Go hidden per the contract defaults"
        );

        workspace.update(&mut cx, |workspace, cx| workspace.open_settings(None, cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();
        let toggle = cx
            .debug_bounds("provider-claude-visibility")
            .expect("Claude's Show in usage bar toggle renders");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();

        // The real Back route is queued through the workspace's polling loop;
        // hide the overlay directly here so the assertion inspects the bar,
        // rather than the settings surface covering it.
        workspace.update(&mut cx, |workspace, cx| {
            workspace.show_settings = false;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("Claude-usage-text").is_none(),
            "hiding Claude in settings removes its usage segment"
        );
        assert!(
            cx.debug_bounds("Codex-usage-text").is_some(),
            "the other visible providers stay"
        );
    }

    #[gpui::test]
    async fn drawn_disabled_save_row_shows_reason_and_does_not_dispatch(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("palette workspace root")
        });

        open_palette_for_test(&mut cx, &workspace);
        cx.simulate_input("save file");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("command-palette-disabled-no-active-file")
                .is_some()
        );
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(cx.debug_bounds("command-palette").is_some());
    }

    #[gpui::test]
    async fn resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(cx.debug_bounds("command-palette").is_none());
        assert!(cx.debug_bounds("command-menu-bar").is_none());
    }

    #[gpui::test]
    async fn linux_window_command_chords_dispatch_typed_shell_actions(cx: &mut TestAppContext) {
        let fired = Rc::new(RefCell::new(Vec::new()));
        let window = cx.add_window(|_window, cx| WindowCommandFixture::new(fired.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let fixture = cx.update(|window, _| {
            window
                .root::<WindowCommandFixture>()
                .flatten()
                .expect("fixture root")
        });
        let focus_handle = fixture.read_with(&cx.cx, |fixture, _| fixture.focus_handle.clone());
        cx.update(|window, app| focus_handle.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-t ctrl-o ctrl-s ctrl-shift-s ctrl-shift-i");
        cx.run_until_parked();

        assert_eq!(
            fired.borrow().as_slice(),
            &[
                WindowCommand::NewTerminalTab,
                WindowCommand::OpenFile,
                WindowCommand::SaveFile,
                WindowCommand::ToggleSidebar,
                WindowCommand::ToggleRightPanel,
            ],
            "Linux primary and secondary chords must reach typed shell actions"
        );
    }

    #[test]
    fn linux_shell_commands_use_linux_primary_and_secondary_chords() {
        assert_eq!(
            linux_window_shortcuts(),
            [
                (WindowCommand::NewTerminalTab, "ctrl-t"),
                (WindowCommand::OpenFile, "ctrl-o"),
                (WindowCommand::SaveFile, "ctrl-s"),
                (WindowCommand::ToggleSidebar, "ctrl-shift-s"),
                (WindowCommand::ToggleRightPanel, "ctrl-shift-i"),
            ]
        );
    }

    #[test]
    fn terminal_context_app_actions_have_workspace_routes() {
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SetTitle),
            Some(TerminalContextCommand::SetTitle)
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SplitRight),
            Some(TerminalContextCommand::Split {
                direction: SplitDirection::Horizontal,
                placement: SplitPlacement::After,
            })
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SplitDown),
            Some(TerminalContextCommand::Split {
                direction: SplitDirection::Vertical,
                placement: SplitPlacement::After,
            })
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SplitLeft),
            Some(TerminalContextCommand::Split {
                direction: SplitDirection::Horizontal,
                placement: SplitPlacement::Before,
            })
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SplitAbove),
            Some(TerminalContextCommand::Split {
                direction: SplitDirection::Vertical,
                placement: SplitPlacement::Before,
            })
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::CloseTerminal),
            Some(TerminalContextCommand::Close)
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::Copy),
            None
        );
        assert_eq!(
            split_event_name(SplitDirection::Horizontal, SplitPlacement::Before),
            "horizontal-before"
        );
        assert_eq!(
            parse_split_event("vertical-before"),
            Some((SplitDirection::Vertical, SplitPlacement::Before))
        );
        assert_eq!(SPLIT_DIVIDER_SIZE, 6.0);
        assert_eq!(MIN_SPLIT_PANE_SIZE, 160.0);
    }

    #[test]
    fn save_command_is_disabled_without_an_active_file_and_explains_why() {
        assert_eq!(
            window_command_availability(WindowCommand::SaveFile, None),
            WindowCommandAvailability::Disabled(WindowCommandDisabledReason::NoActiveFile)
        );
        assert_eq!(
            window_command_availability(WindowCommand::SaveFile, Some(TabKind::Editor)),
            WindowCommandAvailability::Enabled
        );
    }

    #[test]
    fn persisted_settings_round_trip_maps_all_sixteen_fields_explicitly() {
        let persisted = AppSettings {
            appearance: AppearanceMode::Dark,
            ui_font_size: 17,
            terminal_font_size: 19,
            file_icon_theme: FileIconTheme::Material,
            control_socket_enabled: false,
            resume_agent_sessions: false,
            auto_naming: true,
            limit_chat_history: false,
            chat_retention: 37,
            limit_mounted_worktrees: true,
            mounted_worktrees: 17,
            summarizer_agent: "codex".into(),
            claude_show_in_bar: false,
            codex_show_in_bar: false,
            opencode_show_in_bar: true,
            refresh_interval_min: 11,
        };

        let snapshot = settings_snapshot_from_app_settings(persisted.clone());
        assert_eq!(snapshot.theme, tiller_theme::ThemeMode::Dark);
        assert_eq!(snapshot.interface_font_size, 17);
        assert_eq!(snapshot.terminal_font_size, 19);
        assert_eq!(
            snapshot.file_icons,
            tiller_ui::settings::FileIconChoice::Material
        );
        assert!(!snapshot.control_socket_enabled);
        assert!(!snapshot.resume_agent_sessions);
        assert!(snapshot.auto_naming);
        assert!(!snapshot.limit_chat_history);
        assert_eq!(snapshot.chat_retention, 37);
        assert!(snapshot.limit_mounted_worktrees);
        assert_eq!(snapshot.mounted_worktrees, 17);
        assert_eq!(
            snapshot.summarizer_agent,
            tiller_ui::settings::SummarizerChoice::Codex
        );
        assert!(!snapshot.claude_show_in_bar);
        assert!(!snapshot.codex_show_in_bar);
        assert!(snapshot.opencode_show_in_bar);
        assert_eq!(snapshot.refresh_interval, 11);

        let restored = app_settings_from_snapshot(snapshot);
        assert_eq!(restored.appearance, persisted.appearance);
        assert_eq!(restored.ui_font_size, persisted.ui_font_size);
        assert_eq!(restored.terminal_font_size, persisted.terminal_font_size);
        assert_eq!(restored.file_icon_theme, persisted.file_icon_theme);
        assert_eq!(
            restored.control_socket_enabled,
            persisted.control_socket_enabled
        );
        assert_eq!(
            restored.resume_agent_sessions,
            persisted.resume_agent_sessions
        );
        assert_eq!(restored.auto_naming, persisted.auto_naming);
        assert_eq!(restored.limit_chat_history, persisted.limit_chat_history);
        assert_eq!(restored.chat_retention, persisted.chat_retention);
        assert_eq!(
            restored.limit_mounted_worktrees,
            persisted.limit_mounted_worktrees
        );
        assert_eq!(restored.mounted_worktrees, persisted.mounted_worktrees);
        assert_eq!(restored.summarizer_agent, persisted.summarizer_agent);
        assert_eq!(restored.claude_show_in_bar, persisted.claude_show_in_bar);
        assert_eq!(restored.codex_show_in_bar, persisted.codex_show_in_bar);
        assert_eq!(
            restored.opencode_show_in_bar,
            persisted.opencode_show_in_bar
        );
        assert_eq!(
            restored.refresh_interval_min,
            persisted.refresh_interval_min
        );

        let mut invalid_summarizer = persisted;
        invalid_summarizer.summarizer_agent = "not-a-supported-agent".into();
        assert_eq!(
            settings_snapshot_from_app_settings(invalid_summarizer).summarizer_agent,
            tiller_ui::settings::SummarizerChoice::Claude
        );
    }

    #[test]
    fn changing_only_the_theme_does_not_erase_other_persisted_settings() {
        let root = std::env::temp_dir().join(format!(
            "tiller-settings-destructive-property-{}-{}",
            std::process::id(),
            TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create settings fixture");
        let store = SessionStore::open(&root.join("tiller.sqlite"));
        let persisted = AppSettings {
            appearance: AppearanceMode::Dark,
            ui_font_size: 17,
            terminal_font_size: 19,
            file_icon_theme: FileIconTheme::Material,
            control_socket_enabled: false,
            resume_agent_sessions: false,
            auto_naming: true,
            limit_chat_history: false,
            chat_retention: 37,
            limit_mounted_worktrees: true,
            mounted_worktrees: 17,
            summarizer_agent: "codex".into(),
            claude_show_in_bar: false,
            codex_show_in_bar: false,
            opencode_show_in_bar: true,
            refresh_interval_min: 11,
        };
        store.save_settings(&persisted);

        let mut changed_snapshot = settings_snapshot_from_app_settings(persisted.clone());
        changed_snapshot.theme = tiller_theme::ThemeMode::Light;
        store.save_settings(&app_settings_from_snapshot(changed_snapshot));

        let restored = store.load_settings();
        assert_eq!(restored.appearance, AppearanceMode::Light);
        assert_eq!(restored.ui_font_size, persisted.ui_font_size);
        assert_eq!(restored.terminal_font_size, persisted.terminal_font_size);
        assert_eq!(restored.file_icon_theme, persisted.file_icon_theme);
        assert_eq!(
            restored.control_socket_enabled,
            persisted.control_socket_enabled
        );
        assert_eq!(
            restored.resume_agent_sessions,
            persisted.resume_agent_sessions
        );
        assert_eq!(restored.auto_naming, persisted.auto_naming);
        assert_eq!(restored.limit_chat_history, persisted.limit_chat_history);
        assert_eq!(restored.chat_retention, persisted.chat_retention);
        assert_eq!(
            restored.limit_mounted_worktrees,
            persisted.limit_mounted_worktrees
        );
        assert_eq!(restored.mounted_worktrees, persisted.mounted_worktrees);
        assert_eq!(restored.summarizer_agent, persisted.summarizer_agent);
        assert_eq!(restored.claude_show_in_bar, persisted.claude_show_in_bar);
        assert_eq!(restored.codex_show_in_bar, persisted.codex_show_in_bar);
        assert_eq!(
            restored.opencode_show_in_bar,
            persisted.opencode_show_in_bar
        );
        assert_eq!(
            restored.refresh_interval_min,
            persisted.refresh_interval_min
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tillerctl_resolver_installs_the_sibling_binary_in_xdg_data_bin() {
        let root = std::env::temp_dir().join(format!(
            "tiller-tillerctl-resolution-{}-{}",
            std::process::id(),
            TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let executable_dir = root.join("target/debug");
        let data_home = root.join("data");
        std::fs::create_dir_all(&executable_dir).expect("create executable fixture");
        let current_exe = executable_dir.join("tiller");
        let tillerctl = executable_dir.join("tillerctl");
        std::fs::write(&current_exe, b"tiller").expect("write app fixture");
        std::fs::write(&tillerctl, b"tillerctl").expect("write tillerctl fixture");
        make_executable(&current_exe);
        make_executable(&tillerctl);

        let environment = BTreeMap::from([
            (
                "XDG_DATA_HOME".to_string(),
                data_home.to_string_lossy().into_owned(),
            ),
            (
                "HOME".to_string(),
                root.join("home").to_string_lossy().into_owned(),
            ),
            (
                "PATH".to_string(),
                root.join("empty-path").to_string_lossy().into_owned(),
            ),
        ]);

        let resolved = resolve_tillerctl_path(&current_exe, &environment)
            .expect("sibling tillerctl should be installed");
        assert_eq!(resolved, data_home.join("TillerRust/bin/tillerctl"));
        assert!(resolved.is_absolute());
        assert_eq!(
            std::fs::canonicalize(&resolved).expect("installed tillerctl exists"),
            std::fs::canonicalize(&tillerctl).expect("source tillerctl exists")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tillerctl_resolver_falls_back_to_path_and_reports_missing_binary() {
        let root = std::env::temp_dir().join(format!(
            "tiller-tillerctl-path-resolution-{}-{}",
            std::process::id(),
            TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let executable_dir = root.join("app");
        let path_dir = root.join("path");
        let data_home = root.join("data");
        std::fs::create_dir_all(&executable_dir).expect("create app fixture");
        std::fs::create_dir_all(&path_dir).expect("create PATH fixture");
        let current_exe = executable_dir.join("tiller");
        let path_tillerctl = path_dir.join("tillerctl");
        std::fs::write(&current_exe, b"tiller").expect("write app fixture");
        std::fs::write(&path_tillerctl, b"tillerctl").expect("write PATH fixture");
        make_executable(&current_exe);
        make_executable(&path_tillerctl);

        let mut environment = BTreeMap::from([
            (
                "XDG_DATA_HOME".to_string(),
                data_home.to_string_lossy().into_owned(),
            ),
            ("PATH".to_string(), path_dir.to_string_lossy().into_owned()),
        ]);
        let resolved = resolve_tillerctl_path(&current_exe, &environment)
            .expect("PATH tillerctl should be installed");
        assert_eq!(resolved, data_home.join(TILLERCTL_INSTALL_SUBPATH));
        assert_eq!(
            std::fs::canonicalize(&resolved).expect("PATH installation exists"),
            std::fs::canonicalize(&path_tillerctl).expect("PATH source exists")
        );

        environment.insert(
            "XDG_DATA_HOME".into(),
            root.join("missing-data").display().to_string(),
        );
        environment.insert(
            "PATH".into(),
            root.join("missing-path").display().to_string(),
        );
        let error = resolve_tillerctl_path(&root.join("missing/tiller"), &environment)
            .expect_err("missing tillerctl must be surfaced");
        assert!(error.contains("tillerctl is unavailable"));
        assert!(!error.contains("using bare tillerctl"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("make fixture executable");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    #[test]
    fn restoring_launch_snapshot_adds_missing_tabs_without_replacing_current_tabs() {
        let snapshot = vec![
            SessionTab {
                title: "Chat".into(),
                kind: "chat".into(),
                agent_id: None,
                active: false,
            },
            SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            },
        ];
        let current = vec![SessionTab {
            title: "Changes".into(),
            kind: "diff".into(),
            agent_id: None,
            active: true,
        }];

        let restored = merge_launch_snapshot_tabs(&snapshot, &current);

        assert_eq!(
            restored,
            vec![
                current[0].clone(),
                SessionTab {
                    active: false,
                    ..snapshot[0].clone()
                },
                SessionTab {
                    active: false,
                    ..snapshot[1].clone()
                },
            ],
            "restore must append closed launch tabs while preserving the current tab"
        );
    }

    #[test]
    fn pane_event_history_replays_three_panes_and_preserves_surviving_ids() {
        let events = vec![
            PaneEvent::Split {
                focused: 0,
                new_id: 1,
                direction: "horizontal".into(),
            },
            PaneEvent::Split {
                focused: 1,
                new_id: 2,
                direction: "vertical".into(),
            },
            PaneEvent::SetRatio {
                path: vec![true],
                ratio_millis: 750,
            },
            PaneEvent::Close { id: 0 },
        ];

        let tree = replay_pane_events(0, "root", &events, |id| match id {
            1 => "right",
            2 => "down",
            _ => "unexpected",
        });

        assert_eq!(tree.leaf_ids(), vec![1, 2]);
        assert!(tree.contains(1));
        assert!(tree.contains(2));
    }

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

        assert_eq!(
            agent_icon_for_action(NewTabAction::ClaudeCode),
            Some(Icon::ClaudeCode)
        );
        assert_eq!(
            agent_icon_for_action(NewTabAction::Codex),
            Some(Icon::Codex)
        );
        assert_eq!(
            agent_icon_for_action(NewTabAction::OpenCode),
            Some(Icon::OpenCode)
        );
        assert_eq!(agent_icon_for_action(NewTabAction::Pi), Some(Icon::Pi));
        assert_eq!(
            agent_icon_for_action(NewTabAction::OhMyPi),
            Some(Icon::OhMyPi)
        );
        assert_eq!(agent_icon_for_action(NewTabAction::NewTerminal), None);
        assert_eq!(agent_icon_for_action(NewTabAction::NewChat), None);
    }

    #[test]
    fn boot_settings_honor_tiller_socket_enable_environment_override() {
        let previous = std::env::var_os("TILLER_SOCKET_ENABLE");
        let settings = AppSettings::default();

        unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", "off") };
        assert!(
            !app_settings_with_environment_override(settings.clone()).control_socket_enabled,
            "the boot settings used by control_socket.set_enabled must observe off"
        );

        unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", "on") };
        assert!(app_settings_with_environment_override(settings).control_socket_enabled);

        match previous {
            Some(value) => unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", value) },
            None => unsafe { std::env::remove_var("TILLER_SOCKET_ENABLE") },
        }
    }

    #[test]
    fn needs_input_agent_status_reaches_the_dedicated_activity_state() {
        assert_eq!(
            activity_status_for_agent(AgentStatus::NeedsInput),
            ActivityStatus::NeedsInput
        );
    }

    #[test]
    fn opening_the_same_file_twice_reuses_one_editor_tab_path() {
        let open_paths = vec![PathBuf::from("/tmp/notes.md")];
        assert!(file_path_is_already_open(
            &open_paths,
            Path::new("/tmp/notes.md")
        ));
        assert!(!file_path_is_already_open(
            &open_paths,
            Path::new("/tmp/other.md")
        ));
    }

    #[test]
    fn selected_chat_adapter_is_copied_to_the_new_tab_identity() {
        let adapter = AGENT_CATALOG
            .iter()
            .find(|adapter| adapter.id() == "codex")
            .copied()
            .expect("Codex is in the fixed catalog");
        assert_eq!(
            chat_tab_identity(Some(adapter)),
            (
                "Codex".to_string(),
                Some(Icon::Codex),
                Some("codex".to_string())
            )
        );
    }

    #[test]
    fn restored_codex_chat_uses_codex_command_and_identity() {
        let (command, icon, agent_id) = restored_chat_spec(Some("codex"));
        assert_eq!(command.program, PathBuf::from("npx"));
        assert_eq!(
            command.args,
            ["-y", "@agentclientprotocol/codex-acp@latest"]
        );
        assert_eq!(icon, Some(Icon::Codex));
        assert_eq!(agent_id.as_deref(), Some("codex"));

        let (claude_command, _, _) = restored_chat_spec(Some("claude"));
        assert_ne!(
            command.args, claude_command.args,
            "restoration must preserve the selected adapter's ACP command"
        );
    }

    #[test]
    fn restored_unknown_or_absent_chat_identity_falls_back_without_claiming_an_agent() {
        let (default_command, default_icon, default_id) = restored_chat_spec(None);
        let (unknown_command, unknown_icon, unknown_id) = restored_chat_spec(Some("unknown-agent"));
        assert_eq!(unknown_command, default_command);
        assert_eq!(unknown_icon, None);
        assert_eq!(unknown_id, None);
        assert_eq!(default_icon, None);
        assert_eq!(default_id, None);
    }

    #[test]
    fn selecting_a_worktree_updates_current_and_row_flags() {
        let main_path = PathBuf::from("/tmp/tiller-selection-main");
        let feature_path = PathBuf::from("/tmp/tiller-selection-feature");
        let catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project".into(),
            name: "tiller".into(),
            root_path: main_path.clone(),
            is_git: true,
            worktrees: vec![
                session::CatalogWorktree {
                    branch: "main".into(),
                    path: main_path.clone(),
                    is_primary: true,
                },
                session::CatalogWorktree {
                    branch: "feature".into(),
                    path: feature_path.clone(),
                    is_primary: false,
                },
            ],
        }]);
        let mut state = ControlState::from_catalog(&catalog, &main_path);
        let expected_path = feature_path.to_string_lossy().into_owned();

        assert_eq!(state.current, Some(0));
        assert_eq!(
            state
                .workspaces
                .iter()
                .map(|workspace| workspace.selected)
                .collect::<Vec<_>>(),
            vec![true, false]
        );

        assert!(state.select_worktree(&feature_path));

        assert_eq!(state.current, Some(1));
        assert_eq!(
            state
                .workspaces
                .iter()
                .map(|workspace| workspace.selected)
                .collect::<Vec<_>>(),
            vec![false, true]
        );
        assert_eq!(
            state
                .current_workspace()
                .map(|workspace| workspace.path.as_str()),
            Some(expected_path.as_str())
        );
    }

    #[test]
    fn primary_context_transition_updates_one_catalog_worktree() {
        let main_path = PathBuf::from("/tmp/tiller-primary-main");
        let feature_path = PathBuf::from("/tmp/tiller-primary-feature");
        let mut catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project".into(),
            name: "tiller".into(),
            root_path: main_path.clone(),
            is_git: true,
            worktrees: vec![
                session::CatalogWorktree {
                    branch: "main".into(),
                    path: main_path.clone(),
                    is_primary: true,
                },
                session::CatalogWorktree {
                    branch: "feature".into(),
                    path: feature_path.clone(),
                    is_primary: false,
                },
            ],
        }]);

        catalog
            .set_primary(&feature_path, true)
            .expect("known worktree can become primary");
        assert_eq!(
            catalog.projects()[0]
                .worktrees
                .iter()
                .map(|worktree| worktree.is_primary)
                .collect::<Vec<_>>(),
            vec![false, true]
        );

        catalog
            .set_primary(&feature_path, false)
            .expect("known primary can be unset");
        assert!(
            catalog.projects()[0]
                .worktrees
                .iter()
                .all(|worktree| { !worktree.is_primary })
        );
    }

    #[test]
    fn duplicate_catalog_paths_have_one_selected_control_row() {
        let path = PathBuf::from("/tmp/tiller-selection-duplicate");
        let worktree = || session::CatalogWorktree {
            branch: "main".into(),
            path: path.clone(),
            is_primary: true,
        };
        let catalog = ProjectCatalog::from_projects(vec![
            session::CatalogProject {
                id: "first".into(),
                name: "first".into(),
                root_path: path.clone(),
                is_git: true,
                worktrees: vec![worktree()],
            },
            session::CatalogProject {
                id: "second".into(),
                name: "second".into(),
                root_path: path.clone(),
                is_git: true,
                worktrees: vec![worktree()],
            },
        ]);
        let state = ControlState::from_catalog(&catalog, &path);

        assert_eq!(
            state
                .workspaces
                .iter()
                .filter(|workspace| workspace.selected)
                .count(),
            1
        );
        assert_eq!(state.current, Some(0));
    }

    #[test]
    fn control_state_exposes_empty_and_discovered_project_rows() {
        let empty = ControlState::from_catalog(&ProjectCatalog::default(), Path::new("/tmp"));
        assert!(empty.project_rows().is_empty());

        let root = PathBuf::from("/tmp/tiller-project-row");
        let catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project-1".into(),
            name: "Tiller".into(),
            root_path: root.clone(),
            is_git: true,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: root.clone(),
                is_primary: true,
            }],
        }]);
        let state = ControlState::from_catalog(&catalog, &root);
        let rows = state.project_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("id").map(String::as_str), Some("project-1"));
        assert_eq!(rows[0].get("name").map(String::as_str), Some("Tiller"));
        assert_eq!(rows[0].get("worktreeCount").map(String::as_str), Some("1"));
        assert_eq!(rows[0].get("empty").map(String::as_str), Some("false"));
    }

    #[test]
    fn control_state_can_associate_a_session_and_comment_with_a_worktree() {
        let path = PathBuf::from("/tmp/tiller-control-association");
        let catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project".into(),
            name: "tiller".into(),
            root_path: path.clone(),
            is_git: true,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: path.clone(),
                is_primary: true,
            }],
        }]);
        let mut state = ControlState::from_catalog(&catalog, &path);

        let workspace = state
            .set_worktree("project-wt-0", Some("agent pane"), Some("pane-1"))
            .expect("worktree selector");

        assert_eq!(workspace.comment, "agent pane");
        assert_eq!(workspace.session.as_deref(), Some("pane-1"));
        let row = &state.workspace_rows()[0];
        assert_eq!(row.get("comment").map(String::as_str), Some("agent pane"));
        assert_eq!(row.get("mounted").map(String::as_str), Some("true"));
    }

    #[test]
    fn closing_a_worktree_removes_current_selection_but_keeps_the_row() {
        let path = PathBuf::from("/tmp/tiller-control-close");
        let catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project".into(),
            name: "tiller".into(),
            root_path: path.clone(),
            is_git: true,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: path.clone(),
                is_primary: true,
            }],
        }]);
        let mut state = ControlState::from_catalog(&catalog, &path);

        assert!(state.close_worktree(&path));
        assert!(state.current_workspace().is_none());
        assert_eq!(state.workspace_rows().len(), 1);
        assert_eq!(
            state.workspace_rows()[0]
                .get("selected")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            state.workspace_rows()[0].get("mounted").map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn control_ping_returns_the_documented_pong_result() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-ping-test.sock")),
        );
        let response = handler.handle(&ControlRequest {
            id: "ping-test".into(),
            method: "system.ping".into(),
            params: BTreeMap::new(),
        });

        assert!(response.ok);
        assert_eq!(
            response
                .result
                .as_ref()
                .and_then(|result| result.get("pong"))
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn raw_notify_accepts_user_title_and_body() {
        let state = Arc::new(Mutex::new(ControlState {
            projects: Vec::new(),
            workspaces: Vec::new(),
            current: None,
        }));
        let handler = AppControlHandler::new(
            state,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-test-control.sock")),
        );
        let notify = ControlRequest {
            id: "notify-user".into(),
            method: "notify".into(),
            params: BTreeMap::from([
                ("title".into(), "Build finished".into()),
                ("body".into(), "The Linux build is ready.".into()),
            ]),
        };

        let response = handler.handle(&notify);
        assert!(response.ok, "user notify failed: {:?}", response.error);

        let list = ControlRequest {
            id: "list-notifications".into(),
            method: "notification.list".into(),
            params: BTreeMap::new(),
        };
        let response = handler.handle(&list);
        let raw_rows = response
            .result
            .as_ref()
            .and_then(|result| result.get("notifications"))
            .expect("notification rows");
        let rows = tiller_control::protocol::rows::decode(raw_rows).expect("valid rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("title").map(String::as_str),
            Some("Build finished")
        );
        assert_eq!(
            rows[0].get("body").map(String::as_str),
            Some("The Linux build is ready.")
        );
    }

    #[test]
    fn session_ref_handler_write_is_visible_after_store_reopen() {
        let database = std::env::temp_dir().join(format!(
            "tiller-control-session-ref-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&database);
        let store = SessionStore::open(&database);
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            Some(store.clone()),
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-session-ref-test.sock")),
        );
        let response = handler.handle(&ControlRequest {
            id: "session-ref".into(),
            method: "session.ref".into(),
            params: BTreeMap::from([
                ("session".into(), "pane-nonce".into()),
                ("ref".into(), "agent-nonce".into()),
            ]),
        });
        assert!(response.ok, "session.ref failed: {:?}", response.error);
        drop(handler);
        drop(store);

        let reopened = SessionStore::open(&database);
        assert_eq!(
            reopened.load_session_refs().get("pane-nonce"),
            Some(&"agent-nonce".to_string())
        );
        drop(reopened);
        let _ = std::fs::remove_file(&database);
    }

    #[test]
    fn browser_methods_are_explicit_and_capabilities_are_truthful() {
        let state = Arc::new(Mutex::new(ControlState {
            projects: Vec::new(),
            workspaces: Vec::new(),
            current: None,
        }));
        let control_actions = Arc::new(Mutex::new(Vec::new()));
        let handler = AppControlHandler::new(
            state,
            control_actions.clone(),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-browser-test.sock")),
        );

        let capabilities = handler.handle(&ControlRequest {
            id: "capabilities".into(),
            method: "system.capabilities".into(),
            params: BTreeMap::new(),
        });
        let methods = capabilities
            .result
            .as_ref()
            .and_then(|result| result.get("methods"))
            .and_then(|json| tiller_control::protocol::rows::decode(json))
            .expect("capability rows");
        let advertised: Vec<_> = methods
            .iter()
            .filter_map(|row| row.get("method").map(String::as_str))
            .filter(|method| method.starts_with("browser."))
            .collect();
        assert_eq!(
            advertised,
            ["browser.open", "browser.navigate", "browser.act"]
        );

        for method in [
            "browser.get",
            "browser.screenshot",
            "browser.snapshot",
            "browser.wait",
            "browser.eval",
            "browser.console",
            "browser.errors",
        ] {
            let response = handler.handle(&ControlRequest {
                id: method.to_string(),
                method: method.to_string(),
                params: BTreeMap::new(),
            });
            assert!(!response.ok, "{method} must not report fabricated success");
            let error = response.error.as_deref().expect("unsupported error");
            assert!(error.contains(method), "error must name {method}: {error}");
            assert!(
                error.contains("unsupported"),
                "error must explain {method}: {error}"
            );
            assert!(control_actions.lock().expect("action queue").is_empty());
        }

        for (method, params) in [
            ("browser.open", BTreeMap::new()),
            (
                "browser.navigate",
                BTreeMap::from([(String::from("action"), String::from("reload"))]),
            ),
            (
                "browser.act",
                BTreeMap::from([(String::from("verb"), String::from("click"))]),
            ),
        ] {
            let response = handler.handle(&ControlRequest {
                id: method.to_string(),
                method: method.to_string(),
                params,
            });
            assert!(!response.ok, "{method} must reject unsupported input");
            let error = response.error.as_deref().expect("request error");
            assert!(error.contains(method), "error must name {method}: {error}");
            assert!(
                error.contains("requires") || error.contains("unsupported"),
                "error must explain {method}: {error}"
            );
            assert!(control_actions.lock().expect("action queue").is_empty());
        }
    }

    #[test]
    fn browser_tabs_have_shell_icon_and_width() {
        assert_eq!(tab_icon(TabKind::Browser, false, None), Icon::Globe);
        assert_eq!(TillerWorkspace::tab_width(TabKind::Browser), CHAT_TAB_WIDTH);
    }

    #[test]
    fn seam_width_matches_reference_divider() {
        assert_eq!(SEAM_WIDTH, 6.0);
    }

    #[test]
    fn surface_methods_are_advertised_by_capabilities() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-surface-test.sock")),
        );

        let response = handler.handle(&ControlRequest {
            id: "capabilities".into(),
            method: "system.capabilities".into(),
            params: BTreeMap::new(),
        });
        let methods = response
            .result
            .as_ref()
            .and_then(|result| result.get("methods"))
            .and_then(|encoded| tiller_control::protocol::rows::decode(encoded))
            .expect("capability rows");
        for method in [
            "project.list",
            "project.add",
            "panel.state",
            "panel.scrollback",
            "surface.changes.open",
            "surface.changes.read",
            "surface.changes.stage",
            "surface.changes.unstage",
            "surface.changes.discard",
            "surface.changes.stage_all",
            "surface.changes.discard_all",
            "surface.settings.open",
            "surface.settings.select",
            "surface.settings.read",
        ] {
            assert!(
                methods
                    .iter()
                    .any(|row| row.get("method").map(String::as_str) == Some(method)),
                "{method} must be advertised"
            );
        }
    }

    #[test]
    fn chat_surface_methods_are_advertised_and_reach_the_app_handler() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-chat-dispatch-test.sock")),
        );

        let capabilities = handler.handle(&ControlRequest {
            id: "capabilities".into(),
            method: "system.capabilities".into(),
            params: BTreeMap::new(),
        });
        let methods = capabilities
            .result
            .as_ref()
            .and_then(|result| result.get("methods"))
            .and_then(|encoded| tiller_control::protocol::rows::decode(encoded))
            .expect("capability rows");
        for method in [
            "surface.chat.open",
            "surface.chat.send",
            "surface.chat.compose",
            "surface.chat.permission",
            "surface.chat.stop",
            "surface.chat.read",
        ] {
            assert!(
                methods
                    .iter()
                    .any(|row| row.get("method").map(String::as_str) == Some(method)),
                "{method} must be advertised"
            );
        }

        for (index, method) in [
            "surface.chat.send",
            "surface.chat.compose",
            "surface.chat.permission",
            "surface.chat.stop",
            "surface.chat.read",
        ]
        .into_iter()
        .enumerate()
        {
            let mut params = BTreeMap::from([(String::from("surfaceId"), String::from("missing"))]);
            match method {
                "surface.chat.send" | "surface.chat.compose" => {
                    params.insert("text".into(), "input".into());
                }
                "surface.chat.permission" => {
                    params.insert("requestId".into(), "1".into());
                    params.insert("optionId".into(), "deny".into());
                }
                "surface.chat.stop" | "surface.chat.read" => {}
                _ => unreachable!("chat dispatch test method is exhaustive"),
            }
            let response = handler.handle(&ControlRequest {
                id: format!("chat-dispatch-{index}"),
                method: method.into(),
                params,
            });
            assert!(!response.ok, "unopened {method} must fail");
            let expected_error = if method == "surface.chat.read" {
                "unknown chat surface: missing"
            } else {
                "chat surface is not open: missing"
            };
            assert!(
                response
                    .error
                    .as_deref()
                    .is_some_and(|error| error == expected_error),
                "{method} must report an unopened surface: {response:?}"
            );
            assert_ne!(
                response.error.as_deref(),
                Some(format!("unknown control method: {method}").as_str()),
                "{method} must reach its chat handler"
            );
        }
    }

    fn app_chat_read(socket_path: &Path, surface_id: &str) -> BTreeMap<String, String> {
        let response = tiller_control::round_trip(
            socket_path,
            &tiller_control::protocol::request::chat_read(surface_id),
            Duration::from_secs(5),
        )
        .expect("chat read round trip");
        assert!(response.ok, "chat read failed: {response:?}");
        response.result.expect("chat read result")
    }

    fn wait_for_app_chat_read<F>(
        socket_path: &Path,
        surface_id: &str,
        mut predicate: F,
    ) -> BTreeMap<String, String>
    where
        F: FnMut(&BTreeMap<String, String>) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = app_chat_read(socket_path, surface_id);
            if predicate(&result) {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "chat state did not settle: {result:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn app_chat_surface_streams_stops_and_restores_over_a_real_socket() {
        let root =
            std::env::temp_dir().join(format!("tiller-main-chat-door-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let worktree_path = root.join("worktree");
        std::fs::create_dir_all(&worktree_path).expect("create chat worktree");
        let database_path = root.join("chat.sqlite");
        let socket_path = root.join("chat.sock");
        let worktree_id = "project-chat-wt-0";
        let surface_id = "project-chat-wt-0-tab-0";

        let database = AppDatabase::open(&database_path).expect("open chat database");
        database
            .save_project(&tiller_persistence::ProjectRecord::new(
                "project-chat",
                "fixture",
                worktree_path.to_string_lossy(),
            ))
            .expect("save chat project");
        database
            .save_worktree(&tiller_persistence::WorktreeRecord::new(
                worktree_id,
                "project-chat",
                "main",
                worktree_path.to_string_lossy(),
            ))
            .expect("save chat worktree");
        database
            .save_tab(&TabRecord::new(surface_id, worktree_id, "Chat", "chat"))
            .expect("save chat tab");
        drop(database);

        let catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "project-chat".into(),
            name: "fixture".into(),
            root_path: worktree_path.clone(),
            is_git: false,
            worktrees: vec![session::CatalogWorktree {
                branch: "main".into(),
                path: worktree_path.clone(),
                is_primary: true,
            }],
        }]);
        let state = Arc::new(Mutex::new(ControlState::from_catalog(
            &catalog,
            &worktree_path,
        )));
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tiller_acp/tests/fixtures/acp_fixture.py");
        let normal_command = AgentCommand::new("python3")
            .arg(fixture.to_string_lossy().into_owned())
            .arg("normal");
        let normal_handler = Arc::new(AppControlHandler::new_with_chat_config(
            state.clone(),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(socket_path.clone()),
            database_path.clone(),
            normal_command,
        ));
        let server = ControlServer::new(socket_path.clone(), normal_handler.clone());
        server.start().expect("start chat control server");

        let round_trip = |request: ControlRequest| {
            tiller_control::round_trip(&socket_path, &request, Duration::from_secs(5))
                .expect("chat control round trip")
        };
        let opened = round_trip(tiller_control::protocol::request::chat_open(Some(
            worktree_id,
        )));
        assert!(opened.ok, "chat open failed: {opened:?}");
        assert_eq!(
            opened
                .result
                .as_ref()
                .and_then(|result| result.get("status")),
            Some(&"idle".to_string())
        );

        let composed = round_trip(tiller_control::protocol::request::chat_compose(
            surface_id,
            "queued after this turn",
        ));
        assert!(composed.ok, "chat compose failed: {composed:?}");
        assert_eq!(
            composed
                .result
                .as_ref()
                .and_then(|result| result.get("composerText")),
            Some(&"queued after this turn".to_string())
        );

        let sent = round_trip(tiller_control::protocol::request::chat_send(
            surface_id,
            "exercise the app chat door",
        ));
        assert!(sent.ok, "chat send failed: {sent:?}");
        assert_eq!(
            sent.result.as_ref().and_then(|result| result.get("status")),
            Some(&"streaming".to_string())
        );

        let pending = wait_for_app_chat_read(&socket_path, surface_id, |result| {
            tiller_control::protocol::rows::decode(result.get("transcript").expect("transcript"))
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get("kind").map(String::as_str) == Some("permission")
                            && row.get("status").map(String::as_str) == Some("pending")
                    })
                })
        });
        assert_eq!(pending.get("status").map(String::as_str), Some("streaming"));

        let permission = round_trip(tiller_control::protocol::request::chat_permission(
            surface_id, 1, "deny",
        ));
        assert!(permission.ok, "chat permission failed: {permission:?}");
        let completed = wait_for_app_chat_read(&socket_path, surface_id, |result| {
            result.get("status").map(String::as_str) == Some("completed")
        });
        let completed_transcript = completed.get("transcript").cloned().expect("transcript");

        server.stop();
        drop(server);
        drop(normal_handler);

        let cancel_command = AgentCommand::new("python3")
            .arg(fixture.to_string_lossy().into_owned())
            .arg("cancel");
        let restored_handler = Arc::new(AppControlHandler::new_with_chat_config(
            state,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(socket_path.clone()),
            database_path.clone(),
            cancel_command,
        ));
        let restored_server = ControlServer::new(socket_path.clone(), restored_handler.clone());
        restored_server
            .start()
            .expect("restart chat control server");

        let restored = app_chat_read(&socket_path, surface_id);
        assert_eq!(
            restored.get("status").map(String::as_str),
            Some("completed")
        );
        assert_eq!(restored.get("transcript"), Some(&completed_transcript));

        let reopened = round_trip(tiller_control::protocol::request::chat_open(Some(
            worktree_id,
        )));
        assert!(reopened.ok, "reopen chat failed: {reopened:?}");
        round_trip(tiller_control::protocol::request::chat_send(
            surface_id,
            "stop this turn",
        ));
        wait_for_app_chat_read(&socket_path, surface_id, |result| {
            result.get("status").map(String::as_str) == Some("streaming")
                && tiller_control::protocol::rows::decode(
                    result.get("transcript").expect("transcript"),
                )
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get("kind").map(String::as_str) == Some("assistant")
                            && row.get("text").map(String::as_str) == Some("partial")
                    })
                })
        });
        let stopped_request = round_trip(tiller_control::protocol::request::chat_stop(surface_id));
        assert!(stopped_request.ok, "chat stop failed: {stopped_request:?}");
        let stopped = wait_for_app_chat_read(&socket_path, surface_id, |result| {
            result.get("status").map(String::as_str) == Some("stopped")
                && tiller_control::protocol::rows::decode(
                    result.get("transcript").expect("transcript"),
                )
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get("kind").map(String::as_str) == Some("turn")
                            && row.get("text").map(String::as_str) == Some("Cancelled")
                    })
                })
        });
        assert_eq!(stopped.get("status").map(String::as_str), Some("stopped"));

        restored_server.stop();
        drop(restored_server);
        drop(restored_handler);
        let relaunch_handler = Arc::new(AppControlHandler::new_with_chat_config(
            Arc::new(Mutex::new(ControlState::from_catalog(
                &catalog,
                &worktree_path,
            ))),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(socket_path.clone()),
            database_path,
            AgentCommand::new("python3"),
        ));
        let relaunch_server = ControlServer::new(socket_path.clone(), relaunch_handler.clone());
        relaunch_server.start().expect("start relaunch chat server");
        let relaunched = { app_chat_read(&socket_path, surface_id) };
        assert_eq!(
            relaunched.get("status").map(String::as_str),
            Some("stopped")
        );
        assert!(
            tiller_control::protocol::rows::decode(
                relaunched.get("transcript").expect("relaunch transcript")
            )
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.get("kind").map(String::as_str) == Some("turn")
                        && row.get("text").map(String::as_str) == Some("Cancelled")
                })
            })
        );
        relaunch_server.stop();
        drop(relaunch_server);
        drop(relaunch_handler);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_list_is_an_observable_empty_state() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-project-list-test.sock")),
        );

        let response = handler.handle(&tiller_control::protocol::request::project_list());
        assert!(response.ok, "project.list failed: {:?}", response.error);
        let rows = response
            .result
            .as_ref()
            .and_then(|result| result.get("projects"))
            .and_then(|encoded| tiller_control::protocol::rows::decode(encoded))
            .expect("project rows");
        assert!(
            rows.is_empty(),
            "clean launch must expose an empty project list"
        );
    }

    #[test]
    fn changes_mutations_validate_path_and_worktree_before_git() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-changes-mutation-test.sock")),
        );

        let missing_path = handler.handle(&ControlRequest {
            id: "missing-path".into(),
            method: "surface.changes.stage".into(),
            params: BTreeMap::new(),
        });
        assert!(!missing_path.ok);
        assert!(
            missing_path
                .error
                .as_deref()
                .is_some_and(|error| error.contains("path")),
            "missing path error: {:?}",
            missing_path.error
        );

        let unknown_worktree = handler.handle(&ControlRequest {
            id: "unknown-worktree".into(),
            method: "surface.changes.stage".into(),
            params: BTreeMap::from([
                ("path".into(), "f.txt".into()),
                ("worktree".into(), "wt-missing".into()),
            ]),
        });
        assert!(!unknown_worktree.ok);
        assert_eq!(unknown_worktree.error.as_deref(), Some("unknown worktree"));
    }

    #[test]
    fn socket_controller_starts_and_stops_the_server_for_the_setting() {
        let socket_path =
            std::env::temp_dir().join(format!("tiller-control-toggle-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);
        let info = ControlSocketInfo::new(socket_path.clone());
        let handler = Arc::new(AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            info.clone(),
        ));
        let controller = ControlSocketController::new(info.clone(), handler);

        controller.set_enabled(true);
        assert!(info.enabled());
        assert!(socket_path.exists());
        controller.set_enabled(false);
        assert!(!info.enabled());
        assert!(!socket_path.exists());
    }

    fn missing_directory(tag: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("tiller-restore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    /// Restoring a session whose terminal tab points at a directory that no
    /// longer exists must degrade per item: the workspace still opens with
    /// every tab present, and the bad terminal renders as a failed pane —
    /// never a process abort. This is the launch path that runs BEFORE the
    /// window shows, so a panic here would brick the app for good.
    #[gpui::test]
    async fn restore_survives_a_terminal_tab_in_a_missing_directory(cx: &mut TestAppContext) {
        // A session saved while a worktree existed; the worktree is gone now.
        let working_directory = missing_directory("layout");
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![
                session::SessionTab {
                    title: "Chat".into(),
                    kind: "chat".into(),
                    agent_id: None,
                    active: false,
                },
                session::SessionTab {
                    title: "Terminal".into(),
                    kind: "terminal".into(),
                    agent_id: None,
                    active: true,
                },
            ],
            tab_states: vec![
                session::SessionTabState::default(),
                session::SessionTabState::default(),
            ],
            diagnostics: vec![],
        };

        let (tabs, active) = cx.update(|cx| restore_tabs(&restored, &working_directory, None, cx));

        assert_eq!(
            tabs.len(),
            2,
            "the chat tab AND the bad terminal tab are both present"
        );
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
                    TabContent::Changes(_) => {}
                    TabContent::Browser(_) => {}
                });
            }
            (failed, live)
        });
        assert_eq!(failed, 1, "the terminal pane is a visible failed pane");
        assert_eq!(live, 0);
        // (Rendering the failed pane headlessly is covered by the terminal
        // crate's own gpui test: `failed_pane_renders_and_retry_recovers`.)
    }

    #[gpui::test]
    async fn drawn_restore_round_trips_codex_identity_through_quit_and_relaunch(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let unique = TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed);
        let scratch_root = std::env::temp_dir().join(format!(
            "tiller-p73-codex-restore-{}-{unique}",
            std::process::id()
        ));
        let working_directory = scratch_root.join("worktree");
        std::fs::create_dir_all(&working_directory).expect("create restore worktree");
        // Keep the database and its SQLite `-wal` sidecar in this test's
        // private namespace; a neighbouring restore cannot observe either.
        let database_path = scratch_root.join("tiller.sqlite");
        let store = SessionStore::open(&database_path);
        store.schedule(SessionLayout {
            working_directory: working_directory.clone(),
            branch: "main".into(),
            tabs: vec![SessionTab {
                title: "Codex".into(),
                kind: "chat".into(),
                agent_id: Some("codex".into()),
                active: true,
            }],
            tab_states: vec![SessionTabState::default()],
        });
        store.flush_now();
        drop(store);
        let restored = session::restore(&database_path, &working_directory);

        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // Chat restoration starts an ACP worker. Permit its late wakeup while
        // the entity is being released at the end of this test.
        cx.cx.executor().allow_parking();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let (tabs, active) =
            cx.update(|_, cx| restore_tabs(&restored, &working_directory, None, cx));
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs = tabs;
            workspace.active_tab = active;
            workspace.rebuild_tab_machinery();
            workspace.sync_activity(cx);
            cx.notify();
        });
        cx.run_until_parked();

        assert!(workspace.read_with(&cx.cx, |workspace, _| {
            workspace.tabs.iter().any(|tab| {
                tab.kind == TabKind::AgentChat
                    && tab.agent_id.as_deref() == Some("codex")
                    && tab.agent_icon == Some(Icon::Codex)
            })
        }));
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs.clear();
            cx.notify();
        });
        cx.run_until_parked();
        drop(workspace);
        let _ = std::fs::remove_dir_all(&scratch_root);
    }

    #[gpui::test]
    async fn drawn_changes_open_diff_action_opens_a_diff_tab_in_the_workspace(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let repo = changed_test_repo("changes-open-diff");
        let window = cx.add_window({
            let repo = repo.clone();
            move |_window, cx| test_workspace_for_repo(cx, repo, true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let row = wait_for_drawn(&mut cx, "changes-file-row");
        cx.simulate_click(row.center(), Modifiers::none());
        let open_diff = wait_for_drawn(&mut cx, "changes-open-diff");
        cx.simulate_click(open_diff.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace
                    .tabs
                    .iter()
                    .filter(|tab| tab.kind == TabKind::Diff)
                    .count()
            }),
            2,
            "the host subscriber opens a new Diff tab after the drawn action"
        );
    }

    #[gpui::test]
    async fn drawn_right_panel_open_diff_action_opens_a_diff_tab_in_the_workspace(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let repo = changed_test_repo("right-panel-open-diff");
        let window = cx.add_window({
            let repo = repo.clone();
            move |_window, cx| test_workspace_for_repo(cx, repo, false)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let open_diff = wait_for_drawn(&mut cx, "file-open-diff");
        cx.simulate_click(open_diff.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs.iter().any(|tab| tab.kind == TabKind::Diff)
            }),
            "the right-panel action subscriber opens a Diff tab"
        );
    }

    #[gpui::test]
    async fn drawn_conflict_resolve_action_opens_terminal_with_the_exact_path(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let repo = conflicted_test_repo("resolve-conflict");
        let window = cx.add_window({
            let repo = repo.clone();
            move |_window, cx| test_workspace_for_repo(cx, repo, true)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let row = wait_for_drawn(&mut cx, "changes-file-row");
        cx.simulate_click(row.center(), Modifiers::none());
        let resolve = wait_for_drawn(&mut cx, "changes-resolve");
        cx.simulate_click(resolve.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            workspace.read_with(&cx.cx, |workspace, app| {
                workspace.tabs.iter().any(|tab| {
                    if tab.kind != TabKind::Terminal {
                        return false;
                    }
                    let mut matches_path = false;
                    tab.panes.for_each(&mut |_, content| {
                        if let TabContent::Terminal { view } = content
                            && let TerminalShell::WithArguments { args, .. } =
                                view.read(app).launch_shell()
                        {
                            matches_path = args.iter().any(|arg| arg.contains("conflicted.txt"));
                        }
                    });
                    matches_path
                })
            }),
            "the conflict action must create a terminal prepared for conflicted.txt"
        );
    }

    #[gpui::test]
    async fn restore_replays_persisted_terminal_scrollback(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::current_dir().expect("current directory");
        let nonce = b"F-PER-01_RESTORE_NONCE\n".to_vec();
        let needle = b"F-PER-01_RESTORE_NONCE";
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![session::SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![session::SessionTabState {
                root_id: Some(0),
                pane_events: Vec::new(),
                scrollback: std::collections::BTreeMap::from([(0, nonce.clone())]),
            }],
            diagnostics: Vec::new(),
        };

        let (mut tabs, _) = cx.update(|cx| restore_tabs(&restored, &working_directory, None, cx));
        let terminal = cx.update(|cx| {
            cx.new(|cx| {
                TerminalView::with_shell(
                    &working_directory,
                    TerminalShell::WithArguments {
                        program: "/bin/sh".into(),
                        args: vec!["-c".into(), "exec sleep 1".into()],
                    },
                    cx,
                )
                .expect("create deterministic restore test terminal")
            })
        });
        tabs[0].panes = PaneNode::leaf(
            0,
            TabContent::Terminal {
                view: terminal.clone(),
            },
        );
        let window = cx.add_window({
            let terminal = terminal.clone();
            move |_window, _cx| TerminalReplayFixture { terminal }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|_, app| replay_persisted_terminal_scrollback(&mut tabs, app));
        let snapshot = terminal.read_with(&cx.cx, |terminal, _| terminal.snapshot());
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
        assert!(
            snapshot
                .scrollback
                .windows(needle.len())
                .any(|window| window == needle),
            "restored terminal must contain the persisted nonce"
        );
    }

    #[gpui::test]
    async fn restore_mounts_a_changes_surface_in_the_application_shell(cx: &mut TestAppContext) {
        let working_directory = std::env::temp_dir();
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![session::SessionTab {
                title: "Changes".into(),
                kind: "diff".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![session::SessionTabState::default()],
            diagnostics: vec![],
        };

        let (tabs, active) = cx.update(|cx| restore_tabs(&restored, &working_directory, None, cx));

        assert_eq!(active, 0, "the restored Changes tab is active");
        assert_eq!(tabs.len(), 1, "the shell retains the Changes tab");
        let mounted = cx.update(|_| {
            let mut mounted = false;
            tabs[0].panes.for_each(&mut |_, content| {
                mounted = matches!(content, TabContent::Changes(_));
            });
            mounted
        });
        assert!(
            mounted,
            "the application shell mounts ChangesTab as pane content"
        );
    }

    #[gpui::test]
    async fn probe_escape_dispatch(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        let focus = palette_test_sidebar_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();

        // Main branch, escape:
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        eprintln!("PROBE main-branch escape done");

        workspace.update(&mut cx, |workspace, cx| workspace.open_settings(None, cx));
        cx.run_until_parked();
        eprintln!(
            "PROBE settings open: {:?}",
            cx.debug_bounds("settings-category-General").is_some()
        );
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        eprintln!(
            "PROBE settings-branch escape done, closed={:?}",
            cx.debug_bounds("settings-category-General").is_none()
        );
    }

    /// F-SID-12: the worktree context menu's primary transitions reach the
    /// shell's catalog through the real click path — Unset Primary flips
    /// the fixture's primary worktree off (drawn menu, real click, typed
    /// event, shell handler, catalog mutation), Set Primary flips it back
    /// on, and the sidebar row follows the catalog both ways.
    #[gpui::test]
    async fn worktree_primary_context_transition_reaches_the_catalog(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let primary = |cx: &mut VisualTestContext| {
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.project_catalog.projects()[0].worktrees[0].is_primary
            })
        };
        assert!(primary(&mut cx), "the fixture worktree starts primary");

        // Unset Primary: the menu offers it because the worktree is primary.
        right_click_sidebar_row(&mut cx);
        let unset = cx
            .debug_bounds("sidebar-context-item-unset-primary")
            .expect("Unset Primary is offered for the primary worktree");
        cx.simulate_click(unset.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !primary(&mut cx),
            "Unset Primary flips the catalog marker off"
        );

        // Set Primary: the menu now offers Set, and it flips back on.
        right_click_sidebar_row(&mut cx);
        let set = cx
            .debug_bounds("sidebar-context-item-set-primary")
            .expect("Set Primary is offered for the unset worktree");
        cx.simulate_click(set.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            primary(&mut cx),
            "Set Primary flips the catalog marker back on"
        );
    }

    /// Right-clicks the fixture's first worktree row (id 1) and parks.
    fn right_click_sidebar_row(cx: &mut VisualTestContext) {
        let bounds = cx
            .debug_bounds("sidebar-row-1")
            .expect("the worktree row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: bounds.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: bounds.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
    }
}

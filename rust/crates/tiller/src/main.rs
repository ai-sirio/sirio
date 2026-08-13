use gpui::{
    AnyElement, App, Bounds, Context, DefiniteLength, DragMoveEvent, Entity, Focusable, FontWeight,
    InteractiveElement, KeyBinding, MouseButton, PathPromptOptions, PromptLevel, Render,
    StatefulInteractiveElement, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div,
    point, prelude::*, px, size,
};
use gpui_platform::application;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiller_activity::{AgentActivityModel, AgentStatus};
use tiller_agents::ALL as AGENT_CATALOG;
use tiller_control::{
    ControlHandler, ControlRequest, ControlResponse, ControlServer, PaneError, PaneExitStatus,
    PaneInfo, PaneRegistry, PaneStateSnapshot, base64_encode,
};
use tiller_git::{GitError, discard, discard_all, init_repository, stage, stage_all, unstage};
use tiller_persistence::{AppSettings, AppearanceMode, FileIconTheme};
use tiller_project::{TabKind, current_branch, is_git_repository};
use tiller_terminal::{
    TerminalContextAction, TerminalContextEvent, TerminalExitStatus, TerminalIdentity,
    TerminalShell, TerminalStateSnapshot, TerminalView,
};
use tiller_theme::{Theme, ThemeMode};
use tiller_ui::{
    changes::{ChangesReport, ChangesTab, ChangesTabEvent},
    chat::Chat,
    file_view::FileView,
    right_panel::{ActivityStatus, ActivitySurface, RightPanel, RightPanelEvent},
    settings::{Settings, SettingsCategory, SettingsReport, SettingsSnapshot},
    sidebar::{
        Sidebar, SidebarContextAction, SidebarContextTarget, SidebarEvent, SidebarProject,
        SidebarTab, SidebarWorktree,
        icons::{Icon, IconElement},
    },
    status_bar::{StatusBar, UsageBarData},
    tab_bar::{NewTabAction, TabBar},
    titlebar::{Titlebar, TitlebarEvent},
};

mod panes;
mod session;
mod tab_machinery;

use panes::{
    CloseOtherTabs, ClosePane, CloseTab, CloseTabsToRight, CycleTabBackward, CycleTabForward,
    FocusPaneAbove, FocusPaneBelow, FocusPaneLeft, FocusPaneRight, JumpToTab1, JumpToTab2,
    JumpToTab3, JumpToTab4, JumpToTab5, JumpToTab6, JumpToTab7, JumpToTab8, JumpToTab9,
    MoveTabEarlier, MoveTabLater, MoveTabToCurrentPane, MoveTabToOtherPane, OpenAllTabs,
    OpenTabMenu, PaneContent as TabContent, PaneNode, ResumeChat, SplitDirection, SplitPaneDown,
    SplitPaneRight, TabSelection,
};
use session::{
    PaneEvent, ProjectCatalog, RestoredSession, SessionLayout, SessionStore, SessionTab,
    SessionTabState,
};
use tab_machinery::{MoveDirection, MoveTarget, TabGroup, TabMachinery};

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
            .collect::<Vec<_>>(),
    );
}

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

type ControlReply = Sender<Result<Vec<(String, String)>, String>>;

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedChat {
    id: usize,
    title: String,
    transcript: String,
}

enum WorkspaceAction {
    NewTab(NewTabAction),
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
            workspaces,
            current,
        }
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
        Self {
            state,
            control_actions,
            panes,
            notifications,
            session_refs,
            session_store,
            socket_info,
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
}

impl ControlHandler for AppControlHandler {
    fn handle(&self, request: &ControlRequest) -> ControlResponse {
        match request.method.as_str() {
            "system.ping" => Self::success(&request.id, [("status".to_string(), "ok".to_string())]),
            "system.capabilities" => {
                let methods = [
                    "system.ping",
                    "system.capabilities",
                    "system.identify",
                    "system.quit",
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
                ];
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
            method if BROWSER_METHODS.contains(&method) => ControlResponse::failure(
                &request.id,
                format!(
                    "{method} is unsupported on Linux: Tiller has no browser/webview surface in this build"
                ),
            ),
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

fn split_direction_name(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Horizontal => "horizontal",
        SplitDirection::Vertical => "vertical",
    }
}

fn parse_split_direction(direction: &str) -> Option<SplitDirection> {
    match direction {
        "horizontal" => Some(SplitDirection::Horizontal),
        "vertical" => Some(SplitDirection::Vertical),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalContextCommand {
    SetTitle,
    Split(SplitDirection),
    Close,
}

fn delegated_terminal_context_action(
    action: TerminalContextAction,
) -> Option<TerminalContextCommand> {
    match action {
        TerminalContextAction::SetTitle => Some(TerminalContextCommand::SetTitle),
        TerminalContextAction::SplitRight => {
            Some(TerminalContextCommand::Split(SplitDirection::Horizontal))
        }
        TerminalContextAction::SplitDown => {
            Some(TerminalContextCommand::Split(SplitDirection::Vertical))
        }
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
    /// The single source of truth for agent lifecycle status. Every one of
    /// the sidebar dot, the tab checkmark, and the Activity row reads
    /// through this (or, for a chat pane, through `Chat`'s own state) —
    /// never a second, independently-tracked flag.
    activity: AgentActivityModel,
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
                    .update(cx, |workspace, cx| {
                        for action in pending {
                            match action {
                                WorkspaceAction::NewTab(action) => {
                                    workspace.open_action(action, cx)
                                }
                                WorkspaceAction::OpenSettings => {
                                    workspace.open_settings(None, cx);
                                }
                                WorkspaceAction::CloseSettings => {
                                    workspace.show_settings = false;
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
                                    workspace.activity.notify(&pane_id, status, Instant::now());
                                    workspace.sync_activity(cx);
                                }
                                ControlAction::SelectWorktree { selector, reply } => {
                                    let result = workspace.control_select_worktree(&selector, cx);
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
                                    let result = workspace.restore_launch_snapshot(cx);
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
            activity: AgentActivityModel::new(),
            show_settings: false,
        };
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
                        TabKind::Browser => {
                            unreachable!("browser tabs are unsupported in shell persistence")
                        }
                        TabKind::Diff => "diff",
                    }
                    .to_string(),
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
                    Some(TerminalContextCommand::Split(direction)) => {
                        workspace.split_terminal_at(tab_id, pane_id, direction, None, cx);
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
                self.open_action(action, cx);
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
                TabContent::File { .. } | TabContent::Changes(_) => None,
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
                    TabContent::File { .. } | TabContent::Changes(_) => (
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
        self.status_bar = cx.new(|_| {
            StatusBar::new(status_data).on_settings(move || {
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

    fn seam(&self) -> impl IntoElement {
        div().w(px(SEAM_WIDTH)).h_full().bg(gpui::black())
    }

    fn tab_width(kind: TabKind) -> f32 {
        match kind {
            TabKind::AgentChat | TabKind::Editor | TabKind::Diff => CHAT_TAB_WIDTH,
            TabKind::Terminal => TERMINAL_TAB_WIDTH,
            TabKind::Browser => unreachable!("Browser surfaces are external to the shell"),
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

    fn add_chat_tab(&mut self, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let chat = cx.new(Chat::launch);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon: None,
            agent_id: None,
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
        cx.notify();
    }

    fn resume_chat(&mut self, retained_id: usize, cx: &mut Context<Self>) {
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
        let pane_id = self.next_pane_id;
        let chat = cx.new(|cx| {
            let mut chat = Chat::launch(cx);
            chat.restore_transcript(&transcript, cx);
            chat
        });
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon: None,
            agent_id: None,
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
        let tab_id = self.next_tab_id;
        let pane_id = self.next_pane_id;
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

    fn add_file_tab(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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
        self.add_terminal_tab_with_shell(adapter.display_name(), shell, Some(agent_icon), cx);
        if let Some(tab) = self.tabs.last_mut() {
            tab.agent_id = Some(adapter.id().to_string());
        }
        self.sync_activity(cx);
    }

    fn open_action(&mut self, action: NewTabAction, cx: &mut Context<Self>) {
        match action {
            NewTabAction::NewChat => self.add_chat_tab("Chat", cx),
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
            // Browser is not part of this shell's content set yet. Keeping the
            // action typed and ignored is preferable to opening a fake pane.
            NewTabAction::NewBrowser => {}
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
        self.open_action(NewTabAction::NewChanges, cx);
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
                        TabContent::File { .. } | TabContent::Changes(_) => None,
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
            let split = tab.panes.split_focused(
                focused_pane,
                pane_id,
                direction,
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
                    direction: split_direction_name(direction).to_string(),
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
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        let pane_name = format!("pane-{pane_id}");
        if let Err(error) = adapter.prepare(&worktree_path, &pane_name, "tillerctl") {
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
                adapter.command(&worktree_path, &pane_name, "tillerctl"),
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
                            TabContent::File { .. } | TabContent::Changes(_) => None,
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
                let entity_for_click = entity.clone();
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
                };
                div()
                    .id(format!("pane-{pane_id}"))
                    .relative()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        entity_for_click.update(cx, |workspace, cx| {
                            workspace.select_pane(pane_id, Some(window), cx)
                        });
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
                                .on_drag(divider_drag.clone(), |_, _, _, cx| {
                                    cx.new(|_| gpui::Empty)
                                }),
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
                gpui::rgb(0xca7250)
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
            .child(
                div()
                    .font_weight(FontWeight::NORMAL)
                    .child(tab.title.clone()),
            )
            .child(
                div()
                    .w(px(16.0))
                    .when(status == Some(ActivityStatus::Done), |this| {
                        this.text_color(theme.tab_done).child("✓")
                    }),
            )
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
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            close_entity.update(cx, |this, cx| {
                                this.request_close_tab_by_id(id, window, cx)
                            });
                        })
                        .child(IconElement::new(Icon::Close, px(12.0)).text_color(theme.subtitle)),
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
                TabContent::Changes(_) => false,
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
        let active_group = self.tab_command_machinery().active_group();
        for (index, tab) in self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.group_id == active_group)
        {
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
                        .child(self.render_open_tabs(*theme, entity.clone(), cx)),
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
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_action(NewTabAction::NewTerminal, cx);
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
                    eprintln!("[files] could not open the file picker: {error}");
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
                eprintln!("[files] save failed: {error}");
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

    fn handle_resume_chat(&mut self, _: &ResumeChat, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.retained_chats.first().map(|chat| chat.id) {
            self.resume_chat(id, cx);
        }
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
            .on_action(cx.listener(Self::handle_new_terminal_tab))
            .on_action(cx.listener(Self::handle_open_file))
            .on_action(cx.listener(Self::handle_save_file))
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
                let Some(direction) = parse_split_direction(direction) else {
                    continue;
                };
                if tree.contains(*focused) {
                    let _ = tree.split_focused(*focused, *new_id, direction, new_content(*new_id));
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
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(Chat::launch)),
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
        let persisted_scrollback = &tab_state.scrollback;
        panes.for_each(&mut |pane_id, content| {
            if let TabContent::Terminal { view } = content
                && let Some(bytes) = persisted_scrollback.get(&pane_id)
            {
                view.update(cx, |terminal, _| terminal.replay_scrollback(bytes));
            }
        });
        tabs.push(OpenTab {
            id,
            group_id: 0,
            title: tab.title.clone(),
            kind: match tab.kind.as_str() {
                "chat" => TabKind::AgentChat,
                "diff" => TabKind::Diff,
                _ => TabKind::Terminal,
            },
            agent_icon: None,
            agent_id: None,
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
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(Chat::launch)),
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
        let persisted_scrollback = &tab_state.scrollback;
        panes.for_each(&mut |pane_id, content| {
            if let TabContent::Terminal { view } = content
                && let Some(bytes) = persisted_scrollback.get(&pane_id)
            {
                view.update(cx, |terminal, _| terminal.replay_scrollback(bytes));
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
            } else {
                TabKind::Terminal
            },
            agent_icon: None,
            agent_id: None,
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

fn settings_snapshot_from_app_settings(settings: AppSettings) -> SettingsSnapshot {
    SettingsSnapshot {
        theme: match settings.appearance {
            AppearanceMode::System => ThemeMode::System,
            AppearanceMode::Light => ThemeMode::Light,
            AppearanceMode::Dark => ThemeMode::Dark,
        },
        interface_font_size: settings.ui_font_size as i32,
        terminal_font_size: settings.terminal_font_size as i32,
        file_icons: match settings.file_icon_theme {
            FileIconTheme::SfSymbols => tiller_ui::settings::FileIconChoice::SfSymbols,
            FileIconTheme::Material => tiller_ui::settings::FileIconChoice::Material,
        },
        control_socket_enabled: settings.control_socket_enabled,
        socket_path: String::new(),
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
    }
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
        // Restore is tolerant of old path-based project ids; rewrite the
        // canonical catalog immediately so every later layout save sees one
        // project/worktree id convention.
        session_store.schedule_catalog(&project_catalog);
        let saved_settings = session_store.load_settings();
        Theme::set_mode(
            settings_snapshot_from_app_settings(saved_settings).theme,
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
        let control_socket_for_settings = control_socket.clone();
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
                    Settings::with_snapshot(cx, settings_snapshot)
                        .on_change(move |snapshot| {
                            control_socket_for_settings
                                .set_enabled(snapshot.control_socket_enabled);
                            session_store_for_settings
                                .save_settings(&app_settings_from_snapshot(snapshot));
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
    use gpui::{FocusHandle, Render, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;
    use tiller_persistence::{AppSettings, AppearanceMode, FileIconTheme};

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
            Some(TerminalContextCommand::Split(SplitDirection::Horizontal))
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::SplitDown),
            Some(TerminalContextCommand::Split(SplitDirection::Vertical))
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::CloseTerminal),
            Some(TerminalContextCommand::Close)
        );
        assert_eq!(
            delegated_terminal_context_action(TerminalContextAction::Copy),
            None
        );
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
    fn persisted_settings_map_to_the_ui_snapshot_and_back() {
        let persisted = AppSettings {
            appearance: AppearanceMode::Dark,
            ui_font_size: 17,
            terminal_font_size: 19,
            file_icon_theme: FileIconTheme::Material,
            control_socket_enabled: false,
        };

        let snapshot = settings_snapshot_from_app_settings(persisted);
        assert_eq!(snapshot.theme, tiller_theme::ThemeMode::Dark);
        assert_eq!(snapshot.interface_font_size, 17);
        assert_eq!(snapshot.terminal_font_size, 19);
        assert_eq!(
            snapshot.file_icons,
            tiller_ui::settings::FileIconChoice::Material
        );
        assert!(!snapshot.control_socket_enabled);
        assert_eq!(app_settings_from_snapshot(snapshot), persisted);
    }

    #[test]
    fn restoring_launch_snapshot_adds_missing_tabs_without_replacing_current_tabs() {
        let snapshot = vec![
            SessionTab {
                title: "Chat".into(),
                kind: "chat".into(),
                active: false,
            },
            SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                active: true,
            },
        ];
        let current = vec![SessionTab {
            title: "Changes".into(),
            kind: "diff".into(),
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
    fn raw_notify_accepts_user_title_and_body() {
        let state = Arc::new(Mutex::new(ControlState {
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
    fn browser_methods_return_specific_linux_unsupported_errors_and_are_not_advertised() {
        let state = Arc::new(Mutex::new(ControlState {
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
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-browser-test.sock")),
        );

        for method in BROWSER_METHODS {
            let response = handler.handle(&ControlRequest {
                id: method.to_string(),
                method: method.to_string(),
                params: BTreeMap::new(),
            });
            assert!(!response.ok, "{method} unexpectedly succeeded");
            let error = response.error.expect("unsupported error");
            assert!(error.contains("unsupported on Linux"), "{error}");
        }

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
        for method in [
            "pane.split",
            "pane.focus",
            "pane.close",
            "tab.cycle",
            "tab.select",
        ] {
            assert!(
                methods
                    .iter()
                    .any(|row| row.get("method").map(String::as_str) == Some(method)),
                "missing capability {method}"
            );
        }
        assert!(methods.iter().all(|row| {
            row.get("method")
                .is_none_or(|method| !BROWSER_METHODS.contains(&method.as_str()))
        }));
    }

    #[test]
    fn surface_methods_are_advertised_by_capabilities() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
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
    fn changes_mutations_validate_path_and_worktree_before_git() {
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
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
                    active: false,
                },
                session::SessionTab {
                    title: "Terminal".into(),
                    kind: "terminal".into(),
                    active: true,
                },
            ],
            tab_states: vec![
                session::SessionTabState::default(),
                session::SessionTabState::default(),
            ],
            diagnostics: vec![],
        };

        let (tabs, active) = cx.update(|cx| restore_tabs(&restored, &working_directory, cx));

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
    async fn restore_replays_persisted_terminal_scrollback(cx: &mut TestAppContext) {
        let working_directory = std::env::current_dir().expect("current directory");
        let nonce = b"F-PER-01_RESTORE_NONCE\n".to_vec();
        let needle = b"F-PER-01_RESTORE_NONCE";
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![session::SessionTab {
                title: "Terminal".into(),
                kind: "terminal".into(),
                active: true,
            }],
            tab_states: vec![session::SessionTabState {
                root_id: Some(0),
                pane_events: Vec::new(),
                scrollback: std::collections::BTreeMap::from([(0, nonce.clone())]),
            }],
            diagnostics: Vec::new(),
        };

        let (tabs, _) = cx.update(|cx| restore_tabs(&restored, &working_directory, cx));
        let snapshot = cx.update(|cx| {
            let mut snapshot = None;
            tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Terminal { view } = content {
                    snapshot = Some(view.read(cx).snapshot());
                }
            });
            snapshot.expect("restored terminal pane")
        });
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
                active: true,
            }],
            tab_states: vec![session::SessionTabState::default()],
            diagnostics: vec![],
        };

        let (tabs, active) = cx.update(|cx| restore_tabs(&restored, &working_directory, cx));

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
}

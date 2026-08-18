use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, DefiniteLength, DragMoveEvent, Entity,
    FocusHandle, Focusable, FontWeight, InteractiveElement, KeyBinding, KeyDownEvent, MouseButton,
    PathPromptOptions, PromptLevel, Render, StatefulInteractiveElement, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, deferred, div, point, prelude::*, px, size,
};
use gpui_platform::application;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiller_acp::AgentCommand;
use tiller_activity::{
    AgentActivityModel, AgentSessionRef, AgentSessionRestorePlan, AgentStatus,
    BootstrapRestoreOrder, NotificationPayload, NotificationPolicy, TerminalContentId, Transition,
    WorktreeMountPolicy,
};
use tiller_agents::ALL as AGENT_CATALOG;
use tiller_control::{
    ControlHandler, ControlRequest, ControlResponse, ControlServer, PaneError, PaneExitStatus,
    PaneInfo, PaneRegistry, PaneStateSnapshot, base64_encode,
};
use tiller_git::{
    GitBranches, GitError, discard, discard_all, init_repository, stage, stage_all, unstage,
};
use tiller_persistence::{AppDatabase, AppSettings, AppearanceMode, FileIconTheme};
use tiller_project::{TabKind, UpdateEvent, UpdateState, current_branch, is_git_repository};
use tiller_terminal::{
    TerminalActivityEvent, TerminalContextAction, TerminalContextEvent, TerminalExitStatus,
    TerminalIdentity, TerminalLinkEvent, TerminalPaneCache, TerminalPromptAction,
    TerminalPromptEvent, TerminalShell, TerminalStateSnapshot, TerminalView,
};
use tiller_theme::{AgentBrandColor, Theme, ThemeMode};
use tiller_ui::{
    browser::{BrowserEvent, BrowserSurface, normalize_address},
    changes::{ChangesReport, ChangesTab, ChangesTabActionEvent, ChangesTabEvent},
    chat::{Chat, ChatControlSnapshot, ChatEvent, acp_agent_command},
    file_view::{FileView, FileViewEvent},
    right_panel::{
        ActivityStatus, ActivitySurface, RightPanel, RightPanelActionEvent, RightPanelEvent,
    },
    row_reorder::{ReorderScope, RowDrag},
    settings::{Settings, SettingsCategory, SettingsReport, SettingsSnapshot},
    sidebar::{
        AgentMark, ProjectSettingsUpdate, Sidebar, SidebarContextAction, SidebarContextTarget,
        SidebarEvent, SidebarProject, SidebarTab, SidebarWorktree, TAB_ROW_ID_OFFSET,
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
mod tray;

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
    CatalogProjectSettings, PaneEvent, ProjectCatalog, RestoredSession, SessionLayout,
    SessionStore, SessionTab, SessionTabState,
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
        RestoreLaunchSnapshot,
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
    RestoreLaunchSnapshot,
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

fn linux_window_shortcuts() -> [(WindowCommand, &'static str); 6] {
    [
        (WindowCommand::NewTerminalTab, "ctrl-t"),
        (WindowCommand::OpenFile, "ctrl-o"),
        (WindowCommand::SaveFile, "ctrl-s"),
        (WindowCommand::ToggleSidebar, "ctrl-shift-s"),
        (WindowCommand::ToggleRightPanel, "ctrl-shift-i"),
        // F-WIN-07: Linux stand-in for macOS's `⇧⌘O` "History > Restore
        // Previous Launch" chord.
        (WindowCommand::RestoreLaunchSnapshot, "ctrl-shift-o"),
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
        | WindowCommand::ToggleRightPanel
        | WindowCommand::RestoreLaunchSnapshot => WindowCommandAvailability::Enabled,
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
                WindowCommand::RestoreLaunchSnapshot => {
                    KeyBinding::new(shortcut, RestoreLaunchSnapshot, None)
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
const BROWSER_METHODS: [&str; 11] = [
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
    "browser.permission",
];
// F-CTRL-BROWSER-03/04/05/06/F-PER-08: browser.get, browser.wait,
// browser.eval, browser.console, browser.snapshot and browser.permission
// are real, implemented methods (see handle_browser_action) — they must not
// be turned away here as "not implemented" before ever reaching that
// dispatch.
//
// browser.screenshot stays out of this list on purpose: wry 0.56's public
// WebView API (crates/tiller_ui/src/browser.rs's `build_webview`/
// `build_production_webview`) has no pixel-capture entry point on the
// webkitgtk backend, and there is no other dependency in this workspace
// that reaches WebKitGTK's own `webkit_web_view_get_snapshot`. A true
// screenshot needs a new `webkit2gtk` FFI dependency plus a GAsyncResult
// callback bridged back into GPUI — out of this row's file list. See
// F-CTRL-BROWSER-04's row in the wave report for the sketch.
const BROWSER_CAPABILITIES: [&str; 9] = [
    "browser.open",
    "browser.navigate",
    "browser.act",
    "browser.get",
    "browser.wait",
    "browser.eval",
    "browser.console",
    "browser.snapshot",
    "browser.permission",
];

// F-CTRL-BROWSER-04: a lightweight structural snapshot — role, accessible
// name and bounding box for interactive/labelled elements — serialized as
// JSON text, the same shape browser.console already returns for console
// messages. Deliberately not a full DOM dump: an agent driving the page
// needs "what can I click and what does it say", not the raw markup.
const BROWSER_SNAPSHOT_SCRIPT: &str = r#"JSON.stringify((() => {
    const nodes = document.querySelectorAll(
        "a,button,input,select,textarea,[role],[aria-label],h1,h2,h3"
    );
    const seen = [];
    nodes.forEach((node) => {
        const rect = node.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return;
        seen.push({
            tag: node.tagName.toLowerCase(),
            role: node.getAttribute("role") || node.tagName.toLowerCase(),
            name: (node.getAttribute("aria-label") || node.innerText || node.value || "")
                .trim()
                .slice(0, 200),
            x: Math.round(rect.x),
            y: Math.round(rect.y),
            width: Math.round(rect.width),
            height: Math.round(rect.height),
        });
    });
    return { title: document.title, url: location.href, elements: seen.slice(0, 500) };
})())"#;

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
        "browser.eval"
            if params
                .get("script")
                .or_else(|| params.get("expression"))
                .is_none_or(|script| script.trim().is_empty()) =>
        {
            Some("browser.eval requires a non-empty script".to_string())
        }
        "browser.act" if params.contains_key("driving") || params.contains_key("agentDriving") => {
            None
        }
        // F-CTRL-BROWSER-05: click/fill/type/press/scroll run as JS through
        // BrowserSurface::evaluate_script (added for F-CTRL-BROWSER-06).
        // Validating with the exact same script-building function used at
        // dispatch time means a request missing a verb's required argument
        // fails synchronously here, before ever queuing a ControlAction
        // that needs a live browser surface to answer.
        "browser.act" => {
            let verb = params.get("verb").map(String::as_str).unwrap_or_default();
            let selector = params.get("selector").map(String::as_str);
            let text = params
                .get("text")
                .or_else(|| params.get("value"))
                .map(String::as_str);
            browser_act_script(verb, selector, text).err()
        }
        _ => None,
    }
}

/// F-CTRL-BROWSER-05: builds the JS snippet `browser.act`'s verb runs
/// through `BrowserSurface::evaluate_script` (the same public method
/// F-CTRL-BROWSER-06's `browser.eval`/`browser.console` already use).
/// `selector`/`text` are serialized through `serde_json::to_string` so they
/// land as safe, correctly-escaped JS string literals regardless of what
/// the caller passes — never string-concatenated raw.
fn browser_act_non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn browser_act_script(
    verb: &str,
    selector: Option<&str>,
    text: Option<&str>,
) -> Result<String, String> {
    let non_empty = browser_act_non_empty;
    let selector_literal = |field: &str| -> Result<String, String> {
        let selector = non_empty(selector)
            .ok_or_else(|| format!("browser.act verb '{verb}' requires a non-empty {field}"))?;
        serde_json::to_string(selector).map_err(|error| error.to_string())
    };
    match verb {
        "click" => {
            let selector = selector_literal("selector")?;
            Ok(format!(
                "(() => {{ const el = document.querySelector({selector}); \
                 if (!el) throw new Error('browser.act click: no element matches selector'); \
                 el.click(); return true; }})()"
            ))
        }
        "fill" | "type" => {
            let selector = selector_literal("selector")?;
            let text_literal =
                serde_json::to_string(text.unwrap_or("")).map_err(|error| error.to_string())?;
            Ok(format!(
                "(() => {{ const el = document.querySelector({selector}); \
                 if (!el) throw new Error('browser.act {verb}: no element matches selector'); \
                 el.value = {text_literal}; \
                 el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                 return true; }})()"
            ))
        }
        "press" => {
            let key = non_empty(text).ok_or_else(|| {
                "browser.act verb 'press' requires a non-empty text (the key name)".to_string()
            })?;
            let key_literal = serde_json::to_string(key).map_err(|error| error.to_string())?;
            let target = match non_empty(selector) {
                Some(selector) => {
                    let selector_literal =
                        serde_json::to_string(selector).map_err(|error| error.to_string())?;
                    format!(
                        "document.querySelector({selector_literal}) || document.activeElement || document.body"
                    )
                }
                None => "document.activeElement || document.body".to_string(),
            };
            Ok(format!(
                "(() => {{ const el = {target}; \
                 if (!el) throw new Error('browser.act press: no target element'); \
                 el.dispatchEvent(new KeyboardEvent('keydown', {{ key: {key_literal}, bubbles: true }})); \
                 el.dispatchEvent(new KeyboardEvent('keyup', {{ key: {key_literal}, bubbles: true }})); \
                 return true; }})()"
            ))
        }
        "scroll" => {
            if let Some(selector) = non_empty(selector) {
                let selector_literal =
                    serde_json::to_string(selector).map_err(|error| error.to_string())?;
                Ok(format!(
                    "(() => {{ const el = document.querySelector({selector_literal}); \
                     if (!el) throw new Error('browser.act scroll: no element matches selector'); \
                     el.scrollIntoView({{ block: 'center' }}); return true; }})()"
                ))
            } else {
                let dy = non_empty(text)
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(400);
                Ok(format!(
                    "(() => {{ window.scrollBy(0, {dy}); return true; }})()"
                ))
            }
        }
        "" => Err(
            "browser.act requires a driving flag or a verb (click, fill, type, press, scroll)"
                .to_string(),
        ),
        other => Err(format!(
            "browser.act verb '{other}' is unsupported (known verbs: click, fill, type, press, scroll)"
        )),
    }
}

/// Extracts the `scheme://host[:port]` origin from a normalized
/// `http(s)://host[:port]/...` address, matching the shape stored in
/// `allowed_origins`/`browser_origin_grant`. Returns `None` for a malformed
/// address (already ruled out by `normalize_address` for real callers).
fn browser_origin_of(address: &str) -> Option<String> {
    let (scheme, rest) = address.split_once("://")?;
    let host_port = rest.split('/').next().unwrap_or_default();
    if host_port.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host_port}"))
}

/// F-BRW-04: bounded, synchronous reachability probe for a normalized
/// `http(s)://host[:port]/...` address. `normalize_address` only checks
/// syntax, so `browser.navigate` to a well-formed but dead host (DNS
/// failure, refused connection) otherwise returns `ok:true` with a blank
/// title and no error. Runs the actual resolve+connect on a background
/// thread with a hard timeout so a single navigation attempt can never hang
/// the control socket indefinitely; a probe that neither succeeds nor fails
/// within the timeout is treated as unreachable rather than blocking.
fn probe_host_reachable(address: &str) -> Result<(), String> {
    let Some(rest) = address
        .strip_prefix("https://")
        .map(|rest| (rest, 443u16))
        .or_else(|| address.strip_prefix("http://").map(|rest| (rest, 80u16)))
    else {
        return Ok(());
    };
    let (host_and_port, default_port) = rest;
    let host_port = host_and_port.split('/').next().unwrap_or_default();
    let target = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:{default_port}")
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let probe_target = target.clone();
    std::thread::spawn(move || {
        use std::net::{TcpStream, ToSocketAddrs};
        let result = probe_target
            .to_socket_addrs()
            .map_err(|error| format!("Could not resolve {probe_target}: {error}"))
            .and_then(|mut addrs| {
                let addr = addrs.next().ok_or_else(|| {
                    format!("Could not resolve {probe_target}: no addresses found")
                })?;
                TcpStream::connect_timeout(&addr, Duration::from_millis(1200))
                    .map(|_| ())
                    .map_err(|error| format!("Could not reach {probe_target}: {error}"))
            });
        // The receiver may already be gone if the caller's own timeout won.
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_millis(1500)) {
        Ok(result) => result,
        Err(_) => Err(format!("Could not reach {target}: timed out")),
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
    /// F-WIN-11: drives the update-toast state machine from outside the
    /// process -- the cheapest honest test source, since no Linux update
    /// transport exists yet to raise these events on its own (see
    /// `tiller_project::ui`'s doc-comment on `UpdateState`).
    UpdateEvent(UpdateEvent),
    SelectWorktree {
        selector: String,
        reply: ControlReply,
    },
    /// I3-tray-jump: drives `select_worktree_and_jump` -- the same
    /// select-then-jump-to-worst-status-tab sequence a real tray roster
    /// click makes -- so the behaviour behind `F-USE-05`'s tray jump is
    /// observable over the control socket without a live D-Bus
    /// `StatusNotifierWatcher` session.
    TrayJump {
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
        placement: SplitPlacement,
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
    Chat {
        action: ChatControlAction,
        reply: ControlReply,
    },
    /// F-SID-11: `worktree.set`'s comment mutates the shared `ControlState`
    /// directly from the control-server thread, which the sidebar entity
    /// never observes on its own — nothing re-renders it without this
    /// action nudging the GPUI-thread poll loop to call `refresh_sidebar`.
    RefreshSidebar,
}

enum ChatControlAction {
    Open {
        worktree: Option<String>,
    },
    Send {
        surface_id: String,
        text: String,
    },
    Compose {
        surface_id: String,
        text: String,
    },
    Permission {
        surface_id: String,
        request_id: u64,
        option_id: String,
    },
    Stop {
        surface_id: String,
    },
    Read {
        surface_id: String,
    },
}
struct OpenTab {
    id: usize,
    /// Stable database identity, independent of the tab's visible position.
    persistence_id: String,
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
    /// F-CORE-DOM-07: mirrors Swift's `tab.titleIsAutoNamed` — true until a
    /// user-driven rename (`commit_tab_rename`) turns it off, which is the
    /// only thing that permanently opts a tab out of automatic renaming.
    title_is_auto_named: bool,
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
    /// F-SET-18: an agent row's Install button was clicked. `command` is
    /// [`tiller_agents::AgentAvailability::install_command`]'s documented
    /// shell line for `agent_id`.
    InstallAgent {
        agent_id: &'static str,
        command: &'static str,
    },
    OpenSettings,
    /// F-TAB-08: clicking the New Chat menu's "Other agents…" empty-state
    /// card (drawn when no supported agent is on PATH) opens Settings
    /// straight to the Agents section instead of the general default.
    OpenAgentSettings,
    CloseSettings,
    /// F-BRW-09: a plain (non-Cmd+Shift) click on an HTTP(S) link in chat
    /// opens Tiller's internal browser tab instead of the system browser.
    OpenBrowserLink(String),
    /// F-WIN-07: the titlebar's History entry point (and the `ctrl-shift-o`
    /// chord) both funnel here -- re-invoke the same `session.restore`
    /// control-door path `ControlAction::RestoreSession` already drives.
    RestoreLaunchSnapshot,
    /// F-TERM-02: the empty-pane prompt's "New…" action -- the Linux
    /// equivalent of the Swift reference's `Menu("New…") { newTabMenu() }`,
    /// which shares the same chooser the tab strip's own "+" control opens.
    /// `TerminalPromptEvent` fires from a `cx.subscribe` callback with no
    /// `Window`, so opening it (which needs one, to move focus into the
    /// query field) is deferred through this queue like every other
    /// Window-needing follow-up from a non-Window context.
    OpenNewTabPalette,
}

#[derive(Clone)]
struct ControlWorkspace {
    id: String,
    project: String,
    branch: String,
    path: String,
    selected: bool,
    mounted: bool,
    /// Annotation from `worktree.set`. F-CTRL-WORK-01: persisted to the
    /// `worktree.comment` column (via `AppControlHandler::persist_worktree_comment`)
    /// and reloaded by `apply_persisted_comments` at startup, so it survives
    /// a restart.
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
                    // Provisional -- BootstrapRestoreOrder below decides the
                    // real first-paint mount set; only the selected worktree
                    // (the sole "previously open" id a single-directory
                    // launch snapshot can recover) is treated as priority.
                    mounted: true,
                    comment: String::new(),
                    session: None,
                });
            }
        }
        // F-CORE-ACT-25: mount only the priority set at first paint --
        // today that is just the selected worktree, since the launch
        // snapshot persists a single `working_directory`, not a list of
        // previously open worktree ids. Every other worktree starts
        // deferred (unmounted) rather than eagerly mounted, and picks up a
        // real mount the first time it is selected.
        let selected_path = workspaces
            .iter()
            .find(|workspace| workspace.selected)
            .map(|workspace| workspace.path.clone());
        let open_ids: Vec<String> = selected_path.iter().cloned().collect();
        let restore = BootstrapRestoreOrder::partition(
            &workspaces,
            &open_ids,
            selected_path.as_ref(),
            |workspace| workspace.path.clone(),
        );
        let priority_paths: HashSet<String> = restore
            .priority
            .iter()
            .map(|workspace| workspace.path.clone())
            .collect();
        for workspace in &mut workspaces {
            workspace.mounted = priority_paths.contains(&workspace.path);
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

    /// F-CTRL-WORK-01: seeds `comment` from the durable `worktree.comment`
    /// column so a value set via `worktree.set` before the previous
    /// shutdown survives a restart. `from_catalog` itself stays
    /// database-free (it also builds throwaway states in tests), so this
    /// runs as a separate step at real startup, after the database has been
    /// opened.
    fn apply_persisted_comments(&mut self, comments: &BTreeMap<String, String>) {
        for workspace in &mut self.workspaces {
            if let Some(comment) = comments.get(&workspace.path) {
                workspace.comment = comment.clone();
            }
        }
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
    notification_poster: Arc<dyn Fn(&NotificationPayload) + Send + Sync>,
    session_refs: Arc<Mutex<BTreeMap<String, String>>>,
    session_store: Option<SessionStore>,
    socket_info: ControlSocketInfo,
    /// F-CTRL-WORK-01: set at real startup so `worktree.set`'s comment can
    /// be persisted immediately (a hook write, like `save_session_ref`, not
    /// something worth waiting on the layout debounce for). `None` in most
    /// tests, which matches the pre-existing "runtime only" behavior for
    /// them.
    database_path: Option<PathBuf>,
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
            notification_poster: Arc::new(post_desktop_notification),
            session_refs,
            session_store,
            socket_info,
            database_path: None,
        }
    }

    /// F-CTRL-WORK-01: wires the database path used to persist
    /// `worktree.set`'s comment. Real startup calls this; tests that don't
    /// exercise persistence leave it unset.
    fn with_database_path(mut self, database_path: PathBuf) -> Self {
        self.database_path = Some(database_path);
        self
    }

    /// Upserts the durable `worktree.comment` column for `path`. Best
    /// effort: a missing row (the worktree hasn't been through
    /// `schedule_catalog` yet) or a database error is logged and otherwise
    /// ignored, matching the rest of this handler's persistence calls.
    fn persist_worktree_comment(&self, path: &str, comment: &str) {
        let Some(database_path) = &self.database_path else {
            return;
        };
        let database = match AppDatabase::open(database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!(
                    "[control] cannot open {} ({error})",
                    database_path.display()
                );
                return;
            }
        };
        let existing = match database.worktree_by_path(path) {
            Ok(existing) => existing,
            Err(error) => {
                eprintln!("[control] failed to read worktree row for {path}: {error}");
                return;
            }
        };
        let Some(mut record) = existing else {
            eprintln!("[control] no persisted worktree row for {path}; comment not saved");
            return;
        };
        record.comment = Some(comment.to_string());
        if let Err(error) = database.save_worktree(&record) {
            eprintln!("[control] failed to persist comment for {path}: {error}");
        }
    }

    fn success(id: &str, pairs: impl IntoIterator<Item = (String, String)>) -> ControlResponse {
        ControlResponse::success(id, pairs.into_iter().collect())
    }

    #[cfg(test)]
    fn with_notification_poster(
        mut self,
        poster: impl Fn(&NotificationPayload) + Send + Sync + 'static,
    ) -> Self {
        self.notification_poster = Arc::new(poster);
        self
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
        drop(notifications);
        (self.notification_poster)(&NotificationPayload {
            pane_id: String::new(),
            worktree_id: String::new(),
            title: title.to_string(),
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
        // P126: the dispatch-level wait must never be shorter than a caller-
        // supplied `timeoutMs` (e.g. browser.wait), or the request's own
        // timeout is silently truncated by this outer bound and the caller
        // sees "control action timed out" — a message that reads as "the
        // awaited condition never happened" when the real cause is "the
        // dispatch bound fired first". Derive the dispatch bound from the
        // request's own timeout, plus a margin for the worker to notice its
        // deadline and reply, so the two bounds can never disagree.
        const DISPATCH_MARGIN: Duration = Duration::from_secs(2);
        let dispatch_timeout = request
            .params
            .get("timeoutMs")
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .map(|requested| (requested + DISPATCH_MARGIN).max(CONTROL_ACTION_TIMEOUT))
            .unwrap_or(CONTROL_ACTION_TIMEOUT);

        let (reply, result) = mpsc::channel();
        let Ok(mut actions) = self.control_actions.lock() else {
            return ControlResponse::failure(&request.id, "control action queue unavailable");
        };
        actions.push(action(reply));
        drop(actions);

        match result.recv_timeout(dispatch_timeout) {
            Ok(Ok(pairs)) => Self::success(&request.id, pairs),
            Ok(Err(error)) => ControlResponse::failure(&request.id, error),
            Err(mpsc::RecvTimeoutError::Timeout) => ControlResponse::failure(
                &request.id,
                format!(
                    "control action dispatch bound ({:.1}s) fired before the worker replied; \
                     this means the dispatcher itself stalled, not that the awaited condition \
                     never occurred",
                    dispatch_timeout.as_secs_f64()
                ),
            ),
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
                    "tray.jump",
                    "workspace.current",
                    "workspace.close",
                    "worktree.set",
                    "notify",
                    "update.event",
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
                    "session.transcript",
                    "surface.changes.open",
                    "surface.changes.read",
                    "surface.changes.stage",
                    "surface.changes.unstage",
                    "surface.changes.discard",
                    "surface.changes.stage_all",
                    "surface.changes.discard_all",
                    "git.branches",
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
            // F-GIT-BRANCH-01: GitBranches::list has no UI caller (the New
            // Worktree prompt is free-text with no read-back), so this
            // socket door is the exercisable route the row's own VERIFY
            // asks for — list them through the Git layer, exact names
            // preserved including internal spaces.
            "git.branches" => {
                let repo = match self.changes_worktree(request) {
                    Ok(repo) => repo,
                    Err(error) => return ControlResponse::failure(&request.id, error),
                };
                match GitBranches::list(&repo) {
                    Ok(branches) => Self::success(
                        &request.id,
                        [(
                            "branches".to_string(),
                            tiller_control::protocol::rows::encode(
                                &branches
                                    .into_iter()
                                    .map(|name| BTreeMap::from([("name".to_string(), name)]))
                                    .collect::<Vec<_>>(),
                            ),
                        )],
                    ),
                    Err(error) => ControlResponse::failure(&request.id, error.to_string()),
                }
            }
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
            "surface.chat.open" => {
                let worktree = request.params.get("worktree").cloned();
                self.queue_action(request, move |reply| ControlAction::Chat {
                    action: ChatControlAction::Open { worktree },
                    reply,
                })
            }
            "surface.chat.send" | "surface.chat.compose" => {
                let Some(surface_id) = request.params.get("surfaceId").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        format!("{} requires surfaceId", request.method),
                    );
                };
                let Some(text) = request.params.get("text").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        format!("{} requires text", request.method),
                    );
                };
                let action = if request.method == "surface.chat.send" {
                    ChatControlAction::Send { surface_id, text }
                } else {
                    ChatControlAction::Compose { surface_id, text }
                };
                self.queue_action(request, move |reply| ControlAction::Chat { action, reply })
            }
            "surface.chat.permission" => {
                let Some(surface_id) = request.params.get("surfaceId").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "surface.chat.permission requires surfaceId",
                    );
                };
                let Some(request_id) = request.params.get("requestId") else {
                    return ControlResponse::failure(
                        &request.id,
                        "surface.chat.permission requires requestId",
                    );
                };
                let request_id = match request_id.parse::<u64>() {
                    Ok(request_id) => request_id,
                    Err(error) => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("invalid requestId: {error}"),
                        );
                    }
                };
                let Some(option_id) = request.params.get("optionId").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "surface.chat.permission requires optionId",
                    );
                };
                self.queue_action(request, move |reply| ControlAction::Chat {
                    action: ChatControlAction::Permission {
                        surface_id,
                        request_id,
                        option_id,
                    },
                    reply,
                })
            }
            "surface.chat.stop" | "surface.chat.read" => {
                let Some(surface_id) = request.params.get("surfaceId").cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        format!("{} requires surfaceId", request.method),
                    );
                };
                let action = if request.method == "surface.chat.stop" {
                    ChatControlAction::Stop { surface_id }
                } else {
                    ChatControlAction::Read { surface_id }
                };
                self.queue_action(request, move |reply| ControlAction::Chat { action, reply })
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
                let (direction, placement) = match direction.as_str() {
                    "right" => (SplitDirection::Horizontal, SplitPlacement::After),
                    "left" => (SplitDirection::Horizontal, SplitPlacement::Before),
                    "down" => (SplitDirection::Vertical, SplitPlacement::After),
                    "up" => (SplitDirection::Vertical, SplitPlacement::Before),
                    _ => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("unknown pane split direction: {direction}"),
                        );
                    }
                };
                self.queue_action(request, move |reply| ControlAction::SplitPane {
                    direction,
                    placement,
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
            // F-AGENT-SESSION-02: reads a native agent's own on-disk
            // transcript for a session reference this socket already
            // recorded via `session.ref`/`notify`'s `agentSession`, using
            // `ClaudeTranscriptSource`/`CodexTranscriptSource` directly —
            // the agent identity comes from the caller's explicit `agent`
            // param rather than `panel.list`'s `agent` field, which is not
            // a reliable read of live activity state.
            "session.transcript" => {
                let Some(pane_id) = request.params.get("session") else {
                    return ControlResponse::failure(
                        &request.id,
                        "session.transcript requires session",
                    );
                };
                let Some(agent_id) = request.params.get("agent") else {
                    return ControlResponse::failure(
                        &request.id,
                        "session.transcript requires agent",
                    );
                };
                let Some(worktree_path) = request.params.get("worktree") else {
                    return ControlResponse::failure(
                        &request.id,
                        "session.transcript requires worktree",
                    );
                };
                let Ok(references) = self.session_refs.lock() else {
                    return ControlResponse::failure(&request.id, "session store unavailable");
                };
                let Some(session_ref) = references.get(pane_id).cloned() else {
                    return ControlResponse::failure(
                        &request.id,
                        "no session reference recorded for this pane",
                    );
                };
                drop(references);
                let home_directory = PathBuf::from(std::env::var("HOME").unwrap_or_default());
                let text = match agent_id.as_str() {
                    "claude" => tiller_agents::ClaudeTranscriptSource::new(
                        worktree_path.clone(),
                        session_ref,
                        home_directory,
                    )
                    .recent_text(),
                    "codex" => {
                        tiller_agents::CodexTranscriptSource::new(session_ref, home_directory)
                            .recent_text()
                    }
                    other => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("session.transcript has no reader for agent '{other}'"),
                        );
                    }
                };
                let Some(text) = text else {
                    return ControlResponse::failure(
                        &request.id,
                        "no transcript available for this session",
                    );
                };
                Self::success(&request.id, [("text".to_string(), text)])
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
                if comment.is_some() {
                    self.persist_worktree_comment(&workspace.path, &workspace.comment);
                    // F-SID-11: nudge the GPUI-thread poll loop so the
                    // materialized Sidebar entity picks up the new comment;
                    // the shared ControlState mutation above is otherwise
                    // invisible to it.
                    if let Ok(mut actions) = self.control_actions.lock() {
                        actions.push(ControlAction::RefreshSidebar);
                    }
                }
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
            // F-WIN-11: injects one `UpdateEvent` into the update toast's
            // state machine. `event` names the transition;
            // `available`/`failed` carry a free-text payload in `version`/
            // `message`, `download-progress` carries an integer `percent`.
            "update.event" => {
                let Some(kind) = request.params.get("event") else {
                    return ControlResponse::failure(&request.id, "update.event requires event");
                };
                let event = match kind.as_str() {
                    "check-started" => UpdateEvent::CheckStarted,
                    "available" => {
                        let Some(version) = request.params.get("version").cloned() else {
                            return ControlResponse::failure(
                                &request.id,
                                "update.event available requires version",
                            );
                        };
                        UpdateEvent::Available(version)
                    }
                    "download-progress" => {
                        let Some(percent) = request
                            .params
                            .get("percent")
                            .and_then(|value| value.parse::<u8>().ok())
                        else {
                            return ControlResponse::failure(
                                &request.id,
                                "update.event download-progress requires an integer percent",
                            );
                        };
                        UpdateEvent::DownloadProgress(percent)
                    }
                    "install-started" => UpdateEvent::InstallStarted,
                    "finished" => UpdateEvent::Finished,
                    "failed" => {
                        let Some(message) = request.params.get("message").cloned() else {
                            return ControlResponse::failure(
                                &request.id,
                                "update.event failed requires message",
                            );
                        };
                        UpdateEvent::Failed(message)
                    }
                    "reset" => UpdateEvent::Reset,
                    other => {
                        return ControlResponse::failure(
                            &request.id,
                            format!("update.event has an unknown event {other}"),
                        );
                    }
                };
                let Ok(mut actions) = self.control_actions.lock() else {
                    return ControlResponse::failure(
                        &request.id,
                        "control action queue unavailable",
                    );
                };
                actions.push(ControlAction::UpdateEvent(event));
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
            // I3-tray-jump: exercises the exact tray-roster-click code path
            // (select worktree, then jump to its worst-status tab) over the
            // socket, so it is observable without a live D-Bus
            // StatusNotifierWatcher session -- see `select_worktree_and_jump`.
            "tray.jump" => {
                let Some(selector) = request.params.get("workspace").cloned() else {
                    return ControlResponse::failure(&request.id, "tray.jump requires workspace");
                };
                self.queue_action(request, move |reply| ControlAction::TrayJump {
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

/// Builds the shell that runs an agent row's Install command (F-SET-18) —
/// the documented `install_command` string, executed through the user's
/// shell exactly like [`skill_install_shell`] runs the skill provisioner.
fn agent_install_shell(command: &str) -> TerminalShell {
    let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    TerminalShell::WithArguments {
        program: shell_program,
        args: vec!["-lc".to_string(), command.to_string()],
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

/// F-CORE-DOM-07: matches `AutoNamer.summarize`'s wide timeout — `claude -p`
/// alone measured ~20s cold; 10s truncated every real pass.
const AUTO_NAMING_TIMEOUT: Duration = Duration::from_secs(60);
/// Matches `AutoNamer.maxTitleLength`.
const AUTO_NAMING_MAX_TITLE_LEN: usize = 60;

/// The exact prompt text from the Swift `AutoNamer.summarize`, unchanged so
/// a summarizer tuned against the reference behaves the same here.
fn auto_naming_prompt(transcript: &str) -> String {
    format!(
        "Summarize this coding-agent conversation into a short title, 2-5 words, in the conversation's own language, no quotes, no punctuation at the end. Reply with only the title.\n\n{transcript}"
    )
}

/// Ordered summarize candidates for one auto-naming pass, ported from
/// `SummarizerSelection.adapters`: the user-selected summarizer agent
/// first, then the tab's own agent as a runtime fallback — each already
/// resolved to a concrete shell command via `AgentAdapter::summarizer_command`,
/// so a candidate with no summarizer support (an adapter this port has not
/// yet ported one for) is dropped rather than attempted.
fn summarizer_candidate_commands(
    selected_id: &str,
    tab_agent_id: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    let primary = AGENT_CATALOG
        .iter()
        .find(|adapter| adapter.id() == selected_id);
    let fallback =
        tab_agent_id.and_then(|id| AGENT_CATALOG.iter().find(|adapter| adapter.id() == id));
    let mut adapters: Vec<&dyn tiller_agents::AgentAdapter> = Vec::new();
    if let Some(primary) = primary {
        adapters.push(*primary);
    }
    if let Some(fallback) = fallback
        && Some(fallback.id()) != primary.map(|adapter| adapter.id())
    {
        adapters.push(*fallback);
    }
    adapters
        .into_iter()
        .filter_map(|adapter| adapter.summarizer_command(prompt))
        .collect()
}

/// Runs one summarizer candidate through the user's shell exactly like a
/// terminal-tab command does (`$SHELL`, falling back to `/bin/zsh -lc`),
/// with its cwd set to the worktree. Every failure mode — missing binary,
/// empty output, a hung process — returns `None` rather than surfacing an
/// error, matching `AutoNamer.summarize`'s "never a visible error" contract.
/// Blocking: callers run this on a background executor, never the UI thread.
fn run_summarizer_command(command: &str, worktree_path: &str, timeout: Duration) -> Option<String> {
    let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut child = std::process::Command::new(&shell_program)
        .args(["-lc", command])
        .current_dir(worktree_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = String::new();
        let _ = stdout.read_to_string(&mut buffer);
        let _ = tx.send(buffer);
    });
    let output = match rx.recv_timeout(timeout) {
        Ok(output) => {
            let _ = child.wait();
            output
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let title = output.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(AUTO_NAMING_MAX_TITLE_LEN).collect())
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
            // F-SET-20: every other SettingsSnapshot field the Appearance
            // section reports here had an entry; translucency did not, so
            // toggling it never showed up in the surface.settings.select
            // payload even though the underlying field flips correctly.
            "translucency".to_string(),
            snapshot.translucency.to_string(),
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
            "ollamaShowInBar".to_string(),
            report.ollama_show_in_bar.to_string(),
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

/// The inverse of [`activity_status_for_agent`]: `tiller_ui`'s render-facing
/// `ActivityStatus` back to the domain lifecycle status the activity crate's
/// rules are written against. `Idle` is not an agent state, so it maps to
/// `None` — the same "no agent status" `AgentActivityModel` returns for an
/// untracked pane, which is what makes it safe to feed straight into
/// `AttentionSort`.
fn agent_status_for_activity(status: ActivityStatus) -> Option<AgentStatus> {
    match status {
        ActivityStatus::Running => Some(AgentStatus::Running),
        ActivityStatus::NeedsInput => Some(AgentStatus::NeedsInput),
        ActivityStatus::Done => Some(AgentStatus::Done),
        ActivityStatus::Error => Some(AgentStatus::Error),
        ActivityStatus::Idle => None,
    }
}

/// The one urgency rank in the app: `AttentionSort::sorted`'s, reached
/// through the same `agent_status_for_activity` bridge, so "which of these
/// is worst" has exactly one answer whether it is asked about panes in a
/// split, tabs in a worktree, or worktrees in the sidebar. `Idle` is not an
/// agent state, so it ranks last — after `Done`, matching
/// `AttentionSort::sorted`'s treatment of an absent status.
///
/// F-CORE-ACT-22's clause is error → needs-input → running → done; a local
/// copy of that rule that tied running with needs-input is what made the
/// tray's jump land on the running tab instead of the one asking a question.
fn activity_rank(status: ActivityStatus) -> u8 {
    agent_status_for_activity(status).map_or(4, AgentStatus::priority)
}

/// F-CORE-ACT-23: whether closing this activity would kill live work and
/// therefore has to be confirmed first. The rule itself is
/// `tiller_activity::ActivityStatus::requires_close_confirmation` — this
/// only crosses the two identically-shaped `ActivityStatus` enums (the
/// render-facing one in `tiller_ui`, the domain one in `tiller_activity`)
/// so both the pane-close banner and the Activity panel's close button ask
/// the same function rather than each carrying its own copy of the list.
fn pane_close_needs_confirmation(status: ActivityStatus) -> bool {
    let domain =
        tiller_activity::ActivityStatus::from_agent_status(agent_status_for_activity(status));
    domain.requires_close_confirmation()
}

/// One worktree row's live agent facts, as resolved by
/// [`TillerWorkspace::sync_worktree_activity`].
#[derive(Clone)]
struct WorktreeActivity {
    /// The sidebar row this belongs to (`sidebar_worktree_id`'s numbering).
    row_id: usize,
    status: Option<ActivityStatus>,
    /// F-CORE-ACT-17: `agent_id_for_panes`, as the *tint* of the status
    /// indicator — `WorktreeStatusGlyph(status:agentId:)` uses `agentId`
    /// for nothing else.
    ///
    /// This used to be a `settings::AgentAccentColor`, i.e. one of the eight
    /// *semantic theme tokens* the agent-colour picker offers, and Claude's
    /// entry there is `Amber` — `theme.tab_needs_input`. A running Claude
    /// worktree therefore painted the byte-identical colour as one that
    /// needed input. `tiller_theme::AgentBrandColor` is a separate table for
    /// a separate job, which is also how the reference keeps them apart:
    /// `App/AgentAccentColor.swift` says in as many words that it is
    /// "unrelated to `AgentIcon.color(for:)`" — and it is `color(for:)`, not
    /// the picker, that `WorktreeStatusGlyph` reads.
    agent_brand: Option<AgentBrandColor>,
    /// F-CORE-ACT-18: `running_agent_ids`, as brand marks in catalog order.
    running: Vec<AgentMark>,
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
    /// F-TERM-PTY-07: "Restart Terminal" -- respawns the PTY in place,
    /// bumping the pane's `TerminalSurfaceHost` generation.
    Restart,
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
        TerminalContextAction::RestartTerminal => Some(TerminalContextCommand::Restart),
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
    /// F-SID-19: GPUI's key dispatch falls back to the *true* window root
    /// (above this workspace's own element tree, hence above every
    /// `on_action`/`capture_key_down` registered on it) whenever nothing
    /// holds keyboard focus -- so a global keybinding like `ctrl-t` is
    /// silently unreachable, not merely unhandled, the moment focus has
    /// nowhere live to land (e.g. a worktree with zero terminal tabs, whose
    /// "No Terminals" placeholder has no focusable element of its own).
    /// This handle gives the workspace root itself a permanent, always-
    /// mounted focus target so `render` can reclaim focus whenever it goes
    /// missing, keeping every root-level keybinding reachable regardless of
    /// what surface is showing.
    root_focus: FocusHandle,
    /// The single source of truth for agent lifecycle status. Every one of
    /// the sidebar dot, the tab checkmark, and the Activity row reads
    /// through this (or, for a chat pane, through `Chat`'s own state) —
    /// never a second, independently-tracked flag.
    activity: AgentActivityModel,
    /// Prevents scheduling restored scrollback more than once before the
    /// first frame mounts the terminal entities.
    restored_scrollback_scheduled: bool,
    /// F-CORE-ACT-20: whether the OS considers this window focused, per
    /// `Window::is_window_active` -- a real, already-portable GPUI API
    /// (backed uniformly by every platform's own window, no linux-specific
    /// code needed here). Polled once per frame in `render` (the same
    /// "Layer E is polled, not pushed" pattern that function already uses
    /// for streaming/exit state), because `NotificationPolicy::should_notify`
    /// is decided from `post_activity_notification`, which fires from
    /// Layer B/C/D activity-transition callbacks that carry no `Window` of
    /// their own -- only `render` and the control-socket dispatch loop do.
    /// Starts `true` (matching this argument's previous hardcoded value)
    /// so the very first transition, before any frame has painted, keeps
    /// today's behaviour rather than guessing unfocused.
    window_active: bool,
    browser_origins: BTreeSet<String>,
    /// F-TERM-08: an interactive pane close (Cmd-W, right-click "Close
    /// Terminal…") that would kill a pane whose `ActivityStatus` reports
    /// `requires_close_confirmation()` is held here instead of closing
    /// immediately, and rendered as a dismissible banner over that pane.
    /// Headless closes (the control socket's `ClosePane`) intentionally
    /// bypass this — same precedent as `project.add`: no user is present to
    /// answer a prompt.
    pending_pane_close: Option<PendingPaneClose>,
    /// F-WIN-10: a transient, floating, auto-dismissing notice -- distinct
    /// from `Sidebar::set_notice`'s persistent inline banner, which stays
    /// silent when the sidebar isn't the visible surface. `toast_id` tags
    /// each raise so a stale auto-dismiss timer (from a toast a newer one
    /// already replaced) can't clear a toast it doesn't own.
    toast: Option<Toast>,
    next_toast_id: u64,
    /// F-WIN-11: the update-toast's own state machine, driven exclusively
    /// by `ControlAction::UpdateEvent` -- no Linux update transport exists
    /// yet to raise these on its own (see `tiller_project::ui`'s
    /// doc-comment on `UpdateState`).
    update_state: UpdateState,
    /// F-CORE-DOM-07: one [`tiller_project::AutoNamingThrottle`] per tab id,
    /// gating how often a running→done/needs-input transition is allowed to
    /// spawn a real summarizer process and rewrite that tab's title.
    auto_naming_throttle: BTreeMap<usize, tiller_project::AutoNamingThrottle>,
    /// Directories this window published app panes under on the last
    /// [`Self::sync_control_panes`]. `set_external_state` replaces one
    /// directory's list at a time, so the ones that drop out have to be
    /// cleared by name or they keep a ghost of a pane that moved or closed.
    published_pane_directories: BTreeSet<PathBuf>,
    /// F-TERM-02: one cached [`TerminalView::empty_prompt`] entity per pane
    /// group that currently has zero tabs, keyed by `TabGroup::id`. Kept
    /// alive across renders (rather than built fresh every frame) so its
    /// `TerminalPromptEvent` subscription is created once, matching how
    /// every other live pane entity in `tabs` is owned. Synced by
    /// [`Self::sync_empty_pane_prompts`], which `render` calls every frame —
    /// entries are added the moment a group's last tab leaves it and dropped
    /// the moment it gets one back, so `render_group_surfaces` can just
    /// look one up instead of deciding whether to build one mid-render.
    empty_pane_prompts: BTreeMap<usize, Entity<TerminalView>>,
    /// F-TERM-PTY-08: records where every live terminal pane in this
    /// worktree currently sits, keyed by the same `terminal-{pane_id}`
    /// content id `bind_terminal` gives its `TerminalIdentity`. Kept in step
    /// by `track_terminal_panes_in_cache` (called from `apply_tab_machinery`,
    /// so it covers every pane-group move) and `select_pane` (which also
    /// records focus) -- moving the *entity* itself already happens by
    /// construction (`OpenTab`/`PaneNode` own it by value, never rebuilt on
    /// a move), so this is the seam's own durable record of that placement,
    /// not what makes the PTY/scrollback survive.
    terminal_pane_cache: TerminalPaneCache<Entity<TerminalView>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Toast {
    id: u64,
    message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingPaneClose {
    tab_id: usize,
    pane_id: usize,
    /// F-CORE-ACT-23: an Activity-row close targets the whole tab, not the
    /// one pane the banner happens to be drawn over. Same held-close, same
    /// banner, same `requires_close_confirmation` gate — only what
    /// "Close Anyway" then closes differs.
    whole_tab: bool,
    /// The status that made this close need confirming, captured at the
    /// moment it was held. The banner describes *this* — it used to say
    /// "has running work" for all three confirming states, so an agent that
    /// had failed was announced as still working.
    status: ActivityStatus,
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
        activity: AgentActivityModel,
        tray_roster: Option<tray::SharedRoster>,
        tray_requests: Option<tray::TrayRequestQueue>,
        tray_handle: Option<tray::TrayHandle>,
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
            // F-USE-04: the last roster ksni was actually told about --
            // `TrayHandle::nudge` is a synchronous round trip to ksni's
            // background thread, so this skips it on ticks where nothing
            // moved rather than paying that cost every 40ms.
            let mut last_tray_roster: Vec<tray::TrayRosterEntry> = Vec::new();
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
                let pending_tray = tray_requests.as_ref().and_then(|queue| {
                    queue
                        .lock()
                        .ok()
                        .map(|mut requests| std::mem::take(&mut *requests))
                });
                if pending.is_empty()
                    && pending_control.is_empty()
                    && pending_tray.as_ref().is_none_or(Vec::is_empty)
                    && tray_roster.is_none()
                {
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
                                WorkspaceAction::InstallAgent { agent_id, command } => {
                                    workspace.add_terminal_tab_with_shell(
                                        format!("Install {agent_id}"),
                                        agent_install_shell(command),
                                        None,
                                        cx,
                                    );
                                }
                                WorkspaceAction::OpenSettings => {
                                    workspace.open_settings(None, cx);
                                }
                                WorkspaceAction::OpenAgentSettings => {
                                    workspace.open_settings(Some(SettingsCategory::Agents), cx);
                                }
                                WorkspaceAction::CloseSettings => {
                                    workspace.show_settings = false;
                                    workspace.restore_focus_pending = true;
                                    cx.notify();
                                }
                                WorkspaceAction::OpenBrowserLink(url) => {
                                    workspace.add_browser_tab(url, window, cx);
                                }
                                WorkspaceAction::OpenNewTabPalette => {
                                    workspace.open_command_palette(window, cx);
                                }
                                WorkspaceAction::RestoreLaunchSnapshot => {
                                    if let Err(error) =
                                        workspace.restore_launch_snapshot(window, cx)
                                    {
                                        workspace.sidebar.update(cx, |sidebar, cx| {
                                            sidebar.set_notice(
                                                format!(
                                                    "[history] could not restore the previous launch: {error}"
                                                ),
                                                cx,
                                            )
                                        });
                                    }
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
                                    workspace.request_auto_rename(&transition, cx);
                                    workspace.sync_activity(cx);
                                }
                                ControlAction::UpdateEvent(event) => {
                                    workspace.update_state =
                                        workspace.update_state.clone().transition(event);
                                    cx.notify();
                                }
                                ControlAction::SelectWorktree { selector, reply } => {
                                    let result = workspace.control_select_worktree(
                                        &selector,
                                        Some(&mut *window),
                                        cx,
                                    );
                                    let _ = reply.send(result);
                                }
                                ControlAction::TrayJump { selector, reply } => {
                                    let result = workspace.control_select_worktree_and_jump(
                                        &selector,
                                        Some(&mut *window),
                                        cx,
                                    );
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
                                    let result = workspace.control_open_changes(
                                        worktree.as_deref(),
                                        Some(&mut *window),
                                        cx,
                                    );
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
                                ControlAction::SplitPane {
                                    direction,
                                    placement,
                                    reply,
                                } => {
                                    workspace.split_focused_terminal_with_placement(
                                        direction, placement, None, cx,
                                    );
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
                                ControlAction::Chat { action, reply } => {
                                    let result = workspace.handle_chat_action(
                                        action,
                                        Some(&mut *window),
                                        cx,
                                    );
                                    let _ = reply.send(result);
                                }
                                ControlAction::RefreshSidebar => {
                                    workspace.refresh_sidebar(cx);
                                }
                            }
                        }
                        // F-USE-04: refresh the tray roster from the same
                        // per-worktree pane/activity state
                        // `evict_over_capacity_worktrees`'s `status_of`
                        // closure already reads, every tick the tray is
                        // registered -- cheap for a handful of worktrees,
                        // and simpler than threading a change notification
                        // through every call site that can move a status.
                        if let Some(roster) = &tray_roster {
                            let snapshot = workspace.tray_roster_snapshot();
                            if snapshot != last_tray_roster {
                                if let Ok(mut guard) = roster.lock() {
                                    *guard = snapshot.clone();
                                }
                                last_tray_roster = snapshot;
                                if let Some(handle) = &tray_handle {
                                    handle.nudge();
                                }
                            }
                        }
                        // F-USE-05 / F-WIN-08: roster clicks and the tray
                        // icon's own left click, drained the same way
                        // `pending`/`pending_control` are above.
                        for request in pending_tray.into_iter().flatten() {
                            match request {
                                tray::TrayRequest::ShowWindow => {
                                    window.activate_window();
                                }
                                tray::TrayRequest::SelectWorktree(path) => {
                                    // I3-tray-jump: `select_worktree_and_jump` is the
                                    // exact same call the control socket's `tray.jump`
                                    // makes, so a socket drive proves this arm's own
                                    // behaviour, not a parallel stand-in for it.
                                    let _ = workspace.select_worktree_and_jump(
                                        path,
                                        Some(&mut *window),
                                        cx,
                                    );
                                    window.activate_window();
                                }
                                tray::TrayRequest::Quit => {
                                    cx.quit();
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
        for tab in &tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Chat(chat) = content {
                    Self::bind_chat(chat, cx);
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
                    // No `&mut Window` reaches an entity-event `cx.subscribe`
                    // callback -- `restore_tabs` tolerates `None` the same
                    // way boot's own call does, skipping only `browser`
                    // tabs. See `select_worktree`'s doc comment.
                    let _ = workspace.select_worktree(path.clone(), None, cx);
                }
                SidebarEvent::CloseTab(id) => workspace.close_tab_by_id(*id, cx),
                SidebarEvent::OpenProjectSettings(id) => {
                    workspace
                        .sidebar
                        .update(cx, |sidebar, cx| sidebar.open_project_settings(id, cx));
                }
                SidebarEvent::ProjectSettingsChanged(update) => {
                    workspace.update_project_settings(update, cx)
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
        let mut workspace = Self {
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
            root_focus: cx.focus_handle(),
            activity,
            restored_scrollback_scheduled: false,
            window_active: true,
            browser_origins,
            show_settings: false,
            restore_focus_pending: false,
            pending_pane_close: None,
            published_pane_directories: BTreeSet::new(),
            toast: None,
            next_toast_id: 0,
            update_state: UpdateState::Idle,
            auto_naming_throttle: BTreeMap::new(),
            empty_pane_prompts: BTreeMap::new(),
            terminal_pane_cache: TerminalPaneCache::new(),
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
                    id: tab.persistence_id.clone(),
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
                        match content {
                            TabContent::Terminal { view } => {
                                state
                                    .scrollback
                                    .insert(pane_id, view.read(cx).capture_scrollback());
                            }
                            // F-CORE-WSP-08: captured fresh at save time,
                            // the same live-read pattern as scrollback
                            // above, so a draft the user never sent still
                            // survives a restart.
                            TabContent::Chat(chat) => {
                                state.chat_draft = chat.read(cx).draft_text();
                            }
                            TabContent::File { .. }
                            | TabContent::Changes(_)
                            | TabContent::Browser(_) => {}
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
        // F-SID-11: comment is control-state metadata (worktree.set), keyed
        // by path -- not part of the git-derived ProjectCatalog -- so it is
        // looked up here rather than carried on session::CatalogWorktree.
        let comments: BTreeMap<String, String> = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .workspaces
            .iter()
            .filter(|workspace| !workspace.comment.is_empty())
            .map(|workspace| (workspace.path.clone(), workspace.comment.clone()))
            .collect();
        sidebar_projects_with_comments(&self.project_catalog, &comments)
    }

    fn refresh_sidebar(&mut self, cx: &mut Context<Self>) {
        let projects = self.sidebar_projects();
        let identities = sidebar_project_identities(&self.project_catalog);
        // F-PRJ-17/F-PRJ-18: pushed the same way identities are, right after
        // `set_projects` rebuilds the row tree and resets both maps to
        // empty -- an already-open Project Settings sheet was seeded from
        // `project_worktree_defaults` in `open_project_settings`, so a
        // reopen after this refresh shows whatever `update_project_settings`
        // just persisted.
        let worktree_defaults = sidebar_project_worktree_defaults(&self.project_catalog);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_projects(projects, cx);
            for (id, display_name, icon) in identities {
                sidebar.set_project_identity(&id, display_name, icon, cx);
            }
            for (id, default_base, location_override) in worktree_defaults {
                sidebar.set_project_worktree_defaults(&id, default_base, location_override, cx);
            }
        });
        // `set_projects` rebuilds every row from the catalog, which drops the
        // per-row agent facts and the urgency order with them. Re-apply both
        // here so a refresh (a project added, a worktree created) never
        // silently reverts a needs-input worktree back down the list.
        self.sync_worktree_activity(cx);
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
                RightPanelEvent::CloseActivity(index) => {
                    workspace.request_close_activity(*index, cx)
                }
                RightPanelEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            },
        )
        .detach();
        cx.subscribe(
            right_panel,
            |workspace, _, event: &RightPanelActionEvent, cx| match event {
                RightPanelActionEvent::OpenDiff(path) => {
                    workspace.add_changes_tab(Some(path.clone()), cx)
                }
            },
        )
        .detach();
    }

    /// F-CORE-FILE-04: a link click inside a file view resolves a path but
    /// can only open it through the workspace, which owns tab identity.
    /// Routes through the same de-duplicating `add_file_tab` path as the
    /// Files panel and chat transcript link clicks.
    fn subscribe_file_view(file_view: &Entity<FileView>, cx: &mut Context<Self>) {
        cx.subscribe(
            file_view,
            |workspace, _, event: &FileViewEvent, cx| match event {
                FileViewEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            },
        )
        .detach();
    }

    /// Chat-local edit summaries emit only an intent to open a file; the
    /// workspace owns the editor tab and routes that intent through the same
    /// de-duplicating path used by the file tree and Changes surface.
    fn bind_chat(chat: &Entity<Chat>, cx: &mut Context<Self>) {
        cx.subscribe(chat, |workspace, _, event: &ChatEvent, cx| match event {
            ChatEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx),
            // F-BRW-09: this subscription predates a Window-aware callback
            // (see `bind_chat`'s two call sites, one of which has no
            // Window), so queue the browser-tab creation for the
            // update_in-based action loop the same way NewTab/OpenSettings
            // already do.
            ChatEvent::OpenLink(url) => {
                if let Ok(mut actions) = workspace.pending_actions.lock() {
                    actions.push(WorkspaceAction::OpenBrowserLink(url.clone()));
                }
            }
        })
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
        let terminal_entity = terminal.clone();
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
                        workspace.request_close_terminal_at(tab_id, pane_id, cx);
                    }
                    Some(TerminalContextCommand::Restart) => {
                        terminal_entity.update(cx, |terminal, cx| terminal.restart(cx));
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
            move |workspace, _, event: &TerminalLinkEvent, _cx| {
                if let Some(url) = terminal_link_url_for_pane(event, &expected_pane_id) {
                    // F-TERM-UI-02: opens in tiller's own Browser tab, the same
                    // route the Chat transcript's own link clicks already use
                    // (`bind_chat`'s `ChatEvent::OpenLink`) -- not `cx.open_url`,
                    // which launches the OS's external browser instead of the
                    // in-app one the row's own contract names.
                    if let Ok(mut actions) = workspace.pending_actions.lock() {
                        actions.push(WorkspaceAction::OpenBrowserLink(url.to_string()));
                    }
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
                    workspace.request_auto_rename(&transition, cx);
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
                            workspace.request_auto_rename(&transition, cx);
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
                let message = format!("already tracked or nested: {}", path.display());
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_notice(message.clone(), cx));
                self.show_toast(message, cx);
            }
            Err(error) => {
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_notice(error.clone(), cx));
                self.show_toast(error, cx);
            }
        }
    }

    fn update_project_settings(&mut self, update: &ProjectSettingsUpdate, cx: &mut Context<Self>) {
        let (icon_kind, icon_value) = update.icon.persisted_parts();
        // F-PRJ-17/F-PRJ-18: `update.default_worktree_base`/
        // `worktree_location_override` are always the sheet's *current*
        // drafts, not just "what the user just typed into those two
        // fields" -- `open_project_settings` seeds them from
        // `project_worktree_defaults` when the sheet opens, and every edit
        // (icon, display name, base, location alike) re-emits the card's
        // full state via `project_settings_update`. So using them directly
        // here already carries forward an untouched value; falling back to
        // `self.project_catalog.project_settings(&update.id)` would instead
        // only be correct before this sheet ever seeded its drafts from it,
        // which `open_project_settings` guarantees it always does.
        let settings = CatalogProjectSettings {
            color_hex: Some(update.icon.tint.id().to_string()),
            display_name: update.display_name.clone(),
            icon_kind,
            icon_value,
            default_worktree_base: update.default_worktree_base.clone(),
            worktree_location_override: update.worktree_location_override.clone(),
        };
        match self
            .project_catalog
            .update_project_settings(&update.id, settings)
        {
            Ok(()) => {
                self.session.schedule_catalog(&self.project_catalog);
                self.refresh_sidebar(cx);
            }
            Err(error) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.set_notice(error, cx)),
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
                    && self.select_worktree(path.clone(), None, cx).is_err()
                {
                    return;
                }
                if let Ok(mut actions) = self.pending_actions.lock() {
                    actions.push(WorkspaceAction::NewTab(action));
                }
            }
            (_, SidebarContextAction::RemoveProject) => {}
            // F-SID-15: RemoveWorktree is intercepted inside
            // Sidebar::dispatch_context_action (confirm-gated there, the
            // same way RemoveProject is) and never reaches this event --
            // this arm exists only so the match stays exhaustive.
            (_, SidebarContextAction::RemoveWorktree) => {}
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

    /// What a live surface entity claims about itself — the evidence
    /// `AgentActivityModel`'s four layers cannot see, because it lives in
    /// the GPUI entity rather than in a hook push, a title, scrollback or
    /// `/proc`: an ACP chat that is mid-stream or has finished a turn, a
    /// terminal that failed to spawn or whose child has already been reaped.
    ///
    /// `None` means the surface has no opinion and the layered status
    /// stands.
    fn surface_evidence(content: &TabContent, cx: &App) -> Option<AgentStatus> {
        match content {
            TabContent::Chat(chat) => {
                let chat = chat.read(cx);
                if chat.is_streaming() {
                    Some(AgentStatus::Running)
                } else if chat.has_completed_turn() {
                    Some(AgentStatus::Done)
                } else {
                    None
                }
            }
            TabContent::Terminal { view } => {
                let terminal = view.read(cx);
                if terminal.is_failed() {
                    Some(AgentStatus::Error)
                } else {
                    terminal.exit_status().map(|exit_status| match exit_status {
                        TerminalExitStatus::Success => AgentStatus::Done,
                        TerminalExitStatus::Code(_)
                        | TerminalExitStatus::Signal(_)
                        | TerminalExitStatus::Unknown => AgentStatus::Error,
                    })
                }
            }
            TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => None,
        }
    }

    /// Pushes every mounted surface's own evidence into the one
    /// `AgentActivityModel` (Layer E), so that from here on *every* view of
    /// a pane's status — the worktree dot, the tab check, the Activity row,
    /// the tray roster, the close confirmation — reads the same map.
    ///
    /// This is the repair for the defect that made the feature misleading:
    /// the worktree row used to consult the model for every worktree but
    /// the selected one, and the live entities for that one, so clicking a
    /// row could change the status it showed. The facts the entities know
    /// are real, so they are fed in here rather than read a second time
    /// downstream. What the layer deliberately does *not* do is invent an
    /// identity: a shell that exited non-zero gets an error status but
    /// never an agent, so it can still never reach the row's brand badge or
    /// tint.
    ///
    /// Returns whether anything actually changed.
    fn sync_entity_evidence(&mut self, cx: &App) -> bool {
        let mut evidence: Vec<(String, Option<AgentStatus>)> = Vec::new();
        for tab in &self.tabs {
            tab.panes.for_each(&mut |pane_id, content| {
                evidence.push((
                    format!("pane-{pane_id}"),
                    Self::surface_evidence(content, cx),
                ));
            });
        }
        let mut changed = false;
        for (pane_id, status) in evidence {
            changed |= self.activity.set_entity_status(&pane_id, status);
        }
        changed
    }

    /// One tab's real, live status — `None` when the tab has no pane that
    /// can carry one at all (a diff, a file, a browser). Every pane's status
    /// comes from `self.activity`, the one `AgentActivityModel` this
    /// workspace owns, with the surfaces' own evidence already folded into
    /// it by [`Self::sync_entity_evidence`]. For a split tab the
    /// highest-priority pane wins, in `AgentStatus::priority` order —
    /// error, needs-input, running, done — with a pane that has no status
    /// at all ranked last, exactly as `AttentionSort` ranks it.
    fn tab_status(&self, tab: &OpenTab, _cx: &App) -> Option<ActivityStatus> {
        let mut status: Option<ActivityStatus> = None;
        tab.panes.for_each(&mut |pane_id, content| {
            let candidate = match content {
                TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => return,
                _ => self.pane_status(tab, pane_id),
            };
            if status.is_none_or(|current| activity_rank(candidate) < activity_rank(current)) {
                status = Some(candidate);
            }
        });
        status
    }

    /// F-TERM-10: `tab_status`/`pane_close_needs_confirmation` only reflect
    /// the *agent* activity model (Layers A-D) -- a real signal, but scoped
    /// to recognized agent CLIs and OSC-title/ACP conventions. A bare shell
    /// command with no agent identity (`sleep 300`, a build, anything typed
    /// at a plain prompt) reads as `ActivityStatus::Idle` there, which made
    /// a worktree switch's reload-from-disk in `select_worktree` silently
    /// tear its PTY down -- contradicting CLAUDE.md's documented contract
    /// that terminal hosts "stay mounted (PTYs alive) across sidebar
    /// selection changes". This checks the terminal's actual process tree
    /// instead of the activity model: any descendant of the login shell
    /// (`tiller_activity::inspect_process_names`, the same Layer-D walk
    /// that powers agent detection) means a real command is running right
    /// now, agent or not, and this tab must not be silently reloaded.
    /// `Err` (non-Linux, or the shell process already gone) is treated as
    /// "nothing found running" -- the existing agent-status check still
    /// gates the genuinely-tracked cases, so this only ever narrows the gap,
    /// never widens what a switch is allowed to tear down.
    fn tab_has_live_foreground_process(&self, tab: &OpenTab, cx: &App) -> bool {
        let mut found = false;
        tab.panes.for_each(&mut |_, content| {
            if found {
                return;
            }
            let Some(terminal) = content.terminal() else {
                return;
            };
            let Some(shell_pid) = terminal.read(cx).shell_pid() else {
                return;
            };
            if let Ok(names) = tiller_activity::inspect_process_names(shell_pid)
                && !names.is_empty()
            {
                found = true;
            }
        });
        found
    }

    /// One pane's status, straight out of the one model. `tab-{id}` is the
    /// fallback key a hook push that names the tab rather than the leaf
    /// lands under.
    fn pane_status(&self, tab: &OpenTab, pane_id: usize) -> ActivityStatus {
        self.activity
            .status(&format!("pane-{pane_id}"))
            .or_else(|| self.activity.status(&format!("tab-{}", tab.id)))
            .map_or(ActivityStatus::Idle, activity_status_for_agent)
    }

    /// Same evidence `tab_status` uses to pick the tab's worst-status pane,
    /// narrowed to one specific pane. Used to decide whether *closing this
    /// pane* would kill live work (F-TERM-08).
    fn pane_activity_status(&self, tab: &OpenTab, pane_id: usize, _cx: &App) -> ActivityStatus {
        let mut found = ActivityStatus::Idle;
        tab.panes.for_each(&mut |id, content| {
            if id != pane_id {
                return;
            }
            found = match content {
                TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => {
                    ActivityStatus::Idle
                }
                _ => self.pane_status(tab, pane_id),
            };
        });
        found
    }

    /// The interactive entry point for closing the focused pane (Cmd-W /
    /// Ctrl-W). Gated by F-TERM-08: a pane doing live work is not closed
    /// silently, it is held in `pending_pane_close` and rendered as a
    /// confirm-or-cancel banner over the pane instead.
    fn request_close_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some((tab_id, focused_pane)) = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.id, tab.focused_pane))
        else {
            return;
        };
        self.request_close_terminal_at(tab_id, focused_pane, cx);
    }

    /// The interactive entry point for closing a specific pane, e.g. the
    /// terminal context menu's "Close Terminal…" item. See
    /// `request_close_focused_pane` for the gating rule.
    fn request_close_terminal_at(&mut self, tab_id: usize, pane_id: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let status = self.pane_activity_status(tab, pane_id, cx);
        // F-TAB-26: `close_terminal_at` refuses to remove a tab's *last*
        // leaf (the F-TAB-13 empty-state guard), so closing this pane when
        // it is the tab's only one is really closing the whole tab. This
        // used to be hardcoded `false` here regardless, so "Close Anyway" on
        // a single-pane tab hit that guard and silently did nothing -- the
        // confirm banner dismissed but the tab never closed.
        let whole_tab = tab.panes.leaf_ids().len() <= 1;
        if pane_close_needs_confirmation(status) {
            self.pending_pane_close = Some(PendingPaneClose {
                tab_id,
                pane_id,
                whole_tab,
                status,
            });
            cx.notify();
            return;
        }
        if whole_tab {
            self.close_tab_by_id(tab_id, cx);
            return;
        }
        self.close_terminal_at(tab_id, pane_id, None, cx);
    }

    /// F-CORE-ACT-23: the Activity panel's own close button. It used to call
    /// `close_tab` straight through, so the one list that exists to show a
    /// running agent was also the one place that could kill it without
    /// asking. Now it consults the same
    /// `tiller_activity::ActivityStatus::requires_close_confirmation` the
    /// pane close does — running, needs-input and error are held for
    /// confirmation, done and idle close immediately — and reuses the
    /// existing hold-and-banner rather than raising a second prompt.
    fn request_close_activity(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let tab_id = tab.id;
        let pane_id = tab.focused_pane;
        let status = self.tab_status(tab, cx).unwrap_or(ActivityStatus::Idle);
        if pane_close_needs_confirmation(status) {
            self.pending_pane_close = Some(PendingPaneClose {
                tab_id,
                pane_id,
                whole_tab: true,
                status,
            });
            cx.notify();
            return;
        }
        self.close_tab(index, cx);
    }

    fn confirm_pending_pane_close(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending_pane_close.take() {
            if pending.whole_tab {
                self.close_tab_by_id(pending.tab_id, cx);
            } else {
                self.close_terminal_at(pending.tab_id, pending.pane_id, None, cx);
            }
        }
    }

    fn cancel_pending_pane_close(&mut self, cx: &mut Context<Self>) {
        self.pending_pane_close = None;
        cx.notify();
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

    /// F-USE-04: the tray menu's roster, one row per worktree with a live
    /// [`AgentStatus`], sorted by the same urgency rule the sidebar uses.
    /// Reads `control_state` rather than `self.project_catalog` --
    /// `project_catalog` is this window's own render-facing snapshot and
    /// does not pick up a bare `project.add`/`worktree.set` the way
    /// `evict_over_capacity_worktrees`'s `status_of` closure (which reads
    /// the same `control_state.workspaces` this does) already relies on.
    /// Pane status is tracked per pane id across the whole app, not scoped
    /// to whichever worktree is selected in this window, so this covers
    /// every mounted worktree, not just `self.tabs`.
    fn tray_roster_snapshot(&self) -> Vec<tray::TrayRosterEntry> {
        let state = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut entries = Vec::new();
        for workspace in &state.workspaces {
            let Ok(panes) = self.panes.list_for(Path::new(&workspace.path)) else {
                continue;
            };
            let pane_ids: Vec<String> = panes.into_iter().map(|pane| pane.id).collect();
            let refs: Vec<&str> = pane_ids.iter().map(String::as_str).collect();
            let Some(status) = self.activity.status_for_panes(&refs) else {
                continue;
            };
            entries.push(tray::TrayRosterEntry {
                path: PathBuf::from(&workspace.path),
                branch: workspace.branch.clone(),
                project_name: workspace.project.clone(),
                status,
            });
        }
        tiller_activity::AttentionSort::sorted(&entries, |entry| Some(entry.status))
    }

    /// F-USE-05: the tab a tray roster click jumps to once its worktree is
    /// selected — `AppModel.worstStatusTab(in:)` in the Swift original,
    /// which is literally `AttentionSort.sorted(tabs) { … }.first`. It goes
    /// through the same `AttentionSort::sorted` here for the same reason:
    /// the local rank this used to carry tied running with needs-input, so
    /// a jump with one running tab and one waiting on an answer brought the
    /// running one forward and left the question in the background. `sorted`
    /// is a stable sort, so ties still keep tab order.
    fn worst_status_tab_id(&self, cx: &App) -> Option<usize> {
        let ranked: Vec<(usize, ActivityStatus)> = self
            .tabs
            .iter()
            .filter_map(|tab| self.tab_status(tab, cx).map(|status| (tab.id, status)))
            .collect();
        tiller_activity::AttentionSort::sorted(&ranked, |(_, status)| {
            agent_status_for_activity(*status)
        })
        .first()
        .map(|(id, _)| *id)
    }

    /// I3-tray-jump: the single code path both the tray's own roster-row
    /// click (`tray::TrayRequest::SelectWorktree`) and the control socket's
    /// `tray.jump` method drive -- selects `path`, then jumps to its
    /// worst-status tab exactly the way `AgentRosterView.select(_:)` calls
    /// `worstStatusTab(in:)` then `activateTab` in the Swift original. This
    /// is factored out (rather than duplicated between the tray handler and
    /// the control dispatcher) specifically so a control-socket drive
    /// exercises the *same* production logic the tray uses, not a parallel
    /// reimplementation that could pass while the tray path itself regressed.
    ///
    /// Returns `Ok(Some((tab_id, tab_title)))` when a tab was actually
    /// activated, `Ok(None)` when the selection succeeded but no tab in
    /// `self.tabs` carried a non-idle status to jump to (or the worktree has
    /// no tabs at all), and `Err` when `select_worktree` itself failed.
    ///
    /// CENTER-01: `select_worktree` now does rehydrate `self.tabs` from
    /// `path`'s own persisted session when the switch is safe to make (see
    /// its own doc comment) -- so this jumps into the tab `path`'s database
    /// row actually names, not whichever worktree happened to be
    /// materialized in `self.tabs` before the call, the way it used to.
    fn select_worktree_and_jump(
        &mut self,
        path: PathBuf,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<Option<(usize, String)>, String> {
        self.select_worktree(path, window, cx)?;
        let Some(id) = self.worst_status_tab_id(cx) else {
            return Ok(None);
        };
        let title = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.title.clone())
            .unwrap_or_default();
        self.select_tab(id, cx);
        Ok(Some((id, title)))
    }

    fn activity_surfaces(&self, cx: &App) -> Vec<ActivitySurface> {
        self.tabs
            .iter()
            .map(|tab| {
                let icon = tab_icon(
                    tab.kind,
                    tab_has_file(tab),
                    self.tab_agent_mark(tab).map(|agent| agent.icon),
                );
                let status = self.tab_status(tab, cx).unwrap_or(ActivityStatus::Idle);
                ActivitySurface::new(icon, tab.title.clone(), self.worktree_label.clone(), status)
            })
            .collect()
    }

    /// Publishes this window's live panes to the control registry.
    ///
    /// Each pane is filed under **the directory it is actually running in**
    /// (a terminal's own `working_directory`, fixed at spawn), not under
    /// whichever worktree happens to be selected. Those are the same
    /// directory in the ordinary case, and differ in exactly one situation:
    /// this port keeps a single, un-scoped-to-worktree tab list
    /// (F-CHG-19's single-open-worktree model), so selecting a different
    /// worktree leaves the previous worktree's terminals mounted. Filing
    /// them under the new selection re-homed live panes on every click,
    /// which is how selecting an untouched worktree could inherit another
    /// one's failing agent and show up red.
    ///
    /// `set_external_state` replaces a directory's whole app-pane list, so
    /// directories that were published last time and own nothing now are
    /// explicitly cleared — otherwise a pane that moved (or closed) would
    /// leave a ghost behind. The selected worktree is always published,
    /// even empty, for the same reason.
    fn sync_control_panes(&mut self, cx: &App) {
        let mut by_directory: BTreeMap<PathBuf, Vec<(PaneInfo, PaneStateSnapshot)>> =
            BTreeMap::new();
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
                by_directory
                    .entry(state.working_directory.clone())
                    .or_default()
                    .push((
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
        by_directory
            .entry(self.working_directory.clone())
            .or_default();
        for stale in self
            .published_pane_directories
            .iter()
            .filter(|directory| !by_directory.contains_key(*directory))
        {
            if let Err(error) = self.panes.set_external(stale, Vec::new()) {
                eprintln!("[control] failed to clear stale application panes: {error}");
            }
        }
        self.published_pane_directories = by_directory.keys().cloned().collect();
        for (directory, panes) in by_directory {
            if let Err(error) = self.panes.set_external_state(&directory, panes) {
                eprintln!("[control] failed to snapshot application panes: {error}");
            }
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

    /// F-CORE-ACT-26: caps how many worktrees stay mounted once the newly
    /// selected one joins them. `WorktreeMountPolicy::ids_to_evict` never
    /// touches the selected worktree, and skips any worktree with a running
    /// or needs-input agent -- it only evicts idle worktrees over the cap,
    /// oldest-open first. Unsaved-work detection is not built yet, so that
    /// gate always reports "safe to evict"; see the wave-G2 report for the
    /// follow-up this leaves open.
    fn evict_over_capacity_worktrees(&mut self, selected_path: &Path, cx: &mut Context<Self>) {
        let snapshot = self.settings.read(cx).snapshot();
        let cap = if snapshot.limit_mounted_worktrees {
            snapshot.mounted_worktrees.clamp(2, 50) as usize
        } else {
            0
        };
        let mut state = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let open_ids: Vec<String> = state
            .workspaces
            .iter()
            .filter(|workspace| workspace.mounted)
            .map(|workspace| workspace.path.clone())
            .collect();
        let selected_id = selected_path.to_string_lossy().into_owned();
        let status_of = |id: &String| -> Option<AgentStatus> {
            let pane_ids: Vec<String> = self
                .panes
                .list_for(Path::new(id))
                .ok()?
                .into_iter()
                .map(|pane| pane.id)
                .collect();
            let refs: Vec<&str> = pane_ids.iter().map(String::as_str).collect();
            self.activity.status_for_panes(&refs)
        };
        let evicted = WorktreeMountPolicy::ids_to_evict(
            &open_ids,
            Some(&selected_id),
            cap,
            status_of,
            |_id| false,
        );
        for id in evicted {
            state.close_worktree(Path::new(&id));
        }
    }

    /// `window` is `None` from every call site that has no `&mut Window` to
    /// give (a `cx.subscribe` callback, a headless control-socket path) --
    /// [`restore_tabs`] tolerates that the same way boot's own call does,
    /// skipping only `browser` tabs (which need a window to construct their
    /// child view) rather than failing the whole switch. Pass `Some(window)`
    /// wherever one is already in scope so a runtime switch restores with
    /// full fidelity.
    fn select_worktree(
        &mut self,
        requested_path: PathBuf,
        window: Option<&mut Window>,
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
        self.evict_over_capacity_worktrees(&selected_path, cx);

        // CENTER-01: `self.tabs` is this window's single, un-scoped-to-worktree
        // tab list (F-CHG-19's "single-open-worktree model", also documented
        // on `session`'s own module doc). Until now, a switch left it
        // materialized for whichever worktree happened to already be on
        // screen -- the stale-centre-pane defect: the sidebar highlight,
        // status bar and right panel all flip to the new selection, but the
        // centre pane keeps showing the old one, exactly relabelled by
        // `sync_control_panes` under the new path on the very next sync.
        //
        // Reloading is only safe when nothing in the outgoing tabs is live.
        // Dropping an `OpenTab` drops its `TerminalView`/`Chat` entities, and
        // `TerminalView::drop` tears its PTY down (`shutdown`) -- the exact
        // consequence `pane_close_needs_confirmation` already exists to gate
        // (F-TERM-08's held-close banner) and the same one
        // `evict_over_capacity_worktrees`'s own `status_of` closure exists to
        // prevent for the *control-registered* pane list. A worktree switch
        // must not silently kill live, needs-input, or just-errored work
        // (its last output may still be the thing the user is about to read)
        // just because its tabs happen to be the ones on screen, so this
        // reload reuses that exact predicate rather than a hand-rolled
        // subset of it. When the gate trips, today's behaviour (stale
        // content, nothing killed) is left in place rather than risking the
        // worse failure -- see docs/linux-rewrite/CENTER-PANE-DESYNC.md for
        // the real fix this stands in for (a genuine multi-worktree mount
        // model, which is a much larger change touching F-SID-14/
        // F-CORE-ACT-26/F-CHG-19/F-TERM-11, previously scoped out for the
        // same reason by the I3-tray-jump wave).
        if selected_path != old_path {
            let outgoing_is_safe = !self.tabs.iter().any(|tab| {
                self.tab_status(tab, cx)
                    .is_some_and(pane_close_needs_confirmation)
                    // F-TERM-10: catches a live foreground command the
                    // agent-activity check above cannot see at all (no
                    // recognized agent, no OSC title, no ACP) -- see
                    // `tab_has_live_foreground_process`'s own comment.
                    || self.tab_has_live_foreground_process(tab, cx)
            });
            if outgoing_is_safe {
                // The debounced `schedule_save` below (and every ordinary
                // mutation's `schedule_save`) always persists under whatever
                // `self.working_directory` is *at flush time* -- about to
                // become `selected_path`. Anything from the outgoing
                // worktree not yet flushed must be saved now, synchronously,
                // under its own directory, or it is silently lost rather
                // than merely delayed.
                let outgoing_layout = self.layout(cx);
                self.session.save_layout_now(&outgoing_layout);

                let restored = self.session.restore_tabs_for(&selected_path);
                let saved_session_refs = if self.settings.read(cx).snapshot().resume_agent_sessions
                {
                    self.session.load_session_refs()
                } else {
                    BTreeMap::new()
                };
                let (new_tabs, active) = restore_tabs(
                    &restored,
                    &selected_path,
                    window,
                    &mut self.activity,
                    &saved_session_refs,
                    cx,
                );
                Self::bind_terminal_tabs(&new_tabs, cx);
                for tab in &new_tabs {
                    tab.panes.for_each(&mut |_, content| {
                        if let TabContent::Chat(chat) = content {
                            Self::bind_chat(chat, cx);
                        }
                    });
                }
                self.tabs = new_tabs;
                self.next_tab_id = self.tabs.len();
                self.next_pane_id = next_pane_id(&self.tabs);
                self.active_tab = active.min(self.tabs.len().saturating_sub(1));
                self.rebuild_tab_machinery();
            }
        }

        let context = worktree_context(&self.project_catalog, &selected_path);
        self.working_directory = selected_path.clone();
        self.worktree_label = context.activity_label;
        self.terminal_breadcrumb = context.terminal_breadcrumb;
        self.rebind_changes_tabs(cx);
        // The old worktree's pane list is NOT wiped here. `sync_control_panes`
        // re-registers this window's own panes under the newly selected path
        // by pane id, and the registry is keyed by id, so those entries leave
        // the old path on their own. Wiping the path as well also deleted
        // panes that belong to the old worktree and were never this window's
        // — a control-socket `panel.create`, an agent registered from a hook
        // — and with them the status its sidebar row was showing. That is
        // how selecting a worktree used to erase a sibling's error.

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
            // Only the tab rows move with the selection. The old row's
            // activity is left alone: `sync_activity` below rewrites every
            // row from the one model, and blanking it here first meant a
            // worktree whose panes are still live lost its dot whenever the
            // model had nothing to say about it yet.
            self.sidebar.update(cx, |sidebar, cx| {
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
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .path_for_selector(selector)
            .ok_or_else(|| format!("unknown worktree: {selector}"))?;
        self.select_worktree(path, window, cx)?;
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

    /// I3-tray-jump: the `tray.jump` control method's handler. Resolves
    /// `selector` the same way `workspace.select` does, then drives
    /// [`Self::select_worktree_and_jump`] -- the identical call the real
    /// tray roster click makes -- and reports whether a tab was actually
    /// activated.
    fn control_select_worktree_and_jump(
        &mut self,
        selector: &str,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let path = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .path_for_selector(selector)
            .ok_or_else(|| format!("unknown worktree: {selector}"))?;
        let jump = self.select_worktree_and_jump(path, window, cx)?;
        let state = self
            .control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let workspace = state
            .current_workspace()
            .ok_or_else(|| "worktree selection did not produce a current workspace".to_string())?;
        let mut rows = vec![
            ("id".to_string(), workspace.id.clone()),
            ("project".to_string(), workspace.project.clone()),
            ("branch".to_string(), workspace.branch.clone()),
            ("path".to_string(), workspace.path.clone()),
        ];
        match jump {
            Some((tab_id, tab_title)) => {
                rows.push(("jumped".to_string(), "true".to_string()));
                rows.push(("tabId".to_string(), tab_id.to_string()));
                rows.push(("tabTitle".to_string(), tab_title));
            }
            None => rows.push(("jumped".to_string(), "false".to_string())),
        }
        Ok(rows)
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
        self.project_catalog.replace_projects(projects);
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
            self.right_panel
                .update(cx, |panel, cx| panel.clear_worktree(cx));
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
        self.select_worktree(snapshot.working_directory.clone(), Some(window), cx)?;

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
            // F-SET-04: resume_agent_sessions gates whether a saved native
            // session ref is honored on restore -- with it off, restored
            // agent panes must start fresh rather than silently resuming.
            let saved_session_refs = if self.settings.read(cx).snapshot().resume_agent_sessions {
                self.session.load_session_refs()
            } else {
                BTreeMap::new()
            };
            let (tabs, _) = restore_tabs_in_workspace(
                &restored,
                &self.working_directory,
                self.next_tab_id,
                self.next_pane_id,
                &mut self.activity,
                &saved_session_refs,
                window,
                cx,
            );
            Self::bind_terminal_tabs(&tabs, cx);
            // F-CHAT-14: Workspace::new binds every freshly-created Chat tab's
            // ChatEvent::OpenFile to add_file_tab via bind_chat; restored chat
            // tabs need the same binding or a restored session's Edit-tool file
            // opens silently no-op even with Follow on.
            for tab in &tabs {
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::Chat(chat) = content {
                        Self::bind_chat(chat, cx);
                    }
                });
            }
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

    /// F-WIN-10: raises a transient, floating notice that clears itself
    /// after `TOAST_DURATION` -- see the `toast` field docs for how this
    /// differs from `Sidebar::set_notice`'s persistent inline banner.
    fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        const TOAST_DURATION: Duration = Duration::from_secs(4);
        self.next_toast_id += 1;
        let id = self.next_toast_id;
        self.toast = Some(Toast {
            id,
            message: message.into(),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = this.update(cx, |workspace, cx| {
                // Only clear it if nothing newer replaced it in the
                // meantime -- an in-flight timer from a stale toast must
                // not wipe a toast raised after it.
                if workspace.toast.as_ref().is_some_and(|toast| toast.id == id) {
                    workspace.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = None;
        cx.notify();
    }

    /// F-WIN-11: the update toast's dismiss control -- the one action wired
    /// to a real transition (`UpdateEvent::Reset`) regardless of which
    /// visible state it is dismissed from, mirroring the reference
    /// `UpdaterModel.dismiss()`.
    fn dismiss_update_toast(&mut self, cx: &mut Context<Self>) {
        self.update_state = self.update_state.clone().transition(UpdateEvent::Reset);
        cx.notify();
    }

    /// The brand mark a tab is currently showing, resolved **live** on every
    /// sync rather than frozen at spawn.
    ///
    /// This is the port of `App/WorkspaceTabIcon.swift`, and its order is
    /// that view's order: a tab that knows its own agent (a chat, or a
    /// terminal Tiller launched an adapter into) keeps that identity, and
    /// otherwise the tab asks the activity model what its panes turned out
    /// to be — Swift's
    /// `tab.leafIds.compactMap { model.agentActivity.paneAgents[$0] }.first`,
    /// leaf order, first match wins.
    ///
    /// That second clause is the whole point. `agent_spawned` is only one of
    /// four ways a pane acquires an identity; the other three —
    /// `handle_title_change` (Layer B, OSC title), `process_identified`
    /// (Layer D, the foreground-process walk) and `register_agent_id` (a
    /// restored session) — all land *after* the tab exists, and every agent
    /// the user starts by hand arrives that way. Reading the field fixed at
    /// spawn meant those tabs kept the generic terminal glyph for their whole
    /// life, while the worktree row directly above them already showed the
    /// brand, because that row reads the same model live.
    ///
    /// **This is a pure read of `pane_agents` and nothing else.** Pane
    /// ownership — spawn-owned, title-owned (`titleOwnedPanes`),
    /// process-owned (`processOwnedPanes`) — decides when an entry appears
    /// and, more delicately, when it is allowed to be cleared; all of those
    /// rules stay inside `AgentActivityModel`, where the layers can be kept
    /// from wiping each other. An icon lookup must never participate in
    /// them, so this never writes, never registers, and never clears.
    fn tab_agent_mark(&self, tab: &OpenTab) -> Option<AgentMark> {
        // `WorkspaceTabIcon`'s order: the tab's own agent, then its panes'.
        // Falling through for the *id* as well as the icon matters for one
        // frame that really happens: `add_agent_tab` calls `agent_spawned`
        // (so the pane knows the agent), then `insert_terminal_tab`, which
        // syncs before the caller has set `OpenTab::agent_id`. Without the
        // fallback that sync draws the right silhouette in the unknown-agent
        // grey.
        let agent_id = tab.agent_id.clone().or_else(|| {
            tab.panes.leaf_ids().into_iter().find_map(|pane_id| {
                self.activity
                    .agent_id(&format!("pane-{pane_id}"))
                    .map(str::to_owned)
            })
        });
        // A tab that already carries a brand icon keeps it even when no id
        // resolves — it is still an agent tab, just an unnamed one, and the
        // neutral grey is what `AgentIcon.color(for: "")` gives.
        let icon = tab
            .agent_icon
            .or_else(|| Icon::for_agent_id(agent_id.as_deref()?))?;
        Some(AgentMark {
            icon,
            brand: agent_id
                .as_deref()
                .map_or(AgentBrandColor::Unknown, AgentBrandColor::for_agent_id),
        })
    }

    fn sync_activity(&mut self, cx: &mut Context<Self>) {
        // Layer E first: every view below reads the model, so the surfaces'
        // own facts have to be in it before any of them ask.
        self.sync_entity_evidence(cx);
        self.sync_control_panes(cx);
        let activity = self.activity_surfaces(cx);
        self.right_panel
            .update(cx, |panel, cx| panel.set_activity(activity, cx));

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
                agent: self.tab_agent_mark(tab),
            })
            .collect();
        if let Some(worktree_id) = self.sidebar_worktree_id(&self.working_directory) {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_worktree_tabs(worktree_id, sidebar_tabs, cx);
            });
        }
        self.sync_worktree_activity(cx);
    }

    /// F-CORE-ACT-17/18/22: everything a worktree row draws about its live
    /// agents, plus the order the rows themselves appear in.
    ///
    /// Three `AgentActivityModel` queries, one per fact, all keyed by the
    /// same pane-id set so they cannot disagree with each other or with the
    /// dot:
    ///
    /// * `status_for_panes`   → the status dot, for **every** row including
    ///   the selected one. The facts only the live entities know (a chat's
    ///   streaming state, a terminal's exit code) reach it through
    ///   `sync_entity_evidence`, which pushes them into the same model
    ///   rather than being read a second time here. Selecting a worktree
    ///   therefore cannot change the status its row shows — which it did,
    ///   in both directions, while the selected row read its own source.
    /// * `agent_id_for_panes` → the tint of the running indicator, exactly
    ///   as `WorktreeStatusGlyph(status:agentId:)` uses `agentId` in the
    ///   Swift original: to colour the loader, never to replace the branch
    ///   glyph, which `App/SidebarView.swift:361` always draws.
    /// * `running_agent_ids`  → the row's trailing running-agents badge,
    ///   already de-duplicated and in `AgentCatalog` order.
    ///
    /// The row order is `AttentionSort::urgent_first` over the project's
    /// worktrees **in catalog order**, and catalog order *is* the user's
    /// manual order: a worktree drag is persisted by `reorder_sidebar`
    /// through `ProjectCatalog::reorder_worktrees` before the rows are
    /// rebuilt. That is precisely the list `urgent_first` exists for — only
    /// `error`/`needs-input` jump the queue, everything else stays where it
    /// was dragged. The other mode, `sorted` (the full
    /// error→needs-input→running→done order), belongs to the tray roster
    /// (`tray_roster_snapshot`), a list with no manual order to respect.
    fn sync_worktree_activity(&self, cx: &mut Context<Self>) {
        let mut projects: Vec<(usize, Vec<WorktreeActivity>)> = Vec::new();
        for (project_index, project) in self.project_catalog.projects().iter().enumerate() {
            let mut worktrees = Vec::new();
            for (worktree_index, worktree) in project.worktrees.iter().enumerate() {
                let row_id = project_index * 1000 + worktree_index + 1;
                let pane_ids: Vec<String> = self
                    .panes
                    .list_for(&worktree.path)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|pane| pane.id)
                    .collect();
                let refs: Vec<&str> = pane_ids.iter().map(String::as_str).collect();
                let status = self
                    .activity
                    .status_for_panes(&refs)
                    .map(activity_status_for_agent);
                worktrees.push(WorktreeActivity {
                    row_id,
                    status,
                    agent_brand: self
                        .activity
                        .agent_id_for_panes(&refs)
                        .map(AgentBrandColor::for_agent_id),
                    running: self
                        .activity
                        .running_agent_ids(&refs, &tiller_activity::CATALOG_IDS)
                        .into_iter()
                        .filter_map(AgentMark::for_agent_id)
                        .collect(),
                });
            }
            projects.push((project_index * 1000, worktrees));
        }

        self.sidebar.update(cx, |sidebar, cx| {
            for (project_row_id, worktrees) in &projects {
                for worktree in worktrees {
                    sidebar.set_worktree_activity(
                        worktree.row_id,
                        worktree.status,
                        worktree.agent_brand,
                        worktree.running.clone(),
                        cx,
                    );
                }
                let order: Vec<usize> =
                    tiller_activity::AttentionSort::urgent_first(worktrees, |worktree| {
                        worktree.status.and_then(agent_status_for_activity)
                    })
                    .into_iter()
                    .map(|worktree| worktree.row_id)
                    .collect();
                sidebar.set_worktree_order(*project_row_id, &order, cx);
            }
        });
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
        // F-CORE-ACT-20: real window-focus, not a hardcoded `true` -- see
        // `window_active`'s own doc comment for why this reads a
        // once-per-frame cache instead of a live `Window` query.
        if !NotificationPolicy::should_notify(
            transition.old,
            transition.new,
            self.window_active,
            visible,
        ) {
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
        let Some(mut payload) = self.activity.build_payload(
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
        // F-CORE-ACT-19: the title is `<agent> — <human worktree label>`,
        // not `<agent> — <status>` — the pane's transition status already
        // drives which notification fires at all (`NotificationPolicy`
        // above); it does not belong in the title text a second time.
        // `AgentActivityModel::build_payload` (tiller_activity, not owned by
        // this file) still emits the status-suffixed title, so it is
        // corrected here rather than left for the desktop notifier to show
        // verbatim.
        payload.title = format!("{agent_display_name} — {}", context.activity_label);
        post_desktop_notification(&payload);
    }

    /// F-CORE-DOM-07: on a running→done/needs-input transition, throttled
    /// auto-naming re-titles the owning chat tab from its own transcript —
    /// ported from `AppModel.requestAutoRename`. Scoped to chat tabs (the
    /// `TabContent::Chat` transcript is a real, already-wired signal); a
    /// terminal tab's file-based transcript source is a separate, larger
    /// port (`resolveFileTranscriptSource` in the Swift original) left out
    /// of this pass.
    fn request_auto_rename(&mut self, transition: &Transition, cx: &mut Context<Self>) {
        let Some(AgentStatus::Running) = transition.old else {
            return;
        };
        if !matches!(transition.new, AgentStatus::Done | AgentStatus::NeedsInput) {
            return;
        }
        if !self.settings.read(cx).snapshot().auto_naming {
            return;
        }
        let Some(pane_id) = transition
            .pane_id
            .strip_prefix("pane-")
            .and_then(|id| id.parse::<usize>().ok())
        else {
            return;
        };
        let Some(tab_index) = self.tabs.iter().position(|tab| tab.panes.contains(pane_id)) else {
            return;
        };
        if !self.tabs[tab_index].title_is_auto_named {
            return;
        }

        let mut transcript = None;
        self.tabs[tab_index]
            .panes
            .for_each(&mut |leaf_id, content| {
                if leaf_id == pane_id
                    && let TabContent::Chat(chat) = content
                {
                    transcript = Some(chat.read(cx).transcript_for_resume());
                }
            });
        let Some(transcript) = transcript else {
            return;
        };
        if transcript.trim().is_empty() {
            return;
        }

        let tab_id = self.tabs[tab_index].id;
        let now = Instant::now();
        let transcript_len = transcript.chars().count();
        let throttle = self.auto_naming_throttle.entry(tab_id).or_default();
        if !throttle.should_request(now, transcript_len) {
            return;
        }
        throttle.record_request(now, transcript_len);

        let selected_id = self.settings.read(cx).snapshot().summarizer_agent.id();
        let tab_agent_id = self.tabs[tab_index].agent_id.clone();
        let prompt = auto_naming_prompt(&transcript);
        let commands = summarizer_candidate_commands(selected_id, tab_agent_id.as_deref(), &prompt);
        if commands.is_empty() {
            return;
        }
        let worktree_path = self.working_directory.to_string_lossy().into_owned();
        cx.spawn(async move |this, cx| {
            for command in commands {
                let worktree_path = worktree_path.clone();
                let title = cx
                    .background_executor()
                    .spawn(async move {
                        run_summarizer_command(&command, &worktree_path, AUTO_NAMING_TIMEOUT)
                    })
                    .await;
                let Some(title) = title else { continue };
                let _ = this.update(cx, |workspace, cx| {
                    workspace.apply_auto_title(tab_id, title, cx);
                });
                return;
            }
        })
        .detach();
    }

    /// Applies a summarizer-generated title, unless the tab was renamed by
    /// the user (or already auto-renamed) while the process was in flight.
    fn apply_auto_title(&mut self, tab_id: usize, title: String, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return;
        };
        if !tab.title_is_auto_named {
            return;
        }
        tab.title = title;
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
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
        // F-TERM-PTY-08: every tab placement transition (MoveTabToOtherPane,
        // MoveTabToCurrentPane, "Move to New Pane", and tab reordering all
        // funnel through here) is a real seam moment -- record each terminal
        // pane's current placement so the cache stays a true mirror of the
        // pane tree, not just of the specific moves the row names.
        let tab_ids: Vec<usize> = self.tabs.iter().map(|tab| tab.id).collect();
        for tab_id in tab_ids {
            self.track_terminal_panes_in_cache(tab_id);
        }
    }

    /// F-TERM-PTY-08: mirrors every terminal leaf in `tab_id`'s pane tree
    /// into `terminal_pane_cache`, addressed by the same `terminal-{pane_id}`
    /// content id `bind_terminal` stamps into each pane's `TerminalIdentity`.
    /// Uses `move_within_worktree` when the content id is already tracked
    /// (the normal case for a real move) and falls back to `insert` the
    /// first time a given pane is seen -- exercising both halves of the
    /// cache's own contract rather than only ever inserting.
    fn track_terminal_panes_in_cache(&mut self, tab_id: usize) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let group_id = tab.group_id;
        let mut panes: Vec<(usize, Entity<TerminalView>)> = Vec::new();
        tab.panes.for_each(&mut |pane_id, content| {
            if let TabContent::Terminal { view } = content {
                panes.push((pane_id, view.clone()));
            }
        });
        if panes.is_empty() {
            return;
        }
        let worktree_id = self.working_directory.to_string_lossy().into_owned();
        for (pane_id, view) in panes {
            let content_id = format!("terminal-{pane_id}");
            let placement = format!("group-{group_id}-pane-{pane_id}");
            if !self
                .terminal_pane_cache
                .move_within_worktree(&content_id, &worktree_id, placement.clone())
            {
                self.terminal_pane_cache
                    .insert(worktree_id.clone(), placement, content_id, view);
            }
        }
    }

    /// F-TERM-02: keeps `empty_pane_prompts` in step with which pane groups
    /// currently hold zero tabs. Called every render (like `sync_activity`)
    /// rather than only from `rebuild_tab_machinery`/`apply_tab_machinery`,
    /// so it also covers a group created by `add_group` before any tab has
    /// landed in it. A group that already has an entry is left alone (its
    /// `TerminalPromptEvent` subscription must survive across renders, or
    /// clicking "New Terminal" the second time in a session would no-op);
    /// a group that regained a tab has its entry dropped, which also drops
    /// its `TerminalView` entity and the subscription tied to it.
    fn sync_empty_pane_prompts(&mut self, cx: &mut Context<Self>) {
        let empty_group_ids: Vec<usize> = self
            .tab_machinery
            .groups()
            .iter()
            .filter(|group| group.tabs.is_empty())
            .map(|group| group.id)
            .collect();
        self.empty_pane_prompts
            .retain(|group_id, _| empty_group_ids.contains(group_id));
        for group_id in empty_group_ids {
            if self.empty_pane_prompts.contains_key(&group_id) {
                continue;
            }
            let prompt = cx.new(|cx| TerminalView::empty_prompt(cx));
            cx.subscribe(&prompt, move |workspace, _, event: &TerminalPromptEvent, cx| {
                workspace.handle_empty_pane_prompt(group_id, event.action, cx);
            })
            .detach();
            self.empty_pane_prompts.insert(group_id, prompt);
        }
    }

    /// Reassigns `active_group` without requiring the group to already own a
    /// tab -- `TabMachinery::select_tab` refuses that, since it is meant for
    /// picking a tab, not just a pane. Rebuilding through `TabMachinery::new`
    /// with the same groups is the only way to do this from `main.rs`
    /// without adding a new public method to `tab_machinery.rs`, which this
    /// wave does not own.
    fn activate_group(&mut self, group_id: usize) {
        let groups = self.tab_machinery.groups().to_vec();
        if let Ok(machinery) = TabMachinery::new(groups, group_id) {
            self.tab_machinery = machinery;
        }
    }

    /// Handles a click on one of the empty-pane prompt's two actions (see
    /// `sync_empty_pane_prompts`), mirroring the Swift reference's
    /// `stripModel.onActivateGroup(); stripModel.onNewTab()`
    /// (`App/PaneEmptyStateView.swift:24-26`): make the clicked pane's group
    /// active, then create a tab in it. `add_terminal_tab` reads
    /// `tab_machinery.active_group()` to decide which group a new tab joins,
    /// so `activate_group` must run first and land before this returns --
    /// there is no `Window` here to defer through, but neither call needs one.
    fn handle_empty_pane_prompt(
        &mut self,
        group_id: usize,
        action: TerminalPromptAction,
        cx: &mut Context<Self>,
    ) {
        self.activate_group(group_id);
        match action {
            TerminalPromptAction::NewTerminal => self.add_terminal_tab("Terminal", cx),
            TerminalPromptAction::NewTerminalWithCommand => {
                if let Ok(mut actions) = self.pending_actions.lock() {
                    actions.push(WorkspaceAction::OpenNewTabPalette);
                }
            }
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
                ChangesTabActionEvent::OpenDiff(path) => {
                    workspace.add_changes_tab(Some(path.clone()), cx)
                }
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

    /// Attaches a fresh, empty pane group and moves the selected tab into
    /// it. This is the only production path that grows `tab_machinery`
    /// beyond a single group -- everywhere else groups are inherited from
    /// existing tabs' `group_id`, so without this the "Move to Other Pane"
    /// family of actions could never have a second pane to target.
    fn move_selected_tab_to_new_pane(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self
            .tab_menu_tab
            .or_else(|| self.tabs.get(self.active_tab).map(|tab| tab.id))
        else {
            return;
        };
        let new_group_id = self
            .tab_machinery
            .groups()
            .iter()
            .map(|group| group.id)
            .max()
            .map_or(1, |max_id| max_id + 1);
        let mut machinery = self.tab_machinery.clone();
        if !machinery.add_group(new_group_id) {
            return;
        }
        if machinery
            .move_tab(tab_id, MoveTarget::Group(new_group_id))
            .is_err()
        {
            return;
        }
        self.move_selected_tab_with_machinery(MoveTarget::Group(new_group_id), machinery, cx);
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
        let persistence_id = session::new_tab_id(&self.working_directory, self.next_tab_id);
        // F-CHAT-34/F-PER-01: every chat tab is launched with durable
        // transcript persistence (database path + this tab's own id + its
        // worktree id) so completed turns are saved as they settle and the
        // Chat History menu has real sessions to list.
        let database_path = session::database_path();
        let worktree_id = session::persisted_worktree_id(&self.working_directory);
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
                let tab_id = persistence_id.clone();
                cx.new(|cx| {
                    Chat::launch_with_command_and_persistence(
                        command,
                        cwd,
                        database_path,
                        tab_id,
                        worktree_id,
                        cx,
                    )
                })
            }
            None => {
                let tab_id = persistence_id.clone();
                cx.new(|cx| Chat::launch_with_persistence(database_path, tab_id, worktree_id, cx))
            }
        };
        let composer_focus = chat.focus_handle(cx);
        Self::bind_chat(&chat, cx);
        // A chat pane is an agent pane: register its identity the same way
        // `register_restored_agent` does for a chat restored from a session.
        // Without this a freshly opened chat was invisible to the one
        // activity model — no identity, and so no place for its streaming
        // state to land either.
        register_restored_agent(&mut self.activity, self.next_pane_id, agent_id.as_deref());
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            persistence_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon,
            agent_id,
            session_state: SessionTabState::with_root(self.next_pane_id),
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Chat(chat)),
            focused_pane: self.next_pane_id,
            title_is_auto_named: true,
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
        let persistence_id = session::new_tab_id(&self.working_directory, self.next_tab_id);
        let chat = cx.new(|cx| {
            let mut chat = Chat::launch_with_command(command, cwd, cx);
            chat.restore_transcript(&transcript, cx);
            chat
        });
        let composer_focus = chat.focus_handle(cx);
        Self::bind_chat(&chat, cx);
        register_restored_agent(&mut self.activity, pane_id, agent_id.as_deref());
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            persistence_id,
            group_id: self.tab_machinery.active_group(),
            title,
            kind: TabKind::AgentChat,
            agent_icon,
            agent_id,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Chat(chat)),
            focused_pane: pane_id,
            title_is_auto_named: true,
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
        self.add_terminal_tab_with_shell_and_agent(title, shell, agent_icon, None, cx);
    }

    /// Like `add_terminal_tab_with_shell`, but also threads the agent id
    /// through to `insert_terminal_tab` so the very first `schedule_save`
    /// snapshot already carries it. F-AGENT-OPENCODE-01: setting
    /// `tab.agent_id` on the pushed tab *after* `insert_terminal_tab` has
    /// already called `schedule_save(cx)` persists a layout snapshot with
    /// `agent_id: None` — `layout(cx)` is computed eagerly at schedule time,
    /// not lazily at write time, so a restart before any later save relaunches
    /// the pane as plain shell instead of the agent command.
    fn add_terminal_tab_with_shell_and_agent(
        &mut self,
        title: impl Into<String>,
        shell: TerminalShell,
        agent_icon: Option<Icon>,
        agent_id: Option<String>,
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
        self.insert_terminal_tab_with_agent(title, terminal, agent_icon, agent_id, cx);
    }

    fn insert_terminal_tab(
        &mut self,
        title: impl Into<String>,
        terminal: Entity<TerminalView>,
        agent_icon: Option<Icon>,
        cx: &mut Context<Self>,
    ) {
        self.insert_terminal_tab_with_agent(title, terminal, agent_icon, None, cx);
    }

    fn insert_terminal_tab_with_agent(
        &mut self,
        title: impl Into<String>,
        terminal: Entity<TerminalView>,
        agent_icon: Option<Icon>,
        agent_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.next_tab_id;
        let pane_id = self.next_pane_id;
        let persistence_id = session::new_tab_id(&self.working_directory, tab_id);
        Self::bind_terminal(&terminal, tab_id, pane_id, cx);
        self.tabs.push(OpenTab {
            id: tab_id,
            persistence_id,
            group_id: self.tab_machinery.active_group(),
            title: title.into(),
            kind: TabKind::Terminal,
            agent_icon,
            agent_id,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Terminal { view: terminal }),
            focused_pane: pane_id,
            title_is_auto_named: true,
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
        Self::subscribe_file_view(&view, cx);
        let persistence_id = session::new_tab_id(&self.working_directory, self.next_tab_id);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            persistence_id,
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
            title_is_auto_named: true,
        });
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        self.rebuild_tab_machinery();
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
    }

    fn add_changes_tab(&mut self, focus_path: Option<PathBuf>, cx: &mut Context<Self>) {
        let changes = cx.new(|cx| ChangesTab::new(self.working_directory.clone(), cx));
        Self::subscribe_changes_tab(&changes, cx);
        // F-CHG-13: OpenDiff(path) expects the Changes tab to do something
        // path-specific with that file, not just open the generic multi-file
        // view -- expand (and un-collapse) whichever section carries it.
        if let Some(path) = focus_path {
            changes.update(cx, |tab, cx| tab.focus_path(&path, cx));
        }
        let persistence_id = session::new_tab_id(&self.working_directory, self.next_tab_id);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            persistence_id,
            group_id: self.tab_machinery.active_group(),
            title: "Changes".to_string(),
            kind: TabKind::Diff,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(self.next_pane_id),
            panes: PaneNode::leaf(self.next_pane_id, TabContent::Changes(changes)),
            focused_pane: self.next_pane_id,
            title_is_auto_named: true,
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
        let persistence_id = session::new_tab_id(&self.working_directory, self.next_tab_id);
        self.tabs.push(OpenTab {
            id: self.next_tab_id,
            persistence_id,
            group_id: self.tab_machinery.active_group(),
            title: "Browser".to_string(),
            kind: TabKind::Browser,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(pane_id),
            panes: PaneNode::leaf(pane_id, TabContent::Browser(browser.clone())),
            focused_pane: pane_id,
            title_is_auto_named: true,
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
            // F-CTRL-BROWSER-02: every other surface-opening control method
            // (surface.changes.open, chat.open, settings.open) refuses to
            // create a surface without a current workspace; browser.open
            // must not be the one exception that leaves a live browser tab
            // behind while workspace.current still reports "no current
            // workspace".
            if !self.has_current_worktree() {
                return Err("no current workspace".to_string());
            }
            let initial_url = params
                .get("url")
                .or_else(|| params.get("address"))
                .filter(|url| !url.trim().is_empty())
                .ok_or_else(|| "browser.open requires a non-empty url".to_string())?;
            // F-BRW-04: reject an invalid address up front instead of
            // silently falling back to https://example.com and reporting
            // ok:true with no error, which is what BrowserSurface::new
            // does internally when handed bad input.
            normalize_address(initial_url)
                .map_err(|error| format!("browser.open failed: {error}"))?;
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
            // F-CTRL-BROWSER-03: read-only status, no navigation side
            // effect — the counterpart to browser.navigate's write path.
            "browser.get" => {
                let state = surface.state();
                Ok(vec![
                    ("url".to_string(), state.address().to_string()),
                    ("title".to_string(), state.page_title().to_string()),
                    ("loading".to_string(), state.is_loading().to_string()),
                    ("canGoBack".to_string(), state.can_go_back().to_string()),
                    (
                        "canGoForward".to_string(),
                        state.can_go_forward().to_string(),
                    ),
                    (
                        "error".to_string(),
                        state.error().unwrap_or_default().to_string(),
                    ),
                ])
            }
            // F-CTRL-BROWSER-05: WebKit's navigation finishes asynchronously
            // off a GTK main-loop callback that only runs when something
            // pumps GTK events (tiller_ui's BrowserSurface does this itself
            // on a 16ms timer while mounted). Spinning here without pumping
            // would just burn the timeout and always report `loading`
            // unchanged, so this drives the same process-global GTK main
            // loop forward directly rather than trusting the timer to win
            // the race before the caller's own timeout.
            "browser.wait" => {
                let timeout = params
                    .get("timeoutMs")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_millis)
                    .unwrap_or(Duration::from_secs(5));
                let deadline = Instant::now() + timeout;
                loop {
                    // Linux only: WebKitGTK's async load only progresses when
                    // something pumps the process-global GTK main loop (see
                    // the comment above). macOS's WKWebView and Windows'
                    // WebView2 drive their own event loops with no
                    // equivalent pump to call here -- see
                    // docs/linux-rewrite/PORTABILITY.md's "Browser child
                    // attach" row -- so this loop just polls
                    // `surface.state()` on those platforms.
                    #[cfg(target_os = "linux")]
                    while gtk::events_pending() {
                        gtk::main_iteration_do(false);
                    }
                    if !surface.state().is_loading() || Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let state = surface.state();
                Ok(vec![
                    ("url".to_string(), state.address().to_string()),
                    ("title".to_string(), state.page_title().to_string()),
                    ("loading".to_string(), state.is_loading().to_string()),
                    ("timedOut".to_string(), state.is_loading().to_string()),
                ])
            }
            "browser.navigate" => {
                let address = params
                    .get("url")
                    .or_else(|| params.get("address"))
                    .or_else(|| params.get("href"))
                    .ok_or_else(|| "browser.navigate requires a non-empty url".to_string())?;
                // F-BRW-06/F-BRW-07: an agent-driven navigation to an
                // origin without a standing grant must stop for an
                // Allow/Deny doorhanger instead of navigating straight
                // through — this is the only production trigger for
                // `request_permission`, which otherwise has zero callers.
                if surface.state().agent_driving() {
                    let normalized_target = normalize_address(address)
                        .map_err(|error| format!("{method} failed: {error}"))?;
                    if let Some(origin) = browser_origin_of(&normalized_target)
                        && !surface.state().is_origin_allowed(&origin)
                    {
                        surface.request_permission(&origin);
                        return Ok(vec![
                            ("permission".to_string(), "requested".to_string()),
                            ("origin".to_string(), origin),
                        ]);
                    }
                }
                surface
                    .submit_address(address)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                let normalized = surface.state().address().to_string();
                // F-BRW-04: normalize_address only validates syntax; probe
                // reachability with a short bounded timeout so a
                // does-not-exist host or a closed port surfaces a visible
                // error instead of a silent ok:true with a blank title.
                if let Err(error) = probe_host_reachable(&normalized) {
                    surface.record_navigation_error(error.clone());
                    return Err(format!("{method} failed: {error}"));
                }
                let state = surface.state();
                Ok(vec![
                    ("url".to_string(), state.address().to_string()),
                    ("title".to_string(), state.page_title().to_string()),
                ])
            }
            // F-CTRL-BROWSER-06: runs arbitrary JS in the page and returns
            // its result. Companion to browser.console below.
            "browser.eval" => {
                let script = params
                    .get("script")
                    .or_else(|| params.get("expression"))
                    .filter(|script| !script.trim().is_empty())
                    .ok_or_else(|| "browser.eval requires a non-empty script".to_string())?;
                let timeout = params
                    .get("timeoutMs")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_millis)
                    .unwrap_or(Duration::from_secs(5));
                let result = surface
                    .evaluate_script(script, timeout)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                Ok(vec![("result".to_string(), result)])
            }
            // F-CTRL-BROWSER-06: reads back the console-message buffer the
            // page's own console.log/warn/error/info/debug calls have been
            // appending to since navigation started (see
            // CONSOLE_CAPTURE_SCRIPT in tiller_ui::browser).
            "browser.console" => {
                let timeout = params
                    .get("timeoutMs")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_millis)
                    .unwrap_or(Duration::from_secs(5));
                let result = surface
                    .evaluate_script("JSON.stringify(window.__tillerConsole || [])", timeout)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                Ok(vec![("messages".to_string(), result)])
            }
            // F-CTRL-BROWSER-04: a structural (accessibility-tree-shaped)
            // snapshot of the page, distinct from a pixel screenshot (which
            // this build has no capture path for — see the comment on
            // BROWSER_CAPABILITIES above). Reuses the same
            // evaluate_script/GTK-pump plumbing browser.eval and
            // browser.console already rely on, so it inherits their real
            // "Browser child is unavailable" failure mode rather than a
            // blanket pre-rejection.
            "browser.snapshot" => {
                let timeout = params
                    .get("timeoutMs")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_millis)
                    .unwrap_or(Duration::from_secs(5));
                let result = surface
                    .evaluate_script(BROWSER_SNAPSHOT_SCRIPT, timeout)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                Ok(vec![("snapshot".to_string(), result)])
            }
            "browser.act" => {
                if let Some(driving) = params.get("driving").or_else(|| params.get("agentDriving"))
                {
                    let driving = matches!(driving.as_str(), "1" | "true" | "yes");
                    surface.set_agent_driving(driving);
                    return Ok(vec![("driving".to_string(), driving.to_string())]);
                }
                // F-CTRL-BROWSER-05: click/fill/type/press/scroll, run as JS
                // through evaluate_script. browser_request_error already
                // validated the verb/selector/text combination synchronously
                // before this queued action was ever dispatched.
                let verb = params.get("verb").map(String::as_str).unwrap_or_default();
                let selector = params.get("selector").map(String::as_str);
                let text = params
                    .get("text")
                    .or_else(|| params.get("value"))
                    .map(String::as_str);
                let script = browser_act_script(verb, selector, text)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                let timeout = params
                    .get("timeoutMs")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_millis)
                    .unwrap_or(Duration::from_secs(5));
                surface
                    .evaluate_script(&script, timeout)
                    .map_err(|error| format!("{method} failed: {error}"))?;
                Ok(vec![("verb".to_string(), verb.to_string())])
            }
            // F-PER-08: allow_permission/deny_permission previously had no
            // caller except the doorhanger's GPUI on_click closures, so a
            // pending browser.navigate permission request (see
            // "browser.navigate" above) could only be resolved by an actual
            // mouse click — the browser-origin half of this row's grant
            // roundtrip had no reachable route at all. This mirrors the
            // doorhanger buttons exactly: resolving it is what makes
            // `newly_allowed`'s scan of `allowed_origins()` (in the render
            // loop) pick the grant up and call
            // `session.save_browser_origin_grant`.
            "browser.permission" => {
                let action = params.get("action").map(String::as_str).unwrap_or("allow");
                let resolved = match action {
                    "allow" => surface.allow_permission(),
                    "deny" => surface.deny_permission(),
                    other => {
                        return Err(format!(
                            "browser.permission action '{other}' must be allow or deny"
                        ));
                    }
                };
                let origin = resolved.ok_or_else(|| {
                    "browser.permission: no pending permission request".to_string()
                })?;
                Ok(vec![
                    ("origin".to_string(), origin),
                    ("action".to_string(), action.to_string()),
                ])
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
        self.add_terminal_tab_with_shell_and_agent(
            adapter.display_name(),
            shell,
            Some(agent_icon),
            Some(adapter.id().to_string()),
            cx,
        );
        self.sync_activity(cx);
    }

    fn open_action(&mut self, action: NewTabAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            NewTabAction::NewChat => self.add_chat_tab(window, None, cx),
            NewTabAction::NewTerminal => self.add_terminal_tab("Terminal", cx),
            NewTabAction::NewChanges => self.add_changes_tab(None, cx),
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
        window: Option<&mut Window>,
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
                self.select_worktree(path, window, cx)?;
            } else if !self.has_current_worktree() {
                // The requested worktree is already the live shell's
                // directory, but a prior `worktree.close` cleared
                // `ControlState::current` without navigating this shell
                // anywhere else. An explicit, valid selector for exactly
                // this worktree re-marks it current rather than being
                // rejected as if no worktree existed — the full
                // `select_worktree` teardown (status bar, right panel,
                // pane eviction) is unneeded and would wrongly evict this
                // worktree's own live panes.
                self.control_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .select_worktree(&path);
            }
        }
        if !self.has_current_worktree() {
            return Err("no current workspace".to_string());
        }
        // This is the same typed action used by the New-tab UI callback.
        self.add_changes_tab(None, cx);
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

    /// Resolves the persisted tab id used by `surface.chat.*` to the one
    /// mounted `Chat` entity that GPUI renders. There is deliberately no
    /// fallback ACP session: a socket request either reaches this surface or
    /// fails honestly.
    fn control_chat_surface(&self, surface_id: &str) -> Result<Entity<Chat>, String> {
        let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.persistence_id == surface_id && tab.kind == TabKind::AgentChat)
        else {
            return Err(format!("unknown chat surface: {surface_id}"));
        };
        let mut chat = None;
        tab.panes.for_each(&mut |_, content| {
            if chat.is_none()
                && let TabContent::Chat(entity) = content
            {
                chat = Some(entity.clone());
            }
        });
        chat.ok_or_else(|| format!("chat surface is not mounted: {surface_id}"))
    }

    fn control_chat_result(
        surface_id: &str,
        snapshot: ChatControlSnapshot,
    ) -> Vec<(String, String)> {
        vec![
            ("surfaceId".to_string(), surface_id.to_string()),
            ("status".to_string(), snapshot.status),
            ("composerText".to_string(), snapshot.composer_text),
            ("queuedText".to_string(), snapshot.queued_text),
            (
                "transcript".to_string(),
                tiller_control::protocol::rows::encode(&snapshot.transcript),
            ),
        ]
    }

    fn control_chat_read(
        &self,
        surface_id: &str,
        cx: &Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        let chat = self.control_chat_surface(surface_id)?;
        Ok(Self::control_chat_result(
            surface_id,
            chat.read(cx).control_snapshot(),
        ))
    }

    fn handle_chat_action(
        &mut self,
        action: ChatControlAction,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        match action {
            ChatControlAction::Open { worktree } => {
                if let Some(worktree) = worktree {
                    self.control_select_worktree(&worktree, window, cx)?;
                } else if self
                    .control_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .current_workspace()
                    .is_none()
                {
                    return Err("no current workspace".to_string());
                }
                let surface_id = self
                    .tabs
                    .iter()
                    .find(|tab| tab.kind == TabKind::AgentChat)
                    .map(|tab| tab.persistence_id.clone())
                    .ok_or_else(|| "no rendered chat surface in current workspace".to_string())?;
                self.control_chat_read(&surface_id, cx)
            }
            ChatControlAction::Compose { surface_id, text } => {
                let chat = self.control_chat_surface(&surface_id)?;
                let snapshot = chat.update(cx, |chat, cx| {
                    chat.control_compose(&text, cx);
                    chat.control_snapshot()
                });
                // F-CORE-WSP-08: without this, a composed-but-unsent draft
                // lives only in the live `Chat` entity — `layout()` would
                // capture it correctly, but nothing schedules that capture
                // until an unrelated mutation (rename, tab switch, …)
                // happens to run one first, so the draft would silently not
                // survive a restart that comes right after composing it.
                self.schedule_save(cx);
                Ok(Self::control_chat_result(&surface_id, snapshot))
            }
            ChatControlAction::Send { surface_id, text } => {
                let chat = self.control_chat_surface(&surface_id)?;
                let snapshot = chat.update(cx, |chat, cx| {
                    chat.control_send(&text, cx);
                    chat.control_snapshot()
                });
                Ok(Self::control_chat_result(&surface_id, snapshot))
            }
            ChatControlAction::Permission {
                surface_id,
                request_id,
                option_id,
            } => {
                let chat = self.control_chat_surface(&surface_id)?;
                let snapshot = chat.update(cx, |chat, cx| {
                    chat.control_permission(request_id, &option_id, cx)?;
                    Ok::<_, String>(chat.control_snapshot())
                })?;
                Ok(Self::control_chat_result(&surface_id, snapshot))
            }
            ChatControlAction::Stop { surface_id } => {
                let chat = self.control_chat_surface(&surface_id)?;
                let snapshot = chat.update(cx, |chat, cx| {
                    chat.control_stop(cx);
                    chat.control_snapshot()
                });
                Ok(Self::control_chat_result(&surface_id, snapshot))
            }
            ChatControlAction::Read { surface_id } => self.control_chat_read(&surface_id, cx),
        }
    }

    fn active_tab_mut(&mut self) -> Option<&mut OpenTab> {
        self.tabs.get_mut(self.active_tab)
    }

    fn select_pane(&mut self, pane_id: usize, window: Option<&mut Window>, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab_mut()
            && tab.panes.contains(pane_id)
        {
            tab.focused_pane = pane_id;
            let tab_id = tab.id;
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
            // F-TERM-PTY-08 (`TerminalPaneCache::focus`): a fresh pane may
            // not be tracked yet (nothing has moved it), so record its
            // placement first -- otherwise `focus` would silently no-op on
            // the very first click, and the cache's `restore_focus` would
            // never have anything to return for a pane nobody ever moved.
            self.track_terminal_panes_in_cache(tab_id);
            let worktree_id = self.working_directory.to_string_lossy().into_owned();
            self.terminal_pane_cache
                .focus(&worktree_id, &format!("terminal-{pane_id}"));
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
        self.split_focused_terminal_with_placement(direction, SplitPlacement::After, window, cx);
    }

    /// F-TERM-SPLIT-01: the control socket's `pane.split` used to always
    /// call the placement-less `split_focused_terminal`, so `direction:
    /// "left"`/`"up"` silently landed the new pane on the opposite side
    /// (`SplitPlacement::After`, the only placement that path could ever
    /// produce) despite the direction itself parsing correctly. This mirrors
    /// `split_focused_terminal` but threads the placement through to
    /// `split_terminal_at_with_placement`, the same call the terminal
    /// context menu already uses correctly.
    fn split_focused_terminal_with_placement(
        &mut self,
        direction: SplitDirection,
        placement: SplitPlacement,
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
        self.split_terminal_at_with_placement(
            tab_id,
            focused_pane,
            direction,
            placement,
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            PaneNode::Leaf { id, content } => {
                let Some(content) = content else {
                    return div().size_full().into_any_element();
                };
                // F-TAB-11 (SoleTabInGroup half): only this render pass can
                // count the pane's tab-group membership -- `tiller_terminal`
                // has no route to `tab_machinery`. Pushed in on every
                // render rather than cached, since a tab moving groups (Move
                // to Other Pane, tab close, …) doesn't itself notify this
                // pane's own `TerminalView` entity.
                if let TabContent::Terminal { view } = content {
                    let sole_tab_in_group = self
                        .tab_machinery
                        .group_tabs(self.tabs[tab_index].group_id)
                        .is_none_or(|tabs| tabs.len() == 1);
                    view.update(cx, |terminal, cx| {
                        terminal.set_sole_tab_in_group(sole_tab_in_group);
                        cx.notify();
                    });
                }
                let pane_id = *id;
                let tab_id = self.tabs[tab_index].id;
                let entity_for_click = entity.clone();
                let entity_for_close = entity.clone();
                let surface = match content {
                    TabContent::Chat(chat) => div()
                        .id("pane-surface")
                        .debug_selector(|| "pane-surface".into())
                        .size_full()
                        .child(chat.clone())
                        .into_any_element(),
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
                    .debug_selector(|| "pane-leaf".into())
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
                    self.render_pane_tree(first, tab_index, entity.clone(), first_path, cx);
                let second_element =
                    self.render_pane_tree(second, tab_index, entity.clone(), second_path, cx);
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

    /// F-CORE-ACT-23 / F-TERM-08: the confirm-or-cancel prompt for a close
    /// that would kill live work.
    ///
    /// Drawn over the **workspace**, not inside the pane being closed. The
    /// Swift original is an `.alert` (`App/SidebarView.swift:601`), which is
    /// modal and therefore always on screen; the Rust banner used to render
    /// inside `render_pane_tree`, which only ever runs for each group's
    /// *active* tab. Closing an Activity row that belongs to a background
    /// tab therefore armed a prompt on a surface nobody could see: the close
    /// silently did nothing, and there was no drawn control to cancel it
    /// with either.
    ///
    /// The message names the state that made the close need confirming.
    /// A single hard-coded "has running work" described a failed agent as
    /// still working.
    fn render_pane_close_confirm(&self, theme: Theme, entity: Entity<Self>) -> Option<AnyElement> {
        let pending = self.pending_pane_close?;
        let confirm_entity = entity.clone();
        let cancel_entity = entity;
        let subject = if pending.whole_tab { "tab" } else { "pane" };
        let message = match pending.status {
            ActivityStatus::NeedsInput => {
                format!("This {subject} is waiting for input. Close anyway?")
            }
            ActivityStatus::Error => format!("This {subject}'s agent failed. Close anyway?"),
            // Running is the only remaining confirming state; done and idle
            // never reach here (`requires_close_confirmation`).
            _ => format!("This {subject} has running work. Close anyway?"),
        };
        Some(
            div()
                .id("pane-close-confirm")
                .debug_selector(|| "pane-close-confirm".into())
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .bg(gpui::black().opacity(0.82))
                .child(div().text_color(gpui::white()).child(message))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id("pane-close-confirm-close")
                                .debug_selector(|| "pane-close-confirm-close".into())
                                .cursor(gpui::CursorStyle::PointingHand)
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(theme.tab_error)
                                .text_color(gpui::white())
                                .on_click(move |_, _, cx| {
                                    confirm_entity.update(cx, |workspace, cx| {
                                        workspace.confirm_pending_pane_close(cx)
                                    });
                                })
                                .child("Close Anyway"),
                        )
                        .child(
                            div()
                                .id("pane-close-confirm-cancel")
                                .debug_selector(|| "pane-close-confirm-cancel".into())
                                .cursor(gpui::CursorStyle::PointingHand)
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(gpui::white().opacity(0.15))
                                .text_color(gpui::white())
                                .on_click(move |_, _, cx| {
                                    cancel_entity.update(cx, |workspace, cx| {
                                        workspace.cancel_pending_pane_close(cx)
                                    });
                                })
                                .child("Cancel"),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Renders one active tab surface per pane group. A group may be empty
    /// after its last tab was moved away; keeping that surface visible is
    /// deliberate because it gives Move Existing Tab a real destination and
    /// makes the F-TAB-13 empty state observable instead of silently deleting
    /// the pane.
    fn render_group_surfaces(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                        .debug_selector(|| "pane-group-surface".into())
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(self.render_pane_tree(
                            &self.tabs[tab_index].panes,
                            tab_index,
                            entity.clone(),
                            Vec::new(),
                            cx,
                        ))
                        .into_any_element()
                })
                .unwrap_or_else(|| {
                    if group.id == 0 && self.has_current_worktree() {
                        let new_terminal_entity = entity.clone();
                        div()
                            .id("empty-worktree")
                            .debug_selector(|| "empty-worktree".to_owned())
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(theme.spacing.card_gap)
                            .text_color(theme.meta)
                            .child(
                                IconElement::new(Icon::SquareTerminal, px(32.0))
                                    .text_color(theme.meta),
                            )
                            .child(
                                div()
                                    .text_size(theme.typography.headline)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.title)
                                    .child("No Terminals"),
                            )
                            .child("Open a new terminal to get started.")
                            .child(
                                div()
                                    .id("empty-worktree-new-terminal")
                                    .debug_selector(|| "empty-worktree-new-terminal".to_owned())
                                    .mt(theme.spacing.titlebar_control_spacing)
                                    .px(theme.spacing.card_gap)
                                    .py(theme.spacing.titlebar_control_spacing)
                                    .rounded(theme.radii.control)
                                    .bg(theme.tab_focus_accent)
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.canvas)
                                    .hover(|style| style.bg(theme.accent))
                                    .on_click(move |_, _, cx| {
                                        new_terminal_entity.update(cx, |workspace, cx| {
                                            workspace.add_terminal_tab("Terminal", cx);
                                        });
                                    })
                                    .child("New Terminal"),
                            )
                            .into_any_element()
                    } else if let Some(prompt) = self.empty_pane_prompts.get(&group.id) {
                        // F-TERM-02: a pane group that lost its last tab
                        // (every other tab moved elsewhere, or a fresh split
                        // group awaiting its first tab) without the whole
                        // pane closing -- mount the real `TerminalView`
                        // empty-prompt surface instead of tearing the group
                        // down to a static label. `sync_empty_pane_prompts`
                        // guarantees an entry exists for every group with
                        // zero tabs before this renders.
                        div()
                            .id(format!("pane-group-empty-{}", group.id))
                            .debug_selector(|| "pane-group-empty".to_owned())
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(prompt.clone())
                            .into_any_element()
                    } else {
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
                    }
                });
            surfaces = surfaces.child(surface);
        }
        surfaces.into_any_element()
    }

    fn has_current_worktree(&self) -> bool {
        self.control_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_workspace()
            .is_some()
    }

    fn render_open_tab(
        tab: &OpenTab,
        // The tab's live agent identity (see
        // `TillerWorkspace::tab_agent_mark`) — resolved by the caller
        // because it needs the activity model, which this associated
        // function deliberately does not take.
        agent: Option<AgentMark>,
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
        let icon = tab_icon(tab.kind, is_file, agent.map(|agent| agent.icon));
        // An agent mark is drawn in that agent's brand, here, on the sidebar
        // tab row and in the worktree badge alike — the reference has one
        // `AgentIcon` view that every one of those three places draws, so a
        // mark looks the same wherever it appears. This port had drifted
        // into three different tints for the same mark, and the badge's was
        // `theme.tab_focus_accent` = `#E2795B`, Claude's own brand coral, so
        // a Codex mark was painted in Claude's colour.
        //
        // This is a deliberate, narrow divergence from a literal port:
        // Swift fills the Codex, OpenCode and Pi marks with `.primary` and
        // only Claude (`#D97757`) and omp (its gradient) with a brand. It
        // can afford to, because those are rendered assets; `IconElement`
        // tints one flat colour into a `currentColor` silhouette, so the
        // tint is the only channel a mark has. Chromatic assets still ignore
        // it — `Icon::is_chromatic`, omp's gradient — exactly as `OmpShape`
        // ignores any inherited tint.
        let glyph_color = if icon.is_agent_mark() {
            agent.map_or(theme.title, |agent| agent.brand.color())
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
            // F-TAB-14: double-click-to-rename was entirely unwired -- no
            // click-count handling existed anywhere in this render function.
            // `ClickEvent::Mouse`'s `up.click_count` is the same signal
            // `right_panel.rs`'s file rows and `titlebar.rs`'s drag area
            // already use to distinguish a double-click from a plain one;
            // routing it through the existing `on_click` (rather than adding
            // a sibling `on_mouse_down`) keeps this a single click-listener
            // on the tab, so it cannot race the close button's own
            // `on_click`/`cx.stop_propagation()` the way a second, separate
            // mouse-down listener on the same hitbox could.
            .on_click(move |event, window, cx| {
                let click_count = match event {
                    ClickEvent::Mouse(mouse) => mouse.up.click_count,
                    _ => 1,
                };
                entity.update(cx, |this, cx| {
                    if click_count >= 2 {
                        this.begin_tab_rename(id, window, cx);
                    } else {
                        this.select_tab(id, cx);
                    }
                });
            })
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

        items.push(TabContextItem::separator());
        if self.can_attach_tab_to_current_terminal(tab_id) {
            items.push(TabContextItem::enabled(
                "Attach to Current Terminal",
                "attach-to-current-terminal",
                TabContextAction::AttachToCurrentTerminal,
            ));
        } else {
            items.push(TabContextItem::disabled(
                "Attach to Current Terminal",
                "attach-to-current-terminal",
                TabContextAction::AttachToCurrentTerminal,
                "select another terminal tab",
            ));
        }

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
            items.push(TabContextItem::enabled(
                "Move to New Pane",
                "move-to-new-pane",
                TabContextAction::MoveToPane(usize::MAX),
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
        let menu = div()
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
            ));
        // Same shared defect as `render_overflow_menu` above: this popover is
        // positioned `top(TAB_BAR_HEIGHT)`, which places it squarely over
        // `#centre-surface`, a *later* sibling of the tab-bar row it is
        // nested under. Undeferred, tree-order painting put the
        // centre-surface on top of it every time, so the tab context menu
        // never appeared to a live right-click no matter how correct its
        // item logic was.
        deferred(menu)
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
            TabContextAction::MoveToPane(_) => {
                self.move_selected_tab_to_new_pane(cx);
            }
            TabContextAction::AttachToCurrentTerminal => {
                if let Some(tab_id) = self.tab_menu_tab {
                    self.attach_tab_to_current_terminal(tab_id, cx);
                }
            }
        }
    }

    /// Mirrors the Swift `workspaceCanAdoptPane` contract at the tab strip:
    /// the source must be another terminal-bearing tab, and the active tab
    /// must provide the current terminal that receives the live pane.
    fn can_attach_tab_to_current_terminal(&self, source_tab_id: usize) -> bool {
        let Some(destination) = self.tabs.get(self.active_tab) else {
            return false;
        };
        if destination.id == source_tab_id || !tab_has_terminal(destination) {
            return false;
        }
        self.tabs
            .iter()
            .find(|tab| tab.id == source_tab_id)
            .is_some_and(tab_has_terminal)
    }

    /// Moves the first terminal leaf from the source tab into a horizontal
    /// split beside the current terminal. The terminal entity itself moves —
    /// no PTY is restarted — and a source tab that becomes empty is removed
    /// without sending its still-live process a close signal.
    fn attach_tab_to_current_terminal(&mut self, source_tab_id: usize, cx: &mut Context<Self>) {
        if !self.can_attach_tab_to_current_terminal(source_tab_id) {
            return;
        }
        let Some(destination_tab_id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id == source_tab_id) else {
            return;
        };
        let source_pane = {
            let source = &self.tabs[source_index];
            let mut terminal_pane = None;
            source.panes.for_each(&mut |pane_id, content| {
                if terminal_pane.is_none() && matches!(content, TabContent::Terminal { .. }) {
                    terminal_pane = Some(pane_id);
                }
            });
            terminal_pane
        };
        let Some(source_pane) = source_pane else {
            return;
        };
        let source_will_be_empty = self.tabs[source_index].panes.leaf_ids().len() == 1;
        let Some(moved) = self.tabs[source_index].panes.take(source_pane) else {
            return;
        };
        let moved_terminal = moved.terminal();
        if source_will_be_empty {
            self.tabs.remove(source_index);
        } else {
            self.tabs[source_index]
                .session_state
                .pane_events
                .push(PaneEvent::Close { id: source_pane });
        }

        let Some(destination_index) = self
            .tabs
            .iter()
            .position(|tab| tab.id == destination_tab_id)
        else {
            return;
        };
        let destination = &mut self.tabs[destination_index];
        let Some(anchor) = destination.panes.first_id() else {
            return;
        };
        if !destination.panes.split_focused_with_placement(
            anchor,
            source_pane,
            SplitDirection::Horizontal,
            SplitPlacement::After,
            moved,
        ) {
            return;
        }
        destination.focused_pane = source_pane;
        destination
            .session_state
            .pane_events
            .push(PaneEvent::Split {
                focused: anchor,
                new_id: source_pane,
                direction: split_event_name(SplitDirection::Horizontal, SplitPlacement::After),
            });
        if let Some(terminal) = moved_terminal {
            Self::bind_terminal(&terminal, destination_tab_id, source_pane, cx);
        }
        self.rebuild_tab_machinery();
        self.active_tab = self
            .tabs
            .iter()
            .position(|tab| tab.id == destination_tab_id)
            .unwrap_or(destination_index);
        self.dismiss_tab_menu(cx);
        self.schedule_save(cx);
        self.sync_activity(cx);
        cx.notify();
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

    fn commit_tab_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.tab_rename.take() else {
            return;
        };
        let title = rename.draft.trim();
        if !title.is_empty()
            && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == rename.tab_id)
        {
            tab.title = title.to_owned();
            // F-CORE-DOM-07: a user-driven rename permanently opts this tab
            // out of automatic renaming, mirroring Swift's
            // `tab.titleIsAutoNamed = false` on manual rename.
            tab.title_is_auto_named = false;
            self.schedule_save(cx);
            self.sync_activity(cx);

            // F-CORE-WSP-04: the rename widget's own FocusHandle is dropped
            // with `rename` above, and nothing else claims keyboard focus —
            // without this, committing a rename left focus on no rendered
            // element, so the terminal/chat pane silently stopped receiving
            // key input until the user clicked back into it. Route the
            // real mutation through `LayoutCommand`/`classify_layout_command`
            // so the fix is driven by the general command classification
            // (`FocusIntent::Tab`), not a one-off hardcoded assumption.
            let command = tiller_project::LayoutCommand::Rename {
                tab: rename.tab_id.to_string(),
                title: title.to_owned(),
            };
            if tiller_project::classify_layout_command(&command).focus
                == tiller_project::FocusIntent::Tab
            {
                self.focus_tab_content(rename.tab_id, window, cx);
            }
        }
        cx.notify();
    }

    /// Focuses the given tab's currently-focused pane content, when it has
    /// a focusable surface (chat composer or terminal). Shared by rename
    /// commit/cancel so both leave keyboard input working without a click.
    fn focus_tab_content(&self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let focused_pane = tab.focused_pane;
        let mut focus_handle = None;
        tab.panes.for_each(&mut |id, content| {
            if id == focused_pane {
                focus_handle = match content {
                    TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                    TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                    TabContent::File { .. } | TabContent::Changes(_) | TabContent::Browser(_) => {
                        None
                    }
                };
            }
        });
        if let Some(focus_handle) = focus_handle {
            window.focus(&focus_handle, cx);
        }
    }

    fn handle_tab_rename_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "enter" | "return" => self.commit_tab_rename(window, cx),
            "escape" => {
                if let Some(rename) = self.tab_rename.take() {
                    self.focus_tab_content(rename.tab_id, window, cx);
                }
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
        // The tab strip lives in an earlier flex-col sibling of
        // `#centre-surface` (see `columns()`); this popover's own bounds
        // overflow below the strip into that sibling's area. Painted inline
        // it is a plain descendant of the strip, so it paints (and is
        // occluded) before `#centre-surface`'s later paint pass runs.
        // `deferred(...)` keeps its layout in place but defers painting
        // until after every ancestor, so it actually lands on top — the
        // same pattern already used for the new-tab menu in
        // `tiller_ui::tab_bar`.
        deferred(menu)
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
        // F-TAB-02 (P104 §Group 1): checking fit against `available_width -
        // overflow_width` unconditionally reserves room for the chevron even
        // when no chevron will ever be shown, so the strip flipped into
        // overflow mode before the tabs actually exceeded the real pixel
        // width. Decide overflow against the *full* width first (no
        // reservation); only once that says the strip truly doesn't fit do
        // we recompute how many tabs fit in the width that remains once the
        // chevron itself is carved out.
        let all_visible_count = visible_tab_count(&tab_widths, available_width, 0.0);
        let has_overflow = all_visible_count < group_tabs.len();
        let visible_count = if has_overflow {
            visible_tab_count(&tab_widths, available_width, overflow_width)
        } else {
            all_visible_count
        };
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
                self.tab_agent_mark(tab),
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
        cx: &mut Context<Self>,
        window: &Window,
    ) -> impl IntoElement {
        let mut columns = div().flex().flex_row().size_full();

        let centre_surface = if self.has_current_worktree() {
            div()
                .id("group-surfaces-wrapper")
                .debug_selector(|| "group-surfaces-wrapper".into())
                .relative()
                // P117: this wrapper declared no height at all. `flex_1` grows it along its
                // parent's main axis, which is a *row* — so it sized width and left height to
                // content. Everything below inherits that, and a chat pane whose content is a
                // virtualized `list()` has no intrinsic height, so the whole subtree collapsed to
                // MIN_SPLIT_PANE_SIZE. That is the 160px in the P117 measurements.
                .h_full()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .child(self.render_group_surfaces(*theme, entity.clone(), cx))
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
                })
                .into_any_element()
        } else {
            div()
                .id("no-worktree-selected")
                .debug_selector(|| "no-worktree-selected".to_owned())
                .flex_1()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(theme.spacing.card_gap)
                .text_color(theme.meta)
                .child(IconElement::new(Icon::SquareTerminal, px(32.0)).text_color(theme.meta))
                .child(
                    div()
                        .text_size(theme.typography.headline)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.title)
                        .child("No worktree selected"),
                )
                .child("Add a project, then select a worktree.")
                .into_any_element()
        };

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
                        .id("centre-surface")
                        .debug_selector(|| "centre-surface".into())
                        .flex_1()
                        .w_full()
                        .overflow_hidden()
                        .child(centre_surface),
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

    /// F-WIN-07: `ctrl-shift-o`, the Linux stand-in for `⇧⌘O`'s "History >
    /// Restore Previous Launch" -- the same path the titlebar's History
    /// button and the `session.restore` control door both drive.
    fn handle_restore_launch_snapshot(
        &mut self,
        _: &RestoreLaunchSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.restore_launch_snapshot(window, cx) {
            self.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_notice(
                    format!("[history] could not restore the previous launch: {error}"),
                    cx,
                )
            });
        }
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

    fn handle_close_pane(&mut self, _: &ClosePane, _window: &mut Window, cx: &mut Context<Self>) {
        self.request_close_focused_pane(cx);
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
                WindowCommand::RestoreLaunchSnapshot => {
                    window.dispatch_action(Box::new(RestoreLaunchSnapshot), cx)
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

    /// F-WIN-10: a floating, bottom-right, auto-dismissing notice -- the
    /// row's clause explicitly, unlike `Sidebar::set_notice`'s persistent
    /// inline banner (which stays silent unless the sidebar happens to be
    /// the visible surface). `None` when no toast is live, so this costs
    /// nothing in the common case.
    fn render_toast(&self, theme: Theme, entity: Entity<Self>) -> Option<AnyElement> {
        let toast = self.toast.clone()?;
        let dismiss_entity = entity;
        Some(
            div()
                .id("workspace-toast")
                .debug_selector(|| "workspace-toast".to_owned())
                .absolute()
                .bottom(px(20.0))
                .right(px(20.0))
                .max_w(px(360.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.hairline)
                .bg(theme.card_fill)
                .shadow_lg()
                .text_size(theme.typography.footnote)
                .text_color(theme.title)
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    dismiss_entity.update(cx, |workspace, cx| workspace.dismiss_toast(cx));
                })
                .child(toast.message)
                .into_any_element(),
        )
    }

    /// F-WIN-11: the update toast -- renders every user-visible
    /// `UpdateState` (all but `Idle`, which is silence) with its message
    /// and, where the reference names one, its action. No Linux update
    /// transport is wired yet to back "Download"/"Retry" (see
    /// `UpdateState`'s doc-comment, deliberately transport-independent),
    /// so those render muted and inert -- the same "not yet wired"
    /// convention `Titlebar`'s cluster seams use rather than a
    /// live-looking dead control. Dismiss is the one action every state
    /// actually supports, and is fully wired to `UpdateEvent::Reset`.
    fn render_update_toast(&self, theme: Theme, entity: Entity<Self>) -> Option<AnyElement> {
        let (message, action_label, accent): (String, Option<&'static str>, gpui::Rgba) =
            match &self.update_state {
                UpdateState::Idle => return None,
                UpdateState::Checking => {
                    ("Checking for updates…".to_string(), None, theme.subtitle)
                }
                UpdateState::Available { version } => (
                    format!("Tiller {version} is available"),
                    Some("Download"),
                    theme.tab_focus_accent,
                ),
                UpdateState::Downloading { progress_percent } => (
                    format!("Downloading Tiller… {progress_percent}%"),
                    None,
                    theme.tab_focus_accent,
                ),
                UpdateState::Installing => (
                    "Installing update…".to_string(),
                    None,
                    theme.tab_focus_accent,
                ),
                UpdateState::UpToDate => ("Tiller is up to date".to_string(), None, theme.tab_done),
                UpdateState::Failed { message } => (
                    format!("Update failed: {message}"),
                    Some("Retry"),
                    theme.tab_error,
                ),
            };
        let progress_percent = match &self.update_state {
            UpdateState::Downloading { progress_percent } => Some(*progress_percent),
            _ => None,
        };
        let dismiss_entity = entity;
        Some(
            div()
                .id("update-toast")
                .debug_selector(|| "update-toast".to_owned())
                .absolute()
                .top(px(20.0))
                .right(px(20.0))
                .max_w(px(320.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.hairline)
                .bg(theme.card_fill)
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.0))
                        .child(
                            div()
                                .id("update-toast-message")
                                .debug_selector(|| "update-toast-message".to_owned())
                                .flex_1()
                                .text_size(theme.typography.footnote)
                                .text_color(accent)
                                .child(message),
                        )
                        .child(
                            div()
                                .id("update-toast-dismiss")
                                .debug_selector(|| "update-toast-dismiss".to_owned())
                                .text_size(theme.typography.footnote)
                                .text_color(theme.meta)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    dismiss_entity.update(cx, |workspace, cx| {
                                        workspace.dismiss_update_toast(cx)
                                    });
                                })
                                .child("×"),
                        ),
                )
                .when_some(progress_percent, |this, progress_percent| {
                    this.child(
                        div()
                            .id("update-toast-progress")
                            .debug_selector(|| "update-toast-progress".to_owned())
                            .w_full()
                            .h(px(5.0))
                            .rounded(px(3.0))
                            .bg(theme.primary_pill_bg)
                            .child(
                                div()
                                    .h(px(5.0))
                                    .rounded(px(3.0))
                                    .bg(theme.tab_focus_accent)
                                    .w(px(280.0 * (progress_percent as f32 / 100.0))),
                            ),
                    )
                })
                .when_some(action_label, |this, label| {
                    this.child(
                        div()
                            .id("update-toast-action")
                            .debug_selector(|| "update-toast-action".to_owned())
                            .text_size(theme.typography.footnote)
                            .text_color(theme.subtitle)
                            .child(label),
                    )
                })
                .into_any_element(),
        )
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
        // Layer E is polled, not pushed: a chat's streaming flag and a
        // terminal's exit status live in their own entities and emit no
        // event this workspace subscribes to. Refreshing here means the
        // frame about to be drawn reads a model that is already current,
        // without any view reaching past it to the entities — and it costs
        // one map comparison per pane. The sidebar is re-pushed only when
        // something actually changed, so this cannot loop.
        if self.sync_entity_evidence(cx) {
            self.sync_worktree_activity(cx);
        }
        // F-CORE-ACT-20: same "polled, not pushed" reasoning as the comment
        // above -- `Window::is_window_active` is a real, portable GPUI call
        // (every platform's own window backs it; nothing linux-specific is
        // needed to read it), but the callbacks that actually decide
        // whether to notify (`post_activity_notification`, off Layer B/C/D
        // activity transitions) carry no `Window`. `render` does, every
        // frame, so the value is cached here for those to read.
        self.window_active = window.is_window_active();
        // F-SID-19: if nothing at all holds keyboard focus this frame (e.g.
        // the previously-focused surface -- a terminal, a sidebar row --
        // was just unmounted, and nothing claimed focus in its place, as
        // happens landing on a worktree with zero tabs), GPUI's key
        // dispatch falls back to the true window root, which sits *above*
        // every `on_action`/`capture_key_down` this workspace registers on
        // its own root element. Every global keybinding (`ctrl-t` among
        // them) then reaches nothing at all, silently. Reclaiming focus
        // onto `root_focus` -- tracked on this same root element every
        // frame, below, so it is always part of the frame this call is
        // building -- keeps this workspace's own element tree always
        // holding *some* live focus target, so those bindings stay
        // reachable. Unlike `restore_focus_pending` below (which targets a
        // handle -- the sidebar's -- that is not guaranteed to already be
        // in this exact frame and so must wait a frame), `root_focus` is
        // this element's own handle and needs no such deferral.
        if window.focused(cx).is_none() {
            window.focus(&self.root_focus, cx);
        }
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
        self.sync_empty_pane_prompts(cx);

        if self.show_settings {
            return div()
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .bg(theme.canvas)
                .track_focus(&self.root_focus)
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
            .track_focus(&self.root_focus)
            .capture_key_down(cx.listener(Self::handle_root_key_down))
            .on_action(cx.listener(Self::handle_new_terminal_tab))
            .on_action(cx.listener(Self::handle_open_file))
            .on_action(cx.listener(Self::handle_restore_launch_snapshot))
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
            .children(self.render_pane_close_confirm(theme, cx.entity()))
            .children(self.render_toast(theme, cx.entity()))
            .children(self.render_update_toast(theme, cx.entity()))
    }
}

/// F-SID-11: `comments` is control-state metadata (`worktree.set`), keyed by
/// the worktree's path string -- not part of the git-derived
/// `ProjectCatalog` -- so a caller with nothing to look up (a fresh
/// instance, a test fixture) can just pass an empty map.
fn sidebar_projects_with_comments(
    catalog: &ProjectCatalog,
    comments: &BTreeMap<String, String>,
) -> Vec<SidebarProject> {
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
                    comment: comments
                        .get(&worktree.path.to_string_lossy().into_owned())
                        .cloned(),
                })
                .collect(),
        })
        .collect()
}

fn sidebar_projects(catalog: &ProjectCatalog) -> Vec<SidebarProject> {
    sidebar_projects_with_comments(catalog, &BTreeMap::new())
}

fn sidebar_project_identities(
    catalog: &ProjectCatalog,
) -> Vec<(
    String,
    Option<String>,
    tiller_ui::project_identity::ProjectIcon,
)> {
    catalog
        .projects()
        .iter()
        .map(|project| {
            let settings = catalog.project_settings(&project.id);
            let icon = tiller_ui::project_identity::ProjectIcon::from_persisted_parts(
                &settings.icon_kind,
                settings.icon_value.as_deref(),
                settings.color_hex.as_deref(),
            );
            (project.id.clone(), settings.display_name, icon)
        })
        .collect()
}

/// F-PRJ-17/F-PRJ-18: the persisted per-project worktree-base pin and
/// location override, pushed into the sidebar the same way
/// `sidebar_project_identities` pushes icons.
fn sidebar_project_worktree_defaults(
    catalog: &ProjectCatalog,
) -> Vec<(String, Option<String>, Option<String>)> {
    catalog
        .projects()
        .iter()
        .map(|project| {
            let settings = catalog.project_settings(&project.id);
            (
                project.id.clone(),
                settings.default_worktree_base,
                settings.worktree_location_override,
            )
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

fn register_restored_agent(
    activity: &mut AgentActivityModel,
    root_pane_id: usize,
    agent_id: Option<&str>,
) {
    if let Some(agent_id) = agent_id {
        activity.register_agent_id(&format!("pane-{root_pane_id}"), agent_id);
    }
}

/// `~/.claude`, honouring the same home resolution used to locate a native
/// Claude session's transcript file — never overridden per-worktree.
fn claude_config_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude")
}

/// `$CODEX_HOME`, falling back to `~/.codex` when unset or empty — the same
/// precedence Codex's own CLI and `tiller_usage`'s credential lookup use.
fn codex_home() -> PathBuf {
    if let Ok(dir) = std::env::var("CODEX_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".codex")
}

/// Splits saved agent session references into resumable/prunable via
/// `AgentSessionRestorePlan`, keyed by the stable `"pane-N"` string that
/// tillerctl hooks report notifications under (mirrors `restore_tabs`'
/// per-tab `pane_id` derivation, so lookups against this map align exactly).
/// A resumable reference is additionally gated by `AgentSessionValidator`:
/// a session id whose on-disk transcript/rollout file has since vanished is
/// treated as unresumable rather than handed to `--resume`/`resume`, which
/// would otherwise fail or silently start a fresh session anyway.
fn resumable_session_refs(
    restored: &RestoredSession,
    saved_refs: &BTreeMap<String, String>,
    worktree_path: &str,
) -> BTreeMap<String, String> {
    let mut refs = Vec::new();
    let mut live_ids = HashSet::new();
    for (tab_index, tab) in restored.tabs.iter().enumerate() {
        let root_id = restored
            .tab_states
            .get(tab_index)
            .and_then(|state| state.root_id)
            .unwrap_or(tab_index);
        let pane_key = format!("pane-{root_id}");
        if let (Some(agent_id), Some(session_ref)) =
            (tab.agent_id.as_deref(), saved_refs.get(&pane_key))
        {
            refs.push(AgentSessionRef::new(
                TerminalContentId::new(pane_key.clone()),
                agent_id,
                session_ref.clone(),
            ));
        }
        live_ids.insert(TerminalContentId::new(pane_key));
    }
    let plan = AgentSessionRestorePlan::plan(&refs, &live_ids);
    plan.resumable
        .into_iter()
        .filter(|reference| {
            tiller_agents::AgentSessionValidator::is_likely_valid(
                &reference.agent_id,
                &reference.session_ref,
                worktree_path,
                claude_config_dir(),
                codex_home(),
            )
        })
        .map(|reference| {
            (
                reference.content_id.as_str().to_string(),
                reference.session_ref,
            )
        })
        .collect()
}

/// Builds the shell an agent-owned restored terminal pane should launch:
/// resumes a validated native session when `resumable` has one on record for
/// `pane_key`, otherwise starts fresh — the same `prepare` + `command` path
/// `add_agent_tab` uses for a brand-new pane. Returns `None` for a tab with
/// no agent identity (an ordinary terminal), which callers fall back to a
/// plain shell for.
fn restored_agent_shell(
    agent_id: Option<&str>,
    pane_key: &str,
    worktree_path: &Path,
    resumable: &BTreeMap<String, String>,
) -> Option<TerminalShell> {
    let agent_id = agent_id?;
    let adapter = AGENT_CATALOG
        .iter()
        .find(|adapter| adapter.id() == agent_id)?;
    let tillerctl_path = resolve_tillerctl_for_process().ok()?;
    let tillerctl_path = tillerctl_path.to_string_lossy().into_owned();
    let worktree_path = worktree_path.to_string_lossy().into_owned();
    if let Err(error) = adapter.prepare(&worktree_path, pane_key, &tillerctl_path) {
        eprintln!(
            "failed to prepare {} in {worktree_path}: {error}",
            adapter.display_name()
        );
    }
    let command = resumable
        .get(pane_key)
        .and_then(|session_ref| {
            adapter.resume_command(&worktree_path, pane_key, &tillerctl_path, session_ref)
        })
        .unwrap_or_else(|| adapter.command(&worktree_path, pane_key, &tillerctl_path));
    let shell_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    Some(TerminalShell::WithArguments {
        program: shell_program,
        args: vec!["-lc".to_string(), command],
    })
}

/// Rebuilds the shell's tabs from a restored session: chat tabs get a fresh
/// `Chat` entity wired to the durable transcript (F-PER-01/F-PERSIST-DB-05 —
/// its prior turns are restored into `entries` and later turns keep saving),
/// terminal tabs get a fresh terminal in the restored worktree directory,
/// and persisted pane events rebuild split trees. Returns the tabs and the
/// active index.
fn restore_tabs(
    restored: &RestoredSession,
    working_directory: &std::path::Path,
    mut window: Option<&mut Window>,
    activity: &mut AgentActivityModel,
    saved_session_refs: &BTreeMap<String, String>,
    cx: &mut App,
) -> (Vec<OpenTab>, usize) {
    let resumable = resumable_session_refs(
        restored,
        saved_session_refs,
        &working_directory.to_string_lossy(),
    );
    // F-PER-01/F-PERSIST-DB-05: a restored chat tab previously got a
    // Chat::launch_with_command with `persistence: None` — this doc's own
    // "identity, not transcript" comment described that as intentional, but
    // it means every completed turn after the *first* restart is silently
    // unsaved (persist_settled_transcript's `self.persistence.as_ref()?`
    // early-returns), and the transcript never restores into `entries`
    // either. `session::database_path()` is the same deterministic path
    // `main()` already opened at startup; `worktree_id` matches what
    // `add_chat_tab` computes for a freshly created chat, so restored and
    // freshly-opened chats persist under the same key convention.
    let database_path = session::database_path();
    let worktree_id = session::persisted_worktree_id(working_directory);
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
        register_restored_agent(activity, pane_id, agent_id.as_deref());
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(|cx| {
                let mut chat = Chat::launch_with_command_and_persistence(
                    command.expect("chat restoration always has a fallback command"),
                    working_directory.to_path_buf(),
                    database_path.clone(),
                    tab.id.clone(),
                    worktree_id.clone(),
                    cx,
                );
                // F-CORE-WSP-08: an unsent draft survives a restart.
                if !tab_state.chat_draft.is_empty() {
                    chat.control_compose(&tab_state.chat_draft, cx);
                }
                chat
            })),
            "terminal" => {
                let cwd = working_directory.to_path_buf();
                let pane_key = format!("pane-{pane_id}");
                let agent_id = tab.agent_id.clone();
                let view = cx.new(|cx| {
                    match restored_agent_shell(agent_id.as_deref(), &pane_key, &cwd, &resumable) {
                        Some(shell) => match TerminalView::with_shell(&cwd, shell, cx) {
                            Ok(view) => view,
                            Err(error) => TerminalView::failed(
                                &cwd,
                                TerminalShell::System,
                                format!("{error:#}"),
                                cx,
                            ),
                        },
                        None => match TerminalView::new(&cwd, cx) {
                            Ok(view) => view,
                            Err(error) => TerminalView::failed(
                                &cwd,
                                TerminalShell::System,
                                format!("{error:#}"),
                                cx,
                            ),
                        },
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
            persistence_id: tab.id.clone(),
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
            title_is_auto_named: true,
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
    activity: &mut AgentActivityModel,
    saved_session_refs: &BTreeMap<String, String>,
    window: &mut Window,
    cx: &mut Context<TillerWorkspace>,
) -> (Vec<OpenTab>, usize) {
    let resumable = resumable_session_refs(
        restored,
        saved_session_refs,
        &working_directory.to_string_lossy(),
    );
    // F-PER-01/F-PERSIST-DB-05: see the matching comment in restore_tabs —
    // this is the same restore path taken by restore_launch_snapshot when a
    // worktree without a mounted host gets pane-only tabs replayed in.
    let database_path = session::database_path();
    let worktree_id = session::persisted_worktree_id(working_directory);
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
        register_restored_agent(activity, pane_id, agent_id.as_deref());
        let content = match tab.kind.as_str() {
            "chat" => TabContent::Chat(cx.new(|cx| {
                let mut chat = Chat::launch_with_command_and_persistence(
                    command.expect("chat restoration always has a fallback command"),
                    working_directory.to_path_buf(),
                    database_path.clone(),
                    tab.id.clone(),
                    worktree_id.clone(),
                    cx,
                );
                // F-CORE-WSP-08: an unsent draft survives a restart.
                if !tab_state.chat_draft.is_empty() {
                    chat.control_compose(&tab_state.chat_draft, cx);
                }
                chat
            })),
            "terminal" => {
                let cwd = working_directory.to_path_buf();
                let pane_key = format!("pane-{pane_id}");
                let agent_id = tab.agent_id.clone();
                let view = cx.new(|cx| {
                    match restored_agent_shell(agent_id.as_deref(), &pane_key, &cwd, &resumable) {
                        Some(shell) => match TerminalView::with_shell(&cwd, shell, cx) {
                            Ok(view) => view,
                            Err(error) => TerminalView::failed(
                                &cwd,
                                TerminalShell::System,
                                format!("{error:#}"),
                                cx,
                            ),
                        },
                        None => match TerminalView::new(&cwd, cx) {
                            Ok(view) => view,
                            Err(error) => TerminalView::failed(
                                &cwd,
                                TerminalShell::System,
                                format!("{error:#}"),
                                cx,
                            ),
                        },
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
            persistence_id: tab.id.clone(),
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
            title_is_auto_named: true,
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
        if current.iter().any(|current| {
            (!tab.id.is_empty() && current.id == tab.id)
                || (tab.id.is_empty() && current.title == tab.title && current.kind == tab.kind)
        }) {
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
        ollama_show_in_bar: settings.ollama_show_in_bar,
        refresh_interval: settings.refresh_interval_min.clamp(1, 60) as i32,
        opencode_workspace_id_override: settings.opencode_workspace_id_override,
        // F-SET-22 has no AppSettings field yet; do not pretend this UI-only
        // picker is persisted until its schema follow-up lands.
        agent_colors: SettingsSnapshot::default().agent_colors,
        translucency: settings.translucency,
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
        ollama_show_in_bar: snapshot.ollama_show_in_bar,
        refresh_interval_min: i64::from(snapshot.refresh_interval.clamp(1, 60)),
        opencode_workspace_id_override: snapshot.opencode_workspace_id_override,
        translucency: snapshot.translucency,
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
        let project_catalog =
            ProjectCatalog::from_restored(restored_catalog.projects, restored_catalog.settings);
        // F-AGENT-SAFE-02: repair a stale leading `tillerctl` path in every
        // known worktree's Claude hook config at launch — before any pane is
        // opened, so a worktree with no pane reopened this run still gets
        // fixed rather than waiting on a fresh `prepare()` that may never
        // come. `migrate_file` touches only the path, never pane ids,
        // arguments, unrelated hooks, or other JSON keys; an unchanged or
        // unparseable file is a no-op.
        if let Ok(tillerctl_path) = resolve_tillerctl_for_process() {
            let tillerctl_path = tillerctl_path.to_string_lossy().into_owned();
            for project in project_catalog.projects() {
                for worktree in &project.worktrees {
                    let settings_path = worktree.path.join(".claude").join("settings.local.json");
                    if settings_path.is_file() {
                        tiller_agents::ClaudeHookMigrator::migrate_file(
                            &settings_path,
                            &tillerctl_path,
                        );
                    }
                }
            }
        }
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
        let pending_for_titlebar = pending_actions.clone();
        let control_actions = Arc::new(Mutex::new(Vec::<ControlAction>::new()));
        let mut control_state_seed =
            ControlState::from_catalog(&project_catalog, &working_directory);
        // F-CTRL-WORK-01: a comment set via worktree.set before the
        // previous shutdown must still be there after a restart.
        if let Ok(database) = AppDatabase::open(&database_path) {
            match database.worktrees() {
                Ok(worktrees) => {
                    let comments: BTreeMap<String, String> = worktrees
                        .into_iter()
                        .filter_map(|worktree| Some((worktree.path, worktree.comment?)))
                        .collect();
                    control_state_seed.apply_persisted_comments(&comments);
                }
                Err(error) => {
                    eprintln!("[session] failed to load persisted worktree comments: {error}");
                }
            }
        }
        let control_state = Arc::new(Mutex::new(control_state_seed));
        let panes = Arc::new(PaneRegistry::new());
        let notifications = Arc::new(Mutex::new(Vec::<ControlNotification>::new()));
        let saved_session_refs = session_store.load_session_refs();
        let session_refs = Arc::new(Mutex::new(saved_session_refs.clone()));
        // F-SET-04: only honor saved native session refs for the initial
        // restore when the resume_agent_sessions setting is on; the shared
        // `session_refs` above stays populated for the control server
        // (session.ref), which is a separate concern from restore-time use.
        let saved_session_refs_for_restore = if saved_settings.resume_agent_sessions {
            saved_session_refs.clone()
        } else {
            BTreeMap::new()
        };
        let control_environment: BTreeMap<String, String> = std::env::vars().collect();
        let socket_path = PathBuf::from(tiller_control::default_socket_path(&control_environment));
        let socket_info = ControlSocketInfo::new(socket_path);
        let control_handler = Arc::new(
            AppControlHandler::new(
                control_state.clone(),
                control_actions.clone(),
                panes.clone(),
                notifications,
                session_refs,
                Some(session_store.clone()),
                socket_info.clone(),
            )
            .with_database_path(database_path.clone()),
        );
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
        // F-PERSIST-DB-06: the account-identity cache lives in the same
        // database file the session store already opened above.
        let database_path_for_settings = database_path.clone();
        // Route the resolved socket path into the settings snapshot so the
        // General screen can display the path the live socket listens on
        // (P23: the socket row must show the real path, not a template).
        let settings_snapshot = {
            let mut snapshot = settings_snapshot_from_app_settings(saved_settings);
            snapshot.socket_path = socket_info.path.to_string_lossy().into_owned();
            // F-SET-22: AppSettings has no agent_colors column yet, so the
            // persisted choices are overlaid from the session store's
            // key-value table (see SessionStore::load_agent_color_ids)
            // rather than round-tripping through settings_snapshot_from_app_settings.
            let saved_colors = session_store.load_agent_color_ids();
            for (index, slot) in snapshot.agent_colors.iter_mut().enumerate() {
                if let Some(id) = saved_colors.get(&index) {
                    *slot = tiller_ui::settings::AgentAccentColor::parse(id);
                }
            }
            snapshot
        };
        let panes_for_window = panes.clone();
        let workspace_for_quit = Arc::new(Mutex::new(None::<Entity<TillerWorkspace>>));
        let workspace_slot = workspace_for_quit.clone();

        // F-USE-04/F-USE-05/F-WIN-08: registers the StatusNotifierItem
        // once, for the process's lifetime. `None` (no SNI host answered
        // -- headless CI, a WM with no tray applet) leaves the app running
        // exactly as it did before this existed.
        let (tray_roster, tray_requests, tray_handle) = match tray::spawn() {
            Some((roster, requests, handle)) => (Some(roster), Some(requests), Some(handle)),
            None => (None, None, None),
        };

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
                let mut activity_model = AgentActivityModel::new();
                let (tabs, active_tab) = restore_tabs(
                    &restored,
                    &working_directory,
                    Some(window),
                    &mut activity_model,
                    &saved_session_refs_for_restore,
                    cx,
                );
                let activity = tabs
                    .iter()
                    .map(|tab| {
                        // Restored panes get their identity from
                        // `register_agent_id` inside `restore_tabs`, i.e.
                        // after the tab exists — the same after-the-fact
                        // path Layers B and D use — so this first Activity
                        // list reads the model rather than the tab's field.
                        let icon = tab_icon(
                            tab.kind,
                            tab_has_file(tab),
                            tab.agent_icon.or_else(|| {
                                tab.panes.leaf_ids().into_iter().find_map(|pane_id| {
                                    activity_model
                                        .agent_id(&format!("pane-{pane_id}"))
                                        .and_then(Icon::for_agent_id)
                                })
                            }),
                        );
                        ActivitySurface::new(
                            icon,
                            tab.title.clone(),
                            activity_label.clone(),
                            ActivityStatus::Idle,
                        )
                    })
                    .collect();
                // F-SID-11: at real startup `control_state` was already
                // seeded from the durable `worktree.comment` column above
                // (`apply_persisted_comments`), but that seed only reached
                // `ControlState` itself -- the initial `Sidebar` entity built
                // a few lines below used to be constructed via bare
                // `sidebar_projects`, which always passes an empty comment
                // map (see `sidebar_projects`'s doc comment). That made a
                // restored comment invisible until the next `worktree.set`
                // triggered `refresh_sidebar` (which does look the comment up
                // via `Workspace::sidebar_projects`). Look the persisted
                // comments up here too so the very first paint already shows
                // them.
                let comments_for_sidebar: BTreeMap<String, String> = control_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .workspaces
                    .iter()
                    .filter(|workspace| !workspace.comment.is_empty())
                    .map(|workspace| (workspace.path.clone(), workspace.comment.clone()))
                    .collect();
                let catalog_for_sidebar =
                    sidebar_projects_with_comments(&project_catalog, &comments_for_sidebar);
                let identities_for_sidebar = sidebar_project_identities(&project_catalog);
                let pending_for_chat_agent = pending_for_tab_bar.clone();
                let pending_for_agent_settings = pending_for_tab_bar.clone();
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
                        .on_open_agent_settings(move || {
                            if let Ok(mut actions) = pending_for_agent_settings.lock() {
                                actions.push(WorkspaceAction::OpenAgentSettings);
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
                        .with_database_path(database_path_for_settings.clone())
                        .on_install_skill({
                            let pending_actions = pending_for_settings.clone();
                            move |command| {
                                if let Ok(mut actions) = pending_actions.lock() {
                                    actions.push(WorkspaceAction::InstallSkill(command));
                                }
                            }
                        })
                        .on_install_agent({
                            let pending_actions = pending_for_settings.clone();
                            move |agent_id, command| {
                                if let Ok(mut actions) = pending_actions.lock() {
                                    actions
                                        .push(WorkspaceAction::InstallAgent { agent_id, command });
                                }
                            }
                        })
                        .on_change(move |snapshot| {
                            control_socket_for_settings
                                .set_enabled(snapshot.control_socket_enabled);
                            // F-SET-22: persist the per-agent accent colours
                            // alongside the rest of the settings snapshot;
                            // app_settings_from_snapshot still drops them
                            // (no AppSettings column yet).
                            for (index, color) in snapshot.agent_colors.iter().enumerate() {
                                session_store_for_settings.save_agent_color_id(index, color.id());
                            }
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
                    let sidebar = cx.new(|cx| {
                        let mut sidebar = Sidebar::from_projects(catalog_for_sidebar, cx);
                        for (id, display_name, icon) in identities_for_sidebar {
                            sidebar.set_project_identity(&id, display_name, icon, cx);
                        }
                        sidebar
                    });
                    TillerWorkspace::new(
                        cx.new(|cx| {
                            Titlebar::new(cx).on_history(move |_, _| {
                                // F-WIN-07: the History entry point --
                                // there is no in-window menu bar by
                                // design (`resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar`),
                                // so this cluster button is the surface's
                                // "History > Restore Previous Launch"
                                // clause; the shell's action queue is how
                                // every other titlebar/tab-bar seam
                                // reaches `TillerWorkspace` from outside
                                // its own render, see `pending_for_settings`.
                                if let Ok(mut actions) = pending_for_titlebar.lock() {
                                    actions.push(WorkspaceAction::RestoreLaunchSnapshot);
                                }
                            })
                        }),
                        sidebar,
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
                        activity_model,
                        tray_roster.clone(),
                        tray_requests.clone(),
                        tray_handle,
                        cx,
                    )
                });
                // F-WIN-08: the Swift original hides the window on close
                // via an `NSWindowDelegate` and reopens it from the Dock
                // icon or the menu-bar roster. `PlatformWindow` exposes no
                // hide/show pair on Linux, so this intercepts the close
                // request and minimizes instead -- the window (and every
                // pane's PTY) never actually closes either way.
                // `window.activate_window()` (see `tray::TrayRequest`'s
                // handlers above) is the re-show half.
                window.on_window_should_close(cx, |window, _cx| {
                    window.minimize_window();
                    false
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
            let restore_launch_snapshot = self.fired.clone();
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
                .on_action(cx.listener(move |_, _: &RestoreLaunchSnapshot, _, _| {
                    restore_launch_snapshot
                        .borrow_mut()
                        .push(WindowCommand::RestoreLaunchSnapshot);
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

    /// F-TERM-UI-02: a real `TerminalLinkEvent`, fed through the real
    /// `subscribe_terminal_link` wiring (not a reimplementation of its
    /// logic), must queue `WorkspaceAction::OpenBrowserLink` -- the same
    /// route `bind_chat`'s `ChatEvent::OpenLink` already uses to open
    /// tiller's own Browser tab -- not call out to an external browser.
    /// `terminal_links_are_only_routed_to_their_owning_pane` above already
    /// covers the pure pane-id filter; this covers the subscription that
    /// filter is wired into.
    #[gpui::test]
    fn a_terminal_link_click_queues_the_in_app_browser_tab_action(cx: &mut TestAppContext) {
        let workspace = cx.new(|cx| {
            let workspace = palette_test_workspace(cx);
            let terminal = match &workspace.tabs[0].panes {
                PaneNode::Leaf {
                    content: Some(TabContent::Terminal { view }),
                    ..
                } => view.clone(),
                _ => panic!("test tab 0 must be a terminal pane"),
            };
            terminal.update(cx, |terminal, _| {
                terminal.set_identity(TerminalIdentity::new("pane-0", "terminal-0"));
            });
            TillerWorkspace::subscribe_terminal_link(&terminal, 0, cx);
            terminal.update(cx, |_, cx| {
                cx.emit(TerminalLinkEvent {
                    target: TerminalIdentity::new("pane-0", "terminal-0"),
                    url: "https://example.test/from-terminal".to_string(),
                });
            });
            workspace
        });
        cx.run_until_parked();
        let queued_url = workspace.read_with(cx, |workspace, _| {
            let actions = workspace.pending_actions.lock().unwrap();
            actions.iter().find_map(|action| match action {
                WorkspaceAction::OpenBrowserLink(url) => Some(url.clone()),
                _ => None,
            })
        });
        assert_eq!(
            queued_url.as_deref(),
            Some("https://example.test/from-terminal"),
            "a terminal link click must queue OpenBrowserLink (the same in-app \
             Browser tab route bind_chat's ChatEvent::OpenLink uses)"
        );
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
            workspace.add_changes_tab(None, cx);
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
                persistence_id: format!("test-tab-{id}"),
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
                title_is_auto_named: true,
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
            AgentActivityModel::new(),
            None,
            None,
            None,
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
            persistence_id: "activity-terminal".into(),
            group_id: 0,
            title: "Terminal".into(),
            kind: TabKind::Terminal,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(0),
            panes: PaneNode::leaf(0, TabContent::Terminal { view: terminal }),
            focused_pane: 0,
            title_is_auto_named: true,
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
            AgentActivityModel::new(),
            None,
            None,
            None,
            cx,
        )
    }

    /// A workspace whose one project has three worktrees, so the sidebar
    /// carries a real manual order for F-CORE-ACT-22's `urgent_first` to
    /// move rows inside. The selected worktree (index 0) holds a *live*
    /// terminal rather than a failed one: a failed pane reports `Error`,
    /// which would outrank everything the test is trying to float.
    fn worktree_urgency_test_workspace(
        cx: &mut Context<TillerWorkspace>,
        root: &Path,
    ) -> TillerWorkspace {
        let worktrees: Vec<PathBuf> = (0..3)
            .map(|index| root.join(format!("wt-{index}")))
            .collect();
        // A live shell, not `TerminalView::failed`: a failed pane reports
        // `Error`, which outranks every status these tests try to float.
        let terminal = cx.new(|cx| {
            TerminalView::with_shell(
                &worktrees[0],
                TerminalShell::WithArguments {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), "sleep 60".into()],
                },
                cx,
            )
            .expect("spawn urgency test terminal")
        });
        let project_catalog = ProjectCatalog::from_projects(vec![session::CatalogProject {
            id: "urgency-project".into(),
            name: "Urgency Project".into(),
            root_path: worktrees[0].clone(),
            is_git: true,
            worktrees: worktrees
                .iter()
                .enumerate()
                .map(|(index, path)| session::CatalogWorktree {
                    branch: format!("branch-{index}"),
                    path: path.clone(),
                    is_primary: index == 0,
                })
                .collect(),
        }]);
        let working_directory = worktrees[0].clone();
        let tabs = vec![OpenTab {
            id: 0,
            persistence_id: "urgency-terminal".into(),
            group_id: 0,
            title: "Terminal".into(),
            kind: TabKind::Terminal,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::with_root(0),
            panes: PaneNode::leaf(0, TabContent::Terminal { view: terminal }),
            focused_pane: 0,
            title_is_auto_named: true,
        }];
        let pending_actions = Arc::new(Mutex::new(Vec::new()));
        let control_actions = Arc::new(Mutex::new(Vec::new()));
        let panes = Arc::new(PaneRegistry::new());
        let state = Arc::new(Mutex::new(ControlState::from_catalog(
            &project_catalog,
            &working_directory,
        )));
        let session = SessionStore::open(&root.join("tiller.sqlite"));
        TillerWorkspace::new(
            cx.new(Titlebar::new),
            cx.new(|cx| Sidebar::from_projects(sidebar_projects(&project_catalog), cx)),
            cx.new(TabBar::new),
            cx.new(|_| {
                StatusBar::new(UsageBarData {
                    branch: "branch-0".into(),
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
            "Urgency Project/branch-0".into(),
            "in test shell".into(),
            RestoredSession {
                working_directory,
                tabs: Vec::new(),
                tab_states: Vec::new(),
                diagnostics: Vec::new(),
            },
            AgentActivityModel::new(),
            None,
            None,
            None,
            cx,
        )
    }

    /// Kills every PTY a drawn workspace test spawned, so the test process
    /// does not leave `sleep` children behind.
    fn shutdown_workspace_terminals(
        workspace: &Entity<TillerWorkspace>,
        cx: &mut VisualTestContext,
    ) {
        let views = workspace.read_with(&cx.cx, |workspace, _| {
            let mut views = Vec::new();
            for tab in &workspace.tabs {
                tab.panes.for_each(&mut |_, content| {
                    if let TabContent::Terminal { view } = content {
                        views.push(view.clone());
                    }
                });
            }
            views
        });
        for view in views {
            view.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        }
        cx.run_until_parked();
    }

    fn urgency_test_root(tag: &str) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!(
            "tiller-worktree-urgency-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let worktrees: Vec<PathBuf> = (0..3)
            .map(|index| root.join(format!("wt-{index}")))
            .collect();
        for path in &worktrees {
            std::fs::create_dir_all(path).expect("create urgency worktree fixture");
        }
        (root, worktrees)
    }

    /// F-CORE-ACT-17 + F-CORE-ACT-18, drawn end to end through the app:
    /// `AgentActivityModel::agent_id_for_panes` decides which brand mark a
    /// worktree row draws, and `running_agent_ids` decides its trailing
    /// badge. Nothing here re-derives either from the pane list — the model
    /// is asked, and the answer is what appears on screen.
    #[gpui::test]
    async fn drawn_worktree_row_shows_the_identity_and_running_set_the_model_resolved(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("identity");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        // Row 3 is the third worktree. Give it two live agent panes: one
        // running Claude, one running Codex, plus a *done* OpenCode pane
        // that must not reach the running badge.
        let third = worktrees[2].clone();
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace
                .panes
                .set_external(
                    &third,
                    vec![
                        PaneInfo {
                            id: "pane-90".into(),
                            tab: "Claude Code".into(),
                            title: "Claude Code".into(),
                            agent: "claude".into(),
                            active: false,
                        },
                        PaneInfo {
                            id: "pane-91".into(),
                            tab: "Codex".into(),
                            title: "Codex".into(),
                            agent: "codex".into(),
                            active: false,
                        },
                        PaneInfo {
                            id: "pane-92".into(),
                            tab: "opencode".into(),
                            title: "opencode".into(),
                            agent: "opencode".into(),
                            active: false,
                        },
                    ],
                )
                .expect("register external panes for the third worktree");
            let now = Instant::now();
            workspace.activity.agent_spawned("pane-90", "claude", now);
            workspace.activity.agent_spawned("pane-91", "codex", now);
            workspace.activity.agent_spawned("pane-92", "opencode", now);
            workspace.activity.notify("pane-92", AgentStatus::Done, now);
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-worktree-mark-3-git-branch")
                .is_some(),
            "the branch glyph stays: an agent never takes the row's own mark"
        );
        assert!(
            cx.debug_bounds("sidebar-status-running-3").is_some(),
            "agent_id_for_panes tints the running indicator this worktree draws"
        );
        assert!(
            cx.debug_bounds("sidebar-running-agent-3-claude-mark")
                .is_some()
        );
        assert!(
            cx.debug_bounds("sidebar-running-agent-3-openai-mark")
                .is_some()
        );
        assert!(
            cx.debug_bounds("sidebar-running-agent-3-agent-opencode")
                .is_none(),
            "a done agent is not in the running set"
        );
        let claude = cx
            .debug_bounds("sidebar-running-agent-3-claude-mark")
            .expect("claude badge");
        let codex = cx
            .debug_bounds("sidebar-running-agent-3-openai-mark")
            .expect("codex badge");
        assert!(
            claude.origin.x < codex.origin.x,
            "running_agent_ids emits catalog order (claude before codex), not discovery order"
        );
        // A worktree with no agent at all keeps the branch glyph.
        assert!(
            cx.debug_bounds("sidebar-worktree-mark-2-git-branch")
                .is_some()
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// CENTER-01: a runtime worktree switch must load the *newly selected*
    /// worktree's own tabs, not leave whichever tabs were already
    /// materialized on screen. Before this fix `select_worktree` never
    /// touched `self.tabs` at all -- proven live by an earlier wave
    /// (`docs/linux-rewrite/wave-i/I3-tray-jump-report.md`: "the center pane
    /// kept showing repo1's live terminal" after `workspace.select
    /// workspace=/tmp/i3jump-repo2`) and reproduced deterministically here
    /// instead of over a live socket: `wt-1` gets a distinct persisted tab
    /// seeded directly into the database (standing in for "the user opened
    /// this worktree before and it remembered its own layout"), then a
    /// switch away from `wt-0` and back proves both halves of the round
    /// trip -- the incoming worktree's own tabs load, and the outgoing
    /// worktree's tabs are saved before being replaced rather than merely
    /// forgotten.
    #[gpui::test]
    async fn switching_worktree_reloads_the_centre_pane_from_that_worktrees_own_tabs(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("center01");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let wt0 = worktrees[0].clone();
        let wt1 = worktrees[1].clone();

        workspace.update(&mut cx.cx, |workspace, _cx| {
            assert_eq!(workspace.working_directory, wt0);
            assert_eq!(
                workspace.tabs.len(),
                1,
                "the fixture starts with exactly one tab on wt-0"
            );
            assert_eq!(workspace.tabs[0].title, "Terminal");
        });

        // F-TERM-10: the fixture's shell runs `sleep 60` in the foreground
        // purely to report a live, non-`Error` status (see
        // `worktree_urgency_test_workspace`'s own comment) -- this test is
        // about reload *content*, not about F-TERM-10's own liveness gate
        // (`tab_has_live_foreground_process`), which now also refuses to
        // reload a tab with a genuinely running foreground command.
        // Interrupt it and wait for the shell to go idle before switching,
        // the same way a real user's finished command would leave it.
        workspace.update(&mut cx.cx, |workspace, cx| {
            let mut terminal = None;
            workspace.tabs[0].panes.for_each(&mut |_, content| {
                if terminal.is_none() {
                    terminal = content.terminal();
                }
            });
            if let Some(terminal) = terminal {
                terminal.update(cx, |terminal, _| terminal.input([3]));
            }
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let still_live = workspace.read_with(&cx.cx, |workspace, cx| {
                workspace.tab_has_live_foreground_process(&workspace.tabs[0], cx)
            });
            if !still_live {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the fixture's sleep 60 never went idle after Ctrl-C"
            );
            std::thread::sleep(Duration::from_millis(20));
            cx.run_until_parked();
        }

        workspace.update(&mut cx.cx, |workspace, cx| {
            // Seed wt-1's own row directly, as if it had been opened and
            // saved on an earlier visit -- distinct title, so a switch that
            // merely relabels wt-0's tab (the bug) is distinguishable from
            // one that genuinely reloads wt-1's own persisted content.
            workspace.session.save_layout_now(&SessionLayout {
                working_directory: wt1.clone(),
                branch: "branch-1".into(),
                tabs: vec![SessionTab {
                    id: "wt1-marker-tab".into(),
                    title: "WT1 Marker".into(),
                    kind: "terminal".into(),
                    agent_id: None,
                    active: true,
                }],
                tab_states: vec![SessionTabState::default()],
            });

            workspace
                .select_worktree(wt1.clone(), None, cx)
                .expect("select wt-1");
        });
        cx.run_until_parked();

        workspace.read_with(&cx.cx, |workspace, _| {
            assert_eq!(
                workspace.working_directory, wt1,
                "the metadata (working_directory) side of the switch, which \
                 already worked before this fix"
            );
            assert_eq!(
                workspace.tabs.len(),
                1,
                "wt-1's own persisted tab replaces wt-0's, rather than \
                 wt-0's tab staying mounted under the new working_directory"
            );
            assert_eq!(
                workspace.tabs[0].title, "WT1 Marker",
                "the centre pane must show wt-1's own content -- CENTER-01's \
                 exact frame-vs-socket disagreement, reproduced as data: the \
                 socket-equivalent (working_directory) already said wt-1, \
                 and now the tab list agrees"
            );
        });

        // Switch back: wt-0's own tab (saved-before-switch, not merely
        // dropped) must come back, proving the outgoing half of the fix as
        // well as the incoming half just checked above.
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace
                .select_worktree(wt0.clone(), None, cx)
                .expect("select wt-0 again");
        });
        cx.run_until_parked();

        workspace.read_with(&cx.cx, |workspace, _| {
            assert_eq!(workspace.working_directory, wt0);
            assert_eq!(
                workspace.tabs.len(),
                1,
                "wt-0's own tab, saved synchronously on the way out, must \
                 come back rather than the switch having silently lost it"
            );
            assert_eq!(workspace.tabs[0].title, "Terminal");
        });

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// CENTER-01: the safety gate. A live (needs-input) tab in the outgoing
    /// worktree must survive a switch untouched -- reloading would drop its
    /// `TerminalView`, and `TerminalView::drop` tears the PTY down. This is
    /// the same predicate `tray_jump_lands_on_the_target_worktrees_worst_status_tab`
    /// already exercises incidentally (its own outgoing tab reports `Error`,
    /// which the gate also protects); this test names the mechanism
    /// directly, with the plainer `NeedsInput` case, so the gate's own
    /// contract has one test that is about nothing else.
    #[gpui::test]
    async fn switching_away_from_a_needs_input_tab_leaves_it_mounted(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("center01-gate");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let wt1 = worktrees[1].clone();

        workspace.update(&mut cx.cx, |workspace, cx| {
            // wt-0's sole tab (pane 0, the fixture's live `sleep 60` shell)
            // is waiting on the user.
            workspace
                .activity
                .notify("pane-0", AgentStatus::NeedsInput, Instant::now());
            workspace
                .select_worktree(wt1.clone(), None, cx)
                .expect("select wt-1");
        });
        cx.run_until_parked();

        workspace.read_with(&cx.cx, |workspace, _| {
            assert_eq!(
                workspace.working_directory, wt1,
                "the metadata side of the switch still happens -- only the \
                 tab reload is gated"
            );
            assert_eq!(
                workspace.tabs.len(),
                1,
                "the needs-input tab is left mounted rather than dropped"
            );
            assert_eq!(
                workspace.tabs[0].title, "Terminal",
                "still wt-0's own tab -- today's stale-but-safe behaviour, \
                 not wt-1's (empty) persisted layout"
            );
        });

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-TERM-10: the sibling test above proves the gate holds for a
    /// *recognized agent* in `NeedsInput`. Live-drive found a plainer,
    /// uncovered case: a bare shell command with no agent identity at all
    /// (`sleep 300` in one worktree, switch away and back, "the pane came
    /// back as a brand-new fresh shell with no marker and no sleep
    /// process") -- `tab_status`/`pane_close_needs_confirmation` read that
    /// tab as `Idle` (nothing in the agent-activity model has ever heard of
    /// it), so the switch reloaded and dropped it, contradicting CLAUDE.md's
    /// documented "PTYs stay alive across sidebar selection changes"
    /// contract. This deliberately never calls `workspace.activity.notify`
    /// at all -- unlike the sibling test -- so only
    /// `tab_has_live_foreground_process`'s direct `/proc` walk over the
    /// fixture's real `sleep 60` child can save this tab.
    #[cfg(target_os = "linux")]
    #[gpui::test]
    async fn switching_away_from_a_bare_running_command_leaves_it_mounted(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("center01-bare-process");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let wt1 = worktrees[1].clone();

        // Poll the real process tree until the fixture's `sh -c "sleep 60"`
        // has actually forked/exec'd `sleep` -- a real OS fork race the
        // virtual test clock cannot settle.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let has_process = workspace.read_with(&cx.cx, |workspace, cx| {
                workspace
                    .tabs
                    .first()
                    .is_some_and(|tab| workspace.tab_has_live_foreground_process(tab, cx))
            });
            if has_process {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the fixture's `sleep 60` never became visible in /proc"
            );
            std::thread::sleep(Duration::from_millis(20));
            cx.run_until_parked();
        }

        workspace.update(&mut cx.cx, |workspace, cx| {
            // Deliberately no `workspace.activity.notify(...)` call: this
            // tab's status stays `Idle` in the agent-activity model for the
            // whole test, unlike `switching_away_from_a_needs_input_tab_leaves_it_mounted`.
            workspace
                .select_worktree(wt1.clone(), None, cx)
                .expect("select wt-1");
        });
        cx.run_until_parked();

        workspace.read_with(&cx.cx, |workspace, _| {
            assert_eq!(
                workspace.working_directory, wt1,
                "the metadata side of the switch still happens -- only the \
                 tab reload is gated"
            );
            assert_eq!(
                workspace.tabs.len(),
                1,
                "a tab with a live foreground command -- no agent involved \
                 at all -- must be left mounted rather than dropped"
            );
            assert_eq!(
                workspace.tabs[0].title, "Terminal",
                "still wt-0's own tab, not wt-1's (empty) persisted layout"
            );
        });

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-CORE-ACT-22, drawn end to end: a needs-input worktree is lifted
    /// above the siblings the user's manual (catalog) order put it under,
    /// while those siblings keep their order relative to each other — the
    /// `urgent_first` contract, not the full `sorted` one. Clearing the
    /// urgency puts the row back where the manual order had it.
    #[gpui::test]
    async fn drawn_needs_input_worktree_floats_above_its_manual_order_siblings(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("order");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let row_top = |cx: &mut VisualTestContext, id: usize| -> gpui::Pixels {
            let selector: &'static str = Box::leak(format!("sidebar-row-{id}").into_boxed_str());
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("worktree row {id} is drawn"))
                .origin
                .y
        };
        assert!(row_top(&mut cx, 1) < row_top(&mut cx, 2));
        assert!(row_top(&mut cx, 2) < row_top(&mut cx, 3));

        let third = worktrees[2].clone();
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace
                .panes
                .set_external(
                    &third,
                    vec![PaneInfo {
                        id: "pane-90".into(),
                        tab: "Claude Code".into(),
                        title: "Claude Code".into(),
                        agent: "claude".into(),
                        active: false,
                    }],
                )
                .expect("register a pane on the third worktree");
            let now = Instant::now();
            workspace.activity.agent_spawned("pane-90", "claude", now);
            workspace
                .activity
                .notify("pane-90", AgentStatus::NeedsInput, now);
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert!(
            row_top(&mut cx, 3) < row_top(&mut cx, 1),
            "a needs-input worktree is drawn above the manual order's first row"
        );
        assert!(
            row_top(&mut cx, 1) < row_top(&mut cx, 2),
            "the non-urgent siblings keep their manual order"
        );

        // `running` is not urgent: only error and needs-input jump the
        // queue, which is exactly what separates `urgent_first` from
        // `sorted`.
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace
                .activity
                .notify("pane-90", AgentStatus::Running, Instant::now());
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();
        assert!(
            row_top(&mut cx, 1) < row_top(&mut cx, 3),
            "a merely running worktree stays where the manual order put it"
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The regression test for the defect that made this feature actively
    /// misleading: **selecting a worktree must not change the status its row
    /// shows.**
    ///
    /// The row used to read `status_for_panes` for every worktree except the
    /// selected one, where it swapped in a second source that aggregated the
    /// window's current tab set. So the moment you clicked the red row that
    /// `urgent_first` had just floated to the top, it lost its error and
    /// dropped back down — the one gesture a user makes to *look at* an
    /// error was the gesture that hid it.
    ///
    /// Both halves are asserted: the drawn glyph, and the row's position,
    /// which is the visible consequence.
    #[gpui::test]
    async fn drawn_selecting_a_worktree_does_not_change_the_status_its_row_shows(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("selection");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let row_top = |cx: &mut VisualTestContext, id: usize| -> gpui::Pixels {
            let selector: &'static str = Box::leak(format!("sidebar-row-{id}").into_boxed_str());
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("worktree row {id} is drawn"))
                .origin
                .y
        };

        // Worktree 3 (row id 3) is not the selected one, and its agent has
        // failed. The pane is registered through `PaneRegistry::create` —
        // the control socket's own path, the way a real `panel.create` or an
        // agent hook registers one — rather than through the app-pane
        // snapshot, so it belongs to that worktree independently of whatever
        // this window happens to be showing.
        let third = worktrees[2].clone();
        let errored_pane = workspace.read_with(&cx.cx, |workspace, _| {
            workspace
                .panes
                .create(&third, Some("sleep 60"), "Claude Code")
                .expect("register a control pane on the third worktree")
                .id
        });
        workspace.update(&mut cx.cx, |workspace, cx| {
            let now = Instant::now();
            workspace
                .activity
                .agent_spawned(&errored_pane, "claude", now);
            workspace
                .activity
                .notify(&errored_pane, AgentStatus::Error, now);
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-status-dot-3").is_some(),
            "an errored worktree draws a lifecycle dot"
        );
        assert!(
            row_top(&mut cx, 3) < row_top(&mut cx, 1),
            "urgent_first floats the errored worktree above its manual-order siblings"
        );

        // Now select it — the one gesture that used to erase the status.
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace
                .select_worktree(third.clone(), None, cx)
                .expect("select the errored worktree");
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-status-dot-3").is_some(),
            "the selected worktree keeps the status its own panes report"
        );
        assert!(
            cx.debug_bounds("sidebar-status-running-3").is_none(),
            "and it is still the error, not something downgraded to running"
        );
        assert!(
            row_top(&mut cx, 3) < row_top(&mut cx, 1),
            "so urgent_first still holds it at the top instead of dropping it back"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace
                .activity
                .status_for_panes(&[errored_pane.as_str()])),
            Some(AgentStatus::Error),
            "and the one model still holds the pane's error"
        );

        workspace.read_with(&cx.cx, |workspace, _| {
            let _ = workspace.panes.close(&errored_pane);
        });
        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-USE-05 / F-CORE-ACT-22: the tab a tray-roster jump lands on is the
    /// one `AttentionSort::sorted` puts first, so a tab waiting for an
    /// answer beats one that is merely working.
    ///
    /// `worst_status_tab_id` used to carry its own rank that put running and
    /// needs-input both at 1; with tab 1 running and tab 2 needing input,
    /// stable ordering then handed back the *running* tab and left the
    /// question in the background — the exact inversion of the clause's
    /// error → needs-input → running order.
    #[gpui::test]
    async fn tray_jump_prefers_the_tab_that_needs_input_over_the_one_merely_running(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, worktrees) = urgency_test_root("jump");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let jumped = workspace.update(&mut cx.cx, |workspace, cx| {
            // Tab 0 owns pane 0 (the fixture's live shell). Add a second
            // terminal tab, then give the *earlier* tab the running agent
            // and the later one the question.
            workspace.add_terminal_tab_with_shell(
                "Terminal 2",
                TerminalShell::WithArguments {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), "sleep 60".into()],
                },
                None,
                cx,
            );
            let panes: Vec<(usize, usize)> = workspace
                .tabs
                .iter()
                .map(|tab| (tab.id, tab.focused_pane))
                .collect();
            assert_eq!(panes.len(), 2, "the fixture has two tabs to choose between");
            let now = Instant::now();
            for (index, (_, pane)) in panes.iter().enumerate() {
                let pane_id = format!("pane-{pane}");
                workspace.activity.agent_spawned(&pane_id, "claude", now);
                workspace.activity.notify(
                    &pane_id,
                    if index == 0 {
                        AgentStatus::Running
                    } else {
                        AgentStatus::NeedsInput
                    },
                    now,
                );
            }
            workspace.sync_activity(cx);
            let jumped = workspace
                .select_worktree_and_jump(worktrees[0].clone(), None, cx)
                .expect("jump into the fixture worktree");
            (jumped.map(|(id, _)| id), panes)
        });
        cx.run_until_parked();

        let (landed, panes) = jumped;
        assert_eq!(
            landed,
            Some(panes[1].0),
            "the jump lands on the tab that needs input, not the one that is running"
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The close-confirmation prompt has to be *seeable* and has to describe
    /// the state it is asking about.
    ///
    /// It used to render inside `render_pane_tree`, which only runs for each
    /// group's active tab — so an Activity-panel close of a **background**
    /// tab armed a prompt on a surface nobody could see, and there was no
    /// drawn control to cancel it with either. And its text was hard-coded
    /// to "has running work" for all three confirming states, so a failed
    /// agent was announced as still working.
    #[gpui::test]
    async fn drawn_close_prompt_is_visible_for_a_background_tab_and_names_the_state(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, _worktrees) = urgency_test_root("banner");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        // Two tabs; the first stays in the background while the second is
        // active. The background one is the one with the failed agent.
        let background_pane = workspace.update(&mut cx.cx, |workspace, cx| {
            let background_pane = workspace.tabs[0].focused_pane;
            workspace.add_terminal_tab_with_shell(
                "Terminal 2",
                TerminalShell::WithArguments {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), "sleep 60".into()],
                },
                None,
                cx,
            );
            let now = Instant::now();
            let pane_id = format!("pane-{background_pane}");
            workspace.activity.agent_spawned(&pane_id, "claude", now);
            workspace.activity.notify(&pane_id, AgentStatus::Error, now);
            workspace.sync_activity(cx);
            background_pane
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("pane-close-confirm").is_none(),
            "no close was requested yet"
        );

        workspace.update(&mut cx.cx, |workspace, cx| {
            assert_ne!(
                workspace.active_tab, 0,
                "tab 0 is the background tab for this test"
            );
            workspace.request_close_activity(0, cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("pane-close-confirm").is_some(),
            "closing a background tab's Activity row draws a prompt the user can answer"
        );
        assert!(cx.debug_bounds("pane-close-confirm-cancel").is_some());
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace
                .pending_pane_close
                .map(|pending| pending.status)),
            Some(ActivityStatus::Error),
            "the held close remembers the state that made it need confirming"
        );

        // Cancel with a real click on the drawn control.
        let cancel = cx
            .debug_bounds("pane-close-confirm-cancel")
            .expect("cancel control");
        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("pane-close-confirm").is_none());
        assert!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()) == 2,
            "cancelling keeps the tab"
        );
        let _ = background_pane;

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-CORE-ACT-23, drawn: the pane close prompt is requested for exactly
    /// the statuses `tiller_activity::ActivityStatus::requires_close_confirmation`
    /// names — running, needs-input, error — and for neither done nor idle.
    /// Every status is attempted against a real pane, and the cancel is a
    /// real click on the drawn banner.
    #[gpui::test]
    async fn drawn_pane_close_prompt_is_requested_for_exactly_the_urgent_statuses(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, _worktrees) = urgency_test_root("close");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        // Running/NeedsInput/Error all hold the close for confirmation, and
        // Cancel must leave the pane alone -- run this loop *before* either
        // no-confirmation case below, both of which (correctly, since
        // F-TAB-26's fix) actually remove this fixture's sole tab rather
        // than leaving it standing.
        for status in [
            AgentStatus::Running,
            AgentStatus::NeedsInput,
            AgentStatus::Error,
        ] {
            workspace.update(&mut cx.cx, |workspace, cx| {
                workspace.activity.notify(
                    "pane-0",
                    status,
                    Instant::now() + Duration::from_millis(1),
                );
                workspace.request_close_terminal_at(0, 0, cx);
            });
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("pane-close-confirm").is_some(),
                "closing a {status:?} pane must be held for confirmation"
            );
            let cancel = cx
                .debug_bounds("pane-close-confirm-cancel")
                .expect("the held close offers a cancel");
            cx.simulate_click(cancel.center(), Modifiers::none());
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("pane-close-confirm").is_none(),
                "cancelling releases the held close"
            );
            assert_eq!(
                workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
                1,
                "a cancelled close leaves the pane alone"
            );
        }

        // Done: `ActivityStatus::from_agent_status` maps both `Done` and "no
        // status notified at all" (Idle) to the same
        // `requires_close_confirmation() == false` outcome
        // (`tiller_activity/src/activity.rs`), so this one case stands for
        // both -- closing this fixture's sole pane a second time to also
        // cover a bare Idle close would leave no tab left for it to act on.
        // F-TAB-26: this used to only need to show no prompt; now that the
        // guard-clause bug is fixed, closing a status that needs no
        // confirmation on a tab's *only* pane must really remove the tab,
        // not silently leave it (that silent-leave was the bug).
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.activity.notify(
                "pane-0",
                AgentStatus::Done,
                Instant::now() + Duration::from_millis(2),
            );
            workspace.request_close_terminal_at(0, 0, cx);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("pane-close-confirm").is_none(),
            "a finished pane closes without a prompt"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            0,
            "a finished pane that is its tab's only pane must actually close the tab"
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-TAB-26, drawn: `close_terminal_at` deliberately refuses to remove a
    /// tab's *last* leaf (the F-TAB-13 empty-state guard), so closing the
    /// sole pane of a single-pane tab must be routed through the whole-tab
    /// close path instead. `request_close_terminal_at` used to always build
    /// `PendingPaneClose { whole_tab: false, .. }`, so "Close Anyway" on a
    /// tab's only pane called `close_terminal_at`, hit that guard, and did
    /// nothing at all -- the confirm banner closed but the tab stayed open.
    /// Reproduced live under Wayland: `panel.list`'s pane count was
    /// unchanged and the terminal process was still alive after clicking
    /// "Close Anyway" on a sole-pane tab, while the identical control
    /// removed a multi-pane tab's pane immediately.
    #[gpui::test]
    async fn close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, _worktrees) = urgency_test_root("close-sole-pane");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            1,
            "the fixture starts with exactly one tab holding exactly one pane"
        );

        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.activity.notify(
                "pane-0",
                AgentStatus::Running,
                Instant::now() + Duration::from_millis(1),
            );
            workspace.request_close_terminal_at(0, 0, cx);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("pane-close-confirm").is_some(),
            "a running pane's close is held for confirmation, same as the multi-pane case"
        );

        let close_anyway = cx
            .debug_bounds("pane-close-confirm-close")
            .expect("the held close offers a Close Anyway control");
        cx.simulate_click(close_anyway.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("pane-close-confirm").is_none(),
            "the banner is dismissed after Close Anyway"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            0,
            "Close Anyway on a tab's sole pane must remove the tab, not silently do nothing"
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-CORE-ACT-23, drawn: the Activity panel's own close control used to
    /// call `close_tab` straight through — the one list that exists to show
    /// a running agent was also the only place that could kill it silently.
    /// It now asks the same `requires_close_confirmation` the pane close
    /// does, and reuses the same banner instead of raising a second prompt.
    #[gpui::test]
    async fn drawn_activity_row_close_holds_a_running_tab_and_lets_an_idle_one_go(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let (root, _worktrees) = urgency_test_root("activity-close");
        let root_for_window = root.clone();
        let window =
            cx.add_window(|_window, cx| worktree_urgency_test_workspace(cx, &root_for_window));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1400.0), px(700.0)));
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        workspace.update(&mut cx.cx, |workspace, cx| {
            // Tab 0 runs an agent; tab 1 is a plain idle shell.
            workspace
                .activity
                .agent_spawned("pane-0", "claude", Instant::now());
            workspace.add_terminal_tab_with_shell(
                "Terminal 2",
                TerminalShell::WithArguments {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), "sleep 60".into()],
                },
                None,
                cx,
            );
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            2
        );

        let header = cx
            .debug_bounds("activity-header")
            .expect("the activity header is drawn");
        cx.simulate_click(header.center(), Modifiers::none());
        cx.run_until_parked();

        // The close control is the row's last child, inside its 10px right
        // padding.
        let close_point = |row: gpui::Bounds<gpui::Pixels>| {
            point(row.origin.x + row.size.width - px(16.0), row.center().y)
        };

        // Idle: closes on the spot, no prompt.
        let idle_row = cx
            .debug_bounds("activity-1")
            .expect("the idle tab has an activity row");
        cx.simulate_click(close_point(idle_row), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("pane-close-confirm").is_none(),
            "an idle activity row closes without a prompt"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            1,
            "the idle tab actually closed"
        );

        let row = cx
            .debug_bounds("activity-0")
            .expect("the running tab has an activity row");
        cx.simulate_click(close_point(row), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            1,
            "closing a running agent from the Activity list must not kill it silently"
        );
        assert!(
            cx.debug_bounds("pane-close-confirm").is_some(),
            "the Activity close is held behind the same banner the pane close uses"
        );

        let confirm = cx
            .debug_bounds("pane-close-confirm-close")
            .expect("the held close offers Close Anyway");
        cx.simulate_click(confirm.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            0,
            "confirming an Activity close closes the whole tab, not just one pane"
        );

        shutdown_workspace_terminals(&workspace, &mut cx);
        let _ = std::fs::remove_dir_all(&root);
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

    /// F-CORE-WSP-04: `commit_tab_rename` routes the real rename through
    /// `LayoutCommand::Rename` + `classify_layout_command`, and its
    /// `FocusIntent::Tab` answer is what sends keyboard focus back to the
    /// pane. Before this, focus was left on the (now-unmounted) rename
    /// field, so typing after a rename silently went nowhere until the
    /// user clicked the terminal.
    #[gpui::test]
    async fn committing_a_tab_rename_returns_focus_to_the_terminal(cx: &mut TestAppContext) {
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
        let focus = palette_test_terminal_focus(&workspace, &cx);
        cx.update(|window, app| focus.focus(window, app));
        cx.run_until_parked();
        assert!(cx.update(|window, _| focus.is_focused(window)));

        right_click_tab(&mut cx, 0);
        let rename = cx
            .debug_bounds("tab-command-rename")
            .expect("rename is reachable from the tab menu");
        cx.simulate_click(rename.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("tab-rename-field").is_some());
        // While the rename field is open, focus is on it, not the terminal.
        assert!(!cx.update(|window, _| focus.is_focused(window)));

        cx.simulate_input(" renamed");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert!(
            cx.update(|window, _| focus.is_focused(window)),
            "committing a tab rename must hand keyboard focus back to the tab's own content"
        );
    }

    /// F-TAB-14: double-click-to-rename was entirely unwired -- no
    /// click-count handling existed anywhere in the tab render code, so a
    /// double click only ever reselected the tab. A single click must keep
    /// only selecting (no rename field, no `select_tab` side effect lost),
    /// and a real double-click (`click_count == 2`, GPUI's own
    /// platform-timing disambiguation already applied, same pattern
    /// `titlebar.rs`'s drag-area tests use) must open the same
    /// `tab-rename-field` the context menu's Rename item opens.
    #[gpui::test]
    async fn double_click_on_a_tab_opens_its_rename_field(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 2));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.active_tab),
            0,
            "the fixture starts on tab 0"
        );

        // A single click on the OTHER tab only selects it -- no rename field.
        let tab1 = cx.debug_bounds("workspace-tab-1").expect("tab 1 is drawn");
        cx.simulate_click(tab1.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.active_tab),
            1,
            "a plain single click must still select the tab"
        );
        assert!(
            cx.debug_bounds("tab-rename-field").is_none(),
            "a single click must not open the rename field"
        );

        let tab0 = cx.debug_bounds("workspace-tab-0").expect("tab 0 is drawn");
        cx.simulate_event(MouseDownEvent {
            position: tab0.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: tab0.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("tab-rename-field").is_some(),
            "a real double-click (click_count == 2) must open the tab's rename field"
        );
    }

    /// The seam a critic found one level below the worktree row: agent
    /// identity reached the worktree row and never the **tab** rows under
    /// it. In its frame a worktree row drew three brand marks while all four
    /// tab rows below drew the generic terminal glyph.
    ///
    /// The cause was that the sidebar's tab rows took their mark from
    /// `OpenTab::agent_icon`, fixed at spawn, while the worktree row read
    /// `AgentActivityModel` live. `App/WorkspaceTabIcon.swift` reads
    /// `model.agentActivity.paneAgents[paneId]` at render time, which is why
    /// a hand-launched agent gets its brand there as soon as a layer
    /// identifies it.
    ///
    /// This drives the **production** identification path, not a helper:
    /// a real `TerminalActivityEvent::OscTitle` through
    /// `panes::apply_terminal_activity_event` — Layer B, `. <task>` being
    /// Claude's own working-title convention — into the workspace's one
    /// activity model.
    #[gpui::test]
    async fn a_layer_b_identity_after_spawn_reaches_the_sidebar_tab_row(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 1));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        // Tab 0 owns pane 0, spawned as a plain shell: no adapter, so no
        // spawn-time icon and nothing in `pane_agents` yet.
        workspace.update(&mut cx, |workspace, cx| {
            assert!(workspace.tabs[0].agent_icon.is_none());
            assert!(workspace.tab_agent_mark(&workspace.tabs[0]).is_none());
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert_eq!(TAB_ROW_ID_OFFSET, 1_000_000);
        const GENERIC: &str = "sidebar-tab-mark-1000000-terminal";
        const CLAUDE: &str = "sidebar-tab-mark-1000000-claude-mark";
        assert!(
            cx.debug_bounds(GENERIC).is_some(),
            "a plain shell's tab row draws the generic terminal glyph"
        );
        assert!(cx.debug_bounds(CLAUDE).is_none());

        // Layer B: the pane's own shell writes Claude's working title.
        workspace.update(&mut cx, |workspace, cx| {
            panes::apply_terminal_activity_event(
                &mut workspace.activity,
                "pane-0",
                &TerminalActivityEvent::OscTitle(". building tiller".to_string()),
                Instant::now(),
            );
            assert_eq!(
                workspace.activity.agent_id("pane-0"),
                Some("claude"),
                "Layer B identifies `. <task>` as Claude"
            );
            assert!(
                workspace.activity.is_title_owned("pane-0"),
                "and takes title ownership of the pane, not spawn ownership"
            );
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds(CLAUDE).is_some(),
            "the tab row shows the brand as soon as a layer identifies the pane"
        );
        assert!(cx.debug_bounds(GENERIC).is_none());

        // The mark is a pure read: resolving it must not have registered,
        // cleared or otherwise touched pane ownership, which is what keeps
        // one layer's signal from wiping state another layer relies on.
        workspace.update(&mut cx, |workspace, _| {
            assert!(workspace.activity.is_title_owned("pane-0"));
            assert!(!workspace.activity.is_process_owned("pane-0"));
            assert!(
                workspace.tabs[0].agent_icon.is_none(),
                "a live read never back-fills the spawn-time field"
            );
        });
    }

    /// The same live read, one layer over: Layer D's foreground-process walk
    /// is the only signal that catches a native agent with no usable title
    /// convention, and it lands after spawn too.
    #[gpui::test]
    async fn a_layer_d_identity_after_spawn_reaches_the_sidebar_tab_row(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 1));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        workspace.update(&mut cx, |workspace, cx| {
            workspace.activity.process_identified("pane-0", "codex");
            workspace.sync_activity(cx);
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-tab-mark-1000000-openai-mark")
                .is_some(),
            "a process-owned pane's brand reaches its tab row"
        );
        workspace.update(&mut cx, |workspace, _| {
            assert!(
                workspace.activity.is_process_owned("pane-0"),
                "reading the mark leaves process ownership exactly as it was"
            );
        });
    }

    /// F-CORE-ACT-17/18, the colours: a *running* Claude worktree must not
    /// paint the same hex as one that *needs input*, and each badge mark
    /// must wear its own agent's brand rather than one shared accent.
    #[test]
    fn worktree_activity_colours_name_the_agent_and_never_a_status() {
        let theme = Theme::dark();
        assert_ne!(
            AgentBrandColor::for_agent_id("claude").color(),
            theme.tab_needs_input,
            "running Claude and needs-input used to be the identical #E0B36A"
        );
        for (id, brand) in [
            ("claude", AgentBrandColor::Claude),
            ("codex", AgentBrandColor::Codex),
            ("opencode", AgentBrandColor::OpenCode),
            ("pi", AgentBrandColor::Pi),
            ("omp", AgentBrandColor::Omp),
        ] {
            assert_eq!(AgentBrandColor::for_agent_id(id), brand);
            let mark = AgentMark::for_agent_id(id).expect("a catalog agent has a brand mark");
            assert_eq!(mark.brand, brand);
            assert_eq!(mark.icon, Icon::for_agent_id(id).expect("catalog icon"));
            assert_ne!(
                mark.brand.color(),
                theme.tab_focus_accent,
                "{id}'s mark used to be tinted tab_focus_accent -- Claude's own coral"
            );
        }
    }

    /// F-TAB-11 (`SplitDisabledReason::SoleTabInGroup` half): only
    /// `render_pane_tree` can count a tab's pane-group membership, so it
    /// pushes the result into the tab's own `TerminalView` on every render
    /// via `TerminalView::set_sole_tab_in_group`. A workspace with a single
    /// tab has that tab as the sole member of pane group 0.
    #[gpui::test]
    async fn a_solo_tab_is_pushed_as_the_sole_tab_in_its_group(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 1));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let sole = workspace.update(&mut cx, |workspace, cx| {
            let mut sole = None;
            workspace.tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Terminal { view } = content {
                    sole = Some(view.read(cx).sole_tab_in_group());
                }
            });
            sole.expect("tab 0 has a terminal pane")
        });
        assert!(
            sole,
            "the only tab in pane group 0 must be pushed as the sole tab in its group"
        );
    }

    /// The other half of the same wiring: two tabs sharing pane group 0
    /// (`palette_test_workspace_with_tab_count`'s default) must both be
    /// pushed as *not* the sole tab, so Split stays offered on size alone.
    #[gpui::test]
    async fn two_tabs_sharing_a_group_are_not_pushed_as_sole(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 2));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let sole_flags = workspace.update(&mut cx, |workspace, cx| {
            workspace
                .tabs
                .iter()
                .map(|tab| {
                    let mut sole = None;
                    tab.panes.for_each(&mut |_, content| {
                        if let TabContent::Terminal { view } = content {
                            sole = Some(view.read(cx).sole_tab_in_group());
                        }
                    });
                    sole.expect("every test tab has a terminal pane")
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            sole_flags,
            vec![false, false],
            "two tabs sharing one pane group must not be marked as the sole tab in their group"
        );
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

    /// P117: a completed chat turn returns a full transcript over the control socket and draws
    /// nothing on screen. The frame shows the composer at the *top* of the pane with ~670 px of
    /// empty below it, which is the layout a `flex_col` produces when its first child measured
    /// zero. This pins the geometry so the defect cannot come back silently.
    ///
    /// This is a test, not the proof — see `docs/linux-rewrite/P117-report.md` for the frame.
    #[gpui::test]
    async fn drawn_chat_transcript_gets_the_full_center_surface_height(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| {
            let mut workspace = palette_test_workspace(cx);
            let chat = cx.new(|cx| {
                Chat::launch_with_command(
                    AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                    std::env::temp_dir(),
                    cx,
                )
            });
            workspace.tabs[0] = OpenTab {
                id: 0,
                persistence_id: "test-chat".into(),
                group_id: 0,
                title: "Chat".into(),
                kind: TabKind::AgentChat,
                agent_icon: Some(Icon::Codex),
                agent_id: Some("codex".into()),
                session_state: SessionTabState::with_root(0),
                panes: PaneNode::leaf(0, TabContent::Chat(chat)),
                focused_pane: 0,
                title_is_auto_named: true,
            };
            workspace.rebuild_tab_machinery();
            workspace
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let centre = cx
            .debug_bounds("centre-surface")
            .expect("the centre surface wrapper is mounted");
        let root = cx
            .debug_bounds("chat-root")
            .expect("the chat root is mounted");
        let transcript = cx
            .debug_bounds("chat-transcript")
            .expect("the chat transcript is mounted");
        assert!(
            transcript.size.height > px(200.0),
            "the mounted chat transcript must receive the center surface height; \
             centre-surface={:?} group-surfaces-wrapper={:?} pane-group-surface={:?} \
             pane-leaf={:?} pane-surface={:?} chat-root={:?} chat-transcript={:?}",
            centre.size,
            cx.debug_bounds("group-surfaces-wrapper").map(|b| b.size),
            cx.debug_bounds("pane-group-surface").map(|b| b.size),
            cx.debug_bounds("pane-leaf").map(|b| b.size),
            cx.debug_bounds("pane-surface").map(|b| b.size),
            root.size,
            transcript.size
        );
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

    /// F-TERM-PTY-08: the exact same UI gesture as the test above (a real
    /// MoveTabToOtherPane through the tab context menu), but asserting on
    /// `terminal_pane_cache` -- the seam row's own row -- rather than only
    /// on `OpenTab::group_id`. `move_selected_tab`/`apply_tab_machinery`
    /// already moved the live `Entity<TerminalView>` by value before this
    /// wiring existed (nothing rebuilds it, so a PTY/scrollback never had a
    /// bug to fix here); what was missing is this durable record of where
    /// the pane ended up, which `TerminalPaneCache::restore_focus` needs.
    #[gpui::test]
    async fn drawn_tab_context_menu_move_records_the_terminal_in_the_pane_cache(
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
        let worktree_id = workspace.read_with(&cx.cx, |workspace, _| {
            workspace.working_directory.to_string_lossy().into_owned()
        });

        right_click_tab(&mut cx, 1);
        let move_to_pane = cx
            .debug_bounds("tab-command-move-to-pane-1")
            .expect("the other pane destination is drawn");
        cx.simulate_click(move_to_pane.center(), Modifiers::none());
        cx.run_until_parked();

        workspace.read_with(&cx.cx, |workspace, _| {
            let cached = workspace
                .terminal_pane_cache
                .get("terminal-1")
                .expect("the moved terminal pane must be tracked in the cache");
            assert_eq!(cached.pane_id, "group-1-pane-1");
            assert_eq!(cached.worktree_id, worktree_id);
        });
    }

    /// F-TERM-PTY-07: proves the app crate is a real caller, not just the
    /// pure `delegated_terminal_context_action` mapping this test's sibling
    /// (`terminal_context_app_actions_have_workspace_routes`) already
    /// covers. Emits the same `TerminalContextEvent` a real "Restart
    /// Terminal" menu click produces on the fixture's own live pane and
    /// confirms `subscribe_terminal`'s dispatch reaches
    /// `TerminalView::restart` and its generation actually bumps --
    /// `tiller_terminal`'s own test proves the mechanism is correct in
    /// isolation; this proves `main.rs` now calls it.
    #[gpui::test]
    async fn drawn_terminal_restart_command_reaches_the_real_pane_and_bumps_its_generation(
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
                .expect("workspace root")
        });
        let terminal = workspace.read_with(&cx.cx, |workspace, _| {
            let mut found = None;
            workspace.tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Terminal { view } = content {
                    found = Some(view.clone());
                }
            });
            found.expect("the fixture's first tab is a terminal pane")
        });
        let identity = terminal.read_with(&cx.cx, |terminal, _| terminal.identity().clone());
        let generation_before =
            terminal.read_with(&cx.cx, |terminal, _| terminal.surface_generation());

        terminal.update(&mut cx.cx, |_, cx| {
            cx.emit(TerminalContextEvent {
                target: identity,
                action: TerminalContextAction::RestartTerminal,
            });
        });
        cx.run_until_parked();

        let generation_after =
            terminal.read_with(&cx.cx, |terminal, _| terminal.surface_generation());
        assert_eq!(
            generation_after,
            generation_before + 1,
            "the app crate's TerminalContextEvent dispatch must reach TerminalView::restart, \
             not just define the command"
        );
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

    /// F-CORE-ACT-20: `window_active` must track the real
    /// `Window::is_window_active`, not stay pinned to its own `true`
    /// starting value -- that pinned value is exactly what the production
    /// call site hardcoded before this fix (`main.rs`, the
    /// `NotificationPolicy::should_notify` call
    /// `post_activity_notification` makes), silently making the
    /// desktop-focus half of that clause unable to vary. `gpui`'s own test
    /// platform makes this a real, non-tautological assertion: a freshly
    /// opened `TestWindow` reports `is_active() == false` unconditionally
    /// (`platform/test/window.rs`), so `Window::is_window_active()` reads
    /// `false` here from the moment the window opens -- the *opposite* of
    /// `window_active`'s `true` default. Only a render that actually reads
    /// the live value flips the field; a reverted fix (the field just
    /// sitting at its constructor default) would leave this `true` and the
    /// test would fail.
    #[gpui::test]
    async fn render_polls_the_real_window_activation_state(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let (window_reports_active, workspace_cached_active) = cx.update(|window, app| {
            let workspace = window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root");
            (
                window.is_window_active(),
                workspace.read(app).window_active,
            )
        });

        assert!(
            !window_reports_active,
            "the gpui test platform's own TestWindow reports is_active() == \
             false unconditionally -- this assertion documents that fact so \
             a future gpui upgrade that changes it fails loudly here rather \
             than silently turning the assertion below tautological"
        );
        assert_eq!(
            workspace_cached_active, window_reports_active,
            "render() must poll Window::is_window_active() every frame -- a \
             hardcoded `true` (the F-CORE-ACT-20 defect) would leave this \
             field stuck at its constructor default and disagree with the \
             window's real (false, in this harness) state"
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
    async fn drawn_tab_strip_does_not_reserve_overflow_room_when_all_tabs_actually_fit(
        cx: &mut TestAppContext,
    ) {
        // F-TAB-02 (P104 §Group 1): `has_overflow` used to be decided against
        // `available_width - overflow_width`, reserving room for the chevron
        // even when it would never be drawn. At this exact window width
        // three 132px terminal tabs fill the strip's full available width
        // (396px) with nothing to spare, but the old check still demanded a
        // further 26px (for a chevron it would then need to show), so it hid
        // the third tab behind a chevron nothing actually required.
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 3));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1164.0), px(600.0)));
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("tab-overflow-button").is_none(),
            "three tabs that exactly fill the strip must not trip overflow"
        );
        assert!(cx.debug_bounds("workspace-tab-0").is_some());
        assert!(cx.debug_bounds("workspace-tab-1").is_some());
        assert!(
            cx.debug_bounds("workspace-tab-2").is_some(),
            "the third tab must stay visible instead of being hidden behind a phantom chevron"
        );
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
                persistence_id: "resumed-chat".into(),
                group_id: 0,
                title: "Resumed chat".into(),
                kind: TabKind::AgentChat,
                agent_icon: Some(Icon::Codex),
                agent_id: Some("codex".into()),
                session_state: SessionTabState::with_root(1),
                panes: PaneNode::leaf(1, TabContent::Chat(chat_for_tab)),
                focused_pane: 1,
                title_is_auto_named: true,
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

        // F-WIN-10: the same error must also raise a floating, transient
        // toast -- distinct from the sidebar's persistent inline banner
        // asserted above, and it must disappear on its own.
        wait_for_drawn(&mut cx, "workspace-toast");
        cx.background_executor.advance_clock(Duration::from_secs(5));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("workspace-toast").is_none(),
            "the toast must auto-dismiss after its duration elapses"
        );
    }

    /// F-WIN-11: drives `UpdateState` through every user-visible state
    /// (all but `Idle`, which renders nothing) and asserts the toast
    /// draws for each, then that its wired Dismiss control resets the
    /// state and un-draws the toast -- the same drawn-control contract
    /// `drawn_add_project_duplicate_shows_sidebar_notice` holds
    /// `workspace-toast` to.
    #[gpui::test]
    async fn drawn_update_toast_renders_every_state_and_dismiss_resets_it(cx: &mut TestAppContext) {
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

        assert!(
            cx.debug_bounds("update-toast").is_none(),
            "Idle must draw no toast"
        );

        for event in [
            UpdateEvent::CheckStarted,
            UpdateEvent::Available("1.4.0".into()),
            UpdateEvent::DownloadProgress(150),
            UpdateEvent::InstallStarted,
            UpdateEvent::Finished,
            UpdateEvent::Failed("network unreachable".into()),
        ] {
            workspace.update(&mut cx, |workspace, cx| {
                workspace.update_state = workspace.update_state.clone().transition(event);
                cx.notify();
            });
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("update-toast").is_some(),
                "the update toast must draw for every non-Idle state"
            );
        }
        // The last transition above (`Failed`) left a clamped 100% progress
        // bar behind it in `Downloading`'s wake -- confirm the clamp
        // itself is visible along the way by re-checking it directly.
        workspace.update(&mut cx, |workspace, cx| {
            workspace.update_state = UpdateState::Downloading {
                progress_percent: 100,
            };
            cx.notify();
        });
        cx.run_until_parked();
        wait_for_drawn(&mut cx, "update-toast-progress");

        let dismiss = cx
            .debug_bounds("update-toast-dismiss")
            .expect("dismiss control is drawn");
        cx.simulate_click(dismiss.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("update-toast").is_none(),
            "dismiss must reset UpdateState to Idle and un-draw the toast"
        );
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.update_state.clone()),
            UpdateState::Idle
        );
    }

    #[cfg(target_os = "linux")]
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

        cx.simulate_keystrokes("ctrl-t ctrl-o ctrl-s ctrl-shift-s ctrl-shift-i ctrl-shift-o");
        cx.run_until_parked();

        assert_eq!(
            fired.borrow().as_slice(),
            &[
                WindowCommand::NewTerminalTab,
                WindowCommand::OpenFile,
                WindowCommand::SaveFile,
                WindowCommand::ToggleSidebar,
                WindowCommand::ToggleRightPanel,
                WindowCommand::RestoreLaunchSnapshot,
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
                (WindowCommand::RestoreLaunchSnapshot, "ctrl-shift-o"),
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
            delegated_terminal_context_action(TerminalContextAction::RestartTerminal),
            Some(TerminalContextCommand::Restart)
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
    fn persisted_settings_round_trip_maps_all_nineteen_fields_explicitly() {
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
            ollama_show_in_bar: true,
            refresh_interval_min: 11,
            opencode_workspace_id_override: "wrk_main".into(),
            translucency: true,
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
        assert!(snapshot.ollama_show_in_bar);
        assert_eq!(snapshot.refresh_interval, 11);
        assert_eq!(snapshot.opencode_workspace_id_override, "wrk_main");
        assert!(snapshot.translucency);

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
        assert_eq!(restored.ollama_show_in_bar, persisted.ollama_show_in_bar);
        assert_eq!(
            restored.refresh_interval_min,
            persisted.refresh_interval_min
        );
        assert_eq!(
            restored.opencode_workspace_id_override,
            persisted.opencode_workspace_id_override
        );
        assert_eq!(restored.translucency, persisted.translucency);

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
            ollama_show_in_bar: true,
            refresh_interval_min: 11,
            opencode_workspace_id_override: "wrk_main".into(),
            translucency: true,
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
        assert_eq!(restored.ollama_show_in_bar, persisted.ollama_show_in_bar);
        assert_eq!(
            restored.refresh_interval_min,
            persisted.refresh_interval_min
        );
        assert_eq!(restored.translucency, persisted.translucency);

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
                id: "snapshot-chat".into(),
                title: "Chat".into(),
                kind: "chat".into(),
                agent_id: None,
                active: false,
            },
            SessionTab {
                id: "snapshot-terminal".into(),
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            },
        ];
        let current = vec![SessionTab {
            id: "current-changes".into(),
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
    fn launch_snapshot_matches_a_tab_by_stable_id_after_rename() {
        let snapshot = vec![SessionTab {
            id: "chat-stable".into(),
            title: "Old title".into(),
            kind: "chat".into(),
            agent_id: None,
            active: false,
        }];
        let current = vec![SessionTab {
            id: "chat-stable".into(),
            title: "Renamed chat".into(),
            kind: "chat".into(),
            agent_id: None,
            active: true,
        }];

        assert_eq!(merge_launch_snapshot_tabs(&snapshot, &current), current);
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

    /// F-CORE-ACT-25: `ControlState::from_catalog` runs the restored
    /// worktree list through `BootstrapRestoreOrder::partition` -- only the
    /// selected worktree (the sole "previously open" id a single-directory
    /// launch snapshot recovers) starts mounted; every deferred worktree
    /// starts unmounted rather than the old "mount everything" default.
    #[test]
    fn bootstrap_restore_order_mounts_only_the_selected_worktree_at_first_paint() {
        let main_path = PathBuf::from("/tmp/tiller-bootstrap-main");
        let feature_path = PathBuf::from("/tmp/tiller-bootstrap-feature");
        let other_path = PathBuf::from("/tmp/tiller-bootstrap-other");
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
                session::CatalogWorktree {
                    branch: "other".into(),
                    path: other_path.clone(),
                    is_primary: false,
                },
            ],
        }]);
        let state = ControlState::from_catalog(&catalog, &feature_path);

        assert_eq!(
            state
                .workspaces
                .iter()
                .map(|workspace| (workspace.path.clone(), workspace.mounted))
                .collect::<Vec<_>>(),
            vec![
                (main_path.to_string_lossy().into_owned(), false),
                (feature_path.to_string_lossy().into_owned(), true),
                (other_path.to_string_lossy().into_owned(), false),
            ]
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

    /// F-WIN-11: `update.event` is the cheapest honest way to drive
    /// `UpdateState` on Linux (see the `ControlAction::UpdateEvent` and
    /// `update_state` field docs) -- this asserts every event kind it
    /// accepts queues the matching `UpdateEvent`, and that malformed input
    /// is rejected rather than silently dropped or defaulted.
    #[test]
    fn update_event_queues_the_matching_control_action() {
        let control_actions = Arc::new(Mutex::new(Vec::new()));
        let handler = AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            control_actions.clone(),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-update-event-test.sock")),
        );

        let cases: Vec<(BTreeMap<String, String>, UpdateEvent)> = vec![
            (
                BTreeMap::from([("event".to_string(), "check-started".to_string())]),
                UpdateEvent::CheckStarted,
            ),
            (
                BTreeMap::from([
                    ("event".to_string(), "available".to_string()),
                    ("version".to_string(), "1.4.0".to_string()),
                ]),
                UpdateEvent::Available("1.4.0".to_string()),
            ),
            (
                BTreeMap::from([
                    ("event".to_string(), "download-progress".to_string()),
                    ("percent".to_string(), "150".to_string()),
                ]),
                UpdateEvent::DownloadProgress(150),
            ),
            (
                BTreeMap::from([("event".to_string(), "install-started".to_string())]),
                UpdateEvent::InstallStarted,
            ),
            (
                BTreeMap::from([("event".to_string(), "finished".to_string())]),
                UpdateEvent::Finished,
            ),
            (
                BTreeMap::from([
                    ("event".to_string(), "failed".to_string()),
                    ("message".to_string(), "network unreachable".to_string()),
                ]),
                UpdateEvent::Failed("network unreachable".to_string()),
            ),
            (
                BTreeMap::from([("event".to_string(), "reset".to_string())]),
                UpdateEvent::Reset,
            ),
        ];

        for (params, expected) in cases {
            let response = handler.handle(&ControlRequest {
                id: "update-event".into(),
                method: "update.event".into(),
                params,
            });
            assert!(response.ok, "update.event failed: {:?}", response.error);
            let mut actions = control_actions.lock().expect("action queue");
            assert_eq!(
                actions.pop().map(|action| matches!(
                    action,
                    ControlAction::UpdateEvent(ref event) if *event == expected
                )),
                Some(true),
                "expected {expected:?} to be queued"
            );
            actions.clear();
        }

        let missing_version = handler.handle(&ControlRequest {
            id: "missing-version".into(),
            method: "update.event".into(),
            params: BTreeMap::from([("event".to_string(), "available".to_string())]),
        });
        assert!(!missing_version.ok, "available without version must fail");

        let bad_percent = handler.handle(&ControlRequest {
            id: "bad-percent".into(),
            method: "update.event".into(),
            params: BTreeMap::from([
                ("event".to_string(), "download-progress".to_string()),
                ("percent".to_string(), "not-a-number".to_string()),
            ]),
        });
        assert!(!bad_percent.ok, "a non-integer percent must fail");

        let unknown_event = handler.handle(&ControlRequest {
            id: "unknown-event".into(),
            method: "update.event".into(),
            params: BTreeMap::from([("event".to_string(), "levitate".to_string())]),
        });
        assert!(!unknown_event.ok, "an unrecognized event name must fail");
        assert!(
            control_actions.lock().expect("action queue").is_empty(),
            "no rejected request may queue an action"
        );
    }

    #[test]
    fn control_notification_create_posts_exact_title_and_body() {
        let captured = Arc::new(Mutex::new(Vec::<NotificationPayload>::new()));
        let capture = captured.clone();
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
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-test-control.sock")),
        )
        .with_notification_poster(move |payload| {
            capture.lock().expect("capture lock").push(payload.clone());
        });
        let response = handler.handle(&ControlRequest {
            id: "notification-create".into(),
            method: "notification.create".into(),
            params: BTreeMap::from([
                ("title".into(), "Build finished".into()),
                ("body".into(), "The Linux build is ready.".into()),
            ]),
        });

        assert!(
            response.ok,
            "notification.create failed: {:?}",
            response.error
        );
        let posted = captured.lock().expect("capture lock");
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].title, "Build finished");
        assert_eq!(posted[0].body, "The Linux build is ready.");
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
            [
                "browser.open",
                "browser.navigate",
                "browser.act",
                "browser.get",
                "browser.wait",
                "browser.eval",
                "browser.console",
                "browser.snapshot",
                "browser.permission",
            ]
        );

        // F-CTRL-BROWSER-03/04/05/06: browser.get, browser.wait,
        // browser.eval, browser.console and browser.snapshot are now real,
        // implemented methods (see handle_browser_action) and must not be
        // pre-rejected as unsupported the way the remaining stubs are.
        for method in ["browser.get", "browser.wait", "browser.snapshot"] {
            assert_eq!(
                browser_request_error(method, &BTreeMap::new()),
                None,
                "{method} must be accepted, not pre-rejected as unsupported"
            );
        }
        assert_eq!(
            browser_request_error(
                "browser.eval",
                &BTreeMap::from([("script".to_string(), "1+1".to_string())])
            ),
            None,
            "browser.eval must be accepted with a script param, not pre-rejected as unsupported"
        );

        for method in ["browser.screenshot", "browser.errors"] {
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

    /// F-CTRL-BROWSER-05: `browser.act`'s click/fill/type/press/scroll verbs
    /// build a real JS snippet against `evaluate_script`'s contract (a
    /// self-invoking function, so the script's return value is well-defined
    /// and any thrown error surfaces through wry's own error path) instead
    /// of only flipping the `driving` flag.
    #[test]
    fn browser_act_script_builds_js_for_every_verb_and_rejects_bad_input() {
        let click = browser_act_script("click", Some("#submit"), None).expect("click builds");
        assert!(click.contains("document.querySelector(\"#submit\")"));
        assert!(click.contains(".click()"));

        let fill = browser_act_script("fill", Some("#name"), Some("Ada \"Lovelace\""))
            .expect("fill builds");
        assert!(fill.contains("document.querySelector(\"#name\")"));
        // serde_json::to_string must have escaped the embedded quote, not
        // string-concatenated it raw into the script.
        assert!(fill.contains("Ada \\\"Lovelace\\\""));
        assert!(fill.contains("dispatchEvent(new Event('input'"));

        let typed = browser_act_script("type", Some("#name"), Some("hi")).expect("type builds");
        assert!(typed.contains("el.value = \"hi\""));

        let press = browser_act_script("press", None, Some("Enter")).expect("press builds");
        assert!(press.contains("document.activeElement"));
        assert!(press.contains("KeyboardEvent('keydown'"));
        assert!(press.contains("\"Enter\""));

        let scroll_selector =
            browser_act_script("scroll", Some(".card"), None).expect("scroll-to-element builds");
        assert!(scroll_selector.contains("scrollIntoView"));

        let scroll_page = browser_act_script("scroll", None, Some("250")).expect("page scroll");
        assert!(scroll_page.contains("window.scrollBy(0, 250)"));

        assert!(
            browser_act_script("click", None, None).is_err(),
            "click without a selector is rejected, not sent as a broken script"
        );
        assert!(
            browser_act_script("press", None, None).is_err(),
            "press without a key name is rejected"
        );
        assert!(
            browser_act_script("teleport", None, None).is_err(),
            "an unknown verb is rejected, not silently ignored"
        );
        assert!(
            browser_act_script("", None, None).is_err(),
            "an empty verb (no driving flag, no verb) is rejected"
        );
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

    #[cfg(any())]
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

    #[test]
    fn chat_compose_is_queued_for_the_rendered_chat_entity() {
        let actions = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(AppControlHandler::new(
            Arc::new(Mutex::new(ControlState {
                projects: Vec::new(),
                workspaces: Vec::new(),
                current: None,
            })),
            actions.clone(),
            Arc::new(PaneRegistry::new()),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(BTreeMap::new())),
            None,
            ControlSocketInfo::new(PathBuf::from("/tmp/tiller-chat-queue-test.sock")),
        ));
        let request = ControlRequest {
            id: "chat-compose".into(),
            method: "surface.chat.compose".into(),
            params: BTreeMap::from([
                ("surfaceId".into(), "default-chat".into()),
                ("text".into(), "MARKER_P107".into()),
            ]),
        };
        let worker = std::thread::spawn({
            let handler = handler.clone();
            move || handler.handle(&request)
        });

        let action = loop {
            if let Some(action) = actions.lock().expect("action queue").pop() {
                break action;
            }
            std::thread::yield_now();
        };
        let ControlAction::Chat {
            action: ChatControlAction::Compose { surface_id, text },
            reply,
        } = action
        else {
            panic!("compose must cross the existing GPUI control-action queue");
        };
        assert_eq!(surface_id, "default-chat");
        assert_eq!(text, "MARKER_P107");
        reply
            .send(Ok(vec![("composerText".into(), "MARKER_P107".into())]))
            .expect("reply to socket worker");
        let response = worker.join().expect("socket worker did not panic");
        assert!(response.ok, "queued compose response: {response:?}");
    }

    #[cfg(any())]
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

    #[cfg(any())]
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

    #[cfg(any())]
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
                    id: "test-chat".into(),
                    title: "Chat".into(),
                    kind: "chat".into(),
                    agent_id: None,
                    active: false,
                },
                session::SessionTab {
                    id: "test-terminal".into(),
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

        let mut activity = AgentActivityModel::new();
        let (tabs, active) = cx.update(|cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });

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

    /// F-SET-04: `restored_agent_shell` is the exact site that decides
    /// whether a restored agent pane resumes a saved native session or
    /// starts fresh -- it is what both `resume_agent_sessions` gates (main.rs
    /// initial-restore and `restore_launch_snapshot`) funnel into via the
    /// `resumable` map. Previously only traced by reading; this proves the
    /// two call shapes it actually receives produce different shell
    /// commands, without needing a fabricated on-disk Claude/Codex
    /// transcript (this layer never reads one -- it only decides which
    /// command string to launch).
    #[test]
    fn restored_agent_shell_resumes_only_when_a_session_ref_is_supplied() {
        let worktree = std::env::temp_dir().join(format!(
            "tiller-f-set-04-restored-shell-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&worktree);

        // Gate ON shape: `resumable` carries this pane's saved session ref
        // (mirrors `saved_session_refs_for_restore` / `load_session_refs()`
        // when `resume_agent_sessions` is true).
        let mut resumable = BTreeMap::new();
        resumable.insert("pane-9".to_string(), "sess-abc123".to_string());
        let with_gate_on = restored_agent_shell(Some("claude"), "pane-9", &worktree, &resumable)
            .expect("claude adapter resolves a shell");
        let TerminalShell::WithArguments { args: on_args, .. } = with_gate_on else {
            panic!("expected a program+args shell");
        };
        assert!(
            on_args
                .iter()
                .any(|arg| arg.contains("--resume") && arg.contains("sess-abc123")),
            "gate on must launch with --resume <ref>, got {on_args:?}"
        );

        // Gate OFF shape: exactly what both call sites pass when
        // `resume_agent_sessions` is false -- an empty map, never the saved
        // refs.
        let empty = BTreeMap::new();
        let with_gate_off = restored_agent_shell(Some("claude"), "pane-9", &worktree, &empty)
            .expect("claude adapter resolves a shell");
        let TerminalShell::WithArguments { args: off_args, .. } = with_gate_off else {
            panic!("expected a program+args shell");
        };
        assert!(
            off_args.iter().all(|arg| !arg.contains("--resume")),
            "gate off must never launch with --resume, got {off_args:?}"
        );

        let _ = std::fs::remove_dir_all(&worktree);
    }

    #[gpui::test]
    async fn restore_tabs_registers_restored_agent_identity(cx: &mut TestAppContext) {
        let working_directory = std::env::temp_dir();
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![session::SessionTab {
                id: "restored-codex".into(),
                title: "Codex".into(),
                kind: "terminal".into(),
                agent_id: Some("codex".into()),
                active: true,
            }],
            tab_states: vec![session::SessionTabState::with_root(7)],
            diagnostics: Vec::new(),
        };
        let mut activity = AgentActivityModel::new();

        let (tabs, _) = cx.update(|cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });

        assert_eq!(tabs[0].agent_id.as_deref(), Some("codex"));
        assert_eq!(activity.agent_id("pane-7"), Some("codex"));
        assert_eq!(
            activity.status("pane-7"),
            None,
            "restore registers identity without claiming running"
        );
    }

    #[gpui::test]
    async fn add_agent_tab_persists_agent_id_in_its_very_first_save(cx: &mut TestAppContext) {
        // F-AGENT-OPENCODE-01: add_agent_tab used to set `tab.agent_id` only
        // *after* add_terminal_tab_with_shell (-> insert_terminal_tab) had
        // already called schedule_save(cx) with an eagerly-computed
        // layout(cx) snapshot -- layout() reads tab.agent_id at schedule
        // time, not at write time, so the very first persisted row for a
        // freshly-added agent tab always carried agent_id: None. A restart
        // before any later save happened to fire relaunched the pane as
        // plain shell instead of `opencode --session <ref>`. This proves the
        // fix the same way the codex round-trip test below does: schedule,
        // flush the debounced writer explicitly, then read the row back
        // through session::restore exactly as a second launch would.
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
        let working_directory =
            workspace.read_with(&cx.cx, |workspace, _| workspace.working_directory.clone());
        let session_path = working_directory
            .parent()
            .expect("palette test workspace scratch root")
            .join("tiller.sqlite");

        let adapter = AGENT_CATALOG
            .iter()
            .find(|adapter| adapter.id() == "opencode")
            .expect("opencode adapter is registered in the catalog");
        let icon = Icon::for_agent_id(adapter.id()).expect("opencode has a brand icon");
        let adapter_display_name = adapter.display_name().to_string();
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.add_agent_tab(*adapter, icon, cx);
            // Flush the debounced writer now, capturing exactly the snapshot
            // schedule_save took inside add_terminal_tab_with_shell --
            // before this test's fix, that snapshot's agent_id was still None.
            workspace.session.flush_now();
        });
        cx.run_until_parked();

        let restored = session::restore(&session_path, &working_directory);
        let agent_tab = restored
            .tabs
            .iter()
            .find(|tab| tab.title == adapter_display_name)
            .expect("the persisted layout must include the freshly-added agent tab");
        assert_eq!(
            agent_tab.agent_id.as_deref(),
            Some("opencode"),
            "the first save after add_agent_tab must already carry the agent id, \
             or a restart before any later save relaunches the pane as plain shell"
        );
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
                id: "codex-chat".into(),
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
        let mut activity = AgentActivityModel::new();
        let (tabs, active) = cx.update(|_, cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });
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

    /// F-SID-18: an otherwise selected worktree with no tabs must offer a
    /// visible terminal-first recovery, rather than the generic empty-pane
    /// message used for a detached pane group.
    #[gpui::test]
    async fn drawn_selected_worktree_without_tabs_offers_a_new_terminal(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs.clear();
            workspace.rebuild_tab_machinery();
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("empty-worktree").is_some(),
            "a selected worktree without tabs needs its terminal empty state"
        );
        assert!(
            cx.debug_bounds("empty-worktree-new-terminal").is_some(),
            "the empty state must provide the New Terminal action"
        );

        let new_terminal = cx
            .debug_bounds("empty-worktree-new-terminal")
            .expect("new terminal action is drawn");
        cx.simulate_click(new_terminal.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            1,
            "New Terminal must replace the empty state with a terminal tab"
        );
    }

    /// F-SID-19: `ctrl-t` (`WindowCommand::NewTerminalTab`) must reach the
    /// workspace even when the "No Terminals" empty state holds no
    /// focusable element of its own. GPUI's key dispatch falls back to the
    /// true window root -- above every `on_action`/`capture_key_down` this
    /// workspace registers on its own root element -- whenever nothing at
    /// all holds focus, so this reproduces the live-drive finding: land on
    /// a zero-tab worktree, drop focus the way an unmounted terminal would,
    /// and confirm the global keybinding still creates a tab rather than
    /// silently reaching nothing.
    #[gpui::test]
    async fn ctrl_t_from_the_empty_worktree_state_creates_a_terminal(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs.clear();
            workspace.rebuild_tab_machinery();
            cx.notify();
        });
        // Drop focus outright, the same way it goes missing in the live
        // app: the terminal that used to hold it is gone from this frame,
        // and nothing else has claimed it yet.
        cx.update(|window, _| window.blur());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("empty-worktree").is_some(),
            "must land on the empty-worktree state before the chord is sent"
        );

        cx.simulate_keystrokes("ctrl-t");
        cx.run_until_parked();

        assert_eq!(
            workspace.read_with(&cx.cx, |workspace, _| workspace.tabs.len()),
            1,
            "ctrl-t must create a terminal tab from the empty-worktree state, \
             the same as clicking its New Terminal button does"
        );
    }

    /// F-TERM-02: a pane group that lost its last tab to a move -- distinct
    /// from F-SID-18's group-0-with-a-worktree case above, which the comment
    /// on `drawn_selected_worktree_without_tabs_offers_a_new_terminal`
    /// explicitly calls out as a different state -- must fall back to the
    /// real `TerminalView::empty_prompt` surface, not the bare "No tabs in
    /// this pane" label the code used to draw with no way back into the
    /// group at all.
    #[gpui::test]
    async fn drawn_detached_pane_group_offers_the_real_empty_prompt(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window =
            cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 2));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        // Move tab 1 into a brand-new second pane group, then move that same
        // tab straight back to group 0. Nothing ever collapses the second
        // group -- `TabMachinery::move_tab` only ever empties a group's
        // `tabs` list, it never removes the group itself -- so this leaves a
        // genuinely detached, non-zero-id, tabless pane group behind.
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.active_tab = 1;
            workspace.move_selected_tab_to_new_pane(cx);
        });
        cx.run_until_parked();
        let detached_group_id = workspace.read_with(&cx.cx, |workspace, _| {
            workspace
                .tab_machinery
                .groups()
                .iter()
                .map(|group| group.id)
                .find(|id| *id != 0)
                .expect("move_selected_tab_to_new_pane created a second group")
        });
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.move_selected_tab(MoveTarget::Group(0), cx);
        });
        cx.run_until_parked();
        assert!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace
                    .tab_machinery
                    .groups()
                    .iter()
                    .any(|group| group.id == detached_group_id && group.tabs.is_empty())
            }),
            "the second group must survive with zero tabs, not collapse away"
        );

        assert!(
            cx.debug_bounds("pane-group-empty").is_some(),
            "the detached group must draw the empty-pane surface"
        );
        assert!(
            cx.debug_bounds("terminal-new").is_some(),
            "the empty prompt's New Terminal action must be visible, not a static label"
        );
        assert!(
            cx.debug_bounds("terminal-new-command").is_some(),
            "the empty prompt's New… action must be visible, not a static label"
        );

        let new_terminal = cx
            .debug_bounds("terminal-new")
            .expect("New Terminal action is drawn");
        cx.simulate_click(new_terminal.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace
                    .tabs
                    .iter()
                    .any(|tab| tab.group_id == detached_group_id)
            }),
            "New Terminal must land the fresh tab back in the pane group the user clicked in, \
             not silently in whichever group happened to be active before"
        );
    }

    /// F-TERM-11: a deselected workspace must cover retained terminal tabs
    /// with an explicit no-worktree state, not leave stale PTY output visible.
    #[gpui::test]
    async fn drawn_deselected_worktree_replaces_terminals_with_an_empty_state(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            let path = workspace.working_directory.clone();
            assert!(
                workspace
                    .control_state
                    .lock()
                    .expect("control state")
                    .close_worktree(&path)
            );
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("no-worktree-selected").is_some(),
            "the centre surface must explain that no worktree is selected"
        );
        assert!(
            cx.debug_bounds("pane-0").is_none(),
            "retained terminal output must not remain visible without a worktree"
        );
    }

    /// F-CHG-02: closing the selected worktree must clear the panel's bound
    /// path as well as the centre surface, so it cannot present old Git data
    /// as the current selection.
    #[gpui::test]
    async fn drawn_closed_worktree_clears_the_right_panel_and_blocks_changes_open(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            let selector = workspace
                .control_state
                .lock()
                .expect("control state")
                .current_workspace()
                .expect("selected test worktree")
                .id
                .clone();
            workspace
                .close_workspace(&selector, cx)
                .expect("close selected test worktree");
            assert_eq!(
                workspace.control_open_changes(None, None, cx),
                Err("no current workspace".to_string()),
                "Changes must not reopen against the last closed worktree"
            );
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("right-panel-no-worktree").is_some(),
            "the right panel must explain that no worktree is selected"
        );
        assert!(
            cx.debug_bounds("right-panel-files").is_none(),
            "the old worktree's file list must be unmounted"
        );
    }

    /// F-TAB-25: the terminal pane menu exposes Attach only for a terminal
    /// owned by another tab, and the command moves that live leaf into the
    /// active terminal's split tree.
    #[gpui::test]
    async fn drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 2));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| {
            workspace.active_tab = 1;
            assert!(workspace.tab_machinery.select_tab(0, 1));
            workspace.sync_activity(cx);
            cx.notify();
        });
        cx.run_until_parked();

        let source_tab = cx
            .debug_bounds("workspace-tab-0")
            .expect("source terminal tab");
        cx.simulate_mouse_down(source_tab.center(), MouseButton::Right, Modifiers::none());
        let attach = cx
            .debug_bounds("tab-command-attach-to-current-terminal")
            .expect("Attach to Current Terminal command is drawn for an eligible pane");
        cx.simulate_click(attach.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            workspace.read_with(&cx.cx, |workspace, _| {
                workspace.tabs.len() == 1
                    && workspace.tabs[0].id == 1
                    && workspace.tabs[0].panes.leaf_ids() == vec![1, 0]
            }),
            "Attach must preserve the source pane id and join it to the current terminal"
        );
    }

    #[gpui::test]
    async fn drawn_terminal_attach_command_is_disabled_for_the_current_terminal(
        cx: &mut TestAppContext,
    ) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });
        workspace.update(&mut cx, |workspace, cx| workspace.open_tab_menu(0, cx));
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("tab-command-disabled-attach-to-current-terminal")
                .is_some(),
            "a terminal cannot attach itself to the current terminal"
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

    /// F-CORE-WSP-08: `layout()` reads a chat tab's live composer text the
    /// same way it already reads a terminal's live scrollback — proven by
    /// typing through the real `Chat` entity (never pressing Enter) and
    /// reading the resulting `SessionLayout` back out.
    #[gpui::test]
    async fn layout_captures_the_live_unsent_chat_draft(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| {
            let mut workspace = palette_test_workspace(cx);
            let chat = cx.new(|cx| {
                Chat::launch_with_command(
                    AgentCommand::new("/definitely/missing/tiller-acp-agent"),
                    std::env::temp_dir(),
                    cx,
                )
            });
            workspace.tabs[0] = OpenTab {
                id: 0,
                persistence_id: "test-chat".into(),
                group_id: 0,
                title: "Chat".into(),
                kind: TabKind::AgentChat,
                agent_icon: Some(Icon::Codex),
                agent_id: Some("codex".into()),
                session_state: SessionTabState::with_root(0),
                panes: PaneNode::leaf(0, TabContent::Chat(chat)),
                focused_pane: 0,
                title_is_auto_named: true,
            };
            workspace.rebuild_tab_machinery();
            workspace
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        workspace.update(&mut cx, |workspace, cx| {
            workspace.tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Chat(chat) = content {
                    chat.update(cx, |chat, cx| chat.control_compose("an unsent idea", cx));
                }
            });
        });
        cx.run_until_parked();

        let draft = workspace.update(&mut cx, |workspace, cx| {
            workspace.layout(cx).tab_states[0].chat_draft.clone()
        });
        assert_eq!(
            draft, "an unsent idea",
            "an unsent composer draft must be captured into the session snapshot"
        );
    }

    /// F-CORE-WSP-08's other half: a persisted `chat_draft` is pushed back
    /// into the freshly-restored `Chat` entity's composer through the same
    /// `control_compose` the control socket already uses.
    #[gpui::test]
    async fn restore_tabs_seeds_the_composer_with_the_persisted_draft(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let working_directory = std::env::current_dir().expect("current directory");
        let restored = session::RestoredSession {
            working_directory: working_directory.clone(),
            tabs: vec![session::SessionTab {
                id: "restored-chat".into(),
                title: "Chat".into(),
                kind: "chat".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![session::SessionTabState {
                root_id: Some(0),
                pane_events: Vec::new(),
                scrollback: std::collections::BTreeMap::new(),
                chat_draft: "an idea I never sent".into(),
            }],
            diagnostics: Vec::new(),
        };

        let mut activity = AgentActivityModel::new();
        let (tabs, _) = cx.update(|cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });

        let draft = cx.update(|cx| {
            let mut draft = None;
            tabs[0].panes.for_each(&mut |_, content| {
                if let TabContent::Chat(chat) = content {
                    draft = Some(chat.read(cx).draft_text());
                }
            });
            draft.expect("restored chat tab has a chat pane")
        });
        assert_eq!(draft, "an idea I never sent");
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
                id: "missing-terminal".into(),
                title: "Terminal".into(),
                kind: "terminal".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![session::SessionTabState {
                root_id: Some(0),
                pane_events: Vec::new(),
                scrollback: std::collections::BTreeMap::from([(0, nonce.clone())]),
                chat_draft: String::new(),
            }],
            diagnostics: Vec::new(),
        };

        let mut activity = AgentActivityModel::new();
        let (mut tabs, _) = cx.update(|cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });
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
                id: "changes-tab".into(),
                title: "Changes".into(),
                kind: "diff".into(),
                agent_id: None,
                active: true,
            }],
            tab_states: vec![session::SessionTabState::default()],
            diagnostics: vec![],
        };

        let mut activity = AgentActivityModel::new();
        let (tabs, active) = cx.update(|cx| {
            restore_tabs(
                &restored,
                &working_directory,
                None,
                &mut activity,
                &BTreeMap::new(),
                cx,
            )
        });

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

    /// F-CORE-ACT-26: once the mounted-worktree cap is exceeded, selecting
    /// a new worktree evicts the oldest idle mounted worktree (never the
    /// one just selected) — proven through the real
    /// `evict_over_capacity_worktrees` call site `select_worktree` invokes,
    /// not just the pure `WorktreeMountPolicy` unit test.
    #[gpui::test]
    async fn selecting_past_the_mount_cap_evicts_the_oldest_idle_worktree(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        workspace.update(&mut cx, |workspace, cx| {
            let unique = TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("tiller-mount-cap-{}-{unique}", std::process::id()));
            let paths: Vec<PathBuf> = ["main", "second", "third"]
                .iter()
                .map(|name| root.join(name))
                .collect();
            for path in &paths {
                std::fs::create_dir_all(path).expect("create fixture worktree");
            }
            workspace.project_catalog =
                ProjectCatalog::from_projects(vec![session::CatalogProject {
                    id: "mount-cap-project".into(),
                    name: "Mount Cap Project".into(),
                    root_path: paths[0].clone(),
                    is_git: true,
                    worktrees: paths
                        .iter()
                        .enumerate()
                        .map(|(index, path)| session::CatalogWorktree {
                            branch: format!("branch-{index}"),
                            path: path.clone(),
                            is_primary: index == 0,
                        })
                        .collect(),
                }]);
            workspace.control_state = Arc::new(Mutex::new(ControlState::from_catalog(
                &workspace.project_catalog,
                &paths[0],
            )));
            // Simulate both other worktrees already mounted from earlier
            // selections, so the cap of two is exceeded the moment a third
            // is selected.
            {
                let mut state = workspace.control_state.lock().expect("control state");
                for workspace in &mut state.workspaces {
                    workspace.mounted = true;
                }
            }
            workspace.settings = cx.new(|cx| {
                Settings::with_snapshot(
                    cx,
                    SettingsSnapshot {
                        limit_mounted_worktrees: true,
                        mounted_worktrees: 2,
                        ..SettingsSnapshot::default()
                    },
                )
            });

            workspace
                .select_worktree(paths[2].clone(), None, cx)
                .expect("select the third worktree");

            let state = workspace.control_state.lock().expect("control state");
            let mounted: Vec<bool> = paths
                .iter()
                .map(|path| {
                    state
                        .workspaces
                        .iter()
                        .find(|workspace| Path::new(&workspace.path) == path)
                        .expect("fixture worktree row")
                        .mounted
                })
                .collect();
            assert_eq!(
                mounted,
                vec![false, true, true],
                "the oldest idle worktree (main) is evicted; the just-selected \
                 worktree (third) and the other still-open one (second) stay mounted"
            );
        });
    }

    /// F-USE-04: the tray roster only lists worktrees with a live agent
    /// pane, sourced from `control_state` (populated here the same way
    /// `project.add`/`worktree.set` populate it live) rather than
    /// `project_catalog`, and orders them by [`AgentStatus`] priority --
    /// the worse status leads regardless of insertion order.
    #[gpui::test]
    async fn tray_roster_lists_only_active_worktrees_sorted_by_urgency(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        workspace.update(&mut cx, |workspace, _cx| {
            let unique = TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "tiller-tray-roster-{}-{unique}",
                std::process::id()
            ));
            let paths: Vec<PathBuf> = ["alpha", "beta", "gamma"]
                .iter()
                .map(|name| root.join(name))
                .collect();
            for path in &paths {
                std::fs::create_dir_all(path).expect("create fixture worktree");
            }
            workspace.project_catalog =
                ProjectCatalog::from_projects(vec![session::CatalogProject {
                    id: "tray-roster-project".into(),
                    name: "Tray Roster Project".into(),
                    root_path: paths[0].clone(),
                    is_git: true,
                    worktrees: paths
                        .iter()
                        .enumerate()
                        .map(|(index, path)| session::CatalogWorktree {
                            branch: format!("branch-{index}"),
                            path: path.clone(),
                            is_primary: index == 0,
                        })
                        .collect(),
                }]);
            workspace.control_state = Arc::new(Mutex::new(ControlState::from_catalog(
                &workspace.project_catalog,
                &paths[0],
            )));

            // alpha finishes, beta errors, gamma has a pane but no notified
            // status yet -- only alpha and beta should reach the roster.
            workspace
                .panes
                .set_external(
                    &paths[0],
                    vec![PaneInfo {
                        id: "pane-alpha".into(),
                        tab: "control".into(),
                        title: "shell".into(),
                        agent: String::new(),
                        active: true,
                    }],
                )
                .expect("register alpha pane");
            workspace
                .panes
                .set_external(
                    &paths[1],
                    vec![PaneInfo {
                        id: "pane-beta".into(),
                        tab: "control".into(),
                        title: "shell".into(),
                        agent: String::new(),
                        active: true,
                    }],
                )
                .expect("register beta pane");
            workspace
                .panes
                .set_external(
                    &paths[2],
                    vec![PaneInfo {
                        id: "pane-gamma".into(),
                        tab: "control".into(),
                        title: "shell".into(),
                        agent: String::new(),
                        active: true,
                    }],
                )
                .expect("register gamma pane");
            workspace
                .activity
                .notify("pane-alpha", AgentStatus::Done, Instant::now());
            workspace
                .activity
                .notify("pane-beta", AgentStatus::Error, Instant::now());

            let roster = workspace.tray_roster_snapshot();
            let summary: Vec<(String, AgentStatus)> = roster
                .iter()
                .map(|entry| (entry.branch.clone(), entry.status))
                .collect();
            assert_eq!(
                summary,
                vec![
                    ("branch-1".to_string(), AgentStatus::Error),
                    ("branch-0".to_string(), AgentStatus::Done),
                ],
                "gamma (no notified status) is absent; beta (Error) outranks \
                 alpha (Done) despite being added second"
            );
        });
    }

    /// I3-tray-jump / F-USE-05: `select_worktree_and_jump` -- the exact
    /// method both the tray's own roster-row click and the control
    /// socket's `tray.jump` drive -- must land on the target worktree's
    /// own worst-status tab, not whichever tab merely happens to be
    /// active beforehand. Two tabs exist (`palette_test_workspace_with_tab_count`
    /// gives them pane ids 0 and 1, matching the `pane-{id}` key
    /// `AgentActivityModel` is notified under); tab 1 is pushed to
    /// `NeedsInput` while the workspace is currently showing a *different*
    /// worktree (mirroring a real roster click arriving while another
    /// worktree is on screen) -- reproduces live wayland-drive.sh evidence,
    /// wave-I, 2026-08-16.
    #[gpui::test]
    async fn tray_jump_lands_on_the_target_worktrees_worst_status_tab(cx: &mut TestAppContext) {
        cx.set_global(Theme::light());
        let window = cx.add_window(|_window, cx| palette_test_workspace_with_tab_count(cx, 2));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = cx.update(|window, _| {
            window
                .root::<TillerWorkspace>()
                .flatten()
                .expect("workspace root")
        });

        let target_path =
            workspace.read_with(&cx.cx, |workspace, _| workspace.working_directory.clone());
        workspace.update(&mut cx, |workspace, cx| {
            let unique = TEST_WORKSPACE_ID.fetch_add(1, AtomicOrdering::Relaxed);
            let other_path = std::env::temp_dir().join(format!(
                "tiller-tray-jump-other-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&other_path).expect("create second fixture worktree");

            // Widen the single-worktree fixture to two worktrees under one
            // project, then rebuild `control_state` with the *other*
            // worktree already selected -- the workspace is "currently
            // showing" `other_path` when the jump request arrives, exactly
            // like a roster click for a worktree that is not the one on
            // screen.
            let project = workspace.project_catalog.projects()[0].clone();
            let mut worktrees = project.worktrees.clone();
            worktrees.push(session::CatalogWorktree {
                branch: "other".into(),
                path: other_path.clone(),
                is_primary: false,
            });
            workspace.project_catalog =
                ProjectCatalog::from_projects(vec![session::CatalogProject {
                    worktrees,
                    ..project
                }]);
            workspace.control_state = Arc::new(Mutex::new(ControlState::from_catalog(
                &workspace.project_catalog,
                &other_path,
            )));
            workspace.working_directory = other_path;

            // `palette_test_workspace_with_tab_count`'s tabs use
            // `TerminalView::failed`, which reports `ActivityStatus::Error`
            // on its own and would mask the notify below (both tabs tying
            // at Error, first-wins). Swap in real, non-failed terminals so
            // `tab_status` falls through to `self.activity`, exactly as it
            // does for a live PTY that hasn't exited.
            for tab in workspace.tabs.iter_mut() {
                let pane_id = tab.focused_pane;
                tab.panes = PaneNode::leaf(
                    pane_id,
                    TabContent::Terminal {
                        view: cx.new(|cx| {
                            TerminalView::new(&target_path, cx).expect("spawn a real test terminal")
                        }),
                    },
                );
            }

            // Tab 1 (pane id 1) is worse than tab 0's default Idle.
            workspace
                .activity
                .notify("pane-1", AgentStatus::NeedsInput, Instant::now());

            let jump = workspace
                .select_worktree_and_jump(target_path.clone(), None, cx)
                .expect("select_worktree_and_jump succeeds for a known worktree");
            assert_eq!(
                jump,
                Some((1, "Terminal 1".to_string())),
                "the needs-input tab (id 1) must win over the idle tab (id 0), \
                 regardless of which worktree was on screen when the jump ran"
            );
            assert_eq!(
                workspace.working_directory, target_path,
                "the jump must also have actually selected the target worktree"
            );
            assert_eq!(
                workspace.active_tab, 1,
                "select_tab must have made the needs-input tab the active one"
            );
        });
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

    // F-CORE-DOM-07: the real trigger-and-sink side of `AutoNamingThrottle`
    // — the throttle's own gating logic already has a pure unit test in
    // `tiller_project::domain`; these cover the production plumbing wired
    // on top of it in this file.

    #[test]
    fn auto_naming_prompt_matches_the_reference_wording() {
        let prompt = auto_naming_prompt("user: hi\nassistant: hello");
        assert!(
            prompt.starts_with(
                "Summarize this coding-agent conversation into a short title, 2-5 words"
            )
        );
        assert!(prompt.ends_with("user: hi\nassistant: hello"));
    }

    #[test]
    fn summarizer_candidates_prefer_the_selected_agent_then_the_tab_agent() {
        // I1-autoname: all five adapters now have a ported `summarizer_command`.
        let commands = summarizer_candidate_commands("opencode", Some("omp"), "prompt");
        assert_eq!(commands.len(), 2);
        assert!(commands[0].starts_with("opencode run --pure"));
        assert!(commands[1].starts_with("oh-my-pi --print --no-tools"));

        // Selected + fallback, both ported: two candidates in priority order.
        let commands = summarizer_candidate_commands("claude", Some("opencode"), "prompt");
        assert_eq!(
            commands,
            vec![
                "claude -p 'prompt'".to_string(),
                "opencode run --pure 'prompt'".to_string(),
            ]
        );

        // Same agent selected and fallback: no duplicate entry.
        let commands = summarizer_candidate_commands("omp", Some("omp"), "prompt");
        assert_eq!(commands.len(), 1);

        // Default chat tab: default selected agent (claude) and no tab
        // agent (`agent_id: None`) — the exact route this row's regression
        // named. This must no longer be empty.
        let commands = summarizer_candidate_commands("claude", None, "prompt");
        assert_eq!(commands, vec!["claude -p 'prompt'".to_string()]);

        // Both candidates ported but distinct: still two, in order.
        let commands = summarizer_candidate_commands("claude", Some("pi"), "prompt");
        assert_eq!(
            commands,
            vec![
                "claude -p 'prompt'".to_string(),
                "pi --print --no-tools 'prompt'".to_string(),
            ]
        );
    }

    #[test]
    fn run_summarizer_command_trims_and_truncates_real_process_output() {
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let title = run_summarizer_command(
            "printf '  Fix the login race  \\n'",
            &cwd,
            Duration::from_secs(5),
        );
        assert_eq!(title, Some("Fix the login race".to_string()));

        let long = "x".repeat(200);
        let title = run_summarizer_command(
            &format!("printf '%s' '{long}'"),
            &cwd,
            Duration::from_secs(5),
        );
        assert_eq!(
            title.map(|title| title.len()),
            Some(AUTO_NAMING_MAX_TITLE_LEN)
        );
    }

    #[test]
    fn run_summarizer_command_returns_none_for_empty_output_or_missing_binary() {
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        assert_eq!(
            run_summarizer_command("true", &cwd, Duration::from_secs(5)),
            None
        );
        assert_eq!(
            run_summarizer_command(
                "/definitely/not/a/real/binary --summarize",
                &cwd,
                Duration::from_secs(5)
            ),
            None
        );
    }

    #[test]
    fn run_summarizer_command_kills_and_returns_none_on_timeout() {
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let start = std::time::Instant::now();
        let title = run_summarizer_command(
            "sleep 5 && printf too-late",
            &cwd,
            Duration::from_millis(200),
        );
        assert_eq!(title, None);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "a timed-out summarizer must not block the caller for the full sleep"
        );
    }
}

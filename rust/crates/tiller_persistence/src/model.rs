//! The records the persistence layer stores: projects, worktrees, tabs,
//! application settings and sidebar state.
//!
//! These are plain value types, deliberately separate from the domain types
//! in `tiller_project` — exactly as the Swift app keeps `ProjectRecord`
//! separate from `Project`. Ids are opaque strings (the Swift app uses UUID
//! strings; callers own the format). Column names are snake_case; the data
//! mirrors what the Swift `TillerPersistence` package persists.

/// A project the user added, as persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRecord {
    /// Opaque stable id (a UUID string in the Swift app).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Absolute path of the project root.
    pub root_path: String,
    /// Position in the sidebar, 0-based; maintained by the save functions.
    pub order_idx: i64,
    /// Per-project settings the Swift layer keeps.
    pub color_hex: Option<String>,
    pub display_name: Option<String>,
    /// Icon presentation: "avatar" | "icon" | "emoji" (Swift `IconKind`).
    pub icon_kind: String,
    pub icon_value: Option<String>,
    pub avatar_image: Option<Vec<u8>>,
    pub default_worktree_base: Option<String>,
    pub worktree_location_override: Option<String>,
}

impl ProjectRecord {
    /// A project with the default icon presentation ("icon", like Swift's
    /// `IconKind.icon`).
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        root_path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            root_path: root_path.into(),
            order_idx: 0,
            color_hex: None,
            display_name: None,
            icon_kind: "icon".to_string(),
            icon_value: None,
            avatar_image: None,
            default_worktree_base: None,
            worktree_location_override: None,
        }
    }
}

/// A git worktree of a project, as persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeRecord {
    /// Opaque stable id.
    pub id: String,
    /// The project this worktree belongs to.
    pub project_id: String,
    /// Branch label (the Swift app falls back to "main" for the primary
    /// checkout of a detached HEAD).
    pub branch: String,
    /// Absolute path of the checkout.
    pub path: String,
    /// Position within the project's worktree list, 0-based.
    pub order_idx: i64,
    /// Whether this is the primary checkout (git main worktree).
    pub is_primary: bool,
    /// User-authored sidebar note for this worktree.
    pub comment: Option<String>,
    /// Creation time as Unix milliseconds, when known.
    pub created_at: Option<i64>,
    /// Last update time as Unix milliseconds, when known.
    pub updated_at: Option<i64>,
}

impl WorktreeRecord {
    pub fn new(
        id: impl Into<String>,
        project_id: impl Into<String>,
        branch: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            project_id: project_id.into(),
            branch: branch.into(),
            path: path.into(),
            order_idx: 0,
            is_primary: false,
            comment: None,
            created_at: None,
            updated_at: None,
        }
    }
}

/// One open tab inside a worktree, as persisted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRecord {
    /// Opaque stable id.
    pub id: String,
    /// The worktree this tab lives in.
    pub worktree_id: String,
    /// The tab's title.
    pub title: String,
    /// Surface kind: "terminal" | "chat" | "browser" | "editor" | "diff".
    pub kind: String,
    /// Stable agent identity for chat tabs, when one has been recorded.
    pub agent_id: Option<String>,
    /// Position in the worktree's tab strip, 0-based.
    pub order_idx: i64,
    /// Whether this tab is the active one in its worktree. At most one tab
    /// per worktree is active; saves normalize this invariant.
    pub is_active: bool,
}

impl TabRecord {
    pub fn new(
        id: impl Into<String>,
        worktree_id: impl Into<String>,
        title: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            worktree_id: worktree_id.into(),
            title: title.into(),
            kind: kind.into(),
            agent_id: None,
            order_idx: 0,
            is_active: false,
        }
    }

    /// Records which agent owns this tab's chat connection.
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }
}

/// Opaque per-tab state owned by a surface host. The persistence layer keeps
/// the JSON opaque so the schema can store pane/layout state without knowing
/// about GPUI or terminal types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStateRecord {
    pub tab_id: String,
    pub state: String,
}

/// A stored row removed from active state because its payload could not be
/// materialized. The original bytes remain available for diagnosis or recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedRecord {
    /// Monotonic SQLite id of the quarantine entry.
    pub id: i64,
    /// Source table or logical record kind, such as `tab_state`.
    pub record_type: String,
    /// Stable source identity when it could be decoded, otherwise a row id.
    pub record_id: String,
    /// Original serialized bytes.
    pub payload: Vec<u8>,
    /// Why the row was not materialized.
    pub reason: String,
    /// Quarantine time as Unix milliseconds.
    pub quarantined_at: i64,
}

impl TabStateRecord {
    pub fn new(tab_id: impl Into<String>, state: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.into(),
            state: state.into(),
        }
    }
}

/// One rendered chat transcript owned by a chat tab.
///
/// This is deliberately independent of ACP wire messages. The chat surface
/// maps settled protocol events into these rendered values before handing a
/// snapshot to persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTranscript {
    /// Stable id of the owning chat tab.
    pub tab_id: String,
    /// Completed turns in display order, oldest first.
    pub turns: Vec<ChatTurn>,
}

/// Summary used by the chat-history browser. The activity value is Unix
/// milliseconds written when the transcript snapshot was saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSessionSummary {
    /// Stable tab/session identity.
    pub tab_id: String,
    /// The persisted shell-tab title.
    pub title: String,
    /// The agent associated with the chat tab, when known.
    pub agent_id: Option<String>,
    /// Number of complete turns currently retained.
    pub turn_count: usize,
    /// Last transcript save time in Unix milliseconds.
    pub last_activity: i64,
}

/// One completed rendered chat turn.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatTurn {
    /// Rendered entries in display order, oldest first.
    pub entries: Vec<ChatEntry>,
}

/// A rendered entry that can be restored without replaying ACP traffic.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatEntry {
    /// The user's submitted prompt.
    UserMessage { text: String },
    /// Rendered assistant prose (stored as source text, not a parsed view tree).
    AssistantMessage { text: String },
    /// Rendered assistant reasoning text.
    Thought { text: String },
    /// A tool card, including its terminal status when the turn settled.
    ToolCall {
        id: String,
        title: String,
        status: String,
    },
    /// A permission card and the outcome selected by the user or agent.
    /// `title` names the tool that asked (an empty string predates the
    /// field and renders as the generic label).
    Permission {
        request_id: u64,
        #[serde(default)]
        title: String,
        options: Vec<ChatPermissionOption>,
        outcome: ChatPermissionOutcome,
    },
    /// The agent's execution plan, replaced in place as entries advance.
    Plan { entries: Vec<ChatPlanEntry> },
    /// The rendered footer that closes a completed turn.
    TurnFooter { text: String },
    /// A permanent error that belongs in the restored transcript.
    Error { message: String, retryable: bool },
}

/// One option rendered in a permission card.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatPermissionOption {
    /// Protocol option id used when answering the live request.
    pub id: String,
    /// Human-readable option label.
    pub name: String,
    /// Protocol-defined permission kind.
    pub kind: String,
}

/// The durable outcome of a rendered permission card.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatPermissionOutcome {
    /// The card was persisted before the live request was answered.
    Pending,
    /// The user selected one of the offered options.
    Selected { option_id: String, label: String },
    /// The request was cancelled rather than selected.
    Cancelled,
    /// The agent or client timed out while waiting for a choice.
    TimedOut,
    /// The turn ended while the request was still unanswered, so the card
    /// is no longer answerable (F-CHAT-27).
    Expired,
}

/// One rendered row of an agent plan (F-CHAT-24).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatPlanEntry {
    /// Human-readable description of what this task aims to accomplish.
    pub content: String,
    /// Wire status: `pending`, `in_progress`, or `completed`.
    pub status: String,
}

/// Maximum sum of serialized `ChatTurn` payload bytes retained for one tab.
pub const MAX_CHAT_TRANSCRIPT_BYTES: usize = 1_048_576;

/// The app appearance, mirroring Swift's `AppAppearance` raw values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceMode {
    System,
    Light,
    Dark,
}

impl AppearanceMode {
    pub fn raw(self) -> &'static str {
        match self {
            AppearanceMode::System => "system",
            AppearanceMode::Light => "light",
            AppearanceMode::Dark => "dark",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "system" => Some(AppearanceMode::System),
            "light" => Some(AppearanceMode::Light),
            "dark" => Some(AppearanceMode::Dark),
            _ => None,
        }
    }
}

/// The file-icon theme for the Files explorer, mirroring Swift's
/// `FileIconTheme` raw values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileIconTheme {
    SfSymbols,
    Material,
}

impl FileIconTheme {
    pub fn raw(self) -> &'static str {
        match self {
            FileIconTheme::SfSymbols => "sfSymbols",
            FileIconTheme::Material => "material",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "sfSymbols" => Some(FileIconTheme::SfSymbols),
            "material" => Some(FileIconTheme::Material),
            _ => None,
        }
    }
}

/// The application settings the Swift app keeps in UserDefaults, persisted
/// here as key-value rows under the *exact* Swift key names. Loading applies
/// the Swift defaults for keys that were never written, and clamps font
/// sizes into the Swift ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSettings {
    /// "appearance.theme" — default: system.
    pub appearance: AppearanceMode,
    /// "appearance.uiFontSize" — default 13, clamped to 10...20.
    pub ui_font_size: i64,
    /// "appearance.terminalFontSize" — default 13, clamped to 9...24.
    pub terminal_font_size: i64,
    /// "appearance.fileIconTheme" — default: sfSymbols.
    pub file_icon_theme: FileIconTheme,
    /// "controlSocket.enabled" — default: true.
    pub control_socket_enabled: bool,
    /// "session.resumeAgentSessions" — default: true.
    pub resume_agent_sessions: bool,
    /// "general.autoNaming" — default: false.
    pub auto_naming: bool,
    /// "chat.limitHistory" — default: true.
    pub limit_chat_history: bool,
    /// "chat.retentionCount" — default 100, clamped to 5...500.
    pub chat_retention: i64,
    /// "worktrees.limitMounted" — default: false.
    pub limit_mounted_worktrees: bool,
    /// "worktrees.mountedCount" — default 6, clamped to 2...50.
    pub mounted_worktrees: i64,
    /// "general.summarizerAgent" — default: claude.
    pub summarizer_agent: String,
    /// "usage.claudeVisible" — default: true.
    pub claude_show_in_bar: bool,
    /// "usage.codexVisible" — default: true.
    pub codex_show_in_bar: bool,
    /// "usage.opencodeVisible" — default: false.
    pub opencode_show_in_bar: bool,
    /// "usage.ollamaVisible" — default: false.
    pub ollama_show_in_bar: bool,
    /// "usage.refreshIntervalMin" — default 5, clamped to 1...60.
    pub refresh_interval_min: i64,
    /// "usage.opencodeGo.workspaceIdOverride" — default: empty (the
    /// fetcher discovers the workspace from `/_server`). Free text, no
    /// clamp: the Swift `@AppStorage` field it mirrors is unvalidated.
    pub opencode_workspace_id_override: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: AppearanceMode::System,
            ui_font_size: 13,
            terminal_font_size: 13,
            file_icon_theme: FileIconTheme::SfSymbols,
            control_socket_enabled: true,
            resume_agent_sessions: true,
            auto_naming: false,
            limit_chat_history: true,
            chat_retention: 100,
            limit_mounted_worktrees: false,
            mounted_worktrees: 6,
            summarizer_agent: "claude".to_owned(),
            claude_show_in_bar: true,
            codex_show_in_bar: true,
            opencode_show_in_bar: false,
            ollama_show_in_bar: false,
            refresh_interval_min: 5,
            opencode_workspace_id_override: String::new(),
        }
    }
}

/// The Swift keys each setting is stored under, verbatim from
/// `TillerCore/AppSettings.swift` (plus `appearance.theme` from
/// `AppAppearance`).
pub mod settings_keys {
    pub const APPEARANCE_THEME: &str = "appearance.theme";
    pub const UI_FONT_SIZE: &str = "appearance.uiFontSize";
    pub const TERMINAL_FONT_SIZE: &str = "appearance.terminalFontSize";
    pub const FILE_ICON_THEME: &str = "appearance.fileIconTheme";
    pub const CONTROL_SOCKET_ENABLED: &str = "controlSocket.enabled";
    pub const RESUME_AGENT_SESSIONS: &str = "session.resumeAgentSessions";
    pub const AUTO_NAMING: &str = "general.autoNaming";
    pub const LIMIT_CHAT_HISTORY: &str = "chat.limitHistory";
    pub const CHAT_RETENTION: &str = "chat.retentionCount";
    pub const LIMIT_MOUNTED_WORKTREES: &str = "worktrees.limitMounted";
    pub const MOUNTED_WORKTREES: &str = "worktrees.mountedCount";
    pub const SUMMARIZER_AGENT: &str = "general.summarizerAgent";
    pub const CLAUDE_SHOW_IN_BAR: &str = "usage.claudeVisible";
    pub const CODEX_SHOW_IN_BAR: &str = "usage.codexVisible";
    pub const OPENCODE_SHOW_IN_BAR: &str = "usage.opencodeVisible";
    pub const OLLAMA_SHOW_IN_BAR: &str = "usage.ollamaVisible";
    pub const REFRESH_INTERVAL_MIN: &str = "usage.refreshIntervalMin";
    pub const OPENCODE_WORKSPACE_ID_OVERRIDE: &str = "usage.opencodeGo.workspaceIdOverride";
}

/// The Swift ranges settings values are clamped into.
pub mod settings_ranges {
    /// `AppSettings.uiFontSizeRange`
    pub const UI_FONT_SIZE: std::ops::RangeInclusive<i64> = 10..=20;
    /// `AppSettings.terminalFontSizeRange`
    pub const TERMINAL_FONT_SIZE: std::ops::RangeInclusive<i64> = 9..=24;
    pub const CHAT_RETENTION: std::ops::RangeInclusive<i64> = 5..=500;
    pub const MOUNTED_WORKTREES: std::ops::RangeInclusive<i64> = 2..=50;
    pub const REFRESH_INTERVAL_MIN: std::ops::RangeInclusive<i64> = 1..=60;
}

/// Per-worktree UI state for the sidebar: which projects are expanded and
/// which worktree is selected. The Swift app keeps the selected worktree in
/// UserDefaults (`session.selectedWorktreeId`) and does not persist
/// expansion; the Rust rewrite persists both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarState {
    /// Project ids whose worktree rows are shown in the sidebar.
    pub expanded_project_ids: Vec<String>,
    /// The selected worktree, if any.
    pub selected_worktree_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_match_swift() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.appearance, AppearanceMode::System);
        assert_eq!(defaults.ui_font_size, 13);
        assert_eq!(defaults.terminal_font_size, 13);
        assert_eq!(defaults.file_icon_theme, FileIconTheme::SfSymbols);
        assert!(defaults.control_socket_enabled);
    }

    #[test]
    fn settings_keys_match_swift_exactly() {
        assert_eq!(settings_keys::APPEARANCE_THEME, "appearance.theme");
        assert_eq!(settings_keys::UI_FONT_SIZE, "appearance.uiFontSize");
        assert_eq!(
            settings_keys::TERMINAL_FONT_SIZE,
            "appearance.terminalFontSize"
        );
        assert_eq!(settings_keys::FILE_ICON_THEME, "appearance.fileIconTheme");
        assert_eq!(
            settings_keys::CONTROL_SOCKET_ENABLED,
            "controlSocket.enabled"
        );
    }

    #[test]
    fn enums_parse_their_swift_raw_values() {
        assert_eq!(
            AppearanceMode::parse("system"),
            Some(AppearanceMode::System)
        );
        assert_eq!(AppearanceMode::parse("light"), Some(AppearanceMode::Light));
        assert_eq!(AppearanceMode::parse("dark"), Some(AppearanceMode::Dark));
        assert_eq!(AppearanceMode::parse("banana"), None);
        assert_eq!(
            FileIconTheme::parse("sfSymbols"),
            Some(FileIconTheme::SfSymbols)
        );
        assert_eq!(
            FileIconTheme::parse("material"),
            Some(FileIconTheme::Material)
        );
        assert_eq!(FileIconTheme::parse("banana"), None);
    }
}

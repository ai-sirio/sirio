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
            order_idx: 0,
            is_active: false,
        }
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

impl TabStateRecord {
    pub fn new(tab_id: impl Into<String>, state: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.into(),
            state: state.into(),
        }
    }
}

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: AppearanceMode::System,
            ui_font_size: 13,
            terminal_font_size: 13,
            file_icon_theme: FileIconTheme::SfSymbols,
            control_socket_enabled: true,
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
}

/// The Swift ranges settings values are clamped into.
pub mod settings_ranges {
    /// `AppSettings.uiFontSizeRange`
    pub const UI_FONT_SIZE: std::ops::RangeInclusive<i64> = 10..=20;
    /// `AppSettings.terminalFontSizeRange`
    pub const TERMINAL_FONT_SIZE: std::ops::RangeInclusive<i64> = 9..=24;
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

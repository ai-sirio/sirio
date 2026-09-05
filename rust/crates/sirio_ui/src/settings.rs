//! Full-window settings surface and its small fixture model.

use crate::caret;
use crate::controls;
use crate::loading;
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use crate::status_bar::{UpdateState, UpdateStatus};
use bezel::theme::Theme as BezelTheme;
use bezel::ui::input::TextField;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyBinding,
    KeyDownEvent, MouseButton, Render, Rgba, ScrollHandle, Window, actions, div, point, prelude::*,
    px, text,
};
use sirio_agents::{AgentAvailability, DiscoveryError, try_discover_availability};
use sirio_project::SkillInstallCommand;
use sirio_theme::{Theme, ThemeMode};
use sirio_usage::{
    AgentAccountIdentity, CredentialStore, CredentialStoreError, LocalAccountState,
    OllamaCloudUsageFetcher, OpenCodeGoUsageFetcher, UsageProvider, codex_auth_file_path,
    parse_codex_identity,
};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::{collections::BTreeSet, path::PathBuf, process::Command};

// The action bound to Escape while the summarizer picker menu is focused.
// Scoped to the menu's key context so the shell's own Escape handling is
// untouched whenever the menu is not the focused thing.
actions!(settings_summarizer, [CloseSummarizerPicker]);

mod agents_page;

/// The settings content column — the frozen 720px content column of
/// `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` (waku
/// `CONTENT_MAX_WIDTH`). It used to be 704, a value nobody recorded; the
/// measured column is 720.
pub(crate) const CONTENT_WIDTH: f32 = 720.0;
const HEADER_HEIGHT: f32 = 40.0;
const CATEGORY_WIDTH: f32 = 200.0;
/// The header's back control: a square target holding the arrow alone. It
/// used to be a 24px-tall strip carrying a chevron and the word "Back";
/// the word is gone and the arrow is the control.
const BACK_CONTROL_SIZE: f32 = 28.0;
const DETAIL_TOP_PADDING: f32 = 15.0;
const DETAIL_BOTTOM_PADDING: f32 = 12.0;
const DETAIL_SECTION_MARGIN: f32 = 20.0;

const SEGMENTED_THEME: &[&str] = &["System", "Light", "Dark"];
/// bezel's five base colours, in its own order. Text rather than swatches:
/// the five differ by hue at chroma 0.013–0.046, which a 16px pill cannot
/// show, and the names are Tailwind's — a vocabulary a user may already have.
const SEGMENTED_BASE_COLOR: &[&str] = &["Neutral", "Stone", "Zinc", "Gray", "Slate"];

/// The segment index of a base colour. `BaseColor::ALL` is the display order,
/// so the index is its position in that array.
fn base_color_segment(base: sirio_theme::BaseColor) -> usize {
    sirio_theme::BaseColor::ALL
        .iter()
        .position(|candidate| *candidate == base)
        .unwrap_or(0)
}

fn settings_section(title: &'static str, card: gpui::Div, theme: Theme) -> impl IntoElement {
    div()
        .id(format!("settings-section-{title}"))
        .w_full()
        .mb(px(DETAIL_SECTION_MARGIN))
        .child(
            div()
                .pl(px(10.0))
                .mb(px(9.0))
                .text_size(theme.typography.headline)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(text!(id = format!("settings-section-title-{title}"), title)),
        )
        .child(card)
}

/// Settings categories in the same order as the Swift surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsCategory {
    AiProviders,
    Agents,
    General,
    Permissions,
    Appearance,
}

impl SettingsCategory {
    /// The categories offered to the user, in sidebar order.
    ///
    /// Permissions contains the platform-specific privacy controls and the
    /// in-app browser's durable origin grants.
    const ALL: [Self; 5] = [
        Self::AiProviders,
        Self::Agents,
        Self::General,
        Self::Permissions,
        Self::Appearance,
    ];

    /// Returns the categories this platform can actually render.
    pub fn all() -> Vec<Self> {
        Self::ALL.to_vec()
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AiProviders => "AI Providers",
            Self::Agents => "Agents",
            Self::General => "General",
            Self::Permissions => "Permissions",
            Self::Appearance => "Appearance",
        }
    }

    /// Parses the labels accepted by the settings socket methods.
    pub fn from_title(value: &str) -> Option<Self> {
        let normalized = value
            .trim()
            .chars()
            .map(|character| match character {
                ' ' | '_' => '-',
                character => character.to_ascii_lowercase(),
            })
            .collect::<String>();
        match normalized.as_str() {
            "ai-providers" | "aiproviders" | "providers" => Some(Self::AiProviders),
            "agents" => Some(Self::Agents),
            "general" => Some(Self::General),
            "permissions" => Some(Self::Permissions),
            "appearance" => Some(Self::Appearance),
            _ => None,
        }
    }

    fn glyph(self) -> Icon {
        match self {
            Self::AiProviders => Icon::Sparkles,
            Self::Agents => Icon::SquareTerminal,
            Self::General => Icon::Settings,
            Self::Permissions => Icon::Shield,
            Self::Appearance => Icon::SunMoon,
        }
    }
}

/// The agent used to summarize sessions into short tab titles
/// (F-SET-05's summarizer picker). The choice is one of the five supported
/// agent CLIs; the persisted value is [`SummarizerChoice::id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummarizerChoice {
    Claude,
    Codex,
    OpenCode,
    Pi,
    OhMyPi,
}

impl SummarizerChoice {
    /// The choices offered by the picker, in display order.
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::OpenCode,
        Self::Pi,
        Self::OhMyPi,
    ];

    /// The stable agent id, used as the persisted value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::OhMyPi => "omp",
        }
    }

    /// The user-visible agent name.
    pub fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Pi => "Pi",
            Self::OhMyPi => "Oh-My-Pi",
        }
    }

    /// Parses a persisted agent id; anything unknown falls back to the
    /// default summarizer (Claude Code), exactly like the other persisted
    /// settings fall back to their defaults on an unparseable value.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == value)
    }
}

/// A project-icon tint palette (F-PRJ-15). This palette remains available to
/// `project_identity`; it is unrelated to the removed per-agent settings
/// override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAccentColor {
    Coral,
    Amber,
    Green,
    Red,
    Blue,
    Purple,
    Gold,
    Slate,
}

impl AgentAccentColor {
    /// The choices offered by the project-icon tint picker, in display order.
    pub const ALL: [Self; 8] = [
        Self::Coral,
        Self::Amber,
        Self::Green,
        Self::Red,
        Self::Blue,
        Self::Purple,
        Self::Gold,
        Self::Slate,
    ];

    /// The stable id, used as the persisted project-icon tint value and as
    /// the picker's per-swatch element id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Coral => "coral",
            Self::Amber => "amber",
            Self::Green => "green",
            Self::Red => "red",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Gold => "gold",
            Self::Slate => "slate",
        }
    }

    /// The `Theme` token this project-icon tint draws with.
    pub fn resolve(self, theme: Theme) -> Rgba {
        match self {
            Self::Coral => theme.brand_coral,
            Self::Amber => theme.warning,
            Self::Green => theme.success,
            Self::Red => theme.danger,
            Self::Blue => theme.accent,
            Self::Purple => theme.border_strong,
            Self::Gold => theme.favorite,
            Self::Slate => theme.border_strong,
        }
    }

    /// Parses a persisted project-icon tint id; anything unknown falls back
    /// to `Coral`.
    pub fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|choice| choice.id() == value)
            .unwrap_or(Self::Coral)
    }
}

/// The persistence-facing values owned by the settings surface.
///
/// This deliberately contains plain UI data rather than a persistence or
/// database type. The host supplies the initial snapshot and receives a new
/// one through [`Settings::on_change`].
///
/// P58: the snapshot is the *whole* persistence contract — every control
/// that holds a value the user can change flows through here, so nothing
/// the surface draws can be report-only. The five original keys keep their
/// Swift-parity names; the values added in P58 are Linux-rewrite keys (the
/// Swift app has no such settings), named after the settings report's own
/// field names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsSnapshot {
    pub theme: ThemeMode,
    pub interface_font_size: i32,
    pub terminal_font_size: i32,
    /// The bezel base colour the greys are tinted with (Appearance → Theme).
    pub base_color: sirio_theme::BaseColor,
    pub control_socket_enabled: bool,
    /// The control socket's resolved path, routed from the host. This is
    /// runtime state (the path the live socket listens on), not a persisted
    /// setting — the host fills it, and the surface only displays it.
    pub socket_path: String,
    /// "Resume agent sessions on launch" (F-SET-04).
    pub resume_agent_sessions: bool,
    /// "Auto-rename tabs and agents" (F-SET-05).
    pub auto_naming: bool,
    /// "Limit stored chats" (F-SET-06).
    pub limit_chat_history: bool,
    /// "Keep chats per worktree" (F-SET-06), 5..=500.
    pub chat_retention: i32,
    /// "Limit mounted worktrees" (F-SET-07).
    pub limit_mounted_worktrees: bool,
    /// "Keep mounted" (F-SET-07), 2..=50.
    pub mounted_worktrees: i32,
    /// The summarizer agent picker (F-SET-05), gated on `auto_naming`.
    pub summarizer_agent: SummarizerChoice,
    /// "Show in usage bar" per provider (F-SET-10), consumed by the status
    /// bar's segment visibility.
    pub claude_show_in_bar: bool,
    pub codex_show_in_bar: bool,
    pub opencode_show_in_bar: bool,
    /// "Show in usage bar" for Ollama Cloud (F-SET-13) — same contract as
    /// the three above; its cookie, like OpenCode Go's, is *not* here.
    pub ollama_show_in_bar: bool,
    /// "Refresh interval" in minutes (F-SET-10), consumed by the status
    /// bar's fetch loop.
    pub refresh_interval: i32,
    /// OpenCode Go workspace-ID override (F-SET-12): a `wrk_…` id pasted
    /// from the opencode.ai URL. Empty means the usage fetch discovers the
    /// workspace itself; the status bar routes a non-empty value into
    /// [`OpenCodeGoUsageFetcher::fetch`]. The session cookie is *not*
    /// here — secrets live in [`CredentialStore`], never the settings
    /// database.
    pub opencode_workspace_id_override: String,
    /// The Appearance screen's Translucency toggle (F-SET-20).
    pub translucency: bool,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            interface_font_size: 13,
            terminal_font_size: 13,
            base_color: sirio_theme::BaseColor::Neutral,
            control_socket_enabled: true,
            socket_path: String::new(),
            resume_agent_sessions: true,
            auto_naming: false,
            limit_chat_history: true,
            chat_retention: 100,
            limit_mounted_worktrees: false,
            mounted_worktrees: 6,
            summarizer_agent: SummarizerChoice::Claude,
            claude_show_in_bar: true,
            codex_show_in_bar: true,
            opencode_show_in_bar: false,
            ollama_show_in_bar: false,
            refresh_interval: 5,
            opencode_workspace_id_override: String::new(),
            translucency: false,
        }
    }
}

/// One provider row's display data, derived entirely from what discovery
/// actually found on this machine. Nothing here is a literal claim about
/// the environment — the crate answers, the surface reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRowModel {
    pub id: &'static str,
    pub name: &'static str,
    /// The agent's own brand mark, resolved through the same
    /// [`Icon::for_agent_id`] the tab bar uses.
    pub icon: Icon,
    pub description: String,
    pub status: ProviderStatus,
    /// The resolved launch source behind this row, when known.
    pub launch: Option<sirio_registry::LaunchSource>,
    /// The version this row would launch or install (Task 9).
    pub version: Option<String>,
}

/// The honest status shown on one AI Provider card: what local credential
/// state exists on this machine, derived — never a mock default. The word
/// "Active" used to sit here and claimed the provider's account worked
/// without anything having checked it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAccountStatus {
    /// The label next to the dot, from the account state's own wording.
    pub label: &'static str,
    /// Whether real credentials exist: green dot when true, neutral
    /// otherwise. "Not signed in" and "Unknown" are information, not
    /// errors.
    pub signed_in: bool,
    /// A provider-supplied identity, when the local account store exposes one.
    /// This is display data only; Sirio never stores or renders credentials.
    pub identity: Option<String>,
}

impl ProviderAccountStatus {
    /// Derives the card's status from the local account state the usage
    /// crate read off disk.
    pub fn from_account_state(state: LocalAccountState) -> Self {
        Self {
            label: state.label(),
            signed_in: state == LocalAccountState::SignedIn,
            identity: None,
        }
    }

    fn with_identity(state: LocalAccountState, identity: Option<String>) -> Self {
        Self {
            label: state.label(),
            signed_in: state == LocalAccountState::SignedIn,
            identity: identity.filter(|identity| !identity.is_empty()),
        }
    }
}

/// The four provider cards' account states, in card order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAccountStates {
    pub claude: ProviderAccountStatus,
    pub codex: ProviderAccountStatus,
    pub opencode_go: ProviderAccountStatus,
    pub ollama_cloud: ProviderAccountStatus,
}

impl ProviderAccountStates {
    /// Reads each provider's local credential state from disk (the usage
    /// crate owns the credential files; the surface only reports).
    pub fn discovered() -> Self {
        Self {
            claude: discover_provider_account(UsageProvider::Claude),
            codex: discover_provider_account(UsageProvider::Codex),
            opencode_go: discover_provider_account(UsageProvider::OpenCodeGo),
            ollama_cloud: discover_provider_account(UsageProvider::OllamaCloud),
        }
    }
}

/// The command Sirio delegates to an installed CLI for account management.
/// Keeping this as data makes the mapping testable without spawning a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderLoginCommand {
    program: &'static str,
    args: Vec<&'static str>,
}

/// `None` for Ollama Cloud: the reference card manages it by pasted cookie
/// only — there is no CLI login flow to delegate to, and its card renders
/// no Accounts section (F-SET-13), so the button that would call this
/// never exists for it.
fn provider_login_command(provider: ProviderKind) -> Option<ProviderLoginCommand> {
    match provider {
        ProviderKind::Claude => Some(ProviderLoginCommand {
            program: "claude",
            args: vec!["auth", "login"],
        }),
        ProviderKind::Codex => Some(ProviderLoginCommand {
            program: "codex",
            args: vec!["login"],
        }),
        ProviderKind::OpenCodeGo => Some(ProviderLoginCommand {
            program: "opencode",
            args: vec!["auth", "login"],
        }),
        ProviderKind::OllamaCloud => None,
    }
}

fn format_account_identity(identity: &AgentAccountIdentity) -> String {
    match identity
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|organization| !organization.is_empty())
    {
        Some(organization) => format!("{} · {organization}", identity.email),
        None => identity.email.clone(),
    }
}

fn discover_provider_account(provider: UsageProvider) -> ProviderAccountStatus {
    let state = provider.local_account_state();
    let identity = match (provider, state) {
        (UsageProvider::Claude, LocalAccountState::SignedIn) => discover_claude_identity(),
        (UsageProvider::Codex, LocalAccountState::SignedIn) => discover_codex_identity(),
        _ => None,
    };
    ProviderAccountStatus::with_identity(state, identity)
}

fn discover_claude_identity() -> Option<String> {
    let output = Command::new("claude")
        .args(["auth", "status"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let identity =
        AgentAccountIdentity::parse_claude_json(&String::from_utf8_lossy(&output.stdout))?;
    identity
        .logged_in
        .then(|| format_account_identity(&identity))
}

fn discover_codex_identity() -> Option<String> {
    if !codex_auth_file_path().is_file() {
        return None;
    }
    let output = Command::new("codex")
        .args(["login", "status"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_codex_identity(&String::from_utf8_lossy(&output.stdout))
}

/// The complete state the Settings surface currently holds. The socket
/// reads this from the mounted entity, so provider badges and selected
/// sections cannot drift from what the UI renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsReport {
    pub category: SettingsCategory,
    pub snapshot: SettingsSnapshot,
    pub provider_availability: Vec<AgentAvailability>,
    pub resume_agent_sessions: bool,
    pub auto_naming: bool,
    pub limit_chat_history: bool,
    pub chat_retention: i32,
    pub limit_mounted_worktrees: bool,
    pub mounted_worktrees: i32,
    pub claude_show_in_bar: bool,
    pub codex_show_in_bar: bool,
    pub opencode_show_in_bar: bool,
    pub ollama_show_in_bar: bool,
    pub refresh_interval: i32,
}

/// The truthful status of one provider's CLI on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    /// The CLI is installed; `path` is the resolved executable.
    Installed(PathBuf),
    /// The CLI was not found on `PATH`.
    NotInstalled,
}

/// The brand mark for a discovered agent id.
///
/// This screen used to draw Unicode stand-ins here. A code point is not an
/// asset: the UI face does not carry those marks on every platform, so the
/// column rendered blank or tofu, and where it did resolve it still was not
/// the agent's logo. `Icon::for_agent_id` is the answer the tab bar already
/// uses, so settings uses it too.
fn provider_icon(id: &str) -> Icon {
    Icon::for_agent_id(id).unwrap_or(Icon::Sparkles)
}

/// The colour a discovered agent's brand mark wears, from its published
/// identity — Claude's orange; the monochrome Codex/OpenCode/Pi marks in
/// the foreground they are authored in. Only an id outside the catalog
/// (the Sparkles stand-in) keeps a neutral meta tint: it is nobody's
/// brand, so it must not borrow one.
fn provider_glyph_color(theme: Theme, id: &str) -> Rgba {
    match Icon::for_agent_id(id) {
        Some(icon) => icon
            .agent_mark_color(theme.text)
            .unwrap_or_else(|| theme.text_faint),
        None => theme.text_faint,
    }
}

/// Derives the row a settings entry shows from one discovery result.
///
/// The description and status follow the availability data, never a fixed
/// claim: an installed CLI reports the resolved executable, an absent one
/// says so in a way the user can act on.
pub fn provider_row(
    availability: &AgentAvailability,
    source: Option<&sirio_registry::LaunchSource>,
) -> ProviderRowModel {
    let name = availability.display_name;
    let description = if availability.is_available() {
        format!(
            "Built-in: uses the {} binary on your PATH.",
            availability.id
        )
    } else {
        format!("Not installed — install the {name} CLI to use it.")
    };
    let status = match &availability.executable {
        Some(path) => ProviderStatus::Installed(path.clone()),
        None => ProviderStatus::NotInstalled,
    };
    let launch = source.cloned();
    let version = match source {
        Some(sirio_registry::LaunchSource::Installed(installed)) => Some(installed.version.clone()),
        Some(sirio_registry::LaunchSource::Installable { agent }) => Some(agent.version.clone()),
        _ => None,
    };
    ProviderRowModel {
        id: availability.id,
        name,
        icon: provider_icon(availability.id),
        description,
        status,
        launch,
        version,
    }
}

/// What an installed agent could not prove about itself, or `None` when it
/// could.
pub(crate) fn installed_integrity_note(
    source: &sirio_registry::LaunchSource,
) -> Option<&'static str> {
    match source {
        sirio_registry::LaunchSource::Installed(agent)
            if agent.integrity == sirio_registry::Integrity::None =>
        {
            Some("no published checksum")
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Claude,
    Codex,
    OpenCodeGo,
    OllamaCloud,
}

impl ProviderKind {
    /// The brand mark this card shows.
    ///
    /// Ollama has no mark in the vendored catalog, so the globe stands in —
    /// the same admission the cloud code point used to make, except a globe
    /// is an asset that draws on every platform.
    fn icon(self) -> Icon {
        match self {
            Self::Claude => Icon::ClaudeCode,
            Self::Codex => Icon::Codex,
            Self::OpenCodeGo => Icon::OpenCode,
            Self::OllamaCloud => Icon::Globe,
        }
    }
}

/// The foreground for text painted on one of the saturated status fills
/// (`tab_error`, `tab_needs_input`).
///
/// `title` is tuned for the *page*, so on a saturated chip it lands at the
/// chip's own lightness and vanishes. The page ground is the neutral that
/// actually contrasts with both fills, and it is already what the granted
/// permission badge uses two screens over.
///
/// macOS-only in production (the permission rows it serves are macOS
/// gates); the contrast test below uses it on every platform.
#[cfg(any(test, target_os = "macos"))]
fn on_status_fill(theme: &Theme) -> Rgba {
    theme.surface
}

/// Every process id currently a descendant of `root` (not including `root`
/// itself), found by walking the kernel's live parent/child view via
/// `/proc/<pid>/task/<tid>/children`. Walked fresh at kill time rather than
/// captured once at spawn: the login command a terminal emulator runs
/// attaches to a brand new PTY session as soon as it starts (confirmed live
/// — `cosmic-term -e sleep N` puts the child in a *different* process group
/// from the launcher's own, `getpgid(child) != getpgid(launcher)`), so a
/// single pgid recorded at spawn time never covers it; only a walk done now
/// does.
#[cfg(target_os = "linux")]
fn descendant_pids(root: u32) -> Vec<u32> {
    let mut discovered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(root);
    let mut frontier = vec![root];
    while let Some(pid) = frontier.pop() {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(contents) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for token in contents.split_whitespace() {
                let Ok(child) = token.parse::<u32>() else {
                    continue;
                };
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
    }
    discovered
}

/// macOS implementation of [`descendant_pids`], using libproc's live child
/// table because macOS has no `/proc` filesystem.
#[cfg(all(unix, not(target_os = "linux")))]
fn descendant_pids(root: u32) -> Vec<u32> {
    #[cfg(target_os = "macos")]
    {
        let mut discovered = Vec::new();
        let mut seen = std::collections::HashSet::from([root]);
        let mut frontier = vec![root];
        while let Some(pid) = frontier.pop() {
            let Ok(children) = macos_child_pids(pid) else {
                continue;
            };
            for child in children {
                if seen.insert(child) {
                    discovered.push(child);
                    frontier.push(child);
                }
            }
        }
        discovered
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn macos_child_pids(parent: u32) -> std::io::Result<Vec<u32>> {
    use std::os::raw::{c_int, c_void};

    const INITIAL_PID_CAPACITY: usize = 64;
    const MAX_PID_CAPACITY: usize = 16_384;

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listchildpids(ppid: c_int, buffer: *mut c_void, buffersize: c_int) -> c_int;
    }

    let parent = c_int::try_from(parent).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "process id is too large")
    })?;
    let mut buffer = vec![0_i32; INITIAL_PID_CAPACITY];
    loop {
        let buffer_size = (buffer.len() * std::mem::size_of::<i32>()) as c_int;
        // libproc returns the number of PIDs copied, not a byte count.
        let reported_count =
            unsafe { proc_listchildpids(parent, buffer.as_mut_ptr().cast(), buffer_size) };
        if reported_count < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let count = (reported_count as usize).min(buffer.len());
        if (reported_count as usize) < buffer.len() {
            return Ok(buffer[..count]
                .iter()
                .copied()
                .filter(|pid| *pid > 0)
                .map(|pid| pid as u32)
                .collect());
        }

        if buffer.len() >= MAX_PID_CAPACITY {
            return Err(std::io::Error::new(
                std::io::ErrorKind::OutOfMemory,
                "macOS child-process list exceeded safety limit",
            ));
        }
        buffer.resize((buffer.len() * 2).min(MAX_PID_CAPACITY), 0);
    }
}

/// Kills the login terminal and every process it has spawned since launch
/// (F-SET-14). `kill <pid>` on the launcher alone only ever reaches the
/// launcher itself — the interactive login command it runs lands in its own
/// PTY session, detached from the launcher's process group, which is
/// exactly the failure this row's evidence recorded (the recorded pid, and
/// even the terminal's own pid killed manually, left the login command
/// alive). Walking `/proc` for every current descendant and signaling each
/// one directly — SIGTERM first, SIGKILL after a short grace period for
/// anything that ignored it — reaches the login command wherever it landed,
/// without depending on process-group membership at all.
#[cfg(unix)]
fn terminate_login_process_group(pid: u32) {
    let mut targets = vec![pid];
    targets.extend(descendant_pids(pid));
    for target in &targets {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(target.to_string())
            .status();
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    for target in &targets {
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(target.to_string())
            .status();
    }
}

/// Windows twin of [`terminate_login_process_group`]. There is no
/// descendant walk to do here: [`login_launcher`] runs the login command
/// itself in a fresh console, so the recorded pid *is* the login command
/// (no terminal-emulator launcher sits in front of it), and one
/// `TerminateProcess` ends it and closes its console window with it. A
/// pid that no longer exists (the login already finished) opens no handle
/// and is left alone.
#[cfg(windows)]
fn terminate_login_process_group(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: plain Win32 calls on a handle this function opens, checks
    // for null, and closes itself; nothing is dereferenced.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return;
        }
        TerminateProcess(handle, 1);
        CloseHandle(handle);
    }
}

/// The process that opens a provider's interactive login for
/// [`Settings::launch_account_login`]. Unix delegates to the desktop's
/// terminal emulator (`x-terminal-emulator -e <program> <args>`), so the
/// child is the emulator and the login command runs inside it.
#[cfg(not(windows))]
fn login_launcher(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new("x-terminal-emulator");
    command.arg("-e").arg(program).args(args);
    command
}

/// Windows twin of [`login_launcher`]: there is no terminal-emulator
/// alternatives name to delegate to, and a console program started from a
/// GUI process gets no window of its own unless asked — so the login
/// command runs directly, in a fresh console (`CREATE_NEW_CONSOLE`). The
/// child *is* the login command: `wait` ends when the login ends, and the
/// pid handed to Cancel is the one [`terminate_login_process_group`] ends.
#[cfg(windows)]
fn login_launcher(program: &str, args: &[&str]) -> Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let mut command = Command::new(program);
    command.args(args).creation_flags(CREATE_NEW_CONSOLE);
    command
}

/// The card's message when the login could not even be started. Names the
/// launcher only when it is a different program from the login itself —
/// on Linux a missing `x-terminal-emulator` is what fails, and blaming
/// `claude` for it sent a reader to check the wrong install.
fn login_start_failure(program: &str, launcher: &str, error: &std::io::Error) -> String {
    if launcher == program {
        format!("could not start {program} login: {error}")
    } else {
        format!("could not start {program} login via {launcher}: {error}")
    }
}

/// Small settings view model. The real application can replace these values
/// with its persistence layer without changing the reusable settings UI.
/// The badge shown at the trailing edge of a permission row.
#[cfg(target_os = "macos")]
struct PermissionBadge {
    label: &'static str,
    background: Rgba,
    foreground: Rgba,
}

/// What an Agents-row button asks the host to do. Emitted only — this
/// crate never runs installs: `sirio` owns the installer (Task 8), and
/// `sirio_ui` has no filesystem or network access in this design.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    InstallAgent(String),
    UpdateAgent(String),
    /// The Agents screen opened or its Refresh was pressed: the host
    /// re-fetches the registry document (24 h cache respected) and
    /// recomputes every launch source.
    RefreshAgentSources,
}

impl EventEmitter<SettingsEvent> for Settings {}

/// Where one row's install stands right now. Owned by the host (`sirio`
/// runs the installer), rendered here; success clears the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallState {
    InFlight,
    /// The installer's own error text — it names the format, the package
    /// or the remedy.
    Failed(String),
}

pub struct Settings {
    category: SettingsCategory,
    detail_scroll: ScrollHandle,
    on_back: Option<Rc<dyn Fn()>>,
    on_change: Option<Rc<dyn Fn(SettingsSnapshot)>>,
    theme_mode: ThemeMode,
    /// The Appearance screen's Translucency toggle (F-SET-20). Part of the
    /// persistence contract — see [`SettingsSnapshot::translucency`].
    translucency: bool,
    interface_font_size: i32,
    terminal_font_size: i32,
    base_color: sirio_theme::BaseColor,
    claude_show_in_bar: bool,
    codex_show_in_bar: bool,
    opencode_show_in_bar: bool,
    ollama_show_in_bar: bool,
    refresh_interval: i32,
    resume_agent_sessions: bool,
    auto_naming: bool,
    limit_chat_history: bool,
    chat_retention: i32,
    limit_mounted_worktrees: bool,
    mounted_worktrees: i32,
    /// The agent used to summarize sessions into tab titles, gated on
    /// `auto_naming` (F-SET-05).
    summarizer_agent: SummarizerChoice,
    control_socket_enabled: bool,
    socket_path: String,
    /// The host-supplied app version shown in General settings. The UI crate
    /// deliberately does not depend on `sirio_control` for this fact.
    version: String,
    /// The host-supplied release channel shown next to the version (F-305):
    /// same independence rule as `version`.
    channel: String,
    /// Host-owned update facts rendered in General settings.
    update_state: UpdateState,
    /// Host action used by the confirming update control. The updater itself
    /// remains outside this crate.
    on_apply_update: Option<Rc<dyn Fn()>>,
    /// Host persistence hook for the per-install update opt-out.
    on_update_enabled_change: Option<Rc<dyn Fn(bool)>>,
    /// Whether the summarizer picker's agent menu is open (F-SET-05).
    summarizer_popover_open: bool,
    /// Focus handle for the picker menu, so Escape closes the menu alone:
    /// the menu's own scoped Escape binding beats the shell's global one
    /// while the menu holds focus (GPUI resolves the more specific context
    /// first), so the keystroke never reaches the workspace handler.
    summarizer_focus: gpui::FocusHandle,
    /// Focus handle for the settings surface. The surface must hold focus
    /// while it is open: GPUI dispatches keys and actions along the focus
    /// path, and the shell's Escape handler lives on that path (the
    /// workspace root). With nothing in the surface focused, the fallback
    /// dispatch path is the window's synthetic root, which carries no
    /// handlers — Escape would be dead (F-SET-02). The host requests this
    /// focus through [`Settings::request_surface_focus`] when the surface
    /// opens; the popover requests it again when it closes.
    surface_focus: gpui::FocusHandle,
    /// Set whenever something must take focus back (the surface opening,
    /// the picker menu closing); the next render schedules the focus on
    /// the following frame (focus cannot move during render).
    surface_focus_pending: bool,
    /// Discovery results for every supported agent CLI, from the crate that
    /// owns the catalog. The Agents screen renders only this data.
    provider_availability: Vec<AgentAvailability>,
    /// Renderable failure of the last availability sweep (F-SET-17). `None`
    /// after a successful sweep. While set, the Agents screen shows the
    /// message above the last successful rows — never five false "Not found
    /// on PATH" claims — and the screen's "↻ Refresh" button is the retry.
    agent_registry_error: Option<String>,
    /// When the Agents screen's rows were last populated by a discovery
    /// sweep (F-SET-16) — construction counts as one, and every "↻ Refresh"
    /// click stamps a fresh one, so the render has a real, changing value
    /// to show rather than nothing at all.
    agent_last_refreshed: Option<String>,
    /// Account state for the three AI Provider cards, derived from local
    /// credential files at construction — never a mock default.
    provider_accounts: ProviderAccountStates,
    /// Host callback for the Agent Skill card's Install button (F-SET-09).
    /// The provisioner (`sirio_project::agent_skill_install_command`) is
    /// tested and reachable from here; running the resulting command (in a
    /// terminal or in the background) is the host's call, not this crate's
    /// — `chat.rs`/`sirio_terminal` own process spawning, and neither is
    /// mine to touch. Unset, the button renders muted and does not respond
    /// to clicks, the same dead-control avoidance P76's `on_back` seam
    /// uses: a click that reaches nothing is exactly the defect this brief
    /// exists to close, so an unwired button must not look wired.
    on_install_skill: Option<Rc<dyn Fn(SkillInstallCommand)>>,
    /// Set the instant Install Skill is clicked and reaches a wired
    /// `on_install_skill` (F-SET-09). The button hands the actual install
    /// off to the host (a spawned terminal), which this crate cannot watch
    /// finish — but the click itself must leave a visible trace on this
    /// screen rather than looking identical to a no-op, so this renders a
    /// confirmation line under the button naming where the install is
    /// running. Cleared the next time the button is clicked again.
    skill_install_launched: bool,
    /// The resolved launch source per adapter id (Task 9). The Agents
    /// screen's pill, version and Install/Update buttons render from this —
    /// a resolved fact — instead of the compiled claim they replaced.
    launch_sources: Vec<(String, sirio_registry::LaunchSource)>,
    /// The registry's current version per adapter-mapped agent id, when a
    /// registry document has been fetched. Drives the Update button: an
    /// installed version behind it offers an update; equal or absent means
    /// nothing to offer.
    registry_versions: BTreeMap<String, String>,
    /// Live install state per adapter id (Task 9 fix round). `InFlight`
    /// hides the row's action; `Failed` shows the installer's own message
    /// and offers the action again; success removes the entry.
    install_states: BTreeMap<String, InstallState>,
    /// Optional host override for a provider card's Add Account button
    /// (F-SET-14). The payload is the provider's stable id (`"claude"`,
    /// `"codex"`, `"opencode"` — [`UsageProvider::id`]'s own convention),
    /// not a credential or command. When unset, Settings invokes the
    /// installed provider CLI in an external terminal. Unlike the macOS
    /// original, this app never holds isolated per-provider credentials of
    /// its own — [`ProviderAccountStates`] only reads the one credential file
    /// the provider's CLI manages on this machine (see the "System default"
    /// row's comment in [`Settings::render_provider_card`]).
    on_manage_account: Option<Rc<dyn Fn(&'static str)>>,
    /// Last failure while handing account management to an external terminal.
    /// A failed spawn must be visible rather than implying that login started.
    account_action_error: Option<(ProviderKind, String)>,
    /// The provider whose [`Self::launch_account_login`] is currently
    /// spawned and being waited on (F-SET-14). While set, the card renders
    /// "Signing in…" and a Cancel button in place of Add Account, so a
    /// click that already reached a real subprocess is not left with no
    /// visible sign anything is happening.
    account_login_pending: Option<ProviderKind>,
    /// The spawned login terminal's pid, once known — set slightly after
    /// [`Self::account_login_pending`] (the process must exist before it
    /// has one) and what [`Self::cancel_account_login`] signals.
    account_login_pid: Option<u32>,
    /// Set by [`Self::cancel_account_login`] so the login task's own
    /// completion handler — which still runs after the killed process's
    /// `wait()` resolves — does not overwrite the "Sign-in canceled"
    /// message with an exit-status one.
    account_login_canceled: bool,
    /// The OpenCode Go session cookie being typed (F-SET-12). Transient UI
    /// state: Save moves it into [`CredentialStore`], and the field only
    /// ever renders mask dots — the value is never drawn back or persisted
    /// through the settings contract.
    opencode_cookie_input: String,
    /// Focus handle for the cookie field — the agents search field's
    /// click-to-focus + raw-keystroke pattern.
    opencode_cookie_focus: FocusHandle,
    /// Last failure writing the credential store. A Save that failed must
    /// say so: the one outcome this control may not have is silently
    /// dropping the pasted cookie (the macOS original surfaces "Failed to
    /// update Keychain…" in the same spot).
    opencode_cookie_error: Option<String>,
    /// The Ollama Cloud session cookie being typed (F-SET-13) — same
    /// transient-only rule as [`Self::opencode_cookie_input`].
    ollama_cookie_input: String,
    /// Focus handle for the Ollama cookie field.
    ollama_cookie_focus: FocusHandle,
    /// Last failure writing the Ollama cookie — same forbidden-outcome
    /// rule as [`Self::opencode_cookie_error`].
    ollama_cookie_error: Option<String>,
    /// The workspace-ID override (F-SET-12) — part of the persistence
    /// contract, see [`SettingsSnapshot::opencode_workspace_id_override`].
    opencode_workspace_id_override: String,
    /// Focus handle for the override field.
    opencode_override_focus: FocusHandle,
    /// Where the cookie is saved: the app's own on-disk store (macOS uses
    /// the Keychain instead). `None` only when no home directory exists to
    /// place the store — Save then reports the failure instead of
    /// pretending it worked.
    credential_store: Option<CredentialStore>,
    /// Durable grants remembered by the in-app browser. The host seeds this
    /// from its session store and receives revoke callbacks below.
    browser_origins: BTreeSet<String>,
    on_revoke_browser_origin: Option<Rc<dyn Fn(String)>>,
    on_revoke_all_browser_origins: Option<Rc<dyn Fn()>>,
    /// The Agents screen's search field (F-SET-16): a bezel `TextField`
    /// whose content is the filter query. Transient UI state, not part of
    /// the persistence contract — nothing durable depends on what was last
    /// typed into a filter box.
    agent_search_field: Entity<TextField>,
    /// Shared blink state for every settings text field's insertion caret
    /// (search, both cookies, workspace override). One is enough: window
    /// focus is unique, so at most one field can show a caret. Computed
    /// visibility is stashed here each render for the field builders.
    field_blink: caret::Blink,
    field_caret_visible: bool,
    /// Where the account-identity cache lives (F-PERSIST-DB-06) — the same
    /// database file `chat.rs`'s transcript persistence opens on demand.
    /// `None` in every UI-only/test construction; the host wires this once
    /// via [`Self::with_database_path`], right after [`Self::with_snapshot`].
    database_path: Option<PathBuf>,
    /// F-SET-15: isolated accounts stored for Claude/Codex — the only two
    /// providers the reference app's own `AgentAccountStore` ever gave
    /// multi-account support (OpenCode Go and Ollama Cloud authenticate
    /// with a single pasted cookie, not a CLI login, so there is nothing
    /// for a second isolated account to mean there). Loaded from the
    /// database alongside the identity cache; empty in every UI-only/test
    /// construction that never called [`Self::with_database_path`].
    claude_accounts: Vec<sirio_persistence::AgentAccountRecord>,
    codex_accounts: Vec<sirio_persistence::AgentAccountRecord>,
    /// The selected account id for each provider, `None` meaning "System
    /// default" (the CLI's own unmodified on-disk login). Persisted under
    /// the same key the Swift app's `AgentAccountStore` used.
    active_claude_account_id: Option<String>,
    active_codex_account_id: Option<String>,
}

/// The display data for one AI Provider card — everything the renderer
/// needs except the entity and theme. Bundled so the render function stays
/// within clippy's argument budget and each card reads as one view model.
#[derive(Clone)]
struct ProviderCardView {
    kind: ProviderKind,
    title: &'static str,
    glyph_color: Rgba,
    status: ProviderAccountStatus,
}

impl ProviderCardView {
    fn new(
        kind: ProviderKind,
        title: &'static str,
        glyph_color: Rgba,
        status: ProviderAccountStatus,
    ) -> Self {
        Self {
            kind,
            title,
            glyph_color,
            status,
        }
    }
}

impl Settings {
    /// Creates the settings model with the persistence contract's defaults.
    ///
    /// The demo constructor remains available for UI-only consumers; the
    /// application host should use [`Settings::with_snapshot`].
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_snapshot(cx, SettingsSnapshot::default())
    }

    /// Creates the settings model from the host's durable snapshot.
    pub fn with_snapshot(cx: &mut Context<Self>, initial: SettingsSnapshot) -> Self {
        Self::bind_keys(cx);
        // A sweep that cannot answer (F-SET-17) starts the surface with the
        // error banner and no rows, rather than claiming every agent is
        // absent.
        let (provider_availability, agent_registry_error) = match try_discover_availability() {
            Ok(rows) => (rows, None),
            Err(error) => (Vec::new(), Some(Self::registry_error_message(&error))),
        };
        // F-SET-16: construction just ran the sweep above, successful or
        // not — it stamped the rows (or the error) the Agents screen opens
        // showing, so it is as much a "last refreshed" as a button click.
        let agent_last_refreshed = Some(Self::format_refreshed_stamp());
        Self {
            category: SettingsCategory::Appearance,
            detail_scroll: ScrollHandle::new(),
            on_back: None,
            on_change: None,
            theme_mode: initial.theme,
            translucency: initial.translucency,
            interface_font_size: initial.interface_font_size.clamp(10, 20),
            terminal_font_size: initial.terminal_font_size.clamp(9, 24),
            base_color: initial.base_color,
            // A persisted choice from another platform (the database default
            // is the Swift-parity sfSymbols) is clamped to the first set
            // that exists here, so the surface never shows a choice it
            // cannot render.
            control_socket_enabled: initial.control_socket_enabled,
            // The Agents screen reports what discovery finds on this
            // machine — never a fixed list of "Available" claims.
            provider_availability,
            agent_registry_error,
            agent_last_refreshed,
            // The provider cards report what local credential state exists
            // on this machine — never a fixed list of "Active" claims.
            provider_accounts: ProviderAccountStates::discovered(),
            socket_path: initial.socket_path,
            version: String::new(),
            channel: String::new(),
            update_state: UpdateState::default(),
            on_apply_update: None,
            on_update_enabled_change: None,
            summarizer_popover_open: false,
            summarizer_focus: cx.focus_handle(),
            surface_focus: cx.focus_handle(),
            surface_focus_pending: false,
            resume_agent_sessions: initial.resume_agent_sessions,
            auto_naming: initial.auto_naming,
            limit_chat_history: initial.limit_chat_history,
            chat_retention: initial.chat_retention.clamp(5, 500),
            limit_mounted_worktrees: initial.limit_mounted_worktrees,
            mounted_worktrees: initial.mounted_worktrees.clamp(2, 50),
            summarizer_agent: initial.summarizer_agent,
            claude_show_in_bar: initial.claude_show_in_bar,
            codex_show_in_bar: initial.codex_show_in_bar,
            opencode_show_in_bar: initial.opencode_show_in_bar,
            ollama_show_in_bar: initial.ollama_show_in_bar,
            refresh_interval: initial.refresh_interval.clamp(1, 60),
            on_install_skill: None,
            skill_install_launched: false,
            launch_sources: Vec::new(),
            registry_versions: BTreeMap::new(),
            install_states: BTreeMap::new(),
            on_manage_account: None,
            account_action_error: None,
            account_login_pending: None,
            account_login_pid: None,
            account_login_canceled: false,
            opencode_cookie_input: String::new(),
            opencode_cookie_focus: cx.focus_handle(),
            opencode_cookie_error: None,
            ollama_cookie_input: String::new(),
            ollama_cookie_focus: cx.focus_handle(),
            ollama_cookie_error: None,
            opencode_workspace_id_override: initial.opencode_workspace_id_override,
            opencode_override_focus: cx.focus_handle(),
            credential_store: CredentialStore::from_env().ok(),
            browser_origins: BTreeSet::new(),
            on_revoke_browser_origin: None,
            on_revoke_all_browser_origins: None,
            agent_search_field: {
                let field = cx.new(|cx| {
                    TextField::new(cx)
                        .with_placeholder("Search agents")
                        .with_key_context("SettingsAgentSearch")
                });
                cx.observe(&field, |_, _, cx| cx.notify()).detach();
                field
            },
            field_blink: caret::Blink::new(),
            field_caret_visible: false,
            database_path: None,
            claude_accounts: Vec::new(),
            codex_accounts: Vec::new(),
            active_claude_account_id: None,
            active_codex_account_id: None,
        }
    }

    /// Supplies the host's compiled app version for the General settings row.
    /// The host owns the value so this UI crate stays independent of
    /// `sirio_control`.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Supplies the host's compiled release channel for the General settings
    /// row. Same host-owns-the-value rule as [`Self::with_version`].
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = channel.into();
        self
    }

    /// Supplies the host-owned update facts for General settings.
    pub fn with_update_state(mut self, state: UpdateState) -> Self {
        self.update_state = state;
        self
    }

    /// Installs the host callback for the platform-specific update action.
    pub fn on_apply_update(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_apply_update = Some(Rc::new(callback));
        self
    }

    /// Installs the host callback that persists the per-install opt-out.
    pub fn on_update_enabled_change(mut self, callback: impl Fn(bool) + 'static) -> Self {
        self.on_update_enabled_change = Some(Rc::new(callback));
        self
    }

    /// Replaces the host-owned update facts without starting updater work.
    pub fn apply_update_state(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.update_state = state;
        cx.notify();
    }

    /// The current host-owned update facts, for the host and tests.
    pub fn update_state(&self) -> &UpdateState {
        &self.update_state
    }

    /// Wires the durable account-identity cache (F-PERSIST-DB-06). Call
    /// once, right after construction: a provider whose live discovery
    /// already failed by then (offline, or the CLI binary transiently
    /// unavailable) falls back to the last cached identity immediately, and
    /// every later discovery sweep (Refresh, or a completed login) has a
    /// database to persist a fresh success into.
    pub fn with_database_path(mut self, path: PathBuf) -> Self {
        self.database_path = Some(path);
        self.sync_account_identity_cache();
        self.sync_agent_accounts();
        self
    }

    /// F-SET-15: (re)loads the isolated Claude/Codex account lists and each
    /// provider's current selection from the database. A no-op until
    /// [`Self::with_database_path`] has been called, same as the identity
    /// cache above.
    fn sync_agent_accounts(&mut self) {
        let Some(path) = self.database_path.clone() else {
            return;
        };
        let Ok(db) = sirio_persistence::AppDatabase::open(&path) else {
            return;
        };
        self.claude_accounts = db.agent_accounts("claude").unwrap_or_default();
        self.codex_accounts = db.agent_accounts("codex").unwrap_or_default();
        self.active_claude_account_id = db.active_agent_account_id("claude").unwrap_or(None);
        self.active_codex_account_id = db.active_agent_account_id("codex").unwrap_or(None);
    }

    /// Selects the active account for `provider_id` (`"claude"` |
    /// `"codex"`) — `None` returns to "System default". Persists to the
    /// database (a no-op provider, or a database not yet wired, still
    /// updates the in-memory badge so tests and previews work without a
    /// database) and moves the "Active" badge on the next render.
    pub fn select_agent_account(
        &mut self,
        provider_id: &str,
        account_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if provider_id != "claude" && provider_id != "codex" {
            return;
        }
        if let Some(path) = self.database_path.clone()
            && let Ok(db) = sirio_persistence::AppDatabase::open(&path)
        {
            let _ = db.set_active_agent_account_id(provider_id, account_id.as_deref());
        }
        match provider_id {
            "claude" => self.active_claude_account_id = account_id,
            "codex" => self.active_codex_account_id = account_id,
            _ => unreachable!("checked above"),
        }
        cx.notify();
    }

    /// Registers a new isolated account for `provider_id` (`"claude"` |
    /// `"codex"`) and makes it the active selection. `config_dir_path` is
    /// the isolated directory this account's CLI invocations should be
    /// pointed at (`CLAUDE_CONFIG_DIR`/`CODEX_HOME`-style); `None` creates a
    /// fresh, empty one (mirrors the Swift store's `newAccountConfigDir`),
    /// for a caller that has not set one up. This does not move any
    /// credentials into the directory or complete a login — that remains
    /// the CLI's own job, the same as it always was for "System default".
    /// Returns the new account's id.
    pub fn add_agent_account(
        &mut self,
        provider_id: &str,
        label: String,
        config_dir_path: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if provider_id != "claude" && provider_id != "codex" {
            return None;
        }
        let path = self.database_path.clone()?;
        let db = sirio_persistence::AppDatabase::open(&path).ok()?;
        let id = format!(
            "acct-{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
            self.claude_accounts.len() + self.codex_accounts.len()
        );
        // Mirrors the Swift store's `newAccountConfigDir`: a caller may
        // register an already-existing directory it set up out of band
        // (e.g. a pre-authenticated `CODEX_HOME` copied in by a script);
        // otherwise a fresh, empty isolated directory is created here,
        // alongside the app's own database rather than in a fixed OS
        // location, since this rewrite has no `Application Support`
        // equivalent path threaded through yet.
        let config_dir_path = match config_dir_path {
            Some(explicit) => explicit,
            None => {
                let dir = path
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("agent-accounts")
                    .join(provider_id)
                    .join(&id);
                std::fs::create_dir_all(&dir).ok()?;
                dir.to_string_lossy().into_owned()
            }
        };
        let record = sirio_persistence::AgentAccountRecord {
            id: id.clone(),
            provider: provider_id.to_string(),
            label,
            config_dir_path,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or_default(),
        };
        db.save_agent_account(&record).ok()?;
        let _ = db.set_active_agent_account_id(provider_id, Some(&id));
        match provider_id {
            "claude" => {
                self.claude_accounts.push(record);
                self.active_claude_account_id = Some(id.clone());
            }
            "codex" => {
                self.codex_accounts.push(record);
                self.active_codex_account_id = Some(id.clone());
            }
            _ => unreachable!("checked above"),
        }
        cx.notify();
        Some(id)
    }

    /// Reconciles the just-discovered Claude/Codex account states against
    /// the durable cache: a fresh identity is saved for next time, and a
    /// provider that is locally signed in but whose live query for a
    /// display identity failed this time (rather than never having signed
    /// in) falls back to the last successfully cached one instead of
    /// showing no identity at all. A no-op until [`Self::with_database_path`]
    /// has been called.
    fn sync_account_identity_cache(&mut self) {
        let Some(path) = self.database_path.clone() else {
            return;
        };
        let Ok(db) = sirio_persistence::AppDatabase::open(&path) else {
            return;
        };
        Self::sync_one_account_identity("claude", &db, &mut self.provider_accounts.claude);
        Self::sync_one_account_identity("codex", &db, &mut self.provider_accounts.codex);
    }

    fn sync_one_account_identity(
        provider_id: &str,
        db: &sirio_persistence::AppDatabase,
        status: &mut ProviderAccountStatus,
    ) {
        if let Some(identity) = &status.identity {
            let _ = db.save_account_identity(provider_id, identity);
        } else if status.signed_in
            && let Ok(Some((cached, _detected_at))) = db.account_identity(provider_id)
        {
            status.identity = Some(cached);
        }
    }

    /// Installs the Escape binding that closes the summarizer picker menu.
    /// The binding is scoped to the menu's key context, so it consumes the
    /// keystroke only while the menu is focused — the shell's own Escape
    /// handling (P58, F-SET-02) keeps closing the settings surface when the
    /// menu is not the focused thing.
    fn bind_keys(cx: &mut App) {
        cx.bind_keys([KeyBinding::new(
            "escape",
            CloseSummarizerPicker,
            Some("SettingsSummarizerPopover"),
        )]);
    }

    /// Pins the provider cards' account states to explicit values.
    ///
    /// Production code always reads real credential state; this seam
    /// exists for tests and previews that must exercise the surface
    /// without touching the machine's actual auth files.
    pub fn with_account_states(mut self, states: ProviderAccountStates) -> Self {
        self.provider_accounts = states;
        self
    }

    /// Pins the provider list to explicit discovery results.
    ///
    /// Production code always gets live discovery; this seam exists for
    /// tests and previews that must exercise the surface against a
    /// controlled environment without touching the real `PATH`.
    pub fn with_availability(mut self, availability: Vec<AgentAvailability>) -> Self {
        self.provider_availability = availability;
        self
    }

    /// Pins the credential store to an explicit location (F-SET-12).
    ///
    /// Production code resolves the store from the environment; this seam
    /// exists for tests that must exercise Save/Clear against a fixture
    /// file without touching the user's real store.
    pub fn with_credential_store(mut self, store: CredentialStore) -> Self {
        self.credential_store = Some(store);
        self
    }

    /// Installs the host callback used to return from the full-window
    /// settings surface to the host's main surface.
    pub fn on_back(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_back = Some(Rc::new(callback));
        self
    }

    /// Installs the host callback invoked after a persisted setting changes.
    pub fn on_change(mut self, callback: impl Fn(SettingsSnapshot) + 'static) -> Self {
        self.on_change = Some(Rc::new(callback));
        self
    }

    /// Installs the host callback that actually runs the Agent Skill
    /// install command (F-SET-09). Unset, the Install Skill button renders
    /// muted and inert — see the field doc on [`Settings::on_install_skill`].
    pub fn on_install_skill(mut self, callback: impl Fn(SkillInstallCommand) + 'static) -> Self {
        self.on_install_skill = Some(Rc::new(callback));
        self
    }

    /// Installs the host callback that runs an agent row's Install command
    /// (F-SET-18). Unset, an agent with a known install command still
    /// renders the button muted and inert — see the field doc on
    /// [`Settings::on_install_agent`].
    /// Pins the launch sources used by the Agents screen (Task 9).
    pub fn with_launch_sources(
        mut self,
        sources: Vec<(String, sirio_registry::LaunchSource)>,
    ) -> Self {
        self.launch_sources = sources;
        self
    }

    /// Applies a fresh launch-source sweep plus the registry's current
    /// versions. Called by the host whenever Task 8 recomputes.
    pub fn apply_launch_sources(
        &mut self,
        sources: Vec<(String, sirio_registry::LaunchSource)>,
        registry_versions: BTreeMap<String, String>,
    ) {
        self.launch_sources = sources;
        self.registry_versions = registry_versions;
    }

    /// Sets (or clears, on `None`) one row's live install state. Callers
    /// notify afterwards.
    pub fn set_install_state(&mut self, id: &str, state: Option<InstallState>) {
        match state {
            Some(state) => {
                self.install_states.insert(id.to_string(), state);
            }
            None => {
                self.install_states.remove(id);
            }
        }
    }

    /// Installs a host override for a provider card's Add Account button
    /// (F-SET-14). Without this callback the built-in external-CLI fallback
    /// remains active; embedders can use the callback to provide their own
    /// terminal or account-management surface.
    pub fn on_manage_account(mut self, callback: impl Fn(&'static str) + 'static) -> Self {
        self.on_manage_account = Some(Rc::new(callback));
        self
    }

    /// Seeds the Permissions screen from the host's durable browser grants.
    pub fn with_browser_origins(mut self, origins: impl IntoIterator<Item = String>) -> Self {
        self.browser_origins = origins.into_iter().collect();
        self
    }

    /// Installs the host callback used when one browser origin is revoked.
    pub fn on_revoke_browser_origin(mut self, callback: impl Fn(String) + 'static) -> Self {
        self.on_revoke_browser_origin = Some(Rc::new(callback));
        self
    }

    /// Installs the host callback used when all browser origins are revoked.
    pub fn on_revoke_all_browser_origins(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_revoke_all_browser_origins = Some(Rc::new(callback));
        self
    }

    /// Replaces the browser-grant snapshot when the host observes a grant in
    /// a browser surface or a grant was revoked elsewhere.
    pub fn set_browser_origins(
        &mut self,
        origins: impl IntoIterator<Item = String>,
        cx: &mut Context<Self>,
    ) {
        self.browser_origins = origins.into_iter().collect();
        cx.notify();
    }

    /// Returns the browser-grant snapshot currently shown in Permissions.
    pub fn browser_origins(&self) -> impl Iterator<Item = &str> {
        self.browser_origins.iter().map(String::as_str)
    }

    fn revoke_browser_origin(&mut self, origin: String, cx: &mut Context<Self>) {
        if !self.browser_origins.remove(&origin) {
            return;
        }
        if let Some(callback) = self.on_revoke_browser_origin.clone() {
            callback(origin);
        }
        cx.notify();
    }

    fn revoke_all_browser_origins(&mut self, cx: &mut Context<Self>) {
        if self.browser_origins.is_empty() {
            return;
        }
        self.browser_origins.clear();
        if let Some(callback) = self.on_revoke_all_browser_origins.clone() {
            callback();
        }
        cx.notify();
    }

    /// Returns the current values that belong to the persistence contract.
    pub fn snapshot(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            theme: self.theme_mode,
            interface_font_size: self.interface_font_size,
            terminal_font_size: self.terminal_font_size,
            base_color: self.base_color,
            control_socket_enabled: self.control_socket_enabled,
            socket_path: self.socket_path.clone(),
            resume_agent_sessions: self.resume_agent_sessions,
            auto_naming: self.auto_naming,
            limit_chat_history: self.limit_chat_history,
            chat_retention: self.chat_retention,
            limit_mounted_worktrees: self.limit_mounted_worktrees,
            mounted_worktrees: self.mounted_worktrees,
            summarizer_agent: self.summarizer_agent,
            claude_show_in_bar: self.claude_show_in_bar,
            codex_show_in_bar: self.codex_show_in_bar,
            opencode_show_in_bar: self.opencode_show_in_bar,
            ollama_show_in_bar: self.ollama_show_in_bar,
            refresh_interval: self.refresh_interval,
            opencode_workspace_id_override: self.opencode_workspace_id_override.clone(),
            translucency: self.translucency,
        }
    }

    /// Returns the complete state currently shown by the Settings surface.
    pub fn report(&self) -> SettingsReport {
        SettingsReport {
            category: self.category,
            snapshot: self.snapshot(),
            provider_availability: self.provider_availability.clone(),
            resume_agent_sessions: self.resume_agent_sessions,
            auto_naming: self.auto_naming,
            limit_chat_history: self.limit_chat_history,
            chat_retention: self.chat_retention,
            limit_mounted_worktrees: self.limit_mounted_worktrees,
            mounted_worktrees: self.mounted_worktrees,
            claude_show_in_bar: self.claude_show_in_bar,
            codex_show_in_bar: self.codex_show_in_bar,
            opencode_show_in_bar: self.opencode_show_in_bar,
            ollama_show_in_bar: self.ollama_show_in_bar,
            refresh_interval: self.refresh_interval,
        }
    }

    fn changed(&self) {
        if let Some(callback) = &self.on_change {
            callback(self.snapshot());
        }
    }

    /// Selects the category shown in the detail column. Both the sidebar
    /// click and the control socket call this function.
    pub fn select_category(&mut self, category: SettingsCategory, cx: &mut Context<Self>) {
        if self.category != category {
            self.detail_scroll.set_offset(point(px(0.0), px(0.0)));
        }
        // Entering the Agents screen asks the host to re-check the launch
        // sources (registry fetch respecting its cache + recompute).
        let entering_agents =
            category == SettingsCategory::Agents && self.category != SettingsCategory::Agents;
        self.category = category;
        if entering_agents {
            cx.emit(SettingsEvent::RefreshAgentSources);
        }
        cx.notify();
    }

    fn set_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.theme_mode = mode;
        Theme::set_mode(mode, cx);
        // Repaint every mounted surface, including the shell behind this
        // full-window view. Theme is a GPUI global, so existing entities read
        // the new palette on their next render without owning a stale copy.
        cx.refresh_windows();
        self.changed();
        cx.notify();
    }

    fn set_base_color(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(base) = sirio_theme::BaseColor::ALL.get(index).copied() else {
            return;
        };
        self.base_color = base;
        Theme::set_base_color(base, cx);
        // Same reason as set_theme_mode: Theme is a GPUI global, so every
        // mounted surface repaints from the new palette on its next render.
        cx.refresh_windows();
        self.changed();
        cx.notify();
    }

    fn set_translucency(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.translucency = enabled;
        self.changed();
        cx.notify();
    }

    fn set_interface_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.interface_font_size = value.clamp(10, 20);
        Theme::set_interface_font_size(self.interface_font_size, cx);
        cx.refresh_windows();
        self.changed();
        cx.notify();
    }

    fn set_terminal_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.terminal_font_size = value.clamp(9, 24);
        self.changed();
        cx.notify();
    }

    fn set_control_socket_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.control_socket_enabled = enabled;
        self.changed();
        cx.notify();
    }

    fn set_updates_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.update_state.enabled = enabled;
        if let Some(callback) = &self.on_update_enabled_change {
            callback(enabled);
        }
        cx.notify();
    }

    fn theme_segment(mode: ThemeMode) -> usize {
        match mode {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }
    }

    fn set_provider_visibility(
        &mut self,
        provider: ProviderKind,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        match provider {
            ProviderKind::Claude => self.claude_show_in_bar = enabled,
            ProviderKind::Codex => self.codex_show_in_bar = enabled,
            ProviderKind::OpenCodeGo => self.opencode_show_in_bar = enabled,
            ProviderKind::OllamaCloud => self.ollama_show_in_bar = enabled,
        }
        self.changed();
        cx.notify();
    }

    fn provider_visibility(&self, provider: ProviderKind) -> bool {
        match provider {
            ProviderKind::Claude => self.claude_show_in_bar,
            ProviderKind::Codex => self.codex_show_in_bar,
            ProviderKind::OpenCodeGo => self.opencode_show_in_bar,
            ProviderKind::OllamaCloud => self.ollama_show_in_bar,
        }
    }

    fn set_refresh_interval(&mut self, value: i32, cx: &mut Context<Self>) {
        self.refresh_interval = value.clamp(1, 60);
        self.changed();
        cx.notify();
    }

    fn set_resume_agent_sessions(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.resume_agent_sessions = enabled;
        self.changed();
        cx.notify();
    }

    fn set_auto_naming(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_naming = enabled;
        self.changed();
        cx.notify();
    }

    fn set_limit_chat_history(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.limit_chat_history = enabled;
        self.changed();
        cx.notify();
    }

    fn set_chat_retention(&mut self, value: i32, cx: &mut Context<Self>) {
        self.chat_retention = value.clamp(5, 500);
        self.changed();
        cx.notify();
    }

    fn set_limit_mounted_worktrees(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.limit_mounted_worktrees = enabled;
        self.changed();
        cx.notify();
    }

    fn set_mounted_worktrees(&mut self, value: i32, cx: &mut Context<Self>) {
        self.mounted_worktrees = value.clamp(2, 50);
        self.changed();
        cx.notify();
    }

    fn set_summarizer_agent(&mut self, choice: SummarizerChoice, cx: &mut Context<Self>) {
        self.summarizer_agent = choice;
        self.close_summarizer_picker(cx);
        self.changed();
        cx.notify();
    }

    /// Re-reads the provider cards' account states from disk. This is what
    /// "Refresh now" means on this surface: the cards show local credential
    /// state, so a refresh is a fresh read — a user who just ran
    /// `claude login` in a terminal gets the card to match without a
    /// relaunch (F-SET-10).
    /// Handles the Agent Skill card's Install button (F-SET-09): reaches
    /// the wired host callback with the provisioner's command, and — click
    /// or no host wired to actually run it — flips `skill_install_launched`
    /// so the screen shows a real, notified state change rather than
    /// nothing happening.
    fn install_skill_clicked(&mut self, cx: &mut Context<Self>) {
        self.skill_install_launched = true;
        cx.notify();
        if let Some(handler) = self.on_install_skill.clone() {
            handler(sirio_project::agent_skill_install_command());
        }
    }

    fn refresh_provider_accounts(&mut self, cx: &mut Context<Self>) {
        self.provider_accounts = ProviderAccountStates::discovered();
        self.sync_account_identity_cache();
        self.sync_agent_accounts();
        cx.notify();
    }

    /// Opens the provider's own interactive login flow in a terminal (the
    /// desktop's emulator on Unix, a fresh console on Windows — see
    /// [`login_launcher`]). Sirio waits off the render thread and re-reads
    /// the local account stores when that terminal session ends, so
    /// cancel/retry and a successful login all leave the card truthful.
    ///
    /// F-SET-14: while the terminal is up, [`Self::account_login_pending`]
    /// is set so the card can render "Signing in…" and a Cancel button
    /// instead of leaving a click that reached a real subprocess with no
    /// visible sign anything happened.
    fn launch_account_login(&mut self, provider: ProviderKind, cx: &mut Context<Self>) {
        self.account_action_error = None;
        // Unreachable from the UI for a provider without a login command
        // (its card renders no Add Account button); a no-op beats a panic.
        let Some(login) = provider_login_command(provider) else {
            return;
        };
        let program = login.program;
        let args = login.args;
        let entity = cx.entity();
        self.account_login_pending = Some(provider);
        self.account_login_pid = None;
        self.account_login_canceled = false;
        cx.notify();
        cx.spawn(async move |_, cx| {
            let spawned = cx
                .background_spawn(async move {
                    let mut command = login_launcher(program, &args);
                    let launcher = command.get_program().to_string_lossy().into_owned();
                    command
                        .spawn()
                        .map_err(|error| login_start_failure(program, &launcher, &error))
                })
                .await;

            let mut child = match spawned {
                Ok(child) => child,
                Err(message) => {
                    entity.update(cx, |settings, cx| {
                        settings.account_login_pending = None;
                        settings.account_login_pid = None;
                        settings.account_action_error = Some((provider, message));
                        cx.notify();
                    });
                    return;
                }
            };
            // Publish the pid as soon as the process exists, so a Cancel
            // click that lands before this point still has something to
            // kill once it does; [`Self::cancel_account_login`] guards on
            // `account_login_pending` still naming this provider, so a
            // Cancel that already fired is not clobbered by this update.
            let pid = child.id();
            entity.update(cx, |settings, cx| {
                if settings.account_login_pending == Some(provider) {
                    settings.account_login_pid = Some(pid);
                    cx.notify();
                }
            });

            let result = cx.background_spawn(async move { child.wait() }).await;

            entity.update(cx, |settings, cx| {
                settings.account_login_pending = None;
                settings.account_login_pid = None;
                if settings.account_login_canceled {
                    // Cancel already set the "Sign-in canceled" message and
                    // fired its own notify; the process's own exit status
                    // (a signal, once `kill` lands) is not a real outcome.
                    settings.account_login_canceled = false;
                    return;
                }
                settings.provider_accounts = ProviderAccountStates::discovered();
                settings.sync_account_identity_cache();
                settings.account_action_error = match result {
                    Ok(status) if status.success() => None,
                    Ok(status) => Some((
                        provider,
                        format!(
                            "{} login exited without success ({})",
                            program,
                            status
                                .code()
                                .map_or_else(|| "signal".to_string(), |code| code.to_string())
                        ),
                    )),
                    Err(error) => Some((
                        provider,
                        format!("could not start {program} login: {error}"),
                    )),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Cancels a login [`Self::launch_account_login`] spawned (F-SET-14).
    /// Killing the terminal emulator process closes its pty, which takes
    /// the foreground login command down with it on every terminal this app
    /// targets; the outstanding `child.wait()` in the login task then
    /// resolves on its own and clears the pending state from there — this
    /// only needs to handle the pid and the message. If this runs before
    /// the spawn task has published a pid yet (a race no human click can
    /// realistically win, since the surface must render the Cancel button
    /// first), there is nothing to kill and the terminal is left running;
    /// the button simply reappears as "Add Account" once that login exits
    /// on its own.
    fn cancel_account_login(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.account_login_pending.take() else {
            return;
        };
        if let Some(pid) = self.account_login_pid.take() {
            // Off the render thread: the escalation below deliberately waits
            // out a grace period before the kill-9, and blocking here would
            // freeze the surface for that whole window.
            cx.background_spawn(async move { terminate_login_process_group(pid) })
                .detach();
        }
        self.account_login_canceled = true;
        self.account_action_error = Some((provider, "Sign-in canceled".to_string()));
        cx.notify();
    }

    /// Re-runs agent discovery for the Agents screen's "↻ Refresh" button
    /// (F-SET-16), the same re-read-from-disk meaning "Refresh now" already
    /// has on the AI Providers screen: an agent installed or removed since
    /// launch shows up without a relaunch. This button doubles as the
    /// registry error state's retry (F-SET-17).
    ///
    /// Also stamps [`Self::agent_last_refreshed`] — Search and Refresh were
    /// both genuinely wired before this row; what was missing was a
    /// rendered sign the click did anything, which a byte-identical capture
    /// (a Refresh with nothing new to discover) then read as "absent".
    fn refresh_agent_availability(&mut self, cx: &mut Context<Self>) {
        self.apply_agent_discovery(try_discover_availability());
        self.agent_last_refreshed = Some(Self::format_refreshed_stamp());
        // The PATH sweep above is local; the registry document itself is
        // the host's to re-fetch (24 h cache respected) — Task 9's promise,
        // wired here.
        cx.emit(SettingsEvent::RefreshAgentSources);
        cx.notify();
    }

    /// The Agents screen's "Refreshed …" stamp (F-SET-16), local time to
    /// the second — the same clock and precision `chat.rs`'s message
    /// timestamps already use.
    fn format_refreshed_stamp() -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    /// The registry failure message, shaped like the Swift original's
    /// `AcpAgentCenter.registryError`.
    fn registry_error_message(error: &DiscoveryError) -> String {
        format!("Could not load the agent registry: {error}")
    }

    /// Applies one discovery sweep's outcome (F-SET-17): success replaces
    /// the rows and clears the banner; failure keeps the previous rows on
    /// screen — the last good list stays visible under the error, exactly
    /// as the Swift registry keeps its rows from the last successful fetch
    /// — and records the renderable message.
    fn apply_agent_discovery(&mut self, outcome: Result<Vec<AgentAvailability>, DiscoveryError>) {
        match outcome {
            Ok(rows) => {
                self.provider_availability = rows;
                self.agent_registry_error = None;
            }
            Err(error) => {
                self.agent_registry_error = Some(Self::registry_error_message(&error));
            }
        }
    }

    /// The clipboard text a paste into one of these single-line fields
    /// inserts. A DevTools or URL-bar copy usually carries a trailing
    /// newline, which would corrupt the stored cookie header and the
    /// persisted workspace id — so it is trimmed here, once, for every
    /// field that pastes.
    fn pasted_field_text(cx: &App) -> Option<String> {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    /// Blink timer tick shared by every settings text field's caret.
    fn flip_field_blink(&mut self, cx: &mut Context<Self>) {
        self.field_blink.flip();
        cx.notify();
    }

    /// Raw-keystroke handling for the OpenCode Go cookie field (F-SET-12),
    /// the agents search field's backspace/character pattern. The typed
    /// text stays transient — Save moves it into the credential store.
    /// A cookie is pasted, never typed: ctrl-v/cmd-v insert the clipboard
    /// text, which the character branch below must not also swallow.
    fn on_opencode_cookie_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let key = event.keystroke.key.as_str();
        if key == "v" && (event.keystroke.modifiers.platform || event.keystroke.modifiers.control) {
            if let Some(text) = Self::pasted_field_text(cx) {
                self.opencode_cookie_input.push_str(&text);
            }
        } else if key == "backspace" || key == "delete" {
            self.opencode_cookie_input.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.opencode_cookie_input.push_str(character);
        }
        cx.notify();
    }

    /// Raw-keystroke handling for the workspace-ID override (F-SET-12).
    /// Unlike the cookie this *is* the durable value, so every edit goes
    /// through the persistence contract — the macOS original binds the
    /// same field to `@AppStorage` and refreshes usage on change, which
    /// here falls out of the host routing the changed snapshot into the
    /// status bar's preferences.
    fn on_opencode_override_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let key = event.keystroke.key.as_str();
        if key == "v" && (event.keystroke.modifiers.platform || event.keystroke.modifiers.control) {
            if let Some(text) = Self::pasted_field_text(cx) {
                self.opencode_workspace_id_override.push_str(&text);
            }
        } else if key == "backspace" || key == "delete" {
            self.opencode_workspace_id_override.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.opencode_workspace_id_override.push_str(character);
        }
        self.changed();
        cx.notify();
    }

    /// Saves the typed cookie into the credential store (F-SET-12). On
    /// success the input clears, the account state flips to signed in and
    /// the provider's usage bar segment turns on through the persistence
    /// contract — the macOS Save button's exact side effects (its
    /// `showInBar = true` + `refreshOpencodeGo` pair rides the existing
    /// visibility plumbing: `changed()` → host persists → the status bar
    /// re-derives its preferences and refetches). On failure the input is
    /// *kept* alongside a visible error: this control's one forbidden
    /// outcome is dropping the pasted value silently.
    fn save_opencode_cookie(&mut self, cx: &mut Context<Self>) {
        let cookie = self.opencode_cookie_input.trim().to_string();
        if cookie.is_empty() {
            return;
        }
        let outcome = match &self.credential_store {
            Some(store) => store.set(OpenCodeGoUsageFetcher::COOKIE_KEY, &cookie),
            None => Err(CredentialStoreError::NoHome),
        };
        match outcome {
            Ok(()) => {
                self.opencode_cookie_input.clear();
                self.opencode_cookie_error = None;
                // Presence in the store we just wrote is known — no disk
                // re-read needed, and none would say more (presence, not
                // validity, is the account-state contract).
                self.provider_accounts.opencode_go =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn);
                self.set_provider_visibility(ProviderKind::OpenCodeGo, true, cx);
            }
            Err(error) => {
                self.opencode_cookie_error =
                    Some(format!("Failed to update the credential store — {error}"));
                cx.notify();
            }
        }
    }

    /// Deletes the stored cookie (F-SET-12): the account state flips to
    /// not signed in and the provider leaves the usage bar, mirroring the
    /// macOS Clear button.
    fn clear_opencode_cookie(&mut self, cx: &mut Context<Self>) {
        let outcome = match &self.credential_store {
            Some(store) => store.delete(OpenCodeGoUsageFetcher::COOKIE_KEY),
            None => Err(CredentialStoreError::NoHome),
        };
        match outcome {
            Ok(()) => {
                self.opencode_cookie_error = None;
                self.provider_accounts.opencode_go =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut);
                self.set_provider_visibility(ProviderKind::OpenCodeGo, false, cx);
            }
            Err(error) => {
                self.opencode_cookie_error =
                    Some(format!("Failed to update the credential store — {error}"));
                cx.notify();
            }
        }
    }

    /// Clears the workspace-ID override back to discovery (F-SET-12).
    fn clear_opencode_override(&mut self, cx: &mut Context<Self>) {
        self.opencode_workspace_id_override.clear();
        self.changed();
        cx.notify();
    }

    /// Raw-keystroke handling for the Ollama Cloud cookie (F-SET-13) —
    /// transient like [`Self::on_opencode_cookie_key`]: nothing durable
    /// happens until Save. Paste lands the same way, for the same reason.
    fn on_ollama_cookie_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let key = event.keystroke.key.as_str();
        if key == "v" && (event.keystroke.modifiers.platform || event.keystroke.modifiers.control) {
            if let Some(text) = Self::pasted_field_text(cx) {
                self.ollama_cookie_input.push_str(&text);
            }
        } else if key == "backspace" || key == "delete" {
            self.ollama_cookie_input.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.ollama_cookie_input.push_str(character);
        }
        cx.notify();
    }

    /// Saves the typed Ollama Cloud cookie (F-SET-13) — the macOS Save
    /// button's exact side effects, same shape as
    /// [`Self::save_opencode_cookie`]: on success the input clears, the
    /// account state flips to signed in and the bar segment turns on
    /// through the persistence contract (the status bar refetches from its
    /// re-derived preferences). On failure the input is *kept* alongside a
    /// visible error — never a silently dropped paste.
    fn save_ollama_cookie(&mut self, cx: &mut Context<Self>) {
        let cookie = self.ollama_cookie_input.trim().to_string();
        if cookie.is_empty() {
            return;
        }
        let outcome = match &self.credential_store {
            Some(store) => store.set(OllamaCloudUsageFetcher::COOKIE_KEY, &cookie),
            None => Err(CredentialStoreError::NoHome),
        };
        match outcome {
            Ok(()) => {
                self.ollama_cookie_input.clear();
                self.ollama_cookie_error = None;
                self.provider_accounts.ollama_cloud =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn);
                self.set_provider_visibility(ProviderKind::OllamaCloud, true, cx);
            }
            Err(error) => {
                self.ollama_cookie_error =
                    Some(format!("Failed to update the credential store — {error}"));
                cx.notify();
            }
        }
    }

    /// Deletes the stored Ollama Cloud cookie (F-SET-13): the account
    /// state flips to not signed in and the provider leaves the usage bar,
    /// mirroring the macOS Clear button.
    fn clear_ollama_cookie(&mut self, cx: &mut Context<Self>) {
        let outcome = match &self.credential_store {
            Some(store) => store.delete(OllamaCloudUsageFetcher::COOKIE_KEY),
            None => Err(CredentialStoreError::NoHome),
        };
        match outcome {
            Ok(()) => {
                self.ollama_cookie_error = None;
                self.provider_accounts.ollama_cloud =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut);
                self.set_provider_visibility(ProviderKind::OllamaCloud, false, cx);
            }
            Err(error) => {
                self.ollama_cookie_error =
                    Some(format!("Failed to update the credential store — {error}"));
                cx.notify();
            }
        }
    }

    /// Tells the surface to take focus on its next render. Called by the
    /// host when the settings surface opens, so the shell's Escape handler
    /// (attached to the workspace root, which the focused surface's dispatch
    /// path includes) can receive the keystroke (F-SET-02).
    pub fn request_surface_focus(&mut self) {
        self.surface_focus_pending = true;
    }

    /// Whether the summarizer picker may open: the picker chooses the agent
    /// that auto-renames tabs, so it is disabled while auto-naming is off
    /// (F-SET-05's VERIFY clause: "the summarizer picker is disabled when
    /// off and enabled when on").
    fn summarizer_picker_enabled(&self) -> bool {
        self.auto_naming
    }

    fn toggle_summarizer_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.summarizer_picker_enabled() {
            return;
        }
        self.summarizer_popover_open = !self.summarizer_popover_open;
        if self.summarizer_popover_open {
            let focus = self.summarizer_focus.clone();
            window.focus(&focus, cx);
        } else {
            self.surface_focus_pending = true;
        }
        cx.notify();
    }

    fn close_summarizer_picker(&mut self, cx: &mut Context<Self>) {
        if !self.summarizer_popover_open {
            return;
        }
        self.summarizer_popover_open = false;
        // The menu held focus; hand it back to the surface so the shell's
        // Escape keeps closing settings rather than dying on the stale
        // focus the closed menu left behind.
        self.surface_focus_pending = true;
        cx.notify();
    }

    fn render_header(&self, theme: Theme) -> impl IntoElement {
        let back = self.on_back.clone();
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(theme.surface)
            .child(
                // The arrow is the control: no word beside it, and sized
                // off `large_title` so it still tracks the interface font
                // size rather than freezing at one pixel count.
                div()
                    .id("settings-back")
                    .debug_selector(|| "settings-back".into())
                    .w(px(BACK_CONTROL_SIZE))
                    .h(px(BACK_CONTROL_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.chip_active)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, _| {
                        if let Some(callback) = &back {
                            callback();
                        }
                    })
                    .child(
                        IconElement::new(
                            Icon::ChevronLeft,
                            IconSize::Custom(theme.typography.large_title),
                        )
                        .text_color(theme.text),
                    ),
            )
            .child(
                div()
                    .text_size(theme.typography.title3)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(text!("Settings")),
            )
    }

    fn render_categories(&self, theme: Theme, entity: Entity<Self>) -> impl IntoElement {
        let mut list = div()
            .w(px(CATEGORY_WIDTH))
            .h_full()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .bg(theme.surface);

        for (category_index, category) in SettingsCategory::ALL.into_iter().enumerate() {
            let selected = self.category == category;
            let entity = entity.clone();
            list = list.child(
                div()
                    .id(format!("settings-category-{:?}", category))
                    .debug_selector(move || format!("settings-category-{:?}", category))
                    .w_full()
                    .h(px(30.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.headline)
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .when(selected, |this| this.bg(theme.element_active))
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| this.select_category(category, cx));
                    })
                    .child(
                        div()
                            .w(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                IconElement::new(category.glyph(), IconSize::Small).text_color(
                                    if selected {
                                        theme.text
                                    } else {
                                        theme.text_muted
                                    },
                                ),
                            ),
                    )
                    .child(text!(
                        id = ("settings-category-title", category_index),
                        category.title()
                    )),
            );
        }

        list.child(div().flex_1())
    }

    fn render_appearance(&self, theme: Theme, mode: ThemeMode, entity: Entity<Self>) -> gpui::Div {
        let theme_entity = entity.clone();
        let theme_control = controls::segmented(
            "appearance-theme",
            SEGMENTED_THEME,
            Self::theme_segment(mode),
            theme,
            move |index, cx| {
                let mode = match index {
                    1 => ThemeMode::Light,
                    2 => ThemeMode::Dark,
                    _ => ThemeMode::System,
                };
                theme_entity.update(cx, |this, cx| this.set_theme_mode(mode, cx));
            },
        );
        let base_entity = entity.clone();
        let base_control = controls::segmented(
            "appearance-base-color",
            SEGMENTED_BASE_COLOR,
            base_color_segment(self.base_color),
            theme,
            move |index, cx| {
                base_entity.update(cx, |this, cx| this.set_base_color(index, cx));
            },
        );
        let translucency_entity = entity.clone();
        let translucency = controls::toggle(
            "appearance-translucency",
            self.translucency,
            theme,
            move |_, _, cx| {
                translucency_entity
                    .update(cx, |this, cx| this.set_translucency(!this.translucency, cx));
            },
        );

        let mut theme_card = controls::card(theme)
            .child(
                div()
                    .id("settings-appearance-theme-row")
                    .child(controls::row("Appearance", None, theme_control, theme)),
            )
            .child(controls::separator(theme));
        theme_card = theme_card
            .child(
                div()
                    .id("settings-appearance-base-color-row")
                    .child(controls::row("Base color", None, base_control, theme)),
            )
            .child(controls::separator(theme));
        theme_card = theme_card.child(
            div()
                .id("settings-appearance-translucency-row")
                .child(controls::row("Translucency", None, translucency, theme)),
        );

        let interface_entity = entity.clone();
        let interface_stepper = controls::stepper(
            "interface-font-size",
            self.interface_font_size,
            "pt",
            theme,
            move |value, cx| {
                interface_entity.update(cx, |this, cx| this.set_interface_font_size(value, cx));
            },
        );
        let terminal_entity = entity.clone();
        let terminal_stepper = controls::stepper(
            "terminal-font-size",
            self.terminal_font_size,
            "pt",
            theme,
            move |value, cx| {
                terminal_entity.update(cx, |this, cx| this.set_terminal_font_size(value, cx));
            },
        );

        let interface_card =
            controls::card(theme).child(div().id("settings-interface-font-size-row").child(
                // #201: no subtitle. `controls::stepper` is given the
                // value *and* the unit, so it already reads "13 pt"; a
                // subtitle repeating it left the row saying the number
                // twice and saying nothing about the setting. The
                // comparable stepper row, "Keep chats per worktree",
                // passes `None` here for the same reason.
                controls::row("Font size", None, interface_stepper, theme),
            ));
        let terminal_card =
            controls::card(theme).child(div().id("settings-terminal-font-size-row").child(
                // #201: see the interface row above.
                controls::row("Font size", None, terminal_stepper, theme),
            ));

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("Theme", theme_card, theme))
            .child(settings_section("Interface", interface_card, theme))
            .child(settings_section("Terminal", terminal_card, theme))
    }

    fn render_provider_card(
        &self,
        view: ProviderCardView,
        entity: Entity<Self>,
        theme: Theme,
        window: &Window,
    ) -> gpui::Div {
        let ProviderCardView {
            kind: provider,
            title,
            glyph_color,
            status,
        } = view;
        let status_label = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(theme.typography.headline)
            .text_color(theme.text)
            .child(
                div()
                    .w(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        IconElement::new(provider.icon(), IconSize::Medium).text_color(glyph_color),
                    ),
            )
            .child(text!(
                id = format!("settings-provider-status-label-{title}"),
                "Status"
            ));
        // The status value is the derived account state — signed in, not
        // signed in, or honestly unknown — never a hardcoded "Active" that
        // claimed a working account without checking. Green only when real
        // credentials exist; "Not signed in" and "Unknown" are neutral,
        // informational states.
        let mut status_value = div()
            .id(format!("settings-provider-account-status-{title}"))
            .debug_selector(move || format!("settings-provider-account-status-{title}"))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(theme.typography.callout)
            .text_color(theme.text)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(if status.signed_in {
                        theme.success
                    } else {
                        theme.text_faint
                    }),
            )
            .child(text!(status.label));
        if let Some(identity) = status.identity.clone() {
            status_value = status_value.child(
                div()
                    .id(format!("settings-provider-account-identity-{title}"))
                    .debug_selector(move || format!("settings-provider-account-identity-{title}"))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(text!(identity)),
            );
        }

        let visibility_entity = entity.clone();
        let visibility = controls::toggle(
            match provider {
                ProviderKind::Claude => "provider-claude-visibility",
                ProviderKind::Codex => "provider-codex-visibility",
                ProviderKind::OpenCodeGo => "provider-opencode-visibility",
                ProviderKind::OllamaCloud => "provider-ollama-visibility",
            },
            self.provider_visibility(provider),
            theme,
            move |_, _, cx| {
                visibility_entity.update(cx, |this, cx| {
                    let enabled = !this.provider_visibility(provider);
                    this.set_provider_visibility(provider, enabled, cx);
                });
            },
        );

        let refresh_entity = entity.clone();
        let refresh_stepper = controls::stepper(
            match provider {
                ProviderKind::Claude => "provider-claude-refresh",
                ProviderKind::Codex => "provider-codex-refresh",
                ProviderKind::OpenCodeGo => "provider-opencode-refresh",
                ProviderKind::OllamaCloud => "provider-ollama-refresh",
            },
            self.refresh_interval,
            "min",
            theme,
            move |value, cx| {
                refresh_entity.update(cx, |this, cx| this.set_refresh_interval(value, cx));
            },
        );
        let refresh_now_entity = entity.clone();
        let refresh_now = controls::button(
            match provider {
                ProviderKind::Claude => "refresh-claude-now",
                ProviderKind::Codex => "refresh-codex-now",
                ProviderKind::OpenCodeGo => "refresh-opencode-now",
                ProviderKind::OllamaCloud => "refresh-ollama-now",
            },
            "Refresh now",
            theme,
            move |_, _, cx| {
                refresh_now_entity.update(cx, |this, cx| this.refresh_provider_accounts(cx));
            },
        );

        let mut card = controls::card(theme)
            .child(controls::row_view(status_label, status_value, theme))
            .child(controls::separator(theme))
            .child(controls::row("Show in usage bar", None, visibility, theme))
            .child(controls::separator(theme))
            .child(controls::row(
                "Refresh interval",
                None,
                refresh_stepper,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::action_row(refresh_now, theme))
            .child(controls::separator(theme));

        // F-SET-12/F-SET-13: OpenCode Go and Ollama Cloud authenticate
        // with a pasted web cookie — Claude and Codex read the credential
        // files their own CLIs write, so their cards have no equivalent
        // rows.
        if provider == ProviderKind::OpenCodeGo {
            card = self.opencode_cookie_section(card, entity.clone(), theme, window);
        }
        if provider == ProviderKind::OllamaCloud {
            card = self.ollama_cookie_section(card, entity.clone(), theme, window);
        }

        // F-SET-13: the Ollama Cloud card ends at its cookie section — the
        // reference card has no Accounts list, no System default row and no
        // login flow (its only credential is the pasted cookie above), so
        // none of the account rows below exist for it.
        if provider == ProviderKind::OllamaCloud {
            return card;
        }

        // F-SET-14: this app never holds its own per-provider credentials
        // (see the "System default" row's comment below), so "Add Account"
        // cannot open an isolated in-app account the way the macOS original
        // does — it delegates to the provider CLI's own login flow rather
        // than inventing a Sirio-owned account store. The optional host
        // callback remains an override for embedding and tests.
        let provider_id = match provider {
            ProviderKind::Claude => "claude",
            ProviderKind::Codex => "codex",
            ProviderKind::OpenCodeGo => "opencode",
            ProviderKind::OllamaCloud => "ollama",
        };
        let manage_account_entity = entity.clone();
        let host_manage_account = self.on_manage_account.clone();
        let manage_account_handler =
            Some(move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                if let Some(handler) = host_manage_account.as_ref() {
                    handler(provider_id);
                } else {
                    manage_account_entity.update(cx, |settings, cx| {
                        settings.launch_account_login(provider, cx)
                    });
                }
            });
        if let Some((error_provider, error)) = self.account_action_error.as_ref()
            && *error_provider == provider
        {
            card = card.child(
                div()
                    .id(format!("provider-account-error-{title}"))
                    .debug_selector(move || format!("provider-account-error-{title}"))
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_XS))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.danger)
                    .child(text!(error.clone())),
            );
        }
        // F-SET-14: while this card's login is spawned and being waited on,
        // "Add Account" is replaced by a "Signing in…" indicator and a
        // Cancel button — a click that already reached a real subprocess
        // must not look like nothing happened, and a login that will not
        // finish (a wrong password, a login the user no longer wants) must
        // be escapable without alt-tabbing to the spawned terminal.
        let account_action: AnyElement = if self.account_login_pending == Some(provider) {
            let cancel_entity = entity.clone();
            div()
                .flex()
                .items_center()
                .gap(px(BezelTheme::SPACE_MD))
                .child(
                    div()
                        .id(format!("account-login-pending-{title}"))
                        .debug_selector(move || format!("account-login-pending-{title}"))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text_muted)
                        .child(text!("Signing in…")),
                )
                .child(controls::button(
                    match provider {
                        ProviderKind::Claude => "cancel-claude-account",
                        ProviderKind::Codex => "cancel-codex-account",
                        ProviderKind::OpenCodeGo => "cancel-opencode-account",
                        // Unreachable: the Ollama card returned above.
                        ProviderKind::OllamaCloud => "cancel-ollama-account",
                    },
                    "Cancel",
                    theme,
                    move |_, _, cx| {
                        cancel_entity.update(cx, |settings, cx| settings.cancel_account_login(cx));
                    },
                ))
                .into_any_element()
        } else {
            controls::button_maybe(
                match provider {
                    ProviderKind::Claude => "add-claude-account",
                    ProviderKind::Codex => "add-codex-account",
                    ProviderKind::OpenCodeGo => "add-opencode-account",
                    // Unreachable: the Ollama card returned above.
                    ProviderKind::OllamaCloud => "add-ollama-account",
                },
                "Add Account",
                theme,
                manage_account_handler,
            )
            .into_any_element()
        };
        // F-SET-15: the "System default" row's "Active" badge is a
        // selection marker among however many accounts exist for this
        // provider, not a hardcoded claim — it is active exactly when no
        // isolated account is selected. Claude/Codex may have any number of
        // additional isolated accounts (from `add_agent_account`); every
        // other provider keeps exactly the one always-active row it always
        // had, since the reference app never gave them multi-account
        // support either (see the comment above `account_action`).
        let (accounts, active_account_id) = match provider {
            ProviderKind::Claude => (
                self.claude_accounts.as_slice(),
                &self.active_claude_account_id,
            ),
            ProviderKind::Codex => (
                self.codex_accounts.as_slice(),
                &self.active_codex_account_id,
            ),
            ProviderKind::OpenCodeGo | ProviderKind::OllamaCloud => (&[][..], &None),
        };
        let select_entity = entity.clone();
        let select_provider_id = provider_id;
        card = card.child(controls::subsection_header(
            "Accounts",
            "Showing accounts for this device. New accounts are added there.",
            account_action,
            theme,
        ));
        card = card.child(controls::account_row(
            format!("system-default-{select_provider_id}"),
            "System default".to_string(),
            "Use your current CLI login on this device.".to_string(),
            active_account_id.is_none(),
            theme,
            {
                let entity = select_entity.clone();
                let provider_id = select_provider_id;
                move |_, _, cx| {
                    entity.update(cx, |settings, cx| {
                        settings.select_agent_account(provider_id, None, cx)
                    });
                }
            },
        ));
        for account in accounts {
            let is_active = active_account_id.as_deref() == Some(account.id.as_str());
            let account_id = account.id.clone();
            card = card.child(controls::account_row(
                format!("account-{}", account.id),
                account.label.clone(),
                account.config_dir_path.clone(),
                is_active,
                theme,
                {
                    let entity = select_entity.clone();
                    let provider_id = select_provider_id;
                    move |_, _, cx| {
                        entity.update(cx, |settings, cx| {
                            settings.select_agent_account(provider_id, Some(account_id.clone()), cx)
                        });
                    }
                },
            ));
        }
        card
    }

    /// One text-entry row for the OpenCode Go section (F-SET-12): the
    /// agents search field's click-to-focus + raw-keystroke shape, reused
    /// for the cookie (masked) and the workspace override (plain).
    #[allow(clippy::too_many_arguments)]
    fn opencode_text_field(
        id: &'static str,
        display_text: String,
        placeholder: &'static str,
        is_empty: bool,
        focus: FocusHandle,
        is_focused: bool,
        caret_visible: bool,
        theme: Theme,
        on_focus: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        on_key: impl Fn(&mut Self, &KeyDownEvent, &mut Window, &mut Context<Self>) + 'static,
        entity: Entity<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let click_entity = entity.clone();
        let key_entity = entity;
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .track_focus(&focus)
            .w(px(300.0))
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(theme.radii.control)
            .bg(theme.input_bg)
            .border_1()
            .border_color(if is_focused { theme.ring } else { theme.border })
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_entity.update(cx, |this, cx| on_focus(this, window, cx));
            })
            .on_key_down(move |event, window, cx| {
                key_entity.update(cx, |this, cx| on_key(this, event, window, cx));
            })
            .text_size(theme.typography.callout)
            .text_color(if is_empty {
                theme.text_faint
            } else {
                theme.text
            })
            // #212: clip inside the field rather than drawing past its
            // border. Must shrink without growing: `flex_1` would push
            // the end-of-text caret to the far right.
            .overflow_hidden()
            .child(
                caret::field_value(text!(if is_empty {
                    placeholder.to_string()
                } else {
                    display_text
                }))
                .id("settings-text-field-text")
                .debug_selector(|| "settings-text-field-text".to_owned()),
            )
            // The field's insertion caret: end-of-text, since these compact
            // single-line fields always append. Invisible (but still laid
            // out) while unfocused so the bar never shifts the text.
            .when(is_focused, |this| {
                this.child(caret::bar(px(16.0), theme.text, caret_visible))
            })
    }

    /// A caption line under a cookie-section row, the same footnote tone
    /// `controls::row` uses for descriptions.
    fn opencode_caption(
        id: &'static str,
        caption: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .px(px(BezelTheme::SPACE_MD))
            .pb(px(BezelTheme::SPACE_XS))
            .text_size(theme.typography.footnote)
            .text_color(theme.text_muted)
            .child(text!(caption))
    }

    /// The OpenCode Go card's session-cookie and workspace-override rows
    /// (F-SET-12), between the usage controls and the Accounts subsection.
    fn opencode_cookie_section(
        &self,
        mut card: gpui::Div,
        entity: Entity<Self>,
        theme: Theme,
        window: &Window,
    ) -> gpui::Div {
        let cookie_text = self.opencode_cookie_input.clone();
        let cookie_is_empty = cookie_text.is_empty();
        // The cookie renders as mask dots only — the real value is never
        // drawn, matching the macOS SecureField.
        let masked = "•".repeat(cookie_text.chars().count());
        let cookie_field = Self::opencode_text_field(
            "provider-opencode-cookie-field",
            masked,
            "Session cookie",
            cookie_is_empty,
            self.opencode_cookie_focus.clone(),
            self.opencode_cookie_focus.is_focused(window),
            self.field_caret_visible,
            theme,
            |this, window, cx| this.opencode_cookie_focus.focus(window, cx),
            |this, event, window, cx| this.on_opencode_cookie_key(event, window, cx),
            entity.clone(),
        );

        // Save is inert while the trimmed input is empty — the same
        // muted "not wired" rendering every unwirable control here uses,
        // and the macOS Save button's own disabled condition.
        let save_entity = entity.clone();
        let save_handler = if cookie_text.trim().is_empty() {
            None
        } else {
            Some(move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                save_entity.update(cx, |this, cx| this.save_opencode_cookie(cx));
            })
        };
        let clear_cookie_entity = entity.clone();

        card = card
            .child(
                div()
                    .w_full()
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_SM))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(cookie_field)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(controls::button_maybe(
                                "save-opencode-cookie",
                                "Save",
                                theme,
                                save_handler,
                            ))
                            .child(controls::button(
                                "clear-opencode-cookie",
                                "Clear",
                                theme,
                                move |_, _, cx| {
                                    clear_cookie_entity
                                        .update(cx, |this, cx| this.clear_opencode_cookie(cx));
                                },
                            )),
                    ),
            )
            .child(Self::opencode_caption(
                "provider-opencode-cookie-caption",
                "Paste either the raw token value (e.g. Fe26.2**…) or the full cookie \
                 header (e.g. auth=Fe26.2**…). Find it in your browser's DevTools → \
                 Network → any opencode.ai request → Cookie header.",
                theme,
            ));
        if let Some(error) = self.opencode_cookie_error.clone() {
            card = card.child(
                div()
                    .id("provider-opencode-cookie-error")
                    .debug_selector(|| "provider-opencode-cookie-error".to_string())
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_XS))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.danger)
                    .child(text!(error)),
            );
        }
        card = card.child(controls::separator(theme));

        let override_text = self.opencode_workspace_id_override.clone();
        let override_is_empty = override_text.is_empty();
        let override_field = Self::opencode_text_field(
            "provider-opencode-workspace-override",
            override_text.clone(),
            "Workspace ID override",
            override_is_empty,
            self.opencode_override_focus.clone(),
            self.opencode_override_focus.is_focused(window),
            self.field_caret_visible,
            theme,
            |this, window, cx| this.opencode_override_focus.focus(window, cx),
            |this, event, window, cx| this.on_opencode_override_key(event, window, cx),
            entity.clone(),
        );
        let clear_override_entity = entity;
        let clear_override_handler = if override_is_empty {
            None
        } else {
            Some(move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                clear_override_entity.update(cx, |this, cx| this.clear_opencode_override(cx));
            })
        };
        card.child(
            div()
                .w_full()
                .px(px(BezelTheme::SPACE_MD))
                .py(px(BezelTheme::SPACE_SM))
                .flex()
                .items_center()
                .justify_between()
                .child(override_field)
                .child(controls::button_maybe(
                    "clear-opencode-workspace-override",
                    "Clear",
                    theme,
                    clear_override_handler,
                )),
        )
        .child(Self::opencode_caption(
            "provider-opencode-workspace-caption",
            "Find this in the URL after logging into opencode.ai \
             (e.g. opencode.ai/workspace/wrk_…/go).",
            theme,
        ))
        .child(controls::separator(theme))
    }

    /// The Ollama Cloud card's cookie rows (F-SET-13): the OpenCode Go
    /// section's masked-field/Save/Clear shape, minus the workspace
    /// override the Ollama fetch has no equivalent of.
    fn ollama_cookie_section(
        &self,
        mut card: gpui::Div,
        entity: Entity<Self>,
        theme: Theme,
        window: &Window,
    ) -> gpui::Div {
        let cookie_text = self.ollama_cookie_input.clone();
        let cookie_is_empty = cookie_text.is_empty();
        // Mask dots only — the real value is never drawn, matching the
        // macOS SecureField.
        let masked = "•".repeat(cookie_text.chars().count());
        let cookie_field = Self::opencode_text_field(
            "provider-ollama-cookie-field",
            masked,
            "Session cookie",
            cookie_is_empty,
            self.ollama_cookie_focus.clone(),
            self.ollama_cookie_focus.is_focused(window),
            self.field_caret_visible,
            theme,
            |this, window, cx| this.ollama_cookie_focus.focus(window, cx),
            |this, event, window, cx| this.on_ollama_cookie_key(event, window, cx),
            entity.clone(),
        );

        // Save is inert while the trimmed input is empty — the muted
        // "not wired" rendering, and the macOS Save's disabled condition.
        let save_entity = entity.clone();
        let save_handler = if cookie_text.trim().is_empty() {
            None
        } else {
            Some(move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                save_entity.update(cx, |this, cx| this.save_ollama_cookie(cx));
            })
        };
        let clear_cookie_entity = entity;

        card = card
            .child(
                div()
                    .w_full()
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_SM))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(cookie_field)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(controls::button_maybe(
                                "save-ollama-cookie",
                                "Save",
                                theme,
                                save_handler,
                            ))
                            .child(controls::button(
                                "clear-ollama-cookie",
                                "Clear",
                                theme,
                                move |_, _, cx| {
                                    clear_cookie_entity
                                        .update(cx, |this, cx| this.clear_ollama_cookie(cx));
                                },
                            )),
                    ),
            )
            .child(Self::opencode_caption(
                "provider-ollama-cookie-caption",
                "Paste the raw token value or the full cookie header from \
                 ollama.com. Find it in your browser's DevTools → Network → \
                 any ollama.com request → Cookie header.",
                theme,
            ));
        if let Some(error) = self.ollama_cookie_error.clone() {
            card = card.child(
                div()
                    .id("provider-ollama-cookie-error")
                    .debug_selector(|| "provider-ollama-cookie-error".to_string())
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_XS))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.danger)
                    .child(text!(error)),
            );
        }
        card.child(controls::separator(theme))
    }

    fn render_ai_providers(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        window: &Window,
    ) -> gpui::Div {
        let accounts = self.provider_accounts.clone();
        let claude_mark = Icon::ClaudeCode
            .agent_mark_color(theme.text)
            .unwrap_or_else(|| theme.text);
        let monochrome_mark = theme.text;
        let cards = [
            ProviderCardView::new(
                ProviderKind::Claude,
                "Claude Code",
                claude_mark,
                accounts.claude,
            ),
            ProviderCardView::new(
                ProviderKind::Codex,
                "Codex",
                monochrome_mark,
                accounts.codex,
            ),
            ProviderCardView::new(
                ProviderKind::OpenCodeGo,
                "OpenCode Go",
                monochrome_mark,
                accounts.opencode_go,
            ),
            // F-SET-13: the fourth reference card. No Ollama brand mark
            // exists in the vendored catalog — the globe is a declared
            // stand-in, not a silent leftover.
            ProviderCardView::new(
                ProviderKind::OllamaCloud,
                "Ollama Cloud",
                theme.text_muted,
                accounts.ollama_cloud,
            ),
        ];
        let mut page = div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING));
        for card in cards {
            page = page.child(settings_section(
                card.title,
                self.render_provider_card(card, entity.clone(), theme, window),
                theme,
            ));
        }
        page
    }

    /// Resolves one adapter row's source from the last applied sweep.
    /// Rows without a known source resolve honestly to NotInRegistry.
    fn launch_source_for_row(&self, id: &str) -> sirio_registry::LaunchSource {
        self.launch_sources
            .iter()
            .find(|(agent_id, _)| agent_id == id)
            .map(|(_, source)| source.clone())
            .unwrap_or_else(|| {
                sirio_registry::LaunchSource::Unavailable(
                    sirio_registry::UnavailableReason::NotInRegistry,
                )
            })
    }

    /// The summarizer agent trigger (F-SET-05): a button showing the
    /// current choice. The menu itself is rendered at the surface level
    /// (see [`Settings::render_summarizer_menu`]) — a card's `overflow_hidden`
    /// would clip its hitboxes, making the options unclickable. The menu is
    /// disabled — drawn muted, unopenable — while auto-naming is off, per
    /// the row's VERIFY clause; the choice itself persists through the
    /// snapshot contract like every other control here.
    fn render_summarizer_trigger(&self, theme: Theme, entity: Entity<Self>) -> impl IntoElement {
        let enabled = self.summarizer_picker_enabled();
        let selected = self.summarizer_agent;
        let toggle_entity = entity.clone();
        div()
            .id("general-summarizer")
            .debug_selector(|| "general-summarizer".into())
            .h(px(28.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(theme.radii.control)
            .text_size(theme.typography.callout)
            .text_color(if enabled {
                theme.text
            } else {
                theme.text_faint
            })
            .bg(theme.surface_raised)
            .when(enabled, |this| {
                this.hover(|style| style.bg(theme.element_hover))
            })
            .on_click(move |_, window, cx| {
                toggle_entity.update(cx, |this, cx| this.toggle_summarizer_picker(window, cx));
            })
            .child(text!(selected.title()))
            .child(
                IconElement::new(Icon::ChevronDown, IconSize::XSmall).text_color(if enabled {
                    theme.text
                } else {
                    theme.border
                }),
            )
    }

    /// The summarizer picker menu, rendered at the settings surface root —
    /// outside any card, so no `overflow_hidden` clips its hitboxes. It
    /// floats bottom-right of the surface, the same anchoring the chat
    /// model picker uses (a real anchor under the trigger needs layout
    /// information GPUI does not expose at render time).
    fn render_summarizer_menu(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.summarizer_agent;
        let mut menu = div()
            .id("summarizer-popover")
            .debug_selector(|| "summarizer-popover".into())
            .key_context("SettingsSummarizerPopover")
            .track_focus(&self.summarizer_focus)
            .on_action(cx.listener(|this, _: &CloseSummarizerPicker, _, cx| {
                this.close_summarizer_picker(cx);
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_summarizer_picker(cx)))
            .absolute()
            .right(px(24.0))
            .bottom(px(24.0))
            .w(px(200.0))
            .p(px(4.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised)
            .shadow_lg();
        for choice in SummarizerChoice::ALL {
            let is_selected = choice == selected;
            let choice_entity = entity.clone();
            menu = menu.child(
                div()
                    .id(format!("summarizer-option-{}", choice.id()))
                    .debug_selector(move || format!("summarizer-option-{}", choice.id()))
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.callout)
                    .text_color(if is_selected { theme.text } else { theme.text })
                    .when(is_selected, |this| this.bg(theme.element_active))
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, cx| {
                        choice_entity.update(cx, |this, cx| {
                            this.set_summarizer_agent(choice, cx);
                        });
                    })
                    .child(text!(choice.title()))
                    .when(is_selected, |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(text!("✓")),
                        )
                    }),
            );
        }
        menu
    }

    fn update_channel(&self) -> &str {
        let state_channel = self.update_state.channel.trim();
        if state_channel.is_empty() {
            self.channel.trim()
        } else {
            state_channel
        }
    }

    fn update_status_text(&self) -> Option<String> {
        if !self.update_state.enabled {
            return Some("Updates are off".into());
        }
        match &self.update_state.status {
            UpdateStatus::Disabled => Some("Updates are off".into()),
            UpdateStatus::NotDue => self
                .update_state
                .last_checked
                .as_ref()
                .map(|stamp| format!("Checked {stamp}")),
            UpdateStatus::Checking => Some("Checking for updates…".into()),
            UpdateStatus::UpToDate => Some(
                self.update_state
                    .last_checked
                    .as_ref()
                    .map_or_else(|| "Up to date".into(), |stamp| format!("Checked {stamp}")),
            ),
            UpdateStatus::Available { version, .. } => {
                Some(self.update_state.last_checked.as_ref().map_or_else(
                    || format!("Update available: {version}"),
                    |stamp| format!("Checked {stamp} · update available: {version}"),
                ))
            }
            UpdateStatus::Ready { version, .. } => {
                Some(self.update_state.last_checked.as_ref().map_or_else(
                    || format!("Update ready: {version}"),
                    |stamp| format!("Checked {stamp} · update ready: {version}"),
                ))
            }
            UpdateStatus::Failed { message } => {
                let mut text = "Could not check for updates".to_string();
                if let Some(stamp) = &self.update_state.last_checked {
                    text.push_str(" · ");
                    text.push_str(stamp);
                }
                if !message.trim().is_empty() {
                    text.push_str(": ");
                    text.push_str(message);
                }
                Some(text)
            }
        }
    }

    fn update_action_label() -> &'static str {
        if cfg!(target_os = "windows") {
            "Update and restart Sirio"
        } else {
            "Update and keep working"
        }
    }

    fn render_general(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let resume_entity = entity.clone();
        let auto_entity = entity.clone();
        let history_entity = entity.clone();
        let mounted_entity = entity.clone();
        let socket_entity = entity.clone();
        let updates_entity = entity.clone();
        let resume = controls::toggle(
            "general-resume-sessions",
            self.resume_agent_sessions,
            theme,
            move |_, _, cx| {
                resume_entity.update(cx, |this, cx| {
                    this.set_resume_agent_sessions(!this.resume_agent_sessions, cx);
                });
            },
        );
        let auto = controls::toggle(
            "general-auto-naming",
            self.auto_naming,
            theme,
            move |_, _, cx| {
                auto_entity.update(cx, |this, cx| this.set_auto_naming(!this.auto_naming, cx));
            },
        );
        let history = controls::toggle(
            "general-chat-history",
            self.limit_chat_history,
            theme,
            move |_, _, cx| {
                history_entity.update(cx, |this, cx| {
                    this.set_limit_chat_history(!this.limit_chat_history, cx);
                });
            },
        );
        let mounted = controls::toggle(
            "general-mounted-worktrees",
            self.limit_mounted_worktrees,
            theme,
            move |_, _, cx| {
                mounted_entity.update(cx, |this, cx| {
                    this.set_limit_mounted_worktrees(!this.limit_mounted_worktrees, cx);
                });
            },
        );
        let socket = controls::toggle(
            "general-control-socket",
            self.control_socket_enabled,
            theme,
            move |_, _, cx| {
                socket_entity.update(cx, |this, cx| {
                    this.set_control_socket_enabled(!this.control_socket_enabled, cx);
                });
            },
        );
        let updates_enabled = controls::toggle(
            "general-updates-enabled",
            self.update_state.enabled,
            theme,
            move |_, _, cx| {
                updates_entity.update(cx, |this, cx| {
                    this.set_updates_enabled(!this.update_state.enabled, cx);
                });
            },
        );
        // The About card states the version and the last check result. The
        // updater itself is host-owned; this surface only displays its facts.
        let mut about = controls::card(theme)
            .child(controls::row(
                "Version",
                None,
                div()
                    .debug_selector(|| "settings-version".into())
                    .text_size(theme.typography.callout)
                    .text_color(theme.text_muted)
                    .child(text!(self.version.clone())),
                theme,
            ))
            .child(controls::row(
                "Channel",
                None,
                div()
                    .debug_selector(|| "settings-channel".into())
                    .text_size(theme.typography.callout)
                    .text_color(theme.text_muted)
                    .child(text!(self.channel.clone())),
                theme,
            ));
        if let Some(status) = self.update_status_text() {
            about = about.child(controls::row(
                "Update status",
                None,
                div()
                    .debug_selector(|| "settings-update-last-checked".into())
                    .text_size(theme.typography.callout)
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .debug_selector(|| "settings-update-status".into())
                            .child(text!(status)),
                    ),
                theme,
            ));
        }

        let agents = controls::card(theme).child(controls::row(
            "Resume agent sessions on launch",
            Some(
                "Relaunch supported agents with their previous conversation after Sirio restarts."
                    .into(),
            ),
            resume,
            theme,
        ));
        let automation = controls::card(theme)
            .child(controls::row(
                "Auto-rename tabs and agents",
                Some(
                    "Names each session after your first prompt, then shortens it with the selected agent."
                        .into(),
                ),
                auto,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row(
                "Summarizer agent",
                Some("Falls back to the session's own agent when it fails.".into()),
                self.render_summarizer_trigger(theme, entity.clone()),
                theme,
            ));
        let history_card = controls::card(theme)
            .child(controls::row(
                "Limit stored chats",
                Some("Keeps only the most recent conversations per worktree.".into()),
                history,
                theme,
            ))
            .child(controls::separator(theme));
        let history_stepper_entity = entity.clone();
        let history_stepper = controls::stepper(
            "general-chat-retention",
            self.chat_retention,
            "",
            theme,
            move |value, cx| {
                history_stepper_entity.update(cx, |this, cx| this.set_chat_retention(value, cx));
            },
        );
        let history_card = history_card.child(controls::row(
            "Keep chats per worktree",
            None,
            history_stepper,
            theme,
        ));
        let mounted_stepper_entity = entity.clone();
        let mounted_stepper = controls::stepper(
            "general-mounted-count",
            self.mounted_worktrees,
            "",
            theme,
            move |value, cx| {
                mounted_stepper_entity.update(cx, |this, cx| this.set_mounted_worktrees(value, cx));
            },
        );
        let performance = controls::card(theme)
            .child(controls::row(
                "Limit mounted worktrees",
                Some("Frees terminal RAM by unmounting idle worktrees beyond this count.".into()),
                mounted,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row("Keep mounted", None, mounted_stepper, theme));
        // The socket row must display the *resolved* path (the one the live
        // socket listens on), not a template — a user needs to find the
        // socket to talk to it. The host routes it through the snapshot.
        #[cfg(windows)]
        let socket_kind = "Named pipe";
        #[cfg(not(windows))]
        let socket_kind = "Socket path";
        let socket_label = div()
            .flex()
            .flex_col()
            .justify_center()
            .flex_1()
            .text_size(theme.typography.headline)
            .text_color(theme.text)
            .child(text!(
                id = "settings-control-socket-title",
                "Control socket"
            ))
            .child(
                div()
                    .debug_selector(|| "settings-control-socket-path".into())
                    .mt(px(2.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(text!(format!("{socket_kind}: {}", self.socket_path))),
            );
        // The sirioctl card shows the bundled binary's name. "Copy install
        // command" is not offered: no install mechanism exists on this
        // platform (F-CTRL-CLI-02 is its own absent row), so there is no
        // real command to copy — a button that copied a made-up one would
        // lie (P58, F-SET-08).
        let control = controls::card(theme)
            .child(
                controls::row_view(socket_label, socket, theme)
                    .id("settings-control-socket-row")
                    .debug_selector(|| "settings-control-socket-row".into()),
            )
            .child(controls::separator(theme))
            .child(controls::row(
                "Bundled binary",
                None,
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(text!("sirioctl")),
                theme,
            ));
        // F-SET-09: the provisioner (`agent_skill_install_command`) is
        // built and tested; this button's job is only to reach it and hand
        // the resulting command to whoever the host wires as
        // `on_install_skill` — running it (terminal or background) is the
        // host's call, not this crate's.
        let install_skill_entity = entity.clone();
        let install_skill_handler = self.on_install_skill.clone().map(|_| {
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                install_skill_entity.update(cx, |settings, cx| settings.install_skill_clicked(cx));
            }
        });
        let mut skill = controls::card(theme).child(controls::action_row(
            controls::button_maybe(
                "general-install-skill",
                "Install Skill",
                theme,
                install_skill_handler,
            ),
            theme,
        ));
        // F-SET-09: a visible confirmation that the click reached the host
        // and an install command was actually handed off — not just that
        // the button is wired, but that this click did something.
        if self.skill_install_launched {
            skill = skill.child(
                div()
                    .id("general-install-skill-status")
                    .debug_selector(|| "general-install-skill-status".into())
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_XS))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(text!("Installing… running in a new terminal tab.")),
            );
        }

        let apply_update_handler = self
            .on_apply_update
            .clone()
            .map(|callback| move |_: &gpui::ClickEvent, _: &mut Window, _: &mut App| callback());
        let mut updates = controls::card(theme).child(controls::row(
            "Automatic updates",
            Some("Check for and download updates in the background.".into()),
            updates_enabled,
            theme,
        ));
        if self.update_state.enabled {
            match &self.update_state.status {
                UpdateStatus::Available { version, notes }
                | UpdateStatus::Ready { version, notes } => {
                    let update_label =
                        if matches!(&self.update_state.status, UpdateStatus::Ready { .. }) {
                            "Ready to install"
                        } else {
                            "Available version"
                        };
                    updates = updates
                        .child(controls::separator(theme))
                        .child(controls::row(
                            update_label,
                            None,
                            div()
                                .debug_selector(|| "settings-update-version".into())
                                .text_size(theme.typography.callout)
                                .text_color(theme.text_muted)
                                .child(text!(version.clone())),
                            theme,
                        ));
                    if self.update_channel().eq_ignore_ascii_case("nightly") {
                        updates = updates.child(
                            div()
                                .id("settings-update-notes")
                                .debug_selector(|| "settings-update-notes".into())
                                .px(px(BezelTheme::SPACE_MD))
                                .py(px(BezelTheme::SPACE_SM))
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_muted)
                                .child(text!("Nightly builds track main")),
                        );
                    } else if !notes.trim().is_empty() {
                        updates = updates.child(
                            div()
                                .id("settings-update-notes")
                                .debug_selector(|| "settings-update-notes".into())
                                .px(px(BezelTheme::SPACE_MD))
                                .py(px(BezelTheme::SPACE_SM))
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_muted)
                                .child(text!(notes.clone())),
                        );
                    }
                    updates = updates.child(controls::action_row(
                        controls::button_maybe(
                            "general-apply-update",
                            Self::update_action_label(),
                            theme,
                            apply_update_handler,
                        ),
                        theme,
                    ));
                }
                UpdateStatus::Disabled
                | UpdateStatus::NotDue
                | UpdateStatus::Checking
                | UpdateStatus::UpToDate
                | UpdateStatus::Failed { .. } => {}
            }
        }

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("About", about, theme))
            .child(settings_section("Updates", updates, theme))
            .child(settings_section("Agents", agents, theme))
            .child(settings_section("Automation", automation, theme))
            .child(settings_section("Chat history", history_card, theme))
            .child(settings_section("Performance", performance, theme))
            .child(settings_section("sirioctl", control, theme))
            .child(settings_section("Agent Skill", skill, theme))
    }

    #[cfg(target_os = "macos")]
    fn render_permission_row(
        &self,
        glyph: &'static str,
        title: &'static str,
        description: &'static str,
        badge: PermissionBadge,
        action: &'static str,
        theme: Theme,
    ) -> impl IntoElement {
        let label = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().w(px(20.0)).text_color(theme.text_faint).child(text!(
                id = format!("settings-permission-glyph-{title}"),
                glyph
            )))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(text!(
                        id = format!("settings-permission-title-{title}"),
                        title
                    )),
            )
            .child(controls::badge(
                theme,
                badge.label,
                badge.background,
                badge.foreground,
            ));
        let description = div()
            .text_size(theme.typography.footnote)
            .text_color(theme.text_muted)
            .child(text!(
                id = format!("settings-permission-description-{title}"),
                description
            ));
        let label = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(label)
            .child(description);
        controls::row_view(
            label,
            controls::button(
                match title {
                    "Notifications" => "permission-notifications",
                    "Screen Recording" => "permission-screen-recording",
                    "Accessibility" => "permission-accessibility",
                    "Full Disk Access" => "permission-full-disk",
                    "Automation" => "permission-automation",
                    _ => "permission-local-network",
                },
                action,
                theme,
                |_, _, _| {},
            ),
            theme,
        )
        .id(format!("settings-permission-row-{title}"))
    }

    fn render_browser_grants(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let origins = self.browser_origins.iter().cloned().collect::<Vec<_>>();
        let revoke_all_entity = entity.clone();
        let revoke_all = div()
            .id("settings-revoke-all-browser-origins")
            .debug_selector(|| "settings-revoke-all-browser-origins".into())
            .px(px(BezelTheme::SPACE_MD))
            .py(px(BezelTheme::SPACE_XS))
            .rounded(theme.radii.control)
            .text_size(theme.typography.callout)
            .text_color(if origins.is_empty() {
                theme.text_faint
            } else {
                theme.text
            })
            .bg(theme.surface_raised)
            .child(text!("Revoke all"));
        let revoke_all = revoke_all.when(!origins.is_empty(), move |this| {
            this.hover(|style| style.bg(theme.element_hover))
                .on_click(move |_, _, cx| {
                    revoke_all_entity
                        .update(cx, |settings, cx| settings.revoke_all_browser_origins(cx));
                })
        });

        let mut card = controls::card(theme).child(controls::row(
            "Granted browser origins",
            Some("Origins allowed by the browser agent permission prompt.".into()),
            revoke_all,
            theme,
        ));
        if origins.is_empty() {
            return card.child(
                div()
                    .id("settings-browser-grants-empty")
                    .debug_selector(|| "settings-browser-grants-empty".into())
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_SM))
                    .flex()
                    .items_center()
                    .text_size(theme.typography.callout)
                    .text_color(theme.text_muted)
                    .child(text!("No browser origins have been granted.")),
            );
        }

        for (index, origin) in origins.into_iter().enumerate() {
            let origin_entity = entity.clone();
            if index > 0 {
                card = card.child(controls::separator(theme));
            }
            let display_origin = origin.clone();
            let revoke = div()
                .id(format!("settings-revoke-browser-origin-{index}"))
                .debug_selector(move || format!("settings-revoke-browser-origin-{index}"))
                .px(px(BezelTheme::SPACE_MD))
                .py(px(BezelTheme::SPACE_XS))
                .rounded(theme.radii.control)
                .text_size(theme.typography.callout)
                .text_color(theme.text)
                .bg(theme.surface_raised)
                .hover(|style| style.bg(theme.element_hover))
                .on_click(move |_, _, cx| {
                    origin_entity.update(cx, |settings, cx| {
                        settings.revoke_browser_origin(origin.clone(), cx)
                    });
                })
                .child(text!("Revoke"));
            card = card.child(
                div()
                    .id(format!("settings-browser-grant-{index}"))
                    .debug_selector(move || format!("settings-browser-grant-{index}"))
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(BezelTheme::SPACE_MD))
                    .py(px(BezelTheme::SPACE_SM))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.typography.callout)
                            .text_color(theme.text)
                            .child(text!(
                                id = ("settings-browser-origin", index),
                                display_origin
                            )),
                    )
                    .child(revoke),
            );
        }
        card
    }

    fn render_permissions(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        #[cfg(target_os = "macos")]
        let granted = theme.success;
        #[cfg(target_os = "macos")]
        let denied = theme.danger;
        #[cfg(target_os = "macos")]
        let neutral = theme.surface_raised;
        #[cfg(target_os = "macos")]
        let rows = controls::card(theme)
            .child(self.render_permission_row(
                "♧",
                "Notifications",
                "Alerts when agents finish or need input.",
                PermissionBadge {
                    label: "GRANTED",
                    background: granted,
                    foreground: theme.surface,
                },
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "◉",
                "Screen Recording",
                "Screenshot, visual automation, and UI inspection tools.",
                PermissionBadge {
                    label: "GRANTED",
                    background: granted,
                    foreground: theme.surface,
                },
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "♙",
                "Accessibility",
                "Keystroke injection, window control, and UI automation tools.",
                PermissionBadge {
                    label: "DENIED",
                    background: denied,
                    foreground: on_status_fill(&theme),
                },
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "▱",
                "Full Disk Access",
                "Recommended when projects or worktrees touch macOS-protected folders.",
                PermissionBadge {
                    label: "CHECK MANUALLY",
                    background: neutral,
                    foreground: theme.text_muted,
                },
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "⚙",
                "Automation",
                "Apple Events for scripts that control other local apps.",
                PermissionBadge {
                    label: "GRANTED",
                    background: granted,
                    foreground: theme.surface,
                },
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "◎",
                "Local Network",
                "Discovery and access for development servers on your network.",
                PermissionBadge {
                    label: "CHECK MANUALLY",
                    background: neutral,
                    foreground: theme.text_muted,
                },
                "Trigger Prompt",
                theme,
            ));

        #[allow(unused_mut)]
        let mut surface = div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING));
        #[cfg(target_os = "macos")]
        {
            surface = surface
                .child(
                controls::card(theme)
                    .child(controls::row(
                        "Terminal tools inherit Sirio's macOS privacy envelope.",
                        Some("Use these controls when a CLI or agent in a pane needs macOS privacy access. Sirio does not ask at startup.".into()),
                        controls::button("refresh-permissions", "Refresh", theme, |_, _, _| {}),
                        theme,
                    )),
                )
                .child(
                div()
                    .mt(px(8.0))
                    .child(settings_section("macOS Permissions", rows, theme)),
                );
        }
        surface.child(settings_section(
            "Browser origin grants",
            self.render_browser_grants(theme, entity),
            theme,
        ))
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let mode = Theme::get(cx).mode;
        let entity = cx.entity();
        // Every settings text field shares one blink: window focus is
        // unique, so at most one caret is ever visible. The computed bar
        // visibility is stashed for this frame's field builders.
        let field_focused = [
            &self.opencode_cookie_focus,
            &self.ollama_cookie_focus,
            &self.opencode_override_focus,
        ]
        .iter()
        .any(|focus| focus.is_focused(window));
        caret::schedule(
            &mut self.field_blink,
            field_focused,
            Self::flip_field_blink,
            cx,
        );
        self.field_caret_visible = field_focused && self.field_blink.visible();
        let category_sidebar = self.render_categories(theme, entity.clone());
        let detail = match self.category {
            SettingsCategory::AiProviders => {
                self.render_ai_providers(theme, entity.clone(), window)
            }
            SettingsCategory::Agents => self.render_agents(theme, entity.clone(), window, cx),
            SettingsCategory::General => self.render_general(theme, entity.clone()),
            SettingsCategory::Permissions => self.render_permissions(theme, entity.clone()),
            SettingsCategory::Appearance => self.render_appearance(theme, mode, entity.clone()),
        };

        let mut surface = div()
            .id("settings-surface")
            .track_focus(&self.surface_focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .child(self.render_header(theme))
            .child(div().h(px(1.0)).w_full().bg(theme.border))
            .child(
                // `min_h(0)` is what lets this row be shorter than what it
                // holds. A column flex item takes its content height as its
                // automatic minimum, so without this the row grew to the
                // full height of the settings page (measured: 1572px inside
                // a 600px window) and every viewport nested in it grew with
                // it — which is why no scroller here could ever have
                // anything to scroll. Same guard `changes-list` carries.
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .flex()
                    .child(category_sidebar)
                    .child(div().w(px(1.0)).h_full().bg(theme.border))
                    .child(
                        // A column, not a row. GPUI derives a scroller's
                        // `content_size` from its children's laid-out bounds,
                        // and a row stretches its child to the viewport's
                        // height — so the page was always exactly as tall as
                        // the viewport, `scroll_max` was zero, and the wheel
                        // moved nothing. Scrolling down a page means the page
                        // must be free to be taller than what shows it:
                        // `flex_col` puts the overflow on the scrolled axis,
                        // `items_center` keeps the 720px column centred the
                        // way `justify_center` used to, and `flex_none` stops
                        // the page being shrunk back to the viewport.
                        div()
                            .id("settings-detail-scroll")
                            .debug_selector(|| "settings-detail-scroll".into())
                            .flex_1()
                            .h_full()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .overflow_y_scroll()
                            .track_scroll(&self.detail_scroll)
                            .child(
                                div()
                                    .id("settings-detail-page")
                                    .debug_selector(|| "settings-detail-page".into())
                                    .flex_none()
                                    .child(detail),
                            ),
                    ),
            );
        // The summarizer menu floats above the whole surface — outside the
        // cards, so no `overflow_hidden` clips its hitboxes.
        surface = surface.when(self.summarizer_popover_open, |surface| {
            surface.child(self.render_summarizer_menu(theme, entity, cx))
        });
        // Take focus on the next frame when something requested it (the
        // host opening the surface, or the picker menu closing). Focus
        // cannot move during render; the deferred focus lands before the
        // next frame's input dispatch.
        if self.surface_focus_pending {
            self.surface_focus_pending = false;
            let focus = self.surface_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        surface
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_base_colour_segments_are_bezels_five_in_order() {
        assert_eq!(
            SEGMENTED_BASE_COLOR,
            &["Neutral", "Stone", "Zinc", "Gray", "Slate"]
        );
    }

    #[test]
    fn every_base_colour_maps_to_its_own_segment() {
        for (index, base) in sirio_theme::BaseColor::ALL.into_iter().enumerate() {
            assert_eq!(base_color_segment(base), index, "{base:?}");
        }
    }

    use super::*;
    use gpui::{Modifiers, VisualTestContext};
    use std::cell::{Cell, RefCell};

    /// F-SET-14: proves `descendant_pids` finds a grandchild that has
    /// detached into its own session/process group — the exact shape of
    /// the live failure (`x-terminal-emulator -e <login>` puts the login
    /// command in a *different* pgid from the launcher's own, confirmed
    /// live against this sandbox's real terminal emulator) that made a
    /// single `kill <launcher_pid>` leave the login command running.
    /// `setsid` (or Python's `os.setsid` on macOS, where the command is not
    /// installed) reproduces that detachment without depending on any
    /// terminal emulator being installed.
    ///
    /// Unix-only: the subject `descendant_pids` has no Windows arm (it
    /// walks `/proc`, which Windows lacks), and its partner
    /// `terminate_login_process_group` is a documented no-op there — the
    /// comment above it names the Toolhelp32/Job-Object pairing as the
    /// intended counterpart. Gating here suppresses coverage of that
    /// admitted gap, not of a portable behaviour; the Toolhelp32 walk
    /// should land with this test's fixture ported to a Windows
    /// equivalent.
    #[cfg(unix)]
    #[test]
    fn descendant_pids_finds_a_child_detached_into_its_own_session() {
        // `sh` is the launcher (kept as one live process, same pid the
        // whole time — the same shape `x-terminal-emulator` has, confirmed
        // live against this sandbox's real terminal emulator). Its
        // backgrounded detached child creates a brand new session/process
        // group, the exact detachment that made a single `kill <launcher_pid>`
        // leave the real login command running.
        let detached_command = if cfg!(target_os = "macos") {
            "python3 -c 'import os,time; child=os.fork(); os.setsid() if child == 0 else os.waitpid(child,0); time.sleep(60)'"
        } else {
            "setsid sleep 60"
        };
        let mut launcher = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{detached_command} & wait"))
            .spawn()
            .expect("spawn launcher");
        let launcher_pid = launcher.id();

        // Give the grandchild time to actually fork before walking /proc.
        let mut found = Vec::new();
        for _ in 0..50 {
            found = descendant_pids(launcher_pid);
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !found.is_empty(),
            "expected at least one descendant of the launcher (the `sleep` grandchild)"
        );

        terminate_login_process_group(launcher_pid);
        // Reap the launcher: SIGTERM already ended it, but as its real
        // parent this process, not `kill -0`, is the one that decides
        // whether its pid stays occupied as a zombie — reap it before
        // checking liveness so the check reflects "terminated", not
        // "terminated but not yet reaped".
        let _ = launcher.wait();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let all_gone = found.iter().all(|pid| {
                std::process::Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .status()
                    .map(|status| !status.success())
                    .unwrap_or(true)
            });
            if all_gone {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for pid in found {
            let status = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .status();
            assert!(
                status.map(|status| !status.success()).unwrap_or(true),
                "pid {pid} should have been terminated"
            );
        }
    }

    #[test]
    fn settings_snapshot_defaults_match_the_persisted_contract() {
        let snapshot = SettingsSnapshot::default();

        assert_eq!(snapshot.theme, ThemeMode::System);
        assert_eq!(snapshot.interface_font_size, 13);
        assert_eq!(snapshot.terminal_font_size, 13);
        assert!(snapshot.control_socket_enabled);
        assert!(
            snapshot.socket_path.is_empty(),
            "the socket path is runtime state routed by the host, never a baked-in template"
        );

        // P58: every control that holds a user-changeable value is part of
        // the persistence contract — none of the General-screen values may
        // be report-only. The defaults are exactly what the surface draws
        // on a first launch, so the contract's defaults never change what
        // the user sees.
        assert!(snapshot.resume_agent_sessions);
        assert!(!snapshot.auto_naming);
        assert!(snapshot.limit_chat_history);
        assert_eq!(snapshot.chat_retention, 100);
        assert!(!snapshot.limit_mounted_worktrees);
        assert_eq!(snapshot.mounted_worktrees, 6);
        assert_eq!(snapshot.summarizer_agent, SummarizerChoice::Claude);
        assert!(snapshot.claude_show_in_bar);
        assert!(snapshot.codex_show_in_bar);
        assert!(!snapshot.opencode_show_in_bar);
        assert_eq!(snapshot.refresh_interval, 5);
    }

    #[test]
    fn summarizer_choice_ids_and_titles_are_stable_and_parse_round_trips() {
        // The persisted value is the agent id; the picker displays the
        // title. Every offered choice must parse back to itself, and an
        // unknown stored value must fall back rather than guess.
        assert_eq!(SummarizerChoice::ALL.len(), 5);
        for choice in SummarizerChoice::ALL {
            assert_eq!(SummarizerChoice::parse(choice.id()), Some(choice));
            assert!(!choice.title().is_empty());
        }
        assert_eq!(SummarizerChoice::parse("banana"), None);
        assert_eq!(
            SummarizerChoice::parse("claude"),
            Some(SummarizerChoice::Claude)
        );
        assert_eq!(SummarizerChoice::Claude.title(), "Claude Code");
        assert_eq!(SummarizerChoice::OhMyPi.id(), "omp");
    }

    #[test]
    fn account_identity_cache_saves_a_fresh_identity_and_falls_back_when_absent() {
        // F-PERSIST-DB-06: discover_claude_identity/discover_codex_identity
        // had nowhere to persist a successful shell-out, so the display line
        // could not survive the live query failing later (offline, or the
        // CLI binary transiently unavailable). Drives sync_one_account_identity
        // directly against a real database file rather than through the live
        // shell-out, which this environment cannot make succeed or fail on
        // demand.
        let path = std::env::temp_dir().join(format!(
            "sirio-settings-account-identity-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let db = sirio_persistence::AppDatabase::open(&path).expect("open database");

        // A fresh, successfully discovered identity is cached for later.
        let mut signed_in_with_identity = ProviderAccountStatus {
            label: "Signed in",
            signed_in: true,
            identity: Some("dev@example.com".to_string()),
        };
        Settings::sync_one_account_identity("claude", &db, &mut signed_in_with_identity);
        assert_eq!(
            db.account_identity("claude")
                .expect("query claude identity")
                .map(|(identity, _)| identity),
            Some("dev@example.com".to_string())
        );

        // Locally signed in, but this live query came back empty (offline,
        // slow CLI, transient binary absence) — falls back to the cache
        // rather than showing no identity for a provider that is signed in.
        let mut signed_in_without_identity = ProviderAccountStatus {
            label: "Signed in",
            signed_in: true,
            identity: None,
        };
        Settings::sync_one_account_identity("claude", &db, &mut signed_in_without_identity);
        assert_eq!(
            signed_in_without_identity.identity,
            Some("dev@example.com".to_string()),
            "falls back to the cached identity when the live shell-out fails"
        );

        // Never signed in: no identity is fabricated, cached or otherwise.
        let mut not_signed_in = ProviderAccountStatus {
            label: "Not signed in",
            signed_in: false,
            identity: None,
        };
        Settings::sync_one_account_identity("codex", &db, &mut not_signed_in);
        assert_eq!(not_signed_in.identity, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn provider_rows_report_discovery_not_literals() {
        // The row content must come from what discovery found, never from a
        // fixed table that claims every provider is available. The name and
        // status follow the availability value; an absent binary is
        // described in a way the user can act on.
        let available = AgentAvailability {
            id: "claude",
            display_name: "A Different Name",
            executable: Some(PathBuf::from("/opt/local/bin/claude")),
        };
        let absent = AgentAvailability {
            id: "opencode",
            display_name: "OpenCode",
            executable: None,
        };

        let row = provider_row(&available, None);
        assert_eq!(
            row.name, "A Different Name",
            "the name is discovery data, not a hard-coded display table"
        );
        assert_eq!(
            row.status,
            ProviderStatus::Installed(PathBuf::from("/opt/local/bin/claude")),
            "an installed CLI reports the resolved executable"
        );
        assert!(
            row.description.contains("claude") && row.description.contains("PATH"),
            "an installed CLI keeps the on-PATH description"
        );

        let row = provider_row(&absent, None);
        assert_eq!(
            row.status,
            ProviderStatus::NotInstalled,
            "a binary absent from PATH is reported as not installed"
        );
        assert!(
            row.description.contains("install"),
            "an absent binary is described in a way the user can act on"
        );
        assert!(
            !row.description.contains("Available"),
            "nothing may claim an absent binary is available"
        );
    }

    #[test]
    fn permissions_category_is_offered_for_browser_grants() {
        // Browser origin grants are durable on every platform, while the
        // macOS-only TCC rows remain conditionally rendered inside it.
        let offered: Vec<SettingsCategory> = SettingsCategory::ALL.to_vec();
        assert!(
            offered.contains(&SettingsCategory::Permissions),
            "every platform offers the browser-origin permissions screen"
        );
    }

    #[gpui::test]
    async fn opening_the_agents_screen_asks_the_host_to_recheck_launch_sources(
        cx: &mut gpui::TestAppContext,
    ) {
        use sirio_registry::LaunchSource;
        cx.update(Theme::init);
        let sources = vec![(
            "codex".to_string(),
            LaunchSource::Builtin {
                program: "codex-acp".into(),
                args: vec![],
            },
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_launch_sources(sources)
        });
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorder = events.clone();
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let settings = cx.update(|window, app| {
            let settings_entity = window.root::<Settings>().flatten().expect("settings root");
            let subscription = app.subscribe(
                &settings_entity,
                move |_entity, event: &SettingsEvent, _app| {
                    if let SettingsEvent::RefreshAgentSources = event {
                        recorder.borrow_mut().push("recheck".to_string());
                    }
                },
            );
            // The subscription must outlive this update scope for the whole
            // test; forgetting it pins it to the entities' lifetimes.
            std::mem::forget(subscription);
            settings_entity
        });

        settings.update(&mut cx, |settings, cx| {
            settings.select_category(SettingsCategory::Agents, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            ["recheck".to_string()],
            "entering the Agents screen asks the host to recheck the registry"
        );
    }

    #[gpui::test]
    async fn the_refresh_button_asks_the_host_to_recheck_launch_sources(
        cx: &mut gpui::TestAppContext,
    ) {
        use sirio_registry::LaunchSource;
        cx.update(Theme::init);
        let sources = vec![(
            "codex".to_string(),
            LaunchSource::Builtin {
                program: "codex-acp".into(),
                args: vec![],
            },
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_launch_sources(sources)
        });
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorder = events.clone();
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let settings = cx.update(|window, app| {
            let settings_entity = window.root::<Settings>().flatten().expect("settings root");
            let subscription = app.subscribe(
                &settings_entity,
                move |_entity, event: &SettingsEvent, _app| {
                    if let SettingsEvent::RefreshAgentSources = event {
                        recorder.borrow_mut().push("recheck".to_string());
                    }
                },
            );
            // The subscription must outlive this update scope for the whole
            // test; forgetting it pins it to the entities' lifetimes.
            std::mem::forget(subscription);
            settings_entity
        });

        settings.update(&mut cx, |settings, cx| {
            settings.select_category(SettingsCategory::Agents, cx);
        });
        cx.run_until_parked();
        let opened = events.borrow().len();
        assert_eq!(opened, 1, "entering Agents emits exactly one recheck");

        let refresh = cx
            .debug_bounds("refresh-agents")
            .expect("the Refresh button is drawn");
        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().len(),
            2,
            "pressing ↻ Refresh asks the host to recheck again"
        );
    }

    /// F-SET-16: the Agents screen's "Refresh" button re-runs agent
    /// discovery, the same re-read-from-disk meaning "Refresh now" already
    /// has on the AI Providers screen — a pinned fixture is replaced by a
    /// fresh read.
    #[gpui::test]
    async fn refresh_agents_re_runs_agent_discovery(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let pinned = vec![AgentAvailability {
            id: "not-a-real-agent",
            display_name: "Not A Real Agent",
            executable: None,
        }];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(pinned)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        let refresh = cx
            .debug_bounds("refresh-agents")
            .expect("Refresh renders on the Agents screen");
        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.run_until_parked();

        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_availability
                .clone()
        });
        assert_eq!(
            rendered,
            try_discover_availability().expect("this machine's PATH discovers cleanly"),
            "Refresh replaces the pinned fixture with a fresh discovery read"
        );
    }

    /// F-SET-16: Refresh was already genuinely wired (the row above pins
    /// that); what a byte-identical before/after capture read as "absent"
    /// was the other conjunct — a rendered last-refreshed stamp. Clears the
    /// stamp construction itself set, so the assertion below proves the
    /// button populates it rather than merely inheriting it from startup.
    #[gpui::test]
    async fn refresh_agents_renders_a_last_refreshed_timestamp(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |this, cx| {
                this.agent_last_refreshed = None;
                cx.notify();
            });
        });

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("agents-last-refreshed").is_none(),
            "no timestamp renders before the first refresh"
        );

        let refresh = cx
            .debug_bounds("refresh-agents")
            .expect("Refresh renders on the Agents screen");
        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("agents-last-refreshed").is_some(),
            "Refresh renders the last-refreshed stamp the byte-identical capture never found"
        );
        let stamp = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .agent_last_refreshed
                .clone()
        });
        assert!(
            stamp.is_some(),
            "the field backing the render is populated by the click"
        );
    }

    /// #197: the version in an agent row belongs to the **ACP server
    /// package**, never to the CLI whose path sits beside it. Rendered bare
    /// as `v0.70.0`, in the same colour and size as that path and six pixels
    /// from it, it read as the binary's own version -- and was wrong every
    /// time. On the machine this was found, the row said `v0.70.0` next to a
    /// `claude.exe` reporting `2.1.247`.
    ///
    /// The fixture makes the two disagree on purpose, which is what stops
    /// this from being a restatement of the match arms: the executable is a
    /// claude binary and the launch source is a package at an unrelated
    /// version, and the row must report the package's.
    #[test]
    fn the_row_version_describes_the_acp_package_not_the_cli() {
        use sirio_registry::{Distribution, LaunchSource, RegistryAgent};

        let availability = AgentAvailability {
            id: "claude",
            display_name: "Claude Code",
            executable: Some(PathBuf::from("/home/u/.local/bin/claude")),
        };
        let source = LaunchSource::Installable {
            agent: RegistryAgent {
                id: "claude-code-acp".into(),
                name: "Claude Code ACP".into(),
                version: "0.70.0".into(),
                description: None,
                repository: None,
                website: None,
                license: None,
                icon: None,
                distributions: vec![Distribution::Binary(Default::default())],
            },
        };

        let row = provider_row(&availability, Some(&source));
        assert_eq!(
            row.version.as_deref(),
            Some("0.70.0"),
            "the version tracks the ACP launch source, not the binary --              which is exactly why the rendered string has to name its subject"
        );
        assert_eq!(
            row.status,
            ProviderStatus::Installed(PathBuf::from("/home/u/.local/bin/claude")),
            "the path in the same row is the CLI's, so the two sit together              describing different artifacts"
        );
    }

    /// A `Builtin` source carries no package, so the column simply vanishes
    /// -- OpenCode was the tell that the number never belonged to the
    /// binary, since it showed a path and no version at all (#197).
    #[test]
    fn a_builtin_row_has_no_acp_version_to_show() {
        use sirio_registry::LaunchSource;

        let availability = AgentAvailability {
            id: "opencode",
            display_name: "OpenCode",
            executable: Some(PathBuf::from("/home/u/.opencode/bin/opencode")),
        };
        let source = LaunchSource::Builtin {
            program: "opencode".into(),
            args: vec!["acp".into()],
        };

        assert_eq!(provider_row(&availability, Some(&source)).version, None);
    }

    #[test]
    fn an_unverified_install_says_so_after_the_fact() {
        use sirio_registry::{InstalledAgent, Integrity, LaunchSource};

        let installed = InstalledAgent {
            id: "cursor".into(),
            version: "1.0.0".into(),
            executable: "/data/cursor".into(),
            args: vec![],
            integrity: Integrity::None,
        };
        assert_eq!(
            installed_integrity_note(&LaunchSource::Installed(installed)),
            Some("no published checksum"),
            "9 of 18 binary agents publish at least one unhashed artifact; that stays visible"
        );
    }

    #[gpui::test]
    async fn control_socket_row_shows_resolved_path_and_toggles(cx: &mut gpui::TestAppContext) {
        // The General screen must display the *resolved* path (the one the
        // live socket listens on), and the row must reflect both the
        // enabled and disabled states.
        cx.update(Theme::init);
        let snapshot = SettingsSnapshot {
            socket_path: "/run/user/1000/Sirio/control.sock".into(),
            ..Default::default()
        };
        let window = cx.add_window(|_window, cx| Settings::with_snapshot(cx, snapshot));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-control-socket-row").is_some(),
            "the Control socket row renders"
        );
        assert!(
            cx.debug_bounds("settings-control-socket-path").is_some(),
            "the resolved path line renders"
        );

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(snapshot.socket_path, "/run/user/1000/Sirio/control.sock");
        assert!(snapshot.control_socket_enabled);

        let toggle = cx
            .debug_bounds("general-control-socket")
            .expect("socket toggle renders");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert!(
            !snapshot.control_socket_enabled,
            "the row reflects the disabled state"
        );
        assert_eq!(
            snapshot.socket_path, "/run/user/1000/Sirio/control.sock",
            "the path stays resolved in both states"
        );
    }

    #[test]
    fn provider_account_status_is_derived_not_hardcoded() {
        // The card's status must come from the local account state, never
        // from a fixed table that claims every provider is "Active". The
        // word "Active" asserted a working account without anything having
        // checked it; the derived state is honest about what was read.
        let signed_in = ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn);
        assert_eq!(signed_in.label, "Signed in");
        assert!(signed_in.signed_in);

        let signed_out = ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut);
        assert_eq!(signed_out.label, "Not signed in");
        assert!(!signed_out.signed_in);

        let unknown = ProviderAccountStatus::from_account_state(LocalAccountState::NoLocalStore);
        assert_eq!(unknown.label, "Unknown");
        assert!(!unknown.signed_in);

        assert_ne!(
            signed_in.label, "Active",
            "no provider card may claim a working account it did not check"
        );
    }

    #[test]
    fn provider_account_identity_is_formatted_without_losing_provider_fields() {
        let identity = AgentAccountIdentity {
            logged_in: true,
            email: "user@example.com".into(),
            organization: Some("Acme".into()),
        };
        assert_eq!(
            format_account_identity(&identity),
            "user@example.com · Acme"
        );

        let identity_without_org = AgentAccountIdentity {
            logged_in: true,
            email: "user@example.com".into(),
            organization: None,
        };
        assert_eq!(
            format_account_identity(&identity_without_org),
            "user@example.com"
        );
    }

    #[test]
    fn provider_login_commands_match_the_installed_cli_contracts() {
        let cases = [
            (ProviderKind::Claude, "claude", vec!["auth", "login"]),
            (ProviderKind::Codex, "codex", vec!["login"]),
            (ProviderKind::OpenCodeGo, "opencode", vec!["auth", "login"]),
        ];

        for (provider, program, args) in cases {
            let command = provider_login_command(provider).expect("a CLI login exists");
            assert_eq!(command.program, program);
            assert_eq!(command.args, args);
        }

        // F-SET-13: Ollama Cloud is cookie-only — no CLI login flow
        // exists to delegate to, and its card renders no Add Account.
        assert_eq!(provider_login_command(ProviderKind::OllamaCloud), None);
    }

    /// Windows has no `x-terminal-emulator`: Add Account used to die with
    /// "could not start claude login: program not found" (the launcher was
    /// what was missing, not `claude`). The login now runs itself, in a
    /// console of its own, so the spawned child *is* the login command.
    #[cfg(windows)]
    #[test]
    fn login_launcher_runs_the_login_itself_on_windows() {
        let command = login_launcher("claude", &["auth", "login"]);
        assert_eq!(command.get_program(), "claude");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["auth", "login"]);
    }

    /// Unix keeps delegating to the desktop's terminal emulator, with the
    /// login command as its `-e` payload.
    #[cfg(not(windows))]
    #[test]
    fn login_launcher_delegates_to_the_terminal_emulator_on_unix() {
        let command = login_launcher("claude", &["auth", "login"]);
        assert_eq!(command.get_program(), "x-terminal-emulator");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["-e", "claude", "auth", "login"]);
    }

    /// The failure message blames the launcher only when a separate one
    /// failed; a login that is its own launcher is named once.
    #[test]
    fn login_start_failure_names_a_separate_launcher_only() {
        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "program not found");
        assert_eq!(
            login_start_failure("claude", "x-terminal-emulator", &error),
            "could not start claude login via x-terminal-emulator: program not found"
        );
        assert_eq!(
            login_start_failure("claude", "claude", &error),
            "could not start claude login: program not found"
        );
    }

    /// F-SET-14 on Windows: Cancel really ends the recorded login process.
    /// `ping` stands in for the login command as a process that would
    /// otherwise outlive the test by half a minute.
    #[cfg(windows)]
    #[test]
    fn terminate_login_process_group_ends_the_recorded_process_on_windows() {
        let mut child = std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn a long-lived stand-in for the login");
        terminate_login_process_group(child.id());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match child.try_wait().expect("poll the stand-in") {
                Some(status) => {
                    assert!(!status.success(), "terminated, not finished: {status}");
                    break;
                }
                None if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                None => {
                    let _ = child.kill();
                    panic!("the stand-in login was still running after terminate");
                }
            }
        }
    }

    fn cookie_test_states() -> ProviderAccountStates {
        ProviderAccountStates {
            claude: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
            codex: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
            opencode_go: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
            ollama_cloud: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
        }
    }

    /// F-SET-12: typing a cookie and clicking Save writes the credential
    /// store, clears the input, signs the provider in and turns its usage
    /// bar segment on through the persistence contract — the macOS Save
    /// button's exact side effects, driven through the real click/key path.
    #[gpui::test]
    async fn opencode_cookie_save_stores_signs_in_and_shows_in_bar(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir =
            std::env::temp_dir().join(format!("sirio-settings-cookie-save-{}", std::process::id()));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        // Empty input: Save is inert — clicking it must write nothing.
        let save = cx
            .debug_bounds("save-opencode-cookie")
            .expect("the Save button is drawn");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            CredentialStore::at(&store_path)
                .get(OpenCodeGoUsageFetcher::COOKIE_KEY)
                .is_none(),
            "an inert Save writes nothing"
        );

        // Focus the field the way a user does, type, save.
        let field = cx
            .debug_bounds("provider-opencode-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("Fe26demo");
        cx.run_until_parked();
        let save = cx
            .debug_bounds("save-opencode-cookie")
            .expect("Save stays drawn with text in the field");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            CredentialStore::at(&store_path).get(OpenCodeGoUsageFetcher::COOKIE_KEY),
            Some("Fe26demo".to_string()),
            "Save writes the typed cookie into the store"
        );
        assert!(
            cx.debug_bounds("provider-opencode-cookie-error").is_none(),
            "a successful Save shows no error"
        );
        let (input, signed_in, show_in_bar) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.opencode_cookie_input.clone(),
                settings.provider_accounts.opencode_go.signed_in,
                settings.opencode_show_in_bar,
            )
        });
        assert!(input.is_empty(), "a saved cookie clears the input");
        assert!(signed_in, "the account state flips to signed in");
        assert!(show_in_bar, "Save turns the usage bar segment on");
        let last = saved
            .borrow()
            .last()
            .cloned()
            .expect("Save routed a snapshot through on_change");
        assert!(
            last.opencode_show_in_bar,
            "the persisted snapshot carries the visibility for the status bar"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-12: a cookie is pasted, never typed — a `Fe26.2**…` token is
    /// an opaque blob copied out of the browser's DevTools. Ctrl-V into the
    /// focused field must insert the clipboard text (with the trailing
    /// newline a DevTools copy usually carries trimmed away), so the
    /// placeholder clears and Save has something to store. Regression:
    /// the raw-keystroke handler rejected every modified key, silently
    /// dropping the paste.
    #[gpui::test]
    async fn opencode_cookie_paste_feeds_the_save_flow(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-cookie-paste-{}",
            std::process::id()
        ));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("Fe26pasted\n".to_string()));
        });

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();
        let field = cx
            .debug_bounds("provider-opencode-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-v");
        cx.run_until_parked();

        let (input, signed_in) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.opencode_cookie_input.clone(),
                settings.provider_accounts.opencode_go.signed_in,
            )
        });
        assert_eq!(
            input, "Fe26pasted",
            "the paste lands in the field, trailing newline trimmed"
        );
        assert!(!signed_in, "pasting alone does not sign in — Save does");

        let save = cx
            .debug_bounds("save-opencode-cookie")
            .expect("Save is live once a cookie is pasted");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            CredentialStore::at(&store_path).get(OpenCodeGoUsageFetcher::COOKIE_KEY),
            Some("Fe26pasted".to_string()),
            "the pasted cookie reaches the credential store through Save"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-12: the workspace-ID override is pasted from the opencode.ai
    /// URL the same way. Ctrl-V must insert it — and because this field
    /// *is* the durable value, the paste must route through the
    /// persistence contract like every keystroke does.
    #[gpui::test]
    async fn opencode_workspace_override_paste_persists(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-override-paste-{}",
            std::process::id()
        ));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("wrkpasted".to_string()));
        });

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();
        let field = cx
            .debug_bounds("provider-opencode-workspace-override")
            .expect("the override field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-v");
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(
            snapshot.opencode_workspace_id_override, "wrkpasted",
            "the pasted workspace id reaches the persistence contract"
        );
        assert_eq!(
            saved
                .borrow()
                .last()
                .map(|s| s.opencode_workspace_id_override.clone()),
            Some("wrkpasted".to_string()),
            "the paste routed a snapshot through on_change"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-12: Clear deletes the stored cookie, signs the provider out
    /// and removes it from the usage bar — the macOS Clear button's side
    /// effects. The masked field never echoed the stored value, so there
    /// is nothing to blank.
    #[gpui::test]
    async fn opencode_cookie_clear_deletes_and_signs_out(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-cookie-clear-{}",
            std::process::id()
        ));
        let store_path = dir.join("credentials.json");
        let store = CredentialStore::at(&store_path);
        store
            .set(OpenCodeGoUsageFetcher::COOKIE_KEY, "auth-seeded")
            .expect("seed the stored cookie");

        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                let mut states = cookie_test_states();
                states.opencode_go =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn);
                let snapshot = SettingsSnapshot {
                    opencode_show_in_bar: true,
                    ..Default::default()
                };
                Settings::with_snapshot(cx, snapshot)
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(states)
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let clear = cx
            .debug_bounds("clear-opencode-cookie")
            .expect("the Clear button is drawn");
        cx.simulate_click(clear.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            store.get(OpenCodeGoUsageFetcher::COOKIE_KEY).is_none(),
            "Clear deletes the stored cookie"
        );
        let (signed_in, show_in_bar) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.provider_accounts.opencode_go.signed_in,
                settings.opencode_show_in_bar,
            )
        });
        assert!(!signed_in, "clearing the cookie signs the provider out");
        assert!(
            !show_in_bar,
            "Clear removes the provider from the usage bar"
        );
        let last = saved
            .borrow()
            .last()
            .cloned()
            .expect("Clear routed a snapshot through on_change");
        assert!(!last.opencode_show_in_bar);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-12's forbidden outcome, inverted into a test: a Save that
    /// cannot write the store must keep the typed value and show the
    /// failure — never silently drop the cookie.
    #[gpui::test]
    async fn opencode_cookie_save_failure_keeps_the_input_and_reports(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        // The store path's parent is a regular *file*, so creating the
        // store directory fails deterministically — no chmod games, no
        // root special-casing.
        let dir =
            std::env::temp_dir().join(format!("sirio-settings-cookie-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        std::fs::write(dir.join("blocker"), b"").expect("create blocking file");
        let store_path = dir.join("blocker").join("credentials.json");

        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let field = cx
            .debug_bounds("provider-opencode-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("Fe26demo");
        cx.run_until_parked();
        let save = cx
            .debug_bounds("save-opencode-cookie")
            .expect("the Save button is drawn");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("provider-opencode-cookie-error").is_some(),
            "a failed Save is visible"
        );
        let (input, signed_in, show_in_bar, error) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.opencode_cookie_input.clone(),
                settings.provider_accounts.opencode_go.signed_in,
                settings.opencode_show_in_bar,
                settings.opencode_cookie_error.clone(),
            )
        });
        assert_eq!(
            input, "Fe26demo",
            "a failed Save keeps the typed cookie — dropping it silently is the defect"
        );
        assert!(
            !signed_in,
            "a failed Save must not claim the provider signed in"
        );
        assert!(
            !show_in_bar,
            "a failed Save must not turn the bar segment on"
        );
        let error = error.expect("the failure message is held");
        assert!(
            error.starts_with("Failed to update the credential store —"),
            "the message names the store, not a Keychain this platform lacks: {error}"
        );
        assert!(
            saved.borrow().is_empty(),
            "a failed Save persists nothing through on_change"
        );
        assert!(
            !store_path.exists(),
            "no store file appears behind the failure"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-12: the workspace override is a durable settings value —
    /// every edit flows through the persistence contract, and Clear
    /// resets it to discovery.
    #[gpui::test]
    async fn opencode_workspace_override_edits_persist_and_clear(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir =
            std::env::temp_dir().join(format!("sirio-settings-override-{}", std::process::id()));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // Same reason as the Ollama cookie tests above: the override
        // field sits on the third card, below the fold at the default test
        // window height, so the click that should focus it lands outside
        // the detail column and the keystrokes go to no one. This test used
        // to clear the 1080px fold by a hair; the app-wide +1px type scale
        // grew the rows above it past that margin, and the detail column is
        // now a real viewport that clips rather than a surface that grew to
        // fit. A user reaches it by scrolling; the test grows the window.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let field = cx
            .debug_bounds("provider-opencode-workspace-override")
            .expect("the override field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("wrkdemo");
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(
            snapshot.opencode_workspace_id_override, "wrkdemo",
            "the keystrokes reached the persistence contract"
        );
        assert_eq!(
            saved
                .borrow()
                .last()
                .expect("edits routed through on_change")
                .opencode_workspace_id_override,
            "wrkdemo"
        );

        let clear = cx
            .debug_bounds("clear-opencode-workspace-override")
            .expect("the override Clear button is drawn");
        cx.simulate_click(clear.center(), Modifiers::none());
        cx.run_until_parked();
        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(
            snapshot.opencode_workspace_id_override, "",
            "Clear resets the override to discovery"
        );
        assert_eq!(
            saved
                .borrow()
                .last()
                .expect("Clear routed a snapshot through on_change")
                .opencode_workspace_id_override,
            ""
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-13: the Ollama Cloud Save is the macOS button's exact side
    /// effects — store write, cleared input, signed in, bar segment on —
    /// driven through the real click/key path like the OpenCode Go test.
    #[gpui::test]
    async fn ollama_cookie_save_stores_signs_in_and_shows_in_bar(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir =
            std::env::temp_dir().join(format!("sirio-settings-ollama-save-{}", std::process::id()));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // The Ollama Cloud card is the fourth: at the default test window
        // size its controls paint (debug_bounds sees them) but sit outside
        // the window's hit-test bounds, so clicks would land on nothing.
        // A user reaches them by scrolling; the test grows the window.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        // Empty input: Save is inert — clicking it must write nothing.
        let save = cx
            .debug_bounds("save-ollama-cookie")
            .expect("the Save button is drawn");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            CredentialStore::at(&store_path)
                .get(OllamaCloudUsageFetcher::COOKIE_KEY)
                .is_none(),
            "an inert Save writes nothing"
        );

        // Focus the field the way a user does, type, save.
        let field = cx
            .debug_bounds("provider-ollama-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("sessdemo");
        cx.run_until_parked();
        let save = cx
            .debug_bounds("save-ollama-cookie")
            .expect("Save stays drawn with text in the field");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            CredentialStore::at(&store_path).get(OllamaCloudUsageFetcher::COOKIE_KEY),
            Some("sessdemo".to_string()),
            "Save writes the typed cookie into the store"
        );
        assert!(
            cx.debug_bounds("provider-ollama-cookie-error").is_none(),
            "a successful Save shows no error"
        );
        let (input, signed_in, show_in_bar) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.ollama_cookie_input.clone(),
                settings.provider_accounts.ollama_cloud.signed_in,
                settings.ollama_show_in_bar,
            )
        });
        assert!(input.is_empty(), "a saved cookie clears the input");
        assert!(signed_in, "the account state flips to signed in");
        assert!(show_in_bar, "Save turns the usage bar segment on");
        let last = saved
            .borrow()
            .last()
            .cloned()
            .expect("Save routed a snapshot through on_change");
        assert!(last.ollama_show_in_bar);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-13: the Ollama cookie is pasted like its OpenCode sibling —
    /// ctrl-v into the focused field must land the clipboard text in the
    /// input, so Save has the session cookie to store.
    #[gpui::test]
    async fn ollama_cookie_paste_feeds_the_save_flow(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-ollama-paste-{}",
            std::process::id()
        ));
        let store_path = dir.join("credentials.json");
        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sesspasted".to_string()));
        });

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();
        let field = cx
            .debug_bounds("provider-ollama-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-v");
        cx.run_until_parked();

        let input = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .ollama_cookie_input
                .clone()
        });
        assert_eq!(
            input, "sesspasted",
            "the paste lands in the Ollama cookie field"
        );

        let save = cx
            .debug_bounds("save-ollama-cookie")
            .expect("Save is live once a cookie is pasted");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            CredentialStore::at(&store_path).get(OllamaCloudUsageFetcher::COOKIE_KEY),
            Some("sesspasted".to_string()),
            "the pasted Ollama cookie reaches the credential store through Save"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-13: Clear deletes the stored Ollama cookie, signs the
    /// provider out and removes its bar segment — the macOS Clear button.
    #[gpui::test]
    async fn ollama_cookie_clear_deletes_and_signs_out(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-ollama-clear-{}",
            std::process::id()
        ));
        let store_path = dir.join("credentials.json");
        let store = CredentialStore::at(&store_path);
        store
            .set(OllamaCloudUsageFetcher::COOKIE_KEY, "sess-seeded")
            .expect("seed the stored cookie");

        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                let mut states = cookie_test_states();
                states.ollama_cloud =
                    ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn);
                let snapshot = SettingsSnapshot {
                    ollama_show_in_bar: true,
                    ..Default::default()
                };
                Settings::with_snapshot(cx, snapshot)
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(states)
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // The Ollama Cloud card is the fourth: at the default test window
        // size its controls paint (debug_bounds sees them) but sit outside
        // the window's hit-test bounds, so clicks would land on nothing.
        // A user reaches them by scrolling; the test grows the window.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let clear = cx
            .debug_bounds("clear-ollama-cookie")
            .expect("the Clear button is drawn");
        cx.simulate_click(clear.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            store.get(OllamaCloudUsageFetcher::COOKIE_KEY).is_none(),
            "Clear deletes the stored cookie"
        );
        let (signed_in, show_in_bar) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.provider_accounts.ollama_cloud.signed_in,
                settings.ollama_show_in_bar,
            )
        });
        assert!(!signed_in, "clearing the cookie signs the provider out");
        assert!(
            !show_in_bar,
            "Clear removes the provider from the usage bar"
        );
        let last = saved
            .borrow()
            .last()
            .cloned()
            .expect("Clear routed a snapshot through on_change");
        assert!(!last.ollama_show_in_bar);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F-SET-13's forbidden outcome, inverted into a test: a Save that
    /// cannot write the store must keep the typed value and show the
    /// failure — same contract as the OpenCode Go field.
    #[gpui::test]
    async fn ollama_cookie_save_failure_keeps_the_input_and_reports(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        // The store path's parent is a regular *file*, so creating the
        // store directory fails deterministically.
        let dir =
            std::env::temp_dir().join(format!("sirio-settings-ollama-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        std::fs::write(dir.join("blocker"), b"").expect("create blocking file");
        let store_path = dir.join("blocker").join("credentials.json");

        let saved: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = saved.clone();
        let window = cx.add_window({
            let store_path = store_path.clone();
            move |_window, cx| {
                Settings::with_snapshot(cx, SettingsSnapshot::default())
                    .with_credential_store(CredentialStore::at(&store_path))
                    .with_account_states(cookie_test_states())
                    .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // The Ollama Cloud card is the fourth: at the default test window
        // size its controls paint (debug_bounds sees them) but sit outside
        // the window's hit-test bounds, so clicks would land on nothing.
        // A user reaches them by scrolling; the test grows the window.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let field = cx
            .debug_bounds("provider-ollama-cookie-field")
            .expect("the cookie field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("sessdemo");
        cx.run_until_parked();
        let save = cx
            .debug_bounds("save-ollama-cookie")
            .expect("the Save button is drawn");
        cx.simulate_click(save.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("provider-ollama-cookie-error").is_some(),
            "a failed Save is visible"
        );
        let (input, signed_in, show_in_bar, error) = cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let settings = settings.read(cx);
            (
                settings.ollama_cookie_input.clone(),
                settings.provider_accounts.ollama_cloud.signed_in,
                settings.ollama_show_in_bar,
                settings.ollama_cookie_error.clone(),
            )
        });
        assert_eq!(
            input, "sessdemo",
            "a failed Save keeps the typed cookie — dropping it silently is the defect"
        );
        assert!(
            !signed_in,
            "a failed Save must not claim the provider signed in"
        );
        assert!(
            !show_in_bar,
            "a failed Save must not turn the bar segment on"
        );
        let error = error.expect("the failure message is held");
        assert!(
            error.starts_with("Failed to update the credential store —"),
            "the message names the store, not a Keychain this platform lacks: {error}"
        );
        assert!(
            saved.borrow().is_empty(),
            "a failed Save persists nothing through on_change"
        );
        assert!(
            !store_path.exists(),
            "no store file appears behind the failure"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[gpui::test]
    async fn provider_cards_render_the_derived_status(cx: &mut gpui::TestAppContext) {
        // The AI Providers cards render one derived status row per card —
        // the same seam a host uses to pin the states, exercised here with
        // explicit values so the test never depends on this machine's real
        // auth files.
        cx.update(Theme::init);
        let states = ProviderAccountStates {
            claude: ProviderAccountStatus::with_identity(
                LocalAccountState::SignedIn,
                Some("user@example.com · Acme".into()),
            ),
            codex: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
            opencode_go: ProviderAccountStatus::from_account_state(LocalAccountState::NoLocalStore),
            ollama_cloud: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
        };
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_account_states(states.clone())
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        // The three cards render one derived status row each. The ids are
        // static strings (the titles are fixed), so each is addressable by
        // its debug selector.
        for status_id in [
            "settings-provider-account-status-Claude Code",
            "settings-provider-account-status-Codex",
            "settings-provider-account-status-OpenCode Go",
        ] {
            assert!(
                cx.debug_bounds(status_id).is_some(),
                "{status_id} renders its derived account status"
            );
        }

        assert!(
            cx.debug_bounds("settings-provider-account-identity-Claude Code")
                .is_some(),
            "a parsed provider identity renders on the provider card"
        );

        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_accounts
                .clone()
        });
        assert_eq!(rendered, states);
        assert_eq!(
            rendered.claude.label, "Signed in",
            "the card reports the pinned signed-in state"
        );
        assert_eq!(
            rendered.opencode_go.label, "Unknown",
            "a provider without a local store reports Unknown, not a guess"
        );
    }

    /// F-SET-15: `account_row` used to take no click callback, and
    /// `settings.rs` had exactly one hardcoded `account_row("System
    /// default", ..., true, ...)` call with zero selection state — a second
    /// account could never exist, let alone be selected. A second Claude
    /// account is seeded straight into the database (the same seam
    /// `with_account_states` uses to avoid touching this machine's real
    /// auth files), then the test clicks its drawn row like a user would
    /// and asserts the "Active" badge really moved: both in the persisted
    /// selection state and in a re-opened database handle, proving this is
    /// a real write, not an in-memory-only toggle.
    #[gpui::test]
    async fn clicking_a_second_account_row_moves_the_active_badge(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let dir = std::env::temp_dir().join(format!(
            "sirio-settings-account-row-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        let db_path = dir.join("sirio.sqlite");
        {
            let db = sirio_persistence::AppDatabase::open(&db_path).expect("open db");
            db.save_agent_account(&sirio_persistence::AgentAccountRecord {
                id: "acct-work".to_string(),
                provider: "claude".to_string(),
                label: "Work".to_string(),
                config_dir_path: "/tmp/sirio-agent-accounts/claude/acct-work".to_string(),
                created_at: 1,
            })
            .expect("seed a second account");
        }

        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_account_states(ProviderAccountStates {
                    claude: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
                    ..ProviderAccountStates::discovered()
                })
                .with_database_path(db_path.clone())
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        // Sanity: before any click, System default is active and the seeded
        // account is not — otherwise the click below could pass vacuously.
        let active_before = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .active_claude_account_id
                .clone()
        });
        assert_eq!(
            active_before, None,
            "sanity: System default starts active, nothing selected yet"
        );

        let work_row = cx
            .debug_bounds("account-acct-work")
            .expect("the seeded second account renders its own drawn row");
        cx.simulate_click(work_row.center(), Modifiers::none());
        cx.run_until_parked();

        let active_after = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .active_claude_account_id
                .clone()
        });
        assert_eq!(
            active_after,
            Some("acct-work".to_string()),
            "clicking the second account row must move the active selection to it"
        );

        // The write must be real, not in-memory-only: a fresh handle on the
        // same file sees the same selection.
        let reopened = sirio_persistence::AppDatabase::open(&db_path).expect("reopen db");
        assert_eq!(
            reopened.active_agent_account_id("claude").expect("read"),
            Some("acct-work".to_string()),
            "the moved selection must be durable, not just an in-process toggle"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[gpui::test]
    async fn provider_cards_no_longer_hardcode_active(cx: &mut gpui::TestAppContext) {
        // The old "Active" row id is gone: a card that claims an unchecked
        // account status must not render under the id tests used to assert
        // it did. The status value now lives under the account-status id.
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-provider-active-Claude Code")
                .is_none(),
            "the hardcoded Active status id must not exist"
        );
        assert!(
            cx.debug_bounds("settings-provider-last-read-Claude Code")
                .is_none(),
            "the fabricated 'Last read' time is gone"
        );
    }

    #[gpui::test]
    async fn sidebar_offers_only_categories_this_platform_has(cx: &mut gpui::TestAppContext) {
        // Permissions is shared with browser-origin grants on Linux; macOS
        // additionally renders its TCC rows in the detail surface.
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-category-Permissions").is_some(),
            "browser-origin permissions are offered on every platform"
        );
    }

    #[gpui::test]
    async fn browser_origin_grants_render_empty_and_revoke_actions(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_browser_origins([
                "https://agent.example".to_owned(),
                "https://docs.example".to_owned(),
            ])
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let permissions = cx
            .debug_bounds("settings-category-Permissions")
            .expect("Permissions is offered for browser grants");
        cx.simulate_click(permissions.center(), Modifiers::none());
        cx.run_until_parked();

        let settings =
            cx.update(|window, _| window.root::<Settings>().flatten().expect("settings root"));
        assert_eq!(
            settings.read_with(&cx.cx, |settings, _| {
                settings
                    .browser_origins()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            }),
            vec!["https://agent.example", "https://docs.example"]
        );

        let revoke = cx
            .debug_bounds("settings-revoke-browser-origin-0")
            .expect("each origin has a revoke action");
        cx.simulate_click(revoke.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            settings.read_with(&cx.cx, |settings, _| {
                settings
                    .browser_origins()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            }),
            vec!["https://docs.example"]
        );

        let revoke_all = cx
            .debug_bounds("settings-revoke-all-browser-origins")
            .expect("the card has a revoke-all action");
        cx.simulate_click(revoke_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("settings-browser-grants-empty").is_some(),
            "revoking all origins leaves an explicit empty state"
        );
    }

    /// P58, F-SET-08: controls whose feature is host-owned must still expose
    /// their state without making the UI crate perform the work. The update
    /// toggle is present even when no update is available; the action itself
    /// is only rendered for an available host-supplied update.
    #[gpui::test]
    async fn general_settings_state_the_version_and_no_dead_controls(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_version("test-version")
                .with_channel("dev")
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        // Probe validation: the mechanism finds present controls.
        assert!(
            cx.debug_bounds("settings-version").is_some(),
            "the app version is stated in General settings"
        );
        let version = cx.update(|window, app| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(app)
                .version
                .clone()
        });
        assert_eq!(version, "test-version");
        assert!(
            cx.debug_bounds("settings-channel").is_some(),
            "the release channel is stated in General settings"
        );
        let channel = cx.update(|window, app| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(app)
                .channel
                .clone()
        });
        assert_eq!(channel, "dev");
        assert!(
            cx.debug_bounds("settings-control-socket-row").is_some(),
            "the sirioctl card renders"
        );

        assert!(
            cx.debug_bounds("general-updates-enabled").is_some(),
            "the host-owned update opt-out is rendered"
        );
        assert!(
            cx.debug_bounds("settings-update-status").is_none(),
            "no check result means no fake update status"
        );
    }

    /// Ticket #316: an available update is rendered as a non-modal detail and
    /// both user actions cross host callbacks; this UI never starts a check,
    /// download, or install itself.
    #[gpui::test]
    async fn available_update_renders_details_and_emits_host_intents(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let apply_calls = Rc::new(RefCell::new(0));
        let enabled_changes = Rc::new(RefCell::new(Vec::new()));
        let apply_spy = apply_calls.clone();
        let enabled_spy = enabled_changes.clone();
        let update = UpdateState {
            enabled: true,
            channel: "stable".into(),
            status: UpdateStatus::Available {
                version: "0.7.0".into(),
                notes: "Security and reliability fixes".into(),
            },
            last_checked: Some("14 minutes ago".into()),
        };
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_version("0.6.0")
                .with_update_state(update)
                .on_apply_update(move || *apply_spy.borrow_mut() += 1)
                .on_update_enabled_change(move |enabled| enabled_spy.borrow_mut().push(enabled))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // The update detail follows the existing General sections, just as a
        // user reaches it by scrolling. Grow this fixture so the click test
        // exercises the real button rather than a point outside the viewport.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.debug_bounds("settings-update-status").is_some());
        assert!(cx.debug_bounds("settings-update-last-checked").is_some());
        assert!(cx.debug_bounds("settings-update-version").is_some());
        assert!(cx.debug_bounds("settings-update-notes").is_some());
        let apply = cx
            .debug_bounds("general-apply-update")
            .expect("available update has a confirming control");
        cx.simulate_click(apply.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*apply_calls.borrow(), 1, "apply is host-owned");

        let enabled = cx
            .debug_bounds("general-updates-enabled")
            .expect("the opt-out toggle renders")
            .center();
        cx.simulate_click(enabled, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(enabled_changes.borrow().as_slice(), &[false]);
        assert!(
            cx.debug_bounds("general-apply-update").is_none(),
            "turning updates off hides the update action"
        );
    }

    /// Ticket #316: Stable only renders non-empty manifest notes; Nightly
    /// uses the one honest fixed explanation instead of daily release notes.
    #[gpui::test]
    async fn update_notes_follow_the_host_channel_and_manifest(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let stable_without_notes = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_update_state(
                UpdateState {
                    enabled: true,
                    channel: "stable".into(),
                    status: UpdateStatus::Available {
                        version: "0.7.0".into(),
                        notes: String::new(),
                    },
                    last_checked: None,
                },
            )
        });
        let mut stable_cx = VisualTestContext::from_window(stable_without_notes.into(), cx);
        stable_cx.run_until_parked();
        let general = stable_cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        stable_cx.simulate_click(general.center(), Modifiers::none());
        stable_cx.run_until_parked();
        assert!(
            stable_cx.debug_bounds("settings-update-notes").is_none(),
            "an absent notes field renders no placeholder"
        );

        let nightly = stable_cx.cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_update_state(
                UpdateState {
                    enabled: true,
                    channel: "nightly".into(),
                    status: UpdateStatus::Available {
                        version: "0.7.0-nightly".into(),
                        notes: "ignored for nightly".into(),
                    },
                    last_checked: None,
                },
            )
        });
        let mut nightly_cx = VisualTestContext::from_window(nightly.into(), &mut stable_cx.cx);
        nightly_cx.run_until_parked();
        let general = nightly_cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        nightly_cx.simulate_click(general.center(), Modifiers::none());
        nightly_cx.run_until_parked();
        assert!(
            nightly_cx.debug_bounds("settings-update-notes").is_some(),
            "nightly has the fixed main-tracking explanation"
        );
    }

    /// F-SET-09: clicking Install Skill reaches the wired host callback
    /// with the exact command `sirio_project::agent_skill_install_command`
    /// builds — this crate's job is only to reach that provisioner and hand
    /// its result off; running it is the host's call.
    #[gpui::test]
    async fn install_skill_click_reaches_the_wired_host_callback(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let calls = Rc::new(RefCell::new(Vec::<SkillInstallCommand>::new()));
        let recorder = calls.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .on_install_skill(move |command| recorder.borrow_mut().push(command))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // The new Updates section sits above Agent Skill; reach the lower
        // existing control through the same scrollable surface a user uses.
        cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)));
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        let install = cx
            .debug_bounds("general-install-skill")
            .expect("Install Skill renders");
        cx.simulate_click(install.center(), Modifiers::none());
        cx.run_until_parked();

        let recorded = calls.borrow();
        assert_eq!(
            recorded.as_slice(),
            [sirio_project::agent_skill_install_command()],
            "the click hands the host the exact provisioned command"
        );

        // F-SET-09: the click must also leave a visible trace on this
        // screen — a confirmation line, not just a callback nobody watching
        // the settings surface can see fired.
        assert!(
            cx.debug_bounds("general-install-skill-status").is_some(),
            "a confirmation line appears once the install is handed off"
        );
    }

    /// F-SET-09: unset, Install Skill must not look wired — it renders
    /// muted and a click reaches nothing, the same dead-control-avoidance
    /// convention P76's titlebar cluster seams use.
    #[gpui::test]
    async fn install_skill_renders_muted_and_inert_when_unwired(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        let install = cx
            .debug_bounds("general-install-skill")
            .expect("Install Skill still renders, muted");
        cx.simulate_click(install.center(), Modifiers::none());
        cx.run_until_parked();
    }

    /// F-SET-14: clicking a provider card's Add Account button hands the
    /// host that provider's stable id — the seam this app can honestly
    /// offer, since it never holds isolated per-provider credentials of
    /// its own (see [`Settings::on_manage_account`]'s field doc).
    #[gpui::test]
    async fn manage_account_click_reaches_the_wired_host_callback_with_the_providers_id(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let calls = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let recorder = calls.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .on_manage_account(move |provider_id| recorder.borrow_mut().push(provider_id))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let add_codex = cx
            .debug_bounds("add-codex-account")
            .expect("Codex card's Add Account renders");
        cx.simulate_click(add_codex.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            calls.borrow().as_slice(),
            ["codex"],
            "the click hands the host the Codex card's own stable id, not another card's"
        );
    }

    /// F-SET-14: production Settings supplies a real fallback handler even
    /// when an embedding does not install the optional host callback. The
    /// live click is exercised against the running app, not by spawning a
    /// terminal from this visual test.
    #[gpui::test]
    async fn add_account_renders_wired_by_default(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let add_claude = cx
            .debug_bounds("add-claude-account")
            .expect("Claude card's Add Account renders with the production fallback");
        assert!(add_claude.size.width > px(0.0));
    }

    /// F-SET-14: while a card's login is spawned and being waited on,
    /// "Add Account" is replaced by a "Signing in…" indicator and a Cancel
    /// button, and Cancel both clears that state and leaves an explanatory
    /// message — the residual gap on top of the already-real
    /// `x-terminal-emulator` fallback `add_account_renders_wired_by_default`
    /// covers. Drives `account_login_pending` directly rather than through
    /// a real spawn, since this sandbox may not have `x-terminal-emulator`
    /// installed; [`Settings::cancel_account_login`] itself is exercised
    /// through a real click.
    #[gpui::test]
    async fn add_account_in_flight_renders_signing_in_and_cancel(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("cancel-claude-account").is_none(),
            "no Cancel renders before any login starts"
        );

        let settings_entity =
            cx.update(|window, _cx| window.root::<Settings>().flatten().expect("settings root"));
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |this, cx| {
                this.account_login_pending = Some(ProviderKind::Claude);
                cx.notify();
            });
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("add-claude-account").is_none(),
            "Add Account is replaced while a login is pending"
        );
        assert!(
            cx.debug_bounds("account-login-pending-Claude Code")
                .is_some(),
            "the Signing in… indicator renders"
        );
        let cancel = cx
            .debug_bounds("cancel-claude-account")
            .expect("Cancel renders while a login is pending");

        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("cancel-claude-account").is_none(),
            "Cancel clears the pending state"
        );
        assert!(
            cx.debug_bounds("add-claude-account").is_some(),
            "Add Account re-renders once the login is canceled"
        );
        let message =
            cx.update(|_window, cx| settings_entity.read(cx).account_action_error.clone());
        assert_eq!(
            message,
            Some((ProviderKind::Claude, "Sign-in canceled".to_string())),
            "the card explains why sign-in stopped"
        );
    }

    /// P58, F-SET-04/05/06/07: every General-screen control must flow into
    /// the persistence contract — the snapshot the host saves on `on_change`.
    /// Before P58 these values were drawn, clamped, and dropped on quit;
    /// this test proves each toggle and stepper lands in the snapshot and
    /// reaches the host callback with the flipped value.
    #[gpui::test]
    async fn general_surface_settings_flow_into_the_persistence_contract(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let changes = Rc::new(RefCell::new(Vec::<SettingsSnapshot>::new()));
        let recorder = changes.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .on_change(move |snapshot| recorder.borrow_mut().push(snapshot))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        // Resume agent sessions: on by default, flip it off.
        let target = cx
            .debug_bounds("general-resume-sessions")
            .expect("resume toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        // Auto-rename tabs: off by default, flip it on.
        let target = cx
            .debug_bounds("general-auto-naming")
            .expect("auto-naming toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        // Limit stored chats: on by default, flip it off.
        let target = cx
            .debug_bounds("general-chat-history")
            .expect("chat-history toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        // Limit mounted worktrees: off by default, flip it on.
        let target = cx
            .debug_bounds("general-mounted-worktrees")
            .expect("mounted-worktrees toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        // Steppers: retention 100 -> 101, mounted 6 -> 7.
        let target = cx
            .debug_bounds("general-chat-retention-increment")
            .expect("retention stepper renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        let target = cx
            .debug_bounds("general-mounted-count-increment")
            .expect("mounted stepper renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert!(!snapshot.resume_agent_sessions);
        assert!(snapshot.auto_naming);
        assert!(!snapshot.limit_chat_history);
        assert_eq!(snapshot.chat_retention, 101);
        assert!(snapshot.limit_mounted_worktrees);
        assert_eq!(snapshot.mounted_worktrees, 7);

        // The host callback received every change with the same values the
        // surface shows — nothing is report-only anymore.
        let recorded = changes.borrow();
        assert_eq!(recorded.len(), 6, "one on_change per control");
        assert!(!recorded[0].resume_agent_sessions);
        assert!(recorded[1].auto_naming);
        assert!(!recorded[2].limit_chat_history);
        assert!(recorded[3].limit_mounted_worktrees);
        assert_eq!(recorded[4].chat_retention, 101);
        assert_eq!(recorded[5].mounted_worktrees, 7);
    }

    /// P58, F-SET-05: the summarizer picker is disabled while auto-naming
    /// is off and enabled once it is on, and a chosen agent lands in the
    /// persistence contract. Before P58 the button was a literal no-op.
    #[gpui::test]
    async fn summarizer_picker_is_gated_on_auto_naming_and_selects(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        // Auto-naming is off: the trigger draws muted and refuses to open.
        let trigger = cx
            .debug_bounds("general-summarizer")
            .expect("the summarizer trigger renders");
        cx.simulate_click(trigger.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_none(),
            "the picker does not open while auto-naming is off"
        );

        // Enable auto-naming; the trigger now opens the menu with all five
        // agents.
        let target = cx
            .debug_bounds("general-auto-naming")
            .expect("auto-naming toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        let target = cx
            .debug_bounds("general-summarizer")
            .expect("summarizer trigger renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_some(),
            "the picker opens while auto-naming is on"
        );
        for option_id in [
            "summarizer-option-claude",
            "summarizer-option-codex",
            "summarizer-option-opencode",
            "summarizer-option-pi",
            "summarizer-option-omp",
        ] {
            assert!(
                cx.debug_bounds(option_id).is_some(),
                "{option_id} is offered"
            );
        }

        // Choose Codex: the selection lands in the snapshot and the menu
        // closes.
        let target = cx
            .debug_bounds("summarizer-option-codex")
            .expect("codex option renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_none(),
            "choosing an agent closes the menu"
        );
        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(snapshot.summarizer_agent, SummarizerChoice::Codex);

        // Reopen the menu and dismiss it with Escape: the menu's own key
        // binding consumes the keystroke (it is focused), and the surface
        // asks for focus back so the shell's Escape keeps working after the
        // menu is gone.
        let target = cx
            .debug_bounds("general-summarizer")
            .expect("summarizer trigger renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_some(),
            "the picker reopens"
        );
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_none(),
            "Escape closes the picker menu — the menu's scoped binding beats
            the shell's global one while it holds focus"
        );
    }

    /// P58, F-SET-10: "Refresh now" re-runs provider discovery — the cards
    /// re-read the local credential files instead of keeping whatever they
    /// were pinned to. A pinned, obviously-wrong state is replaced by a
    /// fresh read, which is exactly what the button's handler must do.
    #[gpui::test]
    async fn refresh_now_re_runs_provider_discovery(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let pinned = ProviderAccountStates {
            claude: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
            codex: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
            opencode_go: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
            ollama_cloud: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
        };
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_account_states(pinned)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let refresh = cx
            .debug_bounds("refresh-claude-now")
            .expect("Refresh now renders on the Claude card");
        cx.simulate_click(refresh.center(), Modifiers::none());
        cx.run_until_parked();

        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_accounts
                .clone()
        });
        assert_eq!(
            rendered,
            ProviderAccountStates::discovered(),
            "Refresh now replaces the shown states with a fresh disk read"
        );
    }

    /// F-SET-19 / F-SET-20: the Appearance screen's real controls drive
    /// the snapshot — clicking the Light segment flips the theme mode,
    /// the translucency toggle flips the flag, and the interface font
    /// stepper increments the persisted value.
    #[gpui::test]
    async fn appearance_controls_drive_theme_translucency_and_font_size(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let settings =
            cx.update(|window, _| window.root::<Settings>().flatten().expect("settings root"));

        // Open the Appearance section the way a user does.
        let appearance = cx
            .debug_bounds("settings-category-Appearance")
            .expect("Appearance is offered as a category");
        cx.simulate_click(appearance.center(), Modifiers::none());
        cx.run_until_parked();

        // Light: the second segment of the theme control.
        let light = cx
            .debug_bounds("appearance-theme-1")
            .expect("the theme segment control offers three choices");
        cx.simulate_click(light.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            settings.read_with(&cx.cx, |settings, _| settings.snapshot().theme),
            ThemeMode::Light,
            "clicking the Light segment flips the theme mode"
        );

        // Translucency: the toggle flips from its default.
        let toggle = cx
            .debug_bounds("appearance-translucency")
            .expect("the translucency toggle is drawn");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            settings.read_with(&cx.cx, |settings, _| settings.translucency),
            "clicking the toggle turns translucency on"
        );
        // F-SET-20: the flag must also leave the surface — SettingsSnapshot
        // now has a `translucency` field, so the emitted payload carries it
        // rather than trapping the value inside this view.
        assert!(
            settings.read_with(&cx.cx, |settings, _| settings.snapshot().translucency),
            "the emitted snapshot carries the translucency flag, not just the internal field"
        );

        // Interface font size: the stepper increments 13 to 14.
        let increment = cx
            .debug_bounds("interface-font-size-increment")
            .expect("the interface font stepper is drawn");
        cx.simulate_click(increment.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            settings.read_with(&cx.cx, |settings, _| settings
                .snapshot()
                .interface_font_size),
            14,
            "the + half of the stepper raises the interface font size"
        );
        assert_eq!(
            cx.update(|_, cx| Theme::get(cx).typography.ui_size),
            px(14.0),
            "the interface font size updates the shared typography"
        );
    }

    // The shell's Escape contract (F-SET-02), tested with a faithful host
    // instead of the real workspace: the real shell's harness cannot run in
    // this test scheduler (its control poll loop does real filesystem work,
    // which the deterministic scheduler flags — the quarantined palette-test
    // family). This host replicates exactly the workspace's three pieces:
    // the global `escape` binding, an action handler on the host root, and
    // requesting the surface's focus when it opens.
    actions!(settings_escape_harness, [EscapeClosesSurface]);

    struct EscapeHost {
        settings: Entity<Settings>,
        closed: Rc<Cell<bool>>,
    }

    impl Render for EscapeHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let closed = self.closed.clone();
            div()
                .size_full()
                .on_action(move |_: &EscapeClosesSurface, _, _| {
                    closed.set(true);
                })
                .child(self.settings.clone())
        }
    }

    #[gpui::test]
    async fn shell_escape_closes_settings_through_the_focused_surface(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("escape", EscapeClosesSurface, None)]);
        });
        let closed = Rc::new(Cell::new(false));
        let host_closed = closed.clone();
        let window = cx.add_window(|_window, cx| {
            let settings = cx.new(|cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
            EscapeHost {
                settings,
                closed: host_closed,
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let host = cx.update(|window, _| {
            window
                .root::<EscapeHost>()
                .flatten()
                .expect("escape host root")
        });

        // Without the surface focused, the global binding has no handler on
        // its dispatch path (the fallback path is the window's synthetic
        // root) — Escape is a no-op, exactly the defect F-SET-02 named.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            !closed.get(),
            "before the surface is focused the action has no handler on the dispatch path"
        );

        // The host opens settings the way the workspace does: request the
        // surface's focus, then let the next frame land it.
        host.update(&mut cx, |host, cx| {
            host.settings.update(cx, |settings, cx| {
                settings.request_surface_focus();
                cx.notify();
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let surface_focused = cx.update(|window, app| {
            host.read(app)
                .settings
                .read(app)
                .surface_focus
                .is_focused(window)
        });
        assert!(surface_focused, "the surface holds focus after the request");

        // Escape now reaches the host's handler through the focused
        // surface's dispatch path.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            closed.get(),
            "with the surface focused, Escape dispatches the close action to the host"
        );
    }

    #[gpui::test]
    async fn shell_escape_does_not_close_while_the_picker_menu_is_focused(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("escape", EscapeClosesSurface, None)]);
        });
        let closed = Rc::new(Cell::new(false));
        let host_closed = closed.clone();
        let window = cx.add_window(|_window, cx| {
            let settings = cx.new(|cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
            EscapeHost {
                settings,
                closed: host_closed,
            }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let host = cx.update(|window, _| {
            window
                .root::<EscapeHost>()
                .flatten()
                .expect("escape host root")
        });
        host.update(&mut cx, |host, cx| {
            host.settings.update(cx, |settings, cx| {
                settings.request_surface_focus();
                cx.notify();
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();

        // Open the picker menu (auto-naming on first): the menu takes
        // focus, and its own scoped Escape binding resolves before the
        // shell's global one.
        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();
        let target = cx
            .debug_bounds("general-auto-naming")
            .expect("auto-naming toggle renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        let target = cx
            .debug_bounds("general-summarizer")
            .expect("summarizer trigger renders")
            .center();
        cx.simulate_click(target, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_some(),
            "the picker menu is open"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("summarizer-popover").is_none(),
            "Escape closes the menu"
        );
        assert!(
            !closed.get(),
            "the menu's scoped binding consumed Escape — settings stays open"
        );

        // The menu handed focus back to the surface; the next Escape closes
        // settings through the host.
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            closed.get(),
            "after the menu closes, Escape reaches the shell's handler again"
        );
    }

    /// WCAG 2.1 contrast between two opaque colours, the same ratio
    /// `sirio_theme`'s own palette tests use.
    fn contrast_ratio(one: Rgba, other: Rgba) -> f32 {
        let luminance = |color: Rgba| {
            let channel = |c: f32| {
                if c <= 0.03928 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
        };
        let (a, b) = (luminance(one), luminance(other));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// The detail column must scroll under the wheel.
    ///
    /// It did not. The column was a flex *row*, so the page it holds was
    /// stretched to the viewport's height by the default `align-items:
    /// stretch`; GPUI derives a scroller's `content_size` from its
    /// children's laid-out bounds (`div.rs`), so a page exactly as tall as
    /// the viewport yields `scroll_max == 0` and the wheel moves nothing.
    /// Everything below the fold was unreachable — which is why three
    /// tests in this file grow the window to click a control instead of
    /// scrolling to it.
    #[gpui::test]
    async fn the_detail_column_scrolls_under_the_wheel(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // Deliberately shorter than the AI Providers page: the four cards
        // do not fit, so there is something to scroll to.
        cx.simulate_resize(gpui::size(px(1100.0), px(600.0)));
        cx.run_until_parked();

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let viewport = cx
            .debug_bounds("settings-detail-scroll")
            .expect("the scroll viewport is drawn");
        let page = cx
            .debug_bounds("settings-detail-page")
            .expect("the page is drawn");
        // Stated first because it is the failure that hides behind a dead
        // wheel: if the viewport is as tall as the page, nothing overflows
        // and there is no scrolling to test.
        assert!(
            viewport.size.height < page.size.height,
            "the viewport must be shorter than the page it shows: viewport              {:?}, page {:?}",
            viewport.size.height,
            page.size.height
        );
        let before = cx
            .debug_bounds("settings-provider-account-status-Claude Code")
            .expect("the first card is drawn");

        // A point over the detail column, clear of the category rail.
        let over = gpui::point(px(CATEGORY_WIDTH + 260.0), px(300.0));
        cx.simulate_mouse_move(over, None, Modifiers::none());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: over,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-200.0))),
            modifiers: Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();

        let after = cx
            .debug_bounds("settings-provider-account-status-Claude Code")
            .expect("the first card stays drawn after scrolling");
        assert!(
            after.origin.y < before.origin.y - px(50.0),
            "the wheel must move the detail column: the first card sat at \
             {:?} before the wheel and {:?} after",
            before.origin.y,
            after.origin.y
        );
    }

    #[gpui::test]
    async fn switching_settings_category_resets_the_detail_scroll(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(gpui::size(px(1100.0), px(600.0)));
        cx.run_until_parked();

        let general = cx
            .debug_bounds("settings-category-General")
            .expect("General category is offered");
        cx.simulate_click(general.center(), Modifiers::none());
        cx.run_until_parked();

        let viewport = cx
            .debug_bounds("settings-detail-scroll")
            .expect("the scroll viewport is drawn");
        let before = cx
            .debug_bounds("settings-version")
            .expect("the top of General is drawn");
        let over = gpui::point(px(CATEGORY_WIDTH + 260.0), px(300.0));
        cx.simulate_mouse_move(over, None, Modifiers::none());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: over,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-200.0))),
            modifiers: Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
        let after = cx
            .debug_bounds("settings-version")
            .expect("the General content remains drawn after scrolling");
        assert!(
            after.origin.y < before.origin.y - px(50.0),
            "scrolling General must move its content: before {:?}, after {:?}",
            before.origin.y,
            after.origin.y
        );

        let providers = cx
            .debug_bounds("settings-category-AiProviders")
            .expect("AI Providers category is offered");
        cx.simulate_click(providers.center(), Modifiers::none());
        cx.run_until_parked();

        let first_provider = cx
            .debug_bounds("settings-provider-account-status-Claude Code")
            .expect("the first AI Provider card is drawn");
        assert!(
            first_provider.origin.y >= viewport.origin.y,
            "switching categories must return the detail column to its top: viewport {:?}, first provider {:?}",
            viewport.origin.y,
            first_provider.origin.y
        );
    }

    /// Every agent row and provider card shows its own brand mark, not a
    /// Unicode stand-in.
    ///
    /// The screen used to draw `✳ ◉ ▣ π ☁` as text. Those code points are
    /// not in the UI face on every platform, so the column rendered blank
    /// or tofu — and even where they resolve they are not the agents'
    /// marks. `Icon::for_agent_id` already answers this question for the
    /// tab bar; settings must use the same answer.
    #[test]
    fn provider_rows_and_cards_carry_their_brand_marks() {
        for (id, expected) in [
            ("claude", Icon::ClaudeCode),
            ("codex", Icon::Codex),
            ("opencode", Icon::OpenCode),
            ("pi", Icon::Pi),
            ("omp", Icon::OhMyPi),
        ] {
            let row = provider_row(
                &AgentAvailability {
                    id,
                    display_name: "irrelevant",
                    executable: None,
                },
                None,
            );
            assert_eq!(
                row.icon, expected,
                "the {id} row must carry {id}'s own mark"
            );
        }

        assert_eq!(ProviderKind::Claude.icon(), Icon::ClaudeCode);
        assert_eq!(ProviderKind::Codex.icon(), Icon::Codex);
        assert_eq!(ProviderKind::OpenCodeGo.icon(), Icon::OpenCode);
        // Ollama has no mark in the vendored catalog. The globe is a
        // declared stand-in, and it is still a drawn asset rather than a
        // code point the platform may not have.
        assert_eq!(ProviderKind::OllamaCloud.icon(), Icon::Globe);
    }

    /// Text on a saturated status fill has to be readable in both
    /// appearances.
    ///
    /// The "not installed" pill and the terminal-only ACP badge painted
    /// `title_selected` — which *is* `title`, the page's body colour — on
    /// `tab_error` and `tab_needs_input`. Measured, that lands at 1.13:1
    /// (warning, dark) and 1.66:1 (danger, light): the text is the same
    /// lightness as the chip under it and simply disappears.
    #[test]
    fn status_pill_text_stays_readable_on_its_fill() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let theme = match mode {
                ThemeMode::Dark => Theme::dark(),
                _ => Theme::light(),
            };
            for (name, fill) in [
                ("tab_error", theme.danger),
                ("tab_needs_input", theme.warning),
            ] {
                let ratio = contrast_ratio(on_status_fill(&theme), fill);
                assert!(
                    ratio >= 4.0,
                    "{mode:?}: status text on {name} is {ratio:.2}:1, which no \
                     reader can use"
                );
            }
        }
    }

    /// The header's back control is the arrow alone, at a size that reads
    /// as a target rather than a hint next to a word.
    #[gpui::test]
    async fn the_back_control_is_a_square_arrow_with_no_label(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let back = cx
            .debug_bounds("settings-back")
            .expect("the back control is drawn");
        assert_eq!(
            back.size.width,
            px(BACK_CONTROL_SIZE),
            "the label is gone, so the control is as wide as it is tall"
        );
        assert_eq!(back.size.height, px(BACK_CONTROL_SIZE));
    }
}

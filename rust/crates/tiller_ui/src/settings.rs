//! Full-window settings surface and its small fixture model.

use crate::controls;
use crate::sidebar::icons::{Icon, IconElement};
use gpui::{Context, Entity, FontWeight, Render, Rgba, Window, div, prelude::*, px, text};
use std::path::PathBuf;
use std::rc::Rc;
use tiller_agents::{AgentAvailability, discover_availability};
use tiller_theme::{Theme, ThemeMode};
use tiller_usage::{LocalAccountState, UsageProvider};

/// The settings content column — the frozen 720px content column of
/// `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md` (waku
/// `CONTENT_MAX_WIDTH`). It used to be 704, a value nobody recorded; the
/// measured column is 720.
pub(crate) const CONTENT_WIDTH: f32 = 720.0;
const HEADER_HEIGHT: f32 = 40.0;
const CATEGORY_WIDTH: f32 = 200.0;
const DETAIL_TOP_PADDING: f32 = 15.0;
const DETAIL_BOTTOM_PADDING: f32 = 12.0;
const DETAIL_SECTION_MARGIN: f32 = 20.0;

const SEGMENTED_THEME: &[&str] = &["System", "Light", "Dark"];
/// The file-icon sets this platform can actually render, in display order
/// (see [`file_icon_choices`]). On macOS both SF Symbols and the embedded
/// Material set exist; everywhere else only the embedded set does.
#[cfg(target_os = "macos")]
const SEGMENTED_FILE_ICONS: &[&str] = &["SF Symbols", "Material"];
#[cfg(not(target_os = "macos"))]
const SEGMENTED_FILE_ICONS: &[&str] = &["Material"];

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
                .text_color(theme.title)
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
    /// Permissions drives macOS TCC (camera, microphone, screen recording,
    /// accessibility) — machinery that does not exist outside macOS. On
    /// non-macOS the category is not offered, exactly as the icon-set lists
    /// only renderable sets: a sidebar entry for a permission system the
    /// running OS does not have states something untrue about the program.
    #[cfg(target_os = "macos")]
    const ALL: [Self; 5] = [
        Self::AiProviders,
        Self::Agents,
        Self::General,
        Self::Permissions,
        Self::Appearance,
    ];
    #[cfg(not(target_os = "macos"))]
    const ALL: [Self; 4] = [
        Self::AiProviders,
        Self::Agents,
        Self::General,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileIconChoice {
    SfSymbols,
    Material,
}

impl FileIconChoice {
    /// The user-visible name, matching the Swift settings surface.
    pub fn title(self) -> &'static str {
        match self {
            Self::SfSymbols => "SF Symbols",
            Self::Material => "Material",
        }
    }

    /// Whether this icon set actually exists on the running platform. SF
    /// Symbols is Apple's system icon API — there is no such font and no
    /// such API on Linux — so it must never be offered where it cannot
    /// render (P19: a settings screen that lists a set the program cannot
    /// use states something untrue about the running program).
    pub fn available_on_this_platform(self) -> bool {
        match self {
            Self::SfSymbols => cfg!(target_os = "macos"),
            Self::Material => true,
        }
    }
}

/// The file-icon sets this platform can actually render, in display order.
/// The settings surface derives its segmented control from this list — the
/// platform answers, exactly as the font token resolves through its own
/// candidate list.
pub fn file_icon_choices() -> &'static [FileIconChoice] {
    #[cfg(target_os = "macos")]
    {
        &[FileIconChoice::SfSymbols, FileIconChoice::Material]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[FileIconChoice::Material]
    }
}

/// Clamps a choice to what this platform can actually render. A persisted
/// value written on another platform (the Swift-parity database defaults to
/// sfSymbols) must not be displayed as if it were a real option here.
pub fn clamp_file_icons(choice: FileIconChoice) -> FileIconChoice {
    if choice.available_on_this_platform() {
        choice
    } else {
        file_icon_choices()[0]
    }
}

/// The segment index of a choice inside the platform's list. A choice that
/// does not exist here (a clamped value should prevent that) lands on the
/// first segment rather than producing an out-of-range index.
fn file_icons_segment(choice: FileIconChoice) -> usize {
    file_icon_choices()
        .iter()
        .position(|candidate| *candidate == choice)
        .unwrap_or(0)
}

/// The persistence-facing values owned by the settings surface.
///
/// This deliberately contains plain UI data rather than a persistence or
/// database type. The host supplies the initial snapshot and receives a new
/// one through [`Settings::on_change`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsSnapshot {
    pub theme: ThemeMode,
    pub interface_font_size: i32,
    pub terminal_font_size: i32,
    pub file_icons: FileIconChoice,
    pub control_socket_enabled: bool,
    /// The control socket's resolved path, routed from the host. This is
    /// runtime state (the path the live socket listens on), not a persisted
    /// setting — the host fills it, and the surface only displays it.
    pub socket_path: String,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            interface_font_size: 13,
            terminal_font_size: 13,
            file_icons: FileIconChoice::SfSymbols,
            control_socket_enabled: true,
            socket_path: String::new(),
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
    pub glyph: &'static str,
    pub description: String,
    pub status: ProviderStatus,
}

/// The honest status shown on one AI Provider card: what local credential
/// state exists on this machine, derived — never a mock default. The word
/// "Active" used to sit here and claimed the provider's account worked
/// without anything having checked it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderAccountStatus {
    /// The label next to the dot, from the account state's own wording.
    pub label: &'static str,
    /// Whether real credentials exist: green dot when true, neutral
    /// otherwise. "Not signed in" and "Unknown" are information, not
    /// errors.
    pub signed_in: bool,
}

impl ProviderAccountStatus {
    /// Derives the card's status from the local account state the usage
    /// crate read off disk.
    pub fn from_account_state(state: LocalAccountState) -> Self {
        Self {
            label: state.label(),
            signed_in: state == LocalAccountState::SignedIn,
        }
    }
}

/// The three provider cards' account states, in card order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderAccountStates {
    pub claude: ProviderAccountStatus,
    pub codex: ProviderAccountStatus,
    pub opencode_go: ProviderAccountStatus,
}

impl ProviderAccountStates {
    /// Reads each provider's local credential state from disk (the usage
    /// crate owns the credential files; the surface only reports).
    pub fn discovered() -> Self {
        Self {
            claude: ProviderAccountStatus::from_account_state(
                UsageProvider::Claude.local_account_state(),
            ),
            codex: ProviderAccountStatus::from_account_state(
                UsageProvider::Codex.local_account_state(),
            ),
            opencode_go: ProviderAccountStatus::from_account_state(
                UsageProvider::OpenCodeGo.local_account_state(),
            ),
        }
    }
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

fn provider_glyph(id: &str) -> &'static str {
    match id {
        "claude" => "✳",
        "codex" => "◉",
        "opencode" => "▣",
        _ => "π",
    }
}

fn provider_glyph_color(theme: Theme, id: &str) -> Rgba {
    match id {
        "claude" => theme.rail_question,
        "codex" => theme.tab_focus_accent,
        "opencode" => theme.tab_needs_input,
        _ => theme.rail_task,
    }
}

/// Derives the row a settings entry shows from one discovery result.
///
/// The description and status follow the availability data, never a fixed
/// claim: an installed CLI reports the resolved executable, an absent one
/// says so in a way the user can act on.
pub fn provider_row(availability: &AgentAvailability) -> ProviderRowModel {
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
    ProviderRowModel {
        id: availability.id,
        name,
        glyph: provider_glyph(availability.id),
        description,
        status,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Claude,
    Codex,
    OpenCodeGo,
}

/// Small settings view model. The real application can replace these values
/// with its persistence layer without changing the reusable settings UI.
/// The badge shown at the trailing edge of a permission row.
struct PermissionBadge {
    label: &'static str,
    background: Rgba,
    foreground: Rgba,
}

pub struct Settings {
    category: SettingsCategory,
    on_back: Option<Rc<dyn Fn()>>,
    on_change: Option<Rc<dyn Fn(SettingsSnapshot)>>,
    theme_mode: ThemeMode,
    translucency: bool,
    interface_font_size: i32,
    terminal_font_size: i32,
    file_icons: FileIconChoice,
    claude_show_in_bar: bool,
    codex_show_in_bar: bool,
    opencode_show_in_bar: bool,
    refresh_interval: i32,
    resume_agent_sessions: bool,
    auto_naming: bool,
    limit_chat_history: bool,
    chat_retention: i32,
    limit_mounted_worktrees: bool,
    mounted_worktrees: i32,
    control_socket_enabled: bool,
    socket_path: String,
    /// Discovery results for every supported agent CLI, from the crate that
    /// owns the catalog. The Agents screen renders only this data.
    provider_availability: Vec<AgentAvailability>,
    /// Account state for the three AI Provider cards, derived from local
    /// credential files at construction — never a mock default.
    provider_accounts: ProviderAccountStates,
}

/// The display data for one AI Provider card — everything the renderer
/// needs except the entity and theme. Bundled so the render function stays
/// within clippy's argument budget and each card reads as one view model.
#[derive(Clone, Copy)]
struct ProviderCardView {
    kind: ProviderKind,
    title: &'static str,
    glyph: &'static str,
    glyph_color: Rgba,
    status: ProviderAccountStatus,
}

impl ProviderCardView {
    #[allow(clippy::too_many_arguments)]
    fn new(
        kind: ProviderKind,
        title: &'static str,
        glyph: &'static str,
        glyph_color: Rgba,
        status: ProviderAccountStatus,
    ) -> Self {
        Self {
            kind,
            title,
            glyph,
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
    pub fn with_snapshot(_cx: &mut Context<Self>, initial: SettingsSnapshot) -> Self {
        Self {
            category: SettingsCategory::Appearance,
            on_back: None,
            on_change: None,
            theme_mode: initial.theme,
            translucency: false,
            interface_font_size: initial.interface_font_size.clamp(10, 20),
            terminal_font_size: initial.terminal_font_size.clamp(9, 24),
            // A persisted choice from another platform (the database default
            // is the Swift-parity sfSymbols) is clamped to the first set
            // that exists here, so the surface never shows a choice it
            // cannot render.
            file_icons: clamp_file_icons(initial.file_icons),
            claude_show_in_bar: true,
            codex_show_in_bar: true,
            opencode_show_in_bar: false,
            refresh_interval: 5,
            resume_agent_sessions: true,
            auto_naming: false,
            limit_chat_history: true,
            chat_retention: 100,
            limit_mounted_worktrees: false,
            mounted_worktrees: 6,
            control_socket_enabled: initial.control_socket_enabled,
            // The Agents screen reports what discovery finds on this
            // machine — never a fixed list of "Available" claims.
            provider_availability: discover_availability(),
            // The provider cards report what local credential state exists
            // on this machine — never a fixed list of "Active" claims.
            provider_accounts: ProviderAccountStates::discovered(),
            socket_path: initial.socket_path,
        }
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

    /// Returns the current values that belong to the persistence contract.
    pub fn snapshot(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            theme: self.theme_mode,
            interface_font_size: self.interface_font_size,
            terminal_font_size: self.terminal_font_size,
            file_icons: self.file_icons,
            control_socket_enabled: self.control_socket_enabled,
            socket_path: self.socket_path.clone(),
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
        self.category = category;
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

    fn set_translucency(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.translucency = enabled;
        cx.notify();
    }

    fn set_interface_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.interface_font_size = value.clamp(10, 20);
        self.changed();
        cx.notify();
    }

    fn set_terminal_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.terminal_font_size = value.clamp(9, 24);
        self.changed();
        cx.notify();
    }

    fn set_file_icons(&mut self, index: usize, cx: &mut Context<Self>) {
        // The index is an index into the platform's actual list — never a
        // hardcoded position in a constant that can mention sets this
        // machine cannot render.
        if let Some(choice) = file_icon_choices().get(index) {
            self.file_icons = *choice;
            self.changed();
            cx.notify();
        }
    }

    fn set_control_socket_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.control_socket_enabled = enabled;
        self.changed();
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
        }
        cx.notify();
    }

    fn provider_visibility(&self, provider: ProviderKind) -> bool {
        match provider {
            ProviderKind::Claude => self.claude_show_in_bar,
            ProviderKind::Codex => self.codex_show_in_bar,
            ProviderKind::OpenCodeGo => self.opencode_show_in_bar,
        }
    }

    fn set_refresh_interval(&mut self, value: i32, cx: &mut Context<Self>) {
        self.refresh_interval = value.clamp(1, 60);
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
            .bg(theme.background)
            .child(
                div()
                    .id("settings-back")
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(theme.radii.chip_active)
                    .text_size(theme.typography.headline)
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, _| {
                        if let Some(callback) = &back {
                            callback();
                        }
                    })
                    .child(IconElement::new(Icon::ChevronLeft, px(14.0)).text_color(theme.meta))
                    .child(text!("Back")),
            )
            .child(
                div()
                    .text_size(theme.typography.title3)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
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
            .bg(theme.background);

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
                        theme.title
                    } else {
                        theme.subtitle
                    })
                    .when(selected, |this| this.bg(theme.selected_fill))
                    .hover(|style| style.bg(theme.row_hover))
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
                                IconElement::new(category.glyph(), px(14.0)).text_color(theme.meta),
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
        theme_card = theme_card.child(
            div()
                .id("settings-appearance-translucency-row")
                .child(controls::row("Translucency", None, translucency, theme)),
        );

        let interface_entity = entity.clone();
        let interface_stepper = controls::stepper(
            "interface-font-size",
            self.interface_font_size,
            theme,
            move |value, cx| {
                interface_entity.update(cx, |this, cx| this.set_interface_font_size(value, cx));
            },
        );
        let terminal_entity = entity.clone();
        let terminal_stepper = controls::stepper(
            "terminal-font-size",
            self.terminal_font_size,
            theme,
            move |value, cx| {
                terminal_entity.update(cx, |this, cx| this.set_terminal_font_size(value, cx));
            },
        );

        let interface_card =
            controls::card(theme).child(div().id("settings-interface-font-size-row").child(
                controls::row(
                    "Font size",
                    Some(format!("{} pt", self.interface_font_size)),
                    interface_stepper,
                    theme,
                ),
            ));
        let terminal_card =
            controls::card(theme).child(div().id("settings-terminal-font-size-row").child(
                controls::row(
                    "Font size",
                    Some(format!("{} pt", self.terminal_font_size)),
                    terminal_stepper,
                    theme,
                ),
            ));

        let file_entity = entity.clone();
        let file_icons = controls::segmented(
            "file-icons",
            SEGMENTED_FILE_ICONS,
            file_icons_segment(self.file_icons),
            theme,
            move |index, cx| file_entity.update(cx, |this, cx| this.set_file_icons(index, cx)),
        );
        let files_card =
            controls::card(theme).child(controls::row("File icons", None, file_icons, theme));

        let agent_card = self.render_agent_colors(theme);

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("Theme", theme_card, theme))
            .child(settings_section("Interface", interface_card, theme))
            .child(settings_section("Terminal", terminal_card, theme))
            .child(settings_section("Files", files_card, theme))
            .child(settings_section("Agent Colors", agent_card, theme))
    }

    fn render_agent_colors(&self, theme: Theme) -> gpui::Div {
        let agents: [(&str, &str, Rgba); 5] = [
            ("✳", "Claude Code", theme.rail_question),
            ("◉", "Codex", theme.tab_focus_accent),
            ("▣", "OpenCode", theme.tab_needs_input),
            ("π", "Pi", theme.tab_done),
            ("π", "Oh-My-Pi", theme.rail_task),
        ];
        let mut card = controls::card(theme);
        for (index, (glyph, name, color)) in agents.into_iter().enumerate() {
            if index > 0 {
                card = card.child(controls::separator(theme));
            }
            let label = div()
                .flex()
                .items_center()
                .gap(px(9.0))
                .text_size(theme.typography.headline)
                .text_color(theme.title)
                .child(
                    div()
                        .w(px(14.0))
                        .text_color(color)
                        .child(text!(id = ("settings-agent-color-glyph", index), glyph)),
                )
                .child(text!(id = ("settings-agent-color-name", index), name));
            card = card.child(
                div()
                    .id(("settings-agent-color-row", index))
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label)
                    .child(controls::color_swatch(
                        match index {
                            0 => "agent-color-claude",
                            1 => "agent-color-codex",
                            2 => "agent-color-opencode",
                            3 => "agent-color-pi",
                            _ => "agent-color-omp",
                        },
                        color,
                        theme,
                    )),
            );
        }
        card
    }

    fn render_provider_card(
        &self,
        view: ProviderCardView,
        entity: Entity<Self>,
        theme: Theme,
    ) -> gpui::Div {
        let ProviderCardView {
            kind: provider,
            title,
            glyph,
            glyph_color,
            status,
        } = view;
        let status_label = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(theme.typography.headline)
            .text_color(theme.title)
            .child(
                div()
                    .w(px(18.0))
                    .text_size(px(17.0))
                    .text_color(glyph_color)
                    .child(text!(
                        id = format!("settings-provider-status-glyph-{title}"),
                        glyph
                    )),
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
        let status_value = div()
            .id(format!("settings-provider-account-status-{title}"))
            .debug_selector(move || format!("settings-provider-account-status-{title}"))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(theme.typography.callout)
            .text_color(theme.title)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(if status.signed_in {
                        theme.tab_done
                    } else {
                        theme.meta
                    }),
            )
            .child(text!(status.label));

        let visibility_entity = entity.clone();
        let visibility = controls::toggle(
            match provider {
                ProviderKind::Claude => "provider-claude-visibility",
                ProviderKind::Codex => "provider-codex-visibility",
                ProviderKind::OpenCodeGo => "provider-opencode-visibility",
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
        let refresh_stepper = controls::stepper_with_unit(
            match provider {
                ProviderKind::Claude => "provider-claude-refresh",
                ProviderKind::Codex => "provider-codex-refresh",
                ProviderKind::OpenCodeGo => "provider-opencode-refresh",
            },
            self.refresh_interval,
            "min",
            theme,
            move |value, cx| {
                refresh_entity.update(cx, |this, cx| this.set_refresh_interval(value, cx));
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
            .child(controls::action_row(
                controls::button(
                    match provider {
                        ProviderKind::Claude => "refresh-claude-now",
                        ProviderKind::Codex => "refresh-codex-now",
                        ProviderKind::OpenCodeGo => "refresh-opencode-now",
                    },
                    "Refresh now",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ))
            .child(controls::separator(theme));

        card = card
            .child(controls::subsection_header(
                "Accounts",
                "Showing accounts for this device. New accounts are added there.",
                controls::button(
                    match provider {
                        ProviderKind::Claude => "add-claude-account",
                        ProviderKind::Codex => "add-codex-account",
                        ProviderKind::OpenCodeGo => "add-opencode-account",
                    },
                    "Add Account",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ))
            // The "Active" badge on the System default row is a *selection*
            // marker, not a credential claim: this app has no isolated
            // accounts, so the current CLI login is definitionally the
            // account agent terminals use. It stays true by construction,
            // not because anything was checked at render time.
            .child(controls::account_row(
                "System default",
                "Use your current CLI login on this device.",
                true,
                theme,
            ));
        card
    }

    fn render_ai_providers(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let accounts = self.provider_accounts;
        let cards = [
            ProviderCardView::new(
                ProviderKind::Claude,
                "Claude Code",
                "✳",
                theme.rail_question,
                accounts.claude,
            ),
            ProviderCardView::new(
                ProviderKind::Codex,
                "Codex",
                "◉",
                theme.subtitle,
                accounts.codex,
            ),
            ProviderCardView::new(
                ProviderKind::OpenCodeGo,
                "OpenCode Go",
                "▣",
                theme.tab_needs_input,
                accounts.opencode_go,
            ),
        ];
        let mut page = div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING));
        for card in cards {
            page = page.child(settings_section(
                card.title,
                self.render_provider_card(card, entity.clone(), theme),
                theme,
            ));
        }
        page
    }

    /// The status pill for one provider row: the resolved executable when
    /// the CLI is installed, the crate's own "not found" wording when it
    /// is absent. A badge that claimed a binary not on PATH was installed
    /// would be the exact lie this screen used to tell.
    fn render_provider_status(availability: &AgentAvailability, theme: Theme) -> impl IntoElement {
        let status_id = format!("settings-agent-status-{}", availability.id);
        match &availability.executable {
            Some(path) => div()
                .id(status_id.clone())
                .debug_selector(move || status_id.clone())
                .px(px(8.0))
                .py(px(3.0))
                .rounded(theme.radii.row_card)
                .text_size(theme.typography.caption2)
                .text_color(theme.subtitle)
                .bg(theme.primary_pill_bg)
                .child(text!(path.to_string_lossy().into_owned())),
            None => div()
                .id(status_id.clone())
                .debug_selector(move || status_id)
                .px(px(8.0))
                .py(px(3.0))
                .rounded(theme.radii.row_card)
                .text_size(theme.typography.caption2)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.title_selected)
                .bg(theme.tab_error)
                .child(text!(availability.status_label())),
        }
    }

    fn render_agents(&self, theme: Theme) -> gpui::Div {
        let mut agent_rows = controls::card(theme);
        for (index, availability) in self.provider_availability.iter().enumerate() {
            if index > 0 {
                agent_rows = agent_rows.child(controls::separator(theme));
            }
            let row = provider_row(availability);
            let label = div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(18.0))
                        .text_color(provider_glyph_color(theme, row.id))
                        .child(text!(id = ("settings-agent-glyph", index), row.glyph)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(theme.typography.headline)
                                .text_color(theme.title)
                                .child(text!(id = ("settings-agent-name", index), row.name)),
                        )
                        .child(
                            div()
                                .text_size(theme.typography.footnote)
                                .text_color(theme.subtitle)
                                .child(text!(
                                    id = ("settings-agent-description", index),
                                    row.description
                                )),
                        ),
                );
            agent_rows = agent_rows.child(
                div()
                    .id(("settings-agent-row", index))
                    .debug_selector(move || format!("settings-agent-row-{index}"))
                    .child(controls::row_view(
                        label,
                        Self::render_provider_status(availability, theme),
                        theme,
                    )),
            );
        }

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(
                div()
                    .w_full()
                    .mb(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .w(px(260.0))
                            .h(px(28.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(theme.radii.control)
                            .bg(theme.primary_pill_bg)
                            .text_size(theme.typography.callout)
                            .text_color(theme.subtitle)
                            .child(text!("Search agents")),
                    )
                    .child(controls::button(
                        "refresh-agents",
                        "↻ Refresh",
                        theme,
                        |_, _, _| {},
                    )),
            )
            .child(agent_rows)
    }

    fn render_general(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let resume_entity = entity.clone();
        let auto_entity = entity.clone();
        let history_entity = entity.clone();
        let mounted_entity = entity.clone();
        let socket_entity = entity.clone();
        let resume = controls::toggle(
            "general-resume-sessions",
            self.resume_agent_sessions,
            theme,
            move |_, _, cx| {
                resume_entity.update(cx, |this, cx| {
                    this.resume_agent_sessions = !this.resume_agent_sessions;
                    cx.notify();
                });
            },
        );
        let auto = controls::toggle(
            "general-auto-naming",
            self.auto_naming,
            theme,
            move |_, _, cx| {
                auto_entity.update(cx, |this, cx| {
                    this.auto_naming = !this.auto_naming;
                    cx.notify();
                });
            },
        );
        let history = controls::toggle(
            "general-chat-history",
            self.limit_chat_history,
            theme,
            move |_, _, cx| {
                history_entity.update(cx, |this, cx| {
                    this.limit_chat_history = !this.limit_chat_history;
                    cx.notify();
                });
            },
        );
        let mounted = controls::toggle(
            "general-mounted-worktrees",
            self.limit_mounted_worktrees,
            theme,
            move |_, _, cx| {
                mounted_entity.update(cx, |this, cx| {
                    this.limit_mounted_worktrees = !this.limit_mounted_worktrees;
                    cx.notify();
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
        let mut about = controls::card(theme).child(controls::row(
            "Version",
            None,
            div()
                .debug_selector(|| "settings-version".into())
                .text_size(theme.typography.callout)
                .text_color(theme.subtitle)
                .child(text!("0.1.0")),
            theme,
        ));
        about = about
            .child(controls::separator(theme))
            .child(controls::action_row(
                controls::button(
                    "general-check-updates",
                    "Check for Updates",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ));

        let agents = controls::card(theme).child(controls::row(
            "Resume agent sessions on launch",
            Some(
                "Relaunch supported agents with their previous conversation after Tiller restarts."
                    .into(),
            ),
            resume,
            theme,
        ));
        let automation = controls::card(theme)
            .child(controls::row(
                "Auto-rename tabs and agents",
                Some(
                    "Summarizes each session into a short tab title using the selected agent."
                        .into(),
                ),
                auto,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row(
                "Summarizer agent",
                Some("Falls back to the session's own agent when it fails.".into()),
                controls::button("general-summarizer", "Claude Code", theme, |_, _, _| {}),
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
            theme,
            move |value, cx| {
                history_stepper_entity.update(cx, |this, cx| {
                    this.chat_retention = value.clamp(5, 500);
                    cx.notify();
                });
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
            theme,
            move |value, cx| {
                mounted_stepper_entity.update(cx, |this, cx| {
                    this.mounted_worktrees = value.clamp(2, 50);
                    cx.notify();
                });
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
        let socket_label = div()
            .flex()
            .flex_col()
            .justify_center()
            .flex_1()
            .text_size(theme.typography.headline)
            .text_color(theme.title)
            .child(text!(
                id = "settings-control-socket-title",
                "Control socket"
            ))
            .child(
                div()
                    .debug_selector(|| "settings-control-socket-path".into())
                    .mt(px(2.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.subtitle)
                    .child(text!(format!("Socket path: {}", self.socket_path))),
            );
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
                    .text_color(theme.subtitle)
                    .child(text!("tillerctl")),
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::action_row(
                controls::button(
                    "general-install-path",
                    "Copy install command",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ));
        let skill = controls::card(theme).child(controls::action_row(
            controls::button(
                "general-install-skill",
                "Install Skill",
                theme,
                |_, _, _| {},
            ),
            theme,
        ));

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("About", about, theme))
            .child(settings_section("Agents", agents, theme))
            .child(settings_section("Automation", automation, theme))
            .child(settings_section("Chat history", history_card, theme))
            .child(settings_section("Performance", performance, theme))
            .child(settings_section("tillerctl", control, theme))
            .child(settings_section("Agent Skill", skill, theme))
    }

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
            .child(div().w(px(20.0)).text_color(theme.meta).child(text!(
                id = format!("settings-permission-glyph-{title}"),
                glyph
            )))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
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
            .text_color(theme.subtitle)
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

    fn render_permissions(&self, theme: Theme) -> gpui::Div {
        let granted = theme.tab_done;
        let denied = theme.tab_error;
        let neutral = theme.primary_pill_bg;
        let rows = controls::card(theme)
            .child(self.render_permission_row(
                "♧",
                "Notifications",
                "Alerts when agents finish or need input.",
                PermissionBadge {
                    label: "GRANTED",
                    background: granted,
                    foreground: theme.background,
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
                    foreground: theme.background,
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
                    foreground: theme.title_selected,
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
                    foreground: theme.subtitle,
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
                    foreground: theme.background,
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
                    foreground: theme.subtitle,
                },
                "Trigger Prompt",
                theme,
            ));

        let grants = [
            "1533B573-3BEF-438C-B3AD-2586AA2546C4",
            "C7CE2EAA-35C1-4DDB-90B4-56CF61CC4528",
            "73C23955-2FAF-420D-8239-AC4FB67C1130",
            "4E0DA223-128C-43B3-8EDD-47991D3AB022",
            "B5B8813-0871-423E-8913-28423E580186",
        ];
        let mut grants_card = controls::card(theme);
        for (index, grant) in grants.into_iter().enumerate() {
            if index > 0 {
                grants_card = grants_card.child(controls::separator(theme));
            }
            grants_card = grants_card.child(
                div()
                    .id(("settings-grant-row", index))
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(theme.typography.headline)
                                    .text_color(theme.title)
                                    .child(text!(id = ("settings-grant-prefix", index), "file:")),
                            )
                            .child(
                                div()
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.subtitle)
                                    .child(text!(id = ("settings-grant", index), grant)),
                            ),
                    )
                    .child(controls::button(
                        match index {
                            0 => "revoke-grant-0",
                            1 => "revoke-grant-1",
                            2 => "revoke-grant-2",
                            3 => "revoke-grant-3",
                            _ => "revoke-grant-4",
                        },
                        "Revoke",
                        theme,
                        |_, _, _| {},
                    )),
            );
        }

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(
                controls::card(theme)
                    .child(controls::row(
                        "Terminal tools inherit Tiller's macOS privacy envelope.",
                        Some("Use these controls when a CLI or agent in a pane needs macOS privacy access. Tiller does not ask at startup.".into()),
                        controls::button("refresh-permissions", "Refresh", theme, |_, _, _| {}),
                        theme,
                    )),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .child(settings_section("macOS Permissions", rows, theme)),
            )
            .child(settings_section("Browser origin grants", grants_card, theme))
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let mode = Theme::get(cx).mode;
        let entity = cx.entity();
        let category_sidebar = self.render_categories(theme, entity.clone());
        let detail = match self.category {
            SettingsCategory::AiProviders => self.render_ai_providers(theme, entity),
            SettingsCategory::Agents => self.render_agents(theme),
            SettingsCategory::General => self.render_general(theme, entity),
            SettingsCategory::Permissions => self.render_permissions(theme),
            SettingsCategory::Appearance => self.render_appearance(theme, mode, entity),
        };

        div()
            .id("settings-surface")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(self.render_header(theme))
            .child(div().h(px(1.0)).w_full().bg(theme.hairline))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .child(category_sidebar)
                    .child(div().w(px(1.0)).h_full().bg(theme.hairline))
                    .child(
                        div()
                            .id("settings-detail-scroll")
                            .flex_1()
                            .h_full()
                            .flex()
                            .justify_center()
                            .overflow_y_scroll()
                            .child(detail),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, VisualTestContext};

    #[test]
    fn settings_snapshot_defaults_match_the_persisted_contract() {
        let snapshot = SettingsSnapshot::default();

        assert_eq!(snapshot.theme, ThemeMode::System);
        assert_eq!(snapshot.interface_font_size, 13);
        assert_eq!(snapshot.terminal_font_size, 13);
        assert_eq!(snapshot.file_icons, FileIconChoice::SfSymbols);
        assert!(snapshot.control_socket_enabled);
        assert!(
            snapshot.socket_path.is_empty(),
            "the socket path is runtime state routed by the host, never a baked-in template"
        );
    }

    #[test]
    fn file_icon_choices_are_platform_derived() {
        // The settings surface must only offer sets that actually exist on
        // this machine. SF Symbols is Apple's system icon API — it cannot
        // exist on Linux — so the list here must never mention it.
        assert_eq!(
            FileIconChoice::SfSymbols.available_on_this_platform(),
            cfg!(target_os = "macos"),
            "SF Symbols is selectable exactly where it exists"
        );
        assert!(FileIconChoice::Material.available_on_this_platform());
        assert!(
            file_icon_choices()
                .iter()
                .all(|choice| choice.available_on_this_platform()),
            "every listed set must be renderable on this platform"
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            file_icon_choices(),
            &[FileIconChoice::SfSymbols, FileIconChoice::Material]
        );
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(file_icon_choices(), &[FileIconChoice::Material]);
            assert_eq!(
                SEGMENTED_FILE_ICONS,
                &["Material"],
                "the segmented control lists exactly the platform's sets"
            );
        }
    }

    #[test]
    fn persisted_sf_symbols_choice_clamps_to_what_exists_here() {
        // The persistence contract defaults to sfSymbols (Swift parity), but
        // a choice for a platform that cannot render it must not surface as
        // if it were real. The clamp is idempotent and selection always
        // lands on the platform's list.
        assert_eq!(
            clamp_file_icons(FileIconChoice::SfSymbols),
            file_icon_choices()[0]
        );
        assert_eq!(
            clamp_file_icons(file_icon_choices()[0]),
            file_icon_choices()[0],
            "clamping a real choice is a no-op"
        );
        assert_eq!(file_icons_segment(file_icon_choices()[0]), 0);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(
            file_icons_segment(FileIconChoice::SfSymbols),
            0,
            "a choice that cannot exist here still selects the first real segment"
        );
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

        let row = provider_row(&available);
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

        let row = provider_row(&absent);
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
    fn permissions_category_is_platform_gated() {
        // Permissions drives macOS TCC — camera, microphone, screen
        // recording, accessibility. None of that machinery exists outside
        // macOS, so the offered categories must not include it there.
        let offered: Vec<SettingsCategory> = SettingsCategory::ALL.to_vec();

        #[cfg(target_os = "macos")]
        assert!(
            offered.contains(&SettingsCategory::Permissions),
            "macOS offers the TCC permission screen"
        );
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(offered.len(), 4);
            assert!(
                !offered.contains(&SettingsCategory::Permissions),
                "non-macOS has no macOS TCC machinery — the sidebar must not offer Permissions"
            );
        }
    }

    #[gpui::test]
    async fn agent_rows_render_what_discovery_found(cx: &mut gpui::TestAppContext) {
        // The Agents screen renders the discovery list, not a fixed set of
        // "Available" claims: the fixture below mirrors this machine's
        // reality (opencode/omp absent) and must render absent rows too.
        cx.update(Theme::init);
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
            AgentAvailability {
                id: "omp",
                display_name: "Oh-My-Pi",
                executable: None,
            },
        ];
        let expected = fixture.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-row-0").is_some(),
            "the first provider row renders"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-2").is_some(),
            "the third provider row renders"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-3").is_none(),
            "rows follow the discovery list, not a fixed count"
        );
        assert!(
            cx.debug_bounds("settings-agent-status-claude").is_some(),
            "an installed CLI renders its status pill"
        );
        assert!(
            cx.debug_bounds("settings-agent-status-opencode").is_some(),
            "an absent CLI still renders its status pill"
        );
        assert!(cx.debug_bounds("settings-agent-status-omp").is_some());

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
            rendered, expected,
            "the surface renders exactly what discovery returned"
        );
    }

    #[gpui::test]
    async fn control_socket_row_shows_resolved_path_and_toggles(cx: &mut gpui::TestAppContext) {
        // The General screen must display the *resolved* path (the one the
        // live socket listens on), and the row must reflect both the
        // enabled and disabled states.
        cx.update(Theme::init);
        let snapshot = SettingsSnapshot {
            socket_path: "/run/user/1000/TillerRust/control.sock".into(),
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
        assert_eq!(
            snapshot.socket_path,
            "/run/user/1000/TillerRust/control.sock"
        );
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
            snapshot.socket_path, "/run/user/1000/TillerRust/control.sock",
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

    #[gpui::test]
    async fn provider_cards_render_the_derived_status(cx: &mut gpui::TestAppContext) {
        // The AI Providers cards render one derived status row per card —
        // the same seam a host uses to pin the states, exercised here with
        // explicit values so the test never depends on this machine's real
        // auth files.
        cx.update(Theme::init);
        let states = ProviderAccountStates {
            claude: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
            codex: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
            opencode_go: ProviderAccountStatus::from_account_state(LocalAccountState::NoLocalStore),
        };
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_account_states(states)
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

        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_accounts
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
        // The category sidebar is derived from the platform-gated list: on
        // non-macOS a Permissions entry would invite the user into a page
        // about a permission system their OS does not have.
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        #[cfg(target_os = "macos")]
        assert!(cx.debug_bounds("settings-category-Permissions").is_some());
        #[cfg(not(target_os = "macos"))]
        assert!(
            cx.debug_bounds("settings-category-Permissions").is_none(),
            "non-macOS must not offer a macOS-only permission screen"
        );
    }

    #[gpui::test]
    async fn selecting_the_listed_file_icon_set_changes_the_snapshot(
        cx: &mut gpui::TestAppContext,
    ) {
        // Whatever the settings screen lists must be selectable and must
        // actually take effect: clicking the File icons segment updates the
        // snapshot with the platform's set.
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let segment = cx
            .debug_bounds("file-icons-0")
            .expect("the File icons segment is rendered");
        cx.simulate_click(segment.center(), Modifiers::none());
        cx.run_until_parked();

        let snapshot = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .snapshot()
        });
        assert_eq!(snapshot.file_icons, file_icon_choices()[0]);
        assert!(
            snapshot.file_icons.available_on_this_platform(),
            "the snapshot must never carry a set the platform cannot render"
        );
    }

    /// F-SET-03 (what exists): General settings state the version and the
    /// Check for Updates control is drawn and clickable. The button's
    /// handler is empty in the Linux rewrite, so the checking/up-to-date/
    /// error result states of the VERIFY clause are recorded as absent —
    /// this test proves the reachable half only.
    #[gpui::test]
    async fn general_settings_state_the_version_and_the_updates_control_is_clickable(
        cx: &mut gpui::TestAppContext,
    ) {
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

        assert!(
            cx.debug_bounds("settings-version").is_some(),
            "the app version is stated in General settings"
        );
        let check = cx
            .debug_bounds("general-check-updates")
            .expect("Check for Updates is drawn");
        cx.simulate_click(check.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("general-check-updates").is_some(),
            "the update control stays in the frame after the click"
        );
    }
}

//! A project's chosen visual identity, and the pickers that set it
//! (F-PRJ-13/14/15/16, which make F-PER-07 persistable).
//!
//! This is a standalone, mountable component, not a screen: the project
//! settings sheet that must host it — `sidebar.rs`'s `render_project_settings`
//! — belongs to `codex12`, so this file never reaches into it. See
//! `../../../docs/linux-rewrite/SEAMS.md` for the mount seam and the avatar
//! network-fetch seam this file deliberately does not build.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    ClickEvent, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, PathPromptOptions,
    Render, Window, div, prelude::*, px, text,
};
use tiller_theme::Theme;
use unicode_segmentation::UnicodeSegmentation;

use crate::controls;
use crate::settings::AgentAccentColor;
use crate::sidebar::icons::{Icon, IconElement};

/// F-PRJ-15: a curated subset of this app's own shipped glyphs offered as
/// project icons — not [`crate::sidebar::icons::ALL_ICONS`]. Most of that
/// 23-icon set is UI chrome (chevrons, the Settings gear, Close) with no
/// plausible reading as a project's face; these six are the ones that do —
/// "this project is a ___" — playing the role SF Symbols' grid plays in the
/// macOS original, over the glyph set this port actually ships instead of
/// Apple's catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectGlyph {
    Folder,
    GitBranch,
    Chat,
    Terminal,
    File,
    Globe,
}

impl ProjectGlyph {
    pub const ALL: [Self; 6] = [
        Self::Folder,
        Self::GitBranch,
        Self::Chat,
        Self::Terminal,
        Self::File,
        Self::Globe,
    ];

    /// The stable id used to persist and to select a swatch.
    pub fn id(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::GitBranch => "git-branch",
            Self::Chat => "chat",
            Self::Terminal => "terminal",
            Self::File => "file",
            Self::Globe => "globe",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Folder => Icon::FolderFill,
            Self::GitBranch => Icon::GitBranch,
            Self::Chat => Icon::MessageSquare,
            Self::Terminal => Icon::SquareTerminal,
            Self::File => Icon::File,
            Self::Globe => Icon::Globe,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|glyph| glyph.id() == value)
    }
}

/// F-PRJ-14: where the avatar image comes from. `LocalPng` is the only
/// source this crate can resolve on its own — it already has the file's
/// bytes. `GitHub` and `Favicon` record what the user asked for; turning
/// either into actual pixels is network I/O and is seamed out (see the
/// module doc).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvatarSource {
    LocalPng(PathBuf),
    GitHub(String),
    Favicon(String),
}

/// F-PRJ-13/14/15/16: exactly one source, plus a tint that only the
/// `Symbol` source uses — matching the reference app, where colour applies
/// to the Icon mode and not to a photographic avatar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectIconValue {
    Symbol(ProjectGlyph),
    Emoji(String),
    Avatar(AvatarSource),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectIcon {
    pub value: ProjectIconValue,
    pub tint: AgentAccentColor,
}

impl Default for ProjectIcon {
    /// F-PRJ-13's reset target: the plain folder glyph, undyed.
    fn default() -> Self {
        Self {
            value: ProjectIconValue::Symbol(ProjectGlyph::Folder),
            tint: AgentAccentColor::Coral,
        }
    }
}

impl ProjectIcon {
    /// Returns the stable pair written to `ProjectRecord.icon_kind` and
    /// `ProjectRecord.icon_value`. The UI crate deliberately returns plain
    /// strings here so the shell can bridge to persistence without making
    /// this component depend on SQLite types.
    pub fn persisted_parts(&self) -> (String, Option<String>) {
        match &self.value {
            ProjectIconValue::Symbol(glyph) => ("icon".into(), Some(glyph.id().into())),
            ProjectIconValue::Emoji(emoji) => ("emoji".into(), Some(emoji.clone())),
            ProjectIconValue::Avatar(AvatarSource::LocalPng(path)) => (
                "avatar".into(),
                Some(format!("png:{}", path.to_string_lossy())),
            ),
            ProjectIconValue::Avatar(AvatarSource::GitHub(identifier)) => {
                ("avatar".into(), Some(format!("github:{identifier}")))
            }
            ProjectIconValue::Avatar(AvatarSource::Favicon(domain)) => {
                ("avatar".into(), Some(format!("favicon:{domain}")))
            }
        }
    }

    /// Rebuilds a picker value from the persisted pair. Unknown or malformed
    /// values fall back to the reset target instead of making a persisted row
    /// unusable. `color_id` is kept separate because it is stored in the
    /// project's existing `color_hex` column.
    pub fn from_persisted_parts(kind: &str, value: Option<&str>, color_id: Option<&str>) -> Self {
        let tint = color_id
            .map(AgentAccentColor::parse)
            .unwrap_or(AgentAccentColor::Coral);
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            return Self {
                value: ProjectIconValue::Symbol(ProjectGlyph::Folder),
                tint,
            };
        };
        let value = match kind {
            "icon" => ProjectGlyph::parse(value)
                .or_else(|| (value == "folder.fill").then_some(ProjectGlyph::Folder))
                .map(ProjectIconValue::Symbol),
            "emoji" => Some(ProjectIconValue::Emoji(value.to_string())),
            "avatar" => value
                .strip_prefix("png:")
                .filter(|path| !path.is_empty())
                .map(|path| ProjectIconValue::Avatar(AvatarSource::LocalPng(path.into())))
                .or_else(|| {
                    value
                        .strip_prefix("github:")
                        .filter(|id| !id.is_empty())
                        .map(|id| ProjectIconValue::Avatar(AvatarSource::GitHub(id.to_string())))
                })
                .or_else(|| {
                    value
                        .strip_prefix("favicon:")
                        .filter(|domain| !domain.is_empty())
                        .map(|domain| {
                            ProjectIconValue::Avatar(AvatarSource::Favicon(domain.to_string()))
                        })
                }),
            _ => None,
        };
        Self {
            value: value.unwrap_or(ProjectIconValue::Symbol(ProjectGlyph::Folder)),
            tint,
        }
    }
}

/// Which sub-picker is showing. Independent of `ProjectIcon::value`: a user
/// can browse the Avatar tab without having committed an avatar yet, the
/// same way the reference app's own segmented "Icon / Emoji / Avatar"
/// choice does not itself change the project's icon until a concrete pick
/// is made inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerMode {
    Icon,
    Emoji,
    Avatar,
}

impl PickerMode {
    const OPTIONS: &'static [&'static str] = &["Icon", "Emoji", "Avatar"];

    fn index(self) -> usize {
        match self {
            Self::Icon => 0,
            Self::Emoji => 1,
            Self::Avatar => 2,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Emoji,
            2 => Self::Avatar,
            _ => Self::Icon,
        }
    }

    fn from_value(value: &ProjectIconValue) -> Self {
        match value {
            ProjectIconValue::Symbol(_) => Self::Icon,
            ProjectIconValue::Emoji(_) => Self::Emoji,
            ProjectIconValue::Avatar(_) => Self::Avatar,
        }
    }
}

/// A PNG over this size is rejected before it is read into memory. The
/// reference app states its own limit in Swift source this port has no
/// access to (copying from a reference checkout is out of bounds here); this
/// is this port's own reasonable cap, not a transcribed figure — pireview
/// should confirm whether it needs to match a specific number.
const MAX_AVATAR_PNG_BYTES: u64 = 1_048_576;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// F-PRJ-16's "Open Emoji Picker" grid: a curated, searchable set of
/// project-relevant emoji (each entry's keywords), not the full Unicode
/// emoji table — the same "curated subset" call `ProjectGlyph::ALL` makes
/// above, for the same reason (no plausible in-app way to browse the full
/// set, and this crate has no OS character palette to shell out to on
/// Linux).
const EMOJI_CHOICES: &[(&str, &[&str])] = &[
    ("🚀", &["rocket", "launch", "fast", "ship"]),
    ("🔥", &["fire", "hot", "flame"]),
    ("⭐", &["star", "favorite"]),
    ("✨", &["sparkles", "magic", "new"]),
    ("🐛", &["bug", "insect", "debug"]),
    ("🧪", &["test", "flask", "science", "experiment"]),
    ("📦", &["package", "box", "ship"]),
    ("🔧", &["wrench", "tool", "fix"]),
    ("⚙", &["gear", "settings", "cog"]),
    ("🔒", &["lock", "secure", "security"]),
    ("🔑", &["key", "unlock", "access"]),
    ("📚", &["book", "books", "docs", "documentation"]),
    ("📝", &["memo", "note", "write"]),
    ("💡", &["bulb", "idea", "light"]),
    ("🎯", &["target", "goal", "aim"]),
    ("🏆", &["trophy", "win", "award"]),
    ("🌐", &["globe", "world", "web", "network"]),
    ("💻", &["laptop", "computer", "code"]),
    ("🖥", &["desktop", "computer", "monitor"]),
    ("📊", &["chart", "graph", "data", "analytics"]),
    ("🗂", &["folder", "files", "organize"]),
    ("🧩", &["puzzle", "piece", "plugin"]),
    ("🛠", &["tools", "build", "hammer"]),
    ("🚧", &["construction", "wip", "progress"]),
    ("✅", &["check", "done", "complete"]),
    ("❌", &["cross", "fail", "error", "no"]),
    ("⚠", &["warning", "caution", "alert"]),
    ("💾", &["save", "disk", "floppy"]),
    ("🔗", &["link", "chain", "connect"]),
    ("🧠", &["brain", "ai", "smart"]),
    ("🤖", &["robot", "bot", "ai", "agent"]),
    ("🐙", &["octopus", "git", "github"]),
    ("🐳", &["whale", "docker", "container"]),
    ("🦀", &["crab", "rust"]),
    ("🐍", &["snake", "python"]),
    ("☕", &["coffee", "java", "cup"]),
    ("🍃", &["leaf", "green", "eco"]),
    ("🌙", &["moon", "night", "dark"]),
    ("☀", &["sun", "light", "day"]),
    ("🎨", &["art", "design", "palette"]),
    ("🧭", &["compass", "navigate", "direction"]),
];

/// The standalone project-identity picker (F-PRJ-13/14/15/16). Mirrors
/// `Settings`/`StatusBar`'s own shape: a self-contained entity with
/// `on_change`-style host callbacks, so whoever mounts it (the seam named in
/// the module doc) only has to construct it, place it, and consume the
/// callback — never reach into its fields.
pub struct ProjectIconPicker {
    value: ProjectIcon,
    mode: PickerMode,
    emoji_draft: String,
    emoji_focus: FocusHandle,
    emoji_error: Option<String>,
    github_draft: String,
    github_focus: FocusHandle,
    github_error: Option<String>,
    favicon_draft: String,
    favicon_focus: FocusHandle,
    favicon_error: Option<String>,
    png_error: Option<String>,
    /// F-PRJ-16: there is no Linux desktop-portal equivalent of macOS's
    /// system character palette this crate could shell out to, so "Open
    /// Emoji Picker" opens a searchable grid this crate renders itself
    /// (`EMOJI_CHOICES`, below) instead of delegating to a host callback —
    /// self-contained, so nothing outside this file needs to wire it up.
    emoji_grid_open: bool,
    emoji_grid_query: String,
    emoji_grid_focus: FocusHandle,
    on_change: Option<Rc<dyn Fn(ProjectIcon)>>,
    on_change_with_context: Option<Rc<dyn Fn(ProjectIcon, &mut Context<Self>)>>,
    /// Test-only substitute for the OS file picker `choose_local_png`
    /// normally opens — mirrors `chat.rs`'s `attach_test_paths` seam, since
    /// nothing can drive a real native file dialog from a `gpui::test`.
    #[cfg(test)]
    png_test_paths: Vec<PathBuf>,
}

impl ProjectIconPicker {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self::with_value(ProjectIcon::default(), _cx)
    }

    pub fn with_value(value: ProjectIcon, cx: &mut Context<Self>) -> Self {
        let mode = PickerMode::from_value(&value.value);
        Self {
            value,
            mode,
            emoji_draft: String::new(),
            emoji_focus: cx.focus_handle(),
            emoji_error: None,
            github_draft: String::new(),
            github_focus: cx.focus_handle(),
            github_error: None,
            favicon_draft: String::new(),
            favicon_focus: cx.focus_handle(),
            favicon_error: None,
            png_error: None,
            emoji_grid_open: false,
            emoji_grid_query: String::new(),
            emoji_grid_focus: cx.focus_handle(),
            on_change: None,
            on_change_with_context: None,
            #[cfg(test)]
            png_test_paths: Vec::new(),
        }
    }

    pub fn on_change(mut self, callback: impl Fn(ProjectIcon) + 'static) -> Self {
        self.on_change = Some(Rc::new(callback));
        self
    }

    /// Context-aware host seam used when the picker is mounted inside a
    /// parent GPUI entity that must repaint immediately after a selection.
    pub fn on_change_with_context(
        mut self,
        callback: impl Fn(ProjectIcon, &mut Context<Self>) + 'static,
    ) -> Self {
        self.on_change_with_context = Some(Rc::new(callback));
        self
    }

    /// The current value, for the host and for tests.
    pub fn value(&self) -> &ProjectIcon {
        &self.value
    }

    #[cfg(test)]
    fn queue_test_png_path(&mut self, path: PathBuf) {
        self.png_test_paths.push(path);
    }

    fn set_mode(&mut self, index: usize, cx: &mut Context<Self>) {
        self.mode = PickerMode::from_index(index);
        cx.notify();
    }

    fn commit(&mut self, value: ProjectIconValue, cx: &mut Context<Self>) {
        self.value.value = value;
        if let Some(callback) = &self.on_change {
            callback(self.value.clone());
        }
        if let Some(callback) = &self.on_change_with_context {
            callback(self.value.clone(), cx);
        }
        cx.notify();
    }

    fn select_glyph(&mut self, glyph: ProjectGlyph, cx: &mut Context<Self>) {
        self.commit(ProjectIconValue::Symbol(glyph), cx);
    }

    fn set_tint(&mut self, tint: AgentAccentColor, cx: &mut Context<Self>) {
        self.value.tint = tint;
        if let Some(callback) = &self.on_change {
            callback(self.value.clone());
        }
        if let Some(callback) = &self.on_change_with_context {
            callback(self.value.clone(), cx);
        }
        cx.notify();
    }

    /// F-PRJ-13: discards whatever source and tint are set and returns to
    /// the undyed folder glyph.
    fn reset(&mut self, cx: &mut Context<Self>) {
        self.value = ProjectIcon::default();
        self.mode = PickerMode::Icon;
        if let Some(callback) = &self.on_change {
            callback(self.value.clone());
        }
        if let Some(callback) = &self.on_change_with_context {
            callback(self.value.clone(), cx);
        }
        cx.notify();
    }

    fn on_emoji_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.emoji_draft.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.emoji_draft.push_str(character);
        }
        self.emoji_error = None;
        cx.notify();
    }

    /// F-PRJ-16: "enter one emoji, try invalid multi-character input."
    /// Counted in grapheme clusters, not `char`s — a real single emoji like
    /// a flag or a family sequence is several `char`s but one grapheme, and
    /// counting `char`s would reject valid input the row asks this to
    /// accept.
    fn commit_emoji(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.emoji_draft.trim();
        if trimmed.graphemes(true).count() != 1 {
            self.emoji_error = Some("Enter exactly one emoji.".to_string());
            cx.notify();
            return;
        }
        let emoji = trimmed.to_string();
        self.emoji_error = None;
        self.commit(ProjectIconValue::Emoji(emoji), cx);
    }

    /// F-PRJ-16: opens the searchable emoji grid. Self-contained -- see
    /// `emoji_grid_open`'s doc comment for why this doesn't delegate to a
    /// host callback the way `choose_local_png`'s file picker does.
    fn open_emoji_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.emoji_grid_open = true;
        self.emoji_grid_query.clear();
        self.emoji_grid_focus.focus(window, cx);
        cx.notify();
    }

    fn close_emoji_grid(&mut self, cx: &mut Context<Self>) {
        self.emoji_grid_open = false;
        cx.notify();
    }

    fn on_emoji_grid_query_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_emoji_grid(cx);
            return;
        }
        if key == "backspace" || key == "delete" {
            self.emoji_grid_query.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.emoji_grid_query.push_str(character);
        }
        cx.notify();
    }

    /// Picking a swatch commits it immediately (mirrors `select_glyph` in
    /// Icon mode) rather than routing back through the typed field + "Set
    /// Emoji" — a click is already exactly-one-grapheme by construction.
    fn pick_emoji_from_grid(&mut self, emoji: &'static str, cx: &mut Context<Self>) {
        self.emoji_draft = emoji.to_string();
        self.emoji_error = None;
        self.emoji_grid_open = false;
        self.commit(ProjectIconValue::Emoji(emoji.to_string()), cx);
    }

    fn on_github_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.github_draft.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.github_draft.push_str(character);
        }
        self.github_error = None;
        cx.notify();
    }

    /// F-PRJ-14's GitHub source: validates the identifier looks entered
    /// (non-empty, no embedded whitespace) and records it. Resolving it into
    /// an actual avatar image is network I/O — see the module doc's seam.
    fn commit_github(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.github_draft.trim();
        if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
            self.github_error = Some("Enter a GitHub user or repository.".to_string());
            cx.notify();
            return;
        }
        let identifier = trimmed.to_string();
        self.github_error = None;
        self.commit(
            ProjectIconValue::Avatar(AvatarSource::GitHub(identifier)),
            cx,
        );
    }

    fn on_favicon_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.favicon_draft.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.favicon_draft.push_str(character);
        }
        self.favicon_error = None;
        cx.notify();
    }

    /// F-PRJ-14's favicon source: validates the domain at least looks like
    /// one (non-empty, no whitespace, contains a `.`) and records it.
    /// Fetching the favicon itself is the same network seam as GitHub.
    fn commit_favicon(&mut self, cx: &mut Context<Self>) {
        let trimmed = self.favicon_draft.trim();
        if trimmed.is_empty() || trimmed.contains(char::is_whitespace) || !trimmed.contains('.') {
            self.favicon_error = Some("Enter a domain, like example.com.".to_string());
            cx.notify();
            return;
        }
        let domain = trimmed.to_string();
        self.favicon_error = None;
        self.commit(ProjectIconValue::Avatar(AvatarSource::Favicon(domain)), cx);
    }

    /// F-PRJ-14's local-PNG source, the one avatar path this crate resolves
    /// entirely on its own: opens the native file picker, then validates the
    /// result is a real PNG under [`MAX_AVATAR_PNG_BYTES`] before committing
    /// it. In tests, `png_test_paths` stands in for the picker the same way
    /// `chat.rs::attach_image` already does — nothing can drive the real
    /// dialog from `gpui::test`.
    fn choose_local_png(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(test)]
        if !self.png_test_paths.is_empty() {
            let path = self.png_test_paths.remove(0);
            self.apply_local_png(path, cx);
            return;
        }
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a PNG".into()),
        };
        let receiver = cx.prompt_for_paths(options);
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |picker, cx| match result {
                Ok(Ok(Some(mut paths))) if !paths.is_empty() => {
                    picker.apply_local_png(paths.remove(0), cx)
                }
                Ok(Ok(_)) => {}
                Ok(Err(_)) | Err(_) => {
                    picker.png_error = Some("Could not open the file picker.".to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn apply_local_png(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path.extension().and_then(|extension| extension.to_str()) != Some("png") {
            self.png_error = Some("Choose a .png file.".to_string());
            cx.notify();
            return;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.png_error = Some("Could not read that file.".to_string());
                cx.notify();
                return;
            }
        };
        if bytes.len() as u64 > MAX_AVATAR_PNG_BYTES {
            self.png_error = Some(format!(
                "That PNG is over the {}KB limit.",
                MAX_AVATAR_PNG_BYTES / 1024
            ));
            cx.notify();
            return;
        }
        if !bytes.starts_with(&PNG_SIGNATURE) {
            self.png_error = Some("That file is not a valid PNG.".to_string());
            cx.notify();
            return;
        }
        self.png_error = None;
        self.commit(ProjectIconValue::Avatar(AvatarSource::LocalPng(path)), cx);
    }

    fn render_icon_mode(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let selected_glyph = match self.value.value {
            ProjectIconValue::Symbol(glyph) => Some(glyph),
            _ => None,
        };
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap(px(theme.cosmic.spacing.xs as f32));
        for glyph in ProjectGlyph::ALL {
            let glyph_entity = entity.clone();
            let active = selected_glyph == Some(glyph);
            let id = format!("project-icon-glyph-{}", glyph.id());
            grid = grid.child(
                div()
                    .id(id.clone())
                    .debug_selector(move || id.clone())
                    .w(px(36.0))
                    .h(px(36.0))
                    .rounded(theme.radii.control)
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_2()
                    .border_color(if active {
                        theme.selection_ring
                    } else {
                        theme.hairline
                    })
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        glyph_entity.update(cx, |picker, cx| picker.select_glyph(glyph, cx));
                    })
                    .child(
                        IconElement::new(glyph.icon(), px(16.0))
                            .text_color(self.value.tint.resolve(theme)),
                    ),
            );
        }

        let palette: Vec<(&'static str, gpui::Rgba)> = AgentAccentColor::ALL
            .iter()
            .map(|choice| (choice.id(), choice.resolve(theme)))
            .collect();
        let tint_entity = entity.clone();
        let tint_selected = self.value.tint;
        let tint_picker = controls::color_picker(
            "project-icon-tint",
            &palette,
            tint_selected.id(),
            theme,
            move |id, cx| {
                tint_entity.update(cx, |picker, cx| {
                    picker.set_tint(AgentAccentColor::parse(id), cx)
                });
            },
        );

        let reset_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(theme.cosmic.spacing.s as f32))
            .child(grid)
            .child(controls::row("Colour", None, tint_picker, theme))
            .child(controls::action_row(
                controls::button("project-icon-reset", "Reset", theme, move |_, _, cx| {
                    reset_entity.update(cx, |picker, cx| picker.reset(cx));
                }),
                theme,
            ))
    }

    fn render_emoji_mode(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let click_entity = entity.clone();
        let key_entity = entity.clone();
        let field = div()
            .id("project-icon-emoji-field")
            .debug_selector(|| "project-icon-emoji-field".into())
            .track_focus(&self.emoji_focus)
            .w(px(96.0))
            .min_h(px(32.0))
            .px(px(theme.cosmic.spacing.xs as f32))
            .rounded(theme.radii.control)
            .bg(theme.filter_field_bg)
            .border_1()
            .border_color(theme.hairline)
            .text_size(theme.typography.headline)
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_entity.update(cx, |picker, cx| picker.emoji_focus.focus(window, cx));
            })
            .on_key_down(move |event, window, cx| {
                key_entity.update(cx, |picker, cx| picker.on_emoji_key(event, window, cx));
            })
            .child(text!(
                id = "project-icon-emoji-draft",
                self.emoji_draft.clone()
            ));

        let commit_entity = entity.clone();
        let open_picker_entity = entity.clone();

        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(theme.cosmic.spacing.s as f32))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(theme.cosmic.spacing.xs as f32))
                    .child(field)
                    .child(controls::button(
                        "project-icon-emoji-set",
                        "Set Emoji",
                        theme,
                        move |_, _, cx| {
                            commit_entity.update(cx, |picker, cx| picker.commit_emoji(cx));
                        },
                    ))
                    .child(controls::button(
                        "project-icon-emoji-open-picker",
                        "Open Emoji Picker",
                        theme,
                        move |_, window, cx| {
                            open_picker_entity
                                .update(cx, |picker, cx| picker.open_emoji_grid(window, cx));
                        },
                    )),
            );

        if let Some(error) = &self.emoji_error {
            column = column.child(
                div()
                    .id("project-icon-emoji-error")
                    .debug_selector(|| "project-icon-emoji-error".into())
                    .text_size(theme.typography.footnote)
                    .text_color(theme.diff_deletion)
                    .child(text!(id = "project-icon-emoji-error-text", error.clone())),
            );
        }
        if self.emoji_grid_open {
            column = column.child(self.render_emoji_grid(theme, entity));
        }
        column
    }

    /// F-PRJ-16's searchable grid, rendered as a normal (non-absolute)
    /// child appended after the field/button row -- deliberately not
    /// `.absolute()`, so it can't repeat F-PRJ-01's paint-order bug
    /// (an overlay painted before later siblings, so they draw over it).
    fn render_emoji_grid(&self, theme: Theme, entity: Entity<Self>) -> impl IntoElement {
        let query_focus_entity = entity.clone();
        let query_key_entity = entity.clone();
        let close_entity = entity.clone();
        let query = self.emoji_grid_query.to_lowercase();
        let matches: Vec<&'static str> = EMOJI_CHOICES
            .iter()
            .filter(|(emoji, keywords)| {
                query.is_empty()
                    || *emoji == query
                    || keywords.iter().any(|keyword| keyword.contains(&query))
            })
            .map(|(emoji, _)| *emoji)
            .collect();

        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap(px(theme.cosmic.spacing.xxs as f32));
        for emoji in &matches {
            let emoji = *emoji;
            let pick_entity = entity.clone();
            let selector = format!("project-icon-emoji-grid-choice-{emoji}");
            grid = grid.child(
                div()
                    .id(selector.clone())
                    .debug_selector(move || selector.clone())
                    .w(px(32.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.headline)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        pick_entity.update(cx, |picker, cx| picker.pick_emoji_from_grid(emoji, cx));
                    })
                    .child(emoji),
            );
        }
        if matches.is_empty() {
            grid = grid.child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.meta)
                    .child("No matching emoji."),
            );
        }

        div()
            .id("project-icon-emoji-grid")
            .debug_selector(|| "project-icon-emoji-grid".into())
            .w_full()
            .p(px(theme.cosmic.spacing.xs as f32))
            .flex()
            .flex_col()
            .gap(px(theme.cosmic.spacing.xs as f32))
            .rounded(theme.radii.control)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(theme.cosmic.spacing.xs as f32))
                    .child(
                        div()
                            .id("project-icon-emoji-grid-query")
                            .debug_selector(|| "project-icon-emoji-grid-query".into())
                            .track_focus(&self.emoji_grid_focus)
                            .flex_1()
                            .min_h(px(28.0))
                            .px(px(theme.cosmic.spacing.xs as f32))
                            .flex()
                            .items_center()
                            .rounded(theme.radii.control)
                            .bg(theme.filter_field_bg)
                            .border_1()
                            .border_color(theme.hairline)
                            .text_size(theme.typography.footnote)
                            .text_color(if self.emoji_grid_query.is_empty() {
                                theme.meta
                            } else {
                                theme.title
                            })
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                query_focus_entity.update(cx, |picker, cx| {
                                    picker.emoji_grid_focus.focus(window, cx);
                                });
                            })
                            .on_key_down(move |event, window, cx| {
                                query_key_entity.update(cx, |picker, cx| {
                                    picker.on_emoji_grid_query_key(event, window, cx);
                                });
                            })
                            .child(if self.emoji_grid_query.is_empty() {
                                "Search emoji…".to_owned()
                            } else {
                                self.emoji_grid_query.clone()
                            }),
                    )
                    .child(
                        div()
                            .id("project-icon-emoji-grid-close")
                            .debug_selector(|| "project-icon-emoji-grid-close".into())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.meta)
                            .hover(|style| style.bg(theme.row_hover))
                            .on_click(move |_, _, cx| {
                                close_entity.update(cx, |picker, cx| picker.close_emoji_grid(cx));
                            })
                            .child("Close"),
                    ),
            )
            .child(grid)
    }

    fn render_avatar_mode(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let spacing = theme.cosmic.spacing;
        let choose_entity = entity.clone();
        let current_avatar_label = match &self.value.value {
            ProjectIconValue::Avatar(AvatarSource::LocalPng(path)) => Some(format!(
                "Current: {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned())
            )),
            ProjectIconValue::Avatar(AvatarSource::GitHub(id)) => {
                Some(format!("Current: GitHub avatar for {id}"))
            }
            ProjectIconValue::Avatar(AvatarSource::Favicon(domain)) => {
                Some(format!("Current: favicon for {domain}"))
            }
            _ => None,
        };

        let mut column =
            div()
                .flex()
                .flex_col()
                .gap(px(spacing.s as f32))
                .child(controls::action_row(
                    controls::button(
                        "project-icon-choose-png",
                        "Choose PNG…",
                        theme,
                        move |_, window, cx| {
                            choose_entity
                                .update(cx, |picker, cx| picker.choose_local_png(window, cx));
                        },
                    ),
                    theme,
                ));

        if let Some(error) = &self.png_error {
            column = column.child(
                div()
                    .id("project-icon-png-error")
                    .debug_selector(|| "project-icon-png-error".into())
                    .text_size(theme.typography.footnote)
                    .text_color(theme.diff_deletion)
                    .child(text!(id = "project-icon-png-error-text", error.clone())),
            );
        }

        column = column
            .child(controls::separator(theme))
            .child(self.render_source_field(
                "github",
                "GitHub user or repository",
                &self.github_draft,
                &self.github_focus,
                &self.github_error,
                entity.clone(),
                Self::on_github_key,
                Self::commit_github,
                "Use GitHub Avatar",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_source_field(
                "favicon",
                "Domain, like example.com",
                &self.favicon_draft,
                &self.favicon_focus,
                &self.favicon_error,
                entity.clone(),
                Self::on_favicon_key,
                Self::commit_favicon,
                "Use Favicon",
                theme,
            ));

        if let Some(label) = current_avatar_label {
            column = column.child(
                div()
                    .id("project-icon-avatar-current")
                    .debug_selector(|| "project-icon-avatar-current".into())
                    .text_size(theme.typography.footnote)
                    .text_color(theme.subtitle)
                    .child(text!(id = "project-icon-avatar-current-text", label)),
            );
        }
        column
    }

    #[allow(clippy::too_many_arguments)]
    fn render_source_field(
        &self,
        id_prefix: &'static str,
        placeholder: &'static str,
        draft: &str,
        focus: &FocusHandle,
        error: &Option<String>,
        entity: Entity<Self>,
        on_key: fn(&mut Self, &KeyDownEvent, &mut Window, &mut Context<Self>),
        on_commit: fn(&mut Self, &mut Context<Self>),
        button_label: &'static str,
        theme: Theme,
    ) -> gpui::Div {
        let field_id = format!("project-icon-{id_prefix}-field");
        let focus_handle = focus.clone();
        let key_entity = entity.clone();
        let commit_entity = entity.clone();
        let shown = if draft.is_empty() {
            placeholder.to_string()
        } else {
            draft.to_string()
        };
        let text_color = if draft.is_empty() {
            theme.subtitle
        } else {
            theme.title
        };

        let field = div()
            .id(field_id.clone())
            .debug_selector(move || field_id.clone())
            .track_focus(focus)
            .w(px(220.0))
            .min_h(px(32.0))
            .px(px(theme.cosmic.spacing.xs as f32))
            .rounded(theme.radii.control)
            .bg(theme.filter_field_bg)
            .border_1()
            .border_color(theme.hairline)
            .text_size(theme.typography.callout)
            .text_color(text_color)
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_handle.focus(window, cx);
            })
            .on_key_down(move |event, window, cx| {
                key_entity.update(cx, |picker, cx| on_key(picker, event, window, cx));
            })
            .child(text!(id = format!("project-icon-{id_prefix}-draft"), shown));

        let mut row = div()
            .flex()
            .items_center()
            .gap(px(theme.cosmic.spacing.xs as f32))
            .child(field)
            .child(controls::button(
                button_label,
                button_label,
                theme,
                move |_, _, cx| {
                    commit_entity.update(cx, on_commit);
                },
            ));

        if let Some(message) = error {
            let error_id = format!("project-icon-{id_prefix}-error");
            row = row.child(
                div()
                    .id(error_id.clone())
                    .debug_selector(move || error_id.clone())
                    .text_size(theme.typography.footnote)
                    .text_color(theme.diff_deletion)
                    .child(message.clone()),
            );
        }
        div().child(row)
    }
}

impl Render for ProjectIconPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();

        let mode_switch = controls::segmented(
            "project-icon-mode",
            PickerMode::OPTIONS,
            self.mode.index(),
            theme,
            {
                let entity = entity.clone();
                move |index, cx| {
                    entity.update(cx, |picker, cx| picker.set_mode(index, cx));
                }
            },
        );

        let body = match self.mode {
            PickerMode::Icon => self.render_icon_mode(theme, entity.clone()),
            PickerMode::Emoji => self.render_emoji_mode(theme, entity.clone()),
            PickerMode::Avatar => self.render_avatar_mode(theme, entity.clone()),
        };

        controls::card(theme)
            .child(controls::row("Project icon", None, mode_switch, theme))
            .child(controls::separator(theme))
            .child(div().p(px(theme.cosmic.spacing.s as f32)).child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    /// A scratch file that removes itself when the test finishes, mirroring
    /// `chat.rs`'s own `TempDir` — the same reasoning applies: nothing can
    /// drive the real file picker in `gpui::test`, but `apply_local_png`'s
    /// signature/size checks read real bytes off disk, so the test needs a
    /// real file.
    struct TempFile(PathBuf);

    impl TempFile {
        fn write(name: &str, bytes: &[u8]) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-project-identity-test-{}-{unique}-{name}",
                std::process::id()
            ));
            std::fs::write(&path, bytes).expect("write temp file");
            Self(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[allow(clippy::type_complexity)]
    fn picker_view_with_capture(
        cx: &mut TestAppContext,
    ) -> (
        Entity<ProjectIconPicker>,
        Rc<RefCell<Vec<ProjectIcon>>>,
        &mut VisualTestContext,
    ) {
        cx.update(Theme::init);
        let captured = Rc::new(RefCell::new(Vec::new()));
        let for_closure = captured.clone();
        let (picker, cx) = cx.add_window_view(move |_, cx| {
            ProjectIconPicker::new(cx).on_change(move |value| {
                for_closure.borrow_mut().push(value);
            })
        });
        (picker, captured, cx)
    }

    #[test]
    fn default_project_icon_is_the_undyed_folder_glyph() {
        let icon = ProjectIcon::default();
        assert_eq!(icon.value, ProjectIconValue::Symbol(ProjectGlyph::Folder));
        assert_eq!(icon.tint, AgentAccentColor::Coral);
    }

    #[test]
    fn project_glyph_ids_round_trip_through_parse() {
        // F-PRJ-15: every offered glyph must persist and reload as itself,
        // and an unrecognised stored id must not panic or silently guess.
        for glyph in ProjectGlyph::ALL {
            assert_eq!(ProjectGlyph::parse(glyph.id()), Some(glyph));
        }
        assert_eq!(ProjectGlyph::parse("not-a-glyph"), None);
    }

    #[test]
    fn project_icon_storage_round_trips_symbol_emoji_and_avatar_variants() {
        let icons = [
            ProjectIcon {
                value: ProjectIconValue::Symbol(ProjectGlyph::GitBranch),
                tint: AgentAccentColor::Green,
            },
            ProjectIcon {
                value: ProjectIconValue::Emoji("🇮🇹".into()),
                tint: AgentAccentColor::Blue,
            },
            ProjectIcon {
                value: ProjectIconValue::Avatar(AvatarSource::GitHub("octocat".into())),
                tint: AgentAccentColor::Purple,
            },
        ];

        for icon in icons {
            let (kind, value) = icon.persisted_parts();
            let restored =
                ProjectIcon::from_persisted_parts(&kind, value.as_deref(), Some(icon.tint.id()));
            assert_eq!(restored, icon, "failed to round-trip {kind:?}/{value:?}");
        }
    }

    /// F-PRJ-13/F-PRJ-15: the glyph grid and the colour row are two
    /// independent controls — picking a glyph must not touch the tint, and
    /// picking a tint must not touch the glyph. Both fire `on_change`, drawn
    /// and clicked exactly as the picker's own consumer would.
    #[gpui::test]
    async fn icon_mode_selecting_a_glyph_and_a_tint_fires_on_change_independently(
        cx: &mut TestAppContext,
    ) {
        let (_picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let glyph = cx
            .debug_bounds("project-icon-glyph-git-branch")
            .expect("the git-branch swatch is drawn");
        cx.simulate_click(glyph.center(), Modifiers::none());
        cx.run_until_parked();

        {
            let history = captured.borrow();
            let last = history.last().expect("selecting a glyph fires on_change");
            assert_eq!(
                last.value,
                ProjectIconValue::Symbol(ProjectGlyph::GitBranch)
            );
            assert_eq!(last.tint, AgentAccentColor::Coral, "tint is untouched");
        }

        refresh_frame(cx);
        let tint = cx
            .debug_bounds("project-icon-tint-green")
            .expect("the green tint swatch is drawn");
        cx.simulate_click(tint.center(), Modifiers::none());
        cx.run_until_parked();

        let history = captured.borrow();
        let last = history.last().expect("selecting a tint fires on_change");
        assert_eq!(
            last.value,
            ProjectIconValue::Symbol(ProjectGlyph::GitBranch),
            "the glyph chosen a moment ago is untouched by the tint change"
        );
        assert_eq!(last.tint, AgentAccentColor::Green);
    }

    /// F-PRJ-13: Reset discards whatever source and tint were chosen and
    /// returns to the undyed folder glyph, and switches the picker back to
    /// the Icon tab so the reset result is what is on screen.
    #[gpui::test]
    async fn reset_button_restores_the_default_value(cx: &mut TestAppContext) {
        let (_picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let globe = cx
            .debug_bounds("project-icon-glyph-globe")
            .expect("the globe swatch is drawn");
        cx.simulate_click(globe.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            captured.borrow().last().unwrap().value,
            ProjectIconValue::Symbol(ProjectGlyph::Globe)
        );

        refresh_frame(cx);
        let reset = cx
            .debug_bounds("project-icon-reset")
            .expect("the reset control is drawn");
        cx.simulate_click(reset.center(), Modifiers::none());
        cx.run_until_parked();

        let last = captured.borrow().last().unwrap().clone();
        assert_eq!(last, ProjectIcon::default());
    }

    /// F-PRJ-16: a single grapheme cluster is accepted even when it spans
    /// more than one `char` (a flag), and anything else is rejected with a
    /// visible, drawn error rather than silently truncated or accepted.
    #[gpui::test]
    async fn emoji_mode_rejects_multi_character_and_accepts_one_grapheme(cx: &mut TestAppContext) {
        let (_picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let emoji_tab = cx
            .debug_bounds("project-icon-mode-1")
            .expect("the Emoji tab is drawn");
        cx.simulate_click(emoji_tab.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);

        let field = cx
            .debug_bounds("project-icon-emoji-field")
            .expect("the emoji field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("ab");

        let set = cx
            .debug_bounds("project-icon-emoji-set")
            .expect("the Set Emoji control is drawn");
        cx.simulate_click(set.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-emoji-error").is_some(),
            "multi-character input is rejected with a drawn error"
        );
        assert!(
            captured.borrow().is_empty(),
            "an invalid emoji never reaches on_change"
        );

        refresh_frame(cx);
        // Backspace the rejected draft, then enter one real, multi-codepoint
        // grapheme: the Italian flag is two regional-indicator `char`s but a
        // single grapheme cluster, exactly the case a naive `.chars().count()`
        // check would wrongly reject.
        let field = cx
            .debug_bounds("project-icon-emoji-field")
            .expect("the emoji field is still drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("backspace backspace");
        cx.simulate_input("🇮🇹");

        let set = cx
            .debug_bounds("project-icon-emoji-set")
            .expect("the Set Emoji control is drawn");
        cx.simulate_click(set.center(), Modifiers::none());
        cx.run_until_parked();

        let last = captured.borrow().last().cloned();
        assert_eq!(
            last.map(|icon| icon.value),
            Some(ProjectIconValue::Emoji("🇮🇹".to_string())),
            "a single real-world grapheme cluster is accepted"
        );
    }

    /// F-PRJ-14: the local-PNG source is the one avatar path this crate
    /// resolves on its own — a real signature check, a real size cap, both
    /// exercised against real bytes on disk via the same test-path seam
    /// `chat.rs::attach_image` uses.
    #[gpui::test]
    async fn avatar_mode_local_png_validates_signature_and_size_before_committing(
        cx: &mut TestAppContext,
    ) {
        let good = TempFile::write("good.png", &PNG_SIGNATURE);
        let bad_signature = TempFile::write("bad-signature.png", b"not a png");
        let oversize_bytes = vec![0u8; MAX_AVATAR_PNG_BYTES as usize + 1];
        let oversize = TempFile::write("oversize.png", &oversize_bytes);

        let (picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let avatar_tab = cx
            .debug_bounds("project-icon-mode-2")
            .expect("the Avatar tab is drawn");
        cx.simulate_click(avatar_tab.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);

        let choose = cx
            .debug_bounds("project-icon-choose-png")
            .expect("the Choose PNG control is drawn");

        picker.update(cx, |picker, _| {
            picker.queue_test_png_path(bad_signature.0.clone())
        });
        cx.simulate_click(choose.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-png-error").is_some(),
            "a file without the PNG signature is rejected"
        );
        assert!(captured.borrow().is_empty());

        refresh_frame(cx);
        picker.update(cx, |picker, _| {
            picker.queue_test_png_path(oversize.0.clone())
        });
        cx.simulate_click(choose.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-png-error").is_some(),
            "a file over the size cap is rejected even with a valid signature check pending"
        );
        assert!(captured.borrow().is_empty());

        refresh_frame(cx);
        picker.update(cx, |picker, _| picker.queue_test_png_path(good.0.clone()));
        cx.simulate_click(choose.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-png-error").is_none(),
            "a valid small PNG clears any prior error"
        );
        assert_eq!(
            captured.borrow().last().map(|icon| icon.value.clone()),
            Some(ProjectIconValue::Avatar(AvatarSource::LocalPng(
                good.0.clone()
            )))
        );
    }

    /// F-PRJ-14: the GitHub and favicon fields validate their input and
    /// commit a descriptor — this crate does not resolve either into pixels
    /// (see the module doc's seam), so what commits here is the source
    /// description, not an image.
    #[gpui::test]
    async fn avatar_mode_github_and_favicon_fields_validate_before_committing(
        cx: &mut TestAppContext,
    ) {
        let (_picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let avatar_tab = cx
            .debug_bounds("project-icon-mode-2")
            .expect("the Avatar tab is drawn");
        cx.simulate_click(avatar_tab.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);

        let github_field = cx
            .debug_bounds("project-icon-github-field")
            .expect("the GitHub field is drawn");
        cx.simulate_click(github_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("has a space");
        let use_github = cx
            .debug_bounds("Use GitHub Avatar")
            .expect("the Use GitHub Avatar control is drawn");
        cx.simulate_click(use_github.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-github-error").is_some(),
            "whitespace in a GitHub identifier is rejected"
        );
        assert!(captured.borrow().is_empty());

        refresh_frame(cx);
        let github_field = cx
            .debug_bounds("project-icon-github-field")
            .expect("the GitHub field is still drawn");
        cx.simulate_click(github_field.center(), Modifiers::none());
        cx.run_until_parked();
        for _ in 0.."has a space".len() {
            cx.simulate_keystrokes("backspace");
        }
        cx.simulate_input("octocat");
        let use_github = cx
            .debug_bounds("Use GitHub Avatar")
            .expect("the Use GitHub Avatar control is drawn");
        cx.simulate_click(use_github.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            captured.borrow().last().map(|icon| icon.value.clone()),
            Some(ProjectIconValue::Avatar(AvatarSource::GitHub(
                "octocat".to_string()
            )))
        );

        refresh_frame(cx);
        let favicon_field = cx
            .debug_bounds("project-icon-favicon-field")
            .expect("the favicon field is drawn");
        cx.simulate_click(favicon_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("nodot");
        let use_favicon = cx
            .debug_bounds("Use Favicon")
            .expect("the Use Favicon control is drawn");
        cx.simulate_click(use_favicon.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-favicon-error").is_some(),
            "a domain without a dot is rejected"
        );

        refresh_frame(cx);
        let favicon_field = cx
            .debug_bounds("project-icon-favicon-field")
            .expect("the favicon field is still drawn");
        cx.simulate_click(favicon_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input(".com");
        let use_favicon = cx
            .debug_bounds("Use Favicon")
            .expect("the Use Favicon control is drawn");
        cx.simulate_click(use_favicon.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            captured.borrow().last().map(|icon| icon.value.clone()),
            Some(ProjectIconValue::Avatar(AvatarSource::Favicon(
                "nodot.com".to_string()
            )))
        );
    }

    /// F-PRJ-16: clicking "Open Emoji Picker" with the field empty used to
    /// have no observable effect at all (the button's only wiring was a
    /// host callback nothing in `rust/` ever chained). It must now open a
    /// real, searchable grid; typing narrows it; and clicking a swatch
    /// commits that emoji and closes the grid.
    #[gpui::test]
    async fn open_emoji_picker_opens_a_searchable_grid_that_commits_on_click(
        cx: &mut TestAppContext,
    ) {
        let (_picker, captured, cx) = picker_view_with_capture(cx);
        cx.update(|window, _| window.refresh());

        let emoji_tab = cx
            .debug_bounds("project-icon-mode-1")
            .expect("the Emoji tab is drawn");
        cx.simulate_click(emoji_tab.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);

        assert!(
            cx.debug_bounds("project-icon-emoji-grid").is_none(),
            "the grid is closed until Open Emoji Picker is clicked"
        );

        let open_picker = cx
            .debug_bounds("project-icon-emoji-open-picker")
            .expect("the open-picker control is drawn");
        cx.simulate_click(open_picker.center(), Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("project-icon-emoji-grid").is_some(),
            "clicking Open Emoji Picker with an empty field opens the grid overlay"
        );
        assert!(
            cx.debug_bounds("project-icon-emoji-grid-choice-🚀").is_some(),
            "the unfiltered grid offers its curated choices"
        );

        let query = cx
            .debug_bounds("project-icon-emoji-grid-query")
            .expect("the grid's search field is drawn");
        cx.simulate_click(query.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("rocket");
        cx.run_until_parked();
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("project-icon-emoji-grid-choice-🚀").is_some(),
            "searching \"rocket\" keeps the matching swatch"
        );
        assert!(
            cx.debug_bounds("project-icon-emoji-grid-choice-🐍").is_none(),
            "searching \"rocket\" filters out unrelated swatches"
        );

        let rocket = cx
            .debug_bounds("project-icon-emoji-grid-choice-🚀")
            .expect("the rocket swatch is drawn");
        cx.simulate_click(rocket.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            captured.borrow().last().map(|icon| icon.value.clone()),
            Some(ProjectIconValue::Emoji("🚀".to_string())),
            "clicking a swatch commits that emoji"
        );
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("project-icon-emoji-grid").is_none(),
            "picking a swatch closes the grid"
        );
    }

    fn refresh_frame(cx: &mut VisualTestContext) {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

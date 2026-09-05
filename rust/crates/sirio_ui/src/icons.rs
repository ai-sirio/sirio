//! Icons for the UI.
//!
//! # Two families, one typed enum
//!
//! [`Icon`] resolves through the pinned, vendored Zed SVG catalog on every
//! platform. GPUI's stock `svg()` element provides the normal monochrome,
//! caller-tinted rendering path.
//!
//! # Agent marks are different
//!
//! The agent brand marks (Claude Code, Codex, OpenCode, Pi, omp) are
//! embedded SVGs on every platform, taken from icon libraries rather than
//! the Zed catalog: Claude and OpenAI from Microsoft's Codicons
//! (`rust/assets/icons/codicons/`), OpenCode and Pi from Simple Icons
//! (`rust/assets/icons/simple-icons/`). No library carries an Oh My Pi
//! mark, so omp keeps Sirio's own asset.
//! Oh My Pi's three-stop gradient is rendered **full-colour, never tinted**:
//! its brand logo keeps its identity and must not be recoloured by a theme.
//! The four library marks are single `currentColor` paths and go through
//! the normal tinted path like the reference does.
//!
//! The SVG bytes are embedded with `include_bytes!`, so icons ship inside
//! the binary. Colour comes from the caller's `text_color` — which must come
//! from `Theme::get(cx)` — never from a hardcoded value, or light mode breaks
//! for that icon alone.

use gpui::{
    App, AssetSource, Bounds, IntoElement, Pixels, Refineable as _, RenderImage, RenderOnce, Rgba,
    SharedString, StyleRefinement, Styled, Window, canvas, px, svg,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use sirio_project::FileIconKey;

use sirio_theme::AgentBrandColor;

/// The named icon set. `path` is the asset file name served by
/// [`SirioAssets`]; `svg` is the embedded byte payload.
///
/// # Pinned Zed catalog
///
/// Generic icons resolve to byte-identical SVGs vendored from Zed's pinned
/// upstream catalog; see `rust/assets/icons/zed/ATTRIBUTION.md`. Agent marks
/// come from Codicons and Simple Icons (each directory carries its own
/// `ATTRIBUTION.md`); Oh My Pi keeps its Sirio asset because no library
/// provides its mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// A project directory (`zed/folder.svg`).
    FolderFill,
    /// A git worktree (`zed/git_branch.svg`).
    GitBranch,
    /// A chat surface (`zed/chat.svg`).
    MessageSquare,
    /// A terminal surface (`zed/terminal.svg`).
    SquareTerminal,
    /// Close (`zed/close.svg`).
    Close,
    /// Collapse (`zed/chevron_down.svg`).
    ChevronDown,
    /// Step up (`zed/chevron_up.svg`).
    ChevronUp,
    /// Expand (`zed/chevron_right.svg`).
    ChevronRight,
    /// Back (`zed/chevron_left.svg`).
    ChevronLeft,
    /// Settings gear (`zed/settings.svg`).
    Settings,
    /// Refresh (`zed/rotate_cw.svg`).
    RefreshCw,
    /// Add (`zed/plus.svg`).
    Plus,
    /// A generic file (`zed/file.svg`).
    File,
    /// AI providers (Zed `sparkle`).
    Sparkles,
    /// Permissions (Zed `lock`).
    Shield,
    /// Appearance (Zed `screen`).
    SunMoon,
    /// A browser surface (`zed/public.svg`).
    Globe,
    /// Anthropic's Claude mark, monochrome (`codicons/claude.svg`).
    ClaudeCode,
    /// OpenAI's knot, monochrome (`codicons/openai.svg`), like the Swift
    /// app's `.primary` rendering.
    Codex,
    /// OpenCode's nested-frame mark, monochrome
    /// (`simple-icons/opencode.svg`).
    OpenCode,
    /// pi.dev's monogram, monochrome (`simple-icons/pi.svg`).
    Pi,
    /// Oh-My-Pi's mark with the pink→purple→cyan gradient, ported from
    /// `App/AgentIcon.swift` — full colour. It remains a Sirio fallback
    /// because Zed does not provide an Oh My Pi mark.
    OhMyPi,
    /// Left-sidebar toggle (`zed/threads_sidebar_left_open.svg`).
    SidebarLeft,
    /// Right-panel toggle (`zed/threads_sidebar_right_open.svg`).
    PanelRight,
    /// An archive file in the Files tree — zip, tar, 7z, … (`zed/archive.svg`).
    /// Added for F-CORE-FILE-08 (`FileIconKey::Archive`); no Phosphor predecessor.
    Archive,
    /// A lock file in the Files tree — `Cargo.lock`, `package-lock.json`, …
    /// (`zed/lock.svg`). Added for F-CORE-FILE-08 (`FileIconKey::Lock`);
    /// no Phosphor predecessor.
    Lock,
    /// The Files view in the right panel's rail (`zed/file_tree.svg`).
    FileTree,
    /// The Activity view in the right panel's rail (`zed/thread.svg`).
    Thread,
    /// The Diff view in the right panel's rail (`zed/diff.svg`).
    Diff,
    /// The History view in the right panel's rail (`zed/git_graph.svg`).
    GitGraph,
    /// Unified Changes view (`zed/diff_unified.svg`).
    DiffUnified,
    /// Split Changes view (`zed/diff_split.svg`).
    DiffSplit,
    /// Expand vertically (`zed/expand_vertical.svg`).
    ExpandVertical,
    /// Fold vertically (`zed/fold_vertical.svg`).
    FoldVertical,
    /// Add a square item (`zed/square_plus.svg`).
    SquarePlus,
    /// Remove a square item (`zed/square_minus.svg`).
    SquareMinus,
    /// Undo (`zed/undo.svg`).
    Undo,
    /// A full-colour Material icon for a file type.
    FileType(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSize {
    XSmall,
    Small,
    Medium,
    Custom(Pixels),
}

impl IconSize {
    pub fn resolve(self, typography: sirio_theme::Typography) -> Pixels {
        let delta = f32::from(typography.base_size) - 13.5;
        match self {
            Self::XSmall => px((12.0 + delta).max(6.0)),
            Self::Small => px((14.0 + delta).max(6.0)),
            Self::Medium => px((16.0 + delta).max(6.0)),
            Self::Custom(size) => size,
        }
    }
}

impl Icon {
    /// Constructs a full-colour Material file-type icon.
    pub const fn file_type(name: &'static str) -> Self {
        Self::FileType(name)
    }

    /// The asset path (also the file name inside `rust/assets/icons`).
    pub fn path(self) -> &'static str {
        match self {
            Icon::FolderFill => "icons/zed/folder.svg",
            Icon::GitBranch => "icons/zed/git_branch.svg",
            Icon::MessageSquare => "icons/zed/chat.svg",
            Icon::SquareTerminal => "icons/zed/terminal.svg",
            Icon::Close => "icons/zed/close.svg",
            Icon::ChevronDown => "icons/zed/chevron_down.svg",
            Icon::ChevronUp => "icons/zed/chevron_up.svg",
            Icon::ChevronRight => "icons/zed/chevron_right.svg",
            Icon::ChevronLeft => "icons/zed/chevron_left.svg",
            Icon::Settings => "icons/zed/settings.svg",
            Icon::RefreshCw => "icons/zed/rotate_cw.svg",
            Icon::Plus => "icons/zed/plus.svg",
            Icon::File => "icons/zed/file.svg",
            Icon::Sparkles => "icons/zed/sparkle.svg",
            Icon::Shield => "icons/zed/lock.svg",
            Icon::SunMoon => "icons/zed/screen.svg",
            Icon::Globe => "icons/zed/public.svg",
            Icon::ClaudeCode => "icons/codicons/claude.svg",
            Icon::Codex => "icons/codicons/openai.svg",
            Icon::OpenCode => "icons/simple-icons/opencode.svg",
            Icon::Pi => "icons/simple-icons/pi.svg",
            Icon::OhMyPi => "icons/agent-omp.svg",
            Icon::SidebarLeft => "icons/zed/threads_sidebar_left_open.svg",
            Icon::PanelRight => "icons/zed/threads_sidebar_right_open.svg",
            Icon::Archive => "icons/zed/archive.svg",
            Icon::Lock => "icons/zed/lock.svg",
            Icon::FileTree => "icons/zed/file_tree.svg",
            Icon::Thread => "icons/zed/thread.svg",
            Icon::Diff => "icons/zed/diff.svg",
            Icon::GitGraph => "icons/zed/git_graph.svg",
            Icon::DiffUnified => "icons/zed/diff_unified.svg",
            Icon::DiffSplit => "icons/zed/diff_split.svg",
            Icon::ExpandVertical => "icons/zed/expand_vertical.svg",
            Icon::FoldVertical => "icons/zed/fold_vertical.svg",
            Icon::SquarePlus => "icons/zed/square_plus.svg",
            Icon::SquareMinus => "icons/zed/square_minus.svg",
            Icon::Undo => "icons/zed/undo.svg",
            Icon::FileType(name) => match name {
                "audio" => "icons/file-types/audio.svg",
                "c" => "icons/file-types/c.svg",
                "console" => "icons/file-types/console.svg",
                "cpp" => "icons/file-types/cpp.svg",
                "csharp" => "icons/file-types/csharp.svg",
                "css" => "icons/file-types/css.svg",
                "database" => "icons/file-types/database.svg",
                "docker" => "icons/file-types/docker.svg",
                "document" => "icons/file-types/document.svg",
                "font" => "icons/file-types/font.svg",
                "git" => "icons/file-types/git.svg",
                "go" => "icons/file-types/go.svg",
                "html" => "icons/file-types/html.svg",
                "image" => "icons/file-types/image.svg",
                "java" => "icons/file-types/java.svg",
                "javascript" => "icons/file-types/javascript.svg",
                "json" => "icons/file-types/json.svg",
                "kotlin" => "icons/file-types/kotlin.svg",
                "lock" => "icons/file-types/lock.svg",
                "log" => "icons/file-types/log.svg",
                "makefile" => "icons/file-types/makefile.svg",
                "markdown" => "icons/file-types/markdown.svg",
                "pdf" => "icons/file-types/pdf.svg",
                "python" => "icons/file-types/python.svg",
                "react" => "icons/file-types/react.svg",
                "ruby" => "icons/file-types/ruby.svg",
                "rust" => "icons/file-types/rust.svg",
                "sass" => "icons/file-types/sass.svg",
                "settings" => "icons/file-types/settings.svg",
                "swift" => "icons/file-types/swift.svg",
                "toml" => "icons/file-types/toml.svg",
                "typescript" => "icons/file-types/typescript.svg",
                "video" => "icons/file-types/video.svg",
                "vue" => "icons/file-types/vue.svg",
                "xml" => "icons/file-types/xml.svg",
                "yaml" => "icons/file-types/yaml.svg",
                "zip" => "icons/file-types/zip.svg",
                _ => "icons/file-types/document.svg",
            },
        }
    }

    /// The embedded SVG payload.
    pub fn svg(self) -> &'static [u8] {
        match self {
            Icon::FolderFill => include_bytes!("../../../assets/icons/zed/folder.svg"),
            Icon::GitBranch => include_bytes!("../../../assets/icons/zed/git_branch.svg"),
            Icon::MessageSquare => include_bytes!("../../../assets/icons/zed/chat.svg"),
            Icon::SquareTerminal => include_bytes!("../../../assets/icons/zed/terminal.svg"),
            Icon::Close => include_bytes!("../../../assets/icons/zed/close.svg"),
            Icon::ChevronDown => include_bytes!("../../../assets/icons/zed/chevron_down.svg"),
            Icon::ChevronUp => include_bytes!("../../../assets/icons/zed/chevron_up.svg"),
            Icon::ChevronRight => include_bytes!("../../../assets/icons/zed/chevron_right.svg"),
            Icon::ChevronLeft => include_bytes!("../../../assets/icons/zed/chevron_left.svg"),
            Icon::Settings => include_bytes!("../../../assets/icons/zed/settings.svg"),
            Icon::RefreshCw => include_bytes!("../../../assets/icons/zed/rotate_cw.svg"),
            Icon::Plus => include_bytes!("../../../assets/icons/zed/plus.svg"),
            Icon::File => include_bytes!("../../../assets/icons/zed/file.svg"),
            Icon::Sparkles => include_bytes!("../../../assets/icons/zed/sparkle.svg"),
            Icon::Shield => include_bytes!("../../../assets/icons/zed/lock.svg"),
            Icon::SunMoon => include_bytes!("../../../assets/icons/zed/screen.svg"),
            Icon::Globe => include_bytes!("../../../assets/icons/zed/public.svg"),
            Icon::ClaudeCode => include_bytes!("../../../assets/icons/codicons/claude.svg"),
            Icon::Codex => include_bytes!("../../../assets/icons/codicons/openai.svg"),
            Icon::OpenCode => include_bytes!("../../../assets/icons/simple-icons/opencode.svg"),
            Icon::Pi => include_bytes!("../../../assets/icons/simple-icons/pi.svg"),
            Icon::OhMyPi => include_bytes!("../../../assets/icons/agent-omp.svg"),
            Icon::SidebarLeft => {
                include_bytes!("../../../assets/icons/zed/threads_sidebar_left_open.svg")
            }
            Icon::PanelRight => {
                include_bytes!("../../../assets/icons/zed/threads_sidebar_right_open.svg")
            }
            Icon::Archive => include_bytes!("../../../assets/icons/zed/archive.svg"),
            Icon::Lock => include_bytes!("../../../assets/icons/zed/lock.svg"),
            Icon::FileTree => include_bytes!("../../../assets/icons/zed/file_tree.svg"),
            Icon::Thread => include_bytes!("../../../assets/icons/zed/thread.svg"),
            Icon::Diff => include_bytes!("../../../assets/icons/zed/diff.svg"),
            Icon::GitGraph => include_bytes!("../../../assets/icons/zed/git_graph.svg"),
            Icon::DiffUnified => include_bytes!("../../../assets/icons/zed/diff_unified.svg"),
            Icon::DiffSplit => include_bytes!("../../../assets/icons/zed/diff_split.svg"),
            Icon::ExpandVertical => include_bytes!("../../../assets/icons/zed/expand_vertical.svg"),
            Icon::FoldVertical => include_bytes!("../../../assets/icons/zed/fold_vertical.svg"),
            Icon::SquarePlus => include_bytes!("../../../assets/icons/zed/square_plus.svg"),
            Icon::SquareMinus => include_bytes!("../../../assets/icons/zed/square_minus.svg"),
            Icon::Undo => include_bytes!("../../../assets/icons/zed/undo.svg"),
            Icon::FileType(name) => match name {
                "audio" => include_bytes!("../../../assets/icons/file-types/audio.svg"),
                "c" => include_bytes!("../../../assets/icons/file-types/c.svg"),
                "console" => include_bytes!("../../../assets/icons/file-types/console.svg"),
                "cpp" => include_bytes!("../../../assets/icons/file-types/cpp.svg"),
                "csharp" => include_bytes!("../../../assets/icons/file-types/csharp.svg"),
                "css" => include_bytes!("../../../assets/icons/file-types/css.svg"),
                "database" => include_bytes!("../../../assets/icons/file-types/database.svg"),
                "docker" => include_bytes!("../../../assets/icons/file-types/docker.svg"),
                "document" => include_bytes!("../../../assets/icons/file-types/document.svg"),
                "font" => include_bytes!("../../../assets/icons/file-types/font.svg"),
                "git" => include_bytes!("../../../assets/icons/file-types/git.svg"),
                "go" => include_bytes!("../../../assets/icons/file-types/go.svg"),
                "html" => include_bytes!("../../../assets/icons/file-types/html.svg"),
                "image" => include_bytes!("../../../assets/icons/file-types/image.svg"),
                "java" => include_bytes!("../../../assets/icons/file-types/java.svg"),
                "javascript" => include_bytes!("../../../assets/icons/file-types/javascript.svg"),
                "json" => include_bytes!("../../../assets/icons/file-types/json.svg"),
                "kotlin" => include_bytes!("../../../assets/icons/file-types/kotlin.svg"),
                "lock" => include_bytes!("../../../assets/icons/file-types/lock.svg"),
                "log" => include_bytes!("../../../assets/icons/file-types/log.svg"),
                "makefile" => include_bytes!("../../../assets/icons/file-types/makefile.svg"),
                "markdown" => include_bytes!("../../../assets/icons/file-types/markdown.svg"),
                "pdf" => include_bytes!("../../../assets/icons/file-types/pdf.svg"),
                "python" => include_bytes!("../../../assets/icons/file-types/python.svg"),
                "react" => include_bytes!("../../../assets/icons/file-types/react.svg"),
                "ruby" => include_bytes!("../../../assets/icons/file-types/ruby.svg"),
                "rust" => include_bytes!("../../../assets/icons/file-types/rust.svg"),
                "sass" => include_bytes!("../../../assets/icons/file-types/sass.svg"),
                "settings" => include_bytes!("../../../assets/icons/file-types/settings.svg"),
                "swift" => include_bytes!("../../../assets/icons/file-types/swift.svg"),
                "toml" => include_bytes!("../../../assets/icons/file-types/toml.svg"),
                "typescript" => include_bytes!("../../../assets/icons/file-types/typescript.svg"),
                "video" => include_bytes!("../../../assets/icons/file-types/video.svg"),
                "vue" => include_bytes!("../../../assets/icons/file-types/vue.svg"),
                "xml" => include_bytes!("../../../assets/icons/file-types/xml.svg"),
                "yaml" => include_bytes!("../../../assets/icons/file-types/yaml.svg"),
                "zip" => include_bytes!("../../../assets/icons/file-types/zip.svg"),
                _ => include_bytes!("../../../assets/icons/file-types/document.svg"),
            },
        }
    }

    /// Whether this icon is an agent brand mark. Marks are embedded SVGs on
    /// every platform and never go through a platform-specific symbol path.
    pub fn is_agent_mark(self) -> bool {
        matches!(
            self,
            Icon::ClaudeCode | Icon::Codex | Icon::OpenCode | Icon::Pi | Icon::OhMyPi
        )
    }

    /// Whether the mark carries its own chromatic colours (omp's gradient,
    /// and every Material file-type asset's baked-in fills). Such marks are
    /// painted full-colour and never tinted by the theme; the remaining
    /// marks are monochrome by design and follow the tinted path, exactly
    /// as the Swift app renders them with `.primary`.
    ///
    /// Only `OhMyPi` and the `FileType` set stay chromatic. The Codicons and
    /// Simple Icons marks are monochrome and follow the theme tint.
    pub fn has_own_colours(self) -> bool {
        matches!(self, Icon::OhMyPi | Icon::FileType(_))
    }

    /// Resolves the stable icon for a catalog agent id.
    pub fn for_agent_id(id: &str) -> Option<Self> {
        match id.strip_suffix("-acp").unwrap_or(id) {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::OpenCode),
            "pi" => Some(Self::Pi),
            "omp" => Some(Self::OhMyPi),
            _ => None,
        }
    }

    /// The colour a catalog agent's brand mark wears when it stands for the
    /// agent itself, taken from each project's own published identity:
    ///
    /// - **Claude** — Anthropic's Claude orange `#D97757` (the "Crail"
    ///   accent claude.ai and code.claude.com brand their mark with), which
    ///   is exactly [`AgentBrandColor::Claude`].
    /// - **Oh-My-Pi** — its mark *is* its colours: the pink→purple→cyan
    ///   gradient baked into the asset. `None` — it must never be tinted.
    /// - **Codex, OpenCode, Pi** — all three publish strictly monochrome
    ///   marks (OpenAI's black-on-white knot; opencode.ai/brand's grey
    ///   `#211E1E`/`#CFCECD` wordmarks with no chromatic accent anywhere in
    ///   the guidelines; pi.dev's `logo-auto.svg`, black on light and white
    ///   on dark). Their original colour is therefore the UI's own
    ///   foreground, passed in as `theme_title`, so they stay legible in
    ///   both appearances — which is precisely how those projects present
    ///   the marks themselves.
    ///
    /// `None` for every non-agent icon: tinting is the caller's business.
    /// Note this answers *mark* colour only. [`AgentBrandColor`] remains
    /// the activity-accent table (running dots, worktree badges) whose hues
    /// are chosen to stay distinguishable at sidebar scale, not to quote
    /// the brands.
    pub fn agent_mark_color(self, theme_title: Rgba) -> Option<Rgba> {
        match self {
            Icon::OhMyPi => None,
            Icon::ClaudeCode => Some(AgentBrandColor::Claude.color()),
            Icon::Codex | Icon::OpenCode | Icon::Pi => Some(theme_title),
            _ => None,
        }
    }

    /// Builds a sized element; colour is applied by the caller with
    /// [`Styled::text_color`] (theme-sourced).
    pub fn element(self, size: IconSize) -> IconElement {
        IconElement::new(self, size)
    }
}

/// The per-type file glyph for a document. The Files tree row and the tab
/// that shows the same file both draw this one mark, so a name looks the
/// same in both places.
///
/// The classification — which name or extension gets which *logical* icon
/// key — is [`FileIconKey`], ported 1:1 from the original's
/// `FileIconKey.swift` (F-CORE-FILE-08): see that type for the exact
/// exact-name/extension/directory-name tables and their fallback rule.
///
/// The *rendering* of each logical key is necessarily narrower than the
/// original's: the pinned `rust/assets/icons/zed/` catalog ships a focused
/// set of general-purpose UI glyphs, not a
/// per-language icon font, so most [`FileIconKey`] variants collapse onto
/// the generic [`Icon::File`] / [`Icon::FolderFill`] marks below rather than
/// getting an invented shape that doesn't exist in the pinned catalog.
/// Only the handful of keys with an unambiguous Zed shape (a terminal for
/// shell scripts, a branch for git files, a gear for env/settings, an
/// archive box, a key for lock files) get their own icon.
pub fn file_glyph(path: &Path, is_dir: bool) -> Icon {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key = if is_dir {
        FileIconKey::for_directory_name(&name)
    } else {
        FileIconKey::for_file_name(&name)
    };
    if !is_dir && let Some(asset) = key.material_asset() {
        return Icon::file_type(asset);
    }
    match key {
        FileIconKey::Shell => Icon::SquareTerminal,
        FileIconKey::Git | FileIconKey::FolderGit => Icon::GitBranch,
        FileIconKey::Env | FileIconKey::Settings => Icon::Settings,
        FileIconKey::Archive => Icon::Archive,
        FileIconKey::Lock => Icon::Lock,
        // Every other file key — the per-language kinds (Swift, Python,
        // Rust, …), markup/data kinds (Json, Yaml, Markdown, …), media
        // kinds (Image, Video, Audio, Font) and the remaining exact-name
        // kinds (Docker, Makefile, Sql, Database, Log) — has no dedicated
        // glyph in the approved Zed subset and shares the generic file mark.
        FileIconKey::Swift
        | FileIconKey::C
        | FileIconKey::Cpp
        | FileIconKey::CSharp
        | FileIconKey::Java
        | FileIconKey::Kotlin
        | FileIconKey::Python
        | FileIconKey::Ruby
        | FileIconKey::Rust
        | FileIconKey::Go
        | FileIconKey::JavaScript
        | FileIconKey::TypeScript
        | FileIconKey::React
        | FileIconKey::Vue
        | FileIconKey::Html
        | FileIconKey::Css
        | FileIconKey::Sass
        | FileIconKey::Json
        | FileIconKey::Yaml
        | FileIconKey::Toml
        | FileIconKey::Xml
        | FileIconKey::Markdown
        | FileIconKey::Text
        | FileIconKey::Pdf
        | FileIconKey::Image
        | FileIconKey::Video
        | FileIconKey::Audio
        | FileIconKey::Font
        | FileIconKey::Sql
        | FileIconKey::Database
        | FileIconKey::Docker
        | FileIconKey::Log
        | FileIconKey::Makefile
        | FileIconKey::File
        | FileIconKey::Symlink => Icon::File,
        // Every folder key beyond `.git` (Src, Tests, Docs, Github,
        // NodeModules, Dist, Scripts, Config, Assets, Public, Packages,
        // Vscode, Lib, Tools, and the plain default) shares the folder
        // mark: the approved Zed subset has one folder shape, not fifteen.
        FileIconKey::Folder
        | FileIconKey::FolderSrc
        | FileIconKey::FolderTests
        | FileIconKey::FolderDocs
        | FileIconKey::FolderGithub
        | FileIconKey::FolderNodeModules
        | FileIconKey::FolderDist
        | FileIconKey::FolderScripts
        | FileIconKey::FolderConfig
        | FileIconKey::FolderAssets
        | FileIconKey::FolderPublic
        | FileIconKey::FolderPackages
        | FileIconKey::FolderVscode
        | FileIconKey::FolderLib
        | FileIconKey::FolderTools => Icon::FolderFill,
    }
}

/// A lightweight adapter around GPUI's stock SVG element.
#[derive(IntoElement)]
pub struct IconElement {
    style: StyleRefinement,
    icon: Icon,
    size: IconSize,
}

impl IconElement {
    pub fn new(icon: Icon, size: IconSize) -> Self {
        Self {
            style: StyleRefinement::default(),
            icon,
            size,
        }
    }
}

/// Path 1: chromatic agent marks, painted full-colour with their own
/// colours. The SVG is rasterized once per (icon, pixel size) and
/// cached; the resulting `RenderImage` is painted into `bounds`.
fn paint_agent_mark(icon: Icon, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    // Render at 2x the point size, matching GPUI's own SVG renderer
    // (`SMOOTH_SVG_SCALE_FACTOR`): pixel-perfect on retina displays.
    let pixel = (f32::from(bounds.size.width) * 2.0).round().max(1.0);
    let key = (icon, pixel as u32);
    let image = agent_mark_cache().get_or_insert(key, || {
        let view_box = view_box_size(icon);
        // render_single_frame multiplies its scale argument by its own
        // 2x factor, so divide it back out.
        let scale = pixel / view_box / 2.0;
        cx.svg_renderer()
            .render_single_frame(icon.svg(), scale)
            .ok()
    });
    let Some(image) = image else {
        return;
    };
    let _ = window.paint_image(bounds, bounds, Default::default(), image, 0, false);
}

impl Styled for IconElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for IconElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size.resolve(sirio_theme::Theme::get(cx).typography);
        if self.icon.has_own_colours() {
            let icon = self.icon;
            let mut element = canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| paint_agent_mark(icon, bounds, window, cx),
            )
            .size(size)
            .flex_none();
            element.style().refine(&self.style);
            element.into_any_element()
        } else {
            let mut element = svg()
                .size(size)
                .flex_none()
                .path(self.icon.path())
                .data(self.icon.svg())
                .text_color(window.text_style().color);
            element.style().refine(&self.style);
            element.into_any_element()
        }
    }
}

/// Cache of full-colour mark rasters, keyed by (icon, pixel size). The
/// sprite atlas caches the texture upload by image id, but the resvg
/// rasterization itself is per-frame unless cached here.
fn agent_mark_cache() -> &'static AgentMarkCache {
    static CACHE: OnceLock<AgentMarkCache> = OnceLock::new();
    CACHE.get_or_init(AgentMarkCache::new)
}

struct AgentMarkCache(Mutex<HashMap<(Icon, u32), Arc<RenderImage>>>);

impl AgentMarkCache {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    fn get_or_insert(
        &self,
        key: (Icon, u32),
        rasterize: impl FnOnce() -> Option<Arc<RenderImage>>,
    ) -> Option<Arc<RenderImage>> {
        if let Ok(cache) = self.0.lock()
            && let Some(image) = cache.get(&key)
        {
            return Some(image.clone());
        }
        let image = rasterize()?;
        if let Ok(mut cache) = self.0.lock() {
            cache.insert(key, image.clone());
        }
        Some(image)
    }
}

/// The largest viewBox dimension of an icon's SVG, in user units. Used to
/// derive the raster scale for a target pixel size.
fn view_box_size(icon: Icon) -> f32 {
    let text = std::str::from_utf8(icon.svg()).unwrap_or_default();
    let view_box = text
        .split("viewBox=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or("0 0 24 24");
    let parts: Vec<f32> = view_box
        .split_whitespace()
        .filter_map(|part| part.parse().ok())
        .collect();
    match parts.as_slice() {
        [_, _, w, h] => w.max(*h),
        _ => 24.0,
    }
}

/// The embedded asset source, for hosts that use GPUI's stock `svg()`
/// element (`application().with_assets(SirioAssets)`). Every icon in the
/// enum is served; `list` reports the icon directory.
pub struct SirioAssets;

impl AssetSource for SirioAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(ALL_ICONS
            .iter()
            .copied()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.svg())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        if path == "icons" || path.is_empty() {
            Ok(ALL_ICONS.iter().map(|icon| icon.path().into()).collect())
        } else {
            Ok(Vec::new())
        }
    }
}

/// Every icon, used by [`SirioAssets::list`] and by tests.
pub const ALL_ICONS: [Icon; 33] = [
    Icon::FolderFill,
    Icon::GitBranch,
    Icon::MessageSquare,
    Icon::SquareTerminal,
    Icon::Close,
    Icon::ChevronDown,
    Icon::ChevronUp,
    Icon::ChevronRight,
    Icon::ChevronLeft,
    Icon::Settings,
    Icon::RefreshCw,
    Icon::Plus,
    Icon::File,
    Icon::Sparkles,
    Icon::Shield,
    Icon::SunMoon,
    Icon::Globe,
    Icon::ClaudeCode,
    Icon::Codex,
    Icon::OpenCode,
    Icon::Pi,
    Icon::OhMyPi,
    Icon::SidebarLeft,
    Icon::PanelRight,
    Icon::Archive,
    Icon::Lock,
    Icon::DiffUnified,
    Icon::DiffSplit,
    Icon::ExpandVertical,
    Icon::FoldVertical,
    Icon::SquarePlus,
    Icon::SquareMinus,
    Icon::Undo,
];

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn every_icon_has_embedded_svg_payload() {
        for icon in ALL_ICONS {
            let payload = icon.svg();
            assert!(
                !payload.is_empty() && payload.len() > 100,
                "{} embeds a real SVG, got {} bytes",
                icon.path(),
                payload.len()
            );
            let text = std::str::from_utf8(payload).expect("svg is utf-8");
            assert!(
                text.contains("<svg"),
                "{} starts with an svg root",
                icon.path()
            );
        }
    }

    #[test]
    fn representative_material_file_icons_are_embedded() {
        for icon in [
            Icon::file_type("java"),
            Icon::file_type("python"),
            Icon::file_type("rust"),
        ] {
            let bytes = icon.svg();
            assert!(!bytes.is_empty());
            assert_eq!(bytes[0], b'<');
        }
    }

    #[test]
    fn catalog_uses_the_approved_zed_and_sirio_assets() {
        let expected = [
            (Icon::FolderFill, "icons/zed/folder.svg"),
            (Icon::GitBranch, "icons/zed/git_branch.svg"),
            (Icon::MessageSquare, "icons/zed/chat.svg"),
            (Icon::SquareTerminal, "icons/zed/terminal.svg"),
            (Icon::Close, "icons/zed/close.svg"),
            (Icon::ChevronDown, "icons/zed/chevron_down.svg"),
            (Icon::ChevronUp, "icons/zed/chevron_up.svg"),
            (Icon::ChevronRight, "icons/zed/chevron_right.svg"),
            (Icon::ChevronLeft, "icons/zed/chevron_left.svg"),
            (Icon::Settings, "icons/zed/settings.svg"),
            (Icon::RefreshCw, "icons/zed/rotate_cw.svg"),
            (Icon::Plus, "icons/zed/plus.svg"),
            (Icon::File, "icons/zed/file.svg"),
            (Icon::Sparkles, "icons/zed/sparkle.svg"),
            (Icon::Shield, "icons/zed/lock.svg"),
            (Icon::SunMoon, "icons/zed/screen.svg"),
            (Icon::Globe, "icons/zed/public.svg"),
            (Icon::ClaudeCode, "icons/codicons/claude.svg"),
            (Icon::Codex, "icons/codicons/openai.svg"),
            (Icon::OpenCode, "icons/simple-icons/opencode.svg"),
            (Icon::Pi, "icons/simple-icons/pi.svg"),
            (Icon::OhMyPi, "icons/agent-omp.svg"),
            (Icon::SidebarLeft, "icons/zed/threads_sidebar_left_open.svg"),
            (Icon::PanelRight, "icons/zed/threads_sidebar_right_open.svg"),
            (Icon::Archive, "icons/zed/archive.svg"),
            (Icon::Lock, "icons/zed/lock.svg"),
            (Icon::DiffUnified, "icons/zed/diff_unified.svg"),
            (Icon::DiffSplit, "icons/zed/diff_split.svg"),
            (Icon::ExpandVertical, "icons/zed/expand_vertical.svg"),
            (Icon::FoldVertical, "icons/zed/fold_vertical.svg"),
            (Icon::SquarePlus, "icons/zed/square_plus.svg"),
            (Icon::SquareMinus, "icons/zed/square_minus.svg"),
            (Icon::Undo, "icons/zed/undo.svg"),
        ];

        assert_eq!(expected.len(), ALL_ICONS.len());
        for (icon, path) in expected {
            assert_eq!(icon.path(), path, "wrong asset for {icon:?}");
        }
    }

    #[test]
    fn every_zed_icon_keeps_its_upstream_16px_canvas() {
        for icon in ALL_ICONS {
            if !icon.path().starts_with("icons/zed/") {
                continue;
            }
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            let has_16px_dimensions = svg.contains("width=\"16\"") && svg.contains("height=\"16\"");
            let has_16px_view_box = svg.contains("viewBox=\"0 0 16 16\"");
            assert!(
                has_16px_dimensions || has_16px_view_box,
                "{} must keep Zed's 16px canvas",
                icon.path()
            );
        }
    }

    #[test]
    fn agent_marks_come_from_codicons_simple_icons_and_sirio_for_omp() {
        // Codicons ships Claude and OpenAI; Simple Icons ships OpenCode and
        // Pi; no library carries Oh My Pi, so its Sirio gradient stays.
        assert_eq!(Icon::ClaudeCode.path(), "icons/codicons/claude.svg");
        assert_eq!(Icon::Codex.path(), "icons/codicons/openai.svg");
        assert_eq!(Icon::OpenCode.path(), "icons/simple-icons/opencode.svg");
        assert_eq!(Icon::Pi.path(), "icons/simple-icons/pi.svg");
        assert_eq!(Icon::OhMyPi.path(), "icons/agent-omp.svg");
    }

    #[test]
    fn asset_source_serves_every_icon_by_path() {
        let assets = SirioAssets;
        for icon in ALL_ICONS {
            let loaded = assets.load(icon.path()).expect("load does not fail");
            assert!(
                loaded.is_some(),
                "{} is served by the asset source",
                icon.path()
            );
            assert_eq!(loaded.unwrap().as_ref(), icon.svg());
        }
        assert!(assets.load("icons/does-not-exist.svg").unwrap().is_none());
    }

    #[test]
    fn catalog_path_aliases_are_deliberate() {
        let mut seen = std::collections::HashSet::new();
        for icon in ALL_ICONS {
            if !seen.insert(icon.path()) {
                assert!(
                    matches!(icon, Icon::Lock),
                    "unexpected duplicate path {}",
                    icon.path()
                );
                assert_eq!(icon.path(), Icon::Shield.path());
            }
        }
    }

    #[test]
    fn catalog_agents_resolve_to_their_brand_marks() {
        assert_eq!(Icon::for_agent_id("claude"), Some(Icon::ClaudeCode));
        assert_eq!(Icon::for_agent_id("codex-acp"), Some(Icon::Codex));
        assert_eq!(Icon::for_agent_id("opencode"), Some(Icon::OpenCode));
        assert_eq!(Icon::for_agent_id("pi"), Some(Icon::Pi));
        assert_eq!(Icon::for_agent_id("omp"), Some(Icon::OhMyPi));
        assert_eq!(Icon::for_agent_id("unknown"), None);
    }

    #[test]
    fn agent_marks_wear_their_published_brand_colours() {
        let foreground = gpui::rgb(0x11_12_13);
        // Claude's mark wears Anthropic's Claude orange.
        assert_eq!(
            Icon::ClaudeCode.agent_mark_color(foreground),
            Some(gpui::rgb(0xD97757)),
            "claude must wear #D97757, not a theme token"
        );
        // Codex, OpenCode and Pi publish monochrome marks only; their
        // original colour is the UI foreground, adaptive per appearance.
        for icon in [Icon::Codex, Icon::OpenCode, Icon::Pi] {
            assert_eq!(
                icon.agent_mark_color(foreground),
                Some(foreground),
                "{icon:?} is a monochrome brand and must take the foreground"
            );
        }
        // omp paints itself with its baked gradient — never tinted.
        assert_eq!(Icon::OhMyPi.agent_mark_color(foreground), None);
        // Non-agent icons are nobody's brand.
        assert_eq!(Icon::FolderFill.agent_mark_color(foreground), None);
    }

    #[test]
    fn semantic_icon_sizes_follow_sirio_typography_scale() {
        // Glyphs ride `base_size`, so the app-wide +1px on the type
        // scale reaches them too — that is the property this pins.
        // Were it to break, icons would shrink against the text they
        // label rather than staying proportional to it.
        let default = sirio_theme::Typography::default_scale();
        assert_eq!(IconSize::XSmall.resolve(default), px(13.0));
        assert_eq!(IconSize::Small.resolve(default), px(15.0));
        assert_eq!(IconSize::Medium.resolve(default), px(17.0));
        assert_eq!(IconSize::Custom(px(32.0)).resolve(default), px(32.0));

        let enlarged = sirio_theme::Typography::for_base_size(15.5);
        assert_eq!(IconSize::XSmall.resolve(enlarged), px(14.0));
        assert_eq!(IconSize::Small.resolve(enlarged), px(16.0));
        assert_eq!(IconSize::Medium.resolve(enlarged), px(18.0));
    }

    #[test]
    fn chromatic_marks_carry_their_own_colours() {
        // Oh My Pi is the sole full-colour fallback; its gradient asset is
        // baked in and never resolved through currentColor.
        let omp = std::str::from_utf8(Icon::OhMyPi.svg()).expect("utf-8");
        assert!(omp.contains("linearGradient"), "omp is a gradient mark");
        assert!(omp.contains("#ED4ABF") && omp.contains("#9B4DFF") && omp.contains("#5AD8E6"));
        assert!(
            !omp.contains("currentColor"),
            "omp must not resolve through the theme tint"
        );
    }

    #[test]
    fn monochrome_agent_marks_use_the_approved_sources() {
        // Every library mark is a single `currentColor` path, so it rides
        // GPUI's tinted svg path and never the full-colour raster.
        for (icon, library) in [
            (Icon::ClaudeCode, "icons/codicons/"),
            (Icon::Codex, "icons/codicons/"),
            (Icon::OpenCode, "icons/simple-icons/"),
            (Icon::Pi, "icons/simple-icons/"),
        ] {
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                icon.path().starts_with(library),
                "{icon:?} must come from {library}, got {}",
                icon.path()
            );
            assert!(
                svg.contains("fill=\"currentColor\""),
                "{icon:?} must resolve its fill through currentColor"
            );
            assert!(
                !svg.contains("fill=\"#"),
                "{icon:?} must not bake a colour into the asset"
            );
            assert!(
                !icon.has_own_colours(),
                "{icon:?} must follow the theme tint"
            );
        }
    }

    #[test]
    fn library_marks_share_one_optical_margin() {
        // Codicons draw inside a padded 16px canvas; Simple Icons fill the
        // whole 24px box. The Simple Icons marks get a 2-unit margin on the
        // viewBox (geometry untouched) so OpenCode and Pi do not read larger
        // than Claude and Codex at the same IconSize.
        for icon in [Icon::ClaudeCode, Icon::Codex] {
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                svg.contains("viewBox=\"0 0 16 16\""),
                "{icon:?} keeps Codicons' 16px canvas"
            );
        }
        for icon in [Icon::OpenCode, Icon::Pi] {
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                svg.contains("viewBox=\"-2 -2 28 28\""),
                "{icon:?} wears the padded Simple Icons canvas"
            );
        }
    }

    #[test]
    fn only_oh_my_pi_uses_the_full_colour_path() {
        for icon in ALL_ICONS {
            assert_eq!(
                icon.has_own_colours(),
                icon == Icon::OhMyPi,
                "unexpected full-colour icon: {icon:?}"
            );
        }
    }

    #[test]
    fn material_file_type_icons_carry_their_own_colours() {
        // GPUI's stock svg element tints its whole render with the text
        // colour, so a FileType icon must take the full-colour raster path
        // to keep the baked-in fills its Material asset ships with.
        for icon in [Icon::file_type("rust"), Icon::file_type("javascript")] {
            assert!(
                icon.has_own_colours(),
                "{icon:?} must be painted full-colour, not tinted"
            );
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                svg.contains("fill=\"#"),
                "{icon:?} must bake its chromatic fills into the asset"
            );
        }
    }

    #[test]
    fn constructor_keeps_the_semantic_size_until_render() {
        let element = IconElement::new(Icon::FolderFill, IconSize::Small);
        assert_eq!(element.size, IconSize::Small);
    }

    #[test]
    fn view_box_size_parses_embedded_svgs() {
        assert!(
            (view_box_size(Icon::FolderFill) - 16.0).abs() < 1.0,
            "zed's 16x16 home format"
        );
        assert!(
            (view_box_size(Icon::Pi) - 28.0).abs() < 1.0,
            "pi's padded Simple Icons viewBox"
        );
    }

    #[test]
    fn the_panel_rail_icons_resolve_to_embedded_zed_assets() {
        for icon in [Icon::FileTree, Icon::Thread, Icon::Diff, Icon::GitGraph] {
            assert!(
                icon.path().starts_with("icons/zed/"),
                "{icon:?} must come from the Zed catalog"
            );
            assert!(!icon.svg().is_empty(), "{icon:?} must embed its bytes");
            assert!(
                icon.svg().starts_with(b"<svg"),
                "{icon:?} must embed an SVG document"
            );
        }
    }
}

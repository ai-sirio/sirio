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
//! The agent brand marks are embedded SVGs on every platform, taken from
//! one icon library rather than the Zed catalog: LobeHub's
//! [lobe-icons](https://github.com/lobehub/lobe-icons), vendored in
//! `rust/assets/icons/lobehub/`. One set, not several, is the point — it
//! carries every mark on the same 24-unit grid, so the marks share an
//! optical weight instead of each needing its own correction. No library
//! carries an Oh My Pi mark, so omp keeps Sirio's own asset.
//!
//! Two of them paint themselves and must never be recoloured by a theme:
//! omp's three-stop gradient, and Gemini's blue base under three gradient
//! overlays. The rest are single `currentColor` paths on the tinted path.
//! Claude is the seam between the two: its asset bakes `#D97757`, but it
//! rides the tinted path anyway, because the tint it is handed is that
//! same hex (see [`Icon::agent_mark_color`]).
//!
//! Gemini and Grok are vendored without an adapter behind them. Nothing in
//! `sirio_agents::ALL` produces those ids; [`Icon::for_agent_id`] maps them
//! for the registry, whose rows otherwise fall back to a generic sparkle.
//!
//! The SVG bytes are embedded with `include_bytes!`, so icons ship inside
//! the binary. Colour comes from the caller's `text_color` — which must come
//! from `Theme::get(cx)` — never from a hardcoded value, or light mode breaks
//! for that icon alone.

use gpui::{
    App, AssetSource, Bounds, IntoElement, Pixels, Refineable as _, RenderImage, RenderOnce, Rgba,
    SharedString, StyleRefinement, Styled, SvgSize, Window, canvas, px, size, svg,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use sirio_theme::AgentBrandColor;

/// The named icon set. `path` is the asset file name served by
/// [`SirioAssets`]; `svg` is the embedded byte payload.
///
/// # Pinned Zed catalog
///
/// Generic icons resolve to byte-identical SVGs vendored from Zed's pinned
/// upstream catalog; see `rust/assets/icons/zed/ATTRIBUTION.md`. Agent marks
/// come from LobeHub's lobe-icons (`rust/assets/icons/lobehub/`, which
/// carries its own `ATTRIBUTION.md`); Oh My Pi keeps its Sirio asset
/// because no library provides its mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// A project directory (`zed/folder.svg`).
    FolderFill,
    /// A project directory whose row is expanded (`zed/folder_open.svg`).
    /// Paired with [`Icon::FolderFill`]: the file tree swaps between the two
    /// instead of drawing a separate disclosure arrow.
    FolderOpen,
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
    /// Anthropic's Claude starburst (`lobehub/claude-color.svg`), the one
    /// mark whose asset bakes its colour and is tinted all the same.
    ClaudeCode,
    /// Codex's own mark, monochrome (`lobehub/codex.svg`). Not the colour
    /// variant: that one paints a white plate behind a gradient, which is
    /// illegible in dark mode and muddy at a sidebar's 15px.
    Codex,
    /// OpenCode's nested-frame mark, monochrome (`lobehub/opencode.svg`).
    OpenCode,
    /// pi.dev's monogram, monochrome (`lobehub/pi.svg`).
    Pi,
    /// Google's Gemini spark (`lobehub/gemini-color.svg`) — a `#3186FF`
    /// base under green, red and yellow gradient overlays, so it takes the
    /// full-colour path. No adapter produces this id; see the module docs.
    Gemini,
    /// xAI's Grok mark, monochrome (`lobehub/grok.svg`). No adapter
    /// produces this id either.
    Grok,
    /// Oh-My-Pi's mark with the pink→purple→cyan gradient, ported from
    /// `App/AgentIcon.swift` — full colour. It remains a Sirio fallback
    /// because Zed does not provide an Oh My Pi mark.
    OhMyPi,
    /// Left-sidebar toggle (`zed/threads_sidebar_left_open.svg`).
    SidebarLeft,
    /// Right-panel toggle (`zed/threads_sidebar_right_open.svg`).
    PanelRight,
    /// The Zed catalog's archive mark (`zed/archive.svg`). It was the Files
    /// tree's glyph for zip, tar, 7z, … until that tree moved to the Material
    /// theme, which draws `zip` instead; nothing draws it today.
    Archive,
    /// The Zed catalog's lock mark (`zed/lock.svg`). It was the Files tree's
    /// glyph for `Cargo.lock`, `package-lock.json`, … until that tree moved to
    /// the Material theme, which draws `lock` instead; nothing draws it today.
    Lock,
    /// The Files view in the right panel's rail (`zed/file_tree.svg`).
    FileTree,
    /// The Activity view in the right panel's rail (`zed/thread.svg`).
    Thread,
    /// The Diff view in the right panel's rail (`zed/diff.svg`).
    Diff,
    /// The History view in the right panel's rail (`zed/git_graph.svg`).
    GitGraph,
    /// The References view in the right panel's rail (`zed/magnifying_glass.svg`).
    MagnifyingGlass,
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
    /// The file editor's Markdown Preview mode (`zed/eye.svg`). Paired with
    /// [`Icon::Code`]: the two are the whole Markdown mode switch, which
    /// carries no text label of its own.
    Eye,
    /// Hide hidden entries in the Files tree (`zed/eye_off.svg`).
    EyeOff,
    /// The file editor's Markdown Code mode (`zed/code.svg`), the other
    /// half of that pair.
    Code,
    /// A full-colour Material file or folder icon, named by its asset stem
    /// (`rust`, `folder-src-open`). The stems and the bytes both come from
    /// `sirio_icons`; an unknown stem falls back to the theme's own `file`.
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
            Icon::FolderOpen => "icons/zed/folder_open.svg",
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
            Icon::ClaudeCode => "icons/lobehub/claude-color.svg",
            Icon::Codex => "icons/lobehub/codex.svg",
            Icon::OpenCode => "icons/lobehub/opencode.svg",
            Icon::Pi => "icons/lobehub/pi.svg",
            Icon::Gemini => "icons/lobehub/gemini-color.svg",
            Icon::Grok => "icons/lobehub/grok.svg",
            Icon::OhMyPi => "icons/agent-omp.svg",
            Icon::SidebarLeft => "icons/zed/threads_sidebar_left_open.svg",
            Icon::PanelRight => "icons/zed/threads_sidebar_right_open.svg",
            Icon::Archive => "icons/zed/archive.svg",
            Icon::Lock => "icons/zed/lock.svg",
            Icon::FileTree => "icons/zed/file_tree.svg",
            Icon::Thread => "icons/zed/thread.svg",
            Icon::Diff => "icons/zed/diff.svg",
            Icon::GitGraph => "icons/zed/git_graph.svg",
            Icon::MagnifyingGlass => "icons/zed/magnifying_glass.svg",
            Icon::DiffUnified => "icons/zed/diff_unified.svg",
            Icon::DiffSplit => "icons/zed/diff_split.svg",
            Icon::ExpandVertical => "icons/zed/expand_vertical.svg",
            Icon::FoldVertical => "icons/zed/fold_vertical.svg",
            Icon::SquarePlus => "icons/zed/square_plus.svg",
            Icon::SquareMinus => "icons/zed/square_minus.svg",
            Icon::Undo => "icons/zed/undo.svg",
            Icon::Eye => "icons/zed/eye.svg",
            Icon::EyeOff => "icons/zed/eye_off.svg",
            Icon::Code => "icons/zed/code.svg",
            Icon::FileType(stem) => sirio_icons::asset_path(stem)
                .or_else(|| sirio_icons::asset_path(sirio_icons::DEFAULT_FILE))
                .unwrap_or("icons/material/file.svg"),
        }
    }

    /// The embedded SVG payload.
    pub fn svg(self) -> &'static [u8] {
        match self {
            Icon::FolderFill => include_bytes!("../../../assets/icons/zed/folder.svg"),
            Icon::FolderOpen => include_bytes!("../../../assets/icons/zed/folder_open.svg"),
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
            Icon::ClaudeCode => include_bytes!("../../../assets/icons/lobehub/claude-color.svg"),
            Icon::Codex => include_bytes!("../../../assets/icons/lobehub/codex.svg"),
            Icon::OpenCode => include_bytes!("../../../assets/icons/lobehub/opencode.svg"),
            Icon::Pi => include_bytes!("../../../assets/icons/lobehub/pi.svg"),
            Icon::Gemini => include_bytes!("../../../assets/icons/lobehub/gemini-color.svg"),
            Icon::Grok => include_bytes!("../../../assets/icons/lobehub/grok.svg"),
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
            Icon::MagnifyingGlass => {
                include_bytes!("../../../assets/icons/zed/magnifying_glass.svg")
            }
            Icon::DiffUnified => include_bytes!("../../../assets/icons/zed/diff_unified.svg"),
            Icon::DiffSplit => include_bytes!("../../../assets/icons/zed/diff_split.svg"),
            Icon::ExpandVertical => include_bytes!("../../../assets/icons/zed/expand_vertical.svg"),
            Icon::FoldVertical => include_bytes!("../../../assets/icons/zed/fold_vertical.svg"),
            Icon::SquarePlus => include_bytes!("../../../assets/icons/zed/square_plus.svg"),
            Icon::SquareMinus => include_bytes!("../../../assets/icons/zed/square_minus.svg"),
            Icon::Undo => include_bytes!("../../../assets/icons/zed/undo.svg"),
            Icon::Eye => include_bytes!("../../../assets/icons/zed/eye.svg"),
            Icon::EyeOff => include_bytes!("../../../assets/icons/zed/eye_off.svg"),
            Icon::Code => include_bytes!("../../../assets/icons/zed/code.svg"),
            Icon::FileType(stem) => sirio_icons::asset(stem)
                .or_else(|| sirio_icons::asset(sirio_icons::DEFAULT_FILE))
                .unwrap_or(b""),
        }
    }

    /// Whether this icon is an agent brand mark. Marks are embedded SVGs on
    /// every platform and never go through a platform-specific symbol path.
    pub fn is_agent_mark(self) -> bool {
        matches!(
            self,
            Icon::ClaudeCode
                | Icon::Codex
                | Icon::OpenCode
                | Icon::Pi
                | Icon::Gemini
                | Icon::Grok
                | Icon::OhMyPi
        )
    }

    /// Whether the mark carries its own chromatic colours (omp's gradient,
    /// Gemini's overlays, and every Material file-type asset's baked-in
    /// fills). Such marks are painted full-colour and never tinted by the
    /// theme; the remaining marks are monochrome by design and follow the
    /// tinted path, exactly as the Swift app renders them with `.primary`.
    ///
    /// `ClaudeCode` is the exception that looks like a bug: its asset bakes
    /// `#D97757`, yet it stays on the tinted path, because that hex is the
    /// tint it is given. Same pixels, one fewer raster cache entry per
    /// size — and a test keeps the two from drifting apart.
    pub fn has_own_colours(self) -> bool {
        matches!(self, Icon::OhMyPi | Icon::Gemini | Icon::FileType(_))
    }

    /// The stem to draw in a given appearance. Upstream ships a `_light`
    /// companion for the icons whose dark form paints a near-white glyph;
    /// everything else serves both appearances from one asset.
    ///
    /// Takes the *resolved* [`sirio_theme::Appearance`], never
    /// `Theme::mode`: the mode is a preference and includes `System`.
    pub fn for_appearance(self, appearance: sirio_theme::Appearance) -> Self {
        match (self, appearance) {
            (Icon::FileType(stem), sirio_theme::Appearance::Light) => {
                Icon::FileType(sirio_icons::light_variant(stem).unwrap_or(stem))
            }
            _ => self,
        }
    }

    /// Resolves the stable icon for a catalog agent id.
    pub fn for_agent_id(id: &str) -> Option<Self> {
        match id.strip_suffix("-acp").unwrap_or(id) {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::OpenCode),
            "pi" => Some(Self::Pi),
            "omp" => Some(Self::OhMyPi),
            // Neither of these has an adapter. They answer for the
            // registry, where an unmapped id draws the generic sparkle.
            "gemini" => Some(Self::Gemini),
            "grok" => Some(Self::Grok),
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
    ///   **Gemini** is `None` for the same reason: a blue base under three
    ///   gradient overlays, which a tint would flatten into one silhouette.
    /// - **Codex, OpenCode, Pi, Grok** — all publish strictly monochrome
    ///   marks (Codex's black-on-white glyph, and xAI's; opencode.ai's
    ///   grey `#211E1E`/`#CFCECD` wordmarks with no chromatic accent in
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
            Icon::OhMyPi | Icon::Gemini => None,
            Icon::ClaudeCode => Some(AgentBrandColor::Claude.color()),
            Icon::Codex | Icon::OpenCode | Icon::Pi | Icon::Grok => Some(theme_title),
            _ => None,
        }
    }

    /// Builds a sized element; colour is applied by the caller with
    /// [`Styled::text_color`] (theme-sourced).
    pub fn element(self, size: IconSize) -> IconElement {
        IconElement::new(self, size)
    }
}

/// The per-type mark for a document. The Files tree row and the tab that
/// shows the same file both draw this one mark, so a name looks the same in
/// both places.
///
/// Classification is `sirio_icons`: the whole name, then the longest dotted
/// tail, then the theme's own `file`. Resolved from the name alone — never
/// from the file's content.
pub fn file_glyph(path: &Path) -> Icon {
    Icon::file_type(sirio_icons::for_file(&entry_name(path)))
}

/// The mark for a directory, in the state its row is drawn in. Upstream
/// ships collapsed and expanded as two assets per name, and the tree draws
/// no disclosure arrow — this swap *is* the disclosure control, so the name
/// has to reach both states.
pub fn folder_glyph(path: &Path, expanded: bool) -> Icon {
    Icon::file_type(sirio_icons::for_directory(&entry_name(path), expanded))
}

fn entry_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
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
    // Rasterize at 2x the point size, matching GPUI's own SVG renderer
    // (`SMOOTH_SVG_SCALE_FACTOR`): pixel-perfect on retina displays.
    //
    // The size is asked for in device pixels, not as a scale factor, and
    // that distinction is the whole correctness of this function. A scale
    // factor multiplies the size the *file* declares, and every asset here
    // declares `width="1em"` — which usvg resolves against its default 12pt
    // font size, never against the 24- or 64-unit viewBox the artwork is
    // drawn in. Scaling from that produced a 15px raster for a 30px box:
    // stretched to fit by `paint_image`, so it never failed, it just went
    // soft. `SvgSize::Size` asks for a width and keeps the aspect ratio.
    let pixel = (f32::from(bounds.size.width) * 2.0).round().max(1.0) as i32;
    let key = (icon, pixel as u32);
    let image = agent_mark_cache().get_or_insert(key, || {
        let renderer = cx.svg_renderer();
        let parsed = renderer.parse_svg(icon.svg()).ok()?;
        renderer
            .render_parsed(&parsed, SvgSize::Size(size(pixel.into(), pixel.into())))
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
        // `Theme::get` hands back a `&Theme` borrowed from `cx`, and the
        // canvas closure below takes `cx` of its own. Copy the two values
        // out so no borrow of `cx` is alive when the closure is built.
        let (typography, appearance) = {
            let theme = sirio_theme::Theme::get(cx);
            (theme.typography, theme.appearance)
        };
        let size = self.size.resolve(typography);
        // Resolved here, so the raster cache keys on the icon that is
        // actually painted: both appearances coexist and a theme switch
        // invalidates nothing.
        let icon = self.icon.for_appearance(appearance);
        if icon.has_own_colours() {
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
                .path(icon.path())
                .data(icon.svg())
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
pub const ALL_ICONS: [Icon; 40] = [
    Icon::FolderFill,
    Icon::FolderOpen,
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
    Icon::Gemini,
    Icon::Grok,
    Icon::OhMyPi,
    Icon::SidebarLeft,
    Icon::PanelRight,
    Icon::Archive,
    Icon::Lock,
    Icon::MagnifyingGlass,
    Icon::DiffUnified,
    Icon::DiffSplit,
    Icon::ExpandVertical,
    Icon::FoldVertical,
    Icon::SquarePlus,
    Icon::SquareMinus,
    Icon::Undo,
    Icon::Eye,
    Icon::EyeOff,
    Icon::Code,
];

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, px};

    /// The embedded payload must be the file its path names.
    ///
    /// `path()` and `svg()` are two independent match arms per variant, so
    /// nothing but this test ties them together: an `include_bytes!`
    /// pointing at the neighbouring file compiles, renders a real icon, and
    /// looks right everywhere except that it is the wrong picture. The
    /// folder pair is exactly the shape that invites the slip — `folder.svg`
    /// and `folder_open.svg`, one character apart at the call site.
    ///
    /// Only `icons/zed/` is walked: the agent marks live in sibling
    /// directories with their own provenance rules.
    #[test]
    fn every_zed_icon_embeds_the_file_its_path_names() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut checked = 0;
        for icon in ALL_ICONS {
            let Some(name) = icon.path().strip_prefix("icons/zed/") else {
                continue;
            };
            let on_disk = std::fs::read(assets.join("icons/zed").join(name))
                .unwrap_or_else(|error| panic!("{} is not vendored: {error}", icon.path()));
            assert_eq!(
                icon.svg(),
                on_disk.as_slice(),
                "{icon:?} embeds bytes other than {}",
                icon.path()
            );
            checked += 1;
        }
        assert!(
            checked >= 29,
            "expected the zed icons to be walked, saw {checked}"
        );
    }

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

    /// `path()` and `svg()` are independent match arms, and a `FileType`
    /// stem now comes from a generated table rather than a list somebody
    /// reads in review. So assert the two agree with the crate that owns
    /// the bytes, not with each other.
    #[test]
    fn file_type_icons_serve_the_vendored_material_asset() {
        for stem in ["rust", "python", "toml", "folder-src", "folder-src-open"] {
            let icon = Icon::file_type(stem);
            assert_eq!(
                icon.svg(),
                sirio_icons::asset(stem).expect("vendored"),
                "{stem:?} embeds the vendored bytes"
            );
            assert_eq!(icon.path(), format!("icons/material/{stem}.svg"));
        }
        // An unknown stem must not panic and must not draw nothing: it
        // falls back to the theme's own default file mark.
        let unknown = Icon::file_type("no-such-icon");
        assert_eq!(unknown.svg(), sirio_icons::asset("file").expect("vendored"));
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
            (Icon::ClaudeCode, "icons/lobehub/claude-color.svg"),
            (Icon::Codex, "icons/lobehub/codex.svg"),
            (Icon::OpenCode, "icons/lobehub/opencode.svg"),
            (Icon::Pi, "icons/lobehub/pi.svg"),
            (Icon::Gemini, "icons/lobehub/gemini-color.svg"),
            (Icon::Grok, "icons/lobehub/grok.svg"),
            (Icon::OhMyPi, "icons/agent-omp.svg"),
            (Icon::SidebarLeft, "icons/zed/threads_sidebar_left_open.svg"),
            (Icon::PanelRight, "icons/zed/threads_sidebar_right_open.svg"),
            (Icon::Archive, "icons/zed/archive.svg"),
            (Icon::Lock, "icons/zed/lock.svg"),
            (Icon::MagnifyingGlass, "icons/zed/magnifying_glass.svg"),
            (Icon::DiffUnified, "icons/zed/diff_unified.svg"),
            (Icon::DiffSplit, "icons/zed/diff_split.svg"),
            (Icon::ExpandVertical, "icons/zed/expand_vertical.svg"),
            (Icon::FoldVertical, "icons/zed/fold_vertical.svg"),
            (Icon::SquarePlus, "icons/zed/square_plus.svg"),
            (Icon::SquareMinus, "icons/zed/square_minus.svg"),
            (Icon::Undo, "icons/zed/undo.svg"),
            (Icon::Eye, "icons/zed/eye.svg"),
            (Icon::EyeOff, "icons/zed/eye_off.svg"),
            (Icon::Code, "icons/zed/code.svg"),
            (Icon::FolderOpen, "icons/zed/folder_open.svg"),
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
    fn agent_marks_come_from_lobehub_and_sirio_for_omp() {
        // One library carries every mark but one, which is why it was
        // chosen; no library carries Oh My Pi, so its Sirio gradient stays.
        assert_eq!(Icon::ClaudeCode.path(), "icons/lobehub/claude-color.svg");
        assert_eq!(Icon::Codex.path(), "icons/lobehub/codex.svg");
        assert_eq!(Icon::OpenCode.path(), "icons/lobehub/opencode.svg");
        assert_eq!(Icon::Pi.path(), "icons/lobehub/pi.svg");
        assert_eq!(Icon::Gemini.path(), "icons/lobehub/gemini-color.svg");
        assert_eq!(Icon::Grok.path(), "icons/lobehub/grok.svg");
        assert_eq!(Icon::OhMyPi.path(), "icons/agent-omp.svg");
    }

    /// The vendoring rule for `icons/lobehub/` is byte-for-byte: what the
    /// binary embeds must be what upstream served, so a refresh is a file
    /// swap and never a hand edit. The same slip the Zed walk guards
    /// against lives here too — `codex.svg` and a hypothetical
    /// `codex-color.svg` are one word apart at the `include_bytes!` site.
    #[test]
    fn every_lobehub_mark_embeds_the_file_its_path_names() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut checked = 0;
        for icon in ALL_ICONS {
            let Some(name) = icon.path().strip_prefix("icons/lobehub/") else {
                continue;
            };
            let on_disk = std::fs::read(assets.join("icons/lobehub").join(name))
                .unwrap_or_else(|error| panic!("{} is not vendored: {error}", icon.path()));
            assert_eq!(
                icon.svg(),
                on_disk.as_slice(),
                "{icon:?} embeds bytes other than {}",
                icon.path()
            );
            checked += 1;
        }
        assert_eq!(checked, 6, "the six LobeHub marks must all be walked");
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

    /// Gemini and Grok have no adapter in `sirio_agents::ALL`; nothing in
    /// the app produces those ids today. The mapping exists for the
    /// registry, whose rows resolve a mark by id and otherwise fall back to
    /// the generic sparkle — so a published `gemini` or `grok` agent draws
    /// its own brand without another code change.
    #[test]
    fn registry_only_agents_resolve_to_their_brand_marks() {
        assert_eq!(Icon::for_agent_id("gemini"), Some(Icon::Gemini));
        assert_eq!(Icon::for_agent_id("gemini-acp"), Some(Icon::Gemini));
        assert_eq!(Icon::for_agent_id("grok"), Some(Icon::Grok));
        assert!(
            !sirio_agents::ALL
                .iter()
                .any(|adapter| { matches!(adapter.id(), "gemini" | "grok") }),
            "these marks are dormant: an adapter would need its own brand colour too"
        );
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
        // Codex, OpenCode, Pi and Grok publish monochrome marks only; their
        // original colour is the UI foreground, adaptive per appearance.
        for icon in [Icon::Codex, Icon::OpenCode, Icon::Pi, Icon::Grok] {
            assert_eq!(
                icon.agent_mark_color(foreground),
                Some(foreground),
                "{icon:?} is a monochrome brand and must take the foreground"
            );
        }
        // omp and Gemini paint themselves — never tinted.
        assert_eq!(Icon::OhMyPi.agent_mark_color(foreground), None);
        assert_eq!(Icon::Gemini.agent_mark_color(foreground), None);
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
        // Oh My Pi's gradient asset is baked in and never resolved through
        // currentColor.
        let omp = std::str::from_utf8(Icon::OhMyPi.svg()).expect("utf-8");
        assert!(omp.contains("linearGradient"), "omp is a gradient mark");
        assert!(omp.contains("#ED4ABF") && omp.contains("#9B4DFF") && omp.contains("#5AD8E6"));
        assert!(
            !omp.contains("currentColor"),
            "omp must not resolve through the theme tint"
        );

        // Gemini is the other one: a `#3186FF` base under three gradient
        // overlays. Tinting it would flatten all four into one silhouette.
        let gemini = std::str::from_utf8(Icon::Gemini.svg()).expect("utf-8");
        assert!(gemini.contains("#3186FF"), "gemini keeps its blue base");
        for stop in ["#08B962", "#F94543", "#FABC12"] {
            assert!(
                gemini.contains(stop),
                "gemini keeps its {stop} gradient overlay"
            );
        }
        assert!(
            !gemini.contains("currentColor"),
            "gemini must not resolve through the theme tint"
        );
    }

    #[test]
    fn monochrome_agent_marks_use_the_approved_source() {
        // Each of these is a single `currentColor` path, so it rides GPUI's
        // tinted svg path and never the full-colour raster.
        for icon in [Icon::Codex, Icon::OpenCode, Icon::Pi, Icon::Grok] {
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                icon.path().starts_with("icons/lobehub/"),
                "{icon:?} must come from the one vendored set, got {}",
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

    /// Claude is the deliberate exception to the rule above: its asset
    /// bakes `#D97757` and it is still painted through the tinted path.
    /// That works because the tint it is given *is* that hex, so the two
    /// agree pixel for pixel — and it saves a raster cache entry per size.
    /// The day the brand colour and the asset diverge, this test is what
    /// fails instead of the colour silently coming from the wrong one.
    #[test]
    fn claudes_baked_colour_matches_the_tint_it_is_painted_with() {
        let svg = std::str::from_utf8(Icon::ClaudeCode.svg()).expect("svg is utf-8");
        assert!(
            svg.contains("fill=\"#D97757\""),
            "claude's asset bakes the brand orange"
        );
        assert!(
            !Icon::ClaudeCode.has_own_colours(),
            "claude still rides the tinted path"
        );
        assert_eq!(
            Icon::ClaudeCode.agent_mark_color(gpui::rgb(0x11_12_13)),
            Some(gpui::rgb(0xD97757)),
            "the tint must equal the hex baked into the asset"
        );
    }

    /// Taking every mark from one set is what buys a shared optical weight:
    /// the previous vendoring mixed two libraries and had to invent a
    /// `-2 -2 28 28` viewBox for half its marks to make them agree. Nothing
    /// here is re-boxed, and this test is what says so — if a future mark
    /// needs an adjustment, it is a recorded deviation in that directory's
    /// ATTRIBUTION.md, made visible by this failing.
    #[test]
    fn lobehub_marks_share_one_24px_canvas() {
        let mut checked = 0;
        for icon in ALL_ICONS {
            if !icon.path().starts_with("icons/lobehub/") {
                continue;
            }
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            assert!(
                svg.contains("viewBox=\"0 0 24 24\""),
                "{icon:?} must keep the set's 24px canvas",
            );
            checked += 1;
        }
        assert_eq!(checked, 6, "the six LobeHub marks must all be walked");
    }

    /// The two ways to paint an icon are chosen by `has_own_colours`, and
    /// the wrong choice fails silently: a chromatic asset on the tinted
    /// path renders as a flat silhouette, which looks like an icon and is
    /// not the brand. So for a brand mark the flag and the bytes must
    /// agree — with Claude the one documented exception, pinned by its own
    /// test above.
    ///
    /// Only marks are walked. The Zed catalog is the other convention
    /// entirely: eight of its glyphs carry design-time greys (`#DCE0E5`,
    /// `#C6CAD0`) that the mask path is *meant* to throw away, so chromatic
    /// bytes there say nothing about how the icon should be painted.
    #[test]
    fn every_chromatic_mark_declares_its_own_colours() {
        for icon in ALL_ICONS {
            if !icon.is_agent_mark() || icon == Icon::ClaudeCode {
                continue;
            }
            let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
            let is_chromatic = svg.contains("fill=\"#") || svg.contains("fill=\"url(#");
            assert_eq!(
                icon.has_own_colours(),
                is_chromatic,
                "{icon:?} paints itself {} but is flagged {}",
                if is_chromatic {
                    "in colour"
                } else {
                    "monochrome"
                },
                icon.has_own_colours(),
            );
        }
        // Both directions of the rule, named: nothing else is chromatic.
        for icon in [Icon::Gemini, Icon::OhMyPi] {
            assert!(icon.has_own_colours(), "{icon:?} is a full-colour mark");
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

    /// The full-colour path is what this change extends, and reading the
    /// file cannot tell you whether it works: the two ways it fails are
    /// both silent. A mark that rasterises smaller than the box it is
    /// painted into is stretched to fit — soft edges, no error — and a
    /// mark whose colours are lost still draws a shape. So drive GPUI's
    /// real renderer with `paint_agent_mark`'s own arithmetic.
    #[gpui::test]
    async fn full_colour_marks_rasterise_in_colour_at_the_size_they_are_painted(
        cx: &mut TestAppContext,
    ) {
        // `IconSize::Small` at the default type scale, on a retina display:
        // `paint_agent_mark`'s own arithmetic, kept in step with it.
        let pixel = 15 * 2;
        for icon in [Icon::Gemini, Icon::OhMyPi, Icon::file_type("rust")] {
            let image = cx
                .update(|cx| {
                    let renderer = cx.svg_renderer();
                    let parsed = renderer.parse_svg(icon.svg()).expect("the asset parses");
                    renderer.render_parsed(&parsed, SvgSize::Size(size(pixel.into(), pixel.into())))
                })
                .unwrap_or_else(|error| panic!("{icon:?} must rasterise: {error}"));

            let width = image.size(0).width.0;
            assert!(
                width >= pixel,
                "{icon:?} rasterises {width}px wide for a {pixel}px box, so it is stretched"
            );

            let bytes = image.as_bytes(0).expect("the frame carries pixels");
            let colours: std::collections::HashSet<[u8; 3]> = bytes
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            assert!(
                colours.len() > 8,
                "{icon:?} must paint its own colours, saw {} distinct",
                colours.len()
            );
        }
    }

    #[test]
    fn constructor_keeps_the_semantic_size_until_render() {
        let element = IconElement::new(Icon::FolderFill, IconSize::Small);
        assert_eq!(element.size, IconSize::Small);
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

    /// 49 vendored assets ship a light companion because their dark form paints a
    /// near-white glyph: `toml` is #cfd8dc, invisible on a light background,
    /// and `toml` is every Cargo.toml. Everything else serves both.
    #[test]
    fn light_appearance_swaps_only_the_stems_that_ship_a_companion() {
        use sirio_theme::Appearance;
        assert_eq!(
            Icon::file_type("toml").for_appearance(Appearance::Light),
            Icon::file_type("toml_light")
        );
        assert_eq!(
            Icon::file_type("toml").for_appearance(Appearance::Dark),
            Icon::file_type("toml")
        );
        assert_eq!(
            Icon::file_type("rust").for_appearance(Appearance::Light),
            Icon::file_type("rust"),
            "a stem with no companion is unchanged"
        );
        assert_eq!(
            Icon::FolderFill.for_appearance(Appearance::Light),
            Icon::FolderFill,
            "the tinted catalog follows the theme's text colour, not a variant"
        );
    }

    /// The resolution must key off the *resolved* appearance. `Theme::mode`
    /// is the preference and includes `System`, so keying on it would send
    /// every "System" user down the dark branch whatever their desktop says.
    #[gpui::test]
    async fn the_element_resolves_against_the_resolved_appearance(cx: &mut TestAppContext) {
        cx.update(|cx| {
            sirio_theme::Theme::install(sirio_theme::ThemeMode::Light, cx);
            assert_eq!(
                Icon::file_type("toml").for_appearance(sirio_theme::Theme::get(cx).appearance),
                Icon::file_type("toml_light")
            );
            sirio_theme::Theme::install(sirio_theme::ThemeMode::Dark, cx);
            assert_eq!(
                Icon::file_type("toml").for_appearance(sirio_theme::Theme::get(cx).appearance),
                Icon::file_type("toml")
            );
        });
    }

    /// The render path itself, not just the helper above: draw a real
    /// `IconElement` for `toml` under a light theme and read which asset
    /// the full-colour raster cache was filled with. If `render` ever goes
    /// back to painting `self.icon`, or resolves against `Theme::mode`,
    /// the cache holds `toml` and this fails — the helper's own test would
    /// stay green. Each nextest test is its own process, so the
    /// process-wide cache starts empty.
    #[gpui::test]
    async fn a_light_theme_rasterises_the_light_companion(cx: &mut TestAppContext) {
        struct TomlIcon;
        impl gpui::Render for TomlIcon {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                use gpui::ParentElement as _;
                gpui::div().child(Icon::file_type("toml").element(IconSize::Small))
            }
        }
        cx.update(sirio_theme::Theme::init);
        cx.update(|cx| sirio_theme::Theme::install(sirio_theme::ThemeMode::Light, cx));
        let window = cx.add_window(|_, _| TomlIcon);
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let painted: Vec<Icon> = agent_mark_cache()
            .0
            .lock()
            .expect("the raster cache lock is not poisoned")
            .keys()
            .map(|(icon, _)| *icon)
            .collect();
        assert!(
            painted.contains(&Icon::file_type("toml_light")),
            "a light theme must paint the light companion, painted {painted:?}"
        );
        assert!(
            !painted.contains(&Icon::file_type("toml")),
            "the near-white dark asset must not be painted on a light theme: {painted:?}"
        );
    }
}

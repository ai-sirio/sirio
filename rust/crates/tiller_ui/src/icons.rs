//! Icons for the UI.
//!
//! # Two families, one typed enum
//!
//! [`Icon`] is the single place that knows how an icon resolves. Two
//! mechanisms exist behind it:
//!
//! - **SF Symbols** (macOS only): the Swift app renders its bar and tab
//!   icons with `Image(systemName:)`, and the user's complaint is that our
//!   Phosphor thin-outline substitutes do not match those dense, filled
//!   system glyphs. On macOS, every `Icon` with a [`system_symbol`] mapping
//!   rasterizes the real symbol through AppKit (`NSImage(systemSymbolName:)`,
//!   see `crate::sfsymbol`) tinted with the caller's theme colour — the same
//!   visual as the reference app.
//! - **Embedded Phosphor SVGs** (every platform): the same enum falls back
//!   to the vendored SVGs (MIT, see `THIRD_PARTY_NOTICES.md`) on non-Apple
//!   targets, and for any icon without a system symbol. The SVG path is
//!   always compiled, so a future Linux/Windows build degrades to the
//!   embedded set instead of failing to compile.
//!
//! # Agent marks are different
//!
//! The agent brand marks (Claude Code, Codex, OpenCode, Pi, omp) are
//! embedded SVGs on *every* platform — SF Symbols has no marks for them.
//! Marks with their own chromatic colours (the Anthropic sunburst, omp's
//! three-stop gradient) are rendered **full-colour, never tinted**: brand
//! logos keep their identity and must not be recoloured by a theme. Marks
//! that are monochrome by design (Codex's knot, OpenCode's frame, Pi's
//! monogram — the Swift app renders these with `.primary`) go through the
//! normal tinted path like the reference does.
//!
//! The SVG bytes are embedded with `include_bytes!`, so icons ship inside
//! the binary and render through GPUI's own SVG pipeline (`paint_svg` for
//! tinted masks, `svg_renderer().render_single_frame` + `paint_image` for
//! full-colour marks). Colour comes from the caller's `text_color` — which
//! must come from `Theme::get(cx)` — never from a hardcoded value, or light
//! mode breaks for that icon alone.

use gpui::{
    App, AssetSource, Bounds, Element, ElementId, GlobalElementId, Hitbox, InspectorElementId,
    InteractiveElement, Interactivity, IntoElement, LayoutId, Pixels, RenderImage, SharedString,
    StyleRefinement, Styled, Window,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// macOS-only SF Symbol rasterization (NSImage -> bitmap). Nested here with
/// an explicit path so the icons module tree stays self-contained and the
/// integrator-owned `lib.rs` does not have to declare it.
#[path = "sfsymbol.rs"]
#[cfg(target_os = "macos")]
mod sfsymbol;

/// The named icon set. `path` is the asset file name served by
/// [`TillerAssets`]; `svg` is the embedded byte payload; on macOS
/// [`Icon::system_symbol`] additionally names the SF Symbol that replaces
/// the SVG for that icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// A project directory (SF `folder.fill`, Phosphor `folder-fill`).
    FolderFill,
    /// A git worktree (Phosphor `git-branch-thin`; no SF equivalent in the
    /// reference app).
    GitBranch,
    /// A chat surface (SF `bubble.left`, Phosphor `chat-circle-thin`).
    MessageSquare,
    /// A terminal surface (SF `terminal`, Phosphor `terminal-window-thin`).
    SquareTerminal,
    /// Close (SF `xmark`, Phosphor `x-thin`).
    Close,
    /// Collapse (SF `chevron.down`, Phosphor `caret-down-thin`).
    ChevronDown,
    /// Expand (SF `chevron.right`, Phosphor `caret-right-thin`).
    ChevronRight,
    /// Back (SF `chevron.left`, Phosphor `caret-left-thin`).
    ChevronLeft,
    /// Settings gear (SF `gearshape`, Phosphor `gear-thin`).
    Settings,
    /// Refresh (SF `arrow.clockwise`, Phosphor `arrow-clockwise-thin`).
    RefreshCw,
    /// Add (SF `plus`, Phosphor `plus-thin`).
    Plus,
    /// A generic file (SF `doc.text`, Phosphor `file-thin`).
    File,
    /// AI providers (Phosphor `sparkle-thin`).
    Sparkles,
    /// Permissions (Phosphor `shield-thin`).
    Shield,
    /// Appearance (Phosphor `sun-dim-thin`).
    SunMoon,
    /// A browser surface (SF `globe`, Phosphor `globe-thin`).
    Globe,
    /// Anthropic's sunburst mark (SVG Logos, CC0) — full colour.
    ClaudeCode,
    /// OpenAI's knot mark (SVG Logos, CC0) — monochrome, like the Swift
    /// app's `.primary` rendering.
    Codex,
    /// OpenCode's mark (simple-icons, CC0) — monochrome.
    OpenCode,
    /// Pi's monogram, ported from `App/AgentIcon.swift` — monochrome.
    Pi,
    /// Oh-My-Pi's mark with the pink→purple→cyan gradient, ported from
    /// `App/AgentIcon.swift` — full colour.
    OhMyPi,
}

/// The SF Symbol that replaces this icon's SVG on macOS, if any. This is
/// the ONE place the symbol-name mapping lives: call sites say
/// `Icon::FolderFill` and know nothing about the platform.
#[cfg(target_os = "macos")]
pub fn system_symbol(icon: Icon) -> Option<&'static str> {
    match icon {
        Icon::FolderFill => Some("folder.fill"),
        Icon::MessageSquare => Some("bubble.left"),
        Icon::SquareTerminal => Some("terminal"),
        Icon::Close => Some("xmark"),
        Icon::ChevronDown => Some("chevron.down"),
        Icon::ChevronRight => Some("chevron.right"),
        Icon::ChevronLeft => Some("chevron.left"),
        Icon::Settings => Some("gearshape"),
        Icon::RefreshCw => Some("arrow.clockwise"),
        Icon::Plus => Some("plus"),
        Icon::File => Some("doc.text"),
        Icon::Globe => Some("globe"),
        Icon::GitBranch
        | Icon::Sparkles
        | Icon::Shield
        | Icon::SunMoon
        | Icon::ClaudeCode
        | Icon::Codex
        | Icon::OpenCode
        | Icon::Pi
        | Icon::OhMyPi => None,
    }
}

impl Icon {
    /// The asset path (also the file name inside `rust/assets/icons`).
    pub fn path(self) -> &'static str {
        match self {
            Icon::FolderFill => "icons/folder-fill.svg",
            Icon::GitBranch => "icons/git-branch-thin.svg",
            Icon::MessageSquare => "icons/chat-circle-thin.svg",
            Icon::SquareTerminal => "icons/terminal-window-thin.svg",
            Icon::Close => "icons/x-thin.svg",
            Icon::ChevronDown => "icons/caret-down-thin.svg",
            Icon::ChevronRight => "icons/caret-right-thin.svg",
            Icon::ChevronLeft => "icons/caret-left-thin.svg",
            Icon::Settings => "icons/gear-thin.svg",
            Icon::RefreshCw => "icons/arrow-clockwise-thin.svg",
            Icon::Plus => "icons/plus-thin.svg",
            Icon::File => "icons/file-thin.svg",
            Icon::Sparkles => "icons/sparkle-thin.svg",
            Icon::Shield => "icons/shield-thin.svg",
            Icon::SunMoon => "icons/sun-dim-thin.svg",
            Icon::Globe => "icons/globe-thin.svg",
            Icon::ClaudeCode => "icons/agent-claude.svg",
            Icon::Codex => "icons/agent-codex.svg",
            Icon::OpenCode => "icons/agent-opencode.svg",
            Icon::Pi => "icons/agent-pi.svg",
            Icon::OhMyPi => "icons/agent-omp.svg",
        }
    }

    /// The embedded SVG payload.
    pub fn svg(self) -> &'static [u8] {
        match self {
            Icon::FolderFill => include_bytes!("../../../assets/icons/folder-fill.svg"),
            Icon::GitBranch => include_bytes!("../../../assets/icons/git-branch-thin.svg"),
            Icon::MessageSquare => include_bytes!("../../../assets/icons/chat-circle-thin.svg"),
            Icon::SquareTerminal => include_bytes!("../../../assets/icons/terminal-window-thin.svg"),
            Icon::Close => include_bytes!("../../../assets/icons/x-thin.svg"),
            Icon::ChevronDown => include_bytes!("../../../assets/icons/caret-down-thin.svg"),
            Icon::ChevronRight => include_bytes!("../../../assets/icons/caret-right-thin.svg"),
            Icon::ChevronLeft => include_bytes!("../../../assets/icons/caret-left-thin.svg"),
            Icon::Settings => include_bytes!("../../../assets/icons/gear-thin.svg"),
            Icon::RefreshCw => include_bytes!("../../../assets/icons/arrow-clockwise-thin.svg"),
            Icon::Plus => include_bytes!("../../../assets/icons/plus-thin.svg"),
            Icon::File => include_bytes!("../../../assets/icons/file-thin.svg"),
            Icon::Sparkles => include_bytes!("../../../assets/icons/sparkle-thin.svg"),
            Icon::Shield => include_bytes!("../../../assets/icons/shield-thin.svg"),
            Icon::SunMoon => include_bytes!("../../../assets/icons/sun-dim-thin.svg"),
            Icon::Globe => include_bytes!("../../../assets/icons/globe-thin.svg"),
            Icon::ClaudeCode => include_bytes!("../../../assets/icons/agent-claude.svg"),
            Icon::Codex => include_bytes!("../../../assets/icons/agent-codex.svg"),
            Icon::OpenCode => include_bytes!("../../../assets/icons/agent-opencode.svg"),
            Icon::Pi => include_bytes!("../../../assets/icons/agent-pi.svg"),
            Icon::OhMyPi => include_bytes!("../../../assets/icons/agent-omp.svg"),
        }
    }

    /// Whether this icon is an agent brand mark. Marks are embedded SVGs on
    /// every platform — SF Symbols has no marks for them — and they never
    /// go through the system-symbol path.
    pub fn is_agent_mark(self) -> bool {
        matches!(
            self,
            Icon::ClaudeCode | Icon::Codex | Icon::OpenCode | Icon::Pi | Icon::OhMyPi
        )
    }

    /// Whether the mark carries its own chromatic colours (the Anthropic
    /// sunburst, omp's gradient). Such marks are painted full-colour and
    /// never tinted by the theme; the remaining marks are monochrome by
    /// design and follow the tinted path, exactly as the Swift app renders
    /// them with `.primary`.
    pub fn has_own_colours(self) -> bool {
        matches!(self, Icon::ClaudeCode | Icon::OhMyPi)
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

    /// Builds a sized element; colour is applied by the caller with
    /// [`Styled::text_color`] (theme-sourced).
    pub fn element(self, size: Pixels) -> IconElement {
        IconElement::new(self, size)
    }
}

/// An element that paints one icon at a fixed size.
///
/// Resolution order in paint (see the module docs for the rationale):
/// 1. chromatic agent marks → full-colour SVG, tint ignored;
/// 2. on macOS, icons with a system symbol → SF Symbol rasterized with the
///    caller's colour baked in;
/// 3. everything else → the tinted SVG mask path (the original behaviour).
///
/// The colour is read from the element's text style in paint, falling back
/// to the window's inherited text style because a custom element's
/// interactivity does not automatically receive a parent's refinement.
pub struct IconElement {
    interactivity: Interactivity,
    icon: Icon,
}

impl IconElement {
    pub fn new(icon: Icon, size: Pixels) -> Self {
        let mut interactivity = Interactivity::new();
        interactivity.base_style.size.width = Some(size.into());
        interactivity.base_style.size.height = Some(size.into());
        Self {
            interactivity,
            icon,
        }
    }

    /// The resolved paint colour for a tinted icon: the caller's explicit
    /// text colour, else the window's inherited one. Mirrors GPUI's own
    /// `Svg` element. Kept as a free helper because `paint` borrows
    /// `self.interactivity` mutably while reading the style.
    fn paint_color(
        style: &gpui::Style,
        window: &mut Window,
    ) -> gpui::Hsla {
        style.text.color.unwrap_or_else(|| window.text_style().color)
    }
}

/// Path 1: chromatic agent marks, painted full-colour with their own
/// colours. The SVG is rasterized once per (icon, pixel size) and
/// cached; the resulting `RenderImage` is painted into `bounds`.
fn paint_agent_mark(
    icon: Icon,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    // Render at 2x the point size, matching GPUI's own SVG renderer
    // (`SMOOTH_SVG_SCALE_FACTOR`): pixel-perfect on retina displays.
    let pixel = (f32::from(bounds.size.width) * 2.0).round().max(1.0);
    let key = (icon, pixel as u32);
    let image = agent_mark_cache().get_or_insert(key, || {
        let view_box = view_box_size(icon);
        // render_single_frame multiplies its scale argument by its own
        // 2x factor, so divide it back out.
        let scale = pixel / view_box / 2.0;
        cx.svg_renderer().render_single_frame(icon.svg(), scale).ok()
    });
    let Some(image) = image else {
        return;
    };
    let _ = window.paint_image(
        bounds,
        bounds,
        Default::default(),
        image,
        0,
        false,
    );
}

/// Path 2: the tinted SVG mask (original behaviour): rasterize the
/// alpha mask once per (path, size) in the sprite atlas, tint per paint.
fn paint_tinted_svg(
    icon: Icon,
    bounds: Bounds<Pixels>,
    color: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let _ = window.paint_svg(
        bounds,
        icon.path().into(),
        Some(icon.svg()),
        gpui::TransformationMatrix::default(),
        color,
        cx,
    );
}

/// Path 3 (macOS): SF Symbols. Rasterize the real system symbol with
/// the paint colour baked in (cached per symbol/size/tint), then paint
/// the bitmap — the same premultiplied result the SVG tint path would
/// produce, at the system's own glyph quality.
#[cfg(target_os = "macos")]
fn paint_system_symbol(
    icon: Icon,
    bounds: Bounds<Pixels>,
    color: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let rgba = gpui::Rgba::from(color);
    let tint = (
        (rgba.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (rgba.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (rgba.b * 255.0).round().clamp(0.0, 255.0) as u8,
    );
    let Some(symbol) = system_symbol(icon) else {
        return;
    };
    let Some(image) = sfsymbol::rasterize_symbol(symbol, f32::from(bounds.size.width), tint)
    else {
        // Unknown symbol name on this system: degrade to the embedded
        // SVG rather than paint nothing.
        paint_tinted_svg(icon, bounds, color, window, cx);
        return;
    };
    let _ = window.paint_image(bounds, bounds, Default::default(), image, 0, false);
}

impl Element for IconElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        self.interactivity.element_id.clone()
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.interactivity.source_location()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self.interactivity.request_layout(
            global_id,
            inspector_id,
            window,
            cx,
            |style, window, cx| window.request_layout(style, None, cx),
        );
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        self.interactivity.prepaint(
            global_id,
            inspector_id,
            bounds,
            bounds.size,
            window,
            cx,
            |_, _, hitbox, _, _| hitbox,
        )
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) where
        Self: Sized,
    {
        self.interactivity.paint(
            global_id,
            inspector_id,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |style, window, cx| {
                let color = Self::paint_color(style, window);

                if self.icon.is_agent_mark() {
                    if self.icon.has_own_colours() {
                        // Brand identity: never recolour the sunburst or the
                        // gradient with a theme tint.
                        paint_agent_mark(self.icon, bounds, window, cx);
                    } else {
                        // Monochrome marks render in the window's primary
                        // text colour — the Swift app's `.primary`.
                        paint_tinted_svg(self.icon, bounds, window.text_style().color, window, cx);
                    }
                    return;
                }

                #[cfg(target_os = "macos")]
                if system_symbol(self.icon).is_some() {
                    paint_system_symbol(self.icon, bounds, color, window, cx);
                    return;
                }

                paint_tinted_svg(self.icon, bounds, color, window, cx);
            },
        )
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
        if let Ok(cache) = self.0.lock() {
            if let Some(image) = cache.get(&key) {
                return Some(image.clone());
            }
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

impl Styled for IconElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl InteractiveElement for IconElement {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl IntoElement for IconElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The embedded asset source, for hosts that use GPUI's stock `svg()`
/// element (`application().with_assets(TillerAssets)`). Every icon in the
/// enum is served; `list` reports the icon directory.
pub struct TillerAssets;

impl AssetSource for TillerAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let icon = match path {
            "icons/folder-fill.svg" => Icon::FolderFill,
            "icons/git-branch-thin.svg" => Icon::GitBranch,
            "icons/chat-circle-thin.svg" => Icon::MessageSquare,
            "icons/terminal-window-thin.svg" => Icon::SquareTerminal,
            "icons/x-thin.svg" => Icon::Close,
            "icons/caret-down-thin.svg" => Icon::ChevronDown,
            "icons/caret-right-thin.svg" => Icon::ChevronRight,
            "icons/caret-left-thin.svg" => Icon::ChevronLeft,
            "icons/gear-thin.svg" => Icon::Settings,
            "icons/arrow-clockwise-thin.svg" => Icon::RefreshCw,
            "icons/plus-thin.svg" => Icon::Plus,
            "icons/file-thin.svg" => Icon::File,
            "icons/sparkle-thin.svg" => Icon::Sparkles,
            "icons/shield-thin.svg" => Icon::Shield,
            "icons/sun-dim-thin.svg" => Icon::SunMoon,
            "icons/globe-thin.svg" => Icon::Globe,
            "icons/agent-claude.svg" => Icon::ClaudeCode,
            "icons/agent-codex.svg" => Icon::Codex,
            "icons/agent-opencode.svg" => Icon::OpenCode,
            "icons/agent-pi.svg" => Icon::Pi,
            "icons/agent-omp.svg" => Icon::OhMyPi,
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(icon.svg())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        if path == "icons" || path.is_empty() {
            Ok(ALL_ICONS.iter().map(|icon| icon.path().into()).collect())
        } else {
            Ok(Vec::new())
        }
    }
}

/// Every icon, used by [`TillerAssets::list`] and by tests.
pub const ALL_ICONS: [Icon; 21] = [
    Icon::FolderFill,
    Icon::GitBranch,
    Icon::MessageSquare,
    Icon::SquareTerminal,
    Icon::Close,
    Icon::ChevronDown,
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
            assert!(text.contains("<svg"), "{} starts with an svg root", icon.path());
            assert!(
                text.contains("viewBox=\""),
                "{} has a viewBox",
                icon.path()
            );
        }
    }

    #[test]
    fn asset_source_serves_every_icon_by_path() {
        let assets = TillerAssets;
        for icon in ALL_ICONS {
            let loaded = assets.load(icon.path()).expect("load does not fail");
            assert!(loaded.is_some(), "{} is served by the asset source", icon.path());
            assert_eq!(loaded.unwrap().as_ref(), icon.svg());
        }
        assert!(assets.load("icons/does-not-exist.svg").unwrap().is_none());
    }

    #[test]
    fn icon_paths_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for icon in ALL_ICONS {
            assert!(seen.insert(icon.path()), "duplicate path {}", icon.path());
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
    fn layout_size_matches_constructor() {
        let element = IconElement::new(Icon::FolderFill, px(14.0));
        let size = element.interactivity.base_style.size;
        assert_eq!(size.width, Some(px(14.0).into()));
        assert_eq!(size.height, Some(px(14.0).into()));
    }

    #[test]
    fn chromatic_marks_carry_their_own_colours() {
        // The Anthropic sunburst and omp's gradient must be baked into the
        // SVG, never resolved through currentColor: a theme tint must not
        // be able to recolour a brand mark. The monochrome marks (Codex,
        // OpenCode, Pi) are exempt by design — the Swift app renders those
        // with `.primary`.
        let claude = std::str::from_utf8(Icon::ClaudeCode.svg()).expect("utf-8");
        assert!(
            claude.contains("#d97757"),
            "the sunburst carries the Anthropic orange"
        );
        assert!(
            !claude.contains("currentColor"),
            "the sunburst must not resolve through the theme tint"
        );
        let omp = std::str::from_utf8(Icon::OhMyPi.svg()).expect("utf-8");
        assert!(omp.contains("linearGradient"), "omp is a gradient mark");
        assert!(omp.contains("#ED4ABF") && omp.contains("#9B4DFF") && omp.contains("#5AD8E6"));
        assert!(!omp.contains("currentColor"), "omp must not resolve through the theme tint");
    }

    #[test]
    fn monochrome_marks_keep_the_reference_shapes() {
        // Codex's knot and OpenCode's frame must not accidentally become
        // the currentColor-tinted generic glyphs they replaced.
        let codex = std::str::from_utf8(Icon::Codex.svg()).expect("utf-8");
        assert!(codex.contains("viewBox=\"-19 0 139 155\""), "the knot is cropped from the wordmark logo");
        let pi = std::str::from_utf8(Icon::Pi.svg()).expect("utf-8");
        assert!(pi.contains("viewBox=\"0 0 800 800\""), "Pi keeps the Swift shape's viewBox");
    }

    #[test]
    fn sf_symbols_map_only_on_macos_and_never_for_marks() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(system_symbol(Icon::FolderFill), Some("folder.fill"));
            assert_eq!(system_symbol(Icon::SquareTerminal), Some("terminal"));
            assert_eq!(system_symbol(Icon::RefreshCw), Some("arrow.clockwise"));
            assert_eq!(system_symbol(Icon::Settings), Some("gearshape"));
            // Marks never resolve to system symbols.
            for icon in ALL_ICONS {
                if icon.is_agent_mark() {
                    assert_eq!(system_symbol(icon), None, "{icon:?} stays an embedded SVG");
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Icon::FolderFill;
        }
    }

    #[test]
    fn view_box_size_parses_embedded_svgs() {
        assert!((view_box_size(Icon::FolderFill) - 256.0).abs() < 1.0, "phosphor viewBox");
        assert!((view_box_size(Icon::ClaudeCode) - 110.0).abs() < 1.0, "sunburst viewBox");
        assert!((view_box_size(Icon::Pi) - 800.0).abs() < 1.0, "pi viewBox");
    }
}

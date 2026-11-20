//! The file view's Markdown Preview: the parsed document as bezel's `Doc`,
//! with raw HTML expanded, lone images turned into pictures and diagram
//! fences replaced by what they render to (design §3 and §5,
//! `docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::App;

use bezel_markdown::BlockKind;
use sirio_diagram::{DiagramKind, Palette};
use sirio_markdown::{Document, expand_html};
use sirio_theme::{Appearance, Theme};

use crate::chat::bezel_doc_from_legacy;

/// Where one diagram stands, by its cache key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagramState {
    /// Scheduled. The fence shows as code meanwhile.
    Pending,
    /// Rendered to `path`, drawn at `width` logical pixels.
    Ready { path: PathBuf, width: u32 },
    /// Not rendered: the note goes under the fence. "Not available" is one
    /// of these too, which is why every failure is forgotten when the
    /// settings change.
    Failed(String),
}

/// One file view's diagram states.
#[derive(Debug, Default)]
pub(crate) struct Diagrams {
    states: HashMap<String, DiagramState>,
    /// Moves with every change to `states`: part of the Preview's cache key, so a diagram
    /// becoming ready, failing or being forgotten draws the tree again.
    version: u64,
}

impl Diagrams {
    pub(crate) fn get(&self, key: &str) -> Option<&DiagramState> {
        self.states.get(key)
    }

    /// How many times the states have changed.
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn insert(&mut self, key: String, state: DiagramState) {
        self.states.insert(key, state);
        self.version += 1;
    }

    /// Drops the entry for `key`, so the next frame requests it again.
    pub(crate) fn remove(&mut self, key: &str) {
        self.states.remove(key);
        self.version += 1;
    }

    /// Drops every failure, so the next frame tries again when settings
    /// change. Installing PlantUML takes effect when reopened; configuring a
    /// server takes effect immediately.
    pub(crate) fn forget_failures(&mut self) {
        self.states
            .retain(|_, state| !matches!(state, DiagramState::Failed(_)));
        self.version += 1;
    }
}

/// A diagram the Preview names that has no state yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagramRequest {
    pub(crate) key: String,
    pub(crate) kind: DiagramKind,
    pub(crate) source: String,
}

/// One frame's Preview: the document to draw, and the diagrams it names
/// that nobody has scheduled yet.
pub(crate) struct Preview {
    pub(crate) doc: bezel_markdown::Doc,
    /// Only the file view read it, and it reads `preview_blocks` now; the bezel build goes
    /// with the change requests' move to Ely.
    #[allow(dead_code)]
    pub(crate) missing: Vec<DiagramRequest>,
}

/// Builds the Preview's document. `base` is the Markdown file's directory.
pub(crate) fn build(
    document: Document,
    base: &Path,
    diagrams: &Diagrams,
    palette: &Palette,
) -> Preview {
    let source_blocks = document.blocks.len() as u64;
    let expanded = {
        let _perf = sirio_perf::span("Preview.expand_html", source_blocks);
        expand_html(document)
    };
    let converted = bezel_doc_from_legacy(expanded);
    let mut blocks = Vec::with_capacity(converted.blocks.len());
    let mut missing = Vec::new();
    for block in converted.blocks {
        let indent = block.indent;
        match block.kind {
            BlockKind::Code {
                language: Some(language),
                code,
            } => {
                let Some(kind) = DiagramKind::from_fence(&language) else {
                    blocks.push(bezel_markdown::Block::at(
                        BlockKind::Code {
                            language: Some(language),
                            code,
                        },
                        indent,
                    ));
                    continue;
                };
                let key = sirio_diagram::cache_key(kind, &code.text, palette);
                let source = code.text.clone();
                let fence = bezel_markdown::Block::at(
                    BlockKind::Code {
                        language: Some(language),
                        code,
                    },
                    indent,
                );
                match diagrams.get(&key) {
                    Some(DiagramState::Ready { path, width }) => {
                        blocks.push(bezel_markdown::Block::at(
                            BlockKind::Image {
                                url: path.to_string_lossy().into_owned(),
                                alt: bezel_markdown::Text::default(),
                                width: Some(*width),
                            },
                            indent,
                        ));
                    }
                    Some(DiagramState::Failed(note)) => {
                        blocks.push(fence);
                        blocks.push(bezel_markdown::Block::at(
                            BlockKind::Quote(bezel_markdown::Text::plain(note.clone())),
                            indent,
                        ));
                    }
                    Some(DiagramState::Pending) => blocks.push(fence),
                    None => {
                        // The same diagram twice in one document is rendered
                        // once: first occurrence wins, both fences show as code.
                        if !missing
                            .iter()
                            .any(|existing: &DiagramRequest| existing.key == key)
                        {
                            missing.push(DiagramRequest {
                                key: key.clone(),
                                kind,
                                source,
                            });
                        }
                        blocks.push(fence);
                    }
                }
            }
            BlockKind::Image { url, alt, width } => blocks.push(bezel_markdown::Block::at(
                BlockKind::Image {
                    url: resolve_image(&url, base),
                    alt,
                    width,
                },
                indent,
            )),
            kind => blocks.push(bezel_markdown::Block::at(kind, indent)),
        }
    }
    Preview {
        doc: bezel_markdown::Doc { blocks },
        missing,
    }
}

/// The Preview's document as the tree Ely draws: `source` read once, the HTML in it
/// expanded, the pictures resolved against `base` (the Markdown file's folder) and the
/// diagram fences replaced by what `diagrams` holds for them. `cache_dir` is where the
/// rendered diagrams live; without it (no diagram settings) a fence stays code and asks
/// for nothing.
pub(crate) fn preview_blocks(
    source: &str,
    base: &Path,
    diagrams: &Diagrams,
    palette: &Palette,
    cache_dir: Option<&Path>,
) -> crate::markdown::MarkdownBlocks {
    let _perf = sirio_perf::span("Preview.build", source.len() as u64);
    crate::markdown::build(
        source,
        &mut crate::markdown::Policy {
            surface: crate::markdown::Surface::Preview,
            base: Some(base),
            diagrams: cache_dir.map(|dir| crate::markdown::DiagramInputs {
                states: diagrams,
                palette,
                dir,
            }),
            file_links: None,
        },
    )
}

/// A relative image path joined onto the Markdown file's directory, because
/// `gpui::img(PathBuf)` would otherwise resolve it against the process's
/// working directory. Percent escapes are decoded, as a browser would
/// (`my%20logo.png` is the file `my logo.png`). URLs and absolute paths pass
/// through.
pub(crate) fn resolve_image(url: &str, base: &Path) -> String {
    if url.is_empty() || has_scheme(url) || Path::new(url).is_absolute() {
        return url.to_string();
    }
    let path = url.split(['#', '?']).next().unwrap_or(url);
    base.join(percent_decode(path))
        .to_string_lossy()
        .into_owned()
}

/// `https:`, `data:`, `file:`: a scheme is letters, digits, `+ - .` before
/// the first `:`.
fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

pub(crate) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// The Mermaid palette for `theme`: the Preview's own surface, text and
/// borders, so a diagram reads as part of the page in either appearance.
pub(crate) fn palette(theme: &Theme) -> Palette {
    Palette {
        dark: matches!(theme.appearance, Appearance::Dark),
        background: hex(theme.ely.bg),
        text: hex(theme.ely.fg),
        node_fill: hex(theme.ely.surface),
        node_border: hex(theme.ely.border),
        line: hex(theme.ely.fg_muted),
        label_background: hex(theme.ely.bg),
    }
}

fn hex(color: gpui::Hsla) -> String {
    let color = gpui::Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

/// Whether a clicked link target is a rendered diagram. bezel treats an
/// image block as a link to its own URL, and a diagram's URL is its cached
/// SVG, which must not open in a tab.
pub(crate) fn is_diagram_target(target: &str, diagram_dir: Option<&Path>) -> bool {
    diagram_dir.is_some_and(|dir| Path::new(target).starts_with(dir))
}

/// Where diagrams are cached and which PlantUML server may be asked
/// (design §3, §6). The host installs it at startup and again whenever
/// Settings change. Without it the Preview renders no diagrams and every
/// fence stays code, which is what a test app that never asked sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramSettings {
    pub cache_dir: PathBuf,
    pub plantuml_server: Option<String>,
}

impl gpui::Global for DiagramSettings {}

impl DiagramSettings {
    /// The production settings: the per-user cache directory, and `server`
    /// unless it is blank. `None` when the platform names no cache
    /// directory, in which case diagrams are not rendered.
    pub fn from_environment(server: &str) -> Option<Self> {
        let cache_dir = sirio_diagram::cache_dir(&|key: &str| std::env::var_os(key))?;
        let server = server.trim();
        Some(Self {
            cache_dir,
            plantuml_server: (!server.is_empty()).then(|| server.to_string()),
        })
    }

    pub fn set(settings: Self, cx: &mut App) {
        cx.set_global(settings);
    }

    /// Installs [`Self::from_environment`] for `server`, if there is one.
    pub fn apply(server: &str, cx: &mut App) {
        if let Some(settings) = Self::from_environment(server) {
            Self::set(settings, cx);
        }
    }

    pub(crate) fn get(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }
}

/// The app-wide PlantUML turn: one JVM at a time (design §3). The lock is
/// async, so a fence waiting its turn holds no background thread.
#[derive(Clone, Default)]
pub(crate) struct PlantUmlQueue(Arc<futures::lock::Mutex<()>>);

impl gpui::Global for PlantUmlQueue {}

impl PlantUmlQueue {
    pub(crate) fn get(cx: &mut App) -> Self {
        if cx.try_global::<Self>().is_none() {
            cx.set_global(Self::default());
        }
        cx.global::<Self>().clone()
    }

    pub(crate) async fn turn(&self) -> futures::lock::MutexGuard<'_, ()> {
        self.0.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::MarkdownBlocks;
    use ely_gpui_component::documents::MarkdownBlock as Node;

    fn test_palette() -> Palette {
        Palette {
            dark: false,
            background: "#fafafa".into(),
            text: "#222222".into(),
            node_fill: "#dddddd".into(),
            node_border: "#888888".into(),
            line: "#666666".into(),
            label_background: "#fafafa".into(),
        }
    }

    fn base() -> PathBuf {
        PathBuf::from("/repo/docs")
    }

    fn preview(source: &str, diagrams: &Diagrams) -> MarkdownBlocks {
        preview_blocks(
            source,
            &base(),
            diagrams,
            &test_palette(),
            Some(Path::new("/cache")),
        )
    }

    fn is_fence(node: &Node, fence_language: &str) -> bool {
        matches!(node, Node::Code { language: Some(language), .. } if language == fence_language)
    }

    /// Every picture of the tree, at any depth.
    fn pictures(blocks: &MarkdownBlocks) -> Vec<Node> {
        fn walk(nodes: &[Node], out: &mut Vec<Node>) {
            for node in nodes {
                match node {
                    Node::Image { .. } => out.push(node.clone()),
                    Node::Quote(inner) => walk(inner, out),
                    Node::List { items, .. } => {
                        for (_, inner) in items {
                            walk(inner, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&blocks.nodes, &mut out);
        out
    }

    const FENCE: &str = "# D\n\n```mermaid\nflowchart TD\n  A --> B\n```\n";

    #[test]
    fn an_unknown_diagram_stays_code_and_is_requested() {
        let preview = preview(FENCE, &Diagrams::default());
        assert!(is_fence(&preview.nodes[1], "mermaid"));
        assert_eq!(preview.missing_diagrams.len(), 1);
        assert_eq!(preview.missing_diagrams[0].kind, DiagramKind::Mermaid);
        assert_eq!(
            preview.missing_diagrams[0].source,
            "flowchart TD\n  A --> B"
        );
    }

    fn key_of(source: &str) -> String {
        preview(source, &Diagrams::default()).missing_diagrams[0]
            .key
            .clone()
    }

    #[test]
    fn a_ready_diagram_is_an_image_at_its_logical_width() {
        let mut diagrams = Diagrams::default();
        diagrams.insert(
            key_of(FENCE),
            DiagramState::Ready {
                path: PathBuf::from("/cache/k.svg"),
                width: 120,
            },
        );
        let preview = preview(FENCE, &diagrams);
        assert!(preview.missing_diagrams.is_empty());
        assert!(matches!(
            &preview.nodes[1],
            Node::Image { url, width: Some(width), .. } if url == "/cache/k.svg" && *width == 120.0
        ));
    }

    #[test]
    fn a_failed_diagram_is_code_then_its_note() {
        let mut diagrams = Diagrams::default();
        diagrams.insert(
            key_of(FENCE),
            DiagramState::Failed("Mermaid diagram is invalid: x".into()),
        );
        let preview = preview(FENCE, &diagrams);
        assert!(is_fence(&preview.nodes[1], "mermaid"));
        let Node::Quote(note) = &preview.nodes[2] else {
            panic!("the note is the muted block after the fence: {:?}", preview.nodes[2]);
        };
        assert!(matches!(
            note.as_slice(),
            [Node::Paragraph(inline)] if inline.text == "Mermaid diagram is invalid: x"
        ));
    }

    #[test]
    fn a_pending_diagram_is_code_and_not_requested_again() {
        let mut diagrams = Diagrams::default();
        diagrams.insert(key_of(FENCE), DiagramState::Pending);
        let preview = preview(FENCE, &diagrams);
        assert!(preview.missing_diagrams.is_empty());
        assert!(is_fence(&preview.nodes[1], "mermaid"));
    }

    #[test]
    fn the_same_diagram_twice_is_requested_once() {
        let twice = format!("{FENCE}\n```mermaid\nflowchart TD\n  A --> B\n```\n");
        let preview = preview(&twice, &Diagrams::default());
        assert_eq!(preview.missing_diagrams.len(), 1);
        let fences = (preview.nodes.iter())
            .filter(|node| is_fence(node, "mermaid"))
            .count();
        assert_eq!(fences, 2, "both fences still show as code");
    }

    /// Review Focus 3.
    #[test]
    fn a_fence_inside_a_list_item_is_rewritten_where_it_stands() {
        let source = "- item\n\n  ```mermaid\n  flowchart TD\n    A --> B\n  ```\n";
        let mut diagrams = Diagrams::default();
        diagrams.insert(
            key_of(source),
            DiagramState::Ready {
                path: PathBuf::from("/cache/k.svg"),
                width: 50,
            },
        );
        let preview = preview(source, &diagrams);
        let Node::List { items, .. } = &preview.nodes[0] else {
            panic!("a list: {:?}", preview.nodes[0]);
        };
        assert!(
            items[0].1.iter().any(|node| matches!(node, Node::Image { .. })),
            "the picture stays inside its list item: {:?}",
            items[0].1
        );
    }

    #[test]
    fn a_plain_code_block_is_left_alone() {
        let preview = preview("```rust\nfn main() {}\n```\n", &Diagrams::default());
        assert!(preview.missing_diagrams.is_empty());
        assert!(is_fence(&preview.nodes[0], "rust"));
    }

    #[test]
    fn a_lone_image_is_a_picture_resolved_against_the_file() {
        let preview = preview("![Logo](logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.nodes[0],
            Node::Image { url, alt, width: None, .. }
                if Path::new(url) == base().join("logo.png") && alt == "Logo"
        ));
    }

    #[test]
    fn a_lone_image_in_a_quote_is_a_picture() {
        let preview = preview("> ![Logo](logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.nodes[0],
            Node::Quote(inner) if matches!(inner.as_slice(), [Node::Image { .. }])
        ));
    }

    #[test]
    fn a_linked_lone_image_is_a_picture() {
        let preview = preview(
            "[![Logo](logo.png)](https://example.com)\n",
            &Diagrams::default(),
        );
        assert!(matches!(
            &preview.nodes[0],
            Node::Image { link: Some(link), .. } if link == "https://example.com"
        ));
    }

    /// D10 (design §5.9): a badge row used to stay one paragraph of bezel's inline pictures;
    /// Ely's reader splits a paragraph at each picture, so each badge is its own block.
    #[test]
    fn a_badge_row_splits_into_its_pictures() {
        let preview = preview("![a](a.svg) ![b](b.svg)\n", &Diagrams::default());
        assert_eq!(pictures(&preview).len(), 2, "{:?}", preview.nodes);
    }

    /// D10 (design §5.9), the rewrite of `an_image_in_running_text_stays_a_paragraph`: a
    /// picture inside a sentence becomes its own block between the text before and after it.
    #[test]
    fn an_image_in_running_text_splits_the_paragraph() {
        let preview = preview("See ![a](a.png) here\n", &Diagrams::default());
        let [Node::Paragraph(before), Node::Image { .. }, Node::Paragraph(after)] =
            preview.nodes.as_ref()
        else {
            panic!("text, picture, text: {:?}", preview.nodes);
        };
        assert_eq!(before.text.trim(), "See");
        assert_eq!(after.text.trim(), "here");
    }

    #[test]
    fn an_html_img_keeps_its_width() {
        let preview = preview(
            "<p align=\"center\"><img src=\"logo.png\" width=\"120\"></p>\n",
            &Diagrams::default(),
        );
        assert!(matches!(
            &preview.nodes[0],
            Node::Image { url, width: Some(width), .. }
                if Path::new(url) == base().join("logo.png") && *width == 120.0
        ));
    }

    #[test]
    fn a_remote_image_is_passed_through() {
        let preview = preview("![b](https://example.com/b.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.nodes[0],
            Node::Image { url, .. } if url == "https://example.com/b.png"
        ));
    }

    /// Review Focus 4.
    #[test]
    fn a_percent_encoded_relative_image_is_decoded() {
        let preview = preview("![a](my%20logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.nodes[0],
            Node::Image { url, .. } if Path::new(url) == base().join("my logo.png")
        ));
    }

    #[test]
    fn forgetting_failures_keeps_everything_else() {
        let mut diagrams = Diagrams::default();
        diagrams.insert("a".into(), DiagramState::Pending);
        diagrams.insert(
            "b".into(),
            DiagramState::Ready {
                path: PathBuf::from("/b.svg"),
                width: 1,
            },
        );
        diagrams.insert("c".into(), DiagramState::Failed("x".into()));
        diagrams.forget_failures();
        assert_eq!(diagrams.get("a"), Some(&DiagramState::Pending));
        assert!(diagrams.get("b").is_some());
        assert_eq!(diagrams.get("c"), None);
    }

    /// Review Focus 5.
    #[test]
    fn a_diagram_path_is_not_a_link_target() {
        let dir = Path::new("/cache/diagrams");
        assert!(is_diagram_target("/cache/diagrams/abc.svg", Some(dir)));
        assert!(!is_diagram_target("/repo/docs/logo.png", Some(dir)));
        assert!(!is_diagram_target("/cache/diagrams/abc.svg", None));
    }

    #[test]
    fn the_palette_follows_the_theme() {
        let dark = palette(&Theme::dark());
        let light = palette(&Theme::light());
        assert!(dark.dark && !light.dark);
        assert_ne!(dark.background, light.background);
        assert!(light.background.starts_with('#') && light.background.len() == 7);
    }

    /// Draws one heading, one paragraph and one code fence through the renderer the Preview
    /// uses, and reports where each landed.
    struct Page {
        blocks: MarkdownBlocks,
        layouts: std::rc::Rc<std::cell::RefCell<Option<crate::markdown::BodyLayouts>>>,
    }

    impl gpui::Render for Page {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::{ParentElement, Styled};
            let view = crate::markdown::markdown_view(
                &self.blocks,
                crate::markdown::SelectionHost::None,
                std::rc::Rc::new(|_, _, _| {}),
                crate::markdown::ViewFlags::default(),
            );
            *self.layouts.borrow_mut() = Some(view.layouts());
            gpui::div().w(gpui::px(600.0)).child(view)
        }
    }

    /// The heading's, the paragraph's and the code fence's heights, as drawn.
    fn heights(
        cx: &mut gpui::VisualTestContext,
        layouts: &crate::markdown::BodyLayouts,
    ) -> [f32; 3] {
        let heading = cx
            .debug_bounds("markdown-heading-0")
            .expect("the heading was drawn");
        let paragraph = layouts.text_bounds(1).expect("the paragraph was drawn");
        let code = layouts.frame_bounds(0).expect("the fence was drawn");
        [
            f32::from(heading.size.height),
            f32::from(paragraph.size.height),
            f32::from(code.size.height),
        ]
    }

    /// D1 (design §5.9): Ely sets the sizes, so a paragraph is one body step taller as the
    /// interface font grows and a heading keeps its Ely ratio to the body (x2.25 for H1),
    /// where bezel's renderer added the same +5 px offset to every size.
    #[gpui::test]
    async fn rendered_markdown_follows_the_interface_font_size(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
        let layouts = std::rc::Rc::new(std::cell::RefCell::new(None));
        let shared = layouts.clone();
        let (_, _, cx) = crate::text_selection::testing::host(cx, move |_, _| Page {
            blocks: preview("# Title\n\nBody paragraph.\n\n```rust\nlet x = 1;\n```\n", &Diagrams::default()),
            layouts: shared,
        });
        let drawn = |layouts: &std::rc::Rc<std::cell::RefCell<Option<crate::markdown::BodyLayouts>>>| {
            layouts.borrow().clone().expect("the page rendered")
        };
        cx.simulate_resize(gpui::size(gpui::px(800.0), gpui::px(400.0)));
        cx.run_until_parked();
        let at_13 = heights(cx, &drawn(&layouts));

        cx.update(|_, cx| {
            Theme::set_interface_font_size(18, cx);
            cx.refresh_windows();
        });
        cx.run_until_parked();
        cx.simulate_resize(gpui::size(gpui::px(801.0), gpui::px(400.0)));
        cx.run_until_parked();
        let at_18 = heights(cx, &drawn(&layouts));
        let [heading_growth, body_growth, code_growth] =
            std::array::from_fn(|ix| at_18[ix] - at_13[ix]);
        // The interface step is +5 pt; the body's line is 1.6 of it, a heading's 1.3 of
        // its own size, which is the body's x2.25.
        assert!(
            (body_growth - 5.0 * 1.6).abs() <= 1.0,
            "paragraph grew {body_growth}"
        );
        assert!(
            (heading_growth - 5.0 * 2.25 * 1.3).abs() <= 1.5,
            "heading grew {heading_growth}"
        );
        // Its one code line grows with the interface too.
        assert!(code_growth >= 5.0, "code block grew {code_growth}");

        cx.update(|_, cx| {
            Theme::set_interface_font_size(13, cx);
            cx.refresh_windows();
        });
        cx.run_until_parked();
        cx.simulate_resize(gpui::size(gpui::px(800.0), gpui::px(400.0)));
        cx.run_until_parked();
        assert_eq!(heights(cx, &drawn(&layouts)), at_13);
    }
}

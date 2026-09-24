//! The file view's Markdown Preview: the parsed document as bezel's `Doc`,
//! with raw HTML expanded, lone images turned into pictures and diagram
//! fences replaced by what they render to (design §3 and §5,
//! `docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::App;

use markdown::BlockKind;
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
}

impl Diagrams {
    pub(crate) fn get(&self, key: &str) -> Option<&DiagramState> {
        self.states.get(key)
    }

    pub(crate) fn insert(&mut self, key: String, state: DiagramState) {
        self.states.insert(key, state);
    }

    /// Drops every failure, so the next frame tries again: installing
    /// PlantUML or configuring a server takes effect without reopening.
    pub(crate) fn forget_failures(&mut self) {
        self.states
            .retain(|_, state| !matches!(state, DiagramState::Failed(_)));
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
    pub(crate) doc: markdown::Doc,
    pub(crate) missing: Vec<DiagramRequest>,
}

/// Builds the Preview's document. `base` is the Markdown file's directory.
pub(crate) fn build(
    document: Document,
    base: &Path,
    diagrams: &Diagrams,
    palette: &Palette,
) -> Preview {
    let converted = bezel_doc_from_legacy(expand_html(document));
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
                    blocks.push(markdown::Block::at(
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
                let fence = markdown::Block::at(
                    BlockKind::Code {
                        language: Some(language),
                        code,
                    },
                    indent,
                );
                match diagrams.get(&key) {
                    Some(DiagramState::Ready { path, width }) => {
                        blocks.push(markdown::Block::at(
                            BlockKind::Image {
                                url: path.to_string_lossy().into_owned(),
                                alt: markdown::Text::default(),
                                width: Some(*width),
                            },
                            indent,
                        ));
                    }
                    Some(DiagramState::Failed(note)) => {
                        blocks.push(fence);
                        blocks.push(markdown::Block::at(
                            BlockKind::Quote(markdown::Text::plain(note.clone())),
                            indent,
                        ));
                    }
                    Some(DiagramState::Pending) => blocks.push(fence),
                    None => {
                        missing.push(DiagramRequest { key, kind, source });
                        blocks.push(fence);
                    }
                }
            }
            BlockKind::Image { url, alt, width } => blocks.push(markdown::Block::at(
                BlockKind::Image {
                    url: resolve_image(&url, base),
                    alt,
                    width,
                },
                indent,
            )),
            kind => blocks.push(markdown::Block::at(kind, indent)),
        }
    }
    Preview {
        doc: markdown::Doc { blocks },
        missing,
    }
}

/// A relative image path joined onto the Markdown file's directory, because
/// `gpui::img(PathBuf)` would otherwise resolve it against the process's
/// working directory. Percent escapes are decoded, as a browser would
/// (`my%20logo.png` is the file `my logo.png`). URLs and absolute paths pass
/// through.
fn resolve_image(url: &str, base: &Path) -> String {
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

fn percent_decode(text: &str) -> String {
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
        background: hex(theme.surface),
        text: hex(theme.text),
        node_fill: hex(theme.surface_raised),
        node_border: hex(theme.border_opaque),
        line: hex(theme.text_muted),
        label_background: hex(theme.surface),
    }
}

fn hex(color: gpui::Rgba) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

/// The directory PlantUML's `!include` is confined to: the nearest ancestor
/// holding a `.git` entry (a directory in a checkout, a file in a linked
/// worktree), or the file's own directory outside any repository.
pub(crate) fn include_root(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or(file);
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(start)
        .to_path_buf()
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
    use markdown::BlockKind;
    use sirio_markdown::parse;

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

    fn preview(source: &str, diagrams: &Diagrams) -> Preview {
        build(parse(source), &base(), diagrams, &test_palette())
    }

    const FENCE: &str = "# D\n\n```mermaid\nflowchart TD\n  A --> B\n```\n";

    #[test]
    fn an_unknown_diagram_stays_code_and_is_requested() {
        let preview = preview(FENCE, &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[1].kind,
            BlockKind::Code { language: Some(language), .. } if language == "mermaid"
        ));
        assert_eq!(preview.missing.len(), 1);
        assert_eq!(preview.missing[0].kind, DiagramKind::Mermaid);
        assert_eq!(preview.missing[0].source, "flowchart TD\n  A --> B");
    }

    fn key_of(source: &str) -> String {
        preview(source, &Diagrams::default()).missing[0].key.clone()
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
        assert!(preview.missing.is_empty());
        assert!(matches!(
            &preview.doc.blocks[1].kind,
            BlockKind::Image { url, width: Some(120), .. } if url == "/cache/k.svg"
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
        assert!(matches!(
            &preview.doc.blocks[1].kind,
            BlockKind::Code { .. }
        ));
        assert!(matches!(
            &preview.doc.blocks[2].kind,
            BlockKind::Quote(text) if text.text == "Mermaid diagram is invalid: x"
        ));
    }

    #[test]
    fn a_pending_diagram_is_code_and_not_requested_again() {
        let mut diagrams = Diagrams::default();
        diagrams.insert(key_of(FENCE), DiagramState::Pending);
        let preview = preview(FENCE, &diagrams);
        assert!(preview.missing.is_empty());
        assert!(matches!(
            &preview.doc.blocks[1].kind,
            BlockKind::Code { .. }
        ));
    }

    #[test]
    fn the_same_diagram_twice_is_one_key() {
        let twice = format!("{FENCE}\n```mermaid\nflowchart TD\n  A --> B\n```\n");
        let preview = preview(&twice, &Diagrams::default());
        assert_eq!(preview.missing.len(), 2);
        assert_eq!(preview.missing[0].key, preview.missing[1].key);
    }

    /// Review Focus 3.
    #[test]
    fn a_fence_inside_a_list_item_is_rewritten_at_its_indent() {
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
        let image = preview
            .doc
            .blocks
            .iter()
            .find(|block| matches!(block.kind, BlockKind::Image { .. }))
            .expect("the nested fence became an image");
        assert_eq!(image.indent, 1);
    }

    #[test]
    fn a_plain_code_block_is_left_alone() {
        let preview = preview("```rust\nfn main() {}\n```\n", &Diagrams::default());
        assert!(preview.missing.is_empty());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Code { .. }
        ));
    }

    #[test]
    fn a_lone_image_is_a_picture_resolved_against_the_file() {
        let preview = preview("![Logo](logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { url, alt, width: None }
                if Path::new(url) == base().join("logo.png") && alt.text == "Logo"
        ));
    }

    #[test]
    fn a_lone_image_in_a_quote_is_a_picture() {
        let preview = preview("> ![Logo](logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { .. }
        ));
    }

    #[test]
    fn a_linked_lone_image_is_a_picture() {
        let preview = preview(
            "[![Logo](logo.png)](https://example.com)\n",
            &Diagrams::default(),
        );
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { .. }
        ));
    }

    #[test]
    fn a_badge_row_stays_a_paragraph() {
        let preview = preview("![a](a.svg) ![b](b.svg)\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Paragraph(_)
        ));
    }

    #[test]
    fn an_image_in_running_text_stays_a_paragraph() {
        let preview = preview("See ![a](a.png) here\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Paragraph(_)
        ));
    }

    #[test]
    fn an_html_img_keeps_its_width() {
        let preview = preview(
            "<p align=\"center\"><img src=\"logo.png\" width=\"120\"></p>\n",
            &Diagrams::default(),
        );
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { url, width: Some(120), .. } if Path::new(url) == base().join("logo.png")
        ));
    }

    #[test]
    fn a_remote_image_is_passed_through() {
        let preview = preview("![b](https://example.com/b.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { url, .. } if url == "https://example.com/b.png"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_image_is_passed_through() {
        let preview = preview("![a](/abs/a.png)\n", &Diagrams::default());
        assert!(
            matches!(&preview.doc.blocks[0].kind, BlockKind::Image { url, .. } if url == "/abs/a.png")
        );
    }

    /// Review Focus 4.
    #[test]
    fn a_percent_encoded_relative_image_is_decoded() {
        let preview = preview("![a](my%20logo.png)\n", &Diagrams::default());
        assert!(matches!(
            &preview.doc.blocks[0].kind,
            BlockKind::Image { url, .. } if Path::new(url) == base().join("my logo.png")
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

    #[test]
    fn the_include_root_is_the_nearest_git_ancestor() {
        let root = std::env::temp_dir().join(format!("sirio-include-root-{}", std::process::id()));
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).expect("docs");
        std::fs::write(root.join(".git"), "gitdir: elsewhere\n")
            .expect(".git file, as in a linked worktree");
        assert_eq!(include_root(&docs.join("a.md")), root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn outside_a_repository_the_include_root_is_the_files_directory() {
        let dir = std::env::temp_dir().join(format!("sirio-no-repo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let found = include_root(&dir.join("a.md"));
        assert!(found == dir || found.join(".git").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
}

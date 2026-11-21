//! Markdown, parsed once with Ely's reader and rewritten for the surface that shows it.
//!
//! [`build`] reads a source into Ely's tree, then rewrites it by what the surface lets
//! through (pictures, links, diagrams, block HTML), and keeps beside it what selection,
//! copy and the anchors need: the plain text, the headings and their slugs.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use ely_gpui_component::documents::{MarkdownBlock, Style, parse_markdown_with};
use gpui::SharedString;
use pulldown_cmark::{Event, Options, Parser};

use crate::chat::file_links::FileLinkMemo;
use crate::markdown_preview::{DiagramRequest, Diagrams};

mod code;
pub(crate) mod cull;
mod gather;
mod menu;
mod rewrite;
mod sideways;
mod slug;
mod text;
mod view;

#[cfg(test)]
mod tests;

// Consumers arrive with Tasks 5-7 (the Preview, change requests, the chat).
#[allow(unused_imports)]
pub(crate) use gather::BodyLayouts;
#[cfg(test)]
pub(crate) use gather::DrawnPart;
#[allow(unused_imports)]
pub(crate) use menu::{MarkdownMenuItem, perform};
pub(crate) use slug::slug;
pub(crate) use text::{Part, PartKind, PlainText};
#[allow(unused_imports)]
pub(crate) use view::{
    BodyActions, LinkHandler, MarkdownView, MenuHandler, MenuPress, SelectionHost, ViewFlags,
    markdown_view, probe_view,
};

/// The deepest nesting drawn as written; past it the source is one plain paragraph. The
/// cap `sirio_markdown` keeps for the same reason.
pub(crate) const MAX_NESTING: usize = 128;

/// The deepest block HTML is built: a reader sees nothing in more levels, and what is built
/// is walked, dropped and drawn by recursion on whatever stack the caller has.
pub(crate) const MAX_HTML_NESTING: usize = 32;

/// Who shows the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Preview,
    Chat,
    ChangeRequest,
}

/// The diagram pipeline's state, borrowed from the file view that owns and changes it.
pub(crate) struct DiagramInputs<'a> {
    pub states: &'a Diagrams,
    pub palette: &'a sirio_diagram::Palette,
    /// Where the rendered diagrams are cached.
    pub dir: &'a Path,
}

/// What finding the files a chat reply names takes: `link_file_paths`'s inputs.
pub(crate) struct FileLinkInputs<'a> {
    pub worktree: &'a Path,
    pub home: Option<&'a Path>,
    pub memo: &'a mut FileLinkMemo,
}

/// What a surface lets through.
pub(crate) struct Policy<'a> {
    pub surface: Surface,
    /// The file's folder (Preview) or the worktree (Chat). Sirio's own: Ely never gets it.
    pub base: Option<&'a Path>,
    pub diagrams: Option<DiagramInputs<'a>>,
    pub file_links: Option<FileLinkInputs<'a>>,
}

/// A heading of the final tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    /// The renderer's `heading-N`: the drawing order over the final tree.
    pub ordinal: usize,
    pub slug: SharedString,
    pub title: SharedString,
    pub depth: usize,
}

/// How many of each kind of node the tree holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeCounts {
    pub math: usize,
    pub alerts: usize,
    pub footnotes: usize,
    pub images: usize,
    pub tables: usize,
    pub code: usize,
    /// Tables of contents turned into links.
    pub toc: usize,
}

/// A parsed document and what each surface needs beside it.
#[derive(Clone)]
pub(crate) struct MarkdownBlocks {
    pub nodes: Rc<[MarkdownBlock]>,
    pub text: Rc<PlainText>,
    pub headings: Rc<[Heading]>,
    /// Diagrams the tree names that nobody has scheduled yet.
    pub missing_diagrams: Vec<DiagramRequest>,
    pub kinds: NodeCounts,
}

impl std::fmt::Debug for MarkdownBlocks {
    /// What is drawn is not content a log should carry: the sizes only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkdownBlocks")
            .field("nodes", &self.nodes.len())
            .field("bytes", &self.text.text.len())
            .finish()
    }
}

/// The options Ely's reader parses with.
fn reader_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
}

/// How deep `source` nests, or one past the cap when it nests deeper. Ely's reader and its
/// drawing recurse once per level, and chat and forge text is not ours: this walks the
/// event stream, which takes no stack, before any tree exists.
fn markdown_depth(source: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    for event in Parser::new_ext(source, reader_options()) {
        match event {
            Event::Start(_) => {
                depth += 1;
                deepest = deepest.max(depth);
                if depth > MAX_NESTING {
                    return depth;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// How deep `nodes` nest, a leaf counting one.
fn tree_depth(nodes: &[MarkdownBlock]) -> usize {
    (nodes.iter())
        .map(|node| {
            let inner = (rewrite::bodies(node).into_iter())
                .map(tree_depth)
                .max()
                .unwrap_or(0);
            1 + inner
        })
        .max()
        .unwrap_or(0)
}

/// `source` as the tree `policy.surface` draws.
pub(crate) fn build(source: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
    let mut missing = Vec::new();
    let mut nodes = vec![rewrite::plain_paragraph(source)];
    let depth = markdown_depth(source);
    if depth <= MAX_NESTING {
        // What depth the Markdown leaves is all block HTML may nest to, wherever it stands.
        let room = (MAX_NESTING - depth).min(MAX_HTML_NESTING);
        let parsed = parse_markdown_with(source, &|raw: &str| rewrite::html_block(raw, room));
        // Block HTML nests on top of the Markdown around it: each is under the cap, their
        // sum may not be, and every walk after this one recurses over the tree.
        if tree_depth(&parsed) <= MAX_NESTING {
            nodes = parsed;
            if policy.surface == Surface::Preview
                && let Some(inputs) = &policy.diagrams
            {
                nodes = rewrite::diagrams(nodes, inputs, &mut missing);
            }
            nodes = rewrite::pictures(nodes, policy);
            if policy.surface == Surface::Chat {
                rewrite::chat_links(&mut nodes, &mut policy.file_links);
            }
        }
    }

    // The headings are read before the tables of contents that link to them are made:
    // a list of links holds no heading, so the order of the others does not move.
    let headings = headings(&nodes);
    let (nodes, toc) = rewrite::tocs(nodes, &headings);

    let mut kinds = NodeCounts {
        toc,
        ..NodeCounts::default()
    };
    rewrite::each(&nodes, &mut |node| match node {
        MarkdownBlock::Math(_) => kinds.math += 1,
        MarkdownBlock::Paragraph(inline) | MarkdownBlock::Heading(_, inline) => {
            kinds.math += (inline.styles.iter())
                .filter(|(_, style)| *style == Style::Math)
                .count();
        }
        MarkdownBlock::Alert { .. } => kinds.alerts += 1,
        MarkdownBlock::Footnote { .. } => kinds.footnotes += 1,
        MarkdownBlock::Image { .. } => kinds.images += 1,
        MarkdownBlock::Table { .. } => kinds.tables += 1,
        MarkdownBlock::Code { .. } => kinds.code += 1,
        _ => {}
    });

    MarkdownBlocks {
        text: Rc::new(text::plain_text(&nodes)),
        nodes: Rc::from(nodes),
        headings: Rc::from(headings),
        missing_diagrams: missing,
        kinds,
    }
}

/// Every heading the renderer will draw, in the order it draws them (alert bodies and
/// footnotes included), with the anchor each one answers to.
fn headings(nodes: &[MarkdownBlock]) -> Vec<Heading> {
    let mut found = Vec::new();
    let mut seen = HashMap::new();
    rewrite::each(nodes, &mut |node| {
        if let MarkdownBlock::Heading(level, inline) = node {
            found.push(Heading {
                ordinal: found.len(),
                slug: slug(&inline.text, &mut seen).into(),
                title: inline.text.clone().into(),
                depth: usize::from(*level),
            });
        }
    });
    found
}

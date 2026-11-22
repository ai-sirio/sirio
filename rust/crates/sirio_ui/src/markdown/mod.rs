//! Markdown, parsed once with Ely's reader and rewritten for the surface that shows it.
//!
//! [`build`] reads a source into Ely's tree, then rewrites it by what the surface lets
//! through (pictures, links, diagrams, block HTML), and keeps beside it what selection,
//! copy and the anchors need: the plain text, the headings and their slugs.

use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::rc::Rc;

use ely_gpui_component::documents::{
    MarkdownBlock, Style, markdown_options, parse_markdown_guarded,
};
use gpui::SharedString;
use pulldown_cmark::{Event, Parser};

use crate::chat::file_links::FileLinkMemo;
use crate::markdown_preview::{DiagramRequest, Diagrams};

mod code;
pub(crate) mod cull;
mod gather;
mod menu;
mod rewrite;
mod sideways;
mod slug;
mod stream;
mod text;
mod view;

#[cfg(test)]
mod tests;

pub(crate) use gather::BodyLayouts;
#[cfg(test)]
pub(crate) use gather::DrawnPart;
pub(crate) use menu::{MarkdownMenuItem, perform};
pub(crate) use rewrite::{names_are_plain, picture_address};
pub(crate) use slug::slug;
pub(crate) use stream::StreamingDoc;
pub(crate) use text::{PartKind, PlainText};
pub(crate) use view::{
    BodyActions, LinkHandler, MarkdownView, MenuHandler, MenuPress, SelectionHost, ViewFlags,
    markdown_view, probe_view,
};

/// The deepest nesting drawn as written; past it the source is one plain paragraph. Ely's
/// reader, its element builder and gpui's layout recurse once per level, a level costs about
/// 115 KiB of stack in a debug build, and the cap must fit the smallest main thread: measured
/// by drawing quotes, lists and alerts, the deepest that draws is 71 levels in a release
/// build on 1 MiB (Windows) and 75 in a debug build on 8 MiB (Linux and macOS). The cap is
/// half the smaller of the two, rounded down to a multiple of 8, so the chat's extra wrappers
/// have room too. Real Markdown rarely nests past ten. A debug draw on 4 MiB (half of 8) and
/// a release one on 1 MiB (`the_deepest_allowed_nesting_is_drawn_on_a_four_mebibyte_stack`,
/// `..._on_a_release_one_mebibyte_stack`) go red when the cap is raised past that margin.
pub(crate) const MAX_NESTING: usize = 32;

/// The deepest block HTML is built: a reader sees nothing in more levels, and what is built
/// is walked, dropped and drawn by recursion on whatever stack the caller has.
pub(crate) const MAX_HTML_NESTING: usize = 32;

/// A panic a test makes Ely's reader raise, from the callback the reader calls about each text
/// it is about to parse: it unwinds through the reader's own frames, as one left in the vendored
/// code would. Compiled into tests only.
#[cfg(test)]
pub(crate) mod reader_fault {
    use std::cell::Cell;

    thread_local!(static ARMED: Cell<bool> = const { Cell::new(false) });

    /// While it lives, every read of a text on this thread panics.
    pub(crate) struct ReaderFault(());

    pub(crate) fn on() -> ReaderFault {
        ARMED.with(|armed| armed.set(true));
        ReaderFault(())
    }

    impl Drop for ReaderFault {
        fn drop(&mut self) {
            ARMED.with(|armed| armed.set(false));
        }
    }

    pub(super) fn strike() {
        if ARMED.with(Cell::get) {
            panic!("a panic injected into the Markdown reader");
        }
    }
}

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
    /// Ely's reader panicked on the source, and `nodes` is that source as plain text. A chat
    /// reply whose document carries it is not read again while it streams.
    pub reader_panicked: bool,
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

/// How deep `source`, the text Ely's parser reads, nests, or one past the cap when it nests
/// deeper. Ely's reader and its drawing recurse once per level, and chat and forge text is not
/// ours: this walks the event stream, which takes no stack, before any tree exists.
fn markdown_depth(source: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    for event in Parser::new_ext(source, markdown_options()) {
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

/// The text Ely reads for `source` (a byte order mark dropped, lone CRs as line ends, `$$` fences
/// as tildes): the first text `parts` hands the guard, taken without reading it into a tree. A
/// guard that refuses it ends the reading there.
pub(super) fn read_of(source: &str) -> String {
    let mut read = None;
    let _ = parse_markdown_guarded(
        source,
        &|_: &str| -> Option<Vec<MarkdownBlock>> { None },
        &mut |text| {
            read = Some(text.to_owned());
            false
        },
    );
    read.unwrap_or_default()
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

/// What the per-block half of [`build`] makes of one source: the nodes the surface draws, before
/// anything that looks across the whole reply (headings, tables of contents, counts, text).
pub(crate) struct Parts {
    pub nodes: Vec<MarkdownBlock>,
    pub missing: Vec<DiagramRequest>,
    /// The deepest nesting the guard accepted in any text Ely read for this source.
    pub deepest: usize,
    /// The guard refused the source (or block HTML made the tree too deep): `nodes` is the
    /// single plain paragraph.
    pub refused: bool,
    /// The guard refused a text after the first, so the first pass's tree was kept: the `\(`
    /// pairs of this source are not math, which a longer text around it may not repeat.
    pub declined: bool,
    /// Block HTML was read, so `deepest` (of this source and every other) decided its room.
    pub block_html: bool,
    /// The first text Ely read for this source (a byte order mark dropped, lone CRs as line
    /// ends, `$$` fences as tildes), the one the guard saw first.
    pub read: Option<String>,
    /// Ely's reader panicked on the source: `nodes` is the single plain paragraph.
    pub panicked: bool,
}

/// `source` as the tree `policy.surface` draws.
pub(crate) fn build(source: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
    let parts = parts(source, policy, Within::default(), false);
    join(parts.nodes, parts.missing, parts.panicked)
}

/// Where a source stands in the reply it is a part of, so that built alone it comes out as it
/// does in the whole.
#[derive(Clone, Copy, Default)]
struct Within {
    /// The deepest Markdown of the rest of the reply: block HTML nests to what it leaves.
    deepest: usize,
    /// Where the source starts in the reply, for the source offsets the tree keeps.
    offset: usize,
}

fn parts(source: &str, policy: &mut Policy<'_>, within: Within, keep_read: bool) -> Parts {
    let mut missing = Vec::new();
    let mut nodes = vec![rewrite::plain_paragraph(source)];
    let mut refused = true;
    // Ely reads another text than the one written (a byte order mark dropped, lone CRs as line
    // ends, `$$` fences as tildes), and reads `\(` pairs as math in a second pass over swapped
    // text. It asks the guard about each text it is about to parse, so the guard measures what
    // Ely reads, in one parse: a refused first text leaves the plain paragraph, a refused
    // second one keeps the first pass's tree. What depth the Markdown leaves is all block HTML
    // may nest to, wherever it stands (in the rest of the reply too, for a part of one); HTML is
    // read after every text was measured.
    //
    // The reader is vendored, and an `expect` left in it would take the app down from a reply:
    // a panic in it leaves the source as the plain paragraph, marked so that a caller can tell.
    let deepest = Cell::new(0usize);
    let (declined, block_html) = (Cell::new(false), Cell::new(false));
    let (mut read, mut texts) = (None, 0usize);
    let offset = within.offset;
    let parsed = catch_unwind(AssertUnwindSafe(|| {
        parse_markdown_guarded(
            source,
            &|raw: &str| {
                block_html.set(true);
                let room = MAX_NESTING
                    .saturating_sub(deepest.get().max(within.deepest))
                    .min(MAX_HTML_NESTING);
                let mut blocks = rewrite::html_block(raw, room)?;
                // The offsets of the tree are moved by `offset` below; what block HTML makes has
                // none, and is moved back by the same amount so that it ends where it began.
                stream::shift(&mut blocks, offset.wrapping_neg());
                Some(blocks)
            },
            &mut |text| {
                #[cfg(test)]
                reader_fault::strike();
                texts += 1;
                if keep_read && texts == 1 {
                    read = Some(text.to_owned());
                }
                let depth = markdown_depth(text);
                if depth <= MAX_NESTING {
                    deepest.set(deepest.get().max(depth));
                } else if texts > 1 {
                    declined.set(true);
                }
                depth <= MAX_NESTING
            },
        )
    }));
    let panicked = parsed.is_err();
    if let Ok(Some(parsed)) = parsed {
        // Block HTML nests on top of the Markdown around it: each is under the cap, their
        // sum may not be, and every walk after this one recurses over the tree.
        if tree_depth(&parsed) <= MAX_NESTING {
            let mut kept = parsed;
            if policy.surface == Surface::Preview
                && let Some(inputs) = &policy.diagrams
            {
                kept = rewrite::diagrams(kept, inputs, &mut missing);
            }
            // A failed diagram's note is a quote beside its fence, one level deeper: it counts
            // once it is inserted, and not before.
            if tree_depth(&kept) <= MAX_NESTING {
                nodes = rewrite::pictures(kept, policy);
                if policy.surface == Surface::Chat {
                    rewrite::chat_links(&mut nodes, &mut policy.file_links);
                }
                stream::shift(&mut nodes, offset);
                refused = false;
            } else {
                missing.clear();
            }
        }
    }
    Parts {
        nodes,
        missing,
        deepest: deepest.get(),
        refused,
        declined: declined.get(),
        block_html: block_html.get(),
        read,
        panicked,
    }
}

/// `source` as plain text, without asking Ely's reader: what a reply it panicked on shows while
/// it streams, so that the reader is not asked, and does not panic, again on each chunk.
pub(crate) fn as_plain_text(source: &str) -> MarkdownBlocks {
    join(vec![rewrite::plain_paragraph(source)], Vec::new(), true)
}

/// The half of [`build`] that looks across the reply: headings and their slugs, the tables of
/// contents that link to them, the counts and the plain text.
///
/// `reader_panicked` is carried to the document: `nodes` are then a plain paragraph of a source
/// Ely's reader panicked on.
pub(crate) fn join(
    nodes: Vec<MarkdownBlock>,
    missing: Vec<DiagramRequest>,
    reader_panicked: bool,
) -> MarkdownBlocks {
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
        reader_panicked,
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

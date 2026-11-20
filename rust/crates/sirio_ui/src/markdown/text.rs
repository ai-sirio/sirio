//! The plain text of a tree, in the renderer's push order (design §5.6).
//!
//! Drawing hands the gather every text it lays out, one by one, a list marker set aside
//! for the next one pushed. This walk reads the final tree in that same order, so a body's
//! text and part offsets are known without drawing it (a chat entry that is off screen
//! has a place in the transcript's one offset space all the same).

use std::ops::Range;

use ely_gpui_component::documents::{MarkdownBlock as Node, Style};

use super::rewrite::bodies;

/// A body's plain text, and where each drawn or silent text sits in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlainText {
    /// Blocks a blank line apart, the texts of one block a line end apart.
    pub text: String,
    pub parts: Vec<Part>,
}

/// One text of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    /// Its bytes in [`PlainText::text`], its list marker included.
    pub range: Range<usize>,
    /// How many bytes at the start of `range` are a list marker, not the text. The
    /// gather's own text starts that far into the part.
    pub lead: usize,
    pub kind: PartKind,
}

/// Whether the renderer pushes a text to the gather.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartKind {
    /// Drawn and pushed: it can be hit and highlighted.
    Drawn,
    /// Drawn without a push: only a selection that spans it, or Select All, copies it.
    Silent,
}

/// What one block pushes, before it has a place in the whole.
#[derive(Default)]
struct Walk {
    /// A list item's marker, kept for the next drawn text, as the gather keeps it.
    lead: String,
    /// Lists open around the node: a marker's indent and its bullet.
    lists: usize,
    parts: Vec<(String, usize, PartKind)>,
}

impl Walk {
    fn drawn(&mut self, text: &str) {
        let lead = std::mem::take(&mut self.lead);
        self.parts
            .push((format!("{lead}{text}"), lead.len(), PartKind::Drawn));
    }

    /// A silent text takes no marker: the renderer does not push it.
    fn silent(&mut self, text: &str) {
        self.parts.push((text.to_owned(), 0, PartKind::Silent));
    }

    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.node(node);
        }
    }

    fn node(&mut self, node: &Node) {
        match node {
            Node::Paragraph(inline) | Node::Heading(_, inline) => {
                // A formula's layout is not a text layout: the renderer does not push it.
                match inline.styles.iter().any(|(_, style)| *style == Style::Math) {
                    true => self.silent(&inline.text),
                    false => self.drawn(&inline.text),
                }
            }
            Node::Code { text, .. } => self.drawn(text),
            Node::Math(tex) => self.silent(tex),
            Node::List { start, items } => {
                let depth = self.lists;
                let bullet = match depth {
                    0 => "•",
                    1 => "◦",
                    _ => "▪",
                };
                self.lists += 1;
                for (ix, (task, body)) in items.iter().enumerate() {
                    let mark = match (task, start) {
                        (Some(true), _) => "[x]".to_owned(),
                        (Some(false), _) => "[ ]".to_owned(),
                        (None, Some(first)) => format!("{}.", first + ix as u64),
                        (None, None) => bullet.to_owned(),
                    };
                    self.lead = format!("{}{mark} ", "  ".repeat(depth));
                    self.nodes(body);
                    self.lead.clear();
                }
                self.lists = depth;
            }
            Node::Footnote { label, body } => {
                self.silent(&format!("[{label}]"));
                self.nodes(body);
            }
            Node::Alert { kind, body } => {
                self.silent(kind.title());
                self.nodes(body);
            }
            Node::Toc(headings) => {
                for heading in headings {
                    self.silent(&heading.title);
                }
            }
            Node::FrontMatter { text, .. } => self.silent(text),
            // Quotes and tables hold blocks; a rule and a picture hold no text.
            Node::Quote(_) | Node::Table { .. } => {
                for body in bodies(node) {
                    self.nodes(body);
                }
            }
            Node::Rule | Node::Image { .. } => {}
        }
    }
}

/// The plain text of `nodes`, one block per root node that has a text.
pub(crate) fn plain_text(nodes: &[Node]) -> PlainText {
    let mut out = PlainText::default();
    let mut first = true;
    for node in nodes {
        let mut walk = Walk::default();
        walk.node(node);
        if walk.parts.is_empty() {
            continue;
        }
        if !std::mem::take(&mut first) {
            out.text.push_str("\n\n");
        }
        for (ix, (text, lead, kind)) in walk.parts.into_iter().enumerate() {
            if ix > 0 {
                out.text.push('\n');
            }
            let start = out.text.len();
            out.text.push_str(&text);
            out.parts.push(Part {
                range: start..out.text.len(),
                lead,
                kind,
            });
        }
    }
    out
}

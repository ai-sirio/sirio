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
    /// The last thing the tree draws is a text the renderer pushes (a paragraph, a heading, code),
    /// so the last part is where its end is. A tree that ends in a picture, a rule, a formula or a
    /// table of contents has the last text above it, and an end that is not here.
    pub ends_in_text: bool,
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
    /// The separator written before this part (empty for the first): a blank line between
    /// blocks, a line end between texts of one block, a tab between the cells of a table row.
    pub sep: String,
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
    parts: Vec<(String, usize, PartKind, Option<String>)>,
    /// The separators of the table cells since the last text, for the next part pushed:
    /// an empty cell is no text and keeps only its separator. `None` is a line end.
    pending: Option<String>,
}

impl Walk {
    fn drawn(&mut self, text: &str) {
        let lead = std::mem::take(&mut self.lead);
        let sep = self.pending.take();
        self.parts
            .push((format!("{lead}{text}"), lead.len(), PartKind::Drawn, sep));
    }

    /// A silent text takes no marker: the renderer does not push it.
    fn silent(&mut self, text: &str) {
        let sep = self.pending.take();
        self.parts.push((text.to_owned(), 0, PartKind::Silent, sep));
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
            Node::Quote(_) => {
                for body in bodies(node) {
                    self.nodes(body);
                }
            }
            // A row reads as a spreadsheet row: cells by tabs, rows by line ends. Every cell
            // position adds its separator, whether the cell holds text or not.
            Node::Table { head, rows, .. } => {
                for (row, cells) in std::iter::once(head).chain(rows).enumerate() {
                    for (column, cell) in cells.iter().enumerate() {
                        let sep = match (row, column) {
                            (_, 1..) => "\t",
                            (0, 0) if self.parts.is_empty() => "",
                            _ => "\n",
                        };
                        self.pending.get_or_insert_with(String::new).push_str(sep);
                        self.nodes(cell);
                    }
                }
                // Separators no text followed have nothing to separate.
                self.pending = None;
            }
            Node::Rule | Node::Image { .. } => {}
        }
    }
}

/// Whether the last thing `nodes` draws is a text the renderer pushes to the gather.
fn ends_in_text(nodes: &[Node]) -> bool {
    match nodes.last() {
        Some(Node::Paragraph(inline) | Node::Heading(_, inline)) => {
            !inline.styles.iter().any(|(_, style)| *style == Style::Math)
        }
        Some(Node::Code { .. }) => true,
        Some(
            node @ (Node::Quote(_)
            | Node::List { .. }
            | Node::Footnote { .. }
            | Node::Alert { .. }
            | Node::Table { .. }),
        ) => bodies(node).last().is_some_and(|body| ends_in_text(body)),
        _ => false,
    }
}

/// The plain text of `nodes`, one block per root node that has a text.
pub(crate) fn plain_text(nodes: &[Node]) -> PlainText {
    let mut out = PlainText {
        ends_in_text: ends_in_text(nodes),
        ..PlainText::default()
    };
    let mut first = true;
    for node in nodes {
        let mut walk = Walk::default();
        walk.node(node);
        if walk.parts.is_empty() {
            continue;
        }
        let between = !std::mem::take(&mut first);
        for (ix, (text, lead, kind, sep)) in walk.parts.into_iter().enumerate() {
            let sep = match (ix, between, sep) {
                (0, true, sep) => format!("\n\n{}", sep.unwrap_or_default()),
                (0, false, sep) => sep.unwrap_or_default(),
                (_, _, sep) => sep.unwrap_or_else(|| "\n".to_owned()),
            };
            out.text.push_str(&sep);
            let start = out.text.len();
            out.text.push_str(&text);
            out.parts.push(Part {
                range: start..out.text.len(),
                lead,
                kind,
                sep,
            });
        }
    }
    out
}

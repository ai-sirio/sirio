//! The structured document model: blocks and inlines a renderer can walk.

/// A parsed markdown document: an ordered list of blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Top-level blocks, in source order.
    pub blocks: Vec<Block>,
}

impl Document {
    /// Returns the text a user should get when copying the rendered document.
    ///
    /// This deliberately describes the visible document rather than the
    /// markdown source: formatting delimiters and link destinations are not
    /// useful in a transcript selection. The renderer uses the same block
    /// and inline ordering when assigning selectable byte ranges.
    pub fn plain_text(&self) -> String {
        join_with(&self.blocks, "\n\n", Block::plain_text)
    }
}

/// A block-level element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `# Heading` — the inline content, without the `#` markers.
    Heading {
        /// 1 to 6.
        level: u8,
        /// Inline content.
        inline: Vec<Inline>,
    },
    /// A paragraph of inline content.
    Paragraph {
        /// Inline content.
        inline: Vec<Inline>,
    },
    /// A bullet or ordered list.
    List {
        /// Bullet vs ordered (with its start number).
        kind: ListKind,
        /// The list's items, in order.
        items: Vec<ListItem>,
        /// A tight list renders its items without paragraph wrapping
        /// (`<li>text</li>`, not `<li><p>text</p></li>`).
        tight: bool,
    },
    /// `> quoted` content, which may itself contain any blocks.
    BlockQuote {
        /// Blocks inside the quote, in order.
        blocks: Vec<Block>,
    },
    /// A fenced or indented code block.
    CodeBlock {
        /// The fence's info string (first word after the opening fence);
        /// `None` for indented code blocks and fenced blocks without a tag.
        language: Option<String>,
        /// The raw code text, exactly as written (including trailing
        /// newlines inside the fence).
        text: String,
        /// True when the source ended inside an unterminated fence: the
        /// block is still receiving content and the renderer should not draw
        /// a closing edge.
        open: bool,
    },
    /// A GFM table.
    Table {
        /// One alignment per column, in column order.
        alignment: Vec<Alignment>,
        /// The header row's cells.
        header: Vec<TableCell>,
        /// The body rows' cells, in order.
        rows: Vec<Vec<TableCell>>,
    },
    /// `---` / `***` / `___`.
    ThematicBreak,
    /// A raw HTML block, passed through untouched.
    Html {
        /// The raw HTML source of the block.
        text: String,
    },
}

impl Block {
    /// Returns the visible plain-text form of this block.
    pub fn plain_text(&self) -> String {
        match self {
            Self::Heading { inline, .. } | Self::Paragraph { inline } => {
                Inline::plain_text_all(inline)
            }
            Self::List { kind, items, .. } => items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let marker = match kind {
                        ListKind::Bullet => "•".to_string(),
                        ListKind::Ordered { start } => format!("{}.", start + index as u64),
                    };
                    let marker = item
                        .checked
                        .map(|checked| if checked { "☑" } else { "☐" })
                        .unwrap_or(&marker);
                    let content = join_with(&item.blocks, "\n", Block::plain_text);
                    if content.is_empty() {
                        marker.to_string()
                    } else {
                        format!("{marker} {content}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Self::BlockQuote { blocks } => join_with(blocks, "\n\n", Block::plain_text),
            Self::CodeBlock {
                language,
                text,
                open,
            } => {
                let label = language.as_deref().unwrap_or("code");
                let label = if *open {
                    format!("{label} · streaming")
                } else {
                    label.to_string()
                };
                format!("{label}\n{text}")
            }
            Self::Table { header, rows, .. } => {
                let mut rendered_rows = Vec::with_capacity(rows.len() + 1);
                rendered_rows.push(
                    header
                        .iter()
                        .map(|cell| Inline::plain_text_all(&cell.inline))
                        .collect::<Vec<_>>()
                        .join("\t"),
                );
                rendered_rows.extend(rows.iter().map(|row| {
                    row.iter()
                        .map(|cell| Inline::plain_text_all(&cell.inline))
                        .collect::<Vec<_>>()
                        .join("\t")
                }));
                rendered_rows.join("\n")
            }
            Self::ThematicBreak => String::new(),
            Self::Html { text } => text.clone(),
        }
    }
}

/// Whether a list is bulleted or ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// `- item` / `* item` / `+ item`.
    Bullet,
    /// `1. item` — carries the first item's number.
    Ordered {
        /// The number of the first item, as written (CommonMark allows a
        /// list to start at any number).
        start: u64,
    },
}

/// One item of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// The item's blocks, in order.
    pub blocks: Vec<Block>,
    /// `Some(true)` / `Some(false)` for `- [x]` / `- [ ]` task items;
    /// `None` for a plain item.
    pub checked: Option<bool>,
}

/// Table column alignment from the delimiter row (`:---`, `:--:`, `---:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// No marker: default alignment.
    None,
    /// `:---`
    Left,
    /// `:---:`
    Center,
    /// `---:`
    Right,
}

/// One table cell's inline content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    /// The cell's inline content.
    pub inline: Vec<Inline>,
}

/// An inline-level element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Plain text.
    Text(String),
    /// `*emphasized*` / `_emphasized_`.
    Emphasis(Vec<Inline>),
    /// `**strong**` / `__strong__`.
    Strong(Vec<Inline>),
    /// `` `inline code` `` — the code text without the backticks.
    Code(String),
    /// `[text](target "title")`.
    Link {
        /// The link destination.
        target: String,
        /// The optional title attribute.
        title: Option<String>,
        /// The link text.
        children: Vec<Inline>,
    },
    /// `![alt](target "title")` — the alt text is kept as plain text.
    Image {
        /// The image source.
        target: String,
        /// The optional title attribute.
        title: Option<String>,
        /// The alt text, flattened to plain text.
        alt: String,
    },
    /// A line break from a single newline inside a paragraph. A renderer may
    /// render this as a space or as a `<br>` depending on its `white-space`
    /// behaviour.
    SoftBreak,
    /// A hard line break (`two trailing spaces` or a backslash).
    HardBreak,
    /// Inline raw HTML, passed through untouched.
    Html(String),
}

impl Inline {
    /// Returns the visible plain-text form of this inline node.
    pub fn plain_text(&self) -> String {
        match self {
            Self::Text(text) | Self::Code(text) | Self::Html(text) => text.clone(),
            Self::Emphasis(children) | Self::Strong(children) | Self::Link { children, .. } => {
                Self::plain_text_all(children)
            }
            Self::Image { alt, .. } => alt.clone(),
            Self::SoftBreak | Self::HardBreak => "\n".to_string(),
        }
    }

    /// Flattens inline nodes without allocating one string per child.
    pub fn plain_text_all(inlines: &[Self]) -> String {
        let mut text = String::new();
        for inline in inlines {
            text.push_str(&inline.plain_text());
        }
        text
    }
}

fn join_with<T>(items: &[T], separator: &str, render: impl Fn(&T) -> String) -> String {
    items.iter().map(render).collect::<Vec<_>>().join(separator)
}

#[cfg(test)]
mod tests {
    use crate::parse;

    #[test]
    fn document_plain_text_matches_visible_markdown_content() {
        let document = parse(
            "# Heading\n\nA **strong** paragraph with `code` and [a link](https://example.com).\n\n- first\n- second\n\n```rust\nlet answer = 42;\n```",
        );

        assert_eq!(
            document.plain_text(),
            "Heading\n\nA strong paragraph with code and a link.\n\n• first\n• second\n\nrust\nlet answer = 42;\n"
        );
    }
}

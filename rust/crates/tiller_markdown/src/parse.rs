//! `pulldown-cmark`'s flat event stream turned into the [`Document`] tree.

use pulldown_cmark::{
    Alignment as CmarkAlignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::model::{Alignment, Block, Document, Inline, ListItem, ListKind, TableCell};
use crate::scan::ends_inside_fence;

/// Maximum nesting retained in the document tree.
///
/// Markdown nesting beyond this point is not useful to a reader, but it is
/// easy for an external model response to produce accidentally.  The parser
/// flattens the remainder at this boundary instead of allowing recursive
/// descent to grow with untrusted input.
const MAX_NESTING_DEPTH: usize = 128;

/// The option set a streaming chat reply is parsed with: tables and task
/// lists are part of the supported surface, everything else stays
/// CommonMark.
fn default_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS
}

/// Parses `source` into a [`Document`] with the default options
/// (tables and task lists enabled).
pub fn parse(source: &str) -> Document {
    parse_with_options(source, default_options())
}

/// Parses `source` into a [`Document`] with explicit [`Options`].
///
/// Safe on any prefix of a document: truncated input never panics, blocks
/// that were already complete are never lost, and a source ending inside an
/// unterminated code fence yields that code block with `open = true`.
pub fn parse_with_options(source: &str, options: Options) -> Document {
    let events = Parser::new_ext(source, options);
    let mut builder = Builder {
        events: events.peekable(),
        depth: 0,
    };
    let mut blocks = builder.parse_blocks();
    if ends_inside_fence(source) {
        // The source ended inside an unclosed fence; pulldown still closed
        // the block at EOF, so mark the last code block (which may sit
        // inside a quote or list item) as open.
        if let Some(Block::CodeBlock { open, .. }) = last_code_block_mut(&mut blocks) {
            *open = true;
        }
    }
    Document { blocks }
}

/// The last code block in document order, possibly nested inside the last
/// chain of containers.
fn last_code_block_mut(blocks: &mut [Block]) -> Option<&mut Block> {
    let mut current = blocks;
    loop {
        let block = current.last_mut()?;
        match block {
            Block::CodeBlock { .. } => return Some(block),
            Block::BlockQuote { blocks } => current = blocks,
            Block::List { items, .. } => current = &mut items.last_mut()?.blocks,
            _ => return None,
        }
    }
}

struct Builder<'a> {
    events: std::iter::Peekable<Parser<'a>>,
    depth: usize,
}

impl<'a> Builder<'a> {
    /// Parses blocks until the enclosing container's `End` event (or the
    /// end of input). The matching `End` is consumed.
    fn parse_blocks(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some(event) = self.events.next() {
            match event {
                Event::Start(tag) => blocks.push(self.parse_block(tag)),
                Event::End(_) => break,
                Event::Rule => blocks.push(Block::ThematicBreak),
                Event::Html(html) => blocks.push(Block::Html {
                    text: html.into_string(),
                }),
                // Block-level stray text (cannot happen in a balanced
                // stream; kept so no content is ever dropped).
                Event::Text(text) => blocks.push(Block::Paragraph {
                    inline: vec![Inline::Text(text.into_string())],
                }),
                _ => {}
            }
        }
        blocks
    }

    /// Parses one block, consuming its `Start` .. `End` range.
    fn parse_block(&mut self, tag: Tag<'a>) -> Block {
        if self.depth >= MAX_NESTING_DEPTH {
            return self.flatten_block();
        }

        self.depth += 1;
        let block = self.parse_block_inner(tag);
        self.depth -= 1;
        block
    }

    /// Parses one block after the depth guard has admitted it.
    fn parse_block_inner(&mut self, tag: Tag<'a>) -> Block {
        match tag {
            Tag::Paragraph => Block::Paragraph {
                inline: self.parse_inlines(TagEnd::Paragraph),
            },
            Tag::Heading { level, .. } => Block::Heading {
                level: heading_level(level),
                inline: self.parse_inlines(TagEnd::Heading(level)),
            },
            Tag::BlockQuote(_) => Block::BlockQuote {
                blocks: self.parse_blocks(),
            },
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(language) if !language.is_empty() => {
                        Some(language.into_string())
                    }
                    _ => None,
                };
                let mut text = String::new();
                while let Some(event) = self.events.next() {
                    match event {
                        Event::Text(chunk) => text.push_str(&chunk),
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::SoftBreak => text.push('\n'),
                        _ => {}
                    }
                }
                Block::CodeBlock {
                    language,
                    text,
                    open: false,
                }
            }
            Tag::HtmlBlock => {
                let mut text = String::new();
                while let Some(event) = self.events.next() {
                    match event {
                        Event::Html(chunk) => text.push_str(&chunk),
                        Event::End(TagEnd::HtmlBlock) => break,
                        Event::Text(chunk) => text.push_str(&chunk),
                        _ => {}
                    }
                }
                Block::Html { text }
            }
            Tag::List(start) => {
                let kind = match start {
                    Some(start) => ListKind::Ordered { start },
                    None => ListKind::Bullet,
                };
                let mut items = Vec::new();
                let mut saw_paragraph = false;
                loop {
                    match self.events.next() {
                        Some(Event::Start(Tag::Item)) => {
                            let (item, wrapped) = self.parse_list_item();
                            saw_paragraph |= wrapped;
                            items.push(item);
                        }
                        Some(Event::End(TagEnd::List(_))) | None => break,
                        _ => {}
                    }
                }
                // pulldown wraps an item's paragraph in a `Paragraph` tag
                // only when the list is loose; a tight item's inlines arrive
                // unwrapped. Either way the model stores the content as a
                // Paragraph block — the flag tells the renderer whether to
                // actually draw the `<p>`.
                let tight = !saw_paragraph;
                Block::List { kind, items, tight }
            }
            Tag::Table(alignment) => {
                let alignment = alignment.into_iter().map(map_alignment).collect();
                let mut header = Vec::new();
                let mut rows = Vec::new();
                loop {
                    match self.events.next() {
                        Some(Event::Start(Tag::TableHead)) => {
                            header = self.parse_table_row();
                        }
                        Some(Event::Start(Tag::TableRow)) => {
                            rows.push(self.parse_table_row());
                        }
                        Some(Event::End(TagEnd::Table)) | None => break,
                        _ => {}
                    }
                }
                Block::Table {
                    alignment,
                    header,
                    rows,
                }
            }
            // Span-level or unknown tags at block level cannot appear in a
            // balanced stream; parse their content as inlines and keep it.
            Tag::Emphasis => Block::Paragraph {
                inline: self.parse_inlines(TagEnd::Emphasis),
            },
            Tag::Strong => Block::Paragraph {
                inline: self.parse_inlines(TagEnd::Strong),
            },
            Tag::Link { .. } => Block::Paragraph {
                inline: self.parse_inlines(TagEnd::Link),
            },
            Tag::Image { .. } => Block::Paragraph {
                inline: self.parse_inlines(TagEnd::Image),
            },
            _ => {
                // Unknown container (footnotes etc. are disabled by the
                // default options): consume until its End so the stream
                // stays balanced.
                while let Some(event) = self.events.next() {
                    match event {
                        Event::End(_) => break,
                        Event::Start(_) => self.skip_container(),
                        _ => {}
                    }
                }
                Block::Paragraph { inline: Vec::new() }
            }
        }
    }

    /// Skips one unknown container, consuming nested containers recursively.
    fn skip_container(&mut self) {
        let mut depth = 1usize;
        while let Some(event) = self.events.next() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    /// Parses one list item: the optional task marker, then its blocks.
    ///
    /// A tight item's inline content arrives without a `Paragraph` wrapper,
    /// so stray inline events are accumulated and flushed as a Paragraph
    /// block when a block starts or the item ends. Returns whether any
    /// `Paragraph` tag was seen directly inside the item (the list is loose
    /// if any item was wrapped).
    fn parse_list_item(&mut self) -> (ListItem, bool) {
        let mut checked = None;
        let mut blocks = Vec::new();
        let mut pending_inline: Vec<Inline> = Vec::new();
        let mut wrapped = false;
        while let Some(event) = self.events.next() {
            match event {
                Event::Start(tag) => {
                    if matches!(tag, Tag::Paragraph) {
                        wrapped = true;
                    }
                    self.flush_inline(&mut pending_inline, &mut blocks);
                    blocks.push(self.parse_block(tag));
                }
                Event::End(TagEnd::Item) => {
                    self.flush_inline(&mut pending_inline, &mut blocks);
                    break;
                }
                Event::Rule => {
                    self.flush_inline(&mut pending_inline, &mut blocks);
                    blocks.push(Block::ThematicBreak);
                }
                Event::TaskListMarker(marked) => checked = Some(marked),
                Event::Html(html) => {
                    self.flush_inline(&mut pending_inline, &mut blocks);
                    blocks.push(Block::Html {
                        text: html.into_string(),
                    });
                }
                // Tight-item inlines: Text, Code, breaks, inline HTML and
                // span tags (emphasis, strong, links, images).
                other => self.push_inline_event(other, &mut pending_inline),
            }
        }
        (ListItem { blocks, checked }, wrapped)
    }

    /// Pushes one inline-level event onto `pending_inline`.
    fn push_inline_event(&mut self, event: Event<'a>, pending: &mut Vec<Inline>) {
        match event {
            Event::Text(text) => pending.push(Inline::Text(text.into_string())),
            Event::Code(code) => pending.push(Inline::Code(code.into_string())),
            Event::InlineHtml(html) => pending.push(Inline::Html(html.into_string())),
            Event::SoftBreak => pending.push(Inline::SoftBreak),
            Event::HardBreak => pending.push(Inline::HardBreak),
            Event::Start(Tag::Emphasis) => {
                pending.push(Inline::Emphasis(self.parse_inlines(TagEnd::Emphasis)));
            }
            Event::Start(Tag::Strong) => {
                pending.push(Inline::Strong(self.parse_inlines(TagEnd::Strong)));
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let target = dest_url.into_string();
                let title = optional_title(title);
                let children = self.parse_inlines(TagEnd::Link);
                pending.push(Inline::Link {
                    target,
                    title,
                    children,
                });
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let target = dest_url.into_string();
                let title = optional_title(title);
                let children = self.parse_inlines(TagEnd::Image);
                let mut alt = String::new();
                for inline in &children {
                    push_plain_text(inline, &mut alt);
                }
                pending.push(Inline::Image { target, title, alt });
            }
            _ => {}
        }
    }

    /// Flushes accumulated tight-item inlines into a Paragraph block.
    fn flush_inline(&mut self, pending: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
        if !pending.is_empty() {
            blocks.push(Block::Paragraph {
                inline: std::mem::take(pending),
            });
        }
    }

    /// Parses one table row: `Start(TableRow)` .. `End(TableRow)` of cells.
    fn parse_table_row(&mut self) -> Vec<TableCell> {
        let mut cells = Vec::new();
        while let Some(event) = self.events.next() {
            match event {
                Event::Start(Tag::TableCell) => cells.push(TableCell {
                    inline: self.parse_inlines(TagEnd::TableCell),
                }),
                Event::End(TagEnd::TableRow) | Event::End(TagEnd::TableHead) => break,
                _ => {}
            }
        }
        cells
    }

    /// Parses inline content until `until`'s matching `End` (consumed).
    fn parse_inlines(&mut self, until: TagEnd) -> Vec<Inline> {
        if self.depth >= MAX_NESTING_DEPTH {
            return self.flatten_inlines();
        }

        self.depth += 1;
        let out = self.parse_inlines_inner(until);
        self.depth -= 1;
        out
    }

    /// Parses inline content after the depth guard has admitted it.
    fn parse_inlines_inner(&mut self, until: TagEnd) -> Vec<Inline> {
        let mut out = Vec::new();
        while let Some(event) = self.events.next() {
            match event {
                Event::Text(text) => out.push(Inline::Text(text.into_string())),
                Event::Code(code) => out.push(Inline::Code(code.into_string())),
                Event::InlineHtml(html) => out.push(Inline::Html(html.into_string())),
                Event::SoftBreak => out.push(Inline::SoftBreak),
                Event::HardBreak => out.push(Inline::HardBreak),
                Event::Start(Tag::Emphasis) => {
                    out.push(Inline::Emphasis(self.parse_inlines(TagEnd::Emphasis)));
                }
                Event::Start(Tag::Strong) => {
                    out.push(Inline::Strong(self.parse_inlines(TagEnd::Strong)));
                }
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) => {
                    let target = dest_url.into_string();
                    let title = optional_title(title);
                    let children = self.parse_inlines(TagEnd::Link);
                    out.push(Inline::Link {
                        target,
                        title,
                        children,
                    });
                }
                Event::Start(Tag::Image {
                    dest_url, title, ..
                }) => {
                    let target = dest_url.into_string();
                    let title = optional_title(title);
                    let children = self.parse_inlines(TagEnd::Image);
                    let mut alt = String::new();
                    for inline in &children {
                        push_plain_text(inline, &mut alt);
                    }
                    out.push(Inline::Image { target, title, alt });
                }
                Event::End(_) => break,
                _ => {}
            }
        }
        let _ = until;
        out
    }

    /// Consumes the current block container without recursive descent.
    ///
    /// The opening `Start` event has already been consumed by the caller, so
    /// a nesting count of one means the matching closing event ends this
    /// container.  Textual content is preserved as a flat paragraph.
    fn flatten_block(&mut self) -> Block {
        let text = self.flatten_events_to_text(1);
        Block::Paragraph {
            inline: (!text.is_empty())
                .then(|| Inline::Text(text))
                .into_iter()
                .collect(),
        }
    }

    /// Consumes a too-deep inline container without recursive descent.
    fn flatten_inlines(&mut self) -> Vec<Inline> {
        let text = self.flatten_events_to_text(1);
        (!text.is_empty())
            .then(|| Inline::Text(text))
            .into_iter()
            .collect()
    }

    /// Turns all events in the current container into plain text iteratively.
    fn flatten_events_to_text(&mut self, mut nesting: usize) -> String {
        let mut text = String::new();
        while let Some(event) = self.events.next() {
            match event {
                Event::Start(_) => nesting += 1,
                Event::End(_) => {
                    nesting = nesting.saturating_sub(1);
                    if nesting == 0 {
                        break;
                    }
                }
                Event::Text(value)
                | Event::Code(value)
                | Event::Html(value)
                | Event::InlineHtml(value) => text.push_str(&value),
                Event::SoftBreak | Event::HardBreak => text.push('\n'),
                Event::Rule => text.push_str("\n---\n"),
                Event::TaskListMarker(checked) => {
                    text.push_str(if checked { "[x] " } else { "[ ] " });
                }
                _ => {}
            }
        }
        text
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    level as u8
}

fn optional_title(title: pulldown_cmark::CowStr<'_>) -> Option<String> {
    if title.is_empty() {
        None
    } else {
        Some(title.into_string())
    }
}

fn map_alignment(alignment: CmarkAlignment) -> Alignment {
    match alignment {
        CmarkAlignment::None => Alignment::None,
        CmarkAlignment::Left => Alignment::Left,
        CmarkAlignment::Center => Alignment::Center,
        CmarkAlignment::Right => Alignment::Right,
    }
}

/// Flattens an inline tree to its plain text (used for image alt text).
fn push_plain_text(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) => out.push_str(text),
        Inline::Emphasis(children) | Inline::Strong(children) | Inline::Link { children, .. } => {
            for child in children {
                push_plain_text(child, out);
            }
        }
        Inline::Code(code) => out.push_str(code),
        Inline::SoftBreak | Inline::HardBreak => out.push('\n'),
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::Html(html) => out.push_str(html),
    }
}

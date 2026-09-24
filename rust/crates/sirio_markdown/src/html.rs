//! Raw HTML inside Markdown, translated into the document model.
//!
//! The file view's Preview draws through bezel, which has no HTML engine, so
//! nothing here *renders* HTML. The GitHub-style subset a README uses is
//! mapped onto blocks and inlines the model already has, and everything else
//! is reduced to its text (design §4,
//! `docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md`).
//!
//! pulldown-cmark has already split the HTML. Inline HTML arrives as one
//! [`Inline::Html`] per tag, between the text it wraps, so an inline list is
//! folded with a tag stack ([`InlineFold`]). A block of HTML arrives as one
//! [`Block::Html`] chunk, tokenised with `html5gum`.

use std::collections::BTreeMap;

use html5gum::{DefaultEmitter, Token, Tokenizer};

use crate::model::{Block, Document, Inline, ListItem, TableCell};

/// Translates the raw HTML in `document`. Applied by the Preview only:
/// [`crate::parse`] stays a faithful CommonMark model.
pub fn expand_html(document: Document) -> Document {
    Document {
        blocks: expand_blocks(document.blocks),
    }
}

fn expand_blocks(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            Block::Paragraph { inline } => {
                let inline = expand_inlines(inline);
                if !is_blank(&inline) {
                    out.push(Block::Paragraph { inline });
                }
            }
            Block::Heading { level, inline } => out.push(Block::Heading {
                level,
                inline: expand_inlines(inline),
            }),
            Block::List { kind, items, tight } => out.push(Block::List {
                kind,
                tight,
                items: items
                    .into_iter()
                    .map(|item| ListItem {
                        blocks: expand_blocks(item.blocks),
                        checked: item.checked,
                    })
                    .collect(),
            }),
            Block::BlockQuote { blocks } => out.push(Block::BlockQuote {
                blocks: expand_blocks(blocks),
            }),
            Block::Table {
                alignment,
                header,
                rows,
            } => out.push(Block::Table {
                alignment,
                header: expand_cells(header),
                rows: rows.into_iter().map(expand_cells).collect(),
            }),
            // Code, rules and HTML blocks carry no inline content.
            other => out.push(other),
        }
    }
    out
}

fn expand_cells(cells: Vec<TableCell>) -> Vec<TableCell> {
    cells
        .into_iter()
        .map(|cell| TableCell {
            inline: expand_inlines(cell.inline),
        })
        .collect()
}

/// Folds an inline list's HTML tags into the inlines they stand for,
/// recursing into the children of Markdown's own spans.
fn expand_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut fold = InlineFold::default();
    for inline in inlines {
        match inline {
            Inline::Html(fragment) => {
                for piece in pieces(&fragment) {
                    fold.piece(piece);
                }
            }
            Inline::Strong(children) => fold.push(Inline::Strong(expand_inlines(children))),
            Inline::Emphasis(children) => fold.push(Inline::Emphasis(expand_inlines(children))),
            Inline::Link {
                target,
                title,
                children,
            } => fold.push(Inline::Link {
                target,
                title,
                children: expand_inlines(children),
            }),
            other => fold.push(other),
        }
    }
    fold.finish()
}

/// One HTML token, reduced to what the translation reads.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Open {
        name: String,
        attrs: BTreeMap<String, String>,
    },
    Close {
        name: String,
    },
    Text(String),
}

/// Tokenises an HTML fragment. Comments, doctypes and parse errors are
/// dropped; tag names arrive lower-cased and entities decoded. A void
/// element (`<img>`, `<br>`, …) is followed by its own `Close`, so nothing
/// downstream waits for an end tag that never comes.
///
/// `naively_switch_states` is what makes a `<script>` or `<style>` body raw
/// text: without it, the `"<b>"` in `var a = "<b>"` is read as a tag.
fn pieces(fragment: &str) -> Vec<Piece> {
    let mut emitter = DefaultEmitter::default();
    emitter.naively_switch_states(true);
    let mut out = Vec::new();
    for token in Tokenizer::new_with_emitter(fragment, emitter) {
        let Ok(token) = token;
        match token {
            Token::StartTag(tag) => {
                let name = lossy(&tag.name);
                let attrs = tag
                    .attributes
                    .iter()
                    .map(|(key, value)| (lossy(key), lossy(&value.value)))
                    .collect();
                let void = tag.self_closing || is_void(&name);
                out.push(Piece::Open {
                    name: name.clone(),
                    attrs,
                });
                if void {
                    out.push(Piece::Close { name });
                }
            }
            Token::EndTag(tag) => out.push(Piece::Close {
                name: lossy(&tag.name),
            }),
            Token::String(text) => out.push(Piece::Text(lossy(&text.value))),
            Token::Comment(_) | Token::Doctype(_) | Token::Error(_) => {}
        }
    }
    out
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Elements with no end tag.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Elements removed together with everything inside them (design §4). Under
/// the general "drop the tag, keep the text" rule a `<style>` would surface
/// as a paragraph of CSS.
fn is_removed_with_content(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "iframe" | "object" | "embed" | "form" | "svg"
    )
}

/// Advances a removed-with-content element's nesting. True while inside it,
/// including for the piece that closes it.
fn skip(skipping: &mut Option<(String, usize)>, piece: &Piece) -> bool {
    let Some((name, depth)) = skipping else {
        return false;
    };
    match piece {
        Piece::Open { name: open, .. } if open == name => *depth += 1,
        Piece::Close { name: close } if close == name => {
            *depth -= 1;
            if *depth == 0 {
                *skipping = None;
            }
        }
        _ => {}
    }
    true
}

/// Whitespace-only inline content: a paragraph of it is not drawn.
fn is_blank(inlines: &[Inline]) -> bool {
    inlines.iter().all(|inline| match inline {
        Inline::Text(text) => text.trim().is_empty(),
        Inline::SoftBreak | Inline::HardBreak => true,
        _ => false,
    })
}

/// Joins adjacent text runs, so a dropped tag leaves one run rather than
/// several that happen to touch.
fn merge_texts(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        if let (Some(Inline::Text(previous)), Inline::Text(next)) = (out.last_mut(), &inline) {
            previous.push_str(next);
            continue;
        }
        out.push(inline);
    }
    out
}

/// What an open inline tag becomes once it closes.
#[derive(Debug)]
enum Wrap {
    Strong,
    Emphasis,
    Code,
    Link(String),
    /// A tag whose own meaning is dropped: its children stay.
    Transparent,
}

impl Wrap {
    fn apply(self, children: Vec<Inline>) -> Vec<Inline> {
        match self {
            Wrap::Strong => vec![Inline::Strong(children)],
            Wrap::Emphasis => vec![Inline::Emphasis(children)],
            Wrap::Code => vec![Inline::Code(Inline::plain_text_all(&children))],
            Wrap::Link(target) => vec![Inline::Link {
                target,
                title: None,
                children,
            }],
            Wrap::Transparent => children,
        }
    }
}

#[derive(Debug)]
struct Frame {
    name: String,
    wrap: Wrap,
    children: Vec<Inline>,
}

/// Inline tags folded with a stack. Only the innermost open tag can close:
/// an end tag that matches anything else (a crossed tag) or nothing (a
/// stray one) is dropped, and a tag still open at the end unwraps. In every
/// malformed case the tag is lost and the text is kept.
#[derive(Debug, Default)]
struct InlineFold {
    root: Vec<Inline>,
    stack: Vec<Frame>,
    /// Inside a removed-with-content element: its name and nesting depth.
    skipping: Option<(String, usize)>,
}

impl InlineFold {
    fn push(&mut self, inline: Inline) {
        if self.skipping.is_some() {
            return;
        }
        match self.stack.last_mut() {
            Some(frame) => frame.children.push(inline),
            None => self.root.push(inline),
        }
    }

    fn piece(&mut self, piece: Piece) {
        if skip(&mut self.skipping, &piece) {
            return;
        }
        match piece {
            Piece::Text(text) => self.push(Inline::Text(text)),
            Piece::Open { name, attrs } => self.open(name, &attrs),
            Piece::Close { name } => self.close(&name),
        }
    }

    fn open(&mut self, name: String, attrs: &BTreeMap<String, String>) {
        if is_removed_with_content(&name) {
            self.skipping = Some((name, 1));
            return;
        }
        let wrap = match name.as_str() {
            "br" => return self.push(Inline::HardBreak),
            "img" => return self.push(image(attrs)),
            "b" | "strong" => Wrap::Strong,
            "i" | "em" => Wrap::Emphasis,
            "code" | "kbd" | "samp" | "tt" => Wrap::Code,
            "a" => match attrs.get("href") {
                Some(href) => Wrap::Link(href.clone()),
                None => Wrap::Transparent,
            },
            _ if is_void(&name) => return,
            _ => Wrap::Transparent,
        };
        self.stack.push(Frame {
            name,
            wrap,
            children: Vec::new(),
        });
    }

    fn close(&mut self, name: &str) {
        if self.stack.last().is_none_or(|frame| frame.name != name) {
            return;
        }
        let frame = self
            .stack
            .pop()
            .expect("the innermost frame was just checked");
        for inline in frame.wrap.apply(merge_texts(frame.children)) {
            self.push(inline);
        }
    }

    fn finish(mut self) -> Vec<Inline> {
        self.skipping = None;
        while let Some(frame) = self.stack.pop() {
            for inline in frame.children {
                self.push(inline);
            }
        }
        merge_texts(self.root)
    }
}

/// An `<img>` as an image inline. One without a `src` has nothing to draw,
/// so its alt text stands in.
fn image(attrs: &BTreeMap<String, String>) -> Inline {
    let alt = attrs.get("alt").cloned().unwrap_or_default();
    match attrs
        .get("src")
        .map(|src| src.trim())
        .filter(|src| !src.is_empty())
    {
        Some(src) => Inline::Image {
            target: src.to_string(),
            title: attrs.get("title").cloned(),
            alt,
            width: attrs.get("width").and_then(|width| pixel_width(width)),
        },
        None => Inline::Text(alt),
    }
}

/// `120` and `120px` are pixels. A percentage is not a width bezel can
/// honour, so the picture keeps its natural size.
fn pixel_width(value: &str) -> Option<u32> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|width| *width > 0)
}

#[cfg(test)]
mod tests {
    use super::expand_html;
    use crate::model::{Block, Inline};
    use crate::parse;

    fn inlines(source: &str) -> Vec<Inline> {
        match expand_html(parse(source)).blocks.as_slice() {
            [Block::Paragraph { inline }] => inline.clone(),
            other => panic!("expected one paragraph, got {other:?}"),
        }
    }

    fn text(value: &str) -> Inline {
        Inline::Text(value.to_string())
    }

    #[test]
    fn b_and_strong_become_strong() {
        assert_eq!(
            inlines("a <b>bold</b> and <strong>strong</strong>"),
            vec![
                text("a "),
                Inline::Strong(vec![text("bold")]),
                text(" and "),
                Inline::Strong(vec![text("strong")]),
            ]
        );
    }

    #[test]
    fn i_and_em_become_emphasis() {
        assert_eq!(
            inlines("a <i>x</i> <em>y</em>"),
            vec![
                text("a "),
                Inline::Emphasis(vec![text("x")]),
                text(" "),
                Inline::Emphasis(vec![text("y")]),
            ]
        );
    }

    #[test]
    fn code_kbd_samp_and_tt_become_code() {
        assert_eq!(
            inlines("press <kbd>Ctrl</kbd>+<samp>C</samp> or <code>ls</code> <tt>t</tt>"),
            vec![
                text("press "),
                Inline::Code("Ctrl".into()),
                text("+"),
                Inline::Code("C".into()),
                text(" or "),
                Inline::Code("ls".into()),
                text(" "),
                Inline::Code("t".into()),
            ]
        );
    }

    #[test]
    fn a_with_href_becomes_a_link() {
        assert_eq!(
            inlines("see <a href=\"docs/a.md\">the docs</a>"),
            vec![
                text("see "),
                Inline::Link {
                    target: "docs/a.md".into(),
                    title: None,
                    children: vec![text("the docs")],
                },
            ]
        );
    }

    #[test]
    fn a_without_href_keeps_its_text() {
        assert_eq!(inlines("x <a name=\"top\">here</a>"), vec![text("x here")]);
    }

    #[test]
    fn br_becomes_a_hard_break() {
        assert_eq!(
            inlines("one<br>two<br/>three"),
            vec![
                text("one"),
                Inline::HardBreak,
                text("two"),
                Inline::HardBreak,
                text("three"),
            ]
        );
    }

    #[test]
    fn img_becomes_an_image_with_its_pixel_width() {
        assert_eq!(
            inlines("logo <img src=\"a.png\" alt=\"A\" width=\"120px\"> end"),
            vec![
                text("logo "),
                Inline::Image {
                    target: "a.png".into(),
                    title: None,
                    alt: "A".into(),
                    width: Some(120),
                },
                text(" end"),
            ]
        );
    }

    #[test]
    fn a_percentage_width_is_not_a_width() {
        assert_eq!(
            inlines("x <img src=\"a.png\" width=\"50%\">"),
            vec![
                text("x "),
                Inline::Image {
                    target: "a.png".into(),
                    title: None,
                    alt: String::new(),
                    width: None,
                },
            ]
        );
    }

    #[test]
    fn img_without_src_leaves_its_alt_text() {
        assert_eq!(inlines("x <img alt=\"Logo\">"), vec![text("x Logo")]);
    }

    #[test]
    fn a_comment_is_removed() {
        assert_eq!(inlines("a <!-- hidden --> b"), vec![text("a  b")]);
    }

    #[test]
    fn an_unknown_tag_keeps_its_text() {
        assert_eq!(inlines("H<sub>2</sub>O"), vec![text("H2O")]);
    }

    #[test]
    fn a_crossed_tag_is_dropped_and_its_text_kept() {
        assert_eq!(
            inlines("w <b>x<i>y</b>z</i>"),
            vec![text("w x"), Inline::Emphasis(vec![text("yz")])]
        );
    }

    #[test]
    fn an_unclosed_tag_is_dropped_and_its_text_kept() {
        assert_eq!(inlines("w <b>open"), vec![text("w open")]);
    }

    #[test]
    fn a_script_is_removed_with_its_content() {
        assert_eq!(inlines("a <script>alert(1)</script> b"), vec![text("a  b")]);
    }

    #[test]
    fn html_inside_markdown_spans_is_expanded() {
        assert_eq!(
            inlines("**<i>x</i>**"),
            vec![Inline::Strong(vec![Inline::Emphasis(vec![text("x")])])]
        );
    }

    #[test]
    fn markdown_images_have_no_width() {
        assert_eq!(
            inlines("![alt](a.png)"),
            vec![Inline::Image {
                target: "a.png".into(),
                title: None,
                alt: "alt".into(),
                width: None,
            }]
        );
    }
}

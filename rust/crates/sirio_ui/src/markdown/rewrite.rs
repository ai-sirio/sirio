//! What Sirio does to a parsed tree before it is drawn (design §5.5): block HTML in,
//! diagrams, pictures and links by surface, tables of contents.
//!
//! Every rewrite reaches every depth of the tree (lists, quotes, table cells, alerts,
//! footnotes), because a hostile source can put a picture or a link anywhere.

use ely_gpui_component::documents::{AlignKind, Inline, MarkdownBlock as Node, Style};
use sirio_diagram::DiagramKind;
use sirio_markdown::{
    Alignment, Block, Document, Inline as Leaf, ListKind, TableCell, expand_html_capped,
};

use super::{DiagramInputs, FileLinkInputs, Heading, Policy, Surface};
use crate::chat::file_links;
use crate::markdown_preview::{DiagramRequest, DiagramState};

// ---- walking ----------------------------------------------------------------------------

/// The block lists `node` holds, in drawing order: a list's items, a table's head cells
/// then its rows' cells.
pub(super) fn bodies(node: &Node) -> Vec<&[Node]> {
    match node {
        Node::Quote(body) | Node::Footnote { body, .. } | Node::Alert { body, .. } => {
            vec![body]
        }
        Node::List { items, .. } => items.iter().map(|(_, body)| body.as_slice()).collect(),
        Node::Table { head, rows, .. } => head
            .iter()
            .chain(rows.iter().flatten())
            .map(Vec::as_slice)
            .collect(),
        _ => Vec::new(),
    }
}

fn bodies_mut(node: &mut Node) -> Vec<&mut Vec<Node>> {
    match node {
        Node::Quote(body) | Node::Footnote { body, .. } | Node::Alert { body, .. } => {
            vec![body]
        }
        Node::List { items, .. } => items.iter_mut().map(|(_, body)| body).collect(),
        Node::Table { head, rows, .. } => head.iter_mut().chain(rows.iter_mut().flatten()).collect(),
        _ => Vec::new(),
    }
}

fn is_container(node: &Node) -> bool {
    matches!(
        node,
        Node::Quote(_) | Node::List { .. } | Node::Table { .. } | Node::Footnote { .. } | Node::Alert { .. }
    )
}

/// Replaces each node that holds no blocks by what `leaf` makes of it.
fn flat_map(nodes: Vec<Node>, leaf: &mut dyn FnMut(Node) -> Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if !is_container(&node) {
            out.extend(leaf(node));
            continue;
        }
        for body in bodies_mut(&mut node) {
            let taken = std::mem::take(body);
            *body = flat_map(taken, leaf);
        }
        out.push(node);
    }
    out
}

/// Visits every node, containers before what they hold.
pub(super) fn each_mut(nodes: &mut [Node], visit: &mut dyn FnMut(&mut Node)) {
    for node in nodes {
        visit(node);
        for body in bodies_mut(node) {
            each_mut(body, visit);
        }
    }
}

/// Visits every node, containers before what they hold.
pub(super) fn each(nodes: &[Node], visit: &mut dyn FnMut(&Node)) {
    for node in nodes {
        visit(node);
        for body in bodies(node) {
            each(body, visit);
        }
    }
}

// ---- small pieces -----------------------------------------------------------------------

/// A picture's width, or none when it is no width: a picture of no size draws as it is.
pub(super) fn clamp_width(width: f32) -> Option<f32> {
    (width.is_finite() && width > 0.0).then_some(width)
}

/// A paragraph of plain `text`.
pub(super) fn plain_paragraph(text: &str) -> Node {
    Node::Paragraph(Inline {
        text: text.to_owned(),
        ..Inline::default()
    })
}

/// A paragraph that is all one link.
fn link_paragraph(text: &str, url: &str) -> Node {
    Node::Paragraph(Inline {
        text: text.to_owned(),
        styles: vec![(0..text.len(), Style::Link)],
        links: vec![(0..text.len(), url.to_owned())],
        ..Inline::default()
    })
}

fn is_web_image(url: &str) -> bool {
    let scheme = |name: &str| {
        url.get(..name.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(name))
    };
    scheme("http://") || scheme("https://")
}

fn is_data_image(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|head| head.eq_ignore_ascii_case("data:"))
}

// ---- block HTML -------------------------------------------------------------------------

/// The blocks that replace one HTML block: what `expand_html` maps it to, built no deeper
/// than `room` levels (past that its tags keep their text and make no level). It always
/// answers, so Ely's own reading of the block as text never shows; an HTML block of
/// nothing a reader sees gives no blocks.
pub(super) fn html_block(raw: &str, room: usize) -> Option<Vec<Node>> {
    let document = Document {
        blocks: vec![Block::Html {
            text: raw.to_owned(),
        }],
    };
    Some(blocks(expand_html_capped(document, room).blocks))
}

fn blocks(blocks: Vec<Block>) -> Vec<Node> {
    blocks.into_iter().flat_map(block).collect()
}

fn block(block: Block) -> Vec<Node> {
    match block {
        Block::Heading { level, inline } => {
            let mut run = Run::new(true);
            run.leaves(&inline);
            vec![Node::Heading(level, run.finish_inline())]
        }
        Block::Paragraph { inline } => paragraph(&inline),
        Block::List { kind, items, .. } => vec![Node::List {
            start: match kind {
                ListKind::Ordered { start } => Some(start),
                ListKind::Bullet => None,
            },
            items: items
                .into_iter()
                .map(|item| (item.checked, blocks(item.blocks)))
                .collect(),
        }],
        Block::BlockQuote { blocks: inner } => vec![Node::Quote(blocks(inner))],
        Block::CodeBlock { language, text, .. } => vec![Node::Code {
            language,
            text: text.trim_end_matches('\n').to_owned(),
            from: 0,
        }],
        Block::Table {
            alignment,
            header,
            rows,
        } => {
            let cell = |cell: TableCell| paragraph(&cell.inline);
            vec![Node::Table {
                aligns: alignment
                    .into_iter()
                    .map(|alignment| match alignment {
                        Alignment::None => AlignKind::None,
                        Alignment::Left => AlignKind::Left,
                        Alignment::Center => AlignKind::Center,
                        Alignment::Right => AlignKind::Right,
                    })
                    .collect(),
                head: header.into_iter().map(cell).collect(),
                rows: rows
                    .into_iter()
                    .map(|row| row.into_iter().map(cell).collect())
                    .collect(),
            }]
        }
        Block::ThematicBreak => vec![Node::Rule],
        // `expand_html` leaves no raw HTML; what it could not read stays as text.
        Block::Html { text } => vec![plain_paragraph(&text)],
    }
}

/// Inline content as blocks: text, split at each picture, as Ely's reader splits it.
fn paragraph(leaves: &[Leaf]) -> Vec<Node> {
    let mut run = Run::new(false);
    run.leaves(leaves);
    run.finish()
}

/// A span still open while its content is written.
struct Open {
    style: Style,
    link: Option<String>,
    start: usize,
}

/// Inline content on its way to blocks.
struct Run {
    out: Vec<Node>,
    text: Inline,
    open: Vec<Open>,
    /// A picture is its alt text here (a heading holds no picture).
    pictures_as_text: bool,
}

impl Run {
    fn new(pictures_as_text: bool) -> Self {
        Self {
            out: Vec::new(),
            text: Inline::default(),
            open: Vec::new(),
            pictures_as_text,
        }
    }

    fn leaves(&mut self, leaves: &[Leaf]) {
        for leaf in leaves {
            self.leaf(leaf);
        }
    }

    fn leaf(&mut self, leaf: &Leaf) {
        match leaf {
            Leaf::Text(text) => self.text.text.push_str(text),
            Leaf::Code(code) => {
                let start = self.text.text.len();
                self.text.text.push_str(code);
                let end = self.text.text.len();
                self.text.styles.push((start..end, Style::Code));
            }
            Leaf::Emphasis(children) => self.span(Style::Emphasis, None, children),
            Leaf::Strong(children) => self.span(Style::Strong, None, children),
            Leaf::Link {
                target, children, ..
            } => self.span(Style::Link, Some(target.clone()), children),
            Leaf::Image {
                target, alt, width, ..
            } => match self.pictures_as_text {
                true => self.text.text.push_str(alt),
                false => {
                    self.cut();
                    let link = self.open.iter().rev().find_map(|open| open.link.clone());
                    self.out.push(Node::Image {
                        alt: alt.clone(),
                        url: target.clone(),
                        link,
                        at: 0,
                        width: width.and_then(|width| clamp_width(width as f32)),
                    });
                }
            },
            Leaf::SoftBreak | Leaf::HardBreak => self.text.text.push('\n'),
            // `expand_html` folded the tags it knows; the rest show nothing.
            Leaf::Html(_) => {}
        }
    }

    fn span(&mut self, style: Style, link: Option<String>, children: &[Leaf]) {
        let start = self.text.text.len();
        self.open.push(Open { style, link, start });
        self.leaves(children);
        let open = self.open.pop().expect("a span opened here");
        Self::close(&mut self.text, open);
    }

    /// Writes `open`'s span over what was added since it started.
    fn close(text: &mut Inline, open: Open) {
        let end = text.text.len();
        if open.start >= end {
            return;
        }
        text.styles.push((open.start..end, open.style));
        if let Some(link) = open.link {
            text.links.push((open.start..end, link));
        }
    }

    /// Ends the text before a picture; the spans around the picture go on after it.
    fn cut(&mut self) {
        let mut text = std::mem::take(&mut self.text);
        for open in &mut self.open {
            let held = Open {
                style: open.style,
                link: open.link.clone(),
                start: open.start,
            };
            Self::close(&mut text, held);
            open.start = 0;
        }
        if !text.text.trim().is_empty() {
            self.out.push(Node::Paragraph(text));
        }
    }

    fn finish_inline(self) -> Inline {
        self.text
    }

    fn finish(mut self) -> Vec<Node> {
        self.cut();
        self.out
    }
}

// ---- diagrams ---------------------------------------------------------------------------

/// Diagram fences as what the Preview has for them: the picture, the fence and a note,
/// or the fence alone (and a request, when nobody has asked for the diagram yet).
pub(super) fn diagrams(
    nodes: Vec<Node>,
    inputs: &DiagramInputs<'_>,
    missing: &mut Vec<DiagramRequest>,
) -> Vec<Node> {
    flat_map(nodes, &mut |node| {
        let Node::Code {
            language: Some(language),
            text,
            from,
        } = &node
        else {
            return vec![node];
        };
        let Some(kind) = DiagramKind::from_fence(language) else {
            return vec![node];
        };
        let source = text.trim_end_matches('\n').to_owned();
        let key = sirio_diagram::cache_key(kind, &source, inputs.palette);
        match inputs.states.get(&key) {
            Some(DiagramState::Ready { path, width }) => vec![Node::Image {
                alt: "Diagram".to_owned(),
                url: address_of(&path.to_string_lossy()),
                link: None,
                at: *from,
                width: clamp_width(*width as f32),
            }],
            // The quote is the muted block the tree has: the note reads as an aside.
            Some(DiagramState::Failed(note)) => {
                vec![node, Node::Quote(vec![plain_paragraph(note)])]
            }
            Some(DiagramState::Pending) => vec![node],
            None => {
                // The same diagram twice is rendered once: both fences show as code.
                if !missing.iter().any(|request| request.key == key) {
                    missing.push(DiagramRequest { key, kind, source });
                }
                vec![node]
            }
        }
    })
}

// ---- pictures ---------------------------------------------------------------------------

/// Each picture as `policy.surface` lets it be drawn: at an address the renderer may load,
/// or as its alt text. Nothing but an absolute path, a `data:` or `http(s)` address, or
/// no picture reaches the renderer, which is never given a folder to resolve against.
pub(super) fn pictures(nodes: Vec<Node>, policy: &mut Policy<'_>) -> Vec<Node> {
    let surface = policy.surface;
    let base = policy.base;
    let mut files = policy.file_links.as_mut();
    flat_map(nodes, &mut |node| {
        let Node::Image {
            alt,
            url,
            link,
            at,
            width,
        } = node
        else {
            return vec![node];
        };
        let kept = match surface {
            Surface::ChangeRequest => None,
            Surface::Preview => preview_address(&url, base),
            Surface::Chat => {
                if is_web_image(&url) || is_data_image(&url) {
                    Some(url.clone())
                } else {
                    files
                        .as_mut()
                        .and_then(|f| file_links::local_image_path(&url, f.worktree, f.home, f.memo))
                }
            }
        };
        match kept {
            Some(url) => vec![Node::Image {
                alt,
                url,
                link,
                at,
                width,
            }],
            None => vec![picture_as_text(surface, &alt, &url, link.as_deref())],
        }
    })
}

/// Whether a picture address, as written or once decoded, names a network share or a
/// device (see [`file_links::is_network_or_device_path`]), or is rooted with one backslash
/// (a drive-relative path on Windows, no folder's file anywhere). Opening one would
/// connect out (NTLM on Windows) or hang. No verbatim spelling is exempt here: an address
/// written in a document is judged strictly. Only the Preview's own folder may be one (see
/// [`without_verbatim_prefix`]).
pub(super) fn names_a_network_path(address: &str) -> bool {
    let rooted = |text: &str| {
        text.starts_with('\\') || file_links::is_network_or_device_path(std::path::Path::new(text))
    };
    rooted(address) || rooted(&crate::markdown_preview::percent_decode(address))
}

/// `folder` as the ordinary disk path `C:\..` when it is spelled as Windows canonicalises one,
/// `\\?\C:\..`, and the plain spelling names the same place (see [`simplified_disk_path`]); any
/// other folder as it is, so a picture under it does not load, and is never resolved wrong. A
/// worktree path read back from git can be spelled so, and it is the Preview's own folder, not
/// text from a document. Ely cuts every picture address at its first `?`, so the renderer must
/// never be handed a verbatim path; std turns a long path back into one inside its file-system
/// calls. Written on the text, so it runs on any platform.
pub(super) fn without_verbatim_prefix(folder: &std::path::Path) -> std::borrow::Cow<'_, std::path::Path> {
    use std::borrow::Cow;
    // A folder that is not UTF-8 stays as it is: a lossy copy would name another place.
    match folder.to_str().and_then(simplified_disk_path) {
        Some(plain) => Cow::Owned(std::path::PathBuf::from(plain)),
        None => Cow::Borrowed(folder),
    }
}

/// The rule of `dunce::simplified`, as string logic (the crate does nothing off Windows, which
/// would leave it out of reach of the tests on Linux): `\\?\C:\..` (backslashes only, one ASCII
/// drive letter, then a backslash or the end) is `C:\..` when that is representable: the whole
/// verbatim path is at most 260 UTF-16 units; every component is a plain name (at most 255
/// units, no control character or any of `< > : " / | ? *`, no `.` or `..` as Windows reads a
/// verbatim path literally, and no trailing dot or space); and none is a reserved DOS name
/// (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`, with or without an extension,
/// in any case). `\\?\UNC\server\share` is not a disk path: it is left as it is, as the network
/// path it names, and is refused, where `\\server\share` would be rooted wrongly as `\server`.
fn simplified_disk_path(verbatim: &str) -> Option<String> {
    let bytes = verbatim.as_bytes();
    let disk = bytes.starts_with(br"\\?\")
        && bytes.get(4).is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(5) == Some(&b':')
        && matches!(bytes.get(6), None | Some(b'\\'));
    if !disk || verbatim.encode_utf16().count() > 260 {
        return None;
    }
    if !verbatim[6..].split('\\').filter(|part| !part.is_empty()).all(plain_name) {
        return None;
    }
    // `\\?\C:` is the disk's root, `C:` alone the folder the process has open on it.
    let plain = &verbatim[4..];
    Some(if bytes.len() == 6 { format!("{plain}\\") } else { plain.to_owned() })
}

/// Whether `name`, a component of a verbatim path, is a name Windows reads the same plain.
fn plain_name(name: &str) -> bool {
    !(name.encode_utf16().count() > 255
        || name
            .bytes()
            .any(|c| matches!(c, 0..=31 | b'<' | b'>' | b':' | b'"' | b'/' | b'|' | b'?' | b'*'))
        || name.ends_with(['.', ' '])
        || is_dos_device_name(name))
}

/// Whether `name` is a reserved DOS device name: `CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9` or
/// `LPT1` to `LPT9` (also with the superscripts `COM¹²³` and `LPT¹²³`), in any case. Windows reads
/// the device whatever the extension, taking the stem before the FIRST dot (`NUL.tar.gz` is
/// `NUL`) and ignoring the spaces that end it. A string rule, so it holds on every platform.
fn is_dos_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().trim_end_matches(' ');
    if ["CON", "PRN", "AUX", "NUL"].iter().any(|device| stem.eq_ignore_ascii_case(device)) {
        return true;
    }
    ["COM", "LPT"].iter().any(|device| {
        stem.get(..3).is_some_and(|head| head.eq_ignore_ascii_case(device))
            && matches!(
                &stem[3..],
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}"
            )
    })
}

/// Whether the last component of `path`, a path as the renderer would open it, is a DOS device
/// name (see [`is_dos_device_name`]). Either separator, as the path may be a Windows one.
fn ends_in_a_dos_device(path: &str) -> bool {
    path.rsplit(['/', '\\']).find(|part| !part.is_empty()).is_some_and(is_dos_device_name)
}

/// `path`, a local path, as an address for the renderer, which reads an address as a URL: it
/// cuts it at the first `?` or `#` and decodes it once. So those two and the `%` are encoded,
/// and nothing else, and what it reads back is exactly `path`.
fn address_of(path: &str) -> String {
    let mut address = String::with_capacity(path.len());
    for ch in path.chars() {
        match ch {
            '%' => address.push_str("%25"),
            '?' => address.push_str("%3F"),
            '#' => address.push_str("%23"),
            ch => address.push(ch),
        }
    }
    address
}

/// Where the Preview loads `url` from, or none: an absolute path, a `data:` or `http(s)`
/// address. A `file:` address becomes the path it names, so the renderer is handed no
/// address that points at a file. A network path, or a path that ends in a DOS device name,
/// is never one.
fn preview_address(url: &str, base: Option<&std::path::Path>) -> Option<String> {
    if url.is_empty() || (!is_web_image(url) && !is_data_image(url) && names_a_network_path(url)) {
        return None;
    }
    if is_web_image(url) || is_data_image(url) {
        return Some(url.to_owned());
    }
    // The renderer reads an address by cutting it at its first `?`: never a verbatim one.
    let base = base.map(without_verbatim_prefix);
    let base = base.as_deref();
    let scheme = url.split_once(':').map(|(scheme, _)| scheme);
    // `found` is a path, decoded as the renderer would decode the address; the checks below are
    // on it, and what is handed over is that path as an address again.
    let found = if std::path::Path::new(url).is_absolute() {
        let written = url.split(['?', '#']).next().unwrap_or_default();
        Some(crate::markdown_preview::percent_decode(written))
    } else {
        match scheme {
            Some(scheme) if scheme.eq_ignore_ascii_case("file") => file_address_path(&url[5..]),
            // Another scheme is no folder's file and no address the renderer loads.
            Some(scheme)
                if !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) =>
            {
                None
            }
            _ => base.map(|base| crate::markdown_preview::resolve_image(url, base)),
        }
    };
    found
        .filter(|path| !names_a_network_path(path) && !ends_in_a_dos_device(path))
        .map(|path| address_of(&path))
}

/// The local path a `file:` address names, after its scheme: no host or `localhost`, no
/// query or fragment, `%XX` decoded. A file on another host is not read.
fn file_address_path(rest: &str) -> Option<String> {
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, written) = match rest.strip_prefix("//") {
        Some(after) => after.split_at(after.find('/').unwrap_or(after.len())),
        None => ("", rest),
    };
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    let path = crate::markdown_preview::percent_decode(written);
    // Windows writes file:///C:/…
    let path = match path.as_bytes() {
        [b'/', _, b':', ..] if cfg!(windows) => path[1..].to_owned(),
        _ => path,
    };
    std::path::Path::new(&path).is_absolute().then_some(path)
}

/// A picture that does not draw: its alt text. A change request keeps a way to the web
/// address, and only to that.
fn picture_as_text(surface: Surface, alt: &str, url: &str, link: Option<&str>) -> Node {
    let label = match alt.trim().is_empty() {
        true => "image",
        false => alt,
    };
    let target = match surface {
        Surface::ChangeRequest => link
            .filter(|link| is_web_image(link))
            .or(is_web_image(url).then_some(url)),
        _ => link,
    };
    match target {
        Some(target) => link_paragraph(label, target),
        None => plain_paragraph(label),
    }
}

// ---- links ------------------------------------------------------------------------------

/// The chat's allowlist on every link: the text links and the picture links.
pub(super) fn chat_links(nodes: &mut [Node], files: &mut Option<FileLinkInputs<'_>>) {
    each_mut(nodes, &mut |node| match node {
        Node::Paragraph(inline) | Node::Heading(_, inline) => {
            file_links::link_inline(inline, files)
        }
        Node::Image { link, .. } => {
            *link = link
                .take()
                .and_then(|target| file_links::allowed_link(&target, files));
        }
        _ => {}
    });
}

// ---- tables of contents -----------------------------------------------------------------

/// Each `[toc]` and each lone `[[toc]]` as a list of links to the headings, by depth.
/// Returns how many were turned.
pub(super) fn tocs(nodes: Vec<Node>, headings: &[Heading]) -> (Vec<Node>, usize) {
    let mut turned = 0;
    let nodes = flat_map(nodes, &mut |node| match node {
        Node::Toc(_) => {
            turned += 1;
            toc_list(headings).into_iter().collect()
        }
        other => vec![other],
    });
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match &node {
            Node::Paragraph(inline) if inline.text.trim().eq_ignore_ascii_case("[[toc]]") => {
                turned += 1;
                out.extend(toc_list(headings));
            }
            _ => out.push(node),
        }
    }
    (out, turned)
}

/// The headings as nested lists of links, a level for each step deeper.
fn toc_list(headings: &[Heading]) -> Option<Node> {
    let top = headings.iter().map(|heading| heading.depth).min()?;
    let mut at = 0;
    Some(Node::List {
        start: None,
        items: toc_level(headings, &mut at, top),
    })
}

fn toc_level(
    headings: &[Heading],
    at: &mut usize,
    here: usize,
) -> Vec<(Option<bool>, Vec<Node>)> {
    let mut items: Vec<(Option<bool>, Vec<Node>)> = Vec::new();
    while let Some(heading) = headings.get(*at) {
        if heading.depth == here || (heading.depth > here && items.is_empty()) {
            let link = format!("#{}", heading.slug);
            items.push((None, vec![link_paragraph(&heading.title, &link)]));
            *at += 1;
        } else if heading.depth > here && !items.is_empty() {
            // Each step deeper opens a list, and a list takes at least one heading.
            let nested = Node::List {
                start: None,
                items: toc_level(headings, at, heading.depth),
            };
            items.last_mut().expect("an item to nest under").1.push(nested);
        } else {
            break;
        }
    }
    items
}

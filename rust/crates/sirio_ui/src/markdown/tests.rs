//! The unit's contract: what each surface lets through, what a hostile source
//! cannot do, and what the plain text and the anchors are made of.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ely_gpui_component::documents::{AlertKind, Inline, MarkdownBlock as Node, Style};
use sirio_diagram::{DiagramKind, Palette};
use sirio_project::FileLinkTarget;

use super::*;
use crate::chat::file_links::{FileLinkMemo, file_link_target, file_link_url};
use crate::markdown_preview::DiagramState;

// ---- fixtures ---------------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sirio-markdown-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A worktree inside a scratch root (so `outside.txt` sits in its parent) and a
/// separate directory playing `home`.
struct Fixture {
    _root: TempDir,
    _home: TempDir,
    wt: PathBuf,
    home: PathBuf,
}

fn fixture() -> Fixture {
    let root = TempDir::new();
    let home_dir = TempDir::new();
    let wt = root.0.join("wt");
    std::fs::create_dir_all(wt.join("docs/sub")).unwrap();
    std::fs::create_dir_all(wt.join("src")).unwrap();
    std::fs::create_dir_all(wt.join("dir with space")).unwrap();
    std::fs::write(wt.join("docs/NOME-FILE.md"), "# nome\n").unwrap();
    let lines: String = (1..=60).map(|n| format!("line {n}\n")).collect();
    std::fs::write(wt.join("src/main.rs"), lines).unwrap();
    std::fs::write(wt.join("README.md"), "readme\n").unwrap();
    std::fs::write(wt.join("dir with space/a b.txt"), "x\n").unwrap();
    std::fs::write(root.0.join("outside.txt"), "x\n").unwrap();
    std::fs::write(home_dir.0.join("notes.txt"), "x\n").unwrap();
    let home = home_dir.0.clone();
    Fixture {
        _root: root,
        _home: home_dir,
        wt,
        home,
    }
}

fn palette() -> Palette {
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

fn plain(surface: Surface) -> Policy<'static> {
    Policy {
        surface,
        base: None,
        diagrams: None,
        file_links: None,
    }
}

fn chat<'a>(fx: &'a Fixture, memo: &'a mut FileLinkMemo) -> Policy<'a> {
    chat_with_home(fx, Some(&fx.home), memo)
}

fn chat_with_home<'a>(
    fx: &'a Fixture,
    home: Option<&'a Path>,
    memo: &'a mut FileLinkMemo,
) -> Policy<'a> {
    Policy {
        surface: Surface::Chat,
        base: Some(&fx.wt),
        diagrams: None,
        file_links: Some(FileLinkInputs {
            worktree: &fx.wt,
            home,
            memo,
        }),
    }
}

fn preview_in(base: &Path) -> Policy<'_> {
    Policy {
        surface: Surface::Preview,
        base: Some(base),
        diagrams: None,
        file_links: None,
    }
}

// ---- walking a tree, written apart from the code under test --------------------------------

fn bodies(node: &Node) -> Vec<&[Node]> {
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

fn all(nodes: &[Node]) -> Vec<&Node> {
    let mut out = Vec::new();
    for node in nodes {
        out.push(node);
        for body in bodies(node) {
            out.extend(all(body));
        }
    }
    out
}

fn inlines(nodes: &[Node]) -> Vec<&Inline> {
    all(nodes)
        .into_iter()
        .filter_map(|node| match node {
            Node::Paragraph(inline) | Node::Heading(_, inline) => Some(inline),
            _ => None,
        })
        .collect()
}

fn texts(nodes: &[Node]) -> Vec<String> {
    inlines(nodes)
        .into_iter()
        .map(|inline| inline.text.clone())
        .collect()
}

fn images(nodes: &[Node]) -> Vec<&Node> {
    all(nodes)
        .into_iter()
        .filter(|node| matches!(node, Node::Image { .. }))
        .collect()
}

fn paragraph(text: &str) -> Node {
    Node::Paragraph(Inline {
        text: text.to_owned(),
        ..Inline::default()
    })
}

/// The only paragraph with this text.
fn inline_of<'a>(nodes: &'a [Node], text: &str) -> &'a Inline {
    let found: Vec<_> = inlines(nodes)
        .into_iter()
        .filter(|inline| inline.text == text)
        .collect();
    assert_eq!(found.len(), 1, "{text:?} in {:?}", texts(nodes));
    found[0]
}

fn on_stack<T: Send + 'static>(bytes: usize, work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("no stack overflow, no panic")
}

// ---- nesting ----------------------------------------------------------------------------

/// One plain paragraph of `source`, built where a recursion would overflow.
fn falls_back(source: String) {
    let kept = source.clone();
    let nodes = on_stack(256 * 1024, move || {
        build(&source, &mut plain(Surface::Chat)).nodes.to_vec()
    });
    assert_eq!(nodes.len(), 1, "{:.40}", kept);
    let Node::Paragraph(inline) = &nodes[0] else {
        panic!("not a paragraph: {:?}", nodes[0]);
    };
    assert_eq!(inline.text, kept);
    assert!(inline.styles.is_empty() && inline.links.is_empty());
}

#[test]
fn deep_nesting_falls_back_to_plain_text() {
    const N: usize = 10_000;
    falls_back(format!("{} x", ">".repeat(N)));
    falls_back(format!("{}x", "- ".repeat(N)));
    // Images nest in their alt text, and so do emphasis and strong runs.
    falls_back(format!("{}x{}", "![".repeat(N), "](u)".repeat(N)));
    falls_back(format!("{}x{}", "*a _a ".repeat(N / 2), " a_ a*".repeat(N / 2)));

    // Markdown and HTML depth add up in the tree: each is under the cap, the sum is not
    // (30 quotes, 100 nested tags). The HTML is built to what the Markdown leaves.
    let html = format!("{}x{}", "<blockquote>".repeat(100), "</blockquote>".repeat(100));
    let source = format!("{} {html}", ">".repeat(30));
    let nodes = on_stack(256 * 1024, move || {
        build(&source, &mut plain(Surface::Chat)).nodes.to_vec()
    });
    assert!(depth(&nodes) <= MAX_NESTING, "{}", depth(&nodes));
    assert!(depth(&nodes) > 30, "the quotes around it are kept");

    // The same under about 90 Markdown quotes: the room left is small, and nothing recurses.
    let html = format!("{}<b>{}x{}", "<blockquote>".repeat(N), "<b>".repeat(N), "</b>".repeat(N));
    let source = format!("{} {html}", ">".repeat(90));
    // (Ely's own reading of 90 nested quotes does not fit 256 KiB in a debug build.)
    let nodes = on_stack(4 * 1024 * 1024, move || {
        build(&source, &mut plain(Surface::Chat)).nodes.to_vec()
    });
    assert!(depth(&nodes) <= MAX_NESTING, "{}", depth(&nodes));

    // Block HTML is built to the depth the cap leaves, however its tags cross or hide
    // their closers: it renders, its neighbours are read as ever, and nothing is deeper
    // than the cap.
    let n = N;
    for html in [
        format!("{}x{}", "<blockquote>".repeat(n), "</blockquote>".repeat(n)),
        "<blockquote></b>".repeat(n),
        "<blockquote title=\"</blockquote>\">".repeat(n),
        format!("<div>{}", "<b><blockquote></b>".repeat(n)),
        format!("<div>{}{}", "<b><i></b>".repeat(n), "</i></b>".repeat(n)),
        format!("<div>{}", "<div><blockquote></div>".repeat(n)),
        format!("{}<b>{}x{}", "<blockquote>".repeat(n), "<b>".repeat(n), "</b>".repeat(n)),
        format!("{}<summary>{}x{}", "<blockquote>".repeat(n), "<b>".repeat(n), "</b>".repeat(n)),
    ] {
        let source = format!("before\n\n{html}\n\nafter\n");
        let nodes = on_stack(256 * 1024, move || {
            build(&source, &mut plain(Surface::Chat)).nodes.to_vec()
        });
        assert!(depth(&nodes) <= MAX_NESTING, "{:.40}: {}", html, depth(&nodes));
        let shown = texts(&nodes);
        assert_eq!(shown.first().map(String::as_str), Some("before"), "{:.40}", html);
        assert_eq!(shown.last().map(String::as_str), Some("after"), "{:.40}", html);
    }
}

// Ely parses another text than the one written, and the guard measures the one it parses.

#[test]
fn a_leading_byte_order_mark_does_not_hide_nesting() {
    // pulldown-cmark reads a BOM as text; Ely drops one, so `>` is a quote marker for it.
    falls_back(format!("\u{FEFF}{} x", ">".repeat(10_000)));
    // Ely drops one and no more: the second is text, so the `>` run is a paragraph, and the guard
    // must not drop a second one on its way (it would see 10 000 quotes Ely never reads).
    let source = format!("\u{FEFF}\u{FEFF}{} x", ">".repeat(10_000));
    let nodes = on_stack(256 * 1024, move || {
        build(&source, &mut plain(Surface::Chat)).nodes.to_vec()
    });
    assert_eq!(nodes.len(), 1);
    assert!(matches!(nodes[0], Node::Paragraph(_)), "{:?}", nodes[0]);
    assert_eq!(depth(&nodes), 1);
}

#[test]
fn a_lone_cr_does_not_hide_nesting() {
    // Inside an indented code block a lone CR ends no line for pulldown-cmark, but it does for
    // Ely, which reads it as LF.
    falls_back(format!("    xx\r{} x", ">".repeat(10_000)));
    falls_back(format!("    xx\r{}x", "- ".repeat(10_000)));
    falls_back(format!("    xx\r{}x{}", "*a _a ".repeat(2_500), " a_ a*".repeat(2_500)));
}

#[test]
fn the_pass_that_reads_parenthesised_math_does_not_hide_nesting() {
    // Each `\(x*\)` closes its opener while the pair is still text; once the pair is math, its
    // `*` is hidden and the closers at the end close every opener at once.
    let n = 5_000;
    falls_back(format!("{}{}", "*a \\(x*\\) ".repeat(n), "a* ".repeat(n)));
}

/// How deep `nodes` nest, a leaf counting one.
fn depth(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|node| 1 + bodies(node).into_iter().map(depth).max().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

#[test]
fn nesting_is_cut_at_exactly_the_cap() {
    // Every open tag counts, the paragraph inside the quotes among them.
    let quotes = |n: usize| format!("{} x", ">".repeat(n));
    let (inside, over) = on_stack(8 * 1024 * 1024, move || {
        let cap = MAX_NESTING;
        let inside = build(&quotes(cap - 1), &mut plain(Surface::Chat));
        let over = build(&quotes(cap), &mut plain(Surface::Chat));
        (inside.nodes.to_vec(), over.nodes.to_vec())
    });
    let depth = all(&inside)
        .into_iter()
        .filter(|node| matches!(node, Node::Quote(_)))
        .count();
    assert_eq!(depth, MAX_NESTING - 1, "drawn as written");
    assert_eq!(over, [paragraph(&quotes(MAX_NESTING))], "one past it falls back");
}

// ---- pictures ---------------------------------------------------------------------------

const HOSTILE: &str = "![a](/etc/passwd) ![b](file:///etc/hosts) ![c](rel.png) \
                       ![d](data:image/png;base64,AAAA) ![e](https://x/y.png)";

#[test]
fn untrusted_text_never_names_a_file_to_the_renderer() {
    let source = format!(
        "{HOSTILE}\n\n\
         - {HOSTILE}\n\n\
         > {HOSTILE}\n\n\
         | h |\n|---|\n| {HOSTILE} |\n\n\
         > [!NOTE]\n> {HOSTILE}\n\n\
         Reference[^1].\n\n\
         [^1]: {HOSTILE}\n\n\
         [![f](/etc/passwd)](https://x)\n"
    );
    let built = build(&source, &mut plain(Surface::ChangeRequest));
    assert!(
        images(&built.nodes).is_empty(),
        "{:?}",
        images(&built.nodes)
    );
    assert_eq!(built.kinds.images, 0);
    let shown = texts(&built.nodes);
    for alt in ["a", "b", "c", "d", "e", "f"] {
        assert!(
            shown.iter().filter(|text| text.as_str() == alt).count() >= 1,
            "{alt}: {shown:?}"
        );
    }
    // A web picture keeps a way to its address; nothing else does.
    let web: Vec<_> = inlines(&built.nodes)
        .into_iter()
        .filter(|inline| inline.text == "e")
        .collect();
    assert_eq!(web.len(), 6, "once per place it was written");
    for inline in web {
        assert_eq!(inline.links, [(0..1, "https://x/y.png".to_owned())]);
    }
    for inline in inlines(&built.nodes) {
        if ["a", "b", "c", "d"].contains(&inline.text.as_str()) {
            assert!(inline.links.is_empty(), "{inline:?}");
        }
    }
    // A picture that was a link keeps the link it sat in.
    assert_eq!(
        inline_of(&built.nodes, "f").links,
        [(0..1, "https://x".to_owned())]
    );
}

#[test]
fn chat_links_keep_only_web_and_verified_files() {
    let fx = fixture();
    let source = "[web](https://a/b) [mail](mailto:x@y.z) [share](smb://host/x) \
                  [vs](vscode://x) [anchor](#top) [gone](nope.md) [file](docs/NOME-FILE.md)\n\n\
                  [![x](https://a/b.png)](smb://host/x)\n\n\
                  [![y](https://a/b.png)](https://ok/)\n";
    let mut memo = FileLinkMemo::default();
    let built = build(source, &mut chat(&fx, &mut memo));
    let line = inlines(&built.nodes)
        .into_iter()
        .find(|inline| inline.text.starts_with("web"))
        .expect("the line of links");
    let kept: Vec<_> = line
        .links
        .iter()
        .map(|(range, url)| (&line.text[range.clone()], url.as_str()))
        .collect();
    let file = file_link_url(&FileLinkTarget {
        path: fx.wt.join("docs/NOME-FILE.md"),
        line: None,
        column: None,
    });
    assert_eq!(
        kept,
        [
            ("web", "https://a/b"),
            ("mail", "mailto:x@y.z"),
            ("file", file.as_str())
        ]
    );
    // A link that goes loses its look with it; its text stays.
    let looks: Vec<_> = line
        .styles
        .iter()
        .filter(|(_, style)| *style == Style::Link)
        .map(|(range, _)| &line.text[range.clone()])
        .collect();
    assert_eq!(looks, ["web", "mail", "file"]);
    for gone in ["share", "vs", "anchor", "gone"] {
        assert!(line.text.contains(gone), "{gone}");
    }

    let pictures: Vec<_> = images(&built.nodes)
        .into_iter()
        .map(|node| match node {
            Node::Image { url, link, .. } => (url.as_str(), link.as_deref()),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        pictures,
        [
            ("https://a/b.png", None),
            ("https://a/b.png", Some("https://ok/"))
        ]
    );
}

#[test]
fn chat_keeps_only_small_regular_local_images() {
    let fx = fixture();
    std::fs::write(fx.wt.join("small.png"), vec![1u8; 1024]).unwrap();
    std::fs::create_dir_all(fx.wt.join("adir")).unwrap();
    let big = std::fs::File::create(fx.wt.join("big.png")).unwrap();
    big.set_len(11 * 1024 * 1024).unwrap();
    let edge = std::fs::File::create(fx.wt.join("edge.png")).unwrap();
    edge.set_len(crate::chat::MAX_IMAGE_BYTES).unwrap();
    let absolute = fx.wt.join("small.png");
    let source = format!(
        "![n1](/\\host/s/x.png)\n\n![n2](\\/host/s/x.png)\n\n![n3](//host/s/x.png)\n\n\
         ![abs]({})\n\n![rel](small.png)\n\n![dir](adir)\n\n![missing](none.png)\n\n\
         ![big](big.png)\n\n![edge](edge.png)\n\n![web](https://x/y.png)\n",
        absolute.display()
    );
    let mut memo = FileLinkMemo::default();
    let built = build(&source, &mut chat(&fx, &mut memo));
    let kept: Vec<_> = images(&built.nodes)
        .into_iter()
        .map(|node| match node {
            Node::Image { alt, url, .. } => (alt.as_str(), url.clone()),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        kept,
        [
            ("abs", absolute.to_string_lossy().into_owned()),
            ("rel", absolute.to_string_lossy().into_owned()),
            ("edge", fx.wt.join("edge.png").to_string_lossy().into_owned()),
            ("web", "https://x/y.png".to_owned()),
        ]
    );
    let shown = texts(&built.nodes);
    for alt in ["dir", "missing", "big", "n1", "n2", "n3"] {
        assert!(shown.contains(&alt.to_owned()), "{alt}: {shown:?}");
    }
}

#[test]
fn preview_never_names_a_network_path() {
    let base = Path::new("/repo/docs");
    let source = "![a](//host/s/x.png)\n\n![b](\\\\host\\s\\x.png)\n\n\
                  ![c](%5C%5Chost%5Cs%5Cx.png)\n\n![d](file://host/s/x.png)\n\n\
                  ![e](file:////host/s/x.png)\n\n![f](%2F%2Fhost%2Fx.png)\n\n\
                  <img src=\"\\\\host\\s\\x.png\">\n\n![g](\\\\?\\C:\\x.png)\n\n\
                  ![h](/\\host/s/x.png)\n\n<img src=\"\\/host/s/x.png\" alt=\"i\">\n\n![ok](fine.png)\n";
    let built = build(source, &mut preview_in(base));
    let urls: Vec<_> = images(&built.nodes)
        .into_iter()
        .map(|node| match node {
            Node::Image { url, .. } => url.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(urls, [base.join("fine.png").to_string_lossy().into_owned()]);
    let shown = texts(&built.nodes);
    for alt in ["a", "b", "c", "d", "e", "f", "g", "h", "i"] {
        assert!(shown.contains(&alt.to_owned()), "{alt}: {shown:?}");
    }
}

#[test]
fn preview_resolves_relative_images_against_the_file_folder() {
    let base = Path::new("/repo/docs");
    let source = "![a](pixel.png)\n\n![b](my%20pic.png)\n\n![c](/abs/x.png)\n\n![d](https://x/y.png)\n\n\
                  ![e](data:image/png;base64,AAAA)\n\n![f](file:///abs/x%20y.png)\n\n\
                  ![g](file://server/share/a.png)\n\n![h](//host/a.png)\n\n![i](ftp://h/a.png)\n";
    let built = build(source, &mut preview_in(base));
    let urls: Vec<_> = images(&built.nodes)
        .into_iter()
        .map(|node| match node {
            Node::Image { url, .. } => url.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        urls,
        [
            base.join("pixel.png").to_string_lossy().into_owned(),
            base.join("my pic.png").to_string_lossy().into_owned(),
            "/abs/x.png".to_owned(),
            "https://x/y.png".to_owned(),
            "data:image/png;base64,AAAA".to_owned(),
            // The renderer is handed the path a file address names, never the address; a
            // file on another host, a bare host and another scheme are no pictures.
            "/abs/x y.png".to_owned(),
        ]
    );
    let shown = texts(&built.nodes);
    for alt in ["g", "h", "i"] {
        assert!(shown.contains(&alt.to_owned()), "{alt}: {shown:?}");
    }
}

// ---- html -------------------------------------------------------------------------------

#[test]
fn html_blocks_expand_through_expand_html() {
    let base = Path::new("/repo/docs");
    let built = build(
        "<p align=\"center\"><img src=\"a.png\" width=\"120\"></p>\n",
        &mut preview_in(base),
    );
    let nodes = built.nodes.to_vec();
    let [Node::Image { url, link, width, .. }] = nodes.as_slice() else {
        panic!("one picture: {nodes:?}");
    };
    assert_eq!(url, &base.join("a.png").to_string_lossy());
    assert_eq!((link, width), (&None, &Some(120.0)));

    // A width that is no width draws at the picture's own size.
    let built = build("<img src=\"a.png\" width=\"0\">\n", &mut preview_in(base));
    assert!(matches!(
        built.nodes.to_vec().as_slice(),
        [Node::Image { width: None, .. }]
    ));

    let built = build(
        "<details><summary>S</summary>x</details>\n",
        &mut plain(Surface::Chat),
    );
    assert_eq!(texts(&built.nodes), ["S", "x"]);
    assert_eq!(inline_of(&built.nodes, "S").styles, [(0..1, Style::Strong)]);

    // What a browser would not show is gone, and so is its place.
    let built = build(
        "before\n\n<script>evil()</script>\n\nafter\n",
        &mut plain(Surface::Chat),
    );
    assert_eq!(texts(&built.nodes), ["before", "after"]);
    assert_eq!(built.nodes.len(), 2, "nothing stands where the script was");

    // At any depth.
    let built = build("- <div align=\"center\">in a list</div>\n", &mut plain(Surface::Chat));
    assert_eq!(texts(&built.nodes), ["in a list"]);
}

// ---- diagrams ---------------------------------------------------------------------------

const FENCE: &str = "```mermaid\nflowchart TD\n  A --> B\n```\n";

fn key_of(source: &str) -> String {
    sirio_diagram::cache_key(DiagramKind::Mermaid, source, &palette())
}

fn with_state(state: Option<DiagramState>) -> Diagrams {
    let mut diagrams = Diagrams::default();
    if let Some(state) = state {
        diagrams.insert(key_of("flowchart TD\n  A --> B"), state);
    }
    diagrams
}

fn diagram_blocks(surface: Surface, diagrams: &Diagrams, source: &str) -> MarkdownBlocks {
    let palette = palette();
    build(
        source,
        &mut Policy {
            surface,
            base: Some(Path::new("/repo/docs")),
            diagrams: Some(DiagramInputs {
                states: diagrams,
                palette: &palette,
                dir: Path::new("/cache"),
            }),
            file_links: None,
        },
    )
}

fn is_mermaid(node: &Node) -> bool {
    matches!(node, Node::Code { language: Some(language), .. } if language == "mermaid")
}

/// A failed diagram's note is a quote beside its fence: one level deeper than the fence.
#[test]
fn a_failed_diagram_note_stays_within_the_cap() {
    let in_quotes = |quotes: usize| {
        let mark = "> ".repeat(quotes);
        ["```mermaid", "flowchart TD", "  A --> B", "```"]
            .iter()
            .map(|line| format!("{mark}{line}\n"))
            .collect::<String>()
    };
    let built = |quotes: usize| {
        let failed = with_state(Some(DiagramState::Failed("no mermaid here".into())));
        let source = in_quotes(quotes);
        on_stack(8 * 1024 * 1024, move || {
            diagram_blocks(Surface::Preview, &failed, &source).nodes.to_vec()
        })
    };
    // The fence is a leaf one level above the cap: its note fits, and is shown.
    let room = built(MAX_NESTING - 2);
    assert!(depth(&room) <= MAX_NESTING, "{}", depth(&room));
    assert!(texts(&room).iter().any(|text| text == "no mermaid here"), "{:?}", texts(&room));
    // At the cap there is no room for it: nothing is deeper than the cap.
    let full = built(MAX_NESTING - 1);
    assert!(depth(&full) <= MAX_NESTING, "{}", depth(&full));
}

#[test]
fn diagram_fences_follow_their_state() {
    let ready = DiagramState::Ready {
        path: PathBuf::from("/cache/abc.svg"),
        width: 300,
    };
    let built = diagram_blocks(Surface::Preview, &with_state(Some(ready.clone())), FENCE);
    let nodes = built.nodes.to_vec();
    let [Node::Image { url, link, width, .. }] = nodes.as_slice() else {
        panic!("one picture: {nodes:?}");
    };
    assert_eq!(url, "/cache/abc.svg");
    assert_eq!((link, width), (&None, &Some(300.0)));
    assert!(built.missing_diagrams.is_empty());

    let failed = with_state(Some(DiagramState::Failed("no mermaid here".into())));
    let built = diagram_blocks(Surface::Preview, &failed, FENCE);
    assert!(is_mermaid(&built.nodes[0]));
    assert_eq!(built.nodes.len(), 2);
    assert!(
        texts(&built.nodes[1..]).iter().any(|text| text == "no mermaid here"),
        "{:?}",
        built.nodes[1]
    );
    assert!(built.missing_diagrams.is_empty());

    let pending = with_state(Some(DiagramState::Pending));
    let built = diagram_blocks(Surface::Preview, &pending, FENCE);
    assert!(built.nodes.len() == 1 && is_mermaid(&built.nodes[0]));
    assert!(built.missing_diagrams.is_empty(), "already scheduled");

    // Nobody has scheduled it: one request, however often it is written.
    let twice = format!("{FENCE}\n{FENCE}");
    let built = diagram_blocks(Surface::Preview, &with_state(None), &twice);
    assert!(built.nodes.len() == 2 && built.nodes.iter().all(is_mermaid));
    assert_eq!(built.missing_diagrams.len(), 1);
    let request = &built.missing_diagrams[0];
    assert_eq!(request.kind, DiagramKind::Mermaid);
    assert_eq!(request.source, "flowchart TD\n  A --> B");
    assert_eq!(request.key, key_of("flowchart TD\n  A --> B"));

    // A diagram of no width draws at its own size.
    let flat = DiagramState::Ready {
        path: PathBuf::from("/cache/abc.svg"),
        width: 0,
    };
    let built = diagram_blocks(Surface::Preview, &with_state(Some(flat)), FENCE);
    assert!(matches!(
        built.nodes.to_vec().as_slice(),
        [Node::Image { width: None, .. }]
    ));

    // Without the pipeline's inputs a fence is code and asks for nothing.
    let built = build(FENCE, &mut preview_in(Path::new("/repo")));
    assert!(built.nodes.len() == 1 && is_mermaid(&built.nodes[0]));
    assert!(built.missing_diagrams.is_empty());

    // Chat and change requests never draw a diagram, whatever is cached.
    for surface in [Surface::Chat, Surface::ChangeRequest] {
        let built = diagram_blocks(surface, &with_state(Some(ready.clone())), FENCE);
        assert!(
            built.nodes.len() == 1 && is_mermaid(&built.nodes[0]),
            "{surface:?}"
        );
        assert!(built.missing_diagrams.is_empty());
        let built = diagram_blocks(surface, &with_state(None), FENCE);
        assert!(built.missing_diagrams.is_empty());
    }

    // In a list item, as everywhere.
    let nested = format!("- item\n\n  {}", FENCE.replace('\n', "\n  "));
    let built = diagram_blocks(Surface::Preview, &with_state(Some(ready)), &nested);
    assert_eq!(images(&built.nodes).len(), 1);
}

// ---- table of contents and anchors -------------------------------------------------------

#[test]
fn toc_blocks_become_heading_links() {
    let source = "[toc]\n\n# Intro\n\n## Setup\n\n### Deep\n\n## Setup\n\n[[toc]]\n\n\
                  > [!NOTE]\n> # In alert\n\ntail [[toc]] text\n";
    let expected = [
        (0, "Intro", "#intro"),
        (1, "Setup", "#setup"),
        (2, "Deep", "#deep"),
        (1, "Setup", "#setup-1"),
        (0, "In alert", "#in-alert"),
    ];
    // The links read the same on every surface; whether they act is the handler's.
    for surface in [Surface::Preview, Surface::Chat, Surface::ChangeRequest] {
        let built = build(source, &mut plain(surface));
        assert!(
            all(&built.nodes)
                .iter()
                .all(|node| !matches!(node, Node::Toc(_))),
            "{surface:?}"
        );
        assert_eq!(built.kinds.toc, 2, "{surface:?}");
        // Both list blocks stand where the markers stood.
        let lists: Vec<_> = built
            .nodes
            .iter()
            .filter(|node| matches!(node, Node::List { .. }))
            .collect();
        assert_eq!(lists.len(), 2, "{surface:?}");
        assert!(matches!(built.nodes[0], Node::List { .. }), "{surface:?}");
        for list in lists {
            let mut seen = Vec::new();
            entries(list, 0, &mut seen);
            let want: Vec<_> = expected
                .iter()
                .map(|(depth, title, link)| (*depth, title.to_string(), link.to_string()))
                .collect();
            assert_eq!(seen, want, "{surface:?}");
        }
        // Only a marker alone is a table of contents.
        assert!(texts(&built.nodes).contains(&"tail [[toc]] text".to_owned()));
        let slugs: Vec<_> = built.headings.iter().map(|h| h.slug.to_string()).collect();
        assert_eq!(slugs, ["intro", "setup", "deep", "setup-1", "in-alert"]);
    }
}

/// Each link of a table-of-contents list: how deep its list sits, its text, its address.
fn entries(list: &Node, depth: usize, out: &mut Vec<(usize, String, String)>) {
    let Node::List { items, .. } = list else {
        panic!("not a list: {list:?}");
    };
    for (_, body) in items {
        for node in body {
            match node {
                Node::Paragraph(inline) => {
                    assert_eq!(inline.links.len(), 1, "{inline:?}");
                    assert_eq!(inline.links[0].0, 0..inline.text.len());
                    out.push((depth, inline.text.clone(), inline.links[0].1.clone()));
                }
                nested @ Node::List { .. } => entries(nested, depth + 1, out),
                other => panic!("unexpected {other:?}"),
            }
        }
    }
}

#[test]
fn slugs_follow_github() {
    let mut seen = HashMap::new();
    assert_eq!(slug("Hello, World!", &mut seen), "hello-world");
    assert_eq!(slug("Hello, World!", &mut seen), "hello-world-1");
    assert_eq!(slug("Hello, World!", &mut seen), "hello-world-2");
    // A title that is already someone's suffixed anchor does not collide with it.
    assert_eq!(slug("Hello World 1", &mut seen), "hello-world-1-1");
    let mut seen = HashMap::new();
    assert_eq!(slug("Ünïcödé Ꙩ", &mut seen), "ünïcödé-ꙩ");
    assert_eq!(slug("a_b-c d", &mut seen), "a_b-c-d");
    assert_eq!(slug("Hi 🎉 there", &mut seen), "hi--there");
    assert_eq!(slug("  Trim me  ", &mut seen), "--trim-me--");
    assert_eq!(slug("100% (sure)", &mut seen), "100-sure");
}

#[test]
fn heading_ordinals_count_alert_bodies() {
    let built = build(
        "> [!NOTE]\n> ### x\n\n## y\n\n> quoted\n>\n> ## z\n",
        &mut plain(Surface::Preview),
    );
    let seen: Vec<_> = built
        .headings
        .iter()
        .map(|h| (h.ordinal, h.slug.as_ref(), h.title.as_ref(), h.depth))
        .collect();
    assert_eq!(seen, [(0, "x", "x", 3), (1, "y", "y", 2), (2, "z", "z", 2)]);
}

// ---- plain text -------------------------------------------------------------------------

const EVERYTHING: &str = "# Title\n\n\
Hello **bold** and [link](https://x.y).\n\n\
> quoted\n\n\
```rust\nlet a = 1;\n```\n\n\
- one\n  - nested\n- [x] done\n- [ ] todo\n\n\
3. three\n4. four\n\n\
| h1 | h2 |\n|----|----|\n| a | b |\n\n\
---\n\n\
$$\nx^2\n$$\n\n\
> [!NOTE]\n> in alert\n\n\
Inline $y$ math.\n\n\
![pic](https://x/p.png)\n\n\
Note[^1].\n\n\
[^1]: the note\n";

#[test]
fn plain_text_parts_follow_the_drawing_order() {
    let built = build(EVERYTHING, &mut plain(Surface::Preview));
    let text = &built.text;
    let parts: Vec<(&str, PartKind)> = text
        .parts
        .iter()
        .map(|part| (&text.text[part.range.clone()], part.kind))
        .collect();
    use PartKind::{Drawn, Silent};
    assert_eq!(
        parts,
        [
            ("Title", Drawn),
            ("Hello bold and link.", Drawn),
            ("quoted", Drawn),
            ("let a = 1;", Drawn),
            ("• one", Drawn),
            ("  ◦ nested", Drawn),
            ("[x] done", Drawn),
            ("[ ] todo", Drawn),
            ("3. three", Drawn),
            ("4. four", Drawn),
            ("h1", Drawn),
            ("h2", Drawn),
            ("a", Drawn),
            ("b", Drawn),
            ("x^2", Silent),
            ("Note", Silent),
            ("in alert", Drawn),
            ("Inline y math.", Silent),
            ("Note¹.", Drawn),
            ("[1]", Silent),
            ("the note", Drawn),
        ]
    );
    // Blocks are a blank line apart, the texts of one block a line end; a rule and a picture
    // are no text and leave no gap.
    assert_eq!(
        text.text,
        "Title\n\nHello bold and link.\n\nquoted\n\nlet a = 1;\n\n\
         • one\n  ◦ nested\n[x] done\n[ ] todo\n\n3. three\n4. four\n\n\
         h1\th2\na\tb\n\nx^2\n\nNote\nin alert\n\nInline y math.\n\nNote¹.\n\n[1]\nthe note"
    );
    // A table copies as a spreadsheet takes it, and each cell is still its own part.
    let table = build("| a | b |\n|---|---|\n| c | d |\n", &mut plain(Surface::Preview));
    assert_eq!(table.text.text, "a\tb\nc\td");
    assert_eq!(table.text.parts.len(), 4);
    let seps: Vec<_> = table.text.parts.iter().map(|part| part.sep.as_str()).collect();
    assert_eq!(seps, ["", "\t", "\n", "\t"]);
    // An empty cell is no text, and keeps its place: every cell position has its separator.
    let copied = |source: &str| build(source, &mut plain(Surface::Preview)).text.text.clone();
    assert_eq!(copied("| h1 | h2 |\n|---|---|\n|  | d |\n"), "h1\th2\n\td");
    assert_eq!(copied("| a |  | c |\n|---|---|---|\n"), "a\t\tc");
    assert_eq!(copied("|  | b |\n|---|---|\n| c | d |\n"), "\tb\nc\td");
    // A list marker leads the text it belongs to, and says how long it is.
    let leads: Vec<_> = text
        .parts
        .iter()
        .filter(|part| part.lead > 0)
        .map(|part| &text.text[part.range.start..part.range.start + part.lead])
        .collect();
    assert_eq!(leads, ["• ", "  ◦ ", "[x] ", "[ ] ", "3. ", "4. "]);
    assert_eq!(built.kinds.math, 2);
    assert_eq!(built.kinds.alerts, 1);
    assert_eq!(built.kinds.footnotes, 1);
    assert_eq!(built.kinds.tables, 1);
    assert_eq!(built.kinds.code, 1);
    assert!(
        matches!(
            built.nodes.iter().find_map(|node| match node {
                Node::Alert { kind, .. } => Some(*kind),
                _ => None,
            }),
            Some(AlertKind::Note)
        ),
        "the alert is read as an alert"
    );
}

// ---- file links in chat -------------------------------------------------------------------

/// Every file link of the tree: the text it covers, and what it opens.
fn file_links_of(nodes: &[Node]) -> Vec<(String, PathBuf, Option<usize>, Option<usize>)> {
    let mut found = Vec::new();
    for inline in inlines(nodes) {
        for (range, url) in &inline.links {
            if let Some(target) = file_link_target(url) {
                found.push((
                    inline.text[range.clone()].to_string(),
                    target.path,
                    target.line,
                    target.column,
                ));
            }
        }
    }
    found
}

#[test]
fn file_paths_in_chat_become_file_links() {
    let fx = fixture();
    std::fs::write(fx.wt.join("Makefile"), "x\n").unwrap();
    let linked = |source: &str| {
        let mut memo = FileLinkMemo::default();
        file_links_of(&build(source, &mut chat(&fx, &mut memo)).nodes)
    };
    let one = |text: &str, path: PathBuf, line, column| vec![(text.to_string(), path, line, column)];
    let main = fx.wt.join("src/main.rs");
    let nome = fx.wt.join("docs/NOME-FILE.md");

    assert_eq!(
        linked("l'ho messo in docs/NOME-FILE.md."),
        one("docs/NOME-FILE.md", nome.clone(), None, None)
    );
    assert_eq!(
        linked("`/docs/NOME-FILE.md`"),
        one("/docs/NOME-FILE.md", nome.clone(), None, None)
    );
    let written = format!("{}/src/main.rs", fx.wt.display());
    assert_eq!(
        linked(&format!("open {written} now")),
        one(&written, main.clone(), None, None)
    );
    assert_eq!(
        linked("see src/main.rs:42"),
        one("src/main.rs:42", main.clone(), Some(42), None)
    );
    assert_eq!(
        linked("see src/main.rs:42:7"),
        one("src/main.rs:42:7", main.clone(), Some(42), Some(7))
    );
    assert_eq!(
        linked("see src/main.rs:42-50"),
        one("src/main.rs:42-50", main.clone(), Some(42), None)
    );
    assert_eq!(
        linked("see src/main.rs#L42"),
        one("src/main.rs#L42", main.clone(), Some(42), None)
    );
    assert_eq!(
        linked("see src/main.rs#L42-L50"),
        one("src/main.rs#L42-L50", main.clone(), Some(42), None)
    );
    for (source, covered) in [
        ("(see ./README.md),", "./README.md"),
        ("\"docs/NOME-FILE.md\"", "docs/NOME-FILE.md"),
        ("docs/NOME-FILE.md:", "docs/NOME-FILE.md"),
    ] {
        let found = linked(source);
        assert_eq!(found.len(), 1, "{source}");
        assert_eq!(found[0].0, covered, "{source}");
    }
    assert_eq!(
        linked("see README.md for details"),
        one("README.md", fx.wt.join("README.md"), None, None)
    );
    assert_eq!(
        linked("`dir with space/a b.txt`"),
        one(
            "dir with space/a b.txt",
            fx.wt.join("dir with space/a b.txt"),
            None,
            None
        )
    );
    assert!(linked("dir with space/a b.txt").is_empty(), "prose has no spaces");
    for source in ["see `Makefile:10` now", "see `Makefile:10:2` now", "see [m](Makefile:10-12) now"] {
        assert_eq!(linked(source).len(), 1, "{source}");
    }
    // A file with an extension followed by a line is still a file and a line.
    std::fs::write(fx.wt.join("main.rs"), "x\n").unwrap();
    assert_eq!(linked("see main.rs:42 now").len(), 1);
    assert_eq!(
        linked("see ~/notes.txt"),
        one("~/notes.txt", fx.home.join("notes.txt"), None, None)
    );
    let mut memo = FileLinkMemo::default();
    let without_home = build("see ~/notes.txt", &mut chat_with_home(&fx, None, &mut memo));
    assert!(file_links_of(&without_home.nodes).is_empty());

    // A Markdown link to a file is retargeted to the file.
    assert_eq!(
        linked("[il file](docs/NOME-FILE.md)"),
        one("il file", nome, None, None)
    );
    // The web's links stay as written, and are no file links.
    let mut memo = FileLinkMemo::default();
    let web = build(
        "[sito](https://example.com/docs/NOME-FILE.md) and https://example.com/README.md",
        &mut chat(&fx, &mut memo),
    );
    assert!(file_links_of(&web.nodes).is_empty());
    let urls: Vec<_> = inlines(&web.nodes)
        .iter()
        .flat_map(|inline| inline.links.iter().map(|(_, url)| url.clone()))
        .collect();
    assert_eq!(
        urls,
        ["https://example.com/docs/NOME-FILE.md", "https://example.com/README.md"]
    );

    // Never linked: what is not there, a directory, what climbs out of the worktree, fenced code.
    for source in [
        "missing/nowhere.md, 1.2.3, 2026/10/24, 3/4",
        "docs/sub/",
        "docs/sub",
        "`docs/sub`",
        "../outside.txt",
        "`../outside.txt`",
        "```text\nsrc/main.rs\n```",
    ] {
        assert!(linked(source).is_empty(), "{source}");
    }

    // Linking marks the text and never changes it.
    let sources = [
        "l'ho messo in docs/NOME-FILE.md.",
        "see src/main.rs:42:7 and src/main.rs#L42-L50",
        "`dir with space/a b.txt` ~/notes.txt",
        "[il file](docs/NOME-FILE.md) and **README.md** and `README.md`",
        "| a | b |\n|---|---|\n| README.md | `src/main.rs` |",
    ];
    for source in sources {
        let before = build(source, &mut plain(Surface::Chat));
        let mut memo = FileLinkMemo::default();
        let after = build(source, &mut chat(&fx, &mut memo));
        assert_eq!(texts(&before.nodes), texts(&after.nodes), "{source}");
        assert_eq!(before.text.text, after.text.text, "{source}");
        for inline in inlines(&after.nodes) {
            for (range, _) in &inline.links {
                assert!(
                    range.end <= inline.text.len()
                        && inline.text.is_char_boundary(range.start)
                        && inline.text.is_char_boundary(range.end),
                    "{source}: {range:?}"
                );
            }
        }
    }

    // One memo answers without the disk until it is cleared.
    let mut memo = FileLinkMemo::default();
    let first = build("see README.md", &mut chat(&fx, &mut memo));
    assert_eq!(file_links_of(&first.nodes).len(), 1);
    std::fs::remove_file(fx.wt.join("README.md")).unwrap();
    let second = build("see README.md", &mut chat(&fx, &mut memo));
    assert_eq!(file_links_of(&second.nodes).len(), 1, "the memo still knows it");
    memo.clear();
    let third = build("see README.md", &mut chat(&fx, &mut memo));
    assert!(file_links_of(&third.nodes).is_empty());
}


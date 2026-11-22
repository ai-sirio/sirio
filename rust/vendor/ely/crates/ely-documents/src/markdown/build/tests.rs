#[cfg(feature = "test-support")]
use super::super::super::clock::spent;
use super::{super::mdast::Node, tree};

fn blocks(root: &Node) -> &[Node] {
    root.children().expect("the root holds blocks")
}

/// Four times the input in at most eight times the time, the best of five rounds of thread time.
#[cfg(feature = "test-support")]
fn linear(source: impl Fn(usize) -> String) {
    let texts = [source(1 << 9), source(1 << 11)];
    let mut best = [std::time::Duration::MAX; 2];
    for _ in 0..5 {
        for (text, best) in texts.iter().zip(&mut best) {
            *best = (*best).min(spent(|| {
                tree(text, false);
            }));
        }
    }
    let [small, large] = best;
    assert!(large < small * 8, "{small:?} then {large:?}");
}

#[test]
fn a_fence_before_wide_text_reads() {
    let root = tree("$$\n中文\n$$", false);
    assert!(
        matches!(blocks(&root), [Node::Math(math)] if math.value == "中文"),
        "{root:?}"
    );
    // Unclosed, GitHub shows it as text.
    let root = tree("$$\n中文", false);
    assert!(matches!(blocks(&root), [Node::Paragraph(_)]), "{root:?}");
}

#[test]
fn dollar_fences_inside_open_code_stay_code() {
    let source = |n: usize| format!("{}\n{}", "~".repeat(n), "$$\nx\n$$\n\n".repeat(n));
    let root = tree(&source(64), false);
    assert!(matches!(blocks(&root), [Node::Code(_)]), "{root:?}");
    #[cfg(feature = "test-support")]
    linear(source);
}

#[cfg(feature = "test-support")]
#[test]
fn closing_parentheses_trail_off_in_linear_time() {
    linear(|n| format!("https://a.b/{}", ")".repeat(n * 16)));
}

#[cfg(feature = "test-support")]
#[test]
fn many_literal_links_read_in_linear_time() {
    linear(|n| "www.a.b &amp; ".repeat(n * 16));
}

#[test]
fn a_definition_in_an_item_stays_in_the_item() {
    let root = tree("- foo\n\n  [r]: /url\n\n[r]\n", false);
    let [Node::List(list), Node::Paragraph(_)] = blocks(&root) else {
        panic!("a list and a paragraph: {root:?}");
    };
    let [Node::ListItem(item)] = &list.children[..] else {
        panic!("one item: {list:?}");
    };
    assert!(
        matches!(
            &item.children[..],
            [Node::Paragraph(_), Node::Definition(_)]
        ),
        "{item:?}"
    );
}

/// Whether each list under `node` holds items and nothing else.
fn lists_hold_items(node: &Node) -> bool {
    let kids = || node.children().into_iter().flatten();
    let list = matches!(node, Node::List(_));
    (!list || kids().all(|kid| matches!(kid, Node::ListItem(_)))) && kids().all(lists_hold_items)
}

/// Blocks as a line, definitions included: `list[item[p def]] def p`.
fn kinds(nodes: &[Node]) -> String {
    let lines: Vec<String> = nodes
        .iter()
        .map(|node| match node {
            Node::Paragraph(_) => "p".into(),
            Node::Definition(_) => "def".into(),
            Node::Blockquote(quote) => format!("quote[{}]", kinds(&quote.children)),
            Node::List(list) => format!("list[{}]", kinds(&list.children)),
            Node::ListItem(item) => format!("item[{}]", kinds(&item.children)),
            other => panic!("not read here: {other:?}"),
        })
        .collect();
    lines.join(" ")
}

#[test]
fn a_definition_after_a_list_sits_beside_it() {
    // pulldown-cmark ends a list at the first line of the paragraph a definition opens, so the
    // list's span covers the definition, which no item holds. It belongs beside the list; one
    // an item holds stays in the item, at any depth.
    for (source, kind) in [
        ("- a\n\n[a]: u\nx", "list[item[p]] def p"),
        ("- \n[a]: u\nx", "list[item[]] def p"),
        ("1. a\n2. b\n\n[a]: u\nx", "list[item[p] item[p]] def p"),
        ("- a\n\n[a]: u\n[b]: v", "list[item[p]] def def"),
        ("- a\n  - b\n\n[a]: u\nx", "list[item[p list[item[p]]]] def p"),
        ("- a\n  - b\n\n  [a]: u\n  x", "list[item[p list[item[p]] def p]]"),
        ("> - a\n>\n> [a]: u\n> x", "quote[list[item[p]] def p]"),
        ("- a\n\n  [a]: u\n\n- b\n\n[b]: v\nx", "list[item[p def] item[p]] def p"),
    ] {
        let root = tree(source, false);
        assert!(lists_hold_items(&root), "{source:?}: {root:?}");
        assert_eq!(kinds(blocks(&root)), kind, "{source:?}");
    }
    let root = tree("- a\n\n[a]: u\nx", false);
    let [Node::List(list), Node::Definition(definition), Node::Paragraph(_)] = blocks(&root) else {
        panic!("a list, its definition and a paragraph: {root:?}");
    };
    assert!(list.position.end <= definition.position.start, "{root:?}");
}

#[test]
fn a_definition_that_does_not_touch_a_list_stays_where_it_was() {
    // None of these reached the list's children: the container's span ended before the definition,
    // or the item held it.
    for (source, kind) in [
        ("- a\n\n[a]: u\n\nx", "list[item[p]] def p"),
        ("- a\n\n[a]: u", "list[item[p]] def"),
        ("- a\n\n  [r]: u\n\nx", "list[item[p def]] p"),
        ("- a\n  - b\n\n  [a]: u\n\n  x", "list[item[p list[item[p]] def p]]"),
        ("> a\n\n[a]: u\nx", "quote[p] def p"),
    ] {
        let root = tree(source, false);
        assert_eq!(kinds(blocks(&root)), kind, "{source:?}");
    }
}

#[test]
fn a_definition_in_every_item_of_a_long_list_stays_in_its_item() {
    // Each definition finds its item among many, and the one after the list sits beside it.
    let count = 64;
    let mut source: String = (0..count)
        .map(|ix| format!("- item {ix}\n\n  [r{ix}]: /u{ix}\n\n"))
        .collect();
    source.push_str("[last]: /l\nx");
    let root = tree(&source, false);
    let [Node::List(list), Node::Definition(last), Node::Paragraph(_)] = blocks(&root) else {
        panic!("a list, a definition and a paragraph: {} blocks", blocks(&root).len());
    };
    assert_eq!(last.identifier, "last");
    assert_eq!(list.children.len(), count);
    for (ix, item) in list.children.iter().enumerate() {
        let Node::ListItem(item) = item else {
            panic!("item {ix}: {item:?}");
        };
        let [Node::Paragraph(_), Node::Definition(definition)] = &item.children[..] else {
            panic!("item {ix} holds a paragraph and its definition: {item:?}");
        };
        assert_eq!(definition.identifier, format!("r{ix}"), "item {ix}");
    }
}

#[test]
fn cells_past_the_head_keep_their_text_as_written() {
    let root = tree("| a |\n| - |\n| b | [l][r] | x\\|y |\n\n[r]: /url\n", false);
    let Node::Table(table) = &blocks(&root)[0] else {
        panic!("a table: {root:?}");
    };
    let cells = table.children[1].children().expect("cells");
    let text = |ix: usize| match &cells[ix].children().expect("inlines")[..] {
        [Node::Text(text)] => text.value.clone(),
        other => panic!("written text: {other:?}"),
    };
    assert_eq!(cells.len(), 3, "{cells:?}");
    assert_eq!((text(1), text(2)), ("[l][r]".into(), "x\\|y".into()));
    #[cfg(feature = "test-support")]
    linear(|n| {
        let rows = "| b | [r0] |\n".repeat(n);
        let labels: String = (0..n).map(|ix| format!("[r{ix}]: /u\n")).collect();
        format!("| a |\n| - |\n{rows}\n{labels}")
    });
}

#[cfg(feature = "test-support")]
#[test]
fn rejected_domains_read_in_linear_time() {
    linear(|n| "www.a_".repeat(n * 16) + "z");
}

#[test]
fn a_byte_order_mark_starts_no_text() {
    let root = tree("\u{FEFF}---\ntitle: x\n---\n", true);
    assert!(
        matches!(blocks(&root), [Node::Yaml(front)] if front.value == "title: x"),
        "{root:?}"
    );
    let root = tree("\u{FEFF}# Title\n", false);
    let [Node::Heading(heading)] = blocks(&root) else {
        panic!("a heading: {root:?}");
    };
    assert_eq!(heading.position, 3..10);
    let root = tree(&("\u{FEFF}".repeat(1024) + "x"), false);
    assert!(matches!(blocks(&root), [Node::Paragraph(_)]), "{root:?}");
}

#[test]
fn a_list_is_loose_when_its_items_hold_paragraphs() {
    let spread = |source: &str| {
        let root = tree(source, false);
        let spread: Vec<bool> = (blocks(&root).iter())
            .filter_map(|node| match node {
                Node::List(list) => Some(list.spread),
                _ => None,
            })
            .collect();
        spread
    };
    assert_eq!(spread("- a\n- b\n"), [false]);
    assert_eq!(spread("- a\n\n- b\n"), [true]);
    assert_eq!(spread("1. a\n\n   more\n2. b\n"), [true]);
    let root = tree("- a\n\n- b\n  - c\n  - d\n", false);
    let Some(Node::List(outer)) = blocks(&root).first() else {
        panic!("{root:?}")
    };
    let inner = outer.children[1].children().expect("an item").last();
    assert!(outer.spread, "{root:?}");
    assert!(
        matches!(inner, Some(Node::List(list)) if !list.spread),
        "{root:?}"
    );
}

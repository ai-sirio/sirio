use super::*;

#[test]
fn blocks_nest_as_written() {
    let nodes = parse(
        "# Lift\n\nBlend **toward** [white](https://ely.dev).\n\n- [x] clamp\n- [ ] gamma\n\n> quoted\n\n```rust\nfn main() {}\n```\n\n---\n",
    );
    assert!(matches!(&nodes[0], Node::Heading(1, text) if text.text == "Lift"));
    let Node::Paragraph(line) = &nodes[1] else {
        panic!("a paragraph, not {:?}", nodes[1])
    };
    assert_eq!(line.text, "Blend toward white.");
    assert_eq!(line.styles, [(6..12, Style::Strong), (13..18, Style::Link)]);
    assert_eq!(line.links, [(13..18, "https://ely.dev".to_string())]);
    let Node::List { start: None, items } = &nodes[2] else {
        panic!("a list, not {:?}", nodes[2])
    };
    assert_eq!(
        items.iter().map(|(task, _)| *task).collect::<Vec<_>>(),
        [Some(true), Some(false)]
    );
    assert!(matches!(&items[0].1[0], Node::Paragraph(text) if text.text == "clamp"));
    assert!(matches!(&nodes[3], Node::Quote(inner) if inner.len() == 1));
    assert_eq!(
        nodes[4],
        Node::Code {
            language: Some("rust".into()),
            text: "fn main() {}".into(),
            from: 95
        }
    );
    assert_eq!(nodes[5], Node::Rule);
}

#[test]
fn an_image_inside_an_image_is_the_outer_one() {
    // markdown-rs drops the inner image from the alt.
    assert_eq!(
        parse("![outer ![inner](inner.png)](outer.png)"),
        [Node::Image {
            alt: "outer ".into(),
            url: "outer.png".into(),
            link: None,
            at: 0,
            width: None
        }]
    );
}

#[test]
fn an_image_splits_its_paragraph_and_references_resolve() {
    let nodes = parse("Before ![a][pic] after [site].\n\n[pic]: a.png\n[site]: https://ely.dev\n");
    assert!(matches!(&nodes[0], Node::Paragraph(text) if text.text == "Before "));
    assert_eq!(
        nodes[1],
        Node::Image {
            alt: "a".into(),
            url: "a.png".into(),
            link: None,
            at: 7,
            width: None
        }
    );
    let Node::Paragraph(after) = &nodes[2] else {
        panic!("a paragraph, not {:?}", nodes[2])
    };
    assert_eq!(after.text, " after site.");
    assert_eq!(after.links, [(7..11, "https://ely.dev".to_string())]);
    assert_eq!(nodes.len(), 3, "definitions draw nothing: {nodes:?}");
}

#[test]
fn a_soft_break_draws_as_a_line() {
    for source in [
        "one\ntwo  \nthree",
        "one\r\ntwo  \r\nthree",
        "one\rtwo  \rthree",
    ] {
        let nodes = parse(source);
        assert!(
            matches!(&nodes[0], Node::Paragraph(text) if text.text == "one\ntwo\nthree"),
            "{source:?} draws {nodes:?}"
        );
    }
}

#[test]
fn a_joined_reader_reads_a_soft_break_as_a_space() {
    let source = "one\ntwo  \nthree";
    let root = super::super::markdown::tree(source);
    let mut out = Vec::new();
    let paragraph = &root.children().expect("blocks")[0];
    Reader::joined(source, &root).block(paragraph, &mut out);
    assert!(
        matches!(&out[0], Node::Paragraph(text) if text.text == "one two\nthree"),
        "{out:?}"
    );
}

#[test]
fn a_heading_title_reads_on_one_line() {
    let source = "one\ntwo\n===\n";
    let root = super::super::markdown::tree(source);
    assert_eq!(Reader::new(source, &root).headings[0].title, "one two");
}

#[test]
fn definitions_nested_in_quotes_and_lists_resolve() {
    for source in [
        "[site] ![pic]\n\n> [site]: https://ely.dev\n> [pic]: a.png\n",
        "[site] ![pic]\n\n- [site]: https://ely.dev\n- [pic]: a.png\n",
    ] {
        let nodes = parse(source);
        let Node::Paragraph(text) = &nodes[0] else {
            panic!("a paragraph, not {:?}", nodes[0])
        };
        assert_eq!(text.links, [(0..4, "https://ely.dev".to_string())]);
        assert_eq!(
            nodes[1],
            Node::Image {
                alt: "pic".into(),
                url: "a.png".into(),
                link: None,
                at: 7,
                width: None
            }
        );
    }
}

/// Every link address in `nodes`, in order; a picture counts by its own address.
fn addresses(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Paragraph(text) | Node::Heading(_, text) => {
                out.extend(text.links.iter().map(|(_, url)| url.clone()));
            }
            Node::Image { url, .. } => out.push(url.clone()),
            Node::Quote(inner)
            | Node::Alert { body: inner, .. }
            | Node::Footnote { body: inner, .. } => out.extend(addresses(inner)),
            Node::List { items, .. } => {
                for (_, inner) in items {
                    out.extend(addresses(inner));
                }
            }
            Node::Table { head, rows, .. } => {
                let cells = head.iter().chain(rows.iter().flatten());
                for cell in cells {
                    out.extend(addresses(cell));
                }
            }
            _ => {}
        }
    }
    out
}

/// Each source must give exactly the one address `want`.
fn resolve_to(want: &str, sources: &[&str]) {
    for source in sources {
        assert_eq!(addresses(&parse(source)), [want], "{source:?}");
    }
}

#[test]
fn a_reference_label_that_breaks_inside_a_quote_resolves() {
    // pulldown-cmark drops the quote marker from a label that goes on past a line end, and matches
    // it to a definition as `one two`; the label as written still has the `> ` in it.
    let nodes = parse("> see [x][one\n> two]\n\n[one two]: https://d.example\n");
    let Node::Quote(inner) = &nodes[0] else {
        panic!("a quote, not {:?}", nodes[0])
    };
    let Node::Paragraph(text) = &inner[0] else {
        panic!("a paragraph, not {:?}", inner[0])
    };
    assert_eq!(text.text, "see x");
    assert_eq!(text.links, [(4..5, "https://d.example".to_string())]);
    assert_eq!(nodes.len(), 1, "the definition draws nothing: {nodes:?}");
    resolve_to(
        "https://d.example",
        &[
            // The marker is deeper, or has no space after it.
            ">> see [x][one\n>> two]\n\n[one two]: https://d.example\n",
            "> see [x][one\n>two]\n\n[one two]: https://d.example\n",
            "> > see [x][one\n> > two]\n\n[one two]: https://d.example\n",
            // The reference follows a line of the quote.
            "> first line\n> see [x][one\n> two] after\n\n[one two]: https://d.example\n",
        ],
    );
}

#[test]
fn a_shortcut_or_collapsed_reference_that_breaks_inside_a_quote_resolves() {
    resolve_to(
        "https://d.example",
        &[
            "> see [one\n> two]\n\n[one two]: https://d.example\n",
            "> see [one\n> two][]\n\n[one two]: https://d.example\n",
        ],
    );
}

#[test]
fn a_definition_label_that_breaks_inside_a_quote_resolves() {
    resolve_to(
        "https://d.example",
        &[
            // Only the definition breaks.
            "see [x][one two]\n\n> [one\n> two]: https://d.example\n",
            // Both do.
            "> see [x][one\n> two]\n>\n> [one\n> two]: https://d.example\n",
            "> see [one\n> two]\n>\n> [one\n> two]: https://d.example\n",
        ],
    );
}

#[test]
fn a_picture_reference_label_that_breaks_inside_a_quote_resolves() {
    resolve_to(
        "a.png",
        &[
            "> ![a][one\n> two]\n\n[one two]: a.png\n",
            "> ![one\n> two]\n\n[one two]: a.png\n",
            "> ![one\n> two][]\n\n[one two]: a.png\n",
        ],
    );
}

#[test]
fn a_reference_label_that_breaks_inside_a_list_item_resolves() {
    resolve_to(
        "https://d.example",
        &[
            // A plain list's indent is only blanks, which `identifier` always collapsed: these two
            // passed before the change and only guard it.
            "- see [x][one\n  two]\n\n[one two]: https://d.example\n",
            "1. see [x][one\n   two]\n\n[one two]: https://d.example\n",
            // A quote marker around or inside the item leads the continuation line, and is cut.
            "> - see [x][one\n>   two]\n\n[one two]: https://d.example\n",
            "> 1. see [x][one\n>    two]\n\n[one two]: https://d.example\n",
            "- > see [x][one\n  > two]\n\n[one two]: https://d.example\n",
        ],
    );
}

#[test]
fn a_reference_label_in_a_table_cell_reads_its_pipe_as_pulldown_does() {
    // pulldown-cmark reads `\|` in a cell as `|` before it matches the label.
    resolve_to(
        "https://d.example",
        &[
            "| head |\n| --- |\n| [x][a\\|b] |\n\n[a|b]: https://d.example\n",
            "| head |\n| --- |\n| [a\\|b] |\n\n[a|b]: https://d.example\n",
            "| head |\n| --- |\n| ![a\\|b] |\n\n[a|b]: https://d.example\n",
        ],
    );
}

#[test]
fn a_vertical_tab_or_form_feed_in_a_label_is_a_blank_as_pulldown_reads_it() {
    // pulldown-cmark folds every ASCII blank, \x0b and \x0c too, to one space in a label.
    resolve_to(
        "https://d.example",
        &[
            "[x][one\x0btwo]\n\n[one two]: https://d.example\n",
            "[x][one\x0ctwo]\n\n[one two]: https://d.example\n",
            "[one\x0b\x0c two]\n\n[one two]: https://d.example\n",
            "[one two]\n\n[one\x0ctwo]: https://d.example\n",
            "[x][ONE\x0btwo]\n\n[one\x0ctwo]: https://d.example\n",
            "> see [x][one\x0b\n> two]\n\n[one two]: https://d.example\n",
        ],
    );
}

#[test]
fn a_reference_finds_its_definition_by_case_and_spacing_as_pulldown_does() {
    resolve_to(
        "https://d.example",
        &[
            "[x][ONE   Two]\n\n[one\ttwo]: https://d.example\n",
            "[one two]\n\n[ONE\n  TWO]: https://d.example\n",
            "[Straße]\n\n[STRASSE]: https://d.example\n",
            // Both at once, over a quote's line break.
            "> see [x][ONE\n> Two]\n\n[one   two]: https://d.example\n",
        ],
    );
}

#[test]
fn an_image_inside_styles_and_links_stays_an_image() {
    let nodes = parse("*a ![x](x.png) b* [![y](y.png)](https://ely.dev)");
    let Node::Paragraph(before) = &nodes[0] else {
        panic!("a paragraph, not {:?}", nodes[0])
    };
    assert_eq!(
        (before.text.as_str(), &before.styles[..]),
        ("a ", &[(0..2, Style::Emphasis)][..])
    );
    assert_eq!(
        nodes[1],
        Node::Image {
            alt: "x".into(),
            url: "x.png".into(),
            link: None,
            at: 3,
            width: None
        }
    );
    let Node::Paragraph(after) = &nodes[2] else {
        panic!("a paragraph, not {:?}", nodes[2])
    };
    assert_eq!(
        (after.text.as_str(), &after.styles[..]),
        (" b ", &[(0..2, Style::Emphasis)][..])
    );
    assert_eq!(
        nodes[3],
        Node::Image {
            alt: "y".into(),
            url: "y.png".into(),
            link: Some("https://ely.dev".into()),
            at: 19,
            width: None
        }
    );
    assert_eq!(nodes.len(), 4, "{nodes:?}");
}

#[test]
fn a_heading_keeps_its_images_after_it() {
    let nodes = parse("# Lift ![x](x.png)");
    assert!(matches!(&nodes[0], Node::Heading(1, text) if text.text == "Lift "));
    assert!(matches!(&nodes[1], Node::Image { url, .. } if url == "x.png"));
}

/// Lists, quotes and paragraphs as a line, for what pulldown-cmark's events say: `ul[item[p(a)]]`,
/// `ol3[..]`, `quote[..]`, blocks side by side after a space.
fn outline(nodes: &[Node]) -> String {
    let lines: Vec<String> = nodes
        .iter()
        .map(|node| match node {
            Node::Paragraph(text) => format!("p({})", text.text),
            Node::Quote(inner) => format!("quote[{}]", outline(inner)),
            Node::List { start, items } => {
                let items: String = (items.iter())
                    .map(|(_, inner)| format!("item[{}]", outline(inner)))
                    .collect();
                match start {
                    Some(number) => format!("ol{number}[{items}]"),
                    None => format!("ul[{items}]"),
                }
            }
            other => panic!("not read here: {other:?}"),
        })
        .collect();
    lines.join(" ")
}

/// Every link address in `nodes`, in order.
fn links(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Paragraph(text) => out.extend(text.links.iter().map(|(_, url)| url.clone())),
            Node::Quote(inner) => out.extend(links(inner)),
            Node::List { items, .. } => {
                for (_, inner) in items {
                    out.extend(links(inner));
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn a_definition_right_after_a_list_builds_as_the_events_say() {
    // pulldown-cmark ends the list at the first line of the paragraph the definition opens, so
    // the list's span covers the definition; its events are the list, then the paragraph.
    for (source, outlined) in [
        ("- a\n\n[a]: https://e.example\nx", "ul[item[p(a)]] p(x)"),
        ("- \n[a]: u\nx", "ul[item[]] p(x)"),
        ("- a\n- b\n\n[a]: u\nx", "ul[item[p(a)]item[p(b)]] p(x)"),
        // The definitions are all there is after the list.
        ("- a\n\n[a]: u\n[b]: v", "ul[item[p(a)]]"),
        ("- a\n\n[a]: u\n[b]: v\nx", "ul[item[p(a)]] p(x)"),
        // An ordered list keeps its first number.
        ("3. a\n\n[a]: u\nx", "ol3[item[p(a)]] p(x)"),
    ] {
        assert_eq!(outline(&parse(source)), outlined, "{source:?}");
    }
}

#[test]
fn a_definition_right_after_a_nested_list_builds_as_the_events_say() {
    for (source, outlined) in [
        // The definition ends both lists: it follows the outer one.
        ("- a\n  - b\n\n[a]: u\nx", "ul[item[p(a) ul[item[p(b)]]]] p(x)"),
        // It sits in the outer item, after the inner list.
        (
            "- a\n  - b\n\n  [a]: u\n  x",
            "ul[item[p(a) ul[item[p(b)]] p(x)]]",
        ),
        // The same, in a quote.
        ("> - a\n>\n> [a]: u\n> x", "quote[ul[item[p(a)]] p(x)]"),
        ("> - a\n>   - b\n>\n> [a]: u\n> x", "quote[ul[item[p(a) ul[item[p(b)]]]] p(x)]"),
    ] {
        assert_eq!(outline(&parse(source)), outlined, "{source:?}");
    }
}

#[test]
fn a_definition_right_after_a_list_still_resolves_its_references() {
    // The definition stays in the tree: the paragraph it opened, the list before it, a list inside
    // it and a quote all reach it.
    for source in [
        "- a\n\n[a]: https://e.example\nsee [a]",
        "- see [a]\n\n[a]: https://e.example\nx",
        "- a\n  - b\n\n  [a]: https://e.example\n  see [a]",
        "> - see [a]\n>\n> [a]: https://e.example\n> x",
    ] {
        assert_eq!(
            links(&parse(source)),
            ["https://e.example".to_string()],
            "{source:?}"
        );
    }
}

#[test]
fn display_math_stands_alone() {
    assert_eq!(parse("$$c = 1$$"), [Node::Math("c = 1".into())]);
    assert_eq!(parse("$$\nc = 1\n$$"), [Node::Math("c = 1".into())]);
    // GitHub closes at the first `$$`, even mid-text.
    let both = parse("$$\na.$$\n\n$$\nb\\tag{8}$$\n");
    assert_eq!(
        both,
        [Node::Math("a.".into()), Node::Math("b\\tag{8}".into())]
    );
    assert_eq!(parse("$$a = 1\n$$"), [Node::Math("a = 1".into())]);
    let three = parse("$$\na.$$\n\n$$\nb\n$$\n\n$$\nc\n$$\n");
    let math = |tex: &str| Node::Math(tex.into());
    assert_eq!(three, [math("a."), math("b"), math("c")]);
    let packed = parse("$$\na.$$\n$$\nb\n$$");
    assert_eq!(packed, [math("a."), math("b")], "GitHub shows both");
    let opened = parse("$$a\n$$\n\n$$\nb\n$$\n");
    assert_eq!(
        opened,
        [math("a"), math("b")],
        "a text-opened one closes first"
    );
    let Node::Paragraph(inline) = &parse("so $c$ holds")[0] else {
        panic!("inline math stays in its paragraph")
    };
    assert_eq!(inline.styles, [(3..4, Style::Math)]);
}

#[test]
fn tables_keep_their_cells_and_alignment() {
    let nodes = parse("| Name | Lift |\n|:--|--:|\n| accent | 0.5 |\n");
    let Node::Table { aligns, head, rows } = &nodes[0] else {
        panic!("a table, not {:?}", nodes[0])
    };
    assert_eq!(aligns, &[AlignKind::Left, AlignKind::Right]);
    let text = |cell: &[Node]| match cell {
        [Node::Paragraph(line)] => line.text.clone(),
        other => panic!("one line, not {other:?}"),
    };
    assert_eq!(
        head.iter().map(|cell| text(cell)).collect::<Vec<_>>(),
        ["Name", "Lift"]
    );
    assert_eq!(text(&rows[0][1]), "0.5");
}

#[test]
fn a_picture_in_a_cell_stays_a_picture() {
    let nodes = parse("| Picture |\n| --- |\n| ![shot](assets/shot.png) |\n");
    let Node::Table { rows, .. } = &nodes[0] else {
        panic!("a table, not {:?}", nodes[0])
    };
    assert!(
        matches!(&rows[0][0][..], [Node::Image { alt, url, .. }] if alt == "shot" && url == "assets/shot.png")
    );
}

#[test]
fn cells_past_the_head_stay_undrawn() {
    let nodes = parse("| a |\n| - |\n| b | c |\n");
    let Node::Table { head, rows, .. } = &nodes[0] else {
        panic!("a table, not {:?}", nodes[0])
    };
    assert_eq!((head.len(), rows[0].len()), (1, 1));
}

#[test]
fn footnotes_number_their_references_and_hold_their_body() {
    let nodes = parse("Lift[^a].\n\n[^a]: Toward white.\n");
    let Node::Paragraph(line) = &nodes[0] else {
        panic!("a paragraph, not {:?}", nodes[0])
    };
    assert_eq!(line.text, "Lift¹.");
    assert_eq!(line.styles, [(4..6, Style::Footnote)]);
    assert!(matches!(&nodes[1], Node::Footnote { label, body } if label == "1" && body.len() == 1));
    let alone = parse(
        "[^z]: Unread.
",
    );
    assert!(matches!(&alone[0], Node::Footnote { label, .. } if label == "z"));
}

mod inline;

#[test]
fn what_a_browser_hides_draws_nothing() {
    // Chromium's innerText, `.work/R20.35/oracle/sweep.html`.
    let read = |source: &str| -> String {
        let texts: Vec<String> = (parse(source).into_iter())
            .map(|node| match node {
                Node::Paragraph(line) => line.text,
                other => panic!("a paragraph, not {other:?}"),
            })
            .collect();
        texts.join("\n\n")
    };
    for (inner, inline, block) in [
        ("<dialog open>b</dialog>", "a\n\nb\nc", "a\nb\nc"),
        ("<dialog>b</dialog>", "a\n\nc", "a c"),
        ("<span hidden><b>x</b></span>", "a  c", "a c"),
        ("<span hidden>x</span>", "a  c", "a c"),
        ("<div hidden>b</div>", "a\n\nc", "a c"),
        ("<datalist>b</datalist>", "a  c", "a c"),
        ("<audio>b</audio>", "a  c", "a c"),
        ("<audio controls>b</audio>", "a  c", "a  c"),
        ("<video><b>x</b> y</video>", "a  c", "a  c"),
        ("<canvas>b</canvas>", "a  c", "a  c"),
        ("<details><p>b</p></details>", "a\n\nc", "a\nc"),
        (
            "<details><summary>s</summary><summary>t</summary>b</details>",
            "a\n\ns\nc",
            "a\ns\nc",
        ),
        (
            "<details open><summary>s</summary>b</details>",
            "a\n\ns\nb\nc",
            "a\ns\nb\nc",
        ),
        (
            "<option>b</option><option>d</option>",
            "a\nb\nd\nc",
            "a\nb\nd\nc",
        ),
        ("<ul><li>b<li hidden>x</ul>", "a\n\nb\nc", "a\nb\nc"),
    ] {
        let source = format!("a {inner} c");
        // Inline text keeps its spaces as written.
        assert_eq!(read(&source), inline, "{source:?}");
        assert_eq!(read(&format!("<div>{source}</div>")), block, "{source:?}");
    }
}

/// The hook of the tests: any block of `<div>` becomes `X`; the raw blocks it saw are noted.
fn replacing(seen: &std::cell::RefCell<Vec<String>>) -> impl Fn(&str) -> Option<Vec<Node>> + '_ {
    |raw| {
        seen.borrow_mut().push(raw.to_owned());
        raw.contains("<div>").then(|| {
            vec![Node::Paragraph(Inline {
                text: "X".into(),
                ..Inline::default()
            })]
        })
    }
}

#[test]
fn parse_markdown_with_replaces_html_blocks_at_any_depth() {
    let source = "<div>x</div>\n\n> <div>x</div>\n\n- <div>x</div>\n";
    let seen = std::cell::RefCell::new(Vec::new());
    let hook = replacing(&seen);
    let nodes = parse_markdown_with(source, &hook);
    let x = || Node::Paragraph(Inline {
        text: "X".into(),
        ..Inline::default()
    });
    assert_eq!(
        nodes,
        [
            x(),
            Node::Quote(vec![x()]),
            Node::List {
                start: None,
                items: vec![(None, vec![x()])]
            }
        ]
    );
    assert_eq!(seen.borrow().len(), 3, "{:?}", seen.borrow());
    assert!(seen.borrow().iter().all(|raw| raw.contains("<div>x</div>")));
    // A hook that declines leaves upstream's reading, which `parse_markdown` is.
    let nodes = parse_markdown_with(source, &|_| None);
    assert_eq!(nodes, parse_markdown(source));
    assert_ne!(nodes[0], x());
}

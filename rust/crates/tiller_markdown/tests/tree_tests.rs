//! Tree-shape and streaming-contract tests for the markdown model.
//!
//! The streaming tests feed a document to [`tiller_markdown::parse`] one byte
//! at a time and assert the contract the chat pane relies on: no prefix
//! panics, the block count never decreases as more text arrives, and an
//! unterminated code fence comes back as an open code block.

use std::process::Command;
use tiller_markdown::{Alignment, Block, Document, Inline, ListItem, ListKind, TableCell, parse};

fn cell(text: &str) -> TableCell {
    TableCell {
        inline: vec![Inline::Text(text.to_string())],
    }
}

fn text(text: &str) -> Inline {
    Inline::Text(text.to_string())
}

#[test]
fn round_trips_every_block_kind() {
    let source = r#"# Heading one

A paragraph with *emphasis*, **strong**, `code` and a [link](https://example.com).

- alpha
- beta
  - beta one

> quoted

```rust
fn main() {}
```

---

| L | C | R |
| :--- | :---: | ---: |
| 1 | 2 | 3 |
"#;

    let expected = Document {
        blocks: vec![
            Block::Heading {
                level: 1,
                inline: vec![text("Heading one")],
            },
            Block::Paragraph {
                inline: vec![
                    text("A paragraph with "),
                    Inline::Emphasis(vec![text("emphasis")]),
                    text(", "),
                    Inline::Strong(vec![text("strong")]),
                    text(", "),
                    Inline::Code("code".to_string()),
                    text(" and a "),
                    Inline::Link {
                        target: "https://example.com".to_string(),
                        title: None,
                        children: vec![text("link")],
                    },
                    text("."),
                ],
            },
            Block::List {
                kind: ListKind::Bullet,
                tight: true,
                items: vec![
                    ListItem {
                        blocks: vec![Block::Paragraph {
                            inline: vec![text("alpha")],
                        }],
                        checked: None,
                    },
                    ListItem {
                        blocks: vec![
                            Block::Paragraph {
                                inline: vec![text("beta")],
                            },
                            Block::List {
                                kind: ListKind::Bullet,
                                tight: true,
                                items: vec![ListItem {
                                    blocks: vec![Block::Paragraph {
                                        inline: vec![text("beta one")],
                                    }],
                                    checked: None,
                                }],
                            },
                        ],
                        checked: None,
                    },
                ],
            },
            Block::BlockQuote {
                blocks: vec![Block::Paragraph {
                    inline: vec![text("quoted")],
                }],
            },
            Block::CodeBlock {
                language: Some("rust".to_string()),
                text: "fn main() {}\n".to_string(),
                open: false,
            },
            Block::ThematicBreak,
            Block::Table {
                alignment: vec![Alignment::Left, Alignment::Center, Alignment::Right],
                header: vec![cell("L"), cell("C"), cell("R")],
                rows: vec![vec![cell("1"), cell("2"), cell("3")]],
            },
        ],
    };

    assert_eq!(parse(source), expected);
}

#[test]
fn nested_lists_keep_structure() {
    let document = parse("- a\n  1. one\n  2. two\n- b");
    assert_eq!(
        document.blocks,
        vec![Block::List {
            kind: ListKind::Bullet,
            tight: true,
            items: vec![
                ListItem {
                    blocks: vec![
                        Block::Paragraph {
                            inline: vec![text("a")],
                        },
                        Block::List {
                            kind: ListKind::Ordered { start: 1 },
                            tight: true,
                            items: vec![
                                ListItem {
                                    blocks: vec![Block::Paragraph {
                                        inline: vec![text("one")],
                                    }],
                                    checked: None,
                                },
                                ListItem {
                                    blocks: vec![Block::Paragraph {
                                        inline: vec![text("two")],
                                    }],
                                    checked: None,
                                },
                            ],
                        },
                    ],
                    checked: None,
                },
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("b")],
                    }],
                    checked: None,
                },
            ],
        }]
    );
}

#[test]
fn an_ordered_list_carries_its_start_number() {
    let document = parse("3. c\n4. d");
    assert_eq!(
        document.blocks,
        vec![Block::List {
            kind: ListKind::Ordered { start: 3 },
            tight: true,
            items: vec![
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("c")],
                    }],
                    checked: None,
                },
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("d")],
                    }],
                    checked: None,
                },
            ],
        }]
    );
}

#[test]
fn a_loose_list_is_reported_as_not_tight() {
    assert_eq!(
        parse("- a\n\n- b").blocks,
        vec![Block::List {
            kind: ListKind::Bullet,
            tight: false,
            items: vec![
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("a")],
                    }],
                    checked: None,
                },
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("b")],
                    }],
                    checked: None,
                },
            ],
        }]
    );
}

#[test]
fn fenced_code_keeps_its_language_tag() {
    assert_eq!(
        parse("```rust\nlet x = 1;\n```").blocks,
        vec![Block::CodeBlock {
            language: Some("rust".to_string()),
            text: "let x = 1;\n".to_string(),
            open: false,
        }]
    );
    // A fence without a tag and an indented code block have no language.
    assert_eq!(
        parse("```\nplain\n```").blocks,
        vec![Block::CodeBlock {
            language: None,
            text: "plain\n".to_string(),
            open: false,
        }]
    );
    assert_eq!(
        parse("    indented\n    code").blocks,
        vec![Block::CodeBlock {
            language: None,
            text: "indented\ncode".to_string(),
            open: false,
        }]
    );
}

#[test]
fn table_with_mixed_alignment() {
    let document = parse("| a | b | c |\n| :--- | ---: | :---: |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |");
    assert_eq!(
        document.blocks,
        vec![Block::Table {
            alignment: vec![Alignment::Left, Alignment::Right, Alignment::Center,],
            header: vec![cell("a"), cell("b"), cell("c")],
            rows: vec![
                vec![cell("1"), cell("2"), cell("3")],
                vec![cell("4"), cell("5"), cell("6")],
            ],
        }]
    );
}

#[test]
fn inline_breaks_and_images_round_trip() {
    let document =
        parse("line one  \nline two\n\nline three\nline four\n\n![alt text](img.png \"t\")");
    assert_eq!(
        document.blocks,
        vec![
            Block::Paragraph {
                inline: vec![text("line one"), Inline::HardBreak, text("line two")],
            },
            Block::Paragraph {
                inline: vec![text("line three"), Inline::SoftBreak, text("line four")],
            },
            Block::Paragraph {
                inline: vec![Inline::Image {
                    target: "img.png".to_string(),
                    title: Some("t".to_string()),
                    alt: "alt text".to_string(),
                }],
            },
        ]
    );
}

#[test]
fn task_list_markers_are_kept() {
    assert_eq!(
        parse("- [x] done\n- [ ] todo").blocks,
        vec![Block::List {
            kind: ListKind::Bullet,
            tight: true,
            items: vec![
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("done")],
                    }],
                    checked: Some(true),
                },
                ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("todo")],
                    }],
                    checked: Some(false),
                },
            ],
        }]
    );
}

/// The streaming contract, byte by byte: parsing any prefix never panics,
/// the block count never decreases as more text arrives, and the source
/// ends inside an unterminated fence, so the final parse reports it open.
///
/// The source is ASCII on purpose: a prefix at every byte index is then a
/// valid `&str` (a caller streaming raw bytes must hold incomplete UTF-8
/// sequences until a character boundary; `parse` takes `&str`).
///
/// Tables are deliberately absent here: a partial table row (`| `) parses
/// as a paragraph that merges into the table once it gains cell content, so
/// the block count can legitimately decrease by one at exactly that
/// boundary — see [`a_partial_table_row_merges_into_the_table`] for the
/// pinned behaviour. Content is never lost either way.
#[test]
fn streaming_prefixes_never_panic_and_blocks_never_shrink() {
    let source = "# Heading\n\nA paragraph with *emphasis* and `code`.\n\n- one\n- two\n  - nested\n\n1. first\n2. second\n\n> quote\n\n```rust\nfn main() {\n    println!(\"hi\");\n";
    assert!(
        source.is_ascii(),
        "the streaming test source must be ASCII (byte = char)"
    );

    let mut previous_count = 0usize;
    for end in 0..=source.len() {
        let prefix = &source[..end];
        let document = parse(prefix);
        let count = document.blocks.len();
        assert!(
            count >= previous_count,
            "block count decreased at byte {end}: {previous_count} -> {count} for {prefix:?}"
        );
        previous_count = count;
    }

    // The final parse ends inside the fence: an open code block.
    let document = parse(source);
    assert_eq!(
        document.blocks.last(),
        Some(&Block::CodeBlock {
            language: Some("rust".to_string()),
            text: "fn main() {\n    println!(\"hi\");\n".to_string(),
            open: true,
        })
    );

    // Every prefix from the moment the opening fence is complete reports a
    // code block as its last block (the fence never closes in this source).
    let fence_start = source.find("```rust").expect("fence in source");
    for end in (fence_start + 7)..=source.len() {
        let last = parse(&source[..end]).blocks.pop().expect("non-empty");
        assert!(
            matches!(last, Block::CodeBlock { .. }),
            "byte {end}: expected a code block, got {last:?}"
        );
    }
}

/// The one place the block count legitimately decreases: a partial table
/// row (`| `) is a paragraph until it gains cell content, at which point it
/// becomes a row of the table above. The text is preserved either way.
#[test]
fn a_partial_table_row_merges_into_the_table() {
    let partial = parse("| a | b |\n| :- | -: |\n| ");
    assert_eq!(partial.blocks.len(), 2, "table plus a paragraph row");
    assert!(matches!(partial.blocks[1], Block::Paragraph { .. }));

    let grown = parse("| a | b |\n| :- | -: |\n| 1 |");
    assert_eq!(grown.blocks.len(), 1, "the paragraph merged into the table");
    assert!(matches!(grown.blocks[0], Block::Table { .. }));
}

#[test]
fn an_unterminated_fence_is_an_open_code_block() {
    assert_eq!(
        parse("```rust\nfn main() {}\n").blocks,
        vec![Block::CodeBlock {
            language: Some("rust".to_string()),
            text: "fn main() {}\n".to_string(),
            open: true,
        }]
    );
    // An empty open fence.
    assert_eq!(
        parse("```").blocks,
        vec![Block::CodeBlock {
            language: None,
            text: String::new(),
            open: true,
        }]
    );
    // Open fences nested in a quote and in a list item are still reported.
    let quoted = parse("> ```\n> code");
    assert!(matches!(
        quoted.blocks.last(),
        Some(Block::BlockQuote { blocks }) if matches!(
            blocks.last(),
            Some(Block::CodeBlock { open: true, .. })
        )
    ));
    let listed = parse("- ```\n  code");
    assert!(matches!(
        listed.blocks.last(),
        Some(Block::List { items, .. }) if matches!(
            items.last().and_then(|item| item.blocks.last()),
            Some(Block::CodeBlock { open: true, .. })
        )
    ));
}

#[test]
fn a_closed_fence_is_not_open() {
    assert_eq!(
        parse("```rust\nfn main() {}\n```").blocks,
        vec![Block::CodeBlock {
            language: Some("rust".to_string()),
            text: "fn main() {}\n".to_string(),
            open: false,
        }]
    );
    // Content after a closed fence: the code block is closed, the document
    // just keeps going.
    let document = parse("```rust\nx\n```\n\nafter");
    assert_eq!(
        document.blocks,
        vec![
            Block::CodeBlock {
                language: Some("rust".to_string()),
                text: "x\n".to_string(),
                open: false,
            },
            Block::Paragraph {
                inline: vec![text("after")],
            },
        ]
    );
}

#[test]
fn earlier_blocks_survive_longer_prefixes() {
    let prefix = "# One\n\nSome text with `code`.\n\n```rust\nlet a = 1;\n```";
    let longer = format!("{prefix}\n\n- more\n- items\n\n## Two");

    let earlier = parse(&prefix).blocks;
    let grown = parse(&longer).blocks;
    assert!(
        grown.len() >= earlier.len(),
        "blocks must not disappear as text arrives"
    );
    // Blank-line separated blocks cannot be retroactively rewritten by later
    // text: the earlier blocks must be structurally identical.
    assert_eq!(
        &grown[..earlier.len()],
        &earlier[..],
        "earlier blocks changed when the document grew"
    );
}

#[test]
fn parses_awkward_input_without_panicking() {
    for source in [
        "\r\n\r\n",
        "```\n```\n```\n",
        "```\n\n\n```\n",
        "a\n```\nb",
        "> > > nested\n> > > quote",
        "- \n- \n",
        "1.\n2.",
        "| |\n| :- |",
        "**unclosed\n\n*em\n",
        "    \n\t\n",
        "#\n##\n###\n",
        "a  \nb  \nc",
        "[x](y",
        "![alt](img",
    ] {
        let _ = parse(source);
    }
}

#[test]
fn ordinary_nesting_keeps_its_existing_tree_shape() {
    assert_eq!(
        parse("> quote\n\n- item").blocks,
        vec![
            Block::BlockQuote {
                blocks: vec![Block::Paragraph {
                    inline: vec![text("quote")],
                }],
            },
            Block::List {
                kind: ListKind::Bullet,
                items: vec![ListItem {
                    blocks: vec![Block::Paragraph {
                        inline: vec![text("item")],
                    }],
                    checked: None,
                }],
                tight: true,
            },
        ]
    );
}

#[test]
fn ten_thousand_nested_quotes_return_without_killing_the_test_runner() {
    if std::env::var_os("TILLER_MARKDOWN_DEEP_CHILD").is_some() {
        let source = "> ".repeat(10_000) + "text";
        let document = parse(&source);
        assert_eq!(document.blocks.len(), 1);
        return;
    }

    let status = Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "ten_thousand_nested_quotes_return_without_killing_the_test_runner",
            "--nocapture",
        ])
        .env("TILLER_MARKDOWN_DEEP_CHILD", "1")
        .status()
        .expect("spawn markdown depth child");
    assert!(
        status.success(),
        "10,000-level markdown parse child failed: {status}"
    );
}

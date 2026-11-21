// The ways a streamed reply built part by part can differ from `build` of the same text,
// written before the code. The oracle (`assert_equivalent_at_every_cut`) holds every one of
// them to "the same tree, headings, kinds and plain text as `build(text[..k])`" at every cut.
//
//  1. A later block changes an earlier one: a link definition or a footnote definition that
//     arrives after the reference it answers.
//  2. The reverse: a definition frozen with its block, and a reference in a later part that no
//     longer sees it (the part is parsed alone).
//  3. A label defined twice: the first definition wins in the whole reply, not in a part.
//  4. Footnote numbers (and a definition's label) go by first reference across the whole
//     reply; a part numbers from one.
//  5. Offset drift: a leading BOM, CRLF and lone CR line ends, and `$$` fences (Ely reads them
//     as longer tilde fences) put the text Ely read and the source out of step.
//  6. A lost or duplicated byte at a cut: segments and the tail must tile the text.
//  7. Source offsets kept in the tree (`Code.from`, `Image.at`, the pieces of an inline) are
//     counted from the part; they must be counted from the reply.
//  8. Heading ordinals, slugs and the table of contents computed per part: a later heading
//     missing from a table of contents above it, a repeated slug not made unique.
//  9. A frozen segment left standing after a rewind, so a block is drawn twice.
// 10. The nesting fallback not applied to the whole reply: past the cap the full build makes
//     one plain paragraph of everything, never of a part.
// 11. Block HTML room is the cap less the deepest Markdown of the whole reply: a segment or
//     tail parsed alone gets too much, and one frozen before a deep block arrives keeps it.
// 12. A `$$` display-math pair that crosses the cut, an open one, or a stray `$$` run that
//     shifts which later lines pair.
// 13. A `\(` pair across the cut (Ely reads one pair inside one paragraph).
// 14. A non-monotonic cut: it may move back only on a rewind.
// 15. A cut inside a fence, a list, a quote, a table or an HTML block; or after the indent of
//     an indented block, which turns an indented code block into a paragraph.
// 16. A cut that never advances: the equality holds only because nothing was ever frozen.
// 17. A block that starts on a line still being written and is no block start once the line is
//     done (`#` then `#tag`, `<div` then `<divine`, a `| ` that is no row of the table above):
//     the block before it frozen as if that start were certain.

use super::*;
use crate::markdown::{Policy, Surface, build};

const SHOWCASE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../Scripts/Tests/fixtures/markdown-documents/showcase.md"
));

fn chat() -> Policy<'static> {
    Policy {
        surface: Surface::Chat,
        base: None,
        diagrams: None,
        file_links: None,
    }
}

/// Fails, naming the first top-level node that differs, unless `streamed` is `full`: its nodes
/// (`PartialEq`), headings, kinds, plain text and the diagrams it still wants.
fn assert_same(streamed: &MarkdownBlocks, full: &MarkdownBlocks, what: &str) {
    assert_eq!(
        streamed.nodes.len(),
        full.nodes.len(),
        "{what}: the number of top-level nodes"
    );
    for (at, (got, want)) in streamed.nodes.iter().zip(full.nodes.iter()).enumerate() {
        assert!(
            got == want,
            "{what}: node {at} differs\nstreamed: {got:#?}\nfull:     {want:#?}"
        );
    }
    assert_eq!(streamed.headings, full.headings, "{what}: headings");
    assert_eq!(streamed.kinds, full.kinds, "{what}: kinds");
    assert_eq!(streamed.text.text, full.text.text, "{what}: plain text");
    assert_eq!(
        streamed.missing_diagrams, full.missing_diagrams,
        "{what}: diagrams still wanted"
    );
}

/// The largest char boundary of `text` at or before `at`.
fn floor(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// `build` of every prefix of `fixture` that ends on a char boundary, by its length.
fn full_builds(fixture: &str) -> Vec<Option<MarkdownBlocks>> {
    let mut policy = chat();
    (0..=fixture.len())
        .map(|cut| fixture.is_char_boundary(cut).then(|| build(&fixture[..cut], &mut policy)))
        .collect()
}

/// For every byte cut k of `fixture` that falls on a char boundary, feeding text[..k] in
/// `chunk`-byte steps gives exactly build(text[..k]): nodes (PartialEq), headings, kinds and
/// text.text.
fn assert_equivalent_at_every_cut(fixture: &str, chunk: usize) {
    assert_equivalent_against(fixture, chunk, &full_builds(fixture));
}

/// [`assert_equivalent_at_every_cut`], against the builds of the cuts already made.
///
/// One stream is fed `chunk` bytes at a time; at each step the cuts up to the next one are
/// checked on a clone of it, which is the stream a reply cut there would have been.
fn assert_equivalent_against(fixture: &str, chunk: usize, fulls: &[Option<MarkdownBlocks>]) {
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let mut fed = 0;
    while fed < fixture.len() {
        let next = floor(fixture, (fed + chunk).min(fixture.len()));
        let next = if next == fed {
            (fed + 1..=fixture.len())
                .find(|at| fixture.is_char_boundary(*at))
                .expect("the text ends on a boundary")
        } else {
            next
        };
        for cut in (fed + 1..=next).filter(|at| fixture.is_char_boundary(*at)) {
            let mut fork = stream.clone();
            let streamed = fork.update(&fixture[..cut], &mut policy);
            let full = fulls[cut].as_ref().expect("a build of every boundary");
            assert_same(
                &streamed,
                full,
                &format!("chunk {chunk}, cut {cut} of {fixture:?}"),
            );
        }
        stream.update(&fixture[..next], &mut policy);
        fed = next;
    }
}

/// The cut after each `chunk`-byte step, and the stream at the end.
fn cuts_by_step(fixture: &str, chunk: usize) -> (Vec<usize>, StreamingDoc) {
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let mut cuts = Vec::new();
    let mut fed = 0;
    while fed < fixture.len() {
        let next = floor(fixture, (fed + chunk).min(fixture.len())).max(fed + 1);
        let next = (next..=fixture.len())
            .find(|at| fixture.is_char_boundary(*at))
            .expect("the text ends on a boundary");
        stream.update(&fixture[..next], &mut policy);
        cuts.push(stream.cut());
        fed = next;
    }
    (cuts, stream)
}

/// The steps at which the cut moved back.
fn rewinds(cuts: &[usize]) -> Vec<usize> {
    (1..cuts.len()).filter(|at| cuts[*at] < cuts[*at - 1]).collect()
}

/// Every segment and the tail tile the text: no byte lost, none twice.
fn assert_tiles(stream: &StreamingDoc, text: &str) {
    let mut end = 0;
    for segment in stream.segments() {
        assert_eq!(segment.range.start, end, "a gap or an overlap before {segment:?}");
        assert!(segment.range.end > segment.range.start, "an empty segment");
        end = segment.range.end;
    }
    assert_eq!(end, stream.cut(), "the segments end where the tail starts");
    assert!(stream.cut() <= text.len());
}

#[test]
fn showcase_matches_a_full_build_at_every_cut() {
    let started = std::time::Instant::now();
    let fulls = full_builds(SHOWCASE);
    eprintln!("showcase, {} builds: {:?}", fulls.len(), started.elapsed());
    for chunk in [7, 200] {
        let started = std::time::Instant::now();
        assert_equivalent_against(SHOWCASE, chunk, &fulls);
        eprintln!("showcase, chunk {chunk}: {:?}", started.elapsed());
    }
}

/// The oracle holds when nothing ever freezes, so it also asks for the cut to have moved:
/// the streamed SHOWCASE ends with only its last block in the tail.
#[test]
fn showcase_freezes_everything_but_its_last_block() {
    let (_, stream) = cuts_by_step(SHOWCASE, 7);
    assert!(
        SHOWCASE[stream.cut()..].starts_with("Tail line 1"),
        "the tail starts at {:?}",
        &SHOWCASE[stream.cut()..][..40.min(SHOWCASE.len() - stream.cut())]
    );
    assert_tiles(&stream, SHOWCASE);
    assert!(!stream.is_whole());
}

#[test]
fn a_late_link_definition_rewinds() {
    let text = "[a][x]\n\npara\n\n[x]: https://e.com\n\nmore one\n\nmore two\n\nmore three\n";
    assert_equivalent_at_every_cut(text, 5);
    assert_equivalent_at_every_cut(text, 1);
    let (cuts, stream) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "the definition rewound nothing: {cuts:?}");
    assert_tiles(&stream, text);
}

#[test]
fn a_late_footnote_definition_rewinds() {
    let text = "ref[^1]\n\npara\n\n[^1]: note\n";
    assert_equivalent_at_every_cut(text, 3);
    assert_equivalent_at_every_cut(text, 1);
    let (cuts, _) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "the definition rewound nothing: {cuts:?}");
}

/// The part after a frozen definition reads it too (failure 2).
#[test]
fn a_definition_above_its_reference_matches() {
    let text = "[x]: https://e.com\n\npara\n\nsee [a][x] and [x]\n\nmore\n\nlast\n";
    assert_equivalent_at_every_cut(text, 4);
    assert_equivalent_at_every_cut(text, 1);
}

/// The first definition of a label wins in the whole reply (failure 3).
#[test]
fn a_repeated_definition_keeps_the_first() {
    let text =
        "[x]: https://one.example\n\npara\n\n[x]: https://two.example\n\nsee [a][x]\n\nend\n";
    assert_equivalent_at_every_cut(text, 4);
}

/// A footnote is numbered by its first reference in the whole reply, and so is its definition
/// (failure 4); labels that are words make a wrong number show.
#[test]
fn footnote_numbers_follow_the_whole_reply() {
    let text = "x[^n]\n\n[^n]: N\n\nmid\n\ny[^m] and again[^n]\n\n[^m]: M\n\nlast\n\nend\n";
    assert_equivalent_at_every_cut(text, 3);
    let later = "one[^a]\n\n[^a]: A\n\nbetween\n\ntwo[^b]\n\n[^b]: B\n\nafter\n\nend\n";
    assert_equivalent_at_every_cut(later, 2);
}

#[test]
fn an_open_display_math_holds_the_cut() {
    let text = "$$\nx\n\ny\n$$\n\nafter\n";
    assert_equivalent_at_every_cut(text, 4);
    assert_equivalent_at_every_cut(text, 1);
    // Until the closing fence arrives the tail starts at the opening one.
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let open = "para\n\n$$\nx\n\ny\n\nz\n";
    for end in 1..=open.len() {
        stream.update(&open[..end], &mut policy);
        assert!(
            stream.cut() <= open.find("$$").expect("an opening fence"),
            "the cut passed the open fence at byte {end}: {}",
            stream.cut()
        );
    }
}

/// A `$$` that is not on a line of its own pairs with the next run, bare lines or not: one
/// stray run before a fence pair leaves the pair unfenced (failure 12).
#[test]
fn a_stray_dollar_run_shifts_which_fences_pair() {
    let text = "cost $$ here\n\npara\n\n$$\nx\n$$\n\nafter\n\nlast\n";
    assert_equivalent_at_every_cut(text, 3);
    assert_equivalent_at_every_cut(text, 1);
}

/// A closed pair lets the cut pass, so a math reply still streams part by part (failure 16),
/// and the text after the fences, which Ely read longer, is mapped back.
#[test]
fn a_closed_display_math_lets_the_cut_pass() {
    let text = "para\n\n$$\nx = 1\n$$\n\nmid\n\n$$\ny = 2\n$$\n\nend\n";
    assert_equivalent_at_every_cut(text, 3);
    let (_, stream) = cuts_by_step(text, 3);
    assert!(
        text[stream.cut()..].starts_with("end"),
        "the tail starts at {:?}",
        &text[stream.cut()..]
    );
    assert_tiles(&stream, text);
}

/// A `$$` line in a code block is code, not a fence: nothing waits for its partner.
#[test]
fn dollars_in_code_hold_nothing() {
    let text = "```sh\necho $$\n```\n\nafter\n\nlast\n";
    assert_equivalent_at_every_cut(text, 2);
    let (_, stream) = cuts_by_step(text, 2);
    assert!(text[stream.cut()..].starts_with("last"));
}

#[test]
fn backslash_paren_math_matches() {
    for text in [
        "\\(a\n\nb\\)\n\nc\n",
        "\\(a\\)\n\nb\n",
        "\\(a\\) and \\(b\\)\n\n\\(c\n\nd\\)\n\ne\n",
    ] {
        assert_equivalent_at_every_cut(text, 2);
        assert_equivalent_at_every_cut(text, 1);
    }
}

#[test]
fn crlf_and_bom_replies_match() {
    for text in [
        "\u{feff}# T\r\n\r\npara\r\n\r\n- a\r\n- b\r\n",
        "# T\r\rpara\r\r- a\r- b\r\r```\rcode\r```\r\rend\r",
        "\u{feff}para\r\n\r\n$$\r\nx\r\n$$\r\n\r\n![a](b.png)\r\n\r\n```rust\r\nlet a = 1;\r\n```\r\n\r\n\
         end\r\n",
    ] {
        assert_equivalent_at_every_cut(text, 3);
        assert_equivalent_at_every_cut(text, 1);
    }
}

#[test]
fn lists_quotes_tables_setext_and_lazy_lines_match() {
    let text = "\
# Title

Setext head
===========

> quote line one
continued lazily
> > nested

- item one
- item two
  continued

  second paragraph of item two

1. first
2. second

| a | b |
|---|:-:|
| 1 | 2 |
| 3 | 4 |

Para with trailing spaces\x20\x20
hard break

    indented code

   indented paragraph

* * *

![local](pixel.png) and a [link](https://example.com)

```rust
let a = 1;

let b = 2;
```

<div>
html

</div>

after
";
    assert_equivalent_at_every_cut(text, 1);
    assert_equivalent_at_every_cut(text, 13);
}

/// An unclosed fence is one block that never freezes: the cut stays at its start.
#[test]
fn an_open_fence_holds_the_cut() {
    let text = "para\n\n```rust\nlet a = 1;\n\nlet b = 2;\n";
    assert_equivalent_at_every_cut(text, 2);
    let (cuts, _) = cuts_by_step(text, 2);
    assert_eq!(*cuts.last().unwrap(), text.find("```").unwrap(), "{cuts:?}");
}

/// The block a line starts is not known until the line is done (failure 17).
#[test]
fn a_block_start_on_an_unfinished_line_is_not_trusted() {
    for text in [
        "I like\n#1 priority and more\n\nafter\n",
        "uses\n<pointer arithmetic here\n\nafter\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\nafter\n",
        "text\n```rust`\n\nafter\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// Block HTML nests to what the Markdown of the whole reply leaves (failure 11). A `div` makes
/// no level, so half of the fixtures nest `blockquote`s, which past the room keep their text.
#[test]
fn block_html_room_follows_the_deepest_markdown() {
    let deep = format!("{} deep\n", ">".repeat(31));
    for text in [
        // The deep quote arrives after the div: the div's room shrinks and it is built again.
        format!("<div><div>x</div></div>\n\n{deep}"),
        // The same with HTML that makes levels, and a block between, so that the HTML is
        // frozen before the deep quote starts to arrive.
        format!("<blockquote><blockquote>x</blockquote></blockquote>\n\nmid\n\n{deep}"),
        // The HTML arrives after the deep quote was frozen: it gets the room that is left.
        format!("{deep}\n<blockquote><blockquote>y</blockquote></blockquote>\n\nend\n"),
        format!("{deep}\n<div><div>y</div></div>\n\nend\n"),
    ] {
        assert_equivalent_at_every_cut(&text, 1);
        assert_equivalent_at_every_cut(&text, 7);
    }
}

#[test]
fn a_reply_past_the_cap_turns_whole() {
    let text = format!("para\n\n{}x\n", "> ".repeat(33));
    assert_equivalent_at_every_cut(&text, 3);
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let mut fed = 0;
    while fed < text.len() {
        fed = (fed + 3).min(text.len());
        stream.update(&text[..fed], &mut policy);
    }
    assert!(stream.is_whole());
    let full = stream.update(&text, &mut policy);
    assert_eq!(full.nodes.len(), 1);
    assert!(matches!(
        &full.nodes[0],
        ely_gpui_component::documents::MarkdownBlock::Paragraph(inline) if inline.text == text
    ));
}

#[test]
fn a_toc_lists_headings_that_arrive_after_it() {
    let text = "[[toc]]\n\n# A\n\n# B\n\n## A\n";
    assert_equivalent_at_every_cut(text, 1);
    assert_equivalent_at_every_cut(text, 4);
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let streamed = stream.update(text, &mut policy);
    assert_eq!(streamed.headings.len(), 3, "every heading, after the table too");
    assert_eq!(streamed.kinds.toc, 1);
}

/// Over SHOWCASE the cut moves back only where a definition arrives after the reference that
/// was frozen: at most at its footnote definition. (The fixtures of the rewinds above hold the
/// other half: that a definition which does arrive late does move it.)
#[test]
fn the_cut_never_moves_back_except_on_a_rewind() {
    let definition = SHOWCASE.find("[^1]: ").expect("the footnote definition");
    let end = definition + "[^1]: The footnote definition.".len();
    for chunk in [7, 60, 200] {
        let (cuts, _) = cuts_by_step(SHOWCASE, chunk);
        for step in rewinds(&cuts) {
            let fed = ((step + 1) * chunk).min(SHOWCASE.len());
            assert!(
                fed > definition && fed < end + chunk,
                "chunk {chunk}: the cut moved back at byte {fed}, not at the definition ({definition})"
            );
        }
        assert!(
            *cuts.last().unwrap() > SHOWCASE.len() / 2,
            "chunk {chunk}: the cut never got anywhere: {cuts:?}"
        );
    }
}

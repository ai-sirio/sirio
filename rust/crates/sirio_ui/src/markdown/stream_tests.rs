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
// 17. A block start that depends on a line that is not there yet: on the line being written
//     (`#` then `#tag`, `<div` then `<divine`, a `| ` that is no row of the table above); on the
//     lines after it (a table that needs its delimiter row, and may interrupt a paragraph; a
//     definition's title on the next lines, which takes what looked like a paragraph); on the
//     line before (after a blank line `2` is a paragraph and `2.` the next item of the list
//     above). The block before it frozen as if that start were certain.
// 18. A cut inside a pair of `$$` runs, a stray one in prose or display math written inline: the
//     runs after the cut pair with each other, and not as they do in the whole.
// 19. A byte order mark that starts a part in the middle of a reply: Ely drops it from the text
//     it reads, and in the whole reply it is text.
// 20. A reference that pads its label (`[ x ]`) not taken for a mention of `x`: no hold, no rewind.
// 21. A text that does not continue the one frozen (another reply, an edited one) answered with
//     the segments of the old one.
// 22. `segments()` and `cut()` out of step after a part was refused: the segments must always
//     tile the text up to the cut.

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
/// checked on a clone of it, which is the stream a reply cut there would have been. The clone's
/// segments must tile the text up to its cut too (failures 6 and 22).
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
            assert_tiles(&fork, &fixture[..cut]);
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
/// stray run before a fence pair takes the first fence, and the second fence is left alone
/// (failures 12 and 18). The math body holds a blank line, so that the two readings differ: a lone
/// formula is the same math block either way.
#[test]
fn a_stray_dollar_run_shifts_which_fences_pair() {
    for text in [
        "cost $$ here\n\npara\n\n$$\nx\n\ny\n$$\n\nafter\n\nlast\n",
        "cost $$ here\n\npara\n\n$$\nx\n$$\n\nafter\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 3);
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// A pair of runs that is no fence (display math written inline) can still stand across
/// blocks, and the cut may not fall inside it either: the fences after it would pair otherwise
/// (failure 18).
#[test]
fn a_dollar_pair_across_blocks_is_not_cut() {
    for text in [
        "$$x = 1\n\ny = 2$$\n\n$$\nz\n\nw\n$$\n\nend\n",
        "a $$b\n\nc$$ d\n\n$$\nz\n\nw\n$$\n\nend\n",
    ] {
        // One byte at a time the cut gets to the fence in time; a chunk that brings both
        // fences at once finds it inside the pair.
        for chunk in [1, 5, 11, 17, 29] {
            assert_equivalent_at_every_cut(text, chunk);
        }
    }
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
        "\u{feff}para\r\n\r\n$$\r\nx\r\n$$\r\n\r\n![a](b.png)\r\n\r\n\
         ```rust\r\nlet a = 1;\r\n```\r\n\r\nend\r\n",
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

/// A block that starts on a line right under a line of text can start there only by what the
/// lines after it say: a table needs its delimiter row, and may interrupt a paragraph, so the
/// text above it is a paragraph of its own or the first lines of the table's (failure 17).
#[test]
fn a_table_under_a_text_line_depends_on_its_delimiter_row() {
    for text in [
        // The delimiter row has three cells, the header two: no table, one paragraph.
        "Intro line\n| a | b |\n|---|---|---|\n| 1 | 2 |\n\nafter\n\nlast\n",
        "Intro line\n| a | b |\n|---|---|\n| 1 | 2 |\n\nafter\n\nlast\n",
        "Intro\nsecond line\n| a |\n|---|\n| 1 |\n\nafter\n\nlast\n",
        "# Head\n| a | b |\n|---|---|\n| 1 | 2 |\n\nafter\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
        assert_equivalent_at_every_cut(text, 5);
    }
}

/// After a blank line a block can still be taken up by the one before it, until its line is
/// done: `2` is a paragraph and `2.` the next item of the list above (failure 17).
#[test]
fn a_list_item_after_a_blank_line_is_no_paragraph_until_its_line_ends() {
    for text in [
        "1. a\n\n2. b\n\nafter\n\nlast\n",
        "- a\n\n- b\n\nafter\n\nlast\n",
        "- a\n\n* b\n\n* c\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// Ely drops one byte order mark from the start of the text it reads, and reads any other as
/// text: a part that starts at one would lose it (failure 19).
#[test]
fn a_byte_order_mark_inside_a_reply_is_text() {
    for text in [
        "para\n\n\u{feff}# T\n\nmore\n\nlast\n",
        "\u{feff}para\n\n\u{feff}\u{feff}x\n\nmore\n\nlast\n",
        "para\r\n\r\n\u{feff}- a\r\n- b\r\n\r\nlast\r\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// A reference may pad its label with white space (`[ x ]` names `[x]:`), so that is a mention
/// of the label too (failure 20).
#[test]
fn a_padded_reference_label_is_a_mention() {
    for text in [
        "[a][ x ]\n\npara\n\n[x]: https://e.com\n\nmore\n\nlast\n",
        "[a][\tx]\n\npara\n\n[x]: https://e.com\n\nmore\n\nlast\n",
        "[ x ]\n\npara\n\n[x]: https://e.com\n\nmore\n\nlast\n",
        "ref[^ n ]\n\npara\n\n[^n]: note\n\nmore\n\nlast\n",
        "[x]: https://e.com\n\npara\n\n[a][ x ]\n\nmore\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// A reference matches its definition by case folding, which writes some ASCII letters in other
/// ones: `ß` is `ss`, and the Kelvin sign is `k` (failure 20).
#[test]
fn a_label_that_folds_to_ascii_is_a_mention() {
    for text in [
        "[a][ß]\n\npara\n\n[ss]: https://e.com\n\nmore\n\nlast\n",
        "[a][\u{212a}]\n\npara\n\n[k]: https://e.com\n\nmore\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// A definition goes on over the lines under it: its title can stand on the next line, and can
/// run over several, so what follows it is no block until the title is closed or given up.
#[test]
fn a_definition_title_on_the_next_lines_stays_with_it() {
    for text in [
        "para\n\n[x]: https://e.com\n\"multi\nline\"\n\nend\n\nlast\n",
        "para\n\n[x]: https://e.com\n'multi\nline'\n\nend\n\nlast\n",
        "para\n\n[x]: https://e.com\n(multi\nline)\n\nend\n\nlast\n",
        "para\n\n[x]:\nhttps://e.com\n\"title\"\nafter\n\nlast\n",
        "para\n\n[x]: https://e.com\n\"never closed\nstill open\n\nend\n\nlast\n",
        "[a][x]\n\n[x]: https://e.com\n\"t\" and more\n\nend\n\nlast\n",
    ] {
        assert_equivalent_at_every_cut(text, 1);
    }
}

/// `update` takes a text that only grows. One that does not start with what was frozen starts
/// over, whatever its length: the doc is for one reply.
#[test]
fn a_text_that_does_not_continue_the_last_one_starts_over() {
    let mut policy = chat();
    let first = "one\n\ntwo\n\nthree\n\nfour\n";
    let mut stream = StreamingDoc::new();
    stream.update(first, &mut policy);
    assert!(stream.cut() > 0, "something froze");
    for second in [
        "ONE\n\ntwo\n\nthree\n\nfour\n\nfive\n",
        "one\n\nTWO\n\nthree\n\nfour\n\nfive\n",
        "uno\n\ndue\n\ntre\n\nquattro\n",
        "one\n",
        "",
    ] {
        let mut fork = stream.clone();
        let streamed = fork.update(second, &mut policy);
        assert_same(&streamed, &build(second, &mut policy), &format!("then {second:?}"));
        assert_tiles(&fork, second);
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

/// Another reply fed to a doc that turned whole starts over, as it does for one that did not
/// (failure 21).
#[test]
fn another_reply_after_a_whole_one_starts_over() {
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let text = format!("one\n\ntwo\n\n{}x\n", "> ".repeat(33));
    let mut fed = 0;
    while fed < text.len() {
        fed = (fed + 3).min(text.len());
        stream.update(&text[..fed], &mut policy);
    }
    assert!(stream.is_whole());
    assert!(stream.cut() > 0, "something was frozen before the reply turned whole");
    let other = "zzz\n\nyyy\n";
    let streamed = stream.update(other, &mut policy);
    assert!(!stream.is_whole(), "another reply is not past the cap");
    assert_same(&streamed, &build(other, &mut policy), "another reply after a whole one");
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
                "chunk {chunk}: the cut moved back at byte {fed}, not at the definition \
                 ({definition})"
            );
        }
        assert!(
            *cuts.last().unwrap() > SHOWCASE.len() / 2,
            "chunk {chunk}: the cut never got anywhere: {cuts:?}"
        );
    }
}

/// A reply that reuses a footnote and a link label in every copy of one document: each reuse
/// rewinds the stream to the first copy, and the chunks after it must go on freezing (the tail
/// is small again at the end) and still give the build of the text at every cut, with segments
/// that tile it. The `$$` runs of the copies pair across them, so the cut waits for them too.
#[test]
fn a_rewind_is_followed_by_chunks_that_freeze_again() {
    const DOC: &str = "## Part\n\nA line with a note[^1] and a [link][ref].\n\n\
        More text $$x^2$$ and `code`.\n\n```sh\necho $$\n```\n\n- one\n- two\n\n\
        [ref]: https://e.com\n\n[^1]: The note.\n\nClosing words.\n\n";
    let text = DOC.repeat(3);
    for chunk in [1, 7, 31] {
        assert_equivalent_at_every_cut(&text, chunk);
    }
    let (cuts, stream) = cuts_by_step(&text, 5);
    assert!(!rewinds(&cuts).is_empty(), "the reuse rewound nothing: {cuts:?}");
    assert_tiles(&stream, &text);
    assert!(
        text.len() - stream.cut() < DOC.len(),
        "the tail still holds {} bytes after the last reuse",
        text.len() - stream.cut()
    );
    assert!(!stream.is_whole());
}

// ---- Labels match whole, a rewind meets `\(`, a shallow rewind (fix round 2) ----

/// A label that only begins another is no mention of it: `[10]` does not name label `1`. With
/// only `[1]` defined, the blocks that cite `[10]` after its definition hold nothing back, and
/// the cut gets to the last block, which cites it too. (Matching the front of a label made `[1]` take every `[10]`
/// for a mention: the span of its mentions then ran to the last of them, no cut fell inside it,
/// and each later `[10]` rewound the stream to the first block that cited `[1]`.)
#[test]
fn a_label_that_starts_another_is_no_mention_of_it() {
    let text = "ref [a][1]\n\n[1]: https://one.example\n\nb1\n\nb2 see [x][10]\n\nb3\n\n\
        b4 see [y][10]\n";
    for chunk in [1, 4, 9] {
        assert_equivalent_at_every_cut(text, chunk);
    }
    let (cuts, stream) = cuts_by_step(text, 4);
    assert!(
        text[stream.cut()..].starts_with("b4"),
        "the tail starts at {:?}, after {} rewinds",
        &text[stream.cut()..],
        rewinds(&cuts).len()
    );
    assert_tiles(&stream, text);
}

/// Labels that start one another are told apart, whichever is defined first, and the same
/// in words: the stream equals the build at every cut.
#[test]
fn labels_that_start_one_another_match_in_either_order() {
    for text in [
        "[a][1] and [b][10]\n\nx\n\n[1]: https://one.example\n\ny\n\n[10]: https://ten.example\n\nz\n\nend\n",
        "[a][1] and [b][10]\n\nx\n\n[10]: https://ten.example\n\ny\n\n[1]: https://one.example\n\nz\n\nend\n",
        "see [a][topic-1] and [b][topic-10]\n\nx\n\n[topic-10]: https://ten.example\n\ny\n\n\
         see [topic-1] again\n\n[topic-1]: https://one.example\n\nz\n\nend\n",
        "see [a][topic-10] and [b][topic-1]\n\nx\n\n[topic-1]: https://one.example\n\ny\n\n\
         [topic-10]: https://ten.example\n\nz\n\nend\n",
    ] {
        for chunk in [1, 6] {
            assert_equivalent_at_every_cut(text, chunk);
        }
        let (_, stream) = cuts_by_step(text, 3);
        assert_tiles(&stream, text);
    }
}

/// A definition that arrives late rewinds to a block that wrote its label in another case: the
/// mention is matched as pulldown matches the label, not letter by letter. Each spelling is a
/// reply of its own, so that one that fails alone fails the test.
#[test]
fn a_late_definition_rewinds_for_every_spelling_of_its_label() {
    for spelling in ["foo", "FOO", "fOo ", " Foo"] {
        let text = format!(
            "see [a][{spelling}]\n\npara\n\nmore\n\n[Foo]: https://f.example\n\nlast one\n\nlast two\n"
        );
        for chunk in [1, 5] {
            assert_equivalent_at_every_cut(&text, chunk);
        }
        let (cuts, _) = cuts_by_step(&text, 3);
        assert!(!rewinds(&cuts).is_empty(), "the spelling {spelling:?} rewound nothing: {cuts:?}");
    }
}

/// White space inside a label is one space, at its ends none, a line end among it too: a
/// definition that writes it otherwise still rewinds to the block that cites it.
#[test]
fn white_space_in_a_label_is_one_space() {
    for text in [
        "see [a][my  label]\n\nmid\n\n[My Label]: https://m.example\n\nafter\n\nlast\n",
        "see [a][ my label ]\n\nmid\n\n[my\tlabel]: https://m.example\n\nafter\n\nlast\n",
        "see [a][one\ntwo]\n\nmid\n\n[ONE TWO]: https://o.example\n\nafter\n\nlast\n",
    ] {
        for chunk in [1, 5] {
            assert_equivalent_at_every_cut(text, chunk);
        }
        let (cuts, _) = cuts_by_step(text, 3);
        assert!(!rewinds(&cuts).is_empty(), "{text:?}: nothing rewound: {cuts:?}");
    }
}

/// An escaped bracket is part of a label: the mention ends at the first bracket that is not.
#[test]
fn an_escaped_bracket_belongs_to_its_label() {
    let text = "see [a][x\\]y]\n\nmid\n\n[X\\]Y]: https://x.example\n\nlast\n\nend\n";
    for chunk in [1, 5] {
        assert_equivalent_at_every_cut(text, chunk);
    }
    let (cuts, _) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "nothing rewound: {cuts:?}");
}

/// A rewind in a reply that holds `\(` and `\[` pairs: they are read in a second pass, and the
/// rewound stream is still the build at every cut.
#[test]
fn a_rewind_with_parenthesised_math_matches() {
    let text = "Euler \\(e^{i\\pi}\\) and [a][x].\n\nmid \\[ y \\]\n\n[x]: https://e.example\n\n\
        after \\(z\\)\n\nend\n";
    for chunk in [1, 5] {
        assert_equivalent_at_every_cut(text, chunk);
    }
    let (cuts, stream) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "the definition rewound nothing: {cuts:?}");
    assert_tiles(&stream, text);
}

/// The same with a block of HTML in it: the room of block HTML is the depth of the whole reply,
/// the second pass included, which only a parse of the tail tells, so this reply is read the way
/// that parses the tail first.
#[test]
fn a_rewind_with_parenthesised_math_and_block_html_matches() {
    let text = "<div>\nblock html\n</div>\n\nEuler \\(e\\) and [a][x].\n\nmid\n\n\
        [x]: https://e.example\n\nafter \\(z\\)\n\nend\n";
    for chunk in [1, 5] {
        assert_equivalent_at_every_cut(text, chunk);
    }
    let (cuts, stream) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "the definition rewound nothing: {cuts:?}");
    assert_tiles(&stream, text);
}

/// A rewind that only reaches back to a late segment leaves most of the reply frozen: the cut
/// moves back and is still in the second half of what has arrived. (Deeper than that the
/// chunk is answered by a plain build and nothing is frozen in it; this is the other branch,
/// where the rewound tail is parsed and what can freeze does.)
#[test]
fn a_late_definition_rewinds_only_a_late_segment() {
    let mut text = String::new();
    for n in 0..10 {
        text.push_str(&format!("Filler paragraph number {n}, long enough to weigh a little.\n\n"));
    }
    text.push_str(
        "A reference [a][late] in a block near the end.\n\nafter one\n\nafter two\n\n\
         [late]: https://late.example\n\nlast\n",
    );
    for chunk in [1, 7] {
        assert_equivalent_at_every_cut(&text, chunk);
    }
    let chunk = 3;
    let (cuts, stream) = cuts_by_step(&text, chunk);
    let shallow = rewinds(&cuts).into_iter().any(|step| {
        let fed = ((step + 1) * chunk).min(text.len());
        cuts[step] * 2 >= fed
    });
    assert!(shallow, "no rewind stayed in the second half of the reply: {cuts:?}");
    assert_tiles(&stream, &text);
}

/// A reply that starts with a long fence: nothing can freeze while the fence is open, and the
/// stream does not look at the reply on every chunk for it (it looks again once the reply has
/// grown). It is still the build at every chunk end, its segments tile the text, and once the
/// fence is closed and the text goes on it freezes again, before the end.
#[test]
fn a_stream_that_could_freeze_nothing_freezes_again_when_the_text_allows_it() {
    let mut text = String::from("```text\n");
    while text.len() < 5000 {
        text.push_str("a line of the fence, as long as a line of code goes\n");
    }
    text.push_str("```\n\n");
    for n in 0..100 {
        text.push_str(&format!("A paragraph after the fence, number {n}, with a few words in it.\n\n"));
    }
    let mut policy = chat();
    let mut stream = StreamingDoc::new();
    let (mut at, mut froze_at) = (0, None);
    while at < text.len() {
        at = floor(&text, (at + 61).min(text.len()));
        let streamed = stream.update(&text[..at], &mut policy);
        assert_same(&streamed, &build(&text[..at], &mut chat()), &format!("chunk end {at}"));
        assert_tiles(&stream, &text[..at]);
        if stream.cut() > 0 && froze_at.is_none() {
            froze_at = Some(at);
        }
    }
    let froze_at = froze_at.expect("the stream never froze anything");
    assert!(froze_at < text.len(), "it froze only at the end of the reply");
}

/// The index of a text's brackets holds every reference that pulldown resolves: for each case,
/// pulldown resolves the reference to a definition appended after the text (a link whose address
/// is the definition's, a footnote reference), and the index must say that the label is mentioned
/// where it is. (Whatever the index says beyond that only keeps parts together.) A label that
/// only begins another is no mention.
#[test]
fn every_reference_pulldown_resolves_is_a_mention_in_the_index() {
    let long = "abcdefghij".repeat(500);
    let long_text = format!("see [a][{long}] there\n");
    let cases: Vec<(&str, &str, bool)> = vec![
        ("see [a][foo] here\n", "foo", false),
        ("[foo]\n", "foo", false),
        ("[foo][]\n", "foo", false),
        ("see [a][FOO]\n", "foo", false),
        ("see [a][ foo  bar ]\n", "foo bar", false),
        ("see [a][foo\nbar]\n", "foo bar", false),
        ("> see [x][one\n> two]\n", "one two", false),
        ("> [one\n> two]\n", "one two", false),
        ("> > [one\n> > two]\n", "one two", false),
        ("- an item [x][one\n  two]\n", "one two", false),
        ("see [a][x\\]y]\n", "x\\]y", false),
        ("see [a][\u{c9}COLE]\n", "\u{e9}cole", false),
        (&long_text, &long, false),
        ("note[^n]\n", "n", true),
        ("note[^N]\n", "n", true),
    ];
    for (text, label, note) in cases {
        let probe = match note {
            true => format!("{text}\n[^{label}]: a note\n"),
            false => format!("{text}\n[{label}]: https://probe.example/x\n"),
        };
        let mut resolved = Vec::new();
        for (event, range) in Parser::new_ext(&probe, markdown_options()).into_offset_iter() {
            match event {
                Event::Start(Tag::Link { dest_url, .. })
                    if !note && &*dest_url == "https://probe.example/x" && range.end <= text.len() =>
                {
                    resolved.push(range);
                }
                Event::FootnoteReference(_) if note && range.end <= text.len() => resolved.push(range),
                _ => {}
            }
        }
        let shown = &text[..text.len().min(60)];
        assert!(!resolved.is_empty(), "pulldown resolves no reference in {shown:?}");
        let found = Brackets::of(text).mentions(&Label { name: label.to_owned(), note });
        for range in resolved {
            let (first, last) = found.unwrap_or_else(|| panic!("{shown:?}: pulldown resolves it, the index finds no mention"));
            assert!(
                first < range.end && range.start <= last,
                "{shown:?}: the reference is at {range:?}, the index puts the label at {first}..={last}"
            );
        }
    }
    for (text, label) in [("see [a][10]\n", "1"), ("see [10]\n", "1"), ("see [x][topic-10]\n", "topic-1")] {
        let found = Brackets::of(text).mentions(&Label { name: label.to_owned(), note: false });
        assert!(found.is_none(), "{label:?} is no mention of {text:?}: {found:?}");
    }
}

/// A label longer than the index scans for is still a mention: pulldown counts only white space,
/// escapes and non-ASCII bytes against its limit of a thousand, so it resolves an ASCII label of
/// any length. A definition early and a reference late, the definition frozen by then: the
/// reference must rewind the stream to it.
#[test]
fn a_label_longer_than_the_scan_reaches_is_still_a_mention() {
    let label = "abcdefghij".repeat(500);
    let mut text = format!("[{label}]: https://long.example\n\n");
    for n in 0..40 {
        text.push_str(&format!("A paragraph between, number {n}, with a few words in it.\n\n"));
    }
    text.push_str(&format!("see [a][{label}] late\n\n"));
    for n in 0..40 {
        text.push_str(&format!("A paragraph after, number {n}, with a few words in it.\n\n"));
    }
    for chunk in [97, 211] {
        let mut policy = chat();
        let mut stream = StreamingDoc::new();
        let mut at = 0;
        while at < text.len() {
            at = floor(&text, (at + chunk).min(text.len()));
            let streamed = stream.update(&text[..at], &mut policy);
            assert_tiles(&stream, &text[..at]);
            assert_same(&streamed, &build(&text[..at], &mut chat()), &format!("chunk {chunk}, end {at}"));
        }
    }
}

/// A label that goes on over a block quote's lines: pulldown drops the `>` of each line from it
/// and reads "one two", so the stream, which compares the label as written ("one > two"), must
/// not tell the two apart. Written in a definition here: the definition arrives late, in a quote,
/// and a reference to it was frozen long before. (In a reference, `[x][one\n> two]`, and in a
/// shortcut reference, the build itself panics once the definition is there, in Ely's reader
/// ("a reference names a definition"): they can only be held to pulldown, which the next test
/// does, and not to a build.)
#[test]
fn a_definition_continued_on_a_quote_line_is_still_a_mention() {
    let text = "see [one two]\n\nmid\n\n> [one\n> two]: https://d.example\n\nlast\n\nend\n";
    for chunk in [1, 5] {
        assert_equivalent_at_every_cut(text, chunk);
    }
    let (cuts, _) = cuts_by_step(text, 3);
    assert!(!rewinds(&cuts).is_empty(), "the definition rewound nothing: {cuts:?}");
}

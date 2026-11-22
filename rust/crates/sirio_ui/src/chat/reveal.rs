//! A live reply drawn at Ely's reading pace, with a caret after it (spec §5.2).
//!
//! While a turn streams, the reply is not drawn from `document` (the tree of all that has arrived)
//! but from [`drawn_blocks`]: the frozen blocks of its [`StreamingDoc`] that are shown whole, then
//! the text revealed since, built as the tail of the reply. The caret is not part of that text: it
//! is a mark on the drawn tree (`MarkdownBlocks::caret`) that the view paints after the last text,
//! so it cannot change how the tail parses (`` ```▍ `` would not close a fence), and no text, copy
//! or selection holds it. Ely's own `next_shown` and `grapheme_end` set the pace (90 graphemes a
//! second, catching up within 350 ms, all at once under reduced motion) and the end of every step
//! on a grapheme.
//!
//! A reply is live while it is the last entry that is not an aside (`push_entry` ends it when
//! something else comes after it), and its drawn blocks are kept from frame to frame while nothing
//! they depend on moves. The text received, `document` included, is what Select All and the
//! control socket read, and what the file-link memo was filled from: the tail only looks files up
//! in it. A drag selection over the reply copies the drawn blocks' text.

use std::cell::Cell;
use std::time::Instant;

use ely_gpui_component::chat::stream::{grapheme_end, next_shown};
use ely_gpui_component::theme::ActiveTheme as _;
use gpui::{Context, Window};
use unicode_segmentation::UnicodeSegmentation;

use super::{Chat, Entry, with_lookup_policy};
use crate::markdown::{self, MarkdownBlocks, Policy, Segment};

/// The most text a frame builds of a live reply on top of its frozen blocks: the revealed bytes
/// past the end of the last frozen block that is shown. A reply that freezes as it arrives has a
/// tail of a few blocks. One that does not (no blank line, labels cited from its first block to
/// its last, the stream's wait after a fruitless look) has the whole of what is revealed in it,
/// and building that on every animation frame costs more as the reply grows: past this, the reply
/// stops revealing for the rest of its turn and is drawn from `document`, as it arrives, as every
/// reply was before the reveal.
///
/// The cost is the tail's, about 0.13 ms to a KiB (`drawn_blocks_costs`, release, on a machine
/// that was busy): a tail of 7 to 8 KiB builds in 0.8 ms on SHOWCASE and 1.0 to 1.1 ms on a compact
/// reply and on a reply of numbered references, where one of 40 KiB takes 4.7 to 4.8 ms.
pub(crate) const TAIL_BUDGET: usize = 8 * 1024;

/// A grapheme boundary of a reply, and how many graphemes lie before it. The text of a reply only
/// grows, and a boundary that has a character after it stays one when it does (what Unicode asks
/// to decide a boundary is the characters before it and the one after it), so a walk can go on
/// from here and not from the start of the reply.
#[derive(Clone, Copy, Default)]
struct Spot {
    bytes: usize,
    graphemes: usize,
}

impl Spot {
    /// This spot, if `text` still has it (a text that is not the one it was made for gives the
    /// start of `text`).
    fn in_(self, text: &str) -> Self {
        if self.bytes < text.len() && text.is_char_boundary(self.bytes) {
            self
        } else {
            Self::default()
        }
    }
}

/// How far the reveal of the one live reply has come.
#[derive(Clone)]
pub(crate) struct Reveal {
    /// Graphemes shown, the fraction carried (Ely's own state).
    shown: f32,
    /// When it last moved.
    at: Instant,
    /// The tail went past [`TAIL_BUDGET`]: the reply is drawn from `document` from now to the end
    /// of its turn, even if what it freezes later makes the tail short again.
    plain: bool,
    /// Where the last count of the reply's graphemes stopped, and where the last walk to the end
    /// of what is shown did: a frame goes on from them (see [`Spot`]), and costs what the reply
    /// grew by and the reveal moved by, not the length of the reply.
    counted: Cell<Spot>,
    walked: Cell<Spot>,
    /// What the last frame drew, and what it depended on: where the reveal stood, where the tail
    /// began, how many segments there were and how long the text was. A render that changes none
    /// of them (a key typed in the composer, a tool call updated) draws the same blocks, the same
    /// `Rc`s, which the culling memory compares; it is not a cache of segments, only of a frame.
    drawn: Option<(Drew, MarkdownBlocks)>,
}

/// What a drawn tail depends on: the byte the reveal stands at, the byte the tail starts at, how
/// many segments the stream held, and how long the text was.
type Drew = (usize, usize, usize, usize);

impl Reveal {
    /// A reveal that has shown nothing yet, started at `at`.
    pub(super) fn new(at: Instant) -> Self {
        Self {
            shown: 0.0,
            at,
            plain: false,
            counted: Cell::default(),
            walked: Cell::default(),
            drawn: None,
        }
    }

    /// Moves the reveal to `graphemes` shown.
    #[cfg(test)]
    pub(super) fn show(&mut self, graphemes: usize) {
        self.shown = graphemes as f32;
    }

    /// How many graphemes `text` holds.
    fn total(&self, text: &str) -> usize {
        let from = self.counted.get().in_(text);
        let (mut count, mut last) = (0, 0);
        for (at, _) in text[from.bytes..].grapheme_indices(true) {
            count += 1;
            last = at;
        }
        if count > 0 {
            // The last grapheme may grow with the text, so the walk goes on from its start.
            self.counted.set(Spot {
                bytes: from.bytes + last,
                graphemes: from.graphemes + count - 1,
            });
        }
        from.graphemes + count
    }

    /// Where the graphemes shown end in `text`, in bytes: [`grapheme_end`] for `shown` of them.
    fn end(&self, text: &str) -> usize {
        let shown = self.shown as usize;
        let mut from = self.walked.get().in_(text);
        if from.graphemes > shown {
            from = Spot::default();
        }
        let rest = &text[from.bytes..];
        let end = grapheme_end(rest, shown - from.graphemes);
        // At the end of the text the last grapheme may still grow: no spot is kept there.
        if end < rest.len() {
            self.walked.set(Spot {
                bytes: from.bytes + end,
                graphemes: shown,
            });
        }
        from.bytes + end
    }
}

/// The largest character boundary of `text` at or before `at` (the end of `text` past it).
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Where the tail of a reply revealed up to `revealed` starts: the end of the last frozen segment
/// that is shown whole (`0` when none is). The segments tile the text, in order.
fn tail_from(segments: &[Segment], revealed: usize) -> usize {
    frozen_by(segments, revealed)
        .last()
        .map_or(0, |segment| segment.range.end)
}

/// The segments that end at or before the byte `revealed`.
fn frozen_by(segments: &[Segment], revealed: usize) -> &[Segment] {
    &segments[..segments.partition_point(|segment| segment.range.end <= revealed)]
}

/// The blocks a live reply draws when `text` is shown up to the byte `revealed`: every frozen
/// segment that ends by it, then `text[from..revealed]` built as the tail (`from` is where the last
/// of those segments ends), all joined, and marked to be drawn with a caret. The caret is not in
/// the tail's source. `document` is not touched.
///
/// `revealed` is a character boundary of `text`, and `segments` are those of the reply's
/// [`StreamingDoc`] (none for a reply gone whole, which is drawn by [`plain_blocks`]). The result
/// is marked `reader_panicked` when the reader panicked on the tail, in which case it is plain
/// text; the caller does not draw it.
pub(crate) fn drawn_blocks(
    text: &str,
    segments: &[Segment],
    revealed: usize,
    policy: &mut Policy<'_>,
) -> MarkdownBlocks {
    let shown = frozen_by(segments, revealed);
    let from = tail_from(shown, revealed);
    let mut blocks = markdown::with_tail(shown, &text[from..revealed], policy);
    blocks.caret = true;
    blocks
}

/// What a reply gone whole (the nesting guard refused it) draws when `text` is shown up to the
/// byte `revealed`: the revealed bytes as one plain paragraph, as the document of such a reply is,
/// and a caret. The reader is not asked.
fn plain_blocks(text: &str, revealed: usize) -> MarkdownBlocks {
    let mut blocks = markdown::as_plain(&text[..revealed]);
    blocks.caret = true;
    blocks
}

impl Chat {
    /// Whether the reply at `entry` is being revealed: it is live (its stream is alive), the
    /// reader did not panic on its text (its document is then plain text, drawn as it is, with no
    /// caret and no pacing, and no frame asks the reader about it again) and its tail has not gone
    /// past the budget or panicked the reader.
    fn revealing(&self, entry: usize) -> bool {
        matches!(
            self.entries.get(entry),
            Some(Entry::Assistant {
                stream: Some(_),
                document,
                ..
            }) if !document.reader_panicked
        ) && self.reveal.as_ref().is_some_and(|reveal| !reveal.plain)
    }

    /// How many bytes of the reply at `entry` show: as far as its reveal has come while it is
    /// revealed, all of it when it is not live (stored, restored, ended) or went past the budget.
    pub(super) fn revealed_bytes(&self, entry: usize) -> usize {
        let Some(Entry::Assistant { text, .. }) = self.entries.get(entry) else {
            return 0;
        };
        match &self.reveal {
            Some(reveal) if self.revealing(entry) => reveal.end(text),
            _ => text.len(),
        }
    }

    /// Moves the reveal of the reply at `entry` on to `now`. Whether it is still behind: it has
    /// more graphemes to show than it shows.
    fn advance_reveal(&mut self, entry: usize, still: bool, now: Instant) -> bool {
        if !self.revealing(entry) {
            return false;
        }
        let (Some(Entry::Assistant { text, .. }), Some(reveal)) =
            (self.entries.get(entry), self.reveal.as_mut())
        else {
            return false;
        };
        let total = reveal.total(text);
        let elapsed = now.saturating_duration_since(reveal.at);
        reveal.shown = next_shown(reveal.shown, total, still, elapsed, true);
        reveal.at = now;
        (reveal.shown as usize) < total
    }

    /// What the reply at `entry` draws now, when the reveal draws it: `None` draws `document` (it
    /// is not live, or the reveal ended for good: its tail went past the budget, or the reader
    /// panicked on it). The tail only looks files up, and the blocks are the last frame's when
    /// nothing they depend on moved.
    fn drawn_reply(&mut self, entry: usize) -> Option<MarkdownBlocks> {
        if !self.revealing(entry) {
            return None;
        }
        let Some(Entry::Assistant {
            text,
            stream: Some(stream),
            ..
        }) = self.entries.get(entry)
        else {
            return None;
        };
        let reveal = self.reveal.as_mut()?;
        let end = reveal.end(text);
        // A reply gone whole has no frozen blocks to show: its document is one plain paragraph.
        let whole = stream.is_whole();
        let segments: &[Segment] = if whole { &[] } else { stream.segments() };
        let from = tail_from(segments, end);
        if end - from > TAIL_BUDGET {
            reveal.plain = true;
            reveal.drawn = None;
            return None;
        }
        let key = (end, from, segments.len(), text.len());
        if let Some((drew, blocks)) = &reveal.drawn
            && *drew == key
        {
            return Some(blocks.clone());
        }
        let blocks = match whole {
            true => plain_blocks(text, end),
            false => with_lookup_policy(&self.agent_cwd, &mut self.file_link_memo, |policy| {
                drawn_blocks(text, segments, end, policy)
            }),
        };
        if blocks.reader_panicked {
            reveal.plain = true;
            reveal.drawn = None;
            return None;
        }
        reveal.drawn = Some((key, blocks.clone()));
        Some(blocks)
    }

    /// What the row of the reply at `entry` draws on this frame: the reveal moves on to now, and
    /// asks for the next frame while it is behind. `None` draws `document`.
    pub(super) fn reveal_row(
        &mut self,
        entry: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<MarkdownBlocks> {
        if !self.revealing(entry) {
            return None;
        }
        let now = cx.background_executor().now();
        if self.advance_reveal(entry, cx.theme().reduced_motion, now) {
            window.request_animation_frame();
        }
        let drawn = self.drawn_reply(entry);
        if let Some(blocks) = &drawn {
            self.clamp_selection_to_drawn(entry, &blocks.text.text);
        }
        drawn
    }

    /// Where the live reply's row is in the transcript's offset space, and the text of the blocks
    /// last drawn for it: what a drag over it selects. `None` when no reply is being revealed.
    fn live_row(&self) -> Option<(std::ops::Range<usize>, &str)> {
        let entry = (self.entries.iter())
            .rposition(|entry| matches!(entry, Entry::Assistant { stream: Some(_), .. }))?;
        if !self.revealing(entry) {
            return None;
        }
        let (_, drawn) = self.reveal.as_ref()?.drawn.as_ref()?;
        Some((self.transcript_entry_ranges()[entry].clone(), drawn.text.text.as_str()))
    }

    /// What a selection copies when it ends inside the live reply: the text of the blocks last
    /// drawn, from where it starts to where the pointer let go. The selection's offsets are in the
    /// drawn text (the body hit-tests what is on screen), and the document's text differs from
    /// it wherever the reveal has not yet read what the document has: `[t][7]` is a link `t`
    /// there, and the bytes past the reveal are not on screen at all. A selection that reaches the
    /// end of the reply's document (Select All) is not a drag over the screen: `None`, the caller
    /// reads the document.
    pub(super) fn copy_from_live_reply(&self, selection: &std::ops::Range<usize>) -> Option<String> {
        let (row, drawn) = self.live_row()?;
        if selection.end <= row.start || selection.start >= row.end || selection.end >= row.end {
            return None;
        }
        let before = match selection.start < row.start {
            true => self.transcript_text().get(selection.start..row.start)?.to_owned(),
            false => String::new(),
        };
        let at = |offset: usize| floor_char_boundary(drawn, offset.saturating_sub(row.start));
        let (from, to) = (at(selection.start.max(row.start)), at(selection.end));
        Some(format!("{before}{}", &drawn[from..to.max(from)]))
    }

    /// Brings a selection that ends inside the live reply back inside what is drawn of it. The
    /// drawn text can get shorter between two frames (a closed mark drops its glyphs), and a
    /// selection made over the text as it was would end past the text as it is, or inside a glyph.
    /// A selection that reaches the end of the document's text (Select All) is left as it is.
    fn clamp_selection_to_drawn(&mut self, entry: usize, drawn: &str) {
        if self.transcript_selection.is_none() {
            return;
        }
        let row = self.transcript_entry_ranges()[entry].clone();
        let Some(selection) = self.transcript_selection.as_mut() else {
            return;
        };
        for end in [&mut selection.anchor, &mut selection.head] {
            if row.start <= *end && *end < row.end {
                *end = row.start + floor_char_boundary(drawn, *end - row.start);
            }
        }
    }

    /// Debug builds only, the control socket's two readouts of a reply row: `live` (its stream is
    /// alive, so the turn is still streaming it) and `shownBytes` (how many of its bytes show).
    #[cfg(debug_assertions)]
    pub(super) fn reveal_columns(
        &self,
        entry: usize,
        row: &mut std::collections::BTreeMap<String, String>,
    ) {
        if let Some(Entry::Assistant { stream, .. }) = self.entries.get(entry) {
            row.insert("live".into(), stream.is_some().to_string());
            row.insert("shownBytes".into(), self.revealed_bytes(entry).to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ely_gpui_component::documents::MarkdownBlock;
    use gpui::{TestAppContext, px};
    use unicode_segmentation::UnicodeSegmentation;

    use super::super::markdown_tests::{
        SHOWCASE, chat_with, last_painted_row, painted_caret, painted_text, revealed, tick,
    };
    use super::super::tests::refresh_frame;
    use super::super::{NoticeKind, TranscriptSelection};
    use super::*;
    use crate::markdown::{Policy, StreamingDoc, Surface};

    fn policy() -> Policy<'static> {
        Policy {
            surface: Surface::Chat,
            base: None,
            diagrams: None,
            file_links: None,
        }
    }

    /// A stream that `source` was fed to in 27-byte chunks, as a reply arrives.
    fn streamed(source: &str) -> StreamingDoc {
        let mut stream = StreamingDoc::new();
        let mut policy = policy();
        let mut at = 0;
        while at < source.len() {
            at = (at + 27).min(source.len());
            while !source.is_char_boundary(at) {
                at += 1;
            }
            stream.update(&source[..at], &mut policy);
        }
        stream
    }

    fn chunk(text: &str) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::AgentMessageChunk(text.into())
    }

    /// The frozen blocks of the segments up to the reveal, then what is revealed of the next one,
    /// with the caret on the tree and in none of its text: the segments are not parsed again, the
    /// tail is, without the caret, so that it parses as the reply does.
    #[test]
    fn drawn_blocks_shows_frozen_segments_then_the_caret_tail() {
        let stream = streamed(SHOWCASE);
        let segments = stream.segments();
        assert!(
            segments.len() >= 3,
            "SHOWCASE freezes at least three blocks: {}",
            segments.len()
        );
        let third = segments[2].range.clone();
        // In the middle of the third block, `## Second section`.
        let revealed = third.start + "## Second ".len();
        assert!(third.start < revealed && revealed < third.end);

        let drawn = drawn_blocks(SHOWCASE, segments, revealed, &mut policy());
        let frozen: Vec<&MarkdownBlock> = segments[..2].iter().flat_map(|s| s.nodes.iter()).collect();
        assert_eq!(
            drawn.nodes.len(),
            frozen.len() + 1,
            "the first two segments, then one tail block"
        );
        for (at, node) in frozen.iter().enumerate() {
            assert!(drawn.nodes[at] == **node, "frozen node {at} was not kept as it was");
        }
        let MarkdownBlock::Heading(level, tail) = &drawn.nodes[frozen.len()] else {
            panic!("the tail is the heading being revealed: {:?}", drawn.nodes[frozen.len()]);
        };
        assert_eq!(usize::from(*level), 2);
        assert_eq!(tail.text, "Second", "the revealed words, nothing after them");
        assert!(drawn.caret, "the tree is drawn with its caret");
        assert!(!drawn.text.text.contains('▍'), "no text holds the caret: {:?}", drawn.text.text);

        // Revealed to the end of the third block, it is frozen too, and nothing is built.
        let drawn = drawn_blocks(SHOWCASE, segments, third.end, &mut policy());
        assert_eq!(drawn.nodes.len(), frozen.len() + segments[2].nodes.len());
        assert!(drawn.caret);
    }

    /// A reply whose blocks do not depend on each other draws, at every byte it can be revealed to,
    /// what a build of that text makes: the same nodes, with the source offsets of a
    /// code block or a picture counted from the start of the reply and not of the tail, the same
    /// headings, kinds and text. Written with line ends of both kinds.
    #[test]
    fn a_drawn_reply_is_the_build_of_what_is_revealed() {
        const REPLY: &str = "# A title\n\nFirst paragraph with `code`, **bold** and a [link](https://example.com/a).\n\n\
            ## A section\n\n- one\n- two with `x`\n- three\n\n> a quote\n> over two lines\n\n\
            ```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n\
            ![a picture](pixel.png)\n\nClosing words after the picture, enough of them to be revealed in steps.\n";
        for reply in [REPLY.to_owned(), REPLY.replace('\n', "\r\n")] {
            let stream = streamed(&reply);
            let segments = stream.segments();
            assert!(segments.len() >= 7, "{} segments froze", segments.len());
            let mut checked = 0;
            for revealed in (1..=reply.len()).filter(|at| reply.is_char_boundary(*at)) {
                let drawn = drawn_blocks(&reply, segments, revealed, &mut policy());
                let full = markdown::build(&reply[..revealed], &mut policy());
                assert!(drawn.caret, "revealed {revealed}: no caret");
                let what = format!("revealed {revealed} of {:?}", &reply[..revealed]);
                assert_eq!(drawn.nodes.len(), full.nodes.len(), "{what}: top-level nodes");
                for (at, (got, want)) in drawn.nodes.iter().zip(full.nodes.iter()).enumerate() {
                    assert!(got == want, "{what}: node {at} differs\ndrawn: {got:#?}\nbuilt: {want:#?}");
                }
                assert_eq!(drawn.headings, full.headings, "{what}: headings");
                assert_eq!(drawn.kinds, full.kinds, "{what}: kinds");
                assert_eq!(drawn.text.text, full.text.text, "{what}: plain text");
                checked += 1;
            }
            assert!(checked > 300, "{checked} positions were checked");
        }
    }

    /// The reveal draws every prefix of a reply the stream passes through, so a text the reader
    /// panics on only at a prefix (a definition at the end of a list, a label broken over a quote
    /// line, a letter that folds to another, a `\|` in a table's label) is as much a crash as one
    /// it panics on whole. Every prefix of each shape draws through `drawn_blocks` without the
    /// reader panicking (the guard would catch it, and the tree would be plain text, marked), with
    /// the source offsets inside the text, whether the stream froze the reply's blocks as it came
    /// or the reply is drawn from no segments (a reply gone whole).
    #[test]
    fn every_prefix_of_a_reply_draws_without_the_reader_panicking() {
        const SHAPES: [&str; 6] = [
            "- see [1]\n\n[1]: https://x\n\nmore",
            "- see [1]\n\n[1]: https://x\n[2]: https://y\n\nmore",
            "> see [x][one\n> two]\n\n[one two]: u\n\nmore",
            "[\u{131}] and [i]\n\n[i]: a\n[\u{131}]: b\n\nmore",
            "| a | b |\n|---|---|\n| [x\\|y] | 2 |\n\n[x|y]: u\n\nmore",
            SHOWCASE,
        ];
        let mut walked = 0;
        for (shape, text) in SHAPES.iter().enumerate() {
            let stream = streamed(text);
            let variants: [(&str, &[Segment]); 2] =
                [("frozen", stream.segments()), ("no segments", &[])];
            for (name, segments) in variants {
                for revealed in (0..=text.len()).filter(|at| text.is_char_boundary(*at)) {
                    let drawn = drawn_blocks(text, segments, revealed, &mut policy());
                    assert!(
                        !drawn.reader_panicked,
                        "shape {shape} ({name}): the reader panicked on the first {revealed} bytes of {text:?}"
                    );
                    let offsets = crate::markdown::source_offsets(&drawn.nodes);
                    assert!(
                        offsets.iter().all(|at| *at <= text.len()),
                        "shape {shape} ({name}), {revealed} bytes: an offset out of {} bytes: {offsets:?}",
                        text.len()
                    );
                    walked += 1;
                }
            }
        }
        assert!(walked > 10_000, "{walked} prefixes were drawn");
    }

    /// The caret is never in the source the tail is parsed from: `` ```▍ `` does not close a fence,
    /// and the Copy of a code block would put the caret on the clipboard. A fence that the
    /// revealed text closes is a closed code block, as in a build of that text, and in a fence
    /// that is still open the code is the revealed code, and nothing else.
    #[test]
    fn the_caret_neither_changes_how_the_tail_parses_nor_enters_a_code_block() {
        let text = "```bash\nnpm i foo\n```\n\nafter";
        let closed = text.find("```\n\n").unwrap() + 3;
        let drawn = drawn_blocks(text, &[], closed, &mut policy());
        let built = markdown::build(&text[..closed], &mut policy());
        assert_eq!(drawn.nodes.len(), 1);
        assert!(drawn.nodes == built.nodes, "the fence is as closed as in a build: {:?}", drawn.nodes);
        assert!(drawn.caret);

        let open = "```bash\nnpm i foo";
        let drawn = drawn_blocks(open, &[], open.len(), &mut policy());
        let [MarkdownBlock::Code { text: code, .. }] = &drawn.nodes[..] else {
            panic!("an open fence is one code block: {:?}", drawn.nodes);
        };
        assert_eq!(code, "npm i foo");
        assert!(drawn.caret && !drawn.text.text.contains('▍'));
    }

    /// The view paints the caret itself, at the end of the last text of the body, and it is in no
    /// text: not in what the paint laid out, not in what a copy reads. A reply that is no longer
    /// live has none.
    #[gpui::test]
    async fn the_caret_is_painted_at_the_end_of_the_last_text(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("First paragraph.\n\nSecond one, which goes on"), cx);
        });
        tick(cx, Duration::from_secs(10));
        let caret = painted_caret(&chat, cx).expect("a live reply is drawn with its caret");
        let (row, text) = last_painted_row(&chat, cx);
        assert_eq!(text, "Second one, which goes on");
        // The row is as wide as the body; the caret is on its line, well past its start, where
        // the text ends.
        assert!(
            row.left() + px(100.0) < caret.x && caret.x < row.right() && caret.y == row.top(),
            "the caret at {caret:?}, the last text in {row:?}"
        );
        assert!(!painted_text(&chat, cx).contains('▍'));
        chat.read_with(&cx.cx, |chat, _| {
            assert!(!chat.transcript_text().contains('▍'));
        });
    }

    /// A reply is live while it is the last entry that is not a notice. A tool call, a thought, a
    /// user message pushed after it finishes it: no caret, its document drawn whole, and its stream
    /// ended. A notice does not (the next chunk still joins the reply, `streamed_entry`). A chunk
    /// after a tool call starts a new reply.
    #[gpui::test]
    async fn a_later_entry_that_is_not_a_notice_ends_the_live_reply(cx: &mut TestAppContext) {
        let reply = "A reply with words enough to take a while to reveal at the pace it has.";
        let entries: [(&str, Entry, bool); 4] = [
            ("a tool call", super::super::tests::test_tool_call("t1"), true),
            (
                "a thought",
                Entry::Thought {
                    text: "hm".into(),
                    open: Default::default(),
                    started: None,
                    duration_ms: None,
                },
                true,
            ),
            (
                "a user message",
                Entry::User {
                    text: "and another thing".into(),
                    at: None,
                },
                true,
            ),
            (
                "a compaction notice",
                Entry::Notice {
                    text: "compacted".into(),
                    kind: NoticeKind::Compaction,
                },
                false,
            ),
        ];
        for (what, entry, ends) in entries {
            let (chat, cx) = chat_with(cx, &[]);
            chat.update(cx, |chat, cx| {
                chat.streaming = true;
                chat.handle_event(chunk(reply), cx);
            });
            tick(cx, Duration::from_millis(100));
            assert!(painted_caret(&chat, cx).is_some(), "{what}: the reply is live and shows its caret");
            chat.update(cx, |chat, _| chat.push_entry(entry));
            refresh_frame(cx);
            chat.read_with(&cx.cx, |chat, _| {
                let Some(Entry::Assistant { stream, .. }) = chat.entries.first() else {
                    panic!("{what}: the reply is the first row");
                };
                assert_eq!(stream.is_none(), ends, "{what}: the reply's stream");
                let bytes = chat.revealed_bytes(0);
                assert_eq!(bytes == reply.len(), ends, "{what}: {bytes} bytes shown");
            });
            assert_eq!(painted_caret(&chat, cx).is_none(), ends, "{what}: the caret");
            if ends {
                chat.update(cx, |chat, cx| chat.handle_event(chunk("A second reply."), cx));
                chat.read_with(&cx.cx, |chat, _| {
                    let replies = (chat.entries.iter())
                        .filter(|entry| matches!(entry, Entry::Assistant { .. }))
                        .count();
                    assert_eq!(replies, 2, "{what}: a chunk after it opens a new reply");
                });
            }
        }
    }

    /// What a render draws of a live reply is memoised on what it depends on, so that a render that
    /// changes none of them (a key typed in the composer, a tool call updated) gets the same
    /// blocks, not a build of them: the same `Rc`, which the culling memory compares. A chunk, or
    /// the reveal moving, is a new one.
    #[gpui::test]
    async fn a_render_that_changes_nothing_reuses_the_drawn_blocks(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("A reply that goes on for a while, in a paragraph.\n\nAnd a second"), cx);
        });
        tick(cx, Duration::from_millis(100));
        let drawn = |cx: &mut gpui::VisualTestContext| {
            chat.update(cx, |chat, _| chat.drawn_reply(0)).expect("a live reply")
        };
        let first = drawn(cx);
        assert!(std::rc::Rc::ptr_eq(&first.nodes, &drawn(cx).nodes), "asked twice, built twice");
        refresh_frame(cx);
        assert!(std::rc::Rc::ptr_eq(&first.nodes, &drawn(cx).nodes), "a frame that moved nothing rebuilt it");

        chat.update(cx, |chat, cx| chat.handle_event(chunk(" paragraph, more"), cx));
        let after_a_chunk = drawn(cx);
        assert!(!std::rc::Rc::ptr_eq(&first.nodes, &after_a_chunk.nodes), "a chunk changed the reply");
        tick(cx, Duration::from_millis(30));
        assert!(!std::rc::Rc::ptr_eq(&after_a_chunk.nodes, &drawn(cx).nodes), "the reveal moved");
    }

    /// A drag over a live reply selects the text on screen, and copies it: the revealed text as it
    /// is drawn (an unresolved `[t][7]` is still `[t][7]`, though the document, which has the
    /// definition, reads `t`), up to the caret and not past it.
    #[gpui::test]
    async fn a_drag_over_a_live_reply_copies_what_is_on_the_screen(cx: &mut TestAppContext) {
        let reply = "Intro paragraph.\n\nSee [t][7] and then some more words.\n\n[7]: https://example.com/seven\n\nEnd.";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(reply), cx);
        });
        let through = reply.find("[t][7] and").unwrap() + "[t][7] and th".len();
        for _ in 0..40 {
            tick(cx, Duration::from_millis(10));
            if revealed(&chat, cx) >= through {
                break;
            }
        }
        let (row, on_screen) = last_painted_row(&chat, cx);
        let shown = revealed(&chat, cx);
        let tail_from = reply.find("See").unwrap();
        assert_eq!(on_screen, reply[tail_from..shown], "the revealed words of the tail, as written");
        assert!(on_screen.starts_with("See [t][7] and th"), "{on_screen:?}");
        let (document, drawn) = chat.update(cx, |chat, _| {
            let Some(Entry::Assistant { document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            let document = document.text.text.clone();
            (document, chat.drawn_reply(0).expect("live").text.text.clone())
        });
        assert!(document.contains("See t and"), "the document has the definition: {document:?}");
        assert_eq!(drawn, reply[..shown]);

        // From the start of the tail to the caret, which stands past the end of the text.
        let (from, to) = (
            gpui::point(row.left() + px(1.0), row.center().y),
            gpui::point(row.right() - px(10.0), row.center().y),
        );
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        let copied = chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text());
        assert_eq!(copied.as_deref(), Some(on_screen.as_str()), "the drag copied the screen's text");

        // From the top of the reply to the caret: all of it, the paragraph above included.
        let body = cx.debug_bounds("assistant-response-0").expect("the reply is drawn");
        let from = gpui::point(body.left() + px(1.0), body.top() + px(2.0));
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        let copied = chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text());
        assert_eq!(copied.as_deref(), Some(drawn.as_str()));
        assert!(!copied.unwrap().contains('▍'));
    }

    /// Select All reads the document, whole, as it did: the reveal does not hold the selection to
    /// what is on screen.
    #[gpui::test]
    async fn select_all_over_a_live_reply_copies_the_document(cx: &mut TestAppContext) {
        let reply = "Intro paragraph.\n\nSee [t][7] and then some more words.\n\n[7]: https://example.com/seven\n\nEnd.";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(reply), cx);
        });
        tick(cx, Duration::from_millis(60));
        assert!(revealed(&chat, cx) < reply.len());
        let (all, document) = chat.update(cx, |chat, _| {
            let len = chat.transcript_text().len();
            chat.transcript_selection = Some(TranscriptSelection { anchor: 0, head: len });
            let Some(Entry::Assistant { document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            (chat.selected_transcript_text(), document.text.text.clone())
        });
        assert_eq!(all, Some(document));
    }

    /// The tail only looks the files a reply names up in the memo the build of the received text
    /// filled, and never asks the disk: a token the memo has not seen (`src/li`, the start of one
    /// to come) is plain text, and adds no entry to the memo.
    #[gpui::test]
    async fn the_tail_looks_files_up_and_never_asks_the_disk(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("sirio-reveal-links-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).expect("a folder for the files the reply names");
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("a file to link to");
        let reply = "Open src/main.rs and then src/lib.rs please";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.agent_cwd = dir.clone();
            chat.handle_event(chunk(reply), cx);
        });
        let known = chat.read_with(&cx.cx, |chat, _| chat.file_link_memo.len());
        assert_eq!(known, 2, "the build of the whole text asked about the two files");
        for through in ["Open src/main.rs and then src/li", "Open src/main.rs and"] {
            // The reveal is moved to `through` by hand: no clock does it to the byte.
            let drawn = chat.update(cx, |chat, _| {
                chat.reveal.as_mut().expect("live").show(through.chars().count());
                chat.drawn_reply(0).expect("live")
            });
            assert_eq!(drawn.text.text, through);
            let links: usize = drawn
                .nodes
                .iter()
                .map(|node| match node {
                    MarkdownBlock::Paragraph(inline) => inline.links.len(),
                    _ => 0,
                })
                .sum();
            assert_eq!(links, 1, "{through:?}: the file the memo knew is a link, the rest is not");
            let now = chat.read_with(&cx.cx, |chat, _| chat.file_link_memo.len());
            assert_eq!(now, known, "{through:?}: the tail added to the memo, so it asked the disk");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A reply marked as plain text because the reader panicked on it stops revealing: it draws
    /// its text as the document has it, with no caret and no pacing, and no frame parses it again.
    #[gpui::test]
    async fn a_reply_marked_plain_is_not_parsed_on_any_frame(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, _| chat.streaming = true);
        let fault = crate::markdown::reader_fault::on();
        chat.update(cx, |chat, cx| {
            chat.handle_event(chunk("# A reply the reader panicked on, **in full**"), cx);
        });
        let asked = crate::markdown::reader_fault::strikes();
        assert!(asked >= 1, "the chunk asked the reader");
        for _ in 0..3 {
            tick(cx, Duration::from_millis(100));
        }
        assert_eq!(crate::markdown::reader_fault::strikes(), asked, "a frame asked the reader");
        drop(fault);
        chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant { text, document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            assert!(document.reader_panicked);
            assert_eq!(chat.revealed_bytes(0), text.len(), "a marked reply shows all of its text");
        });
        assert!(painted_caret(&chat, cx).is_none());
        assert!(painted_text(&chat, cx).contains("# A reply the reader panicked on, **in full**"));
    }

    /// A prefix the reader panics on, of a reply it reads whole: the frame that draws it gets plain
    /// text, marked. That ends the reveal for the turn, as the budget does: no caret, the document
    /// (which is fine) drawn whole, and no later frame asks the reader again.
    #[gpui::test]
    async fn a_prefix_the_reader_panics_on_ends_the_reveal_and_leaves_the_document(
        cx: &mut TestAppContext,
    ) {
        let reply = "A reply read fine in the whole, **bold** and all, which goes on a while.";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(reply), cx);
        });
        let fault = crate::markdown::reader_fault::on();
        tick(cx, Duration::from_millis(100));
        let asked = crate::markdown::reader_fault::strikes();
        assert!(asked >= 1, "the frame asked the reader, and it panicked");
        assert!(chat.update(cx, |chat, _| chat.drawn_reply(0)).is_none());
        assert_eq!(revealed(&chat, cx), reply.len());
        for _ in 0..3 {
            tick(cx, Duration::from_millis(100));
        }
        assert_eq!(crate::markdown::reader_fault::strikes(), asked, "a later frame asked the reader");
        drop(fault);
        chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant { document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            assert!(!document.reader_panicked, "the document of the whole text was read fine");
        });
        assert!(painted_caret(&chat, cx).is_none());
        assert!(painted_text(&chat, cx).contains("A reply read fine in the whole, bold and all"));
    }

    /// Ely drops a leading byte order mark from the text it reads, so the offsets of what it read
    /// run three bytes short of the text received. The reveal counts the text received: what it
    /// draws is that text up to the byte it has come to, and the caret.
    #[test]
    fn a_reply_that_starts_with_a_byte_order_mark_reveals_its_text_so_far() {
        let text = "\u{feff}First block\n\nSecond block\n\nThird block\n\nlast";
        let stream = streamed(text);
        let segments = stream.segments();
        assert!(segments.len() >= 2, "the first blocks freeze: {segments:?}");

        // Inside the first block, nothing is frozen yet: the tail is the reply from its start.
        let first = "\u{feff}Fir".len();
        let drawn = drawn_blocks(text, segments, first, &mut policy());
        assert_eq!(drawn.text.text, "Fir");

        // Inside the third, after two frozen segments that begin with the mark.
        let third = text.find("Third").unwrap() + "Third b".len();
        let drawn = drawn_blocks(text, segments, third, &mut policy());
        assert!(
            drawn.text.text.starts_with("First block") && drawn.text.text.ends_with("Third b"),
            "{:?}",
            drawn.text.text
        );
        assert!(!drawn.text.text.contains('\u{feff}'));
    }

    /// A family of three is one grapheme of eighteen bytes, and an accent a letter's second code
    /// point: the reveal is at a grapheme boundary of the text it has, at every step, and when a
    /// chunk ends inside one the cluster is shown whole once the rest of it has arrived.
    #[gpui::test]
    async fn the_reveal_never_cuts_a_grapheme(cx: &mut TestAppContext) {
        let family = "👩‍👩‍👧";
        let reply = format!("a{family}b e\u{301}x");
        // The first chunk ends after the first of the family's three people.
        let split = "a👩".len();
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply[..split]), cx);
        });
        let on_a_boundary = |cx: &gpui::VisualTestContext| {
            let (at, text) = chat.read_with(&cx.cx, |chat, _| {
                let Some(Entry::Assistant { text, .. }) = chat.entries.last() else {
                    panic!("the reply is the last row");
                };
                (chat.revealed_bytes(chat.entries.len() - 1), text.clone())
            });
            let boundaries: Vec<usize> = (text.grapheme_indices(true).map(|(at, _)| at))
                .chain([text.len()])
                .collect();
            assert!(
                boundaries.contains(&at),
                "{at} is inside a grapheme of {text:?}, whose boundaries are {boundaries:?}"
            );
            at
        };
        let mut seen = Vec::new();
        for _ in 0..2 {
            tick(cx, Duration::from_millis(12));
            seen.push(on_a_boundary(cx));
        }
        assert_eq!(seen, [1, split], "the first chunk: `a`, then `a` and the first person");

        // The rest of the family arrives. The reveal had shown two graphemes, and now the second
        // is the whole family: read before any frame, it is past the family, not inside it.
        chat.update(cx, |chat, cx| chat.handle_event(chunk(&reply[split..]), cx));
        assert_eq!(on_a_boundary(cx), "a".len() + family.len());
        for _ in 0..12 {
            tick(cx, Duration::from_millis(12));
            seen.push(on_a_boundary(cx));
        }
        assert_eq!(*seen.last().unwrap(), reply.len(), "the reveal reached the end");
    }

    /// A reply that is not live (restored, or ended) is drawn from `document`, whole. Ended: a
    /// reply that was live and part of the way through its reveal, with its caret drawn, until its
    /// turn ended.
    #[gpui::test]
    async fn not_live_draws_the_document(cx: &mut TestAppContext) {
        let reply = "A **restored** reply.\n\nWith two blocks.";
        {
            let (chat, cx) = chat_with(cx, &[reply]);
            refresh_frame(cx);
            chat.read_with(&cx.cx, |chat, _| assert_eq!(chat.revealed_bytes(0), reply.len()));
            let drawn = painted_text(&chat, cx);
            assert!(painted_caret(&chat, cx).is_none(), "{drawn:?}");
            assert!(drawn.contains("A restored reply."), "{drawn:?}");
        }

        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(reply), cx);
        });
        tick(cx, Duration::from_millis(100));
        assert!(revealed(&chat, cx) < reply.len(), "the live reply is part of the way through");
        assert!(painted_caret(&chat, cx).is_some());
        assert!(!painted_text(&chat, cx).contains("two blocks"));
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                sirio_chat::ChatEvent::TurnEnded {
                    stop_reason: "EndTurn".into(),
                },
                cx,
            );
        });
        refresh_frame(cx);
        assert_eq!(revealed(&chat, cx), reply.len());
        assert!(painted_caret(&chat, cx).is_none());
        assert!(painted_text(&chat, cx).contains("With two blocks."));
    }

    /// 12 KiB without a blank line never freezes, so the tail is the whole of what is revealed.
    /// Past the budget the reply stops revealing, and the rest of its turn it is drawn from
    /// `document`, as it arrives, even when the stream then freezes and the tail is short again.
    #[gpui::test]
    async fn a_tail_past_the_budget_stops_revealing_for_the_rest_of_the_turn(
        cx: &mut TestAppContext,
    ) {
        let mut reply = String::new();
        let mut line = 0;
        while reply.len() < 12 * 1024 {
            line += 1;
            reply.push_str(&format!(
                "Line {line} of a compact reply, with `code` and **emphasis** in it.\n"
            ));
        }
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            let mut at = 0;
            while at < reply.len() {
                let mut next = (at + 1500).min(reply.len());
                while !reply.is_char_boundary(next) {
                    next += 1;
                }
                chat.handle_event(chunk(&reply[at..next]), cx);
                at = next;
            }
        });
        let drawn = |cx: &mut gpui::VisualTestContext| {
            chat.update(cx, |chat, _| chat.drawn_reply(0))
        };

        // It reveals, within the budget, at first.
        let mut revealing = 0;
        for _ in 0..60 {
            tick(cx, Duration::from_millis(100));
            let shown = revealed(&chat, cx);
            if shown == reply.len() {
                break;
            }
            assert!(shown <= TAIL_BUDGET, "{shown} bytes shown past the budget");
            let blocks = drawn(cx).expect("a reply within the budget is drawn by the reveal");
            assert!(blocks.caret && !blocks.text.text.contains('▍'));
            revealing += 1;
        }
        assert!(revealing >= 2, "it revealed before it gave up: {revealing} frames");

        // Past it, it is the document, whole.
        assert_eq!(revealed(&chat, cx), reply.len());
        assert!(drawn(cx).is_none(), "a reply past the budget is drawn from `document`");
        assert!(painted_caret(&chat, cx).is_none());

        // Blocks arrive that let the stream freeze all of it: the tail is short again.
        let more: String = (1..=40)
            .map(|n| format!("\n\nA paragraph that comes later, number {n}, of some length."))
            .collect();
        chat.update(cx, |chat, cx| chat.handle_event(chunk(&more), cx));
        chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant {
                stream: Some(stream),
                ..
            }) = chat.entries.last()
            else {
                panic!("the reply still streams");
            };
            assert!(
                stream.cut() > TAIL_BUDGET,
                "the stream froze the compact block: cut {}",
                stream.cut()
            );
        });
        tick(cx, Duration::from_millis(100));
        assert!(drawn(cx).is_none(), "the reply came back to the reveal");
        assert_eq!(revealed(&chat, cx), reply.len() + more.len());
        assert!(painted_caret(&chat, cx).is_none());
    }

    /// A reply the nesting guard refused has one plain paragraph for its document, and the frozen
    /// blocks it held before are not what it shows: it reveals as plain text, the revealed bytes
    /// as they were written (a heading's `#` and a `**` stay what they are), from the start of the
    /// reply, with the caret. The budget is the same for it as for any other.
    #[gpui::test]
    async fn a_reply_gone_whole_reveals_as_plain_text(cx: &mut TestAppContext) {
        let lead = "# Heading\n\nFirst **bold** words\n\n";
        let deep = format!("{} too deep\n", ">".repeat(40));
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(lead), cx);
            chat.handle_event(chunk(&deep), cx);
        });
        chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant {
                stream: Some(stream),
                ..
            }) = chat.entries.last()
            else {
                panic!("the reply streams");
            };
            assert!(stream.is_whole(), "the nesting guard refused the deep quote");
            assert!(!stream.segments().is_empty(), "blocks were frozen before it");
        });
        // Some of it: a heading and a bold mark that are Markdown in any other reply.
        tick(cx, Duration::from_millis(100));
        let shown = revealed(&chat, cx);
        assert!(lead.len() / 2 < shown && shown < lead.len(), "{shown} bytes of {}", lead.len());
        let drawn = chat
            .update(cx, |chat, _| chat.drawn_reply(0))
            .expect("a short reply is drawn by the reveal");
        assert_eq!(drawn.nodes.len(), 1, "{:?}", drawn.nodes);
        assert!(drawn.headings.is_empty());
        assert_eq!(drawn.text.text, lead[..shown], "the revealed bytes as written");
        assert!(drawn.caret && !drawn.reader_panicked);

        // All of it: the document's own paragraph, up to the caret.
        tick(cx, Duration::from_secs(10));
        let drawn = chat
            .update(cx, |chat, _| chat.drawn_reply(0))
            .expect("a short reply is drawn by the reveal");
        assert_eq!(drawn.text.text, format!("{lead}{deep}"));
        assert_eq!(drawn.nodes.len(), 1);

        // 9 KiB more of it, and the budget applies as to any reply.
        chat.update(cx, |chat, cx| chat.handle_event(chunk(&"x ".repeat(4608)), cx));
        tick(cx, Duration::from_secs(10));
        assert!(chat.update(cx, |chat, _| chat.drawn_reply(0)).is_none());
        let length = chat.read_with(&cx.cx, |chat, _| match chat.entries.last() {
            Some(Entry::Assistant { text, .. }) => text.len(),
            _ => unreachable!(),
        });
        assert_eq!(revealed(&chat, cx), length);
    }

    /// Under reduced motion a live reply is revealed all at once, with the caret still after it.
    #[gpui::test]
    async fn reduced_motion_reveals_a_live_reply_at_once_with_its_caret(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        cx.update(|_, cx| {
            cx.set_reduce_motion(true);
            crate::ely::sync_theme_if_changed(cx);
        });
        let reply = "first block\n\nsecond block, which would take a while at 90 graphemes a second";
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(reply), cx);
        });
        tick(cx, Duration::from_millis(16));
        assert_eq!(revealed(&chat, cx), reply.len());
        assert!(painted_caret(&chat, cx).is_some());
        assert!(painted_text(&chat, cx).ends_with("a second"));
    }

    /// While the reveal is behind, the view asks for the next frame itself, and when it has
    /// caught up it stops asking: a reply that waits for its next chunk is not redrawn sixty
    /// times a second.
    #[gpui::test]
    async fn the_view_asks_for_frames_while_behind_and_stops_once_caught_up(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = chat_with(cx, &[]);
        let asked = |cx: &mut gpui::VisualTestContext| {
            refresh_frame(cx);
            cx.update(|window, cx| window.simulate_next_frame(cx))
        };
        let idle = asked(cx).min(asked(cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&"A line of a reply that is long enough to take a while. ".repeat(20)), cx);
        });
        let behind = asked(cx);
        assert!(behind > idle, "a reveal that is behind asked for no frame: {behind} against {idle}");

        cx.executor().advance_clock(Duration::from_secs(10));
        let _ = asked(cx);
        let caught_up = asked(cx);
        assert_eq!(revealed(&chat, cx), chat.read_with(&cx.cx, |chat, _| {
            match chat.entries.last() {
                Some(Entry::Assistant { text, .. }) => text.len(),
                _ => unreachable!(),
            }
        }));
        assert!(caught_up < behind, "a reveal that caught up still asks for frames: {caught_up} against {behind}");
    }

    /// A frame walks the reply's graphemes from where the last frame left off, not from its start.
    /// That must come to what a walk from the start comes to, wherever a chunk ends: inside a
    /// family of emoji, between a letter and its accent, between the `\r` and the `\n` of a line
    /// end, in a run of flags (pairs by their place in the run), inside a Hangul syllable; and
    /// with the reveal caught up (at the end of the text, where the next chunk may extend the last
    /// grapheme) or behind.
    #[test]
    fn walking_a_growing_reply_by_its_last_place_equals_a_walk_from_its_start() {
        let text = "a👩‍👩‍👧b e\u{301}x 🇸🇪🇸🇪🇸 line\r\nend \u{1100}\u{1161}\u{11a8} ক্ষ z 👍🏽 ok";
        for chunk in 1..=9usize {
            for caught_up in [false, true] {
                let mut reveal = Reveal::new(std::time::Instant::now());
                let mut at = 0;
                let mut shown = 0.0f32;
                while at < text.len() {
                    at = (at + chunk).min(text.len());
                    while !text.is_char_boundary(at) {
                        at += 1;
                    }
                    let so_far = &text[..at];
                    let total = reveal.total(so_far);
                    assert_eq!(
                        total,
                        so_far.graphemes(true).count(),
                        "chunk {chunk}: graphemes in {so_far:?}"
                    );
                    shown = if caught_up { total as f32 } else { (shown + 1.5).min(total as f32) };
                    reveal.shown = shown;
                    assert_eq!(
                        reveal.end(so_far),
                        ely_gpui_component::chat::stream::grapheme_end(so_far, shown as usize),
                        "chunk {chunk}, {shown} shown of {so_far:?}"
                    );
                }
            }
        }
    }

    /// What a frame costs to build, by hand and in release: `cargo nextest run -p sirio_ui
    /// --release --run-ignored only --no-capture drawn_blocks_costs`. Three replies of 40 KiB that
    /// the stream was fed in 200 chunks: SHOWCASE (footnotes cited from first block to last, so the
    /// stream holds the body), a compact reply (nothing freezes) and shape A of the label-heavy
    /// replies (`[1]` .. `[20]`, defined at the end). For each, the frames of the last tenth of the
    /// reply, one every seven bytes, and the frames whose tail is the largest the budget lets
    /// through (between 7 KiB and the budget), built with the chat's own Policy and a file-link
    /// memo that lives from frame to frame, as a reply's does.
    #[test]
    #[ignore = "a measurement, run by hand"]
    fn drawn_blocks_costs() {
        use super::super::file_links::FileLinkMemo;
        use super::super::markdown_tests::{compact_source, numbered_references_source};
        use super::super::with_lookup_policy;
        use std::time::Instant;

        let mut showcase = String::new();
        while showcase.len() < 40 * 1024 {
            showcase.push_str(SHOWCASE);
            showcase.push_str("\n\n");
        }
        let cwd = std::env::temp_dir();
        let stats = |mut times: Vec<Duration>| {
            times.sort();
            let at = |share: usize| times[(times.len() * share / 100).min(times.len() - 1)];
            format!(
                "median {:?}, p95 {:?}, max {:?} over {} frames",
                at(50),
                at(95),
                times[times.len() - 1],
                times.len()
            )
        };
        for (name, source) in [
            ("SHOWCASE", showcase),
            ("compact", compact_source()),
            ("label shape A", numbered_references_source(false)),
        ] {
            let mut stream = StreamingDoc::new();
            let mut plain = policy();
            for step in 1..=200usize {
                let mut cut = source.len() * step / 200;
                while !source.is_char_boundary(cut) {
                    cut += 1;
                }
                stream.update(&source[..cut], &mut plain);
            }
            let segments = stream.segments();
            let positions = |from: usize| {
                (from..=source.len())
                    .step_by(7)
                    .filter(|at| source.is_char_boundary(*at))
                    .collect::<Vec<_>>()
            };
            let frame = |revealed: usize, memo: &mut FileLinkMemo| {
                let started = Instant::now();
                let drawn = with_lookup_policy(&cwd, memo, |policy| {
                    drawn_blocks(&source, segments, revealed, policy)
                });
                std::hint::black_box(&drawn);
                started.elapsed()
            };
            eprintln!(
                "BENCH drawn_blocks {name} ({} bytes): stream cut {}, {} segments, whole {}",
                source.len(),
                stream.cut(),
                segments.len(),
                stream.is_whole()
            );

            let mut memo = FileLinkMemo::default();
            let last_tenth = positions(source.len() * 9 / 10);
            let tails: Vec<usize> = last_tenth
                .iter()
                .map(|at| at - tail_from(segments, *at))
                .collect();
            let over = tails.iter().filter(|tail| **tail > TAIL_BUDGET).count();
            let times: Vec<Duration> = last_tenth.iter().map(|at| frame(*at, &mut memo)).collect();
            eprintln!(
                "BENCH drawn_blocks {name}: last tenth, tails {}..{} bytes, {over} of {} over the budget: {}",
                tails.iter().min().unwrap(),
                tails.iter().max().unwrap(),
                tails.len(),
                stats(times)
            );

            // Every frame of the reveal the budget lets through, from the first byte to the last.
            let mut memo = FileLinkMemo::default();
            let within: Vec<usize> = positions(0)
                .into_iter()
                .filter(|at| at - tail_from(segments, *at) <= TAIL_BUDGET)
                .collect();
            let all = positions(0).len();
            let times: Vec<Duration> = within.iter().map(|at| frame(*at, &mut memo)).collect();
            eprintln!(
                "BENCH drawn_blocks {name}: every frame within the budget ({} of {all} positions): {}",
                within.len(),
                stats(times)
            );

            let mut memo = FileLinkMemo::default();
            let edge: Vec<usize> = positions(0)
                .into_iter()
                .filter(|at| {
                    let tail = at - tail_from(segments, *at);
                    (TAIL_BUDGET - 1024..=TAIL_BUDGET).contains(&tail)
                })
                .collect();
            if edge.is_empty() {
                eprintln!("BENCH drawn_blocks {name}: no frame has a tail of 7 KiB to the budget");
            } else {
                let times: Vec<Duration> = edge.iter().map(|at| frame(*at, &mut memo)).collect();
                eprintln!(
                    "BENCH drawn_blocks {name}: tails of 7 KiB to the budget: {}",
                    stats(times)
                );
            }
        }

        // What else a frame does on a reply of this length.
        let text = &showcase_text();
        let timed = |what: &str, work: &dyn Fn()| {
            let mut times: Vec<Duration> = (0..200)
                .map(|_| {
                    let started = Instant::now();
                    work();
                    started.elapsed()
                })
                .collect();
            times.sort();
            eprintln!("BENCH frame {what}: median {:?}, max {:?}", times[100], times[199]);
        };
        timed("grapheme count of 40 KiB", &|| {
            std::hint::black_box(text.graphemes(true).count());
        });
        timed("grapheme_end near the end of 40 KiB", &|| {
            std::hint::black_box(ely_gpui_component::chat::stream::grapheme_end(
                text,
                text.len() - 100,
            ));
        });
        let mut before = text.len() - 200;
        while !text.is_char_boundary(before) {
            before -= 1;
        }
        let mut warm = Reveal::new(std::time::Instant::now());
        warm.shown = warm.total(&text[..before]) as f32 - 5.0;
        warm.end(&text[..before]);
        timed("walk on from the last frame (200 bytes more text, 3 graphemes more shown)", &|| {
            let mut frame = warm.clone();
            std::hint::black_box(frame.total(text));
            frame.shown += 3.0;
            std::hint::black_box(frame.end(text));
        });
        let drawn = {
            let stream = streamed(text);
            let mut at = text.len() - 100;
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            drawn_blocks(text, stream.segments(), at, &mut policy())
        };
        timed("a render that moved nothing: the memoised blocks cloned", &|| {
            std::hint::black_box(drawn.clone());
        });
        timed("clone of the 40 KiB text", &|| {
            std::hint::black_box(text.clone());
        });
        let stream = streamed(text);
        timed("clone of its StreamingDoc", &|| {
            std::hint::black_box(stream.clone());
        });
    }

    fn showcase_text() -> String {
        let mut showcase = String::new();
        while showcase.len() < 40 * 1024 {
            showcase.push_str(SHOWCASE);
            showcase.push_str("\n\n");
        }
        showcase
    }
}

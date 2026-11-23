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
use ely_gpui_component::documents::MarkdownBlock;
use ely_gpui_component::theme::ActiveTheme as _;
use gpui::{Context, Window};
use unicode_segmentation::UnicodeSegmentation;

use super::{Chat, Entry, TranscriptSelection, with_lookup_policy};
use crate::markdown::{self, MarkdownBlocks, Policy, Segment};

/// The most text a frame builds of a live reply on top of its frozen blocks: the revealed bytes
/// past the end of the last frozen block that is shown. A reply that freezes as it arrives has a
/// tail of a few blocks. One that does not (no blank line, labels cited from its first block to
/// its last, the stream's wait after a fruitless look) has the whole of what is revealed in it,
/// and building that on every animation frame costs more as the reply grows: past this, the reply
/// stops revealing for the rest of its turn and is drawn from `document`, as it arrives, as every
/// reply was before the reveal.
///
/// The cost is the tail's, about 0.13 ms to a KiB (`drawn_blocks_costs`, release): a tail of the last
/// KiB under 7 KiB builds in 0.78 ms on SHOWCASE, 0.86 ms on a compact reply and 0.92 ms on a
/// reply of numbered references (medians; the figures at 8 KiB were 0.80, 1.08 and 1.19 ms, over the
/// line of about 1 ms that set this), where a tail of 40 KiB takes about 5 ms.
pub(crate) const TAIL_BUDGET: usize = 7 * 1024;

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

/// Whether `document` is the one plain paragraph that a source the nesting guard refused builds
/// to: the source, as written.
fn is_plain_paragraph(document: &MarkdownBlocks, text: &str) -> bool {
    matches!(&document.nodes[..], [MarkdownBlock::Paragraph(inline)] if inline.text == text)
}

/// The largest character boundary of `text` at or before `at` (the end of `text` past it).
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The live reply's entry, its row in the transcript's offset space, and the text last drawn for
/// it.
pub(super) struct LiveRow {
    entry: usize,
    row: std::ops::Range<usize>,
    drawn: String,
}

impl LiveRow {
    /// The live reply's entry.
    pub(super) fn entry(&self) -> usize {
        self.entry
    }

    /// `offset` of an endpoint placed in the live row, brought inside the drawn text, onto a
    /// character boundary of it. However the drawn text changes (it can shrink under a drag as
    /// the reveal reads what the document has already resolved), the endpoint stays in the row.
    pub(super) fn clamp(&self, offset: usize) -> usize {
        self.row.start + floor_char_boundary(&self.drawn, offset.saturating_sub(self.row.start))
    }
}

/// One end of a selection: its offset, and whether it was placed in the live reply (the offset
/// is then in the text drawn for it).
#[derive(Clone, Copy)]
struct End {
    at: usize,
    live: bool,
}

impl TranscriptSelection {
    /// The two ends, the first in the transcript first, while the reply at `live` is live. Ends in
    /// two entries are in the order of their entries, and ends in one entry in the order of their
    /// offsets: an offset in the live reply is a place in its drawn text, which can be longer than
    /// the document's row, and the offsets of the rows below it are placed by the document's.
    fn ends(&self, live: usize) -> [End; 2] {
        let anchor = End {
            at: self.anchor,
            live: self.anchor_entry == Some(live),
        };
        let head = End {
            at: self.head,
            live: self.head_entry == Some(live),
        };
        let anchor_first = match (self.anchor_entry, self.head_entry) {
            (Some(anchor), Some(head)) if anchor != head => anchor < head,
            _ => self.anchor <= self.head,
        };
        match anchor_first {
            true => [anchor, head],
            false => [head, anchor],
        }
    }
}

/// The part of the transcript's `span` that lies in a body of `len` bytes starting at `start`,
/// as the body's own bytes.
fn local(span: std::ops::Range<usize>, start: usize, len: usize) -> Option<std::ops::Range<usize>> {
    let from = span.start.max(start);
    let to = span.end.min(start + len);
    (from < to).then(|| from - start..to - start)
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
    pub(super) fn revealing(&self, entry: usize) -> bool {
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
            document,
            stream: Some(stream),
        }) = self.entries.get(entry)
        else {
            return None;
        };
        let reveal = self.reveal.as_mut()?;
        let end = reveal.end(text);
        // A reply whose stream went whole has no frozen blocks to show. If the guard refused it
        // its document is one plain paragraph, and it reveals as plain text. If it went whole for
        // another reason (a text the guard declined, the math and HTML check) its document is
        // formatted, and the raw source of what is revealed would be a worse draw than the
        // document: it stops revealing, as a reply over the budget does.
        let whole = stream.is_whole();
        if whole && !is_plain_paragraph(document, text) {
            self.stop_revealing(entry);
            return None;
        }
        let segments: &[Segment] = if whole { &[] } else { stream.segments() };
        let from = tail_from(segments, end);
        if end - from > TAIL_BUDGET {
            self.stop_revealing(entry);
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
            self.stop_revealing(entry);
            return None;
        }
        reveal.drawn = Some((key, blocks.clone()));
        Some(blocks)
    }

    /// Ends the reveal of the reply at `entry` for the rest of its turn: it is drawn from `document`
    /// from now on. A drag with an end in it goes, as when the reply ends.
    fn stop_revealing(&mut self, entry: usize) {
        if let Some(reveal) = self.reveal.as_mut() {
            reveal.plain = true;
            reveal.drawn = None;
        }
        self.drop_selection_in(entry);
    }

    /// Drops a selection with an end placed in the reply at `entry`, when the reply stops being
    /// drawn by the reveal (it ends, or it stops revealing in mid-turn): the end is a place in the
    /// text drawn for the reply, which the document it is drawn from now does not measure, and a
    /// selection that silently changed what it covers would be worse than none.
    pub(super) fn drop_selection_in(&mut self, entry: usize) {
        if self.transcript_selection.as_ref().is_some_and(|selection| {
            selection.anchor_entry == Some(entry) || selection.head_entry == Some(entry)
        }) {
            self.transcript_selection = None;
        }
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
        // The reveal's per-frame work: how far it has come, and the tail built for it. Only a
        // reply that is being revealed makes a span, so a frame's cost against the tail budget
        // (`TAIL_BUDGET`) reads off a trace.
        let _perf = sirio_perf::span("Chat.reveal_row", entry as u64);
        let now = cx.background_executor().now();
        if self.advance_reveal(entry, cx.theme().reduced_motion, now) {
            window.request_animation_frame();
        }
        let drawn = self.drawn_reply(entry);
        if drawn.is_some() {
            self.clamp_selection_to_drawn();
        }
        drawn
    }

    /// The entry of the reply being revealed, if one is. It is the last entry that is not an aside
    /// (`push_entry`), so the walk from the end is short.
    pub(super) fn live_entry(&self) -> Option<usize> {
        self.reveal.as_ref()?;
        let entry = (self.entries.iter())
            .rposition(|entry| matches!(entry, Entry::Assistant { stream: Some(_), .. }))?;
        self.revealing(entry).then_some(entry)
    }

    /// The live reply's row in the transcript's offset space, and the text of the blocks last
    /// drawn for it: what a drag over it selects. `None` when no reply is being revealed.
    pub(super) fn live_row(&self) -> Option<LiveRow> {
        let entry = self.live_entry()?;
        let (_, drawn) = self.reveal.as_ref()?.drawn.as_ref()?;
        Some(LiveRow {
            entry,
            row: self.transcript_entry_ranges()[entry].clone(),
            drawn: drawn.text.text.clone(),
        })
    }

    /// The part of the selection a body of `len` bytes that starts at `start` in the transcript's
    /// offset space holds, as the body's own bytes: what it highlights. Where an end is in the live
    /// reply, the ends are taken in their entries' order: the reply's body is lit from an end in
    /// it to the end of its drawn text (the selection goes on below) or from its start (it came
    /// from above), and a row above or below it is lit as if that end stood at the reply's edge.
    /// This is what [`Chat::copy_from_live_reply`] copies.
    pub(super) fn selection_in_body(&self, start: usize, len: usize) -> Option<std::ops::Range<usize>> {
        let selection = self.transcript_selection.as_ref()?;
        let live = self.live_entry().filter(|live| {
            !selection.whole
                && (selection.anchor_entry == Some(*live) || selection.head_entry == Some(*live))
        });
        let Some(live) = live else {
            return local(selection.range(), start, len);
        };
        let row = self.transcript_entry_ranges()[live].clone();
        let [first, last] = selection.ends(live);
        let span = match start == row.start {
            // The reply's own body, in its drawn text's offsets.
            true => {
                let from = if first.live { first.at } else { start };
                let to = if last.live { last.at } else { start + len };
                from..to
            }
            // A row above or below it, in the document's.
            false => {
                let from = if first.live { row.end } else { first.at };
                let to = if last.live { row.start } else { last.at };
                from..to
            }
        };
        local(span, start, len)
    }

    /// What a selection copies when one of its ends was placed in the live reply: the text of the
    /// blocks last drawn, from where it starts to where the pointer let go, up to the drawn text's
    /// length, whatever the drawn text has become since. A drag's offsets are in the drawn text (the
    /// body hit-tests what is on screen), and the document's text differs from it wherever the
    /// reveal has not yet read what the document has: `[t][7]` is a link `t` there, and the bytes
    /// past the reveal are not on screen at all. An end placed above the reply is a place in the
    /// document's text, which is read up to the reply; one placed below it, from the end of the
    /// reply's document row. The ends are taken in their entries' order ([`TranscriptSelection`]),
    /// so the copy is what [`Chat::selection_in_body`] lights, row by row, with the separators
    /// between the rows. Select All is not a drag over the screen: it is of the transcript's
    /// text, and `None` says to read that, as it does for a selection with no end in the reply (a
    /// notice below it, whose offsets can fall inside the drawn text's).
    pub(super) fn copy_from_live_reply(&self, selection: &TranscriptSelection) -> Option<String> {
        if selection.whole {
            return None;
        }
        let live = self.live_row()?;
        let [first, last] = selection.ends(live.entry);
        if !first.live && !last.live {
            return None;
        }
        let document = self.transcript_text();
        let mut copied = String::new();
        if !first.live {
            copied.push_str(document.get(first.at..live.row.start)?);
        }
        let from = match first.live {
            true => live.clamp(first.at),
            false => live.row.start,
        };
        let to = match last.live {
            true => live.clamp(last.at),
            false => live.row.start + live.drawn.len(),
        };
        copied.push_str(&live.drawn[from - live.row.start..to.max(from) - live.row.start]);
        if !last.live {
            copied.push_str(document.get(live.row.end..last.at)?);
        }
        Some(copied)
    }

    /// Brings the ends of a drag that were placed in the live reply back inside what is drawn of
    /// it. The drawn text can get shorter between two frames (a closed mark drops its glyphs, a
    /// reference resolves), and a selection made over the text as it was would end past the text as
    /// it is, or inside a glyph. Select All and the ends placed in other rows are left as they are.
    fn clamp_selection_to_drawn(&mut self) {
        if self.transcript_selection.as_ref().is_none_or(|selection| selection.whole) {
            return;
        }
        let Some(live) = self.live_row() else {
            return;
        };
        let Some(selection) = self.transcript_selection.as_mut() else {
            return;
        };
        if selection.anchor_entry == Some(live.entry) {
            selection.anchor = live.clamp(selection.anchor);
        }
        if selection.head_entry == Some(live.entry) {
            selection.head = live.clamp(selection.head);
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
    use super::super::tests::{focus_composer, refresh_frame};
    use super::super::NoticeKind;
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
    /// or the reply is drawn from no segments (a reply gone whole). The five small shapes are walked
    /// at every character boundary; SHOWCASE, which is 5 KiB, at every seventh and at the line ends.
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
                let steps = match shape == SHAPES.len() - 1 {
                    true => 7,
                    false => 1,
                };
                let mut stops: Vec<usize> = (0..=text.len())
                    .step_by(steps)
                    .chain(text.match_indices('\n').flat_map(|(at, _)| [at, at + 1]))
                    .filter(|at| text.is_char_boundary(*at))
                    .collect();
                stops.sort_unstable();
                stops.dedup();
                for revealed in stops {
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
        assert!(walked > 2_000, "{walked} prefixes were drawn");
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
        let entries: [(&str, Entry, bool); 6] = [
            ("a tool call", super::super::tests::test_tool_call("t1"), true),
            (
                "a permission request",
                Entry::Permission {
                    request_id: 7,
                    title: "Edit a file".into(),
                    prompt: String::new(),
                    options: super::super::question_dock::answer_options(&["Allow", "Deny"]),
                    text_input: None,
                    is_question: false,
                    resolved: None,
                    expired: false,
                    dismissed: false,
                    withdrawn: false,
                    detail: None,
                },
                true,
            ),
            (
                "an error",
                Entry::Error {
                    message: "the agent's connection dropped".into(),
                    retryable: false,
                    kind: super::super::ErrorKind::Connection,
                },
                true,
            ),
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
    /// what is on screen, not when a chunk arrives either (the selection is of what the document
    /// held when it was made).
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
        chat.update(cx, |chat, cx| chat.select_all_transcript(cx));
        let copied = |cx: &mut gpui::VisualTestContext| {
            chat.update(cx, |chat, _| {
                let Some(Entry::Assistant { document, .. }) = chat.entries.first() else {
                    panic!("a reply")
                };
                (chat.selected_transcript_text(), document.text.text.clone())
            })
        };
        let (all, document) = copied(cx);
        assert_eq!(all, Some(document));
        // What was selected stays selected, whatever the reveal is at.
        chat.update(cx, |chat, cx| chat.handle_event(chunk(" A last word."), cx));
        tick(cx, Duration::from_millis(30));
        let (again, _) = copied(cx);
        assert_eq!(again, all, "a frame or a chunk narrowed Select All");
    }

    /// A drag over a live reply is a drag even where the document has already resolved what the
    /// reveal still shows as written: `See [a][1] and [b][2].` is 22 bytes on the screen and 12 in
    /// the document (`See a and b.`) until the definitions are revealed. The drag from the start
    /// of the line to its end ends at the 22nd byte, past the end of the document's row, and it is
    /// still a drag: it copies the line as drawn, whole, and it is not pulled in to the document's
    /// length by the next chunk. (After a Select All, which a press ends.)
    #[gpui::test]
    async fn a_drag_over_text_the_document_has_shortened_is_not_select_all(cx: &mut TestAppContext) {
        let line = "See [a][1] and [b][2].";
        let reply = format!("{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n");
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
        });
        // The reveal is at the end of the line, short of the definitions.
        chat.update(cx, |chat, _| chat.reveal.as_mut().expect("live").show(line.len()));
        refresh_frame(cx);
        let document = chat.update(cx, |chat, _| {
            let Some(Entry::Assistant { document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            document.text.text.clone()
        });
        assert_eq!(document, "See a and b.", "the document has resolved both labels");
        let (row, on_screen) = last_painted_row(&chat, cx);
        assert_eq!(on_screen, line, "the screen shows them as written: 22 bytes against 12");

        chat.update(cx, |chat, cx| chat.select_all_transcript(cx));
        let (from, to) = (
            gpui::point(row.left() + px(1.0), row.center().y),
            gpui::point(row.right() - px(10.0), row.center().y),
        );
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        let copied = |cx: &mut gpui::VisualTestContext| {
            chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text())
        };
        assert_eq!(copied(cx).as_deref(), Some(line), "the drag to the end of the line copied the line");

        // A chunk comes, and a frame: the selection stays where the drag left it.
        chat.update(cx, |chat, cx| chat.handle_event(chunk("[3]: https://example.com/3\n"), cx));
        tick(cx, Duration::from_millis(1));
        assert_eq!(copied(cx).as_deref(), Some(line), "a chunk pulled the selection in to the document's length");
    }

    /// A drag's end that was placed in the live reply stays in it, whatever the drawn text becomes
    /// under it. The line `See [a][1] and [b][2].` is 22 bytes drawn; as the reveal runs through the
    /// definitions that follow, the drawn text shrinks to 17 and then to 12 bytes, the document's
    /// length, and a notice sits below. On every frame the copy is exactly what the drawn text
    /// holds up to the end of the drag, which each frame clamps to the drawn length: never the
    /// document's text or the notice's, and not nothing. Real frames, the reveal at its own pace.
    #[gpui::test]
    async fn a_drag_end_in_the_live_reply_stays_there_as_the_drawn_text_shrinks_under_it(
        cx: &mut TestAppContext,
    ) {
        let line = "See [a][1] and [b][2].";
        let reply = format!("{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n");
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
            chat.push_entry(Entry::Notice {
                text: "a background task finished, and said so at some length".into(),
                kind: NoticeKind::BackgroundTask,
            });
        });
        let drawn_text = |cx: &mut gpui::VisualTestContext| {
            chat.update(cx, |chat, _| chat.drawn_reply(0).map(|blocks| blocks.text.text.clone()))
        };
        let copied = |cx: &mut gpui::VisualTestContext| {
            chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text())
        };
        for _ in 0..60 {
            tick(cx, Duration::from_millis(10));
            if revealed(&chat, cx) >= line.len() {
                break;
            }
        }
        assert_eq!(drawn_text(cx).as_deref(), Some(line), "the reveal stands at the end of the line");
        let (row, on_screen) = last_painted_row(&chat, cx);
        assert_eq!(on_screen, line);

        let (from, to) = (
            gpui::point(row.left() + px(1.0), row.center().y),
            gpui::point(row.right() - px(10.0), row.center().y),
        );
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(copied(cx).as_deref(), Some(line));

        // The reveal runs on through the definitions, and the drawn text under the drag shrinks.
        // The end of the drag is clamped to the drawn text on every frame, so it stays at the
        // shortest the text has been (ASCII here: a byte is a character).
        let mut end = line.len();
        for _ in 0..80 {
            tick(cx, Duration::from_millis(10));
            let drawn = drawn_text(cx).expect("the reply is revealed");
            end = end.min(drawn.len());
            assert_eq!(
                copied(cx).as_deref(),
                Some(&drawn[..end]),
                "the copy is what the drawn text {drawn:?} holds up to the drag's end, {end}"
            );
            if revealed(&chat, cx) == reply.len() {
                break;
            }
        }
        assert!(end < line.len(), "the drawn text never shrank under the drag: {end} bytes");
        assert_eq!(drawn_text(cx).as_deref(), Some("See a and b."));
        assert_eq!(copied(cx).as_deref(), Some("See a and b."));
    }

    /// A selection made in a notice below the live reply is the notice's, even where the reply's
    /// drawn text is longer than its document and the notice's offsets fall inside the drawn text's:
    /// `done` is at 14..18, and the reply's 22 drawn bytes cover them.
    #[gpui::test]
    async fn a_selection_inside_a_notice_below_a_live_reply_copies_the_notice(
        cx: &mut TestAppContext,
    ) {
        let line = "See [a][1] and [b][2].";
        let reply = format!("{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n");
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
            chat.push_entry(Entry::Notice {
                text: "done".into(),
                kind: NoticeKind::BackgroundTask,
            });
            chat.reveal.as_mut().expect("live").show(line.len());
        });
        refresh_frame(cx);
        let drawn = chat.update(cx, |chat, _| chat.drawn_reply(0).expect("live").text.text.clone());
        assert_eq!(drawn, line, "the drawn text is longer than the document's row");

        let notice = cx.debug_bounds("notice-task-text").expect("the notice is drawn");
        let (left, right) = (
            gpui::point(notice.left() + px(1.0), notice.center().y),
            gpui::point(notice.left() + px(22.0), notice.center().y),
        );
        // Left to right, then right to left: the press and the end are both in the notice.
        for (what, from, to) in [("left to right", left, right), ("right to left", right, left)] {
            drag(cx, from, to);
            let copied = chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text());
            let copied = copied.unwrap_or_else(|| panic!("{what}: the drag selected nothing"));
            assert!(
                !copied.is_empty() && "done".contains(&copied),
                "{what}: a drag over the notice copied {copied:?}"
            );
        }
    }

    /// A real drag with the left button, from `from` to `to`.
    fn drag(cx: &mut gpui::VisualTestContext, from: gpui::Point<gpui::Pixels>, to: gpui::Point<gpui::Pixels>) {
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
    }

    /// What Ctrl+C writes now: `None` when it writes nothing (the clipboard keeps what was in it).
    fn copy_keys(cx: &mut gpui::VisualTestContext) -> Option<String> {
        let sentinel = "the clipboard before Ctrl+C";
        cx.cx.write_to_clipboard(gpui::ClipboardItem::new_string(sentinel.into()));
        cx.simulate_keystrokes("ctrl-c");
        cx.run_until_parked();
        let text = cx.cx.read_from_clipboard().and_then(|item| item.text());
        text.filter(|text| text != sentinel)
    }

    /// A live reply `See [a][1] and [b][2].` with its definitions below, revealed to the end of the
    /// line: 22 bytes drawn, 12 in the document (`See a and b.`). The box of the line.
    fn a_live_line_longer_than_its_document(
        chat: &gpui::Entity<Chat>,
        cx: &mut gpui::VisualTestContext,
    ) -> gpui::Bounds<gpui::Pixels> {
        let line = "See [a][1] and [b][2].";
        let reply = format!("{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n");
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
            chat.reveal.as_mut().expect("live").show(line.len());
        });
        refresh_frame(cx);
        let (row, on_screen) = last_painted_row(chat, cx);
        assert_eq!(on_screen, line);
        row
    }

    /// The bytes the last paint lit in the replies' texts, each in its own text's bytes.
    fn highlighted(chat: &gpui::Entity<Chat>, cx: &gpui::VisualTestContext) -> Vec<std::ops::Range<usize>> {
        chat.read_with(&cx.cx, |chat, _| {
            (chat.transcript_links.borrow().iter())
                .flat_map(|layouts| layouts.drawn_parts())
                .flat_map(|part| part.selected)
                .collect()
        })
    }

    fn tool_call_started(id: &str) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::ToolCallStarted {
            id: id.into(),
            title: "Edit a file".into(),
            status: "InProgress".into(),
            kind: "Edit".into(),
            content: vec![],
            locations: vec![],
            raw_input: None,
            raw_output: None,
        }
    }

    /// A drag made in a live reply is of the text drawn for it, whose offsets do not map onto the
    /// document the reply is drawn from once it ends: the drag goes when the reply ends, and it is
    /// not handed to the next reply. Here a tool call ends reply A, and reply B starts below it; at
    /// no step does Ctrl+C write anything.
    #[gpui::test]
    async fn a_drag_in_a_reply_that_a_tool_call_ends_goes_with_it(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        let row = a_live_line_longer_than_its_document(&chat, cx);
        drag(cx, gpui::point(row.left() + px(1.0), row.center().y), gpui::point(row.right() - px(10.0), row.center().y));
        assert_eq!(copy_keys(cx).as_deref(), Some("See [a][1] and [b][2]."), "the drag copies the line as drawn");

        chat.update(cx, |chat, cx| chat.handle_event(tool_call_started("t1"), cx));
        refresh_frame(cx);
        assert_eq!(copy_keys(cx), None, "after the tool call");
        assert!(chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()), "the tool call ended the reply, and the drag in it");

        chat.update(cx, |chat, cx| chat.handle_event(chunk("Reply B, which takes a while to reveal at its pace."), cx));
        for step in 0..3 {
            tick(cx, Duration::from_millis(30));
            assert_eq!(copy_keys(cx), None, "frame {step} of reply B");
            assert!(chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()), "frame {step} of reply B");
        }
    }

    /// A drag from the user's message above a live reply into the reply goes when the reply ends:
    /// its head is in the drawn text, and the rows below it would slide under it.
    #[gpui::test]
    async fn a_drag_from_above_into_a_reply_that_ends_goes_with_it(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, _| {
            chat.push_entry(Entry::User {
                text: "Please read the two links.".into(),
                at: None,
            })
        });
        let row = a_live_line_longer_than_its_document(&chat, cx);
        let user = cx.debug_bounds("user-bubble-0").expect("the message is drawn");
        drag(cx, gpui::point(user.right() - px(60.0), user.center().y), gpui::point(row.right() - px(10.0), row.center().y));
        let copied = copy_keys(cx).expect("the drag selected something");
        let (message, reply) = copied.split_once("\n\n").expect("two rows");
        assert!(!message.is_empty() && "Please read the two links.".ends_with(message), "{copied:?}");
        assert_eq!(reply, "See [a][1] and [b][2].", "the line as drawn");

        chat.update(cx, |chat, cx| chat.handle_event(tool_call_started("t1"), cx));
        refresh_frame(cx);
        assert_eq!(copy_keys(cx), None, "after the tool call");
        assert!(chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()), "the tool call ended the reply, and the drag into it");
    }

    /// The end of the turn ends the reply, and a drag in it.
    #[gpui::test]
    async fn a_drag_in_a_reply_goes_when_its_turn_ends(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        let row = a_live_line_longer_than_its_document(&chat, cx);
        drag(cx, gpui::point(row.left() + px(1.0), row.center().y), gpui::point(row.right() - px(10.0), row.center().y));
        assert_eq!(copy_keys(cx).as_deref(), Some("See [a][1] and [b][2]."));

        chat.update(cx, |chat, cx| {
            chat.handle_event(sirio_chat::ChatEvent::TurnEnded { stop_reason: "EndTurn".into() }, cx)
        });
        refresh_frame(cx);
        assert_eq!(copy_keys(cx), None, "after the turn's end");
        assert!(chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()), "the turn ended, and the drag");
    }

    /// A row removed above a live reply (an error dismissed, or the connection errors a reconnect
    /// clears) renumbers the entries below it: the end of a drag placed in the user's message above
    /// the reply would be taken for one in the reply, which now has the message's number, and the
    /// end in the reply for one in a row past it. The drag goes, as the folds and scrolls keyed by
    /// entry do.
    #[gpui::test]
    async fn a_drag_in_a_live_reply_goes_when_a_row_above_it_is_removed(cx: &mut TestAppContext) {
        for how in ["dismissed", "cleared on reconnect"] {
            let (chat, cx) = chat_with(cx, &[]);
            chat.update(cx, |chat, _| {
                chat.push_entry(Entry::Error {
                    message: "lost".into(),
                    retryable: true,
                    kind: super::super::ErrorKind::Connection,
                });
                chat.push_entry(Entry::User {
                    text: "Please read the two links.".into(),
                    at: None,
                });
            });
            let row = a_live_line_longer_than_its_document(&chat, cx);
            let user = cx.debug_bounds("user-bubble-1").expect("the message is drawn");
            drag(cx, gpui::point(user.right() - px(60.0), user.center().y), gpui::point(row.right() - px(10.0), row.center().y));
            let copied = copy_keys(cx).expect("the drag selected something");
            assert!(copied.ends_with("\n\nSee [a][1] and [b][2]."), "{how}: {copied:?}");

            chat.update(cx, |chat, cx| match how {
                "dismissed" => chat.dismiss_error(0, cx),
                _ => chat.clear_recovered_connection_errors(),
            });
            refresh_frame(cx);
            assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.entries.len()), 2, "{how}: the error went");
            assert_eq!(copy_keys(cx), None, "{how}: after the error went");
            assert_eq!(highlighted(&chat, cx), Vec::<std::ops::Range<usize>>::new(), "{how}: the reply is not lit");
            assert!(chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_none()), "{how}");
        }
    }

    /// A reply that stops revealing in mid-turn is drawn from `document` from the next frame on, and
    /// the offsets of a drag made in it were places in the text drawn for it: the drag goes, as when
    /// the reply ends. Four ways: the tail goes past the budget, the stream goes whole without a
    /// refusal, the reader panics on the tail, the reader panics on the document.
    #[gpui::test]
    async fn a_drag_in_a_live_reply_goes_when_the_reply_stops_revealing(cx: &mut TestAppContext) {
        let ways = ["over the budget", "gone whole", "the tail panics", "the document panics"];
        // Each way: whether the reply stopped revealing, what Ctrl+C writes then, whether a
        // selection is left. Gathered, so that one way's failure does not hide another's.
        let mut found = Vec::new();
        for how in ways {
            let (chat, cx) = chat_with(cx, &[]);
            let row = a_live_line_longer_than_its_document(&chat, cx);
            drag(cx, gpui::point(row.left() + px(1.0), row.center().y), gpui::point(row.right() - px(10.0), row.center().y));
            assert_eq!(copy_keys(cx).as_deref(), Some("See [a][1] and [b][2]."), "{how}: the drag copies the line");

            let mut fault = None;
            match how {
                "over the budget" => {
                    // Lines with no blank one between them never freeze: the tail grows past 7 KiB.
                    let mut more = String::from("\n");
                    let mut line = 0;
                    while more.len() < 8 * 1024 {
                        line += 1;
                        more.push_str(&format!("Line {line} of a compact reply, with `code` and **emphasis**.\n"));
                    }
                    chat.update(cx, |chat, cx| chat.handle_event(chunk(&more), cx));
                    for _ in 0..40 {
                        tick(cx, Duration::from_millis(100));
                        if chat.update(cx, |chat, _| chat.drawn_reply(0)).is_none() {
                            break;
                        }
                    }
                }
                "gone whole" => {
                    // No reply of ours goes whole without a refusal: as in
                    // `a_reply_gone_whole_without_a_refusal_draws_its_formatted_document`, a deep
                    // quote turns the stream whole and the document is swapped for a formatted one.
                    let deep = format!("\n{} too deep\n", ">".repeat(40));
                    chat.update(cx, |chat, cx| {
                        chat.handle_event(chunk(&deep), cx);
                        let Some(Entry::Assistant { document, stream: Some(stream), .. }) = chat.entries.first_mut() else {
                            panic!("{how}: a live reply");
                        };
                        assert!(stream.is_whole(), "{how}: the deep quote turned the stream whole");
                        *document = super::super::parse_chat_markdown("# A heading\n\nWith **bold** words.");
                    });
                    tick(cx, Duration::from_millis(30));
                }
                "the tail panics" => {
                    fault = Some(crate::markdown::reader_fault::on());
                    tick(cx, Duration::from_millis(30));
                }
                _ => {
                    fault = Some(crate::markdown::reader_fault::on());
                    chat.update(cx, |chat, cx| chat.handle_event(chunk(" And more."), cx));
                    tick(cx, Duration::from_millis(30));
                }
            }
            let stopped = chat.update(cx, |chat, _| chat.drawn_reply(0)).is_none();
            let copied = copy_keys(cx);
            let selected = chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.is_some());
            found.push((how, stopped, copied, selected));
            drop(fault);
        }
        let expected: Vec<_> = ways.iter().map(|how| (*how, true, None, false)).collect();
        assert_eq!(found, expected, "(way, stopped revealing, Ctrl+C wrote, a selection left)");
    }

    /// A drag pressed in the live reply and let go in a notice below it, whose offsets (in the
    /// document's text) are smaller than the press's (in the drawn text): the press is still the
    /// start, as the rows are in that order. It highlights the reply from the press to the end of
    /// its drawn text, and copies that, the separator, and the start of the notice.
    #[gpui::test]
    async fn a_drag_from_a_live_reply_down_into_a_notice_copies_both_in_order(
        cx: &mut TestAppContext,
    ) {
        let line = "[a][1] [b][2] [c][3] [d][4] [e][5].";
        let reply = format!(
            "{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n\
             [3]: https://example.com/3\n[4]: https://example.com/4\n[5]: https://example.com/5\n"
        );
        let notice_text = "done, and said so at some length";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
            chat.push_entry(Entry::Notice {
                text: notice_text.into(),
                kind: NoticeKind::BackgroundTask,
            });
            chat.reveal.as_mut().expect("live").show(line.len());
        });
        refresh_frame(cx);
        let (row, on_screen) = last_painted_row(&chat, cx);
        assert_eq!(on_screen, line);
        let (reply_row, notice_row) = chat.read_with(&cx.cx, |chat, _| {
            let ranges = chat.transcript_entry_ranges();
            (ranges[0].clone(), ranges[1].clone())
        });
        assert_eq!(reply_row.len(), "a b c d e.".len(), "the document has resolved every label");

        // The press lands on `[e][5]`, past the 10 bytes of the document's row.
        let press = chat.read_with(&cx.cx, |chat, _| {
            let layouts = chat.transcript_links.borrow();
            let reply = layouts.first().expect("the reply's layouts");
            let mut x = row.left();
            while reply.offset_at(gpui::point(x, row.center().y)).is_some_and(|at| at < line.find("[e]").unwrap() + 1) {
                x += px(1.0);
            }
            gpui::point(x, row.center().y)
        });
        let notice = cx.debug_bounds("notice-task-text").expect("the notice is drawn");
        drag(cx, press, gpui::point(notice.left() + px(22.0), notice.center().y));
        let (anchor, head) = chat.read_with(&cx.cx, |chat, _| {
            let selection = chat.transcript_selection.as_ref().expect("a selection");
            (selection.anchor, selection.head)
        });
        let pressed = anchor - reply_row.start;
        let reached = head - notice_row.start;
        assert!(pressed > reply_row.len() && pressed < line.len(), "the press is past the document's row: {pressed}");
        assert!(reached > 0 && reached < notice_text.len(), "the end is inside the notice: {reached}");
        assert!(anchor > head, "the press's offset is the larger of the two");

        let expected = format!("{}\n\n{}", &line[pressed..], &notice_text[..reached]);
        assert_eq!(copy_keys(cx), Some(expected), "the reply's tail, the separator once, the notice's start");
        assert_eq!(highlighted(&chat, cx), vec![pressed..line.len()], "the reply is lit from the press to its end");
    }

    /// gpui sends a press to the listeners of the frame already drawn, and a chunk can grow the
    /// live reply between that frame and the press: the notice below it has moved in the
    /// transcript's offsets, and the frame's listeners hold where it was. The press is on the
    /// notice all the same, on the character under the pointer: the drag it starts copies the
    /// notice's text and nothing of the reply, and it is still there when the reply ends.
    #[gpui::test]
    async fn a_press_on_a_notice_that_a_chunk_moved_since_the_last_frame_is_in_the_notice(
        cx: &mut TestAppContext,
    ) {
        let notice_text = "a background task finished, and said so at some length";
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("Hello there."), cx);
            chat.push_entry(Entry::Notice {
                text: notice_text.into(),
                kind: NoticeKind::BackgroundTask,
            });
        });
        tick(cx, Duration::from_millis(400));
        let notice_start = |cx: &mut gpui::VisualTestContext| {
            chat.read_with(&cx.cx, |chat, _| chat.transcript_entry_ranges()[1].start)
        };
        let notice = cx.debug_bounds("notice-task-text").expect("the notice is drawn");
        let (press, release) = (
            gpui::point(notice.left() + px(40.0), notice.center().y),
            gpui::point(notice.left() + px(120.0), notice.center().y),
        );

        // On a frame that is current: which characters of the notice the two points are on.
        drag(cx, press, release);
        let before = notice_start(cx);
        let (from, to) = chat.read_with(&cx.cx, |chat, _| {
            let selection = chat.transcript_selection.as_ref().expect("a selection");
            (selection.anchor - before, selection.head - before)
        });
        assert!(0 < from && from < to && to < notice_text.len(), "the drag is inside the notice: {from}..{to}");
        assert_eq!(copy_keys(cx).as_deref(), Some(&notice_text[from..to]));
        chat.update(cx, |chat, cx| {
            chat.transcript_selection = None;
            cx.notify();
        });

        // A chunk, and the press before the next frame: one update, which draws no frame between
        // the two, so the press goes to the listeners of the frame drawn before the chunk.
        let more = " And some more words.";
        let placed = cx.update(|window, cx| {
            chat.update(cx, |chat, cx| chat.handle_event(chunk(more), cx));
            window.dispatch_event(
                gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                    position: press,
                    modifiers: gpui::Modifiers::none(),
                    button: gpui::MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            let selection = chat.read(cx).transcript_selection.clone().expect("the press placed an anchor");
            (selection.anchor, selection.anchor_entry)
        });
        let after = notice_start(cx);
        assert_eq!(after, before + more.len(), "the chunk moved the notice");
        cx.simulate_mouse_move(release, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(release, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            (placed, copy_keys(cx).as_deref()),
            ((after + from, Some(1)), Some(&notice_text[from..to])),
            "(where the press was placed and in which entry, what Ctrl+C writes after the drag)"
        );

        // The reply ends: the selection is the notice's, and stays.
        chat.update(cx, |chat, cx| {
            chat.handle_event(sirio_chat::ChatEvent::TurnEnded { stop_reason: "EndTurn".into() }, cx)
        });
        refresh_frame(cx);
        assert_eq!(copy_keys(cx).as_deref(), Some(&notice_text[from..to]), "after the turn's end");
    }

    /// A reply that has stopped revealing (here its tail went past the budget) is drawn from its
    /// document, and a drag made in it from then on is in the document's offsets. When the turn
    /// ends the reply is the same document, and the drag covers the same text: it stays, and
    /// copies what it copied.
    #[gpui::test]
    async fn a_drag_made_after_a_reply_stopped_revealing_stays_when_its_turn_ends(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        // Lines with no blank one between them never freeze: the tail grows past 7 KiB.
        let mut reply = String::new();
        let mut line = 0;
        while reply.len() < 8 * 1024 {
            line += 1;
            reply.push_str(&format!("Line {line} of a compact reply, with `code` and **emphasis**.\n"));
        }
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
        });
        for _ in 0..40 {
            tick(cx, Duration::from_millis(100));
            if !chat.read_with(&cx.cx, |chat, _| chat.revealing(0)) {
                break;
            }
        }
        let (streams, revealing, document) = chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant { stream, document, .. }) = chat.entries.first() else {
                panic!("a reply")
            };
            (stream.is_some(), chat.revealing(0), document.text.text.clone())
        });
        assert!(streams && !revealing, "the reply still streams, and is no longer revealed");
        refresh_frame(cx);

        // A drag along a line of it that the window shows.
        let shown = chat.read_with(&cx.cx, |chat, _| {
            let viewport = chat.list_state.viewport_bounds();
            let painted = chat.transcript_links.borrow();
            let (last, _) = (painted.iter().flat_map(|layouts| layouts.text_rows()))
                .filter(|(bounds, _)| bounds.intersects(&viewport))
                .last()
                .expect("the window shows a text of the reply");
            last.intersect(&viewport)
        });
        drag(
            cx,
            gpui::point(shown.left() + px(1.0), shown.center().y),
            gpui::point(shown.right() - px(10.0), shown.center().y),
        );
        let copied = copy_keys(cx).expect("the drag selected a line of the reply");
        assert!(copied.len() > 20 && document.contains(&copied), "a piece of the document: {copied:?}");

        chat.update(cx, |chat, cx| {
            chat.handle_event(sirio_chat::ChatEvent::TurnEnded { stop_reason: "EndTurn".into() }, cx)
        });
        refresh_frame(cx);
        assert_eq!(copy_keys(cx), Some(copied), "after the turn's end");
    }

    /// A drag pressed in the live reply and let go in the notice below it, at offsets that are the
    /// same number: the press at the 20th byte of the reply's drawn text, the end at the 20th byte
    /// of the transcript's text, which is the notice's 6th. They are two places, in two rows, and
    /// what lies between them is selected: it is lit, Ctrl+C writes it, and the transcript takes
    /// the focus, as it does for any drag that selects.
    #[gpui::test]
    async fn a_drag_from_a_live_reply_to_the_same_offset_in_a_notice_selects_what_it_lights(
        cx: &mut TestAppContext,
    ) {
        let line = "See [a][1] and [b][2].";
        let reply = format!("{line}\n\n[1]: https://example.com/1\n[2]: https://example.com/2\n");
        let notice_text = "done, and said so at some length";
        let same = 20;
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk(&reply), cx);
            chat.push_entry(Entry::Notice {
                text: notice_text.into(),
                kind: NoticeKind::BackgroundTask,
            });
            chat.reveal.as_mut().expect("live").show(line.len());
        });
        refresh_frame(cx);
        let (row, on_screen) = last_painted_row(&chat, cx);
        assert_eq!(on_screen, line, "22 bytes drawn");
        let rows = chat.read_with(&cx.cx, |chat, _| chat.transcript_entry_ranges());
        assert_eq!(rows, vec![0..12, 14..14 + notice_text.len()], "12 in the document, the notice from 14");

        // Where the 20th byte of the drawn line is.
        let press = chat.read_with(&cx.cx, |chat, _| {
            let layouts = chat.transcript_links.borrow();
            let reply = layouts.first().expect("the reply's layouts");
            let mut x = row.left();
            while reply.offset_at(gpui::point(x, row.center().y)).is_some_and(|at| at < same) {
                x += px(1.0);
            }
            gpui::point(x, row.center().y)
        });
        // Where the same offset is in the notice: a drag inside the notice finds it, a pixel at a
        // time. It is then undone, and the focus put back in the composer.
        let notice = cx.debug_bounds("notice-task-text").expect("the notice is drawn");
        let mut end = gpui::point(notice.left() + px(1.0), notice.center().y);
        cx.simulate_mouse_down(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        let head = |cx: &mut gpui::VisualTestContext| {
            chat.read_with(&cx.cx, |chat, _| chat.transcript_selection.as_ref().expect("a selection").head)
        };
        while head(cx) < same {
            end.x += px(1.0);
            assert!(end.x < notice.right(), "the notice has a 6th byte");
            cx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        }
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        chat.update(cx, |chat, cx| {
            chat.transcript_selection = None;
            cx.notify();
        });
        focus_composer(&chat, cx);
        let transcript_focused = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| chat.read(cx).transcript_focus.is_focused(window))
        };
        assert!(!transcript_focused(cx), "the focus is in the composer before the drag");

        // The drag, in one move: no step of it has two different offsets.
        drag(cx, press, end);
        let ends = chat.read_with(&cx.cx, |chat, _| {
            let selection = chat.transcript_selection.as_ref().expect("the drag left a selection");
            (selection.anchor, selection.anchor_entry, selection.head, selection.head_entry)
        });
        assert_eq!(ends, (same, Some(0), same, Some(1)), "the same offset, in the reply and in the notice");
        let focused = transcript_focused(cx);
        let copied = copy_keys(cx);
        refresh_frame(cx);
        assert_eq!(
            (focused, copied.as_deref(), highlighted(&chat, cx)),
            (true, Some("].\n\ndone, "), vec![same..line.len()]),
            "(the transcript has the focus, what Ctrl+C writes, what the reply lights)"
        );
    }

    /// A reply whose stream went whole without being refused (the guard declined a later text, or
    /// the math and HTML check failed) has a formatted document, and the raw source of its revealed
    /// bytes would be a worse draw than the document: it stops revealing, as a reply over the budget
    /// does, and draws `document` as it arrives, with no caret. (A refused reply, whose document is
    /// the plain paragraph, reveals as plain text: `a_reply_gone_whole_reveals_as_plain_text`.)
    /// No reply of ours goes whole without a refusal, so the document is swapped for a formatted one.
    #[gpui::test]
    async fn a_reply_gone_whole_without_a_refusal_draws_its_formatted_document(
        cx: &mut TestAppContext,
    ) {
        let deep = format!("{} too deep\n", ">".repeat(40));
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("First block\n\nSecond block\n\n"), cx);
            chat.handle_event(chunk(&deep), cx);
            let Some(Entry::Assistant { document, .. }) = chat.entries.first_mut() else {
                panic!("a reply")
            };
            *document = super::super::parse_chat_markdown("# A heading\n\nWith **bold** words.");
        });
        chat.read_with(&cx.cx, |chat, _| {
            let Some(Entry::Assistant {
                stream: Some(stream),
                ..
            }) = chat.entries.first()
            else {
                panic!("the reply streams");
            };
            assert!(stream.is_whole(), "the deep quote turned the stream whole");
        });
        tick(cx, Duration::from_millis(100));
        assert!(chat.update(cx, |chat, _| chat.drawn_reply(0)).is_none());
        let length = chat.read_with(&cx.cx, |chat, _| match chat.entries.first() {
            Some(Entry::Assistant { text, .. }) => text.len(),
            _ => unreachable!(),
        });
        assert_eq!(revealed(&chat, cx), length, "it stopped revealing");
        assert!(painted_caret(&chat, cx).is_none());
        let drawn = painted_text(&chat, cx);
        assert!(drawn.contains("A heading") && !drawn.contains("# A heading"), "{drawn:?}");
    }

    /// A body whose last block holds no text (a rule here, a web picture, a formula) gets no caret:
    /// the nearest text is the paragraph above, and a caret after it would be in the wrong row.
    /// With a text last, the caret is on that text's line.
    #[gpui::test]
    async fn a_body_that_ends_in_a_rule_has_no_caret(cx: &mut TestAppContext) {
        let (chat, cx) = chat_with(cx, &[]);
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("A paragraph of words.\n\n---\n"), cx);
        });
        tick(cx, Duration::from_secs(10));
        assert_eq!(revealed(&chat, cx), "A paragraph of words.\n\n---\n".len());
        let (row, text) = last_painted_row(&chat, cx);
        assert_eq!(text, "A paragraph of words.");
        assert!(painted_caret(&chat, cx).is_none(), "a caret after the text above a rule");

        chat.update(cx, |chat, cx| chat.handle_event(chunk("\nAnd words after it."), cx));
        tick(cx, Duration::from_secs(10));
        let caret = painted_caret(&chat, cx).expect("a text is last: the caret follows it");
        let (last, text) = last_painted_row(&chat, cx);
        assert_eq!(text, "And words after it.");
        assert!(last.top() > row.top() && caret.y == last.top(), "{caret:?} in {last:?}");
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
    /// through (the last KiB under the budget), built with the chat's own Policy and a file-link
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
                eprintln!("BENCH drawn_blocks {name}: no frame has a tail of the last KiB under the budget");
            } else {
                let times: Vec<Duration> = edge.iter().map(|at| frame(*at, &mut memo)).collect();
                eprintln!(
                    "BENCH drawn_blocks {name}: tails of the last KiB under the budget: {}",
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

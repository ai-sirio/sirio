//! A live reply drawn at Ely's reading pace, with a caret after it (spec §5.2).
//!
//! While a turn streams, the reply is not drawn from `document` (the tree of all that has arrived)
//! but from [`drawn_blocks`]: the frozen blocks of its [`StreamingDoc`] that are shown whole, then
//! the text revealed since, built as the tail of the reply, with a `▍` after it. Ely's own
//! `next_shown` and `grapheme_end` set the pace (90 graphemes a second, catching up within 350 ms,
//! all at once under reduced motion) and the end of every step on a grapheme.
//!
//! The text received, `document` included, is what copy, the control socket and the file-link memo
//! read. What is revealed is only what the next frame draws.

use std::cell::Cell;
use std::time::Instant;

use ely_gpui_component::chat::stream::{grapheme_end, next_shown};
use ely_gpui_component::theme::ActiveTheme as _;
use gpui::{Context, Window};
use unicode_segmentation::UnicodeSegmentation;

use super::{Chat, Entry, with_linked_policy};
use crate::markdown::{self, MarkdownBlocks, Policy, Segment};

/// The mark where more will come, set inline after the text, as Ely's `StreamingMarkdown` does.
const CARET: &str = "▍";

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
}

impl Reveal {
    /// A reveal that has shown nothing yet, started at `at`.
    pub(super) fn new(at: Instant) -> Self {
        Self {
            shown: 0.0,
            at,
            plain: false,
            counted: Cell::default(),
            walked: Cell::default(),
        }
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
/// segment that ends by it, then `text[from..revealed]` and the caret built as the tail (`from`
/// is where the last of those segments ends), all joined. `document` is not touched.
///
/// `revealed` is a character boundary of `text`, and `segments` are those of the reply's
/// [`StreamingDoc`] (none for a reply gone whole: its document is one plain paragraph).
pub(crate) fn drawn_blocks(
    text: &str,
    segments: &[Segment],
    revealed: usize,
    policy: &mut Policy<'_>,
) -> MarkdownBlocks {
    let shown = frozen_by(segments, revealed);
    let from = tail_from(shown, revealed);
    let tail = format!("{}{CARET}", &text[from..revealed]);
    markdown::with_tail(shown, &tail, policy)
}

impl Chat {
    /// Whether the reply at `entry` is being revealed: it is live (its stream is alive) and has
    /// not gone past the tail's budget.
    fn revealing(&self, entry: usize) -> bool {
        matches!(
            self.entries.get(entry),
            Some(Entry::Assistant {
                stream: Some(_),
                ..
            })
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

    /// What the reply at `entry` draws now, when the reveal draws it: `None` draws `document`
    /// (it is not live, or its tail went past the budget, which it does for good).
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
        let segments: &[Segment] = if stream.is_whole() {
            &[]
        } else {
            stream.segments()
        };
        if end - tail_from(segments, end) > TAIL_BUDGET {
            reveal.plain = true;
            return None;
        }
        Some(with_linked_policy(
            &self.agent_cwd,
            &mut self.file_link_memo,
            |policy| drawn_blocks(text, segments, end, policy),
        ))
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
        self.drawn_reply(entry)
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

    use gpui::TestAppContext;
    use ely_gpui_component::documents::MarkdownBlock;
    use unicode_segmentation::UnicodeSegmentation;

    use super::super::markdown_tests::{SHOWCASE, chat_with, painted_text, revealed, tick};
    use super::super::tests::refresh_frame;
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

    /// The frozen blocks of the segments up to the reveal, then what is revealed of the next one
    /// with the caret after it: the segments are not parsed again, the tail is.
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
        assert_eq!(tail.text, "Second ▍", "the revealed words and the caret, nothing after");
        assert!(drawn.text.text.ends_with('▍'));

        // Revealed to the end of the third block, it is frozen too, and the caret stands alone.
        let drawn = drawn_blocks(SHOWCASE, segments, third.end, &mut policy());
        assert_eq!(drawn.nodes.len(), frozen.len() + segments[2].nodes.len() + 1);
        let MarkdownBlock::Paragraph(caret) = drawn.nodes.last().unwrap() else {
            panic!("the caret after the last revealed block is a paragraph");
        };
        assert_eq!(caret.text, "▍");
    }

    /// A reply whose blocks do not depend on each other draws, at every byte it can be revealed to,
    /// what a build of that text and the caret makes: the same nodes, with the source offsets of a
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
                let full = markdown::build(&format!("{}{CARET}", &reply[..revealed]), &mut policy());
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
        assert_eq!(drawn.text.text, "Fir▍");

        // Inside the third, after two frozen segments that begin with the mark.
        let third = text.find("Third").unwrap() + "Third b".len();
        let drawn = drawn_blocks(text, segments, third, &mut policy());
        assert!(
            drawn.text.text.starts_with("First block") && drawn.text.text.ends_with("Third b▍"),
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

    /// A reply that is not live (restored, or ended) is drawn from `document`, whole.
    #[gpui::test]
    async fn not_live_draws_the_document(cx: &mut TestAppContext) {
        let reply = "A **restored** reply.\n\nWith two blocks.";
        let (chat, cx) = chat_with(cx, &[reply]);
        refresh_frame(cx);
        chat.read_with(&cx.cx, |chat, _| assert_eq!(chat.revealed_bytes(0), reply.len()));
        let drawn = painted_text(&chat, cx);
        assert!(!drawn.contains('▍'), "{drawn:?}");
        assert!(drawn.contains("A restored reply."), "{drawn:?}");
        assert_eq!(revealed(&chat, cx), reply.len());
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
            chat.update(cx, |chat, _| {
                chat.drawn_reply(0).map(|blocks| blocks.text.text.clone())
            })
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
            let text = drawn(cx).expect("a reply within the budget is drawn by the reveal");
            assert!(text.ends_with('▍'), "{:?}", &text[text.len().saturating_sub(40)..]);
            revealing += 1;
        }
        assert!(revealing >= 2, "it revealed before it gave up: {revealing} frames");

        // Past it, it is the document, whole.
        assert_eq!(revealed(&chat, cx), reply.len());
        assert!(drawn(cx).is_none(), "a reply past the budget is drawn from `document`");
        assert!(!painted_text(&chat, cx).contains('▍'));

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
        assert!(!painted_text(&chat, cx).contains('▍'));
    }

    /// A reply the nesting guard refused has one plain paragraph for its document, and the frozen
    /// blocks it held before are not what it shows: it reveals as that paragraph, from its start.
    /// The budget is the same for it as for any other.
    #[gpui::test]
    async fn a_reply_gone_whole_reveals_as_one_plain_paragraph(cx: &mut TestAppContext) {
        let deep = format!("{} too deep\n", ">".repeat(40));
        let (chat, cx) = chat_with(cx, &[]);
        // All at once: the prefixes of the deep quote that stay under the cap are real nested
        // quotes, and a debug draw of them overflows the 2 MiB stack a test thread has.
        cx.update(|_, cx| {
            cx.set_reduce_motion(true);
            crate::ely::sync_theme_if_changed(cx);
        });
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(chunk("First block\n\nSecond block\n\n"), cx);
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
        tick(cx, Duration::from_secs(10));
        let drawn = chat
            .update(cx, |chat, _| chat.drawn_reply(0))
            .expect("a short reply is drawn by the reveal");
        assert_eq!(drawn.nodes.len(), 1, "{:?}", drawn.nodes);
        assert!(drawn.text.text.starts_with("First block"), "{:?}", drawn.text.text);
        assert!(drawn.text.text.contains("Second block"));
        assert!(drawn.text.text.ends_with("too deep\n▍"), "{:?}", drawn.text.text);

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
        assert!(painted_text(&chat, cx).ends_with('▍'));
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
        use super::super::with_linked_policy;
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
                let drawn = with_linked_policy(&cwd, memo, |policy| {
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

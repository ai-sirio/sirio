//! A chat reply drawn through the Markdown pipeline and selected in the transcript's one
//! offset space: what a click leaves focused, what a drag opens, what the menu offers, and
//! that a reply cut at any byte still draws.

use gpui::{Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px, size};

use super::tests::{composer_is_focused, focus_composer, offline_chat_view, refresh_frame, right_click};
use super::*;

pub(super) const SHOWCASE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../Scripts/Tests/fixtures/markdown-documents/showcase.md"
));

/// A 24 px square PNG: a picture with a size the pointer can hit.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR4nGNQaHhAFcQwatCoQaMGjRo0atCoQQNvEACD/WAuD5bdZgAAAABJRU5ErkJggg==";

/// A drawn chat holding `replies`, one reply each.
pub(super) fn chat_with<'a>(
    cx: &'a mut TestAppContext,
    replies: &[&str],
) -> (Entity<Chat>, &'a mut VisualTestContext) {
    let (chat, cx) = offline_chat_view(cx);
    cx.simulate_resize(size(px(900.0), px(700.0)));
    chat.update(cx, |chat, _| {
        for reply in replies {
            chat.push_entry(Entry::Assistant {
                text: (*reply).into(),
                document: parse_chat_markdown(reply),
                stream: None,
            });
        }
    });
    refresh_frame(cx);
    (chat, cx)
}

/// Where the first link the last paint drew can be pressed: a few pixels into its label.
fn link_point(chat: &Entity<Chat>, cx: &VisualTestContext, label: &str) -> (Point<Pixels>, f32) {
    let rows = chat.read_with(&cx.cx, |chat, _| {
        super::transcript_link_rows(&chat.transcript_links.borrow())
    });
    let row = rows
        .iter()
        .find(|row| row.get("label").map(String::as_str) == Some(label))
        .unwrap_or_else(|| panic!("the paint drew no link {label:?}: {rows:?}"));
    let number = |key: &str| row[key].parse::<f32>().unwrap();
    (point(px(number("x")), px(number("y"))), number("w"))
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
}

mod cull;

/// The cuts of a streamed reply: every char boundary of the first 512 bytes, then about
/// every 97 bytes (rounded up to a char boundary), then the end.
fn cuts(source: &str) -> Vec<usize> {
    let mut cuts: Vec<usize> = (1..=512.min(source.len()))
        .filter(|at| source.is_char_boundary(*at))
        .collect();
    let mut at = 512;
    while at < source.len() {
        at += 97;
        while at < source.len() && !source.is_char_boundary(at) {
            at += 1;
        }
        cuts.push(at.min(source.len()));
    }
    cuts.dedup();
    cuts
}

/// The streamed text shrinks when a chunk closes a mark (`**bol` is five glyphs, `**bold**`
/// is four), so a selection made over what was there can end past what is now. Each step
/// holds a selection over everything, takes the next chunk and draws.
#[gpui::test]
async fn a_reply_cut_at_every_byte_renders_and_keeps_its_selection_in_bounds(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = chat_with(cx, &[]);
    chat.update(cx, |chat, _| chat.streaming = true);
    let started = std::time::Instant::now();
    let mut from = 0;
    let steps = cuts(SHOWCASE);
    for cut in &steps {
        chat.update(cx, |chat, cx| {
            let seen = chat.transcript_text().len();
            if seen > 0 {
                chat.transcript_selection = Some(TranscriptSelection {
                    anchor: 0,
                    head: seen,
                    whole: true,
                    ..TranscriptSelection::default()
                });
            }
            chat.handle_event(
                sirio_chat::ChatEvent::AgentMessageChunk(SHOWCASE[from..*cut].into()),
                cx,
            );
        });
        from = *cut;
        refresh_frame(cx);
        chat.read_with(&cx.cx, |chat, _| {
            let text = chat.transcript_text();
            if let Some(selection) = &chat.transcript_selection {
                let range = selection.range();
                assert!(
                    range.end <= text.len()
                        && text.is_char_boundary(range.start)
                        && text.is_char_boundary(range.end),
                    "cut at {cut}: selection {range:?} in a text of {} bytes",
                    text.len()
                );
                assert!(
                    range.is_empty() || chat.selected_transcript_text().is_some(),
                    "cut at {cut}: a selection that copies nothing"
                );
            }
        });
    }
    assert_eq!(
        chat.read_with(&cx.cx, |chat, _| chat.entries.len()),
        1,
        "every chunk grew the one reply"
    );
    eprintln!("cut test: {} cuts in {:?}", steps.len(), started.elapsed());
}

/// A reply that streamed in 200 chunks and then ended is the reply that came in one chunk and
/// ended: the turn's end drops the streaming state and leaves the tree a full build makes.
#[gpui::test]
async fn a_finished_reply_equals_a_one_chunk_reply(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &[]);
    let end = || sirio_chat::ChatEvent::TurnEnded {
        stop_reason: "EndTurn".into(),
    };
    chat.update(cx, |chat, cx| {
        chat.streaming = true;
        let mut from = 0;
        for step in 1..=200usize {
            let mut cut = SHOWCASE.len() * step / 200;
            while !SHOWCASE.is_char_boundary(cut) {
                cut += 1;
            }
            chat.handle_event(
                sirio_chat::ChatEvent::AgentMessageChunk(SHOWCASE[from..cut].into()),
                cx,
            );
            from = cut;
        }
        assert!(
            matches!(
                chat.entries.last(),
                Some(Entry::Assistant {
                    stream: Some(_),
                    ..
                })
            ),
            "a reply that is still streaming carries its streaming state"
        );
        chat.handle_event(end(), cx);

        chat.streaming = true;
        chat.handle_event(
            sirio_chat::ChatEvent::AgentMessageChunk(SHOWCASE.into()),
            cx,
        );
        chat.handle_event(end(), cx);
    });
    chat.read_with(&cx.cx, |chat, _| {
        let replies: Vec<_> = (chat.entries.iter())
            .filter_map(|entry| match entry {
                Entry::Assistant {
                    text,
                    document,
                    stream,
                } => Some((text, document, stream)),
                _ => None,
            })
            .collect();
        let [(text, chunked, chunked_stream), (whole_text, whole, whole_stream)] = &replies[..]
        else {
            panic!("two replies, found {}", replies.len());
        };
        assert_eq!(text, whole_text);
        assert!(
            chunked_stream.is_none() && whole_stream.is_none(),
            "an ended reply keeps no streaming state"
        );
        assert!(
            chunked.nodes == whole.nodes,
            "the streamed reply's nodes differ from the one-chunk reply's"
        );
        assert_eq!(chunked.text.text, whole.text.text);
    });
}

/// A reply that is still arriving, at several cuts, is the tree `parse_linked_markdown` builds
/// of the text so far: the chat's own Policy (the files a reply names are links), not the bare
/// one the stream oracle builds with. The reply defines a label and a footnote after their
/// mentions, so the stream rewinds on the way.
#[gpui::test]
async fn a_streamed_reply_is_the_linked_build_of_its_text_so_far(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("sirio-stream-links-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder for the files the reply names");
    std::fs::write(dir.join("streamed_link.rs"), "fn main() {}\n").expect("a file to link to");
    let reply = "It names `streamed_link.rs` and `streamed_missing.rs`.\n\n\
        ## A heading\n\nSee [the docs][ref] and a note[^n].\n\n\
        - an item with `streamed_link.rs`\n- another\n\n\
        ```rust\nfn main() {}\n```\n\n\
        [ref]: https://example.com\n\n[^n]: the note\n\n\
        The last paragraph names `streamed_link.rs` again.\n";
    let (chat, cx) = chat_with(cx, &[]);
    let mut checked = 0;
    chat.update(cx, |chat, cx| {
        chat.streaming = true;
        chat.agent_cwd = dir.clone();
        let mut from = 0;
        while from < reply.len() {
            let mut cut = (from + 13).min(reply.len());
            while !reply.is_char_boundary(cut) {
                cut += 1;
            }
            chat.handle_event(
                sirio_chat::ChatEvent::AgentMessageChunk(reply[from..cut].into()),
                cx,
            );
            from = cut;
            let Some(Entry::Assistant {
                text,
                document,
                stream,
            }) = chat.entries.last()
            else {
                panic!("the reply is the last row");
            };
            assert!(stream.is_some(), "cut {cut}: the reply streams");
            let full = parse_linked_markdown(text, &dir, &mut Default::default());
            assert!(
                document.nodes == full.nodes,
                "cut {cut}: the streamed nodes differ from the linked build's"
            );
            assert_eq!(document.text.text, full.text.text, "cut {cut}: the plain text");
            checked += 1;
        }
    });
    assert!(checked > 10, "{checked} cuts were checked");
    std::fs::remove_dir_all(&dir).ok();
}

/// A reply that Stop ended keeps no streaming state, and a chunk that still arrives after it
/// does not start one: the rest of the reply is built whole and shown at once, and nothing
/// stays alive after the turn.
#[gpui::test]
async fn a_chunk_after_stop_starts_no_stream(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &[]);
    let chunk = |text: &str| sirio_chat::ChatEvent::AgentMessageChunk(text.into());
    chat.update(cx, |chat, cx| {
        chat.streaming = true;
        chat.handle_event(chunk("first block\n\nsecond "), cx);
        assert!(
            matches!(
                chat.entries.last(),
                Some(Entry::Assistant {
                    stream: Some(_),
                    ..
                })
            ),
            "the reply streams until Stop"
        );
        chat.cancel_turn(cx);
        chat.handle_event(chunk("block\n\nthird block"), cx);
    });
    chat.read_with(&cx.cx, |chat, _| {
        let Some(Entry::Assistant {
            text,
            document,
            stream,
        }) = chat.entries.last()
        else {
            panic!("the reply is the last row");
        };
        assert_eq!(chat.entries.len(), 1, "the late chunk joined the reply");
        assert_eq!(text, "first block\n\nsecond block\n\nthird block");
        assert!(stream.is_none(), "a chunk after Stop started a streaming state");
        let full = parse_linked_markdown(text, &chat.agent_cwd, &mut Default::default());
        assert!(document.nodes == full.nodes);
    });
}

/// Every text the last paint drew of the transcript's replies, in paint order: what is on the
/// screen, whatever `document` holds.
pub(super) fn painted_text(chat: &Entity<Chat>, cx: &VisualTestContext) -> String {
    chat.read_with(&cx.cx, |chat, _| {
        (chat.transcript_links.borrow().iter())
            .flat_map(|layouts| layouts.text_rows())
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Where the last paint drew the caret of a live reply, if it drew one: a decoration the view
/// paints after the body's last text, in no text of its own.
pub(super) fn painted_caret(chat: &Entity<Chat>, cx: &VisualTestContext) -> Option<Point<Pixels>> {
    chat.read_with(&cx.cx, |chat, _| {
        chat.transcript_links.borrow().iter().find_map(|layouts| layouts.caret())
    })
}

/// The box and the text of the last text the last paint drew of the transcript's replies.
pub(super) fn last_painted_row(
    chat: &Entity<Chat>,
    cx: &VisualTestContext,
) -> (gpui::Bounds<Pixels>, String) {
    chat.read_with(&cx.cx, |chat, _| {
        (chat.transcript_links.borrow().iter())
            .flat_map(|layouts| layouts.text_rows())
            .last()
            .expect("the paint drew a text")
    })
}

/// Lets `by` pass on the clock a reveal reads, and draws the frame after it.
pub(super) fn tick(cx: &mut VisualTestContext, by: std::time::Duration) {
    cx.executor().advance_clock(by);
    refresh_frame(cx);
}

/// How many bytes of the last reply show now.
pub(super) fn revealed(chat: &Entity<Chat>, cx: &VisualTestContext) -> usize {
    chat.read_with(&cx.cx, |chat, _| {
        let reply = (chat.entries.iter())
            .rposition(|entry| matches!(entry, Entry::Assistant { .. }))
            .expect("a reply");
        chat.revealed_bytes(reply)
    })
}

/// A reply that is still streaming shows as much of it as the reveal has come to; when the turn
/// ends, in the middle of that, the rest shows at once and the caret goes.
#[gpui::test]
async fn a_turn_end_in_mid_reveal_shows_everything_at_once(cx: &mut TestAppContext) {
    // Parts of a few blocks each, so that the stream freezes as it goes and the tail stays short.
    let mut source = String::new();
    let mut part = 0;
    while source.len() < 40 * 1024 {
        part += 1;
        source.push_str(&format!(
            "## Part {part}\n\nA paragraph of part {part}, with **bold**, `code` and a \
             [link](https://example.com/{part}).\n\n- the first point\n- the second\n\n"
        ));
    }
    let (chat, cx) = chat_with(cx, &[]);
    chat.update(cx, |chat, cx| {
        chat.streaming = true;
        let mut from = 0;
        for step in 1..=200usize {
            let mut cut = source.len() * step / 200;
            while !source.is_char_boundary(cut) {
                cut += 1;
            }
            chat.handle_event(
                sirio_chat::ChatEvent::AgentMessageChunk(source[from..cut].into()),
                cx,
            );
            from = cut;
        }
    });
    tick(cx, std::time::Duration::from_millis(100));
    let shown = revealed(&chat, cx);
    assert!(
        0 < shown && shown < source.len(),
        "a live reply is revealed part of the way: {shown} of {}",
        source.len()
    );
    assert!(painted_caret(&chat, cx).is_some(), "the live reply is drawn with its caret");

    chat.update(cx, |chat, cx| {
        chat.handle_event(
            sirio_chat::ChatEvent::TurnEnded {
                stop_reason: "EndTurn".into(),
            },
            cx,
        );
    });
    refresh_frame(cx);
    assert_eq!(revealed(&chat, cx), source.len(), "a finished reply shows whole");
    assert!(painted_caret(&chat, cx).is_none(), "the caret stayed after the turn ended");
    assert!(!painted_text(&chat, cx).contains('▍'));
}

/// Stop ends the reveal as a turn's end does, and a chunk that still arrives after it joins a
/// reply that is not live: it shows at once, with no caret.
#[gpui::test]
async fn a_chunk_after_stop_shows_everything_without_a_caret(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &[]);
    let chunk = |text: &str| sirio_chat::ChatEvent::AgentMessageChunk(text.into());
    chat.update(cx, |chat, cx| {
        chat.streaming = true;
        chat.handle_event(chunk("first block\n\nsecond "), cx);
    });
    tick(cx, std::time::Duration::from_millis(30));
    assert!(
        revealed(&chat, cx) < "first block\n\nsecond ".len(),
        "the reply is part of the way through its reveal when Stop comes"
    );
    assert!(painted_caret(&chat, cx).is_some());

    chat.update(cx, |chat, cx| {
        chat.cancel_turn(cx);
        chat.handle_event(chunk("block\n\nthird block"), cx);
    });
    refresh_frame(cx);
    chat.read_with(&cx.cx, |chat, _| {
        let Some(Entry::Assistant { text, .. }) = chat.entries.last() else {
            panic!("the reply is the last row");
        };
        assert_eq!(text, "first block\n\nsecond block\n\nthird block");
        assert_eq!(chat.revealed_bytes(chat.entries.len() - 1), text.len());
    });
    let drawn = painted_text(&chat, cx);
    assert!(!drawn.contains('▍') && painted_caret(&chat, cx).is_none(), "a late chunk after Stop drew a caret: {drawn:?}");
    assert!(drawn.contains("third block"), "the late chunk shows at once: {drawn:?}");
}

/// The anti-goal of spec §2: a click in a reply moves nothing out of the composer. (A pane
/// hands focus back after the press, but a drag and a release are later than that.)
#[gpui::test]
async fn a_plain_click_in_a_reply_leaves_the_composer_focused(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &["a reply with some words in it"]);
    focus_composer(&chat, cx);
    refresh_frame(cx);
    assert!(composer_is_focused(&chat, cx), "the composer starts focused");

    let reply = cx.debug_bounds("assistant-response-0").expect("the reply is drawn");
    click(cx, point(reply.left() + px(30.0), reply.top() + px(8.0)));

    assert!(composer_is_focused(&chat, cx), "a plain click took the focus");
}

#[gpui::test]
async fn a_click_on_a_linked_picture_leaves_the_composer_focused(cx: &mut TestAppContext) {
    let source = format!("[![alt]({PNG})](https://x.test/pic)");
    let (chat, cx) = chat_with(cx, &[&source]);
    focus_composer(&chat, cx);
    refresh_frame(cx);
    assert!(composer_is_focused(&chat, cx), "the composer starts focused");

    let picture = cx.debug_bounds("markdown-picture").expect("the picture drew");
    let before = cx.cx.opened_url();
    click(cx, picture.center());

    assert!(composer_is_focused(&chat, cx), "a click on a picture took the focus");
    assert_eq!(
        cx.cx.opened_url().as_deref(),
        Some("https://x.test/pic"),
        "the click opened the picture's link (and {before:?} before it)"
    );
}

/// D8. A drag that begins on a web link and moves along it selects its text; the release
/// that ends it is not a click.
#[gpui::test]
async fn a_drag_ending_on_a_web_link_selects_without_opening(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(
        cx,
        &["see [an exceptionally long link label](https://x.test/long) and then more"],
    );
    let (start, width) = link_point(&chat, cx, "an exceptionally long link label");
    assert!(width > 100.0, "the label is wide enough to drag along: {width}");
    let end = point(start.x + px(60.0), start.y);

    let before = cx.cx.opened_url();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let selected = chat.read_with(&cx.cx, |chat, _| chat.selected_transcript_text());
    assert!(
        selected.as_deref().is_some_and(|text| text.len() > 4),
        "the drag selected part of the label: {selected:?}"
    );
    assert!(cx.cx.opened_url() == before, "a drag opened the link");
}

/// A click on the same link, for contrast: a left click without a drag still opens it.
#[gpui::test]
async fn a_click_on_a_web_link_opens_it_once(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &["see [the docs](https://x.test/docs) now"]);
    let (spot, _) = link_point(&chat, cx, "the docs");
    click(cx, spot);
    assert_eq!(cx.cx.opened_url().as_deref(), Some("https://x.test/docs"));
}

/// A paragraph holding math is a text the renderer draws without handing it to the
/// selection; Select All still copies it.
#[gpui::test]
async fn select_all_and_copy_keep_math_paragraphs(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &["first para\n\nthe formula $x$ stays\n\nlast para"]);
    let reply = cx.debug_bounds("assistant-response-0").expect("the reply is drawn");
    right_click(cx, reply.center());
    refresh_frame(cx);
    let row = cx
        .debug_bounds("transcript-context-select-all")
        .expect("Select All is offered");
    cx.simulate_click(row.center(), Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-c");
    cx.run_until_parked();

    let _ = chat;
    assert_eq!(
        cx.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some("first para\n\nthe formula x stays\n\nlast para")
    );
}

/// The menu of the transcript offers the link's two rows over a link, and only there.
#[gpui::test]
async fn the_transcript_menu_offers_link_items_over_a_link(cx: &mut TestAppContext) {
    let (chat, cx) = chat_with(cx, &["see [the docs](https://x.test/docs) and plain words after it"]);
    let (on_link, width) = link_point(&chat, cx, "the docs");

    right_click(cx, on_link);
    refresh_frame(cx);
    assert!(cx.debug_bounds("transcript-context-copy").is_some());
    assert!(cx.debug_bounds("transcript-context-open-link").is_some(), "Open link over a link");
    assert!(
        cx.debug_bounds("transcript-context-copy-link").is_some(),
        "Copy link address over a link"
    );

    let copy = cx.debug_bounds("transcript-context-copy-link").unwrap();
    cx.simulate_click(copy.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        cx.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some("https://x.test/docs")
    );

    right_click(cx, on_link);
    refresh_frame(cx);
    let open = cx.debug_bounds("transcript-context-open-link").expect("Open link again");
    let before = cx.cx.opened_url();
    cx.simulate_click(open.center(), Modifiers::none());
    cx.run_until_parked();
    assert_ne!(cx.cx.opened_url(), before, "Open link opens through the chat's opener");
    assert_eq!(cx.cx.opened_url().as_deref(), Some("https://x.test/docs"));

    // Off the link, and the menu is the transcript's own two rows.
    let reply = cx.debug_bounds("assistant-response-0").expect("the reply is drawn");
    let plain = point(on_link.x + px(width + 150.0), on_link.y);
    assert!(reply.contains(&plain));
    right_click(cx, plain);
    refresh_frame(cx);
    assert!(cx.debug_bounds("transcript-context-copy").is_some());
    assert!(cx.debug_bounds("transcript-context-open-link").is_none());
    assert!(cx.debug_bounds("transcript-context-copy-link").is_none());
}

/// A drag that starts in the user's prose and ends in the reply is one selection.
#[gpui::test]
async fn a_drag_from_the_users_prose_into_a_reply_is_one_selection(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat_view(cx);
    cx.simulate_resize(size(px(900.0), px(700.0)));
    chat.update(cx, |chat, _| {
        chat.push_entry(Entry::User {
            text: "a question to ask".into(),
            at: None,
        });
        chat.push_entry(Entry::Assistant {
            text: "an answer that follows".into(),
            document: parse_chat_markdown("an answer that follows"),
            stream: None,
        });
    });
    refresh_frame(cx);

    let question = cx.debug_bounds("user-bubble-0").expect("the question is drawn");
    let reply = cx.debug_bounds("assistant-response-1").expect("the reply is drawn");
    let from = point(question.right() - px(60.0), question.center().y);
    let to = point(reply.left() + px(60.0), reply.top() + px(8.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    let selected = chat
        .read_with(&cx.cx, |chat, _| chat.selected_transcript_text())
        .expect("the drag selected");
    assert!(selected.contains("ask\n\nan a"), "{selected:?}");
}

/// One reply arriving in 200 chunks, as `bench_streaming` times it.
struct Run {
    /// `handle_event` per chunk.
    handles: Vec<std::time::Duration>,
    /// The `update` closure per chunk: `handle_event` and the draw its return flushes.
    built: Vec<std::time::Duration>,
    /// The same and one more frame.
    drawn: Vec<std::time::Duration>,
    /// A full build (`parse_linked_markdown`) of the text so far, timed after the chunk.
    before: Vec<std::time::Duration>,
    /// What the stream holds after each chunk: its cut and its number of segments. Empty when
    /// the chat was not streaming.
    states: Vec<(usize, usize)>,
    /// How many bytes of the reply the chat had after each chunk.
    lengths: Vec<usize>,
}

/// `source` arrives in 200 chunks, as a chat that is `streaming` (or not: then every chunk is
/// built whole, which is what the chat did before a reply was built in parts).
async fn run_reply(cx: &mut TestAppContext, source: &str, streaming: bool) -> Run {
    let (chat, cx) = chat_with(cx, &[]);
    chat.update(cx, |chat, _| chat.streaming = streaming);
    let cwd = std::env::temp_dir();
    let mut run = Run {
        handles: Vec::new(),
        built: Vec::new(),
        drawn: Vec::new(),
        before: Vec::new(),
        states: Vec::new(),
        lengths: Vec::new(),
    };
    let mut from = 0;
    for step in 1..=200usize {
        let mut cut = (source.len() * step / 200).min(source.len());
        while !source.is_char_boundary(cut) {
            cut += 1;
        }
        let started = std::time::Instant::now();
        let mut handled = std::time::Duration::ZERO;
        chat.update(cx, |chat, cx| {
            let handling = std::time::Instant::now();
            chat.handle_event(
                sirio_chat::ChatEvent::AgentMessageChunk(source[from..cut].into()),
                cx,
            );
            handled = handling.elapsed();
        });
        // `update` flushes effects and draws the window when its closure returns, so this
        // holds a whole draw besides the parse; `handled` is the parse alone.
        let parsed = started.elapsed();
        refresh_frame(cx);
        run.handles.push(handled);
        run.built.push(parsed);
        run.drawn.push(started.elapsed());
        let full = std::time::Instant::now();
        let _ = parse_linked_markdown(&source[..cut], &cwd, &mut Default::default());
        run.before.push(full.elapsed());
        chat.read_with(&cx.cx, |chat, _| {
            if let Some(Entry::Assistant {
                stream: Some(stream),
                ..
            }) = chat.entries.last()
            {
                run.states.push((stream.cut(), stream.segments().len()));
            }
        });
        run.lengths.push(cut);
        from = cut;
    }
    run
}

/// The ratio of `ours` to `base`, chunk by chunk, over the chunks whose `base` takes `floor` or
/// more (under it the row's own cost, the list and the layout, is most of both): the median, the
/// worst, how many chunks are above 1.15, and the longest run of consecutive chunks above it.
fn print_ratios(
    fixture: &str,
    what: &str,
    ours: &[std::time::Duration],
    base: &[std::time::Duration],
    floor: std::time::Duration,
) {
    let considered: Vec<Option<f64>> = (ours.iter().zip(base))
        .map(|(ours, base)| (*base >= floor).then(|| ours.as_secs_f64() / base.as_secs_f64()))
        .collect();
    let mut ratios: Vec<(usize, f64)> = considered
        .iter()
        .enumerate()
        .filter_map(|(index, ratio)| ratio.map(|ratio| (index, ratio)))
        .collect();
    if ratios.is_empty() {
        eprintln!("BENCH {fixture} {what}: no chunk has a base of {floor:?} or more");
        return;
    }
    let (mut run, mut longest) = (0, 0);
    for ratio in &considered {
        run = if ratio.is_some_and(|ratio| ratio > 1.15) { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let above = ratios.iter().filter(|(_, ratio)| *ratio > 1.15).count();
    let worst = *ratios.iter().max_by(|a, b| a.1.total_cmp(&b.1)).expect("a chunk");
    ratios.sort_by(|a, b| a.1.total_cmp(&b.1));
    let median = ratios[ratios.len() / 2].1;
    eprintln!(
        "BENCH {fixture} ratio to {what}, over {} chunks that take {floor:?} or more: median {median:.2}, worst {:.2} at chunk {}, above 1.15 in {above} chunks, longest run above 1.15: {longest}",
        ratios.len(),
        worst.1,
        worst.0
    );
}

/// The cost of a reply of `source` arriving in 200 chunks: each chunk costs one parse
/// (`handle_event`) and one drawn frame. The numbers are printed, nothing is asserted: a wall
/// clock in a test is a flake, and the baseline (bezel) is gone.
///
/// The reply arrives twice: once to a chat that is not streaming (every chunk built whole, what
/// the chat did before) and once to one that is. Beside the times it prints the ratio of
/// `handle_event` to a full build of the same text, timed in the same loop after the chunk, and to
/// the whole build the chat does when it is not streaming (that one has the row's own cost in it,
/// so it is the fair base where a build is short); the number of chunks above 1.15 times a build
/// and the longest run of them; the chunks that rewind (the cut of the stream moved back), the
/// slowest chunks, and how much of the reply is still tail (parsed again on each chunk) after the
/// 20th, the 100th and the last chunk.
async fn bench_streaming(cx: &mut TestAppContext, fixture: &str, source: &str) {
    eprintln!(
        "BENCH {fixture}: the rewind counts below are lower bounds (a rewind that freezes again in the same chunk leaves the cut where it was, and is not seen)"
    );
    let plain = run_reply(cx, source, false).await;
    let Run {
        handles,
        built,
        drawn,
        before,
        states,
        lengths,
    } = run_reply(cx, source, true).await;
    let stats = |label: &str, mut times: Vec<std::time::Duration>| {
        let total: std::time::Duration = times.iter().sum();
        times.sort();
        let at = |fraction: f64| times[((times.len() as f64 * fraction) as usize).min(times.len() - 1)];
        eprintln!(
            "BENCH {fixture} {label}: {} chunks of a {} byte reply, median {:?}, p95 {:?}, total {:?}",
            times.len(),
            source.len(),
            at(0.5),
            at(0.95),
            total
        );
    };
    let median = |times: &[std::time::Duration]| {
        let mut times = times.to_vec();
        times.sort();
        times[times.len() / 2]
    };
    for (label, times) in [("handle_event", &handles), ("full build", &before)] {
        let (first, last) = (median(&times[..20]), median(&times[times.len() - 20..]));
        eprintln!(
            "BENCH {label} first20/last20 ({fixture}): first20 median {first:?}, last20 median {last:?}, last/first {:.2}",
            last.as_secs_f64() / first.as_secs_f64()
        );
    }
    for step in [20usize, 100, 200] {
        if let (Some((cut, segments)), Some(length)) = (states.get(step - 1), lengths.get(step - 1)) {
            eprintln!(
                "BENCH {fixture} tail after chunk {step}: {} bytes still parsed again, {segments} frozen segments (cut {cut} of {length})",
                length - cut
            );
        }
    }
    let millisecond = std::time::Duration::from_millis(1);
    let tenth = std::time::Duration::from_micros(100);
    print_ratios(fixture, "a full build", &handles, &before, millisecond);
    print_ratios(fixture, "a full build", &handles, &before, tenth);
    print_ratios(fixture, "the plain handle_event (streaming off)", &handles, &plain.handles, tenth);
    // A rewind (a reply that reuses a footnote or a link label parses again from its first
    // mention) is a slow chunk among fast ones: the median hides it. A chunk rewinds when the cut
    // of the stream moved back, as the tests count them.
    let ratio = |index: usize| handles[index].as_secs_f64() / before[index].as_secs_f64();
    let rewinding = |index: usize| index > 0 && states.get(index).zip(states.get(index - 1)).is_some_and(|(now, last)| now.0 < last.0);
    let rewinds: Vec<usize> = (0..handles.len()).filter(|index| rewinding(*index)).collect();
    let worst_rewind = rewinds.iter().copied().filter(|index| before[*index] >= tenth).max_by(|a, b| ratio(*a).total_cmp(&ratio(*b)));
    match worst_rewind {
        Some(index) => eprintln!(
            "BENCH {fixture} {} chunks rewind (the cut moved back); worst ratio to a full build among those that take 0.1 ms or more: {:.2} at chunk {index}",
            rewinds.len(),
            ratio(index)
        ),
        None => eprintln!("BENCH {fixture} {} chunks rewind (the cut moved back)", rewinds.len()),
    }
    let mut slowest: Vec<usize> = (0..handles.len()).collect();
    slowest.sort_by(|a, b| handles[*b].cmp(&handles[*a]));
    for index in slowest.iter().take(8) {
        eprintln!(
            "BENCH {fixture} slow chunk {index}: handle_event {:?}, full build {:?}, ratio {:.2}; streaming off {:?}{}",
            handles[*index],
            before[*index],
            ratio(*index),
            plain.handles[*index],
            if rewinding(*index) { "; rewinds" } else { "" }
        );
    }
    stats("handle_event per chunk", handles);
    stats("handle_event per chunk, streaming off", plain.handles);
    stats("full build per chunk", before);
    stats("parse per chunk", built);
    stats("parse + frame per chunk", drawn);
}

/// Spec §7's streaming risk, measured by hand (`cargo nextest run -p sirio_ui --release
/// --run-ignored only --no-capture streaming_a_long_reply_costs`): the showcase document,
/// repeated to 40 KiB.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_a_long_reply_costs(cx: &mut TestAppContext) {
    let mut source = String::new();
    while source.len() < 40 * 1024 {
        source.push_str(SHOWCASE);
        source.push_str("\n\n");
    }
    bench_streaming(cx, "SHOWCASE", &source).await;
}

/// One fence that never closes, of 40 KiB of real code: its tail is the whole fence, so a chunk
/// costs what the fence is long. The code is a fixed file (`open-fence-code.txt`, copied from three
/// sources of this crate), so the before and after of a change, and the figures of an earlier one,
/// are on the same input. Run it alone, as the one above.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_an_open_fence_costs(cx: &mut TestAppContext) {
    const CODE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../Scripts/Tests/fixtures/markdown-documents/open-fence-code.txt"
    ));
    let source = format!("```rust\n{CODE}");
    bench_streaming(cx, "open fence", &source).await;
}

/// Spec 5.1's rewinds: 40 KiB in which many distinct link labels are each used in one paragraph
/// and defined three blocks later. Every definition is late, so each rewinds the stream to the
/// paragraph that used it: the median must stay flat, and a rewind shows as a slow chunk. The
/// labels have one width: a label that starts another (`topic-1` and `topic-10`) is taken for a
/// mention of it, and the reply holds everything between the two.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_late_definitions_cost(cx: &mut TestAppContext) {
    let mut source = String::new();
    let mut step = 0;
    while source.len() < 40 * 1024 {
        step += 1;
        source.push_str(&format!(
            "Step {step} reads [the notes on topic {step}][topic-{step:04}] before it goes on, with \
             some `code` and **emphasis** to give the line a few runs.\n\n\
             A second paragraph of step {step}, longer than the first, so that the blocks have a size \
             worth parsing.\n\n\
             - a list item of step {step}\n- and another\n\n\
             [topic-{step:04}]: https://example.com/topics/{step}\n\n"
        ));
    }
    bench_streaming(cx, "late definitions", &source).await;
}

/// A reply with no blank line (headings on their text, `Run:` on its fence), 40 KiB of it. A cut
/// falls only after a blank line, so nothing freezes: it costs what a full build costs.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_a_compact_reply_costs(cx: &mut TestAppContext) {
    bench_streaming(cx, "compact", &compact_source()).await;
}

/// 40 KiB with no blank line (headings on their text, `Run:` on its fence): nothing freezes.
pub(super) fn compact_source() -> String {
    let mut source = String::new();
    let mut step = 0;
    while source.len() < 40 * 1024 {
        step += 1;
        source.push_str(&format!(
            "## Step {step}\nRead `src/lib.rs` and **check** the [notes](https://example.com/{step}).\n\
             The second line carries on, with _emphasis_ and a list under it.\n\
             - the first point of step {step}\n- the second point, with `inline code`\n\
             Run:\n```sh\ncargo test -p sirio_ui step_{step}\n```\n"
        ));
    }
    source
}

/// What a whole build costs, three 40 KiB documents with display math in them, built thirty times
/// each (release, by hand: `cargo nextest run -p sirio_ui --release --run-ignored only
/// --no-capture building_a_whole_reply_costs`). Ely pairs the `$$` runs of a text by walking its
/// bytes outside the code and the markup between its texts, so the cost of that walk shows here.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn building_a_whole_reply_costs(_cx: &mut TestAppContext) {
    const CODE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../Scripts/Tests/fixtures/markdown-documents/open-fence-code.txt"
    ));
    let mut showcase = String::new();
    while showcase.len() < 40 * 1024 {
        showcase.push_str(SHOWCASE);
        showcase.push_str("\n\n");
    }
    let fence = format!("```rust\n{CODE}```\n\n$$\nx = 1\n$$\n");
    let mut spans = String::from("$$\ny = 2\n$$\n\n");
    let mut step = 0;
    while spans.len() < 40 * 1024 {
        step += 1;
        spans.push_str(&format!(
            "## Part {step}\n\nSet `a{step}` to `b{step}`, call `run({step})` and `stop()`, then read `x.y` and `z[{step}]` with *care*.\n\n\
             ```sh\ncargo build -p part_{step}\ncargo test -p part_{step}\necho done\n```\n\n"
        ));
    }
    let cwd = std::env::temp_dir();
    for (name, source) in [("SHOWCASE", &showcase), ("open fence", &fence), ("fences and code spans", &spans)] {
        let mut times: Vec<std::time::Duration> = (0..30)
            .map(|_| {
                let started = std::time::Instant::now();
                let _ = parse_linked_markdown(source, &cwd, &mut Default::default());
                started.elapsed()
            })
            .collect();
        times.sort();
        eprintln!(
            "BENCH whole build {name} ({} bytes): median {:?}, min {:?}, max {:?}",
            source.len(),
            times[times.len() / 2],
            times[0],
            times[times.len() - 1]
        );
    }
}

/// 40 KiB of prose in the shapes of a long answer (a heading, a paragraph with emphasis, code and
/// two `[the text][7]` references, a list, a fence, a table, a quote) that cites twenty numbered
/// references and twelve footnotes, `[^3]`. `late`: each definition follows the section after
/// the one that first used it; else every definition is at the very end.
pub(super) fn numbered_references_source(late: bool) -> String {
    let mut source = String::new();
    let mut seen_links = [false; 21];
    let mut pending: Vec<String> = Vec::new();
    let mut next_note = 1;
    let mut step = 0;
    let budget = if late { 40 * 1024 } else { 40 * 1024 - 2200 };
    while source.len() < budget {
        step += 1;
        let (a, b, c) = ((step * 7) % 20 + 1, (step * 3 + 5) % 20 + 1, (step * 11 + 2) % 20 + 1);
        let note = if step % 5 == 0 && next_note <= 12 {
            next_note += 1;
            Some(next_note - 1)
        } else {
            None
        };
        let note_ref = note.map(|k| format!("[^{k}]")).unwrap_or_default();
        // What the step before it used is defined after this step's blocks.
        let due: Vec<String> = std::mem::take(&mut pending);
        source.push_str(&format!(
            "## Section {step}\n\n\
             This section reads [the first source][{a}] against **{step} claims**, with *some* emphasis and `inline_code({step})`, and then [the specification][{b}]{note_ref} before it goes on to say what follows from it.\n\n\
             - a point that cites [a benchmark][{c}]\n- a second point, with a longer line that wraps because it keeps going for a while\n- a third point\n\n\
             ```rust\nfn step_{step}(input: &str) -> usize {{\n    input.len() + {step}\n}}\n```\n\n\
             | name | value |\n| --- | --- |\n| step | {step} |\n| refs | {a}, {b}, {c} |\n\n\
             > A closing remark for section {step}, in a quote, so that the reply has more than one kind of block.\n\n"
        ));
        for definition in due {
            source.push_str(&definition);
        }
        if late {
            for label in [a, b, c] {
                if !seen_links[label] {
                    seen_links[label] = true;
                    pending.push(format!("[{label}]: https://example.com/ref/{label}\n\n"));
                }
            }
            if let Some(k) = note {
                pending.push(format!("[^{k}]: The note {k}, which says a little more than the line did.\n\n"));
            }
        }
    }
    if late {
        for definition in pending {
            source.push_str(&definition);
        }
    } else {
        for label in 1..=20 {
            source.push_str(&format!("[{label}]: https://example.com/ref/{label}\n"));
        }
        source.push('\n');
        for k in 1..=12 {
            source.push_str(&format!("[^{k}]: The note {k}, which says a little more than the line did.\n\n"));
        }
    }
    source
}

/// Shape A of the label-heavy replies: `[1]` .. `[20]` and `[^1]` .. `[^12]`, all defined at the end.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_numbered_references_at_the_end_cost(cx: &mut TestAppContext) {
    let source = numbered_references_source(false);
    bench_streaming(cx, "numbered refs, definitions at the end", &source).await;
}

/// Shape B: the same prose, each definition after the section that first used it.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_numbered_references_late_cost(cx: &mut TestAppContext) {
    let source = numbered_references_source(true);
    bench_streaming(cx, "numbered refs, definitions late", &source).await;
}

/// Shape C: 130 labels `topic-1` .. `topic-130`, each used once and defined three blocks later,
/// numbered without padding, so that some begin others (`topic-1` and `topic-10`).
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_unpadded_labels_cost(cx: &mut TestAppContext) {
    let mut source = String::new();
    let mut step = 0;
    while source.len() < 40 * 1024 {
        step += 1;
        source.push_str(&format!(
            "Step {step} reads [the notes on topic {step}][topic-{step}] before it goes on, with some \
             `code` and **emphasis** to give the line a few runs.\n\n\
             A second paragraph of step {step}, longer than the first, so that the blocks have a size \
             worth parsing.\n\n\
             - a list item of step {step}\n- and another\n\n\
             [topic-{step}]: https://example.com/topics/{step}\n\n"
        ));
    }
    bench_streaming(cx, "unpadded labels", &source).await;
}

/// A reply with `\(` and `\[` math in it and a footnote that is cited again every fifteenth
/// section, so the stream rewinds on the way, deep. `html`: with a block of HTML at its head,
/// which makes the reply one that is read by parsing the tail first.
fn math_with_rewinds_source(html: bool) -> String {
    let mut source = String::new();
    if html {
        source.push_str("<div>\nA block of HTML at the head of the reply.\n</div>\n\n");
    }
    source.push_str("A first note[^n] to define.\n\n[^n]: The note, defined at once.\n\n");
    let mut step = 0;
    while source.len() < 40 * 1024 {
        step += 1;
        let note = if step % 15 == 0 { "[^n]" } else { "" };
        source.push_str(&format!(
            "## Section {step}\n\nEuler \\(e^{{i\\pi}}\\) says that step {step} is **linear**, see the note{note} and `inline_code({step})`.\n\n\
             - a point with \\(x_{{{step}}}\\) in it\n- another point of section {step}\n\n\
             ```rust\nfn step_{step}() -> usize {{ {step} }}\n```\n\n\
             \\[ y_{step} = \\sum_k a_k \\]\n\n"
        ));
    }
    source
}

/// `\(` math and rewinds, no HTML: read by the stream without parsing the tail first.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_math_with_rewinds_cost(cx: &mut TestAppContext) {
    let source = math_with_rewinds_source(false);
    bench_streaming(cx, "math and rewinds", &source).await;
}

/// `\(` math and rewinds and a block of HTML: the one kind of reply that parses its tail first.
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_math_and_html_with_rewinds_cost(cx: &mut TestAppContext) {
    let source = math_with_rewinds_source(true);
    bench_streaming(cx, "math, html and rewinds", &source).await;
}

// ---- a reply Ely's reader panics on -------------------------------------------------------

/// Delivers `text` as a chunk of the reply. How many texts the reader was asked to read to handle
/// it (a read of the frame that follows is not counted).
fn say(chat: &Entity<Chat>, cx: &mut VisualTestContext, text: &str) -> usize {
    chat.update(cx, |chat, cx| {
        let before = crate::markdown::reader_fault::strikes();
        chat.handle_event(sirio_chat::ChatEvent::AgentMessageChunk(text.into()), cx);
        crate::markdown::reader_fault::strikes() - before
    })
}

fn end_turn(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
    chat.update(cx, |chat, cx| {
        chat.handle_event(
            sirio_chat::ChatEvent::TurnEnded {
                stop_reason: "EndTurn".into(),
            },
            cx,
        );
    });
}

/// The document of the one reply the chat holds.
fn the_reply(chat: &Entity<Chat>, cx: &VisualTestContext) -> MarkdownBlocks {
    chat.read_with(&cx.cx, |chat, _| {
        let replies: Vec<_> = (chat.entries.iter())
            .filter_map(|entry| match entry {
                Entry::Assistant { document, .. } => Some(document.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(replies.len(), 1, "one reply grown in place");
        replies[0].clone()
    })
}

/// A reply whose reader panicked is plain text for the rest of its turn: a later chunk that
/// would make structure does not, since the reader is not asked again (the count of reads the
/// fault saw does not move), and the full text is read once when the turn ends.
#[gpui::test]
async fn a_reply_the_reader_panicked_on_stays_plain_text_until_its_turn_ends(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = offline_chat_view(cx);
    chat.update(cx, |chat, _| chat.streaming = true);

    let fault = crate::markdown::reader_fault::on();
    assert_eq!(say(&chat, cx, "first "), 1, "the first chunk asked the reader once, and it panicked");
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked);
    assert_eq!(reply.text.text, "first ");
    assert_eq!(reply.nodes.len(), 1);

    // A reader that was asked would read a heading here, or panic again.
    assert_eq!(say(&chat, cx, "**bold**\n\n# Title"), 0, "a flagged reply was parsed again");
    drop(fault);
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked, "the reader was not asked again");
    assert_eq!(reply.text.text, "first **bold**\n\n# Title");
    assert_eq!((reply.nodes.len(), reply.headings.len()), (1, 0));

    end_turn(&chat, cx);
    let reply = the_reply(&chat, cx);
    assert!(!reply.reader_panicked, "the turn's end read the whole reply");
    assert_eq!(reply.headings.len(), 1);
}

/// The same for a reply that has no stream (a chunk after Stop, a turn the CLI started): the
/// chunk arm looks at the flag before it picks a branch.
#[gpui::test]
async fn a_reply_without_a_stream_the_reader_panicked_on_is_not_parsed_again(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = offline_chat_view(cx);
    let fault = crate::markdown::reader_fault::on();
    assert_eq!(say(&chat, cx, "first "), 1);
    assert_eq!(say(&chat, cx, "**bold**\n\n# Title"), 0, "a flagged reply was parsed again");
    drop(fault);
    chat.read_with(&cx.cx, |chat, _| {
        assert!(matches!(
            chat.entries.last(),
            Some(Entry::Assistant { stream: None, .. })
        ));
    });
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked);
    assert_eq!(reply.text.text, "first **bold**\n\n# Title");
    assert_eq!((reply.nodes.len(), reply.headings.len()), (1, 0));
}

/// The read at the turn's end is one try: a reader that panics again leaves the text plain.
#[gpui::test]
async fn a_reply_the_reader_panics_on_at_its_turn_end_stays_plain_text(cx: &mut TestAppContext) {
    let (chat, cx) = offline_chat_view(cx);
    chat.update(cx, |chat, _| chat.streaming = true);
    let _fault = crate::markdown::reader_fault::on();
    say(&chat, cx, "first ");
    say(&chat, cx, "**bold**\n\n# Title");
    end_turn(&chat, cx);
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked);
    assert_eq!(reply.text.text, "first **bold**\n\n# Title");
    assert_eq!((reply.nodes.len(), reply.headings.len()), (1, 0));
}

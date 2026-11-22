//! A chat reply drawn through the Markdown pipeline and selected in the transcript's one
//! offset space: what a click leaves focused, what a drag opens, what the menu offers, and
//! that a reply cut at any byte still draws.

use gpui::{Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px, size};

use super::tests::{composer_is_focused, focus_composer, offline_chat_view, refresh_frame, right_click};
use super::*;

const SHOWCASE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../Scripts/Tests/fixtures/markdown-documents/showcase.md"
));

/// A 24 px square PNG: a picture with a size the pointer can hit.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR4nGNQaHhAFcQwatCoQaMGjRo0atCoQQNvEACD/WAuD5bdZgAAAABJRU5ErkJggg==";

/// A drawn chat holding `replies`, one reply each.
fn chat_with<'a>(
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

/// Spec §7's streaming risk, measured by hand (`cargo nextest run -p sirio_ui --run-ignored
/// only --no-capture streaming_a_long_reply_costs`): a reply of 40 KiB arriving in 200 chunks,
/// each chunk costing one parse (`handle_event`) and one drawn frame. The numbers are printed;
/// nothing is asserted, since a wall clock in a test is a flake (the baseline, bezel, is gone).
#[gpui::test]
#[ignore = "a measurement, run by hand"]
async fn streaming_a_long_reply_costs(cx: &mut TestAppContext) {
    let mut source = String::new();
    while source.len() < 40 * 1024 {
        source.push_str(SHOWCASE);
        source.push_str("\n\n");
    }
    let (chat, cx) = chat_with(cx, &[]);
    let (mut from, mut handles, mut built, mut drawn) = (0, Vec::new(), Vec::new(), Vec::new());
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
        handles.push(handled);
        built.push(parsed);
        drawn.push(started.elapsed());
        from = cut;
    }
    let stats = |label: &str, mut times: Vec<std::time::Duration>| {
        let total: std::time::Duration = times.iter().sum();
        times.sort();
        let at = |fraction: f64| times[((times.len() as f64 * fraction) as usize).min(times.len() - 1)];
        eprintln!(
            "BENCH {label}: {} chunks of a {} byte reply, median {:?}, p95 {:?}, total {:?}",
            times.len(),
            source.len(),
            at(0.5),
            at(0.95),
            total
        );
    };
    stats("handle_event per chunk", handles);
    stats("parse per chunk", built);
    stats("parse + frame per chunk", drawn);
}

// ---- a reply Ely's reader panics on -------------------------------------------------------

fn say(chat: &Entity<Chat>, cx: &mut VisualTestContext, text: &str) {
    chat.update(cx, |chat, cx| {
        chat.handle_event(sirio_chat::ChatEvent::AgentMessageChunk(text.into()), cx);
    });
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
/// would make structure does not, since the reader is not asked again, and the full text is
/// read once when the turn ends.
#[gpui::test]
async fn a_reply_the_reader_panicked_on_stays_plain_text_until_its_turn_ends(
    cx: &mut TestAppContext,
) {
    let (chat, cx) = offline_chat_view(cx);
    chat.update(cx, |chat, _| chat.streaming = true);

    let fault = crate::markdown::reader_fault::on();
    say(&chat, cx, "first ");
    drop(fault);
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked);
    assert_eq!(reply.text.text, "first ");
    assert_eq!(reply.nodes.len(), 1);

    // The fault is gone, and the reader would read a heading here.
    say(&chat, cx, "**bold**\n\n# Title");
    let reply = the_reply(&chat, cx);
    assert!(reply.reader_panicked, "the reader was not asked again");
    assert_eq!(reply.text.text, "first **bold**\n\n# Title");
    assert_eq!((reply.nodes.len(), reply.headings.len()), (1, 0));

    end_turn(&chat, cx);
    let reply = the_reply(&chat, cx);
    assert!(!reply.reader_panicked, "the turn's end read the whole reply");
    assert_eq!(reply.headings.len(), 1);
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

//! A long reply stands in for the top-level blocks that sit far outside the window (see
//! `markdown::cull`). The oracle is the same chat with culling off: whatever a reader can
//! reach must be what a full draw shows.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui::{Bounds, FollowMode, ListOffset};

use super::super::tests::{refresh_frame, right_click};
use super::*;
use crate::markdown::DrawnPart;
use crate::markdown::cull::probe;

/// SHOWCASE repeated to 16 KiB for the scenarios, which draw it in full for their oracle: about
/// forty blocks, still many windows tall. (The oracle streams 40 KiB, the bench's reply.)
fn short_reply() -> String {
    reply_of(16)
}

fn reply_of(kib: usize) -> String {
    let mut source = String::new();
    while source.len() < kib * 1024 {
        source.push_str(SHOWCASE);
        source.push_str("\n\n");
    }
    source
}

/// What a reader could reach after one draw.
struct Frame {
    item: Option<Bounds<Pixels>>,
    viewport: Bounds<Pixels>,
    parts: Vec<DrawnPart>,
    links: Vec<BTreeMap<String, String>>,
    copied: Option<String>,
    whole: String,
}

impl Frame {
    fn look(chat: &Chat) -> Self {
        let painted = chat.transcript_links.borrow();
        Frame {
            item: chat.list_state.bounds_for_item(0),
            viewport: chat.list_state.viewport_bounds(),
            parts: painted.iter().flat_map(|body| body.drawn_parts()).collect(),
            links: super::super::transcript_link_rows(&painted),
            copied: chat.selected_transcript_text(),
            whole: chat.transcript_text(),
        }
    }

    /// The parts a reader sees in the window.
    fn visible(&self) -> Vec<&DrawnPart> {
        (self.parts.iter())
            .filter(|part| part.bounds.intersects(&self.viewport))
            .collect()
    }
}

/// Whether two drawings of a text agree: the same part and highlight, laid out within a
/// thousandth of a pixel. The same-frame redraw lays the body out as its own root, so a text's
/// unsnapped bounds can differ from the normal frame's by float noise (seen at scale 1.5:
/// 33.333008 against 33.333984); the snapped boxes (the item) are equal exactly.
fn same_place(a: &DrawnPart, b: &DrawnPart) -> bool {
    let near = |x: Pixels, y: Pixels| f32::from(x - y).abs() < 0.01;
    a.part == b.part
        && a.start == b.start
        && a.selected == b.selected
        && near(a.bounds.origin.x, b.bounds.origin.x)
        && near(a.bounds.origin.y, b.bounds.origin.y)
        && near(a.bounds.size.width, b.bounds.size.width)
        && near(a.bounds.size.height, b.bounds.size.height)
}

/// Where a jump lands.
#[derive(Clone, Copy, Debug)]
enum Jump {
    Top,
    Middle,
}

/// The long reply followed at the tail until its far blocks are stood in for, then one frame
/// after `jump`, read straight from that frame, and what the culling did in it.
fn first_frame_after(
    cx: &mut TestAppContext,
    culling: bool,
    jump: Jump,
    scale: Option<f32>,
) -> (Frame, probe::Stats) {
    probe::culling(culling);
    let source = short_reply();
    let (chat, cx) = chat_with(cx, &[&source]);
    if let Some(scale) = scale {
        cx.simulate_scale_factor_change(scale);
    }
    chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
    for _ in 0..4 {
        refresh_frame(cx);
    }
    let before = probe::stats();
    probe::reset();
    let frame = cx.update(|window, cx| {
        let list = chat.read(cx).list_state.clone();
        let offset_in_item = match jump {
            Jump::Top => px(0.0),
            Jump::Middle => list.bounds_for_item(0).map_or(px(0.0), |item| item.size.height / 2.0),
        };
        list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item,
        });
        window.draw(cx).clear(cx);
        Frame::look(chat.read(cx))
    });
    let after = probe::stats();
    probe::culling(true);
    if culling {
        assert!(before.stood > 0, "the far blocks were stood in for before the jump: {before:?}");
    }
    (frame, after)
}

/// 10. A jump to blocks the last frame stood in for: the first frame after it paints every
/// block the window shows, because the body draws them again in the same frame (its heights are
/// exact at an unchanged width), never a blank frame that the next one repairs.
#[gpui::test]
async fn a_jump_to_culled_blocks_paints_every_visible_block_in_the_first_frame(
    cx: &mut TestAppContext,
) {
    for (jump, scale) in [(Jump::Top, None), (Jump::Middle, None), (Jump::Middle, Some(1.5))] {
        let (full, _) = first_frame_after(cx, false, jump, scale);
        let (culled, stats) = first_frame_after(cx, true, jump, scale);
        let at = format!("{jump:?} at {scale:?}");
        assert!(stats.redrawn > 0, "{at}: the jump met stand-ins and fixed them: {stats:?}");
        assert_eq!(stats.widened, 0, "{at}: the width did not change: {stats:?}");
        let visible = full.visible();
        assert!(visible.len() > 10, "{at}: the window shows text: {}", visible.len());
        for part in visible {
            assert!(
                culled.parts.iter().any(|drawn| same_place(drawn, part)),
                "{at}: a text the window shows was not drawn in the first frame: {part:?}"
            );
        }
        assert_eq!(culled.item, full.item, "{at}: the item's place");
    }
}

/// A PNG wider than the body, so the picture scales to the width and its height is not a whole
/// number of pixels.
fn wide_picture() -> String {
    use base64::Engine as _;
    let mut png = Vec::new();
    image::RgbaImage::new(1500, 517)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a png");
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// What a pass streams.
#[derive(Clone, Copy, Default)]
struct Setup {
    /// The reader scrolled to the top of the reply while it streams (else follows the tail).
    at_top: bool,
    /// The window's scale factor (the test window's own is 2.0).
    scale: Option<f32>,
    /// A picture scaled to the width after every repeat of the showcase.
    picture: bool,
}

/// One reader's view at a cut: with everything selected, and with a selection that starts in
/// the first third of the text and runs to the end.
struct Cut {
    all: Frame,
    some: Frame,
}

/// The bench's 200 cuts of the reply, the ones in `steps` visited: each takes the chunk since
/// the previous visited one, selects all, draws, then selects from a third of the way in, draws.
fn stream(cx: &mut TestAppContext, culling: bool, setup: Setup, steps: &[usize]) -> Vec<Cut> {
    probe::culling(culling);
    let mut source = String::new();
    let picture = setup.picture.then(wide_picture);
    while source.len() < 40 * 1024 {
        source.push_str(SHOWCASE);
        source.push_str("\n\n");
        if let Some(uri) = &picture {
            source.push_str(&format!("![wide]({uri})\n\n"));
        }
    }
    let (chat, cx) = chat_with(cx, &[]);
    if let Some(scale) = setup.scale {
        cx.simulate_scale_factor_change(scale);
        refresh_frame(cx);
    }
    let (mut from, mut cuts) = (0, Vec::new());
    for (visit, step) in steps.iter().enumerate() {
        if setup.at_top && visit == 1 {
            chat.update(cx, |chat, _| {
                chat.list_state.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.0),
                })
            });
        }
        let mut cut = (source.len() * step / 200).min(source.len());
        while !source.is_char_boundary(cut) {
            cut += 1;
        }
        chat.update(cx, |chat, cx| {
            let chunk = source[from..cut].into();
            chat.handle_event(sirio_chat::ChatEvent::AgentMessageChunk(chunk), cx);
            chat.select_all_transcript(cx);
        });
        from = cut;
        refresh_frame(cx);
        let all = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
        chat.update(cx, |chat, cx| {
            let text = chat.transcript_text();
            let mut start = text.len() / 3;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            chat.transcript_selection = Some(TranscriptSelection {
                anchor: start,
                head: text.len(),
            });
            cx.notify();
        });
        refresh_frame(cx);
        let some = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
        cuts.push(Cut { all, some });
    }
    probe::culling(true);
    cuts
}

/// What culling must not change in `culled` against `full`, one frame of a cut.
fn same_to_the_reader(culled: &Frame, full: &Frame, at: &str) {
    // 1. The item's height and place, to the pixel.
    assert_eq!(culled.item, full.item, "{at}: the item's height and origin");
    // 4. Every text drawn is the full draw's, with the same part, start, bounds and highlight,
    // and every text the window shows is drawn.
    for part in &culled.parts {
        assert!(
            full.parts.contains(part),
            "{at}: a drawn text differs from the full draw's: {part:?}"
        );
    }
    for part in full.visible() {
        assert!(culled.parts.contains(part), "{at}: a text in the window is not drawn: {part:?}");
    }
    // 5. The links in the window are the full draw's, and no link is one it does not have.
    for row in &culled.links {
        assert!(full.links.contains(row), "{at}: a link row the full draw lacks: {row:?}");
    }
    let in_window = |frame: &Frame, row: &BTreeMap<String, String>| {
        let y = px(row["y"].parse::<f32>().unwrap());
        y >= frame.viewport.top() && y <= frame.viewport.bottom()
    };
    for row in full.links.iter().filter(|row| in_window(full, row)) {
        assert!(culled.links.contains(row), "{at}: a link in the window is missing: {row:?}");
    }
}

/// Culling on against off at `steps`: checks 1 to 5 of every cut.
fn oracle(cx: &mut TestAppContext, setup: Setup, steps: &[usize]) {
    let off = stream(cx, false, setup, steps);
    probe::reset();
    let on = stream(cx, true, setup, steps);
    let stats = probe::stats();
    assert!(stats.stood > 0, "culling stood in for blocks: {stats:?}");
    for ((on, off), step) in on.iter().zip(&off).zip(steps) {
        let at = format!("cut {step}");
        same_to_the_reader(&on.all, &off.all, &format!("{at}, all selected"));
        same_to_the_reader(&on.some, &off.some, &format!("{at}, selected from a third"));
        // 2. The whole transcript's Select All copies the whole transcript.
        assert_eq!(on.all.whole, off.all.whole, "{at}: the transcript");
        assert_eq!(
            on.all.copied.as_deref(),
            Some(on.all.whole.as_str()),
            "{at}: Select All and copy, culled"
        );
        assert_eq!(off.all.copied, on.all.copied, "{at}: Select All and copy");
        assert_eq!(on.some.copied, off.some.copied, "{at}: a selection from a third");
    }
}

/// Every Nth of the bench's 200 cuts, and the first. The default run takes few (a debug
/// draw of 40 KiB is slow, and nextest ends a test after three minutes); the ignored test
/// takes all.
fn sampled(every: usize) -> Vec<usize> {
    let mut steps: Vec<usize> = (1..=200).filter(|step| step % every == 0).collect();
    if steps.first() != Some(&1) {
        steps.insert(0, 1);
    }
    steps
}

#[gpui::test]
async fn culling_changes_nothing_a_reader_can_reach_at_the_tail(cx: &mut TestAppContext) {
    let started = std::time::Instant::now();
    oracle(cx, Setup::default(), &sampled(40));
    eprintln!("oracle tail: {:?}", started.elapsed());
}

#[gpui::test]
async fn culling_changes_nothing_a_reader_can_reach_at_the_top(cx: &mut TestAppContext) {
    let started = std::time::Instant::now();
    oracle(cx, Setup { at_top: true, ..Setup::default() }, &sampled(40));
    eprintln!("oracle top: {:?}", started.elapsed());
}

/// The scale factor 1.5 (a Windows 150% or a scaled macOS display): heights are not whole
/// logical pixels.
#[gpui::test]
async fn culling_changes_nothing_a_reader_can_reach_at_scale_1_5(cx: &mut TestAppContext) {
    oracle(cx, Setup { scale: Some(1.5), ..Setup::default() }, &sampled(40));
}

/// A picture scaled to the width has a fractional height, at a fractional scale too.
#[gpui::test]
async fn culling_changes_nothing_a_reader_can_reach_with_pictures_scaled_to_the_width(
    cx: &mut TestAppContext,
) {
    oracle(cx, Setup { scale: Some(1.5), picture: true, ..Setup::default() }, &sampled(40));
}

/// All 200 cuts of the bench: by hand, in release (`cargo nextest run -p sirio_ui
/// --cargo-profile release --run-ignored only every_cut`).
#[gpui::test]
#[ignore = "every cut of the bench, minutes in debug: run by hand in release"]
async fn culling_changes_nothing_at_every_cut_of_the_bench(cx: &mut TestAppContext) {
    let steps: Vec<usize> = (1..=200).collect();
    let started = std::time::Instant::now();
    oracle(cx, Setup::default(), &steps);
    oracle(cx, Setup { at_top: true, ..Setup::default() }, &steps);
    oracle(cx, Setup { scale: Some(1.5), picture: true, ..Setup::default() }, &steps);
    eprintln!("oracle, every cut: {:?}", started.elapsed());
}

/// A reply of one fence or table in front of many paragraphs, for the sideways tests.
fn wide_then_long(first: &str) -> String {
    let mut source = format!("{first}\n\n");
    for line in 0..300 {
        source.push_str(&format!("Paragraph {line} with a few words in it.\n\n"));
    }
    source
}

/// Where the thumb of the first sideways frame sits against its frame, if both are drawn.
fn thumb(cx: &mut VisualTestContext) -> Option<f32> {
    let frame = cx.debug_bounds("markdown-frame-0")?;
    let bar = cx.debug_bounds("markdown-bar-0")?;
    Some(f32::from(bar.left() - frame.left()))
}

fn scroll_to_top(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
    chat.update(cx, |chat, _| {
        chat.list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        })
    });
    for _ in 0..3 {
        refresh_frame(cx);
    }
}

/// 7. A frame scrolled sideways keeps its offset through being stood in for, in the shapes a
/// frame comes in: a fence, a table, a fence in a list item and a fence in a quote.
#[gpui::test]
async fn a_frame_scrolled_sideways_keeps_its_offset_through_a_cull(cx: &mut TestAppContext) {
    let wide = "x".repeat(400);
    let cases = [
        ("a fence", format!("```json\n{{\"key\": \"{wide}\"}}\n```")),
        ("a table", format!("| col | other |\n|---|---|\n| a | {wide} |")),
        (
            "a fence in a list item",
            format!("- item\n\n  ```json\n  {{\"key\": \"{wide}\"}}\n  ```"),
        ),
        ("a fence in a quote", format!("> ```json\n> {{\"key\": \"{wide}\"}}\n> ```")),
    ];
    for (what, first) in cases {
        let source = wide_then_long(&first);
        let (chat, cx) = chat_with(cx, &[&source]);
        scroll_to_top(&chat, cx);
        let frame = cx
            .debug_bounds("markdown-frame-0")
            .unwrap_or_else(|| panic!("{what}: drawn at the top"));
        cx.simulate_event(gpui::ScrollEvent {
            position: frame.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(-300.0), px(0.0))),
            ..Default::default()
        });
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let scrolled = thumb(cx);
        assert!(
            scrolled.is_some_and(|x| x > 1.0),
            "{what}: the wheel moved the thumb: {scrolled:?}"
        );
        // Away to the tail: the frame is far above the window and stood in for.
        probe::reset();
        chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        assert!(probe::stats().last > 0, "{what}: blocks were stood in for while away");
        assert!(
            cx.debug_bounds("markdown-frame-0").is_none(),
            "{what}: the frame is not drawn while away"
        );
        scroll_to_top(&chat, cx);
        assert_eq!(thumb(cx), scrolled, "{what}: the offset survived the cull");
    }
}

/// A long reply drawn and followed at the tail until its far blocks are stood in for.
fn at_the_tail<'a>(
    cx: &'a mut TestAppContext,
    culling: bool,
) -> (Entity<Chat>, &'a mut VisualTestContext) {
    probe::culling(culling);
    let source = short_reply();
    let (chat, cx) = chat_with(cx, &[&source]);
    chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
    for _ in 0..4 {
        refresh_frame(cx);
    }
    (chat, cx)
}

/// What a drag from a text in the window to above the window and back leaves, and what the
/// reply's body answers for a pointer `far` pixels above the window.
struct Dragged {
    selection: Option<(usize, usize)>,
    /// What the body says for a pointer that far above the window's top, by distance.
    above: Vec<(f32, Option<usize>)>,
    /// The offset of the first text drawn.
    first_drawn: Option<usize>,
}

fn drag_and_probe(cx: &mut TestAppContext, culling: bool) -> Dragged {
    let (chat, cx) = at_the_tail(cx, culling);
    let frame = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
    // The last text the window shows, from near its bottom to near its top.
    let last = frame.visible().last().expect("the window shows text").bounds;
    let shown = last.intersect(&frame.viewport);
    let (from, back) = (
        point(shown.center().x, shown.bottom() - px(4.0)),
        point(shown.center().x, shown.top() + px(4.0)),
    );
    let outside = point(from.x, frame.viewport.top() - px(300.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(back, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(back, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(back, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let selection = chat.read_with(&cx.cx, |chat, _| {
        (chat.transcript_selection.as_ref()).map(|selection| (selection.anchor, selection.head))
    });
    let (above, first_drawn) = chat.read_with(&cx.cx, |chat, _| {
        let painted = chat.transcript_links.borrow();
        let body = &painted[0];
        let above = [100.0, 400.0, 3000.0]
            .into_iter()
            .map(|far| (far, body.offset_at(point(from.x, frame.viewport.top() - px(far)))))
            .collect();
        let first = body.drawn_parts().first().map(|part| part.start);
        (above, first)
    });
    probe::culling(true);
    Dragged { selection, above, first_drawn }
}

/// 6. A drag from a text in the window to above the window and back selects what it would
/// without culling, and a pointer within the margin above the window finds the text a full
/// draw finds. Past the margin the blocks are not drawn, and the head clamps to the first text
/// that is (an accepted change: the transcript has no drag autoscroll, so nothing reaches that
/// far).
#[gpui::test]
async fn a_drag_past_the_window_top_and_back_selects_what_it_would_and_clamps_past_the_margin(
    cx: &mut TestAppContext,
) {
    let full = drag_and_probe(cx, false);
    let culled = drag_and_probe(cx, true);
    assert!(full.selection.is_some_and(|(a, h)| a != h), "the drag selected: {:?}", full.selection);
    assert_eq!(culled.selection, full.selection, "the dragged selection");
    for ((far, culled_at), (_, full_at)) in culled.above.iter().zip(&full.above).take(2) {
        assert_eq!(culled_at, full_at, "a pointer {far} px above the window, within the margin");
    }
    let (far, culled_at) = culled.above[2];
    let (_, full_at) = full.above[2];
    assert_ne!(
        culled_at, full_at,
        "{far} px above is past the margin: the full draw reaches text there"
    );
    assert_eq!(
        culled_at, culled.first_drawn,
        "past the margin the head clamps to the first text drawn"
    );
}

/// The bodies' pass over blocks with `width` of window and one frame drawn by hand.
fn draw_now(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> Frame {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        Frame::look(chat.read(cx))
    })
}

/// Frames the window schedules and does not yet run.
fn scheduled(cx: &mut VisualTestContext) -> usize {
    cx.update(|window, cx| window.simulate_next_frame(cx))
}

/// 8. A width change with blocks stood in for: heights were taken at the old width, so the
/// first frame after it draws everything (nothing visible is blank), and one more frame is
/// scheduled, which measures at the new width: the item is then as tall as a full draw's.
#[gpui::test]
async fn a_width_change_draws_everything_once_and_settles_in_one_more_frame(
    cx: &mut TestAppContext,
) {
    let mut results = Vec::new();
    for culling in [false, true] {
        let (chat, cx) = at_the_tail(cx, culling);
        probe::reset();
        // The resize draws the frame; what it left is the first frame after the change.
        cx.simulate_resize(size(px(640.0), px(700.0)));
        let first = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
        let stats = probe::stats();
        let more = scheduled(cx);
        refresh_frame(cx);
        refresh_frame(cx);
        let settled = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
        results.push((first, stats, more, settled));
        probe::culling(true);
    }
    let (_, _, _, full) = &results[0];
    let (first, stats, more, settled) = &results[1];
    assert!(stats.widened >= 1, "the first frame drew every block again: {stats:?}");
    assert_eq!(stats.per_pass.first(), Some(&0), "nothing was stood in for in it: {stats:?}");
    assert!(!first.visible().is_empty(), "the window shows text");
    // The full draw's frames are the sideways bars' own requests; the culled body asks for one
    // more, for the new width's heights.
    let full_more = results[0].2;
    assert!(*more > full_more, "the culled body asked for a frame: {more} against {full_more}");
    assert_eq!(
        settled.item, full.item,
        "the item is as tall as a full draw's after one more frame"
    );
}

/// 9. A change of the interface's text size, or of the fonts, forgets every height: the same
/// frame draws everything, and the item is as tall as a full draw's.
#[gpui::test]
async fn a_change_of_text_size_or_font_forgets_every_height(cx: &mut TestAppContext) {
    type Change = fn(&mut sirio_theme::Theme);
    let changes: [(&str, Change); 3] = [
        ("a larger text size", |theme| {
            theme.typography.base_size += px(3.0);
            theme.typography.code_size += px(2.0);
        }),
        ("another interface font", |theme| theme.typography.ui_family = "Noto Serif"),
        ("another code font", |theme| theme.typography.code_family = "DejaVu Sans Mono"),
    ];
    for (what, change) in changes {
        let mut results = Vec::new();
        for culling in [false, true] {
            let (chat, cx) = at_the_tail(cx, culling);
            probe::reset();
            // The theme is the app's, shared by the runs: put it back afterwards.
            let original = cx.update(|_, cx| *cx.global::<sirio_theme::Theme>());
            cx.update(|_, cx| {
                let mut theme = original;
                change(&mut theme);
                cx.set_global(theme);
            });
            let stats = probe::stats();
            refresh_frame(cx);
            let settled = chat.read_with(&cx.cx, |chat, _| Frame::look(chat));
            results.push((stats, settled));
            cx.update(|_, cx| cx.set_global(original));
            probe::culling(true);
        }
        let (_, full) = &results[0];
        let (stats, settled) = &results[1];
        assert_eq!(
            stats.per_pass.first(),
            Some(&0),
            "{what}: the first frame after drew every block: {stats:?}"
        );
        assert_eq!(settled.item, full.item, "{what}: no stale height");
    }
}

/// 11. A closed block rewritten after the blocks that follow it were drawn (a later chunk can
/// do it: a setext underline, a lazy continuation, a reference definition): the block is laid
/// out again and what follows is placed by its new height, so a jump to those blocks shows
/// what a full draw shows.
#[gpui::test]
async fn a_rewritten_closed_block_is_laid_out_again_and_the_blocks_after_it_are_placed_anew(
    cx: &mut TestAppContext,
) {
    let tall = (0..9).map(|n| format!("line {n} of a block that was one line")).collect::<Vec<_>>();
    let mut after = Vec::new();
    for culling in [false, true] {
        let mut source = String::from("intro\n\none line\n\n");
        for n in 0..200 {
            source.push_str(&format!("Paragraph {n} with a few words in it.\n\n"));
        }
        let (chat, cx) = chat_with(cx, &[&source]);
        probe::culling(culling);
        chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
        for _ in 0..4 {
            refresh_frame(cx);
        }
        // The second block, far above the window, becomes nine lines.
        let text = source.replace("one line", &tall.join("  \n"));
        chat.update(cx, |chat, _| {
            chat.entries[0] = Entry::Assistant {
                document: parse_chat_markdown(&text),
                text: text.into(),
            };
        });
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let grown = chat.read_with(&cx.cx, |chat, _| chat.list_state.bounds_for_item(0));
        // A jump to the middle meets blocks whose places moved with it.
        let frame = cx.update(|window, cx| {
            let list = chat.read(cx).list_state.clone();
            let half = list.bounds_for_item(0).map_or(px(0.0), |item| item.size.height / 2.0);
            list.scroll_to(ListOffset { item_ix: 0, offset_in_item: half });
            window.draw(cx).clear(cx);
            Frame::look(chat.read(cx))
        });
        after.push((grown, frame));
        probe::culling(true);
    }
    let (full_height, full) = &after[0];
    let (height, culled) = &after[1];
    assert_eq!(height, full_height, "the item is as tall as a full draw's after the rewrite");
    assert!(full_height.is_some());
    for part in full.visible() {
        assert!(
            culled.parts.contains(part),
            "a text after the rewritten block is not where it was drawn: {part:?}"
        );
    }
}

/// 12. A link in a block that was stood in for opens once, through the click gate, when the
/// block is drawn again; a right press on it offers the link's items.
#[gpui::test]
async fn a_link_in_a_block_drawn_again_opens_once_and_offers_its_menu(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir();
    std::fs::write(dir.join("cull-link-test.rs"), "fn main() {}\n").expect("a file to link to");
    let mut source = String::from("see `cull-link-test.rs` first\n\n");
    for n in 0..300 {
        source.push_str(&format!("Paragraph {n} with a few words in it.\n\n"));
    }
    let (chat, cx) = chat_with(cx, &[]);
    chat.update(cx, |chat, _| {
        let document = parse_linked_markdown(&source, &dir, &mut Default::default());
        chat.push_entry(Entry::Assistant { text: source.clone().into(), document });
    });
    refresh_frame(cx);
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, app| {
        app.subscribe(&chat, move |_, event: &ChatEvent, _| {
            if let ChatEvent::OpenFileAt { path, .. } = event {
                seen.borrow_mut().push(path.clone());
            }
        })
        .detach();
    });
    chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
    probe::reset();
    for _ in 0..3 {
        refresh_frame(cx);
    }
    assert!(probe::stats().last > 0, "the link's block was stood in for");
    scroll_to_top(&chat, cx);
    let (spot, _) = link_point(&chat, cx, "cull-link-test.rs");
    click(cx, spot);
    assert_eq!(opened.borrow().len(), 1, "the link opened once: {:?}", opened.borrow());
    right_click(cx, spot);
    refresh_frame(cx);
    assert!(cx.debug_bounds("transcript-context-open-link").is_some(), "Open link is offered");
    assert!(
        cx.debug_bounds("transcript-context-copy-link").is_some(),
        "Copy link address is offered"
    );
    let _ = std::fs::remove_file(dir.join("cull-link-test.rs"));
}

/// Frames scheduled while the reply streams at the tail, and whether any are left after it
/// ends.
fn scheduled_while_streaming(cx: &mut TestAppContext, culling: bool) -> (usize, usize) {
    probe::culling(culling);
    let source = short_reply();
    let (chat, cx) = chat_with(cx, &[]);
    let (mut from, mut total) = (0, 0);
    for step in (8..=200).step_by(8) {
        let mut cut = (source.len() * step / 200).min(source.len());
        while !source.is_char_boundary(cut) {
            cut += 1;
        }
        chat.update(cx, |chat, cx| {
            let chunk = source[from..cut].into();
            chat.handle_event(sirio_chat::ChatEvent::AgentMessageChunk(chunk), cx);
        });
        from = cut;
        // Run what is scheduled until it stops asking, counting each request.
        for _ in 0..10 {
            let more = scheduled(cx);
            total += more;
            if more == 0 {
                break;
            }
            cx.run_until_parked();
        }
        refresh_frame(cx);
    }
    // The stream ended: the window asks for nothing more.
    let mut left = 0;
    for _ in 0..5 {
        refresh_frame(cx);
        left += scheduled(cx);
    }
    probe::culling(true);
    (total, left)
}

/// 13. With culling on and nothing missed, no more frames are scheduled than with it off, and
/// a window that ended its stream with blocks stood in for is quiet (no idle redraw loop).
#[gpui::test]
async fn culling_schedules_no_more_frames_and_leaves_no_idle_redraw_loop(cx: &mut TestAppContext) {
    let (off, off_left) = scheduled_while_streaming(cx, false);
    probe::reset();
    let (on, on_left) = scheduled_while_streaming(cx, true);
    assert!(probe::stats().stood > 0, "culling engaged");
    assert!(on <= off, "frames scheduled while streaming: {on} culled against {off} full");
    assert_eq!(
        (off_left, on_left),
        (0, 0),
        "frames still scheduled after the stream: full {off_left}, culled {on_left}"
    );
    eprintln!("frames scheduled while streaming: culled {on}, full {off}");
}

/// Accepted change (ruling 6): the link rows of a reply list only the links of the blocks
/// that are drawn, those near the window; a link far above it is not listed, where a full
/// draw lists it at off-screen coordinates.
#[gpui::test]
async fn link_rows_list_only_the_links_of_drawn_blocks(cx: &mut TestAppContext) {
    let mut source = String::from("see [the far docs](https://x.test/far) first\n\n");
    for n in 0..300 {
        source.push_str(&format!("Paragraph {n} with a [link {n}](https://x.test/{n}) in it.\n\n"));
    }
    let mut found = Vec::new();
    for culling in [false, true] {
        probe::culling(culling);
        let (chat, cx) = chat_with(cx, &[&source]);
        chat.update(cx, |chat, _| chat.list_state.set_follow_mode(FollowMode::Tail));
        for _ in 0..4 {
            refresh_frame(cx);
        }
        let labels: Vec<String> = chat.read_with(&cx.cx, |chat, _| {
            super::super::transcript_link_rows(&chat.transcript_links.borrow())
                .into_iter()
                .map(|row| row["label"].clone())
                .collect()
        });
        found.push(labels);
        probe::culling(true);
    }
    let (full, culled) = (&found[0], &found[1]);
    assert!(full.iter().any(|label| label == "the far docs"), "a full draw lists the far link");
    assert!(
        !culled.iter().any(|label| label == "the far docs"),
        "a stood-in block's link is not listed"
    );
    assert!(culled.len() < full.len() && culled.iter().all(|label| full.contains(label)));
    assert!(culled.iter().any(|label| label == "link 299"), "the links near the window are listed");
}

//! A body drawn through Ely and selected through Sirio's own model: what a drag copies,
//! what a click opens, what the menu offers, and that what is drawn is what was counted.
//!
//! Every test hosts the body under `text_selection::testing::host`, the stand-in for the
//! workspace root, and drives it with real mouse events and a real Ctrl+C.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    Context, InteractiveElement, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement, Pixels, Point, Render, Styled, TestAppContext, VisualTestContext, Window, div,
    point, prelude::*, px, size,
};

use super::*;
use crate::markdown::menu::MarkdownMenuItem;
use crate::markdown::{PartKind, Policy, Surface, build};
use crate::text_selection::testing::{self, SENTINEL};

/// A 24 px square PNG: a picture with a size the pointer can hit.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABgAAAAYCAIAAABvFaqvAAAAH0lEQVR4nGNQaHhAFcQwatCoQaMGjRo0atCoQQNvEACD/WAuD5bdZgAAAABJRU5ErkJggg==";

fn blocks(source: &str) -> MarkdownBlocks {
    build(
        source,
        &mut Policy {
            surface: Surface::Preview,
            base: None,
            diagrams: None,
            file_links: None,
        },
    )
}

/// One body, a counter of the presses that reach its host, and what the menu was handed.
struct Page {
    blocks: MarkdownBlocks,
    opened: Rc<RefCell<Vec<String>>>,
    menus: Rc<RefCell<Vec<MenuPress>>>,
    layouts: Rc<RefCell<Option<BodyLayouts>>>,
    host_presses: Rc<Cell<usize>>,
    /// A fixed width, or the text's own.
    width: Option<f32>,
}

#[derive(Clone)]
struct Probes {
    opened: Rc<RefCell<Vec<String>>>,
    menus: Rc<RefCell<Vec<MenuPress>>>,
    layouts: Rc<RefCell<Option<BodyLayouts>>>,
    host_presses: Rc<Cell<usize>>,
}

impl Probes {
    fn new() -> Self {
        Self {
            opened: Rc::default(),
            menus: Rc::default(),
            layouts: Rc::default(),
            host_presses: Rc::default(),
        }
    }

    fn page(&self, source: &str, width: Option<f32>) -> Page {
        Page {
            blocks: blocks(source),
            opened: self.opened.clone(),
            menus: self.menus.clone(),
            layouts: self.layouts.clone(),
            host_presses: self.host_presses.clone(),
            width,
        }
    }
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let opened = self.opened.clone();
        let links: LinkHandler =
            Rc::new(move |target, _, _| opened.borrow_mut().push(target.to_string()));
        let menus = self.menus.clone();
        let view = markdown_view(
            &self.blocks,
            SelectionHost::Run { id: "body".into() },
            links,
            ViewFlags::default(),
        )
        .on_menu(Rc::new(move |press, _, _| menus.borrow_mut().push(press)));
        *self.layouts.borrow_mut() = Some(view.layouts());
        let presses = self.host_presses.clone();
        div().size_full().flex().flex_row().child(
            div()
                .debug_selector(|| "page".into())
                .when_some(self.width, |page, width| page.w(px(width)))
                .on_mouse_down(MouseButton::Left, move |_, _, _| {
                    presses.set(presses.get() + 1)
                })
                .child(view),
        )
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    source: &str,
    width: Option<f32>,
) -> (Probes, &'a mut VisualTestContext) {
    cx.update(crate::ely::init);
    let probes = Probes::new();
    let page = probes.page(source, width);
    let (_, _, vcx) = testing::host(cx, move |_, _| page);
    vcx.simulate_resize(size(px(800.0), px(600.0)));
    vcx.run_until_parked();
    (probes, vcx)
}

const TEXT: &str = "see docs now";
const LINKED: &str = "see [docs](https://x.test) now";

/// The one-line page of [`TEXT`], with the width of one character.
fn line<'a>(
    cx: &'a mut TestAppContext,
    source: &str,
) -> (Probes, &'a mut VisualTestContext, f32) {
    let (probes, vcx) = open(cx, source, None);
    let width = f32::from(vcx.debug_bounds("page").expect("the page renders").size.width);
    (probes, vcx, width / TEXT.chars().count() as f32)
}

fn at(cx: &mut VisualTestContext, chars: f32, char_width: f32) -> Point<Pixels> {
    let bounds = cx.debug_bounds("page").unwrap();
    point(bounds.left() + px(chars * char_width), bounds.top() + px(6.0))
}

fn mouse(
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    button: MouseButton,
    click_count: usize,
) {
    cx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button,
        click_count,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button,
        click_count,
    });
}

/// Drags across the whole page, from just inside its top-left to past its bottom-right.
fn drag_across(cx: &mut VisualTestContext) -> Option<String> {
    let bounds = cx.debug_bounds("page").expect("the page renders");
    testing::reset_clipboard(cx);
    testing::drag(
        cx,
        point(bounds.left() + px(1.0), bounds.top() + px(6.0)),
        point(bounds.right() + px(100.0), bounds.bottom() - px(4.0)),
    );
    cx.simulate_keystrokes("ctrl-c");
    testing::clipboard(cx)
}

// Ways the link path could fail the host:
// - a click of the middle or right button opens (Ely's `InteractiveText` arms on any
//   button, so the click gate is all that stops it);
// - a left click reaches the system opener (the view passes no opener of its own);
// - the check is too wide and an `https:` link stops opening.
// Each case names its button in the address, so a value left by an earlier case cannot
// satisfy a later one.
#[gpui::test]
async fn only_a_file_link_never_reaches_the_system_opener(cx: &mut TestAppContext) {
    let tab = char::from(9u8);
    let cases = [
        ("lower", "see [docs](file:///tmp/a.rs) now".to_string()),
        ("upper", "see [docs](FILE:///tmp/a.rs) now".to_string()),
        ("space", "see [docs](< file:///tmp/a.rs>) now".to_string()),
        ("tab", format!("see [docs](<{tab}file:///tmp/a.rs>) now")),
        ("https", "see [docs](https://x.test/BTN) now".to_string()),
    ];
    for (label, source) in cases {
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            let source = source.replace("BTN", &format!("{button:?}"));
            let (probes, vcx, char_width) = line(cx, &source);
            let before = vcx.cx.opened_url();
            let spot = at(vcx, 5.5, char_width);
            mouse(vcx, spot, button, 1);
            assert!(
                vcx.cx.opened_url() == before,
                "link case {label}, button {button:?}: the system opener was reached"
            );
            let heard = probes.opened.borrow().len();
            let expected = usize::from(button == MouseButton::Left);
            assert!(
                heard == expected,
                "link case {label}, button {button:?}: the surface's opener heard {heard} clicks, want {expected}"
            );
        }
    }
}

/// Dragging across a link selects it; it must not also open it.
#[gpui::test]
async fn a_drag_across_a_link_selects_it_without_following_it(cx: &mut TestAppContext) {
    let (probes, cx, char_width) = line(cx, LINKED);
    testing::reset_clipboard(cx);

    // From the start to the middle of "docs": the release lands on the link, which is
    // exactly where a buggy release would open it.
    let (from, to) = (at(cx, 0.0, char_width), at(cx, 6.0, char_width));
    testing::drag(cx, from, to);
    cx.simulate_keystrokes("ctrl-c");

    assert_eq!(testing::clipboard(cx).as_deref(), Some("see do"));
    assert!(
        probes.opened.borrow().is_empty(),
        "a release that ends a drag is not a click: {:?}",
        probes.opened.borrow()
    );

    // A drag that starts on the link and ends on it is the case Ely's own link text would
    // take for a click: press and release are on the same link.
    testing::reset_clipboard(cx);
    let (from, to) = (at(cx, 4.2, char_width), at(cx, 7.4, char_width));
    testing::drag(cx, from, to);
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(testing::clipboard(cx).as_deref(), Some("doc"));
    assert!(
        probes.opened.borrow().is_empty(),
        "a drag within one link selects it and follows nothing: {:?}",
        probes.opened.borrow()
    );
}

#[gpui::test]
async fn a_plain_click_on_a_link_still_follows_it(cx: &mut TestAppContext) {
    let (probes, cx, char_width) = line(cx, LINKED);
    testing::reset_clipboard(cx);

    let spot = at(cx, 5.5, char_width);
    cx.simulate_click(spot, Modifiers::none());

    assert_eq!(probes.opened.borrow().as_slice(), ["https://x.test"]);
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(
        testing::clipboard(cx).as_deref(),
        Some(SENTINEL),
        "a click selects nothing"
    );
}

#[gpui::test]
async fn double_click_selects_a_word_and_triple_click_the_paragraph(cx: &mut TestAppContext) {
    let (probes, cx, char_width) = line(cx, LINKED);
    for (count, expected) in [(2, "see"), (3, TEXT)] {
        testing::reset_clipboard(cx);
        let spot = at(cx, 1.5, char_width);
        testing::press(cx, spot, count);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            testing::clipboard(cx).as_deref(),
            Some(expected),
            "click count {count}"
        );
    }
    assert!(probes.opened.borrow().is_empty());
}

/// The word is the one the pointer is *on*. In the right half of "see"'s last letter the
/// nearer caret is the space after it; taking that would select the space.
#[gpui::test]
async fn a_double_click_on_the_right_half_of_a_words_last_letter_takes_that_word(
    cx: &mut TestAppContext,
) {
    let (_, cx, char_width) = line(cx, LINKED);
    testing::reset_clipboard(cx);
    let spot = at(cx, 2.7, char_width);
    testing::press(cx, spot, 2);
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(testing::clipboard(cx).as_deref(), Some("see"));
}

#[gpui::test]
async fn a_drag_over_emoji_and_cjk_copies_whole_characters(cx: &mut TestAppContext) {
    let source = ":tada: 漢字 e\u{301}";
    let (_, cx) = open(cx, source, None);
    let drawn = blocks(source).text.text.clone();
    assert!(
        drawn.contains('漢') && !drawn.contains(":tada:"),
        "the shortcode is the picture it names: {drawn:?}"
    );

    assert_eq!(drag_across(cx).as_deref(), Some(drawn.as_str()));

    // A drag that ends inside the text never copies half a character or a letter
    // without the mark that follows it in the drawn text's own units.
    let bounds = cx.debug_bounds("page").unwrap();
    let mut longest = 0;
    for step in 1..16 {
        testing::reset_clipboard(cx);
        let reach = bounds.left() + bounds.size.width * (step as f32 / 16.0);
        testing::drag(
            cx,
            point(bounds.left() + px(1.0), bounds.top() + px(6.0)),
            point(reach, bounds.top() + px(6.0)),
        );
        cx.simulate_keystrokes("ctrl-c");
        let copied = testing::clipboard(cx).unwrap();
        if copied == SENTINEL {
            continue;
        }
        assert!(drawn.starts_with(&copied), "step {step}: {copied:?} of {drawn:?}");
        assert!(copied.len() >= longest, "the span only grows with the drag");
        longest = copied.len();
    }
    assert!(longest > 0, "no drag selected anything");
}

#[gpui::test]
async fn a_drag_across_a_code_block_and_a_paragraph_copies_both(cx: &mut TestAppContext) {
    let (_, cx) = open(
        cx,
        "intro text\n\n```rust\nlet x = 1;\n```\n\nafter text",
        None,
    );
    assert_eq!(
        drag_across(cx).as_deref(),
        Some("intro text\n\nlet x = 1;\n\nafter text")
    );
}

#[gpui::test]
async fn select_all_copies_silent_parts_too(cx: &mut TestAppContext) {
    let source = "a line with $x^2$ in it\n\n> [!NOTE]\n> the alert body\n\nlast words";
    let (probes, cx) = open(cx, source, None);
    let plain = blocks(source).text;
    assert!(
        plain.parts.iter().any(|part| part.kind == PartKind::Silent),
        "the fixture must hold a text the renderer does not push"
    );

    // The menu hands its body's actions to whoever draws it.
    let bounds = cx.debug_bounds("page").unwrap();
    mouse(
        cx,
        point(bounds.left() + px(4.0), bounds.top() + px(6.0)),
        MouseButton::Right,
        1,
    );
    let press = probes.menus.borrow_mut().pop().expect("a right press reports");
    testing::reset_clipboard(cx);
    cx.update(|window, cx| press.actions.select_all(window, cx));
    cx.simulate_keystrokes("ctrl-c");

    assert_eq!(testing::clipboard(cx).as_deref(), Some(plain.text.as_str()));
}

#[gpui::test]
async fn ctrl_c_with_nothing_selected_propagates(cx: &mut TestAppContext) {
    cx.update(crate::ely::init);
    let probes = Probes::new();
    let page = probes.page(LINKED, None);
    let (_, (_, editor), cx) = testing::host(cx, move |_, _| page);
    cx.simulate_resize(size(px(800.0), px(600.0)));
    cx.run_until_parked();

    // A click on the body selects nothing; the editor focused after it must still get its
    // Ctrl+C: an action stops propagating by default, and declining is what lets it through.
    let bounds = cx.debug_bounds("page").unwrap();
    testing::press(cx, point(bounds.left() + px(2.0), bounds.top() + px(6.0)), 1);
    cx.update(|window, cx| window.focus(&editor.focus, cx));
    cx.run_until_parked();
    testing::reset_clipboard(cx);
    cx.simulate_keystrokes("ctrl-c");

    assert_eq!(editor.keys.borrow().as_slice(), ["ctrl-c"]);
    assert_eq!(testing::clipboard(cx).as_deref(), Some(SENTINEL));
}

#[gpui::test]
async fn a_right_press_offers_link_items_over_a_link(cx: &mut TestAppContext) {
    let (probes, cx, char_width) = line(cx, LINKED);

    // Over plain text, nothing selected.
    let spot = at(cx, 1.5, char_width);
    mouse(cx, spot, MouseButton::Right, 1);
    let plain = probes.menus.borrow_mut().pop().expect("a press over text reports");
    assert_eq!(plain.link, None);
    assert_eq!(plain.selected, None);
    assert_eq!(
        plain.items(),
        [
            MarkdownMenuItem::Copy { enabled: false },
            MarkdownMenuItem::SelectAll
        ]
    );

    // Over the link.
    let spot = at(cx, 5.5, char_width);
    mouse(cx, spot, MouseButton::Right, 1);
    let on_link = probes.menus.borrow_mut().pop().expect("a press over a link reports");
    assert_eq!(on_link.link.as_deref(), Some("https://x.test"));
    assert_eq!(
        on_link.items(),
        [
            MarkdownMenuItem::Copy { enabled: false },
            MarkdownMenuItem::SelectAll,
            MarkdownMenuItem::OpenLink("https://x.test".into()),
            MarkdownMenuItem::CopyLinkAddress("https://x.test".into()),
        ]
    );
    assert!(probes.opened.borrow().is_empty());

    // With a selection the press keeps it, and Copy is on.
    let spot = at(cx, 0.0, char_width);
    let end = at(cx, 3.0, char_width);
    testing::drag(cx, spot, end);
    let spot = at(cx, 1.5, char_width);
    mouse(cx, spot, MouseButton::Right, 1);
    let with_selection = probes.menus.borrow_mut().pop().expect("a press reports");
    assert_eq!(with_selection.selected.as_deref(), Some("see"));
    assert_eq!(with_selection.items()[0], MarkdownMenuItem::Copy { enabled: true });
}

/// The linked picture, and where to press it.
const PICTURE: &str = "![alt](PNG)";

fn linked_picture() -> String {
    format!("[{}](https://x.test/pic)", PICTURE.replace("PNG", PNG))
}

fn on_picture(cx: &mut VisualTestContext) -> Point<Pixels> {
    let bounds = cx
        .debug_bounds("markdown-picture")
        .expect("the picture decoded and drew");
    bounds.center()
}

/// Where the link of [`LINKED`], or the linked picture, is pressed.
fn on_link(cx: &mut VisualTestContext, picture: bool) -> Point<Pixels> {
    if picture {
        return on_picture(cx);
    }
    let bounds = cx.debug_bounds("page").unwrap();
    let char_width = f32::from(bounds.size.width) / TEXT.chars().count() as f32;
    point(bounds.left() + px(5.5 * char_width), bounds.top() + px(6.0))
}

#[gpui::test]
async fn a_right_or_middle_press_on_a_link_opens_nothing(cx: &mut TestAppContext) {
    for picture in [false, true] {
        let source = if picture { linked_picture() } else { LINKED.to_string() };
        let (probes, cx) = open(cx, &source, None);
        let target = on_link(cx, picture);
        let before = cx.cx.opened_url();
        for button in [MouseButton::Right, MouseButton::Middle] {
            mouse(cx, target, button, 1);
        }
        assert!(probes.opened.borrow().is_empty(), "picture {picture}");
        assert!(cx.cx.opened_url() == before, "picture {picture}");
    }
}

#[gpui::test]
async fn a_left_click_on_a_link_opens_it_once(cx: &mut TestAppContext) {
    for (picture, want) in [(false, "https://x.test"), (true, "https://x.test/pic")] {
        let source = if picture { linked_picture() } else { LINKED.to_string() };
        let (probes, cx) = open(cx, &source, None);
        let target = on_link(cx, picture);
        mouse(cx, target, MouseButton::Left, 1);
        assert_eq!(
            probes.opened.borrow().as_slice(),
            [want],
            "picture {picture}: one click, one opening, through the one path"
        );
    }
}

/// Every kind of node the renderer draws. `[toc]` and the footnote reference are the two
/// that only exist after Task 3's rewrite and walk.
fn every_node() -> String {
    format!(
        "# Title\n\n[toc]\n\nA paragraph with `code`, a [link](https://x.test) and a note[^1].\n\n\
         Inline math $x^2$ stays in its paragraph.\n\n$$\\frac{{a}}{{b}}$$\n\n\
         - one\n- two\n  1. nested first\n  2. nested second\n\n\
         - [x] done\n- [ ] open\n\n\
         > a quote\n> over two lines\n\n\
         > [!WARNING]\n> an alert body\n\n\
         | head | other |\n|---|---|\n| cell | |\n| more | cells |\n\n\
         ```rust\nfn main() {{}}\n```\n\n\
         ![pic]({PNG})\n\n---\n\n## Second\n\ntail\n\n[^1]: the footnote text\n"
    )
}

/// The renderer pushes every text it draws to the gather, in order; the plain-text walk
/// (`text.rs`) claims to know that order without drawing. Chat selection copies the wrong
/// text the day the two disagree, so this draws the fixture once with the real gather and
/// holds the walk to it.
#[gpui::test]
async fn drawn_parts_are_exactly_the_gathers_pushes(cx: &mut TestAppContext) {
    let source = every_node();
    let (probes, _cx) = open(cx, &source, Some(480.0));
    let body = blocks(&source);

    let kinds = body.kinds;
    assert!(
        kinds.math >= 2
            && kinds.alerts == 1
            && kinds.footnotes == 1
            && kinds.tables == 1
            && kinds.code == 1
            && kinds.toc >= 1
            && kinds.images == 1,
        "the fixture must hold every kind of node: {kinds:?}"
    );

    let expected: Vec<String> = (body.text.parts.iter())
        .filter(|part| part.kind == PartKind::Drawn)
        .map(|part| body.text.text[part.range.clone()].to_owned())
        .collect();
    let layouts = probes.layouts.borrow().clone().expect("the page drew");
    let pushed: Vec<String> = (layouts.pushes().into_iter())
        .map(|(lead, text)| format!("{lead}{text}"))
        .collect();
    assert_eq!(
        pushed, expected,
        "the gather's pushes and the walk's drawn parts differ"
    );

    // And a text only the walk knows is one the renderer did not push: the silent ones are
    // each absent from the pushes, whole.
    let silent: Vec<&str> = (body.text.parts.iter())
        .filter(|part| part.kind == PartKind::Silent)
        .map(|part| &body.text.text[part.range.clone()])
        .collect();
    assert!(silent.len() >= 4, "math, display math, the alert title, the footnote label");
    for text in silent {
        assert!(
            !pushed.iter().any(|drawn| drawn == text),
            "{text:?} is silent in the walk and was pushed"
        );
    }
}

/// A table and a fence wider than the body each get a sideways thumb, and a press on a
/// thumb is the thumb's, never the host's selection.
#[gpui::test]
async fn a_wide_table_and_fence_scroll_sideways(cx: &mut TestAppContext) {
    let wide = "x".repeat(120);
    let source = format!(
        "```json\n{{\"key\": \"{wide}\"}}\n```\n\n\
         | a | b | c | d | e | f |\n|---|---|---|---|---|---|\n\
         | {wide} | 2 | 3 | 4 | 5 | 6 |\n"
    );
    let (probes, cx) = open(cx, &source, Some(300.0));
    // The bar is drawn from the geometry the frame before left: a second frame.
    cx.simulate_resize(size(px(801.0), px(600.0)));
    cx.run_until_parked();

    for (ordinal, (frame, bar)) in [
        ("markdown-frame-0", "markdown-bar-0"),
        ("markdown-frame-1", "markdown-bar-1"),
    ]
    .into_iter()
    .enumerate()
    {
        let frame = cx
            .debug_bounds(frame)
            .unwrap_or_else(|| panic!("block {ordinal} has no frame"));
        assert!(
            frame.size.width <= px(300.0),
            "block {ordinal} is {:?} wide, wider than its body",
            frame.size.width
        );
        let thumb = cx
            .debug_bounds(bar)
            .unwrap_or_else(|| panic!("block {ordinal} scrolls with no bar"));
        assert!(thumb.size.width >= px(20.0) && thumb.size.width < frame.size.width);

        let before = probes.host_presses.get();
        mouse(cx, thumb.center(), MouseButton::Left, 1);
        assert_eq!(
            probes.host_presses.get(),
            before,
            "a press on block {ordinal}'s thumb reached the host"
        );
    }

    // The canary: a press on text does reach the host, or the silence above proves nothing.
    let before = probes.host_presses.get();
    let frame = cx.debug_bounds("markdown-frame-0").unwrap();
    mouse(cx, frame.center(), MouseButton::Left, 1);
    assert_eq!(probes.host_presses.get(), before + 1);
}

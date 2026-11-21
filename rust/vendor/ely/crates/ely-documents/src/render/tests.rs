use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*, px, rems};

use super::{HEAVY, MarkdownRenderer, Style, highlight};
use crate::theme::{ActiveTheme, TextSize, Theme};

/// `source` drawn in a 400-point column.
struct Page(&'static str);

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.0))
            .child(MarkdownRenderer::new("page", self.0))
    }
}

#[gpui::test]
fn a_rule_draws_a_line_two_pixels_high(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Page("above\n\n---\n\nbelow\n"));
    cx.run_until_parked();
    let rule = cx.debug_bounds("markdown-rule").expect("the rule is drawn");
    assert_eq!(rule.size.height, px(2.0), "{rule:?}");
    assert!(rule.size.width > px(300.0), "{rule:?}");
}

#[gpui::test]
fn bullets_turn_with_depth(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Page("- a\n  - b\n    1. c\n       - d\n"));
    cx.run_until_parked();
    for depth in [0, 1, 3] {
        let selector = Box::leak(format!("markdown-bullet-{depth}").into_boxed_str());
        assert!(cx.debug_bounds(selector).is_some(), "depth {depth}");
    }
    assert!(
        cx.debug_bounds("markdown-bullet-2").is_none(),
        "an ordered list"
    );
}

#[gpui::test]
fn alerts_lists_of_headings_and_front_matter_draw(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let source = "> [!IMPORTANT]\n> read\n\n[toc]\n\n# One\n\n## Two\n";
    let (_, cx) = cx.add_window_view(|_, _| Page(source));
    cx.run_until_parked();
    let (one, two) = (
        cx.debug_bounds("markdown-toc-0").expect("drawn"),
        cx.debug_bounds("markdown-toc-1").expect("drawn"),
    );
    assert!(two.top() > one.top(), "{one:?} {two:?}");
}

/// One heading of every level.
const HEADINGS: &str = "# a

## a

### a

#### a

##### a

###### a
";

/// `source` in a 400-point column, body at `size`.
struct Sized(&'static str, TextSize);

impl Render for Sized {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.0))
            .child(MarkdownRenderer::new("page", self.0).text_size(self.1))
    }
}

#[gpui::test]
fn headings_step_up_from_the_body_by_typoras_ratios(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    for body in [TextSize::Base, TextSize::Lg] {
        let (_, cx) = cx.add_window_view(move |_, _| Sized(HEADINGS, body));
        cx.run_until_parked();
        let (base, pad) = cx.update(|window, cx| {
            let rem = window.rem_size();
            (
                cx.theme().text_size(body).to_pixels(rem),
                rems(0.5).to_pixels(rem),
            )
        });
        for (ordinal, step) in [2.25, 1.75, 1.5, 1.25, 1.0, 1.0].into_iter().enumerate() {
            let selector = Box::leak(format!("markdown-heading-{ordinal}").into_boxed_str());
            let high = cx.debug_bounds(selector).expect("drawn").size.height;
            let off = high - pad - base * step * 1.3;
            assert!(
                off.abs() < px(0.5),
                "{body:?} level {}: {high:?}",
                ordinal + 1
            );
        }
    }
}

#[gpui::test]
fn strong_text_in_a_heading_keeps_its_weight(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| {
        Page(
            "### Plain **Strong** plain
",
        )
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("markdown-heading-0").is_some(), "drawn");
    let strong = cx.update(|_, cx| {
        let theme = cx.theme();
        highlight(&Style::Strong, &theme.colors, theme.underline_thickness()).font_weight
    });
    assert_eq!(strong, Some(HEAVY), "strong never lightens a heading");
}

#[gpui::test]
fn a_zoomed_document_scales_its_headings(cx: &mut TestAppContext) {
    use crate::{DocumentLook, MarkdownDocument, MarkdownMode};
    cx.update(crate::init_for_tests);
    let (document, cx) = cx.add_window_view(|window, cx| {
        MarkdownDocument::new("### a\n", MarkdownMode::Preview, window, cx)
    });
    document.update(cx, |document, cx| {
        let look = DocumentLook {
            scale: 1.5,
            ..DocumentLook::default()
        };
        document.set_look(look, cx);
    });
    cx.run_until_parked();
    let (base, pad) = cx.update(|window, cx| {
        let rem = window.rem_size();
        (
            cx.theme().text_size(TextSize::Base).to_pixels(rem),
            rems(0.5).to_pixels(rem),
        )
    });
    let high = cx
        .debug_bounds("markdown-heading-0")
        .expect("drawn")
        .size
        .height;
    let off = high - pad - base * 1.5 * 1.5 * 1.3;
    assert!(off.abs() < px(0.5), "{high:?}");
}

#[gpui::test]
fn task_boxes_match_the_text_and_centre_on_its_first_line(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    for size in [TextSize::Base, TextSize::Xl] {
        let source =
            "- above\n- [ ] open and long enough to wrap onto a second line of text\n- [x] done\n";
        let (_, cx) = cx.add_window_view(|_, _| Sized(source, size));
        cx.run_until_parked();
        let (text, gap) = cx.update(|window, cx| {
            let rem = window.rem_size();
            (
                cx.theme().text_size(size).to_pixels(rem),
                rems(0.25).to_pixels(rem),
            )
        });
        let bullet = cx.debug_bounds("markdown-bullet-0").expect("drawn");
        let line = bullet.size.height;
        for ix in [1, 2] {
            let selector = Box::leak(format!("markdown-task-{ix}").into_boxed_str());
            let task = cx.debug_bounds(selector).expect("drawn");
            assert_eq!(task.size.width, text, "{size:?} {ix}: {task:?}");
            assert_eq!(task.size.height, text, "{size:?} {ix}: {task:?}");
            if ix == 1 {
                // Rows sit a quarter rem apart.
                let mid = bullet.bottom() + gap + line / 2.0;
                assert_eq!(task.center().y, mid, "{size:?}: {task:?} {bullet:?}");
            }
        }
    }
}

#[gpui::test]
fn a_footnotes_task_box_matches_its_smaller_text(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Page("a[^1]\n\n[^1]:\n    - above\n    - [ ] note\n"));
    cx.run_until_parked();
    let small = cx.update(|window, cx| {
        cx.theme()
            .text_size(TextSize::Sm)
            .to_pixels(window.rem_size())
    });
    let bullet = cx.debug_bounds("markdown-bullet-0").expect("drawn");
    let task = cx.debug_bounds("markdown-task-1").expect("drawn");
    assert_eq!(task.size.width, small, "{task:?}");
    let line = bullet.size.height;
    assert!((line - small * super::LINE).abs() < px(0.5), "{bullet:?}");
    let gap = cx.update(|window, _| rems(0.25).to_pixels(window.rem_size()));
    assert_eq!(
        task.center().y,
        bullet.bottom() + gap + line / 2.0,
        "{task:?}"
    );
}

mod host {
    use std::{cell::RefCell, ops::Range, rc::Rc};

    use gpui::{
        App, ElementId, FocusHandle, HighlightStyle, InteractiveElement, ImageFormat, Modifiers,
        MouseButton, ParentElement, SharedString, TextLayout, VisualTestContext, point,
    };

    use super::*;
    use crate::{
        HeldPicture, MarkdownBlock, Pictures,
        render::{BlockCull, BlockUse, TextGather},
    };
    use gpui::AnyElement;

    #[derive(Clone, Debug, PartialEq)]
    enum Event {
        Lead(String),
        Text(String),
    }

    /// A host's gather: what it was told, in order.
    #[derive(Default)]
    struct Recorder(RefCell<Vec<Event>>);

    impl TextGather for Recorder {
        fn over(&self, _: &str, _: &App) -> Vec<(Range<usize>, HighlightStyle)> {
            Vec::new()
        }

        fn push(&self, text: &str, _: TextLayout, _: &[(Range<usize>, String)]) {
            self.0.borrow_mut().push(Event::Text(text.to_owned()));
        }

        fn lead(&self, lead: String) {
            self.0.borrow_mut().push(Event::Lead(lead));
        }
    }

    /// A body built by `with`, under a focusable sibling.
    struct Body {
        source: &'static str,
        width: f32,
        sibling: FocusHandle,
        with: Rc<dyn Fn(MarkdownRenderer) -> MarkdownRenderer>,
    }

    impl Body {
        fn new(
            source: &'static str,
            width: f32,
            cx: &mut Context<Self>,
            with: impl Fn(MarkdownRenderer) -> MarkdownRenderer + 'static,
        ) -> Self {
            Self {
                source,
                width,
                sibling: cx.focus_handle(),
                with: Rc::new(with),
            }
        }
    }

    impl Render for Body {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(self.width))
                .child(
                    div()
                        .id("sibling")
                        .track_focus(&self.sibling)
                        .h(px(20.0))
                        .child("sibling"),
                )
                .child((self.with)(MarkdownRenderer::new("page", self.source)))
        }
    }

    #[gpui::test]
    fn a_host_gather_sees_every_drawn_text_in_order(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = "intro\n\n- one\n- two\n\n```\ncode\n```\n\n| a | b |\n|---|---|\n| c | d |\n";
        let recorder = Rc::new(Recorder::default());
        let host = recorder.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let host = host.clone();
            Body::new(source, 400.0, cx, move |body| body.gather(host.clone()))
        });
        cx.run_until_parked();
        let text = |text: &str| Event::Text(text.to_owned());
        let lead = |lead: &str| Event::Lead(lead.to_owned());
        let frame = vec![
            text("intro"),
            lead("• "),
            text("one"),
            lead(""),
            lead("• "),
            text("two"),
            lead(""),
            text("code"),
            text("a"),
            text("b"),
            text("c"),
            text("d"),
        ];
        let seen = recorder.0.borrow();
        assert!(!seen.is_empty(), "nothing was gathered");
        assert!(
            seen.chunks(frame.len()).all(|chunk| chunk == frame),
            "{seen:?}"
        );
    }

    /// A 64-point square picture, held under `cid:square`.
    fn square() -> Rc<Pictures> {
        let mut png = Vec::new();
        image::RgbaImage::new(64, 64)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("a png");
        let held = HeldPicture::new(ImageFormat::Png, png);
        Rc::new([("cid:square".to_string(), Ok(held))].into())
    }

    const LINKED: &str = "hello there\n\n[![a](cid:square)](https://x.test/)\n";

    /// Presses the left button at `at`, then lets go.
    fn press(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
    }

    /// Whether a press on the text and on the picture leave the sibling its focus.
    fn keeps_focus(
        cx: &mut TestAppContext,
        with: impl Fn(MarkdownRenderer) -> MarkdownRenderer + 'static,
    ) -> (bool, bool) {
        cx.update(Theme::init);
        let (body, cx) = cx.add_window_view(move |_, cx| {
            Body::new(LINKED, 400.0, cx, move |body| {
                with(body.pictures(square()))
            })
        });
        let mut drawn = None;
        for _ in 0..50 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
            drawn = cx.debug_bounds("markdown-picture");
            if drawn.is_some() {
                break;
            }
        }
        let picture = drawn.expect("the picture drew").center();
        let sibling = body.read_with(cx, |body, _| body.sibling.clone());
        let held = |cx: &mut VisualTestContext| cx.update(|window, _| sibling.is_focused(window));
        let focus = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| window.focus(&sibling, cx));
            assert!(held(cx), "the sibling takes focus first");
        };
        focus(cx);
        press(cx, point(px(8.0), px(30.0)));
        let on_text = held(cx);
        focus(cx);
        press(cx, picture);
        (on_text, held(cx))
    }

    #[gpui::test]
    fn a_host_gather_body_takes_no_focus(cx: &mut TestAppContext) {
        let recorder = Rc::new(Recorder::default());
        let host = recorder.clone();
        let (on_text, on_picture) = keeps_focus(cx, move |body| body.gather(host.clone()));
        assert!(on_text, "a press on text took the focus");
        assert!(on_picture, "a press on a linked picture took the focus");
        // The same presses do move focus when Ely owns the body.
        let (on_text, _) = keeps_focus(cx, |body| body.selectable(|_, _, _| {}));
        assert!(!on_text, "an Ely selectable body takes focus on text");
        let (_, on_picture) = keeps_focus(cx, |body| body);
        assert!(!on_picture, "a linked picture is a tab stop without a host");
    }

    /// `blocks` drawn in a 400-point column.
    struct Blocks(Rc<[MarkdownBlock]>, Rc<Pictures>);

    impl Render for Blocks {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(400.0)).child(
                MarkdownRenderer::blocks("blocks", self.0.clone()).pictures(self.1.clone()),
            )
        }
    }

    #[gpui::test]
    fn a_picture_with_a_width_draws_that_wide(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        for (width, drawn) in [(Some(120.0), 120.0), (None, 64.0)] {
            let blocks: Rc<[MarkdownBlock]> = vec![MarkdownBlock::Image {
                alt: "square".into(),
                url: "cid:square".into(),
                link: None,
                at: 0,
                width,
            }]
            .into();
            let (_, cx) = cx.add_window_view(move |_, _| Blocks(blocks, square()));
            let mut found = None;
            for _ in 0..50 {
                cx.run_until_parked();
                cx.update(|window, _| window.refresh());
                found = cx.debug_bounds("markdown-picture");
                if found.is_some() {
                    break;
                }
            }
            let found = found.expect("the picture drew");
            assert_eq!(found.size.width, px(drawn), "{width:?}: {found:?}");
        }
    }

    const WEB: &str = "![logo](https://example.invalid/logo.png)\n";

    #[gpui::test]
    fn web_pictures_off_keeps_the_note_on_drops_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        for on in [false, true] {
            let (_, cx) = cx.add_window_view(move |_, cx| {
                Body::new(WEB, 400.0, cx, move |body| body.web_pictures(on))
            });
            cx.run_until_parked();
            let note = cx.debug_bounds("markdown-picture-web-note").is_some();
            assert_eq!(note, !on, "web_pictures({on})");
        }
    }

    const FENCE: &str = "```mermaid\ngraph TD\n  A --> B\n```\n";

    #[gpui::test]
    fn diagrams_off_hands_mermaid_to_the_code_hook(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        for diagrams in [false, true] {
            let languages = Rc::new(RefCell::new(Vec::new()));
            let seen = languages.clone();
            let (_, cx) = cx.add_window_view(move |_, cx| {
                let seen = seen.clone();
                Body::new(FENCE, 400.0, cx, move |body| {
                    let seen = seen.clone();
                    body.diagrams(diagrams)
                        .code(move |id: ElementId, language, _text: SharedString| {
                            seen.borrow_mut().push(language);
                            div().id(id).into_any_element()
                        })
                })
            });
            cx.run_until_parked();
            let seen = languages.borrow();
            match diagrams {
                false => assert!(
                    !seen.is_empty() && seen.iter().all(|lang| lang.as_deref() == Some("mermaid")),
                    "{seen:?}"
                ),
                true => assert!(seen.is_empty(), "a diagram was drawn, not handed on: {seen:?}"),
            }
        }
    }

    /// Two tables: nine columns of uneven width, then a small one.
    const TABLES: &str = "\
| one | two | three | four | five | six | seven | eight | nine |
|---|---|---|---|---|---|---|---|---|
| a rather long first cell indeed | b | cc | dddd | eeeeeeee | f | g | hhhhhhhhhhhhhhhhhh | i |
| x | a rather long second cell indeed | z | w | v | u | t | s | r |

| p | q |
|---|---|
| 1 | 2 |
";

    #[gpui::test]
    fn a_framed_table_keeps_its_natural_width_and_its_columns(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let framed = Rc::new(RefCell::new(Vec::<String>::new()));
        let calls = framed.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let calls = calls.clone();
            Body::new(TABLES, 300.0, cx, move |body| {
                let calls = calls.clone();
                body.table_frame(move |id, table| {
                    calls.borrow_mut().push(format!("{id:?}"));
                    div()
                        .id(id)
                        .w(px(300.0))
                        .flex()
                        .overflow_x_hidden()
                        .child(table)
                        .into_any_element()
                })
            })
        });
        cx.run_until_parked();
        {
            let calls = framed.borrow();
            let mut once = calls.clone();
            once.sort();
            once.dedup();
            assert_eq!(once.len(), 2, "one frame call per table: {calls:?}");
            assert_eq!(calls.len() % 2, 0, "{calls:?}");
        }
        let table = cx.debug_bounds("markdown-table-0").expect("the table drew");
        assert!(table.size.width > px(300.0), "natural width: {table:?}");
        let mut cell = |row: usize, column: usize| {
            let selector = Box::leak(format!("markdown-cell-0-{row}-{column}").into_boxed_str());
            cx.debug_bounds(selector).expect("the cell drew")
        };
        let mut lefts = Vec::new();
        for column in 0..9 {
            let first = cell(0, column);
            for row in 1..=2 {
                let below = cell(row, column);
                assert_eq!(below.left(), first.left(), "column {column} row {row}");
                assert_eq!(below.size.width, first.size.width, "column {column} row {row}");
            }
            lefts.push(first.left());
        }
        assert!(lefts.windows(2).all(|pair| pair[0] < pair[1]), "{lefts:?}");
        // The long cells set their column's width: the columns are not equal.
        let narrow = cell(0, 2).size.width;
        assert!(cell(0, 0).size.width > narrow + px(20.0));
        assert!(cell(0, 1).size.width > narrow + px(20.0));
        assert!(cell(0, 7).size.width > cell(0, 5).size.width + px(20.0));
    }

    #[gpui::test]
    fn framed_table_off_keeps_upstreams_wrapping(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let (_, cx) = cx.add_window_view(|_, cx| Body::new(TABLES, 300.0, cx, |body| body));
        cx.run_until_parked();
        let last = cx.debug_bounds("markdown-cell-0-0-8").expect("the cell drew");
        assert!(last.right() <= px(300.5), "wrapped to the column: {last:?}");
        assert!(cx.debug_bounds("markdown-table-0").is_none());
    }

    /// A cull that notes what each block took, and stands in for the block `skip` names, when
    /// it knows that block's use (a full draw told it).
    struct TestCull {
        skip: Option<usize>,
        uses: RefCell<Vec<BlockUse>>,
        recorder: Rc<Recorder>,
        /// The recorder's length before and after each drawn block.
        spans: RefCell<Vec<(usize, usize)>>,
        mark: std::cell::Cell<usize>,
        known: Vec<BlockUse>,
    }

    impl BlockCull for TestCull {
        fn skip(&self, ix: usize, _: &MarkdownBlock) -> Option<(AnyElement, BlockUse)> {
            self.mark.set(self.recorder.0.borrow().len());
            if self.skip != Some(ix) {
                return None;
            }
            let used = self.known[ix];
            Some((div().h(px(30.0)).into_any_element(), used))
        }

        fn drawn(&self, _: usize, used: BlockUse) {
            self.uses.borrow_mut().push(used);
            self.spans
                .borrow_mut()
                .push((self.mark.get(), self.recorder.0.borrow().len()));
        }

        fn placed(&self, _: Vec<gpui::Bounds<gpui::Pixels>>) {}
    }

    /// The blocks of `CULLED` and the debug selectors each one draws.
    const CULLED: [(&str, &[&str]); 8] = [
        ("intro with [a link](https://x.test/)", &[]),
        ("# First", &["markdown-heading-0"]),
        ("```rust\nfn a() {}\n```", &[]),
        ("| a | b |\n|---|---|\n| c | d |", &["markdown-cell-0-0-0", "markdown-cell-0-1-1"]),
        ("## Second", &["markdown-heading-1"]),
        ("```\nlater\n```", &[]),
        ("| e |\n|---|\n| f |", &["markdown-cell-1-0-0", "markdown-cell-1-1-0"]),
        ("the end [link](https://y.test/)", &[]),
    ];

    /// What one frame's drawing of `CULLED` left: the host gather's events, the ids the code
    /// and the table hooks were handed, which selectors drew, and what each drawn block took.
    #[derive(Debug, PartialEq)]
    struct Drawn {
        events: Vec<Event>,
        code: Vec<String>,
        frames: Vec<String>,
        selectors: Vec<&'static str>,
        placed: Vec<gpui::Bounds<gpui::Pixels>>,
        uses: Vec<BlockUse>,
        spans: Vec<(usize, usize)>,
    }

    /// `CULLED` drawn, with `skip` stood in for when `cull` is set. The window may draw more
    /// than once before it settles, so every record is cut to its first frame: the frames are
    /// counted by the pushes of a block that was not skipped.
    fn draw_culled(
        cx: &mut TestAppContext,
        skip: Option<usize>,
        known: Vec<BlockUse>,
        cull: bool,
    ) -> Drawn {
        cx.update(Theme::init);
        let source: &'static str = Box::leak(
            (CULLED.iter().map(|(text, _)| *text).collect::<Vec<_>>().join("\n\n")).into_boxed_str(),
        );
        let recorder = Rc::new(Recorder::default());
        let test = Rc::new(TestCull {
            skip,
            uses: RefCell::default(),
            recorder: recorder.clone(),
            spans: RefCell::default(),
            mark: Default::default(),
            known,
        });
        let code = Rc::new(RefCell::new(Vec::<String>::new()));
        let frames = Rc::new(RefCell::new(Vec::<String>::new()));
        let (host, cull_host, seen_code, seen_frames) =
            (recorder.clone(), test.clone(), code.clone(), frames.clone());
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let (host, test, code, frames) =
                (host.clone(), cull_host.clone(), seen_code.clone(), seen_frames.clone());
            Body::new(source, 400.0, cx, move |body| {
                let (code, frames) = (code.clone(), frames.clone());
                let body = body
                    .gather(host.clone())
                    .code(move |id, _, _| {
                        code.borrow_mut().push(format!("{id:?}"));
                        div().h(px(20.0)).into_any_element()
                    })
                    .table_frame(move |id, table| {
                        frames.borrow_mut().push(format!("{id:?}"));
                        table
                    });
                if cull { body.cull(test.clone()) } else { body }
            })
        });
        cx.run_until_parked();
        let selectors = CULLED
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .filter(|name| cx.debug_bounds(name).is_some())
            .collect::<Vec<&'static str>>();
        let placed = selectors.iter().filter_map(|name| cx.debug_bounds(name)).collect();
        let events = recorder.0.borrow().clone();
        let pushes = |text: &str| events.iter().filter(|event| **event == Event::Text(text.into())).count();
        let rounds = pushes("intro with a link").max(pushes("the end link")).max(1);
        let first = |all: &[Event]| all[..all.len() / rounds].to_vec();
        let first_of = |all: &[String]| all[..all.len() / rounds].to_vec();
        let uses = test.uses.borrow().clone();
        let spans = test.spans.borrow().clone();
        Drawn {
            events: first(&events),
            code: first_of(&code.borrow()),
            frames: first_of(&frames.borrow()),
            selectors,
            placed,
            uses: uses[..uses.len() / rounds].to_vec(),
            spans: spans[..spans.len() / rounds].to_vec(),
        }
    }

    #[gpui::test]
    fn a_skipped_block_leaves_every_later_id_heading_table_and_push_as_a_full_draw_has_them(
        cx: &mut TestAppContext,
    ) {
        let full = draw_culled(cx, None, Vec::new(), true);
        assert_eq!(full.uses.len(), CULLED.len(), "every block reported its use");
        // The blocks that draw a code block or a table, in order.
        let (code_blocks, frame_blocks) = ([2usize, 5], [3usize, 6]);
        for skipped in 0..CULLED.len() {
            let cut = draw_culled(cx, Some(skipped), full.uses.clone(), true);
            // The gather: the full draw's pushes without the skipped block's.
            let (from, to) = full.spans[skipped];
            let mut events = full.events[..from].to_vec();
            events.extend_from_slice(&full.events[to..]);
            assert_eq!(cut.events, events, "skipping block {skipped}: the gather's pushes");
            // The element ids the hooks were handed keep the numbers a full draw gives them.
            let own = |blocks: &[usize]| blocks.iter().position(|block| *block == skipped);
            for (cut, full, blocks, what) in [
                (&cut.code, &full.code, code_blocks, "code"),
                (&cut.frames, &full.frames, frame_blocks, "table"),
            ] {
                let mut expected = full.clone();
                if let Some(at) = own(&blocks) {
                    expected.remove(at);
                }
                assert_eq!(*cut, expected, "skipping block {skipped}: the {what} ids");
            }
            // Headings and tables: every other block drew under its own number.
            let expected: Vec<&str> = (CULLED.iter().enumerate())
                .filter(|(ix, _)| *ix != skipped)
                .flat_map(|(_, (_, names))| names.iter().copied())
                .collect();
            assert_eq!(cut.selectors, expected, "skipping block {skipped}: headings and tables");
        }
    }

    #[gpui::test]
    fn without_a_cull_the_tree_is_the_one_a_never_skipping_cull_lays_out(cx: &mut TestAppContext) {
        let plain = draw_culled(cx, None, Vec::new(), false);
        let never = draw_culled(cx, None, Vec::new(), true);
        assert_eq!(plain.selectors, never.selectors);
        assert_eq!(plain.placed, never.placed, "the per-block wrapper moves nothing");
        assert_eq!(plain.code, never.code);
        assert_eq!(plain.frames, never.frames);
        assert_eq!(plain.events, never.events);
        assert!(plain.uses.is_empty(), "no cull, nobody is told");
    }
}

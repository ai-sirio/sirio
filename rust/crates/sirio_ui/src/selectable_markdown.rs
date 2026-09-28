//! A rendered Markdown document that can be selected and copied.
//!
//! The chat has had this for its own transcript — `MarkdownBody::selectable`
//! — but there it is welded to the chat's one selection. The other places a
//! `markdown::Doc` is drawn (the file view's Preview, a change request's
//! description and timeline) had none: bezel's renderer paints a selection it
//! is *given* and hands back the glyph layouts it painted, and nothing there
//! supplied either.
//!
//! This is the same construction on the app-wide selection of
//! [`crate::text_selection`]: the document's plain text (`DocParts`, blocks
//! joined by a blank line and parts by a newline, as the chat copies) is the
//! run's text, so Ctrl/Cmd+C, the focus sink and "one selection at a time"
//! come for free. Unlike the chat's body, a press that selects nothing still
//! follows a link, so a README's links keep working.

use crate::chat::{DocParts, LinkClickOverride, markdown_block_link_at, markdown_link_at};
use crate::text_selection::{
    Active, RunKey, begin, dragging, extend, finish, focus_sink, release, selected_range,
    word_range,
};
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};
use std::{panic::Location, rc::Rc};

/// A selectable, link-following rendering of `document`. Its identity is the
/// call site — the caller's, through `render_markdown_document_with_link_override`
/// — under whatever identified elements enclose it.
#[track_caller]
pub(crate) fn selectable_markdown(
    document: markdown::Doc,
    link_click: LinkClickOverride,
) -> SelectableMarkdown {
    SelectableMarkdown {
        id: ElementId::CodeLocation(*Location::caller()),
        parts: Rc::new(DocParts::of(&document)),
        document: Rc::new(document),
        link_click,
        layouts: markdown::BlockLayouts::default(),
        run: None,
        rendered: None,
    }
}

pub(crate) struct SelectableMarkdown {
    id: ElementId,
    document: Rc<markdown::Doc>,
    parts: Rc<DocParts>,
    link_click: LinkClickOverride,
    /// What the renderer painted last, which is how a pointer position
    /// becomes a place in the document.
    layouts: markdown::BlockLayouts,
    run: Option<RunKey>,
    rendered: Option<AnyElement>,
}

/// The caret position nearest `point`, as a place in the document.
///
/// `BlockLayouts::hit` names the glyph under the pointer — and, off the text,
/// the nearest run — so a drag to a corner still ends somewhere. A caret sits
/// between glyphs, though: the half of the glyph the pointer is in decides
/// which edge, or the last character dragged over would never be included.
fn caret(
    layouts: &markdown::BlockLayouts,
    document: &markdown::Doc,
    point: Point<Pixels>,
) -> Option<markdown::Cursor> {
    let cursor = layouts.hit(point)?;
    let text = document.blocks.get(cursor.block)?.text_at(cursor.part)?;
    let Some(next) = text
        .text
        .get(cursor.offset..)
        .and_then(|rest| rest.chars().next())
        .map(|character| cursor.offset + character.len_utf8())
    else {
        return Some(cursor);
    };
    let after = markdown::Cursor::new(cursor.block, cursor.part, next);
    match (layouts.position(cursor), layouts.position(after)) {
        (Some((left, _)), Some((right, _)))
            if left.y == right.y && point.x - left.x > right.x - point.x =>
        {
            Some(after)
        }
        _ => Some(cursor),
    }
}

impl SelectableMarkdown {
    fn listen(&self, run: RunKey, hitbox: Hitbox, window: &mut Window) {
        let view = window.current_view();

        // A press. Capture: a press anywhere else ends this document's
        // selection even when something on top stops the event. Bubble: a
        // press on the document begins one.
        let (layouts, document, parts) = (
            self.layouts.clone(),
            self.document.clone(),
            self.parts.clone(),
        );
        let down_run = run.clone();
        let down_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if event.button != MouseButton::Left {
                return;
            }
            let hovered = down_hitbox.is_hovered(window);
            match phase {
                DispatchPhase::Capture if !hovered => release(&down_run, cx),
                DispatchPhase::Bubble if hovered => {
                    // A caret for a click or a drag; for a word, the glyph the
                    // pointer is on — not the nearer edge, which in the right
                    // half of a word's last letter is the separator after it.
                    let cursor = if event.click_count == 2 {
                        layouts.hit(event.position)
                    } else {
                        caret(&layouts, &document, event.position)
                    };
                    let Some(cursor) = cursor else {
                        return;
                    };
                    let offset = parts.offset_of(cursor);
                    let (anchor, head) = match event.click_count {
                        0 | 1 => (offset, offset),
                        2 => {
                            let word = word_range(&parts.text, offset);
                            (word.start, word.end)
                        }
                        _ => {
                            let span = parts.part_span(offset);
                            (span.start, span.end)
                        }
                    };
                    begin(
                        Active {
                            run: down_run.clone(),
                            text: parts.text.clone().into(),
                            anchor,
                            head,
                            dragging: true,
                            view,
                        },
                        cx,
                    );
                    if event.click_count >= 2 {
                        focus_sink(window, cx);
                        window.prevent_default();
                    }
                }
                _ => {}
            }
        });

        // The drag follows the pointer wherever it goes; the caret falls back
        // to the nearest text, so a drag past the last line ends on it.
        let (layouts, document, parts) = (
            self.layouts.clone(),
            self.document.clone(),
            self.parts.clone(),
        );
        let move_run = run.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture
                || !event.dragging()
                || !dragging(&move_run, cx)
            {
                return;
            }
            if let Some(cursor) = caret(&layouts, &document, event.position) {
                extend(&move_run, parts.offset_of(cursor), view, window, cx);
            }
        });

        // The release. A press and release with nothing selected between
        // them is a click, and a click on a link follows it — the chat's own
        // body opens a link on any release over it, even one ending a drag.
        let (layouts, document, link_click) = (
            self.layouts.clone(),
            self.document.clone(),
            self.link_click.clone(),
        );
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            let hovered = hitbox.is_hovered(window);
            if finish(&run, view, cx) != Some(false) || !hovered {
                return;
            }
            let position = event.position;
            let inline = layouts
                .over_text(position)
                .then(|| layouts.hit(position))
                .flatten()
                .and_then(|cursor| markdown_link_at(&document, cursor));
            let block = layouts
                .block_at(position)
                .and_then(|block| markdown_block_link_at(&document, block));
            if let Some(target) = inline.or(block) {
                cx.stop_propagation();
                window.prevent_default();
                link_click(&target, window, cx);
            }
        });
    }
}

impl IntoElement for SelectableMarkdown {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableMarkdown {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let run = RunKey::new(id, &self.parts.text);
        let selection = selected_range(&run, &self.parts.text, cx)
            .and_then(|range| self.parts.selection_for(range, 0));
        self.run = Some(run);
        // bezel paints its editor caret at the selection's head as well — a
        // line at the highlight's edge, only while a selection exists — and
        // has no flag to leave it out; the chat's body carries the same.
        let mut rendered = markdown::render_with_selection(
            &self.document,
            selection,
            Some(&self.layouts),
            None,
            markdown::Caption::Shown,
            window,
            cx,
        );
        let layout = rendered.request_layout(window, cx);
        self.rendered = Some(rendered);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.rendered
            .as_mut()
            .expect("markdown requested layout before prepaint")
            .prepaint(window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(run) = self.run.clone() {
            self.listen(run, hitbox.clone(), window);
        }
        self.rendered
            .as_mut()
            .expect("markdown prepainted before paint")
            .paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_selection::testing::{self, SENTINEL};
    use gpui::{
        Context, InteractiveElement, Modifiers, ParentElement, Render, Styled, TestAppContext, div,
        point, px, size,
    };
    use std::cell::RefCell;

    /// One paragraph with a link in the middle, drawn shrink-wrapped so its
    /// width is the text's own.
    struct Page {
        doc: markdown::Doc,
        opened: Rc<RefCell<Vec<String>>>,
    }

    impl Render for Page {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let opened = self.opened.clone();
            let link_click: LinkClickOverride =
                Rc::new(move |target, _, _| opened.borrow_mut().push(target.to_string()));
            div().size_full().flex().flex_row().child(
                div()
                    .debug_selector(|| "page".into())
                    .child(selectable_markdown(self.doc.clone(), link_click)),
            )
        }
    }

    const TEXT: &str = "see docs now";

    fn page(
        cx: &mut TestAppContext,
    ) -> (
        Rc<RefCell<Vec<String>>>,
        &mut gpui::VisualTestContext,
        f32,
    ) {
        cx.update(bezel::ui::input::init);
        let opened = Rc::new(RefCell::new(Vec::new()));
        let shared = opened.clone();
        let (_, _, cx) = testing::host(cx, move |_, _| Page {
            doc: markdown::parse("see [docs](https://x.test) now"),
            opened: shared,
        });
        cx.simulate_resize(size(px(800.0), px(400.0)));
        cx.run_until_parked();
        let width = f32::from(cx.debug_bounds("page").expect("the page renders").size.width);
        (opened, cx, width / TEXT.chars().count() as f32)
    }

    fn at(
        cx: &mut gpui::VisualTestContext,
        chars: f32,
        char_width: f32,
    ) -> gpui::Point<gpui::Pixels> {
        let bounds = cx.debug_bounds("page").unwrap();
        point(bounds.left() + px(chars * char_width), bounds.top() + px(6.0))
    }

    /// Dragging across a link selects it; it must not also open it. The
    /// chat's own body opens a link on any release over it, even one that
    /// ends a drag — right for prose nobody selects, wrong for a README.
    #[gpui::test]
    async fn a_drag_across_a_link_selects_it_without_following_it(cx: &mut TestAppContext) {
        let (opened, cx, char_width) = page(cx);
        testing::reset_clipboard(cx);

        // From the start to the middle of "docs": the release lands on the
        // link, which is exactly where a buggy release would open it.
        let (from, to) = (at(cx, 0.0, char_width), at(cx, 6.0, char_width));
        testing::drag(cx, from, to);
        cx.simulate_keystrokes("ctrl-c");

        assert_eq!(testing::clipboard(cx).as_deref(), Some("see do"));
        assert!(
            opened.borrow().is_empty(),
            "a release that ends a drag is not a click: {:?}",
            opened.borrow()
        );
    }

    #[gpui::test]
    async fn a_plain_click_on_a_link_still_follows_it(cx: &mut TestAppContext) {
        let (opened, cx, char_width) = page(cx);
        testing::reset_clipboard(cx);

        let on_link = at(cx, 5.5, char_width);
        cx.simulate_click(on_link, Modifiers::none());

        assert_eq!(opened.borrow().as_slice(), ["https://x.test"]);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(
            testing::clipboard(cx).as_deref(),
            Some(SENTINEL),
            "a click selects nothing"
        );
    }

    #[gpui::test]
    async fn double_click_selects_a_word_and_triple_click_the_paragraph(cx: &mut TestAppContext) {
        let (opened, cx, char_width) = page(cx);
        for (count, expected) in [(2, "see"), (3, TEXT)] {
            testing::reset_clipboard(cx);
            let position = at(cx, 1.5, char_width);
            testing::press(cx, position, count);
            cx.simulate_keystrokes("ctrl-c");
            assert_eq!(testing::clipboard(cx).as_deref(), Some(expected), "click count {count}");
        }
        assert!(opened.borrow().is_empty());
    }

    /// The word is the one the pointer is *on*. In the right half of "see"'s
    /// last letter the nearer caret is the space after it; taking that would
    /// select the space.
    #[gpui::test]
    async fn a_double_click_on_the_right_half_of_a_words_last_letter_takes_that_word(
        cx: &mut TestAppContext,
    ) {
        let (_, cx, char_width) = page(cx);
        testing::reset_clipboard(cx);
        let position = at(cx, 2.7, char_width);
        testing::press(cx, position, 2);
        cx.simulate_keystrokes("ctrl-c");
        assert_eq!(testing::clipboard(cx).as_deref(), Some("see"));
    }
}

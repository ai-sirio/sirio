use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Stateful,
    Styled, StyledText, Window, div, prelude::*,
};

use super::{
    gutter::Cell,
    layout::Row,
    state::{CodeEditor, EditorEvent},
};
use crate::theme::{ActiveTheme, TextSize};

impl CodeEditor {
    /// Row `ix` as the list draws it.
    pub(crate) fn row(&self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let base = div()
            .id(("editor-row", ix))
            .w_full()
            .h(self.frame.line)
            .flex();
        match self.frame.rows[ix] {
            Row::Line(line) => self.line_row(base, ix, line, window, cx),
            Row::Lens(at) => self.lens_row(base, ix, at, cx),
            Row::Removed(hunk, at) => self.removed_row(base, ix, hunk, at, cx),
            Row::Ghost(at) => self.ghost_row(base, ix, at, cx),
            Row::Chat(_) => self.chat_row(base, ix, cx),
        }
    }

    fn lens_row(
        &self,
        base: Stateful<gpui::Div>,
        ix: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gutter = self.gutter(Cell::Blank, ix, cx);
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let lens = &self.marks.lenses[at];
        let indent = self.buffer.indent(lens.line);
        let items = lens.items.iter().enumerate().map(|(item, words)| {
            let line = lens.line;
            div()
                .id(("lens", item))
                .cursor_pointer()
                .hover(|style| style.text_color(colors.link))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(move |_, _, _, cx| {
                    log::info!("code lens: line {line}, item {item}");
                    cx.emit(EditorEvent::Lens(line, item));
                }))
                .child(words.clone())
        });
        let items: Vec<AnyElement> = items
            .enumerate()
            .flat_map(|(place, item)| {
                let gap = (place > 0).then(|| div().child("·").into_any_element());
                gap.into_iter().chain([item.into_any_element()])
            })
            .collect();
        base.child(gutter)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .pl(self.frame.advance * (indent + 1) as f32)
                    .font_family(theme.font_family.clone())
                    .text_size(theme.text_size(TextSize::Xs))
                    .text_color(colors.fg_subtle)
                    .children(items),
            )
            .into_any_element()
    }

    fn removed_row(
        &self,
        base: Stateful<gpui::Div>,
        ix: usize,
        hunk: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let text = self.marks.hunks[hunk].removed[at].clone();
        base.child(self.gutter(Cell::Removed, ix, cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .pl(self.frame.advance)
                    .bg(colors.danger.opacity(0.08))
                    .line_height(self.frame.line)
                    .whitespace_nowrap()
                    .text_color(colors.fg_muted)
                    .child(text),
            )
            .into_any_element()
    }

    fn ghost_row(
        &self,
        base: Stateful<gpui::Div>,
        ix: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let ghost = self
            .marks
            .ghost
            .as_ref()
            .expect("a ghost row has its ghost");
        let lines: Vec<&str> = ghost.text.split('\n').collect();
        let text = lines[at].to_string();
        let rest = (at + 1 == lines.len()).then(|| {
            let line = self.buffer.line_of(ghost.offset);
            let range = self.buffer.line_range(line);
            let rest = self.buffer.text()[ghost.offset..range.end].to_string();
            // The owner's colors are per line: the rest takes the spans past the ghost.
            let from = ghost.offset - range.start;
            let styles: Vec<_> = self
                .line_colors(line, self.buffer.line(line), cx)
                .into_iter()
                .filter(|(span, _)| span.end > from)
                .map(|(span, style)| (span.start.max(from) - from..span.end - from, style))
                .collect();
            StyledText::new(rest).with_highlights(styles)
        });
        base.child(self.gutter(Cell::Blank, ix, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .pl(self.frame.advance)
                    .line_height(self.frame.line)
                    .whitespace_nowrap()
                    .child(div().italic().text_color(colors.fg_subtle).child(text))
                    .children(rest),
            )
            .into_any_element()
    }

    /// An inline chat's rows keep the gutter only, since the chat widget is not vendored.
    fn chat_row(
        &self,
        base: Stateful<gpui::Div>,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        base.child(self.gutter(Cell::Blank, ix, cx)).into_any_element()
    }
}

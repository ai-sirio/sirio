//! Fenced code and tables, drawn by Sirio inside Ely's renderer.
//!
//! Ely's `code` hook gets no `&App`, so everything theme-dependent is read once into a
//! [`CodeStyle`] when the view lays out. The hook draws the language label, the copy
//! button and the sideways frame, highlights with `bezel_syntax`, and pushes the code to
//! the body's gather as Ely would, so code stays selectable and copyable.

use std::ops::Range;
use std::rc::Rc;

use ely_gpui_component::theme::{ActiveTheme as _, Radius, TextSize};
use gpui::{
    AnyElement, App, ElementId, HighlightStyle, Hsla, IntoElement, MouseButton, ParentElement,
    Pixels, RenderOnce, Rems, SharedString, Styled, StyledText, Window, canvas, combine_highlights, div,
    prelude::*, px,
};

use super::gather::SirioGather;
use super::sideways::{Sideways, scroller};

/// Line height against the text size, Ely's.
const LINE: f32 = 1.6;

/// What the code hook draws with, read from the theme at layout.
#[derive(Clone)]
pub(crate) struct CodeStyle {
    pub syntax: bezel_theme::SyntaxPalette,
    pub mono: SharedString,
    pub text: Hsla,
    pub label: Hsla,
    pub muted: Hsla,
    pub fill: Hsla,
    pub band: Hsla,
    pub border: Hsla,
    pub selection: Hsla,
    pub hover: Hsla,
    pub radius: Rems,
    pub size: Rems,
    pub label_size: Rems,
}

impl CodeStyle {
    pub(crate) fn read(cx: &App) -> Self {
        let theme = cx.theme();
        let colors = &theme.colors;
        Self {
            syntax: bezel_theme::Theme::of(cx).syntax.clone(),
            mono: theme.mono_family.clone(),
            text: colors.syntax.variable,
            label: colors.fg_subtle,
            muted: colors.fg_muted,
            fill: colors.sunken,
            band: colors.hover,
            border: colors.border,
            selection: colors.selection,
            hover: colors.hover,
            radius: theme.radius(Radius::Md),
            size: theme.text_size(TextSize::Sm),
            label_size: theme.text_size(TextSize::Xs),
        }
    }
}

/// The syntax colours of `code` as highlights: sorted, on character boundaries, none
/// overlapping another.
fn syntax_highlights(
    style: &CodeStyle,
    language: Option<&str>,
    code: &str,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(spans) = language.and_then(|language| bezel_syntax::highlight(code, language)) else {
        return Vec::new();
    };
    let mut spans: Vec<_> = spans.into_iter().collect();
    spans.sort_by_key(|(range, _)| (range.start, range.end));
    let mut floor = 0;
    let mut out = Vec::new();
    for (range, kind) in spans {
        let start = range.start.max(floor);
        let end = range.end.min(code.len());
        if start >= end || !code.is_char_boundary(start) || !code.is_char_boundary(end) {
            continue;
        }
        floor = end;
        out.push((
            start..end,
            HighlightStyle {
                color: Some(style.syntax.color(kind)),
                ..Default::default()
            },
        ));
    }
    out
}

/// Ely's `code` hook: id, language, code.
pub(crate) fn code_hook(
    style: CodeStyle,
    gather: Rc<SirioGather>,
) -> impl Fn(ElementId, Option<SharedString>, SharedString) -> AnyElement {
    move |id, language, code| {
        let ordinal = gather.next_frame();
        let syntax = syntax_highlights(&style, language.as_deref(), &code);
        let selected = gather.highlight(&code, style.selection);
        let styles: Vec<_> = combine_highlights(syntax, selected).collect();
        let styled = StyledText::new(code.clone()).with_highlights(styles);
        gather.note(&code, styled.layout().clone(), &[]);

        let layouts = gather.layouts().clone();
        let report: Rc<dyn Fn(usize, gpui::Bounds<Pixels>)> =
            Rc::new(move |ordinal, bounds| layouts.record_frame(ordinal, bounds));
        let scroll_id = ElementId::from((id.clone(), "scroller".to_owned()));
        let line = style.size * LINE;
        let body = Sideways::new(
            id.clone(),
            scroller(scroll_id)
                .relative()
                .flex()
                .flex_row()
                .py(px(8.0))
                .font_family(style.mono.clone())
                .text_size(style.size)
                .line_height(line)
                .text_color(style.text)
                .whitespace_nowrap()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .px(px(12.0))
                        .items_start()
                        .child(styled),
                ),
            ordinal,
        );
        // The whole block, band included, is what a reader sees as one thing.
        let outline = canvas(move |bounds, _, _| report(ordinal, bounds), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full();
        div()
            .rounded(style.radius)
            .bg(style.fill)
            .border_1()
            .border_color(style.border)
            .overflow_hidden()
            .relative()
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(12.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(style.border)
                    .bg(style.band)
                    .text_size(style.label_size)
                    .text_color(style.label)
                    .child(language.clone().unwrap_or_else(|| "text".into())),
            )
            .child(body)
            .child(outline)
            .child(CopyButton {
                id: ElementId::from((id, "copy".to_owned())),
                code,
                color: style.muted,
                hover: style.hover,
                size: style.label_size,
            })
            .into_any_element()
    }
}

/// A table at its natural width, in a sideways frame (Ely's `table_frame`).
pub(crate) fn table_frame(
    gather: Rc<SirioGather>,
) -> impl Fn(ElementId, AnyElement) -> AnyElement {
    move |id, table| {
        let ordinal = gather.next_frame();
        let layouts = gather.layouts().clone();
        let report: Rc<dyn Fn(usize, gpui::Bounds<Pixels>)> =
            Rc::new(move |ordinal, bounds| layouts.record_frame(ordinal, bounds));
        Sideways::new(
            id.clone(),
            scroller(ElementId::from((id, "scroller".to_owned())))
                .flex()
                .flex_row()
                .child(table),
            ordinal,
        )
        .on_bounds(report)
        .into_any_element()
    }
}

/// The "Copy" label over a block's band, which says "Copied" until the pointer leaves.
#[derive(IntoElement)]
struct CopyButton {
    id: ElementId,
    code: SharedString,
    color: Hsla,
    hover: Hsla,
    size: Rems,
}

impl RenderOnce for CopyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let copied = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let showing = *copied.read(cx);
        let (code, hover, left) = (self.code, self.hover, copied.clone());
        div()
            .id(self.id)
            .absolute()
            .top(px(3.0))
            .right(px(5.0))
            .h(px(20.0))
            .px(px(6.0))
            .rounded(px(5.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(self.size)
            .text_color(self.color)
            .hover(move |style| style.bg(hover))
            .child(if showing { "Copied" } else { "Copy" })
            // The press is the button's: a body's host starts a selection from any press.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.to_string()));
                copied.update(cx, |state, cx| {
                    *state = true;
                    cx.notify();
                });
            })
            .on_hover(move |hovering, _, cx| {
                if !*hovering && *left.read(cx) {
                    left.update(cx, |state, cx| {
                        *state = false;
                        cx.notify();
                    });
                }
            })
    }
}

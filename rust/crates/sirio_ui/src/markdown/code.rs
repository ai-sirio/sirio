//! Fenced code and tables, drawn by Sirio inside Ely's renderer.
//!
//! Ely's `code` hook gets no `&App`, so everything theme-dependent is read once into a
//! [`CodeStyle`] when the view lays out. The hook draws the language label, the copy
//! button and the sideways frame, highlights with `bezel_syntax`, and pushes the code to
//! the body's gather as Ely would, so code stays selectable and copyable.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
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

/// The syntax kinds of a code block: sorted, on character boundaries, none overlapping
/// another. Colourless, so the result outlives a change of theme.
type Kinds = Rc<[(Range<usize>, bezel_theme::HighlightKind)]>;

/// Kinds held by the memo, in code bytes (plus language and a nominal cost per span), before
/// the least recently used go.
const KINDS_MEMO_BYTES: usize = 4 * 1024 * 1024;

/// What one span costs the memo, besides the code it is keyed by.
const SPAN_BYTES: usize = std::mem::size_of::<(Range<usize>, bezel_theme::HighlightKind)>();

struct Held {
    kinds: Kinds,
    tick: u64,
}

/// A least-recently-used memo of [`Kinds`] by language and code, compared in full (never by
/// a hash alone), bounded by the bytes it holds.
struct KindsMemo {
    cap: usize,
    bytes: usize,
    tick: u64,
    by_language: HashMap<Rc<str>, HashMap<Rc<str>, Held>>,
    /// Least recently used first: tick -> (language, code).
    order: BTreeMap<u64, (Rc<str>, Rc<str>)>,
}

impl KindsMemo {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            bytes: 0,
            tick: 0,
            by_language: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn cost(language: &str, code: &str, kinds: &Kinds) -> usize {
        language.len() + code.len() + kinds.len() * SPAN_BYTES
    }

    /// The kinds of `code` under `language`: held ones for a block seen before, else what
    /// `compute` makes, held in turn when it fits the cap at all.
    fn get_or_insert_with(
        &mut self,
        language: &str,
        code: &str,
        compute: impl FnOnce() -> Kinds,
    ) -> Kinds {
        self.tick += 1;
        let tick = self.tick;
        if let Some(held) = self
            .by_language
            .get_mut(language)
            .and_then(|codes| codes.get_mut(code))
        {
            let (kinds, was) = (held.kinds.clone(), held.tick);
            held.tick = tick;
            if let Some(key) = self.order.remove(&was) {
                self.order.insert(tick, key);
            }
            return kinds;
        }
        let kinds = compute();
        let cost = Self::cost(language, code, &kinds);
        if cost > self.cap {
            return kinds;
        }
        while self.bytes + cost > self.cap {
            let Some((_, (language, code))) = self.order.pop_first() else {
                break;
            };
            self.evict(&language, &code);
        }
        let (language, code): (Rc<str>, Rc<str>) = (language.into(), code.into());
        self.by_language
            .entry(language.clone())
            .or_default()
            .insert(code.clone(), Held { kinds: kinds.clone(), tick });
        self.order.insert(tick, (language, code));
        self.bytes += cost;
        kinds
    }

    fn evict(&mut self, language: &str, code: &str) {
        let Some(codes) = self.by_language.get_mut(language) else {
            return;
        };
        if let Some(held) = codes.remove(code) {
            self.bytes -= Self::cost(language, code, &held.kinds);
        }
        if codes.is_empty() {
            self.by_language.remove(language);
        }
    }
}

thread_local! {
    // GPUI draws on one thread, so the memo is per thread and needs no lock.
    static KINDS: RefCell<KindsMemo> = RefCell::new(KindsMemo::new(KINDS_MEMO_BYTES));
}

/// `bezel_syntax`'s spans of `code`, clipped to what a highlight may carry.
fn clipped_kinds(code: &str, language: &str) -> Kinds {
    let Some(spans) = bezel_syntax::highlight(code, language) else {
        return Rc::from([]);
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
        out.push((start..end, kind));
    }
    out.into()
}

/// The syntax colours of `code` as highlights. The kinds are memoized across draws, so a
/// closed block of a streaming reply is highlighted once; the colours come from `style` each
/// time.
fn syntax_highlights(
    style: &CodeStyle,
    language: Option<&str>,
    code: &str,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(language) = language else {
        return Vec::new();
    };
    let kinds = KINDS.with(|memo| {
        (memo.borrow_mut()).get_or_insert_with(language, code, || clipped_kinds(code, language))
    });
    coloured(style, &kinds)
}

fn coloured(style: &CodeStyle, kinds: &Kinds) -> Vec<(Range<usize>, HighlightStyle)> {
    (kinds.iter())
        .map(|(range, kind)| {
            (
                range.clone(),
                HighlightStyle {
                    color: Some(style.syntax.color(*kind)),
                    ..Default::default()
                },
            )
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use bezel_theme::HighlightKind;
    use gpui::hsla;

    use super::*;

    fn kinds(spans: &[(usize, usize, HighlightKind)]) -> Kinds {
        spans.iter().map(|(a, b, k)| (*a..*b, *k)).collect()
    }

    /// A palette whose every colour carries `hue`.
    fn palette(hue: f32) -> bezel_theme::SyntaxPalette {
        let c = |lightness: f32| hsla(hue, 1.0, lightness, 1.0);
        bezel_theme::SyntaxPalette {
            comment: c(0.01),
            keyword: c(0.02),
            string: c(0.03),
            string_special: c(0.04),
            escape: c(0.05),
            number: c(0.06),
            boolean: c(0.07),
            type_name: c(0.08),
            type_builtin: c(0.09),
            constructor: c(0.10),
            function: c(0.11),
            function_builtin: c(0.12),
            macro_name: c(0.13),
            property: c(0.14),
            constant: c(0.15),
            variable: c(0.16),
            variable_special: c(0.17),
            parameter: c(0.18),
            operator: c(0.19),
            punctuation: c(0.20),
            tag: c(0.21),
            attribute: c(0.22),
            label: c(0.23),
            invalid: c(0.24),
        }
    }

    fn style(hue: f32) -> CodeStyle {
        let color = hsla(0.0, 0.0, 0.5, 1.0);
        CodeStyle {
            syntax: palette(hue),
            mono: "mono".into(),
            text: color,
            label: color,
            muted: color,
            fill: color,
            band: color,
            border: color,
            selection: color,
            hover: color,
            radius: gpui::rems(0.5),
            size: gpui::rems(0.8),
            label_size: gpui::rems(0.7),
        }
    }

    /// The memo, asked for `(language, code)`; the second value says whether `compute` ran.
    fn ask(memo: &mut KindsMemo, language: &str, code: &str, made: Kinds) -> (Kinds, bool) {
        let ran = Cell::new(false);
        let got = memo.get_or_insert_with(language, code, || {
            ran.set(true);
            made
        });
        (got, ran.get())
    }

    #[test]
    fn a_block_seen_again_is_not_computed_again() {
        let mut memo = KindsMemo::new(1 << 20);
        let spans = kinds(&[(0, 2, HighlightKind::Keyword)]);
        assert!(ask(&mut memo, "rust", "fn a() {}", spans.clone()).1);
        let (got, ran) = ask(&mut memo, "rust", "fn a() {}", kinds(&[]));
        assert!(!ran, "the second ask must be served from the memo");
        assert_eq!(got, spans);
    }

    #[test]
    fn the_same_code_under_two_languages_keeps_two_answers() {
        let mut memo = KindsMemo::new(1 << 20);
        let as_rust = kinds(&[(0, 1, HighlightKind::Keyword)]);
        let as_python = kinds(&[(0, 1, HighlightKind::Function)]);
        ask(&mut memo, "rust", "x", as_rust.clone());
        ask(&mut memo, "python", "x", as_python.clone());
        let (rust, ran) = ask(&mut memo, "rust", "x", kinds(&[]));
        assert!(!ran);
        assert_eq!(rust, as_rust);
        let (python, ran) = ask(&mut memo, "python", "x", kinds(&[]));
        assert!(!ran);
        assert_eq!(python, as_python);
    }

    #[test]
    fn code_that_changes_by_one_byte_is_not_served_the_old_spans() {
        let mut memo = KindsMemo::new(1 << 20);
        let old = kinds(&[(0, 3, HighlightKind::Keyword)]);
        let new = kinds(&[(0, 4, HighlightKind::Keyword)]);
        ask(&mut memo, "rust", "let a", old.clone());
        let (got, ran) = ask(&mut memo, "rust", "let ab", new.clone());
        assert!(ran, "a grown fence is a different key");
        assert_eq!(got, new);
        let (got, _) = ask(&mut memo, "rust", "let a", kinds(&[]));
        assert_eq!(got, old, "the first block is still its own");
    }

    #[test]
    fn the_cap_evicts_the_least_recently_used_and_holds_the_bytes_under_it() {
        let code = |n: usize| format!("{n:0>200}");
        let cost = KindsMemo::cost("rust", &code(0), &kinds(&[]));
        let mut memo = KindsMemo::new(cost * 3);
        for n in 0..3 {
            ask(&mut memo, "rust", &code(n), kinds(&[]));
        }
        // Block 0 is read again, so block 1 is the least recently used.
        assert!(!ask(&mut memo, "rust", &code(0), kinds(&[])).1);
        ask(&mut memo, "rust", &code(3), kinds(&[]));
        assert!(memo.bytes <= memo.cap, "{} held over a cap of {}", memo.bytes, memo.cap);
        assert!(!ask(&mut memo, "rust", &code(0), kinds(&[])).1, "recently used stays");
        assert!(ask(&mut memo, "rust", &code(1), kinds(&[])).1, "least recently used went");
        for n in 4..40 {
            ask(&mut memo, "rust", &code(n), kinds(&[]));
            assert!(memo.bytes <= memo.cap);
        }
    }

    #[test]
    fn a_block_larger_than_the_cap_is_computed_each_time_and_evicts_nothing() {
        let mut memo = KindsMemo::new(100);
        ask(&mut memo, "rust", "small", kinds(&[]));
        let big = "x".repeat(500);
        assert!(ask(&mut memo, "rust", &big, kinds(&[])).1);
        assert!(ask(&mut memo, "rust", &big, kinds(&[])).1);
        assert!(!ask(&mut memo, "rust", "small", kinds(&[])).1);
        assert!(memo.bytes <= memo.cap);
    }

    #[test]
    fn a_hit_paints_what_a_miss_paints_in_the_colours_of_the_style_asked() {
        let code = "fn main() {\n    let n = 42; // answer\n    println!(\"{n}\");\n}\n";
        let (dark, light) = (style(0.6), style(0.1));
        let direct = |style: &CodeStyle| coloured(style, &clipped_kinds(code, "rust"));
        let first = syntax_highlights(&dark, Some("rust"), code);
        assert!(!first.is_empty(), "the fixture must highlight");
        assert_eq!(first, direct(&dark));
        // The memo now holds the block: served for a different theme, with its colours.
        let again = syntax_highlights(&light, Some("rust"), code);
        assert_eq!(again, direct(&light));
        assert_ne!(again, first, "a theme change must repaint");
        assert!(syntax_highlights(&light, None, code).is_empty());
    }
}

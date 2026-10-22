//! How one row of the Trace tab is drawn: the tree cell on the left, the bar
//! on the shared axis on the right, and the tooltip over both.

use std::rc::Rc;
use std::time::Duration;

use ely_gpui_component::{
    primitives::{Icon, IconName, Tooltip},
    theme::IconSize,
};
use gpui::{
    AnyElement, CursorStyle, Entity, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, div, pattern_slash,
    prelude::FluentBuilder as _, px, relative,
};
use sirio_acp::SpanKind;
use sirio_theme::Theme;

use super::{TraceTab, has_children};
use crate::chat::trace::{SpanState, TreeRow, format_duration};
use crate::trace_style::compact_count;

/// A row's height beside its bar, and stacked.
const ROW_HEIGHT: f32 = 28.0;
const STACKED_ROW_HEIGHT: f32 = 48.0;
/// How far a row steps in for each level it is nested.
const INDENT: f32 = 14.0;
/// The bar's place in a row.
const BAR_TOP: f32 = 8.0;
const BAR_HEIGHT: f32 = 12.0;
/// A bar that ends past this share of the axis has its label on its left.
const LABEL_FLIPS_AT: f32 = 0.72;

/// `340ms` below a second, `4.1s` below a minute, then `1m 02s`: bars are
/// short, so the whole seconds of `format_duration` would say `0s` of most.
pub(super) fn short_duration(took: Duration) -> String {
    if took < Duration::from_secs(1) {
        format!("{}ms", took.as_millis())
    } else if took < Duration::from_secs(60) {
        let tenths = took.as_millis() / 100;
        format!("{}.{}s", tenths / 10, tenths % 10)
    } else {
        format_duration(took)
    }
}

/// What the row's tooltip says, as label and value: the kind, the name, when
/// it started and how long it took, the model and tokens a call reports, and
/// how it stands, with the error of one that failed.
pub(super) fn tooltip_lines(row: &TreeRow, elapsed: Duration) -> Vec<(&'static str, String)> {
    let kind = match row.kind {
        None => "Turn",
        Some(SpanKind::Subagent) => "Subagent",
        Some(SpanKind::Llm) => "Model call",
        Some(SpanKind::Tool) => "Tool",
    };
    let took = row.end.unwrap_or(elapsed).saturating_sub(row.start);
    let mut lines = vec![
        ("Kind", kind.to_string()),
        ("Name", row.name.clone()),
        ("Start", format!("+{}", short_duration(row.start))),
        ("Duration", short_duration(took)),
    ];
    if let Some(model) = row.model.as_ref().filter(|model| !model.is_empty()) {
        lines.push(("Model", model.clone()));
    }
    match (&row.tokens, row.progress_tokens) {
        (Some(tokens), _) => lines.push((
            "Tokens",
            format!(
                "{} in · {} out",
                compact_count(tokens.input),
                compact_count(tokens.output)
            ),
        )),
        (None, Some(total)) => lines.push(("Tokens", format!("{} so far", compact_count(total)))),
        (None, None) => {}
    }
    let word = row.state.word();
    let mut status = word[..1].to_uppercase();
    status.push_str(&word[1..]);
    lines.push(("Status", status));
    if let Some(note) = &row.note {
        lines.push(("Error", note.clone()));
    }
    lines
}

/// What a row is drawn with, once per frame.
pub(super) struct RowStyle {
    pub(super) theme: Theme,
    pub(super) range: Duration,
    pub(super) elapsed: Duration,
    pub(super) running: bool,
    pub(super) stacked: bool,
    pub(super) slowest: Option<String>,
    pub(super) selected: Option<String>,
    pub(super) entity: Entity<TraceTab>,
}

impl RowStyle {
    /// How far across the axis `offset` is, 0 to 1.
    pub(super) fn across(&self, offset: Duration) -> f32 {
        (offset.as_secs_f32() / self.range.as_secs_f32().max(0.001)).clamp(0.0, 1.0)
    }

    /// The colour of a row's bar: a failure is red, a model call violet, a
    /// tool teal, an agent from the grey ladder.
    fn bar_color(&self, row: &TreeRow) -> Hsla {
        let ely = &self.theme.ely;
        let color = if row.state == SpanState::Failed {
            ely.danger
        } else {
            match row.kind {
                Some(SpanKind::Llm) => ely.chart[4],
                Some(SpanKind::Tool) => ely.chart[1],
                Some(SpanKind::Subagent) => ely.fg_subtle.opacity(0.7),
                None => ely.border_strong,
            }
        };
        if row.state == SpanState::Interrupted {
            color.opacity(0.7)
        } else {
            color
        }
    }

    /// The colour of a row's glyph: the bar's, except for an agent, whose
    /// bar is a quiet grey and whose glyph must still be read.
    pub(super) fn glyph_color(&self, row: &TreeRow) -> Hsla {
        match row.kind {
            Some(SpanKind::Subagent) | None if row.state != SpanState::Failed => {
                self.theme.ely.fg_muted
            }
            _ => self.bar_color(row),
        }
    }

    /// The left column's width: a share of the tab, kept between two sizes.
    pub(super) fn name_column(&self) -> gpui::Div {
        div()
            .flex_none()
            .w(relative(0.34))
            .min_w(px(200.0))
            .max_w(px(360.0))
    }
}

fn kind_glyph(kind: &Option<SpanKind>) -> IconName {
    match kind {
        None => IconName::Bot,
        Some(SpanKind::Subagent) => IconName::Users,
        Some(SpanKind::Llm) => IconName::Sparkles,
        Some(SpanKind::Tool) => IconName::Wrench,
    }
}

fn selector(prefix: &'static str, span: &str) -> impl Fn() -> String + 'static {
    let id = format!("{prefix}-{span}");
    move || id.clone()
}

/// A row's left cell: the indent, a chevron when it has children, the kind's
/// glyph, the name, what the agent said of it, and how many rows a fold hides.
fn render_name(rows: &[TreeRow], at: usize, style: &RowStyle) -> AnyElement {
    let row = &rows[at];
    let theme = &style.theme;
    let glyph_color = style.glyph_color(row);
    let folded = row.hidden_children > 0;
    let chevron = if has_children(rows, at) {
        let entity = style.entity.clone();
        let span = row.span.clone();
        div()
            .id(("trace-chevron", at))
            .debug_selector(selector("trace-chevron", &row.span))
            .flex_none()
            .size(px(16.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::PointingHand)
            .on_click(move |_, _, cx| {
                // The row's own click would select and reveal it.
                cx.stop_propagation();
                entity.update(cx, |tab, cx| tab.set_folded(&span, !folded, cx));
            })
            .child(
                Icon::new(if folded {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                })
                .size(IconSize::Xs)
                .color(theme.ely.fg_subtle),
            )
            .into_any_element()
    } else {
        div().flex_none().size(px(16.0)).into_any_element()
    };
    style
        .name_column()
        .debug_selector(selector("trace-row-name", &row.span))
        .when(style.stacked, |cell| cell.w_full().max_w_full().min_w_0())
        .h(px(if style.stacked { 22.0 } else { ROW_HEIGHT }))
        .flex()
        .items_center()
        .gap(px(5.0))
        .pl(px(f32::from(row.depth) * INDENT))
        .child(chevron)
        .child(
            Icon::new(kind_glyph(&row.kind))
                .size(IconSize::Xs)
                .color(glyph_color),
        )
        .child(
            div()
                .flex_none()
                .max_w(relative(0.55))
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.ely.fg)
                .child(row.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.ely.fg_subtle)
                .child(row.detail.clone()),
        )
        .when(row.hidden_children > 0, |cell| {
            cell.child(
                div()
                    .flex_none()
                    .px(px(5.0))
                    .rounded(theme.radii.chip)
                    .bg(theme.sirio.overlay_strong)
                    .text_color(theme.ely.fg_muted)
                    .child(format!("+{}", row.hidden_children)),
            )
        })
        .into_any_element()
}

/// A row's right cell: the grid, its bar, the hatched stretches it waited,
/// the marks a folded parent shows for what it hides, the "now" line while
/// the turn runs, and the label that says how long it took.
fn render_track(row: &TreeRow, style: &RowStyle) -> AnyElement {
    let theme = &style.theme;
    let span = row.span.as_str();
    let start = style.across(row.start);
    let end_offset = row.end.unwrap_or(style.elapsed).max(row.start);
    let end = style.across(end_offset);
    let color = style.bar_color(row);
    let open_end = row.state == SpanState::Interrupted && row.end.is_some();
    let band = |track: gpui::Div| track.absolute().top(px(BAR_TOP)).h(px(BAR_HEIGHT));

    let bar = band(div())
        .debug_selector(selector("trace-bar", span))
        .left(relative(start))
        .w(relative((end - start).max(0.0)))
        .min_w(px(2.0))
        .rounded_l(px(3.0))
        .when(!open_end, |bar| bar.rounded_r(px(3.0)))
        .bg(color);
    let waits = row.waits.iter().enumerate().map(|(index, (from, to))| {
        let from_at = style.across(*from);
        let to_at = style.across(to.unwrap_or(style.elapsed));
        let id = format!("trace-wait-{span}-{index}");
        band(div())
            .debug_selector(move || id.clone())
            .left(relative(from_at))
            .w(relative((to_at - from_at).max(0.0)))
            .min_w(px(2.0))
            .rounded(px(2.0))
            .bg(pattern_slash(theme.ely.warning.opacity(0.85), 3.0, 6.0))
    });
    let marks = row.child_marks.iter().enumerate().map(|(index, mark)| {
        let id = format!("trace-mark-{span}-{index}");
        band(div())
            .debug_selector(move || id.clone())
            .left(relative(style.across(*mark)))
            .w(px(1.5))
            .bg(theme.ely.fg.opacity(0.75))
    });
    let frayed = open_end.then(|| {
        band(div())
            .debug_selector(selector("trace-open-end", span))
            .left(relative(end))
            .w(px(7.0))
            .bg(pattern_slash(color, 2.0, 4.0))
    });
    let grid = (0..=4).map(|tick| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(tick as f32 / 4.0))
            .w(px(1.0))
            .bg(theme.ely.border.opacity(0.7))
    });
    let now_line = style.running.then(|| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(style.across(style.elapsed)))
            .w(px(1.0))
            .bg(theme.ely.info.opacity(0.75))
    });

    // Its duration, on the side of the bar with room for it, and what else
    // there is to say of it.
    let took = end_offset.saturating_sub(row.start);
    let left_of_bar = end > LABEL_FLIPS_AT;
    let label = div()
        .absolute()
        .top_0()
        .h_full()
        .flex()
        .items_center()
        .gap(px(6.0))
        .whitespace_nowrap()
        .text_size(px(11.0))
        .text_color(theme.ely.fg_muted)
        .when(left_of_bar, |label| {
            label.right(relative(1.0 - start)).mr(px(6.0))
        })
        .when(!left_of_bar, |label| {
            label
                .left(relative(end))
                .ml(px(if open_end { 14.0 } else { 6.0 }))
        })
        .child(short_duration(took))
        .children(row.note.as_ref().map(|note| {
            div()
                .debug_selector(selector("trace-note", span))
                .max_w(px(240.0))
                .truncate()
                .text_color(theme.ely.danger)
                .child(note.clone())
        }))
        .children(
            (style.slowest.as_deref() == Some(span)).then(|| {
                div()
                    .debug_selector(selector("trace-slowest", span))
                    .px(px(5.0))
                    .rounded(theme.radii.chip)
                    .border_1()
                    .border_color(theme.ely.warning.opacity(0.5))
                    .bg(theme.ely.warning_subtle)
                    .text_color(theme.ely.warning)
                    .child("Slowest")
            }),
        );

    div()
        .debug_selector(selector("trace-row-track", span))
        .relative()
        .overflow_hidden()
        .when(style.stacked, |track| track.w_full().h(px(20.0)))
        .when(!style.stacked, |track| track.flex_1().h(px(ROW_HEIGHT)))
        .children(grid)
        .child(bar)
        .children(waits)
        .children(marks)
        .children(frayed)
        .children(now_line)
        .child(label)
        .into_any_element()
}

/// One row of the tree and its bar.
pub(super) fn render_row(rows: &[TreeRow], at: usize, style: &RowStyle) -> AnyElement {
    let row = &rows[at];
    let theme = &style.theme;
    let selected = style.selected.as_deref() == Some(row.span.as_str());
    let lines = Rc::new(tooltip_lines(row, style.elapsed));
    let entity = style.entity.clone();
    let span = row.span.clone();
    div()
        .id(("trace-row", at))
        .debug_selector(selector("trace-row", &row.span))
        .w_full()
        .h(px(if style.stacked {
            STACKED_ROW_HEIGHT
        } else {
            ROW_HEIGHT
        }))
        .px(px(12.0))
        .flex()
        .when(style.stacked, |row| row.flex_col().justify_center())
        .when(!style.stacked, |row| row.items_center())
        .cursor(CursorStyle::PointingHand)
        .when(selected, |row| row.bg(theme.ely.active))
        .when(!selected, |row| row.hover(|hovered| hovered.bg(theme.ely.hover)))
        .on_click(move |_, window, cx| {
            entity.update(cx, |tab, cx| tab.click_row(span.clone(), window, cx));
        })
        .tooltip(Tooltip::rich(move |_, _| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(lines.iter().map(|(label, value)| {
                    div()
                        .flex()
                        .gap_2()
                        .child(div().w(px(56.0)).opacity(0.6).child(*label))
                        .child(div().child(value.clone()))
                }))
                .into_any_element()
        }))
        .child(render_name(rows, at, style))
        .child(render_track(row, style))
        .into_any_element()
}

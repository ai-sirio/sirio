// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The settings page skeleton: the headline and its subtitle, the section
//! card, its rows, and the empty state and status dot they share. The rhythm
//! every settings page is drawn in, so a spacing change is a one-file change.
//!
//! Everything returns a plain `Div` (or a refinement) for the page to chain
//! its own ids, selectors and click handlers onto. Hover is the caller's:
//! gpui panics on a second `hover`, so [`card_row_hover`] is the default wash
//! for the caller to hand to `.hover(..)`.

use bezel_theme::{Theme, ink};
use ely_gpui_component::{
    primitives::{Icon, IconName},
    theme::IconSize,
};
use gpui::{AnyElement, Div, Hsla, SharedString, StyleRefinement, div, prelude::*, px};

pub(crate) trait Scaffolding {
    /// Page headline row: a `16px` semibold title and, when there is one, a
    /// `13px` count sharing its baseline.
    fn page_header(&self, title: impl Into<SharedString>, count: Option<usize>) -> Div;

    /// The subtitle under the headline: `13px`, muted.
    fn page_subtitle(&self, copy: impl Into<SharedString>) -> Div;

    /// The section card: bordered, rounded, and the card tone thinned to a
    /// translucent tint over glass ([`Theme::card_glass_bg`]), so it reads as
    /// frost on macOS instead of a solid slab.
    fn group_box(&self) -> Div;

    /// One card row. Rows past the first draw their own top border.
    fn card_row(&self, first: bool) -> Div;

    /// A row's title: `13.5px` medium, truncating.
    fn row_title(&self, title: impl Into<SharedString>) -> Div;

    /// The quiet meta line under a row title: `11.5px`, muted, its fragments
    /// joined by dots.
    fn meta_line(&self, fragments: Vec<AnyElement>) -> Div;
}

impl Scaffolding for Theme {
    fn page_header(&self, title: impl Into<SharedString>, count: Option<usize>) -> Div {
        div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(self.text)
                    .child(title.into()),
            )
            .when_some(count, |el, count| {
                el.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(self.text_muted.opacity(0.7))
                        .child(format!("{count}")),
                )
            })
    }

    fn page_subtitle(&self, copy: impl Into<SharedString>) -> Div {
        div()
            .mt(px(4.0))
            .text_size(px(13.0))
            .text_color(self.text_muted)
            .child(copy.into())
    }

    fn group_box(&self) -> Div {
        div()
            .mt(px(24.0))
            .rounded(px(Theme::surface_radius()))
            .border_1()
            .border_color(self.border)
            .bg(self.card_glass_bg())
            .overflow_hidden()
            .flex()
            .flex_col()
    }

    fn card_row(&self, first: bool) -> Div {
        div()
            .px(px(20.0))
            .py(px(14.0))
            .when(!first, |el| el.border_t_1().border_color(self.border))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(14.0))
    }

    fn row_title(&self, title: impl Into<SharedString>) -> Div {
        div()
            .min_w_0()
            .truncate()
            .text_size(px(13.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(self.text)
            .child(title.into())
    }

    fn meta_line(&self, fragments: Vec<AnyElement>) -> Div {
        let mut line = div()
            .mt(px(4.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x(px(8.0))
            .gap_y(px(2.0))
            .text_size(px(11.5))
            .text_color(self.text_muted.opacity(0.65));
        let mut first = true;
        for fragment in fragments {
            if !first {
                line = line.child(
                    div()
                        .text_color(self.text_muted.opacity(0.3))
                        .child(SharedString::from("·")),
                );
            }
            line = line.child(fragment);
            first = false;
        }
        line
    }
}

/// The default card-row hover wash.
pub(crate) fn card_row_hover(style: StyleRefinement) -> StyleRefinement {
    style.bg(ink(0.015))
}

/// The centered "nothing here yet" panel: an icon, a headline and one line of
/// hint.
pub(crate) fn empty_state(
    theme: &Theme,
    icon: IconName,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .py(px(40.0))
        .child(Icon::new(icon).size(IconSize::Xl).color(theme.text_faint))
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text_muted)
                .child(hint.into()),
        )
}

/// A small state dot, the bead on a row. It takes its tone from the caller, so
/// the meaning stays with the caller's domain.
pub(crate) fn status_dot(tone: Hsla) -> Div {
    div().flex_none().size(px(6.0)).rounded_full().bg(tone)
}

//! The History view's filter toolbar.
//!
//! Lives apart from `history.rs` because that file already carries the view's
//! state, loading and row rendering; folding eight controls into it as well
//! would put two unrelated concerns in one place.

use gpui::{
    Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use tiller_theme::Theme;

use super::history::GitHistory;

/// How much of the toolbar fits side by side at the panel's current width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolbarLayout {
    /// Field, toggles, four chips and IntelliSort on one line.
    OneRow,
    /// Field and toggles above; chips and IntelliSort below.
    TwoRows,
    /// Two rows, but the four chips share one `Filters (n)` popup.
    Collapsed,
}

/// Below this the search field would compress under ~190px, where it stops
/// being readable, so the chips move to their own row.
const ONE_ROW_MIN: f32 = 470.0;
/// Below this the four chips (~257px together) no longer fit on a row of
/// their own, so they collapse behind a single button.
const CHIP_ROW_MIN: f32 = 260.0;

/// The toolbar's shape at a given panel width.
///
/// The thresholds are derived rather than chosen: four chips measure ~257px
/// together and the field needs ~190px to stay readable, so one row needs
/// ~455px plus margin. At the panel's 220px minimum, 204px remain usable —
/// a 150px field and two 24px toggles.
pub(super) fn toolbar_layout(panel_width: f32) -> ToolbarLayout {
    if panel_width >= ONE_ROW_MIN {
        ToolbarLayout::OneRow
    } else if panel_width >= CHIP_ROW_MIN {
        ToolbarLayout::TwoRows
    } else {
        ToolbarLayout::Collapsed
    }
}

/// The search row: field, then the two toggles that change what the text
/// means rather than what it is.
pub(super) fn render_search_row(
    draft: &str,
    regex: bool,
    case_sensitive: bool,
    caret_visible: bool,
    focus: &FocusHandle,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let regex_entity = entity.clone();
    let case_entity = entity.clone();
    let key_entity = entity;
    div()
        .id("history-toolbar")
        .debug_selector(|| "history-toolbar".to_owned())
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(5.0))
        .child(
            div()
                .id("history-search-field")
                .debug_selector(|| "history-search-field".to_owned())
                .track_focus(focus)
                .on_key_down(move |event, _, cx| {
                    key_entity.update(cx, |history, cx| {
                        history.on_search_key(event, cx)
                    });
                })
                .flex_1()
                .min_w(px(0.0))
                .px(px(6.0))
                .py(px(3.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.hairline)
                .text_size(theme.typography.footnote)
                .text_color(if draft.is_empty() {
                    theme.meta
                } else {
                    theme.title
                })
                .flex()
                .items_center()
                .child(if draft.is_empty() {
                    "Text or hash".to_owned()
                } else {
                    draft.to_owned()
                })
                // `caret::bar` and not a `|` appended to the string: the bar
                // always occupies layout, so text does not shift as it
                // blinks. It is this repo's one way to draw a caret.
                .child(crate::caret::bar(
                    px(14.0),
                    theme.title,
                    caret_visible,
                )),
        )
        .child(toggle(".*", "history-search-regex", regex, theme, move |cx| {
            regex_entity.update(cx, |history, cx| {
                history.search_regex = !history.search_regex;
                history.apply_search_now(cx);
            });
        }))
        .child(toggle("Cc", "history-search-case", case_sensitive, theme, move |cx| {
            case_entity.update(cx, |history, cx| {
                history.search_case_sensitive = !history.search_case_sensitive;
                history.apply_search_now(cx);
            });
        }))
}

/// One of the two square toggles. They apply immediately rather than through
/// the debounce: a click is not typing, and waiting after one reads as a bug.
fn toggle(
    label: &'static str,
    selector: &'static str,
    on: bool,
    theme: Theme,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .w(px(24.0))
        .h(px(20.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if on { theme.title } else { theme.meta })
        .bg(if on { theme.row_hover } else { theme.background })
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundaries, not the middles. A layout exercised at 300 and 500px
    /// passes whether the threshold sits at 400 or at 470, which is why
    /// those are the numbers not tested here.
    #[test]
    fn the_toolbar_changes_shape_exactly_at_its_thresholds() {
        assert_eq!(toolbar_layout(469.0), ToolbarLayout::TwoRows);
        assert_eq!(toolbar_layout(470.0), ToolbarLayout::OneRow);
        assert_eq!(toolbar_layout(259.0), ToolbarLayout::Collapsed);
        assert_eq!(toolbar_layout(260.0), ToolbarLayout::TwoRows);
    }

    /// The panel clamps to 220..=640, so those two are the only widths the
    /// toolbar will ever actually be asked for at the extremes.
    #[test]
    fn both_ends_of_the_panels_range_have_a_shape() {
        assert_eq!(toolbar_layout(220.0), ToolbarLayout::Collapsed);
        assert_eq!(toolbar_layout(640.0), ToolbarLayout::OneRow);
    }

    /// A degenerate width must not panic or fall through to the widest
    /// shape: during the first frame, before layout has run, zero is a real
    /// value this can be called with.
    #[test]
    fn a_zero_width_collapses_rather_than_expanding() {
        assert_eq!(toolbar_layout(0.0), ToolbarLayout::Collapsed);
    }
}
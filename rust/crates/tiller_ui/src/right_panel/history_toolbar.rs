//! The History view's filter toolbar.
//!
//! Lives apart from `history.rs` because that file already carries the view's
//! state, loading and row rendering; folding eight controls into it as well
//! would put two unrelated concerns in one place.

use gpui::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use tiller_theme::Theme;

use super::history::GitHistory;

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
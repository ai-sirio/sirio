//! The History view's filter toolbar.
//!
//! Lives apart from `history.rs` because that file already carries the view's
//! state, loading and row rendering; folding eight controls into it as well
//! would put two unrelated concerns in one place.

use gpui::{
    App, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use sirio_theme::Theme;

use crate::loading;
use gpui::prelude::FluentBuilder as _;

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

/// Which of the four dropdowns a chip, a popup or a selection belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FilterChip {
    Branch,
    User,
    Date,
    Paths,
}

impl FilterChip {
    pub(super) fn label(self) -> &'static str {
        match self {
            FilterChip::Branch => "Branch",
            FilterChip::User => "User",
            FilterChip::Date => "Date",
            FilterChip::Paths => "Paths",
        }
    }

    pub(super) fn selector(self) -> &'static str {
        match self {
            FilterChip::Branch => "history-chip-branch",
            FilterChip::User => "history-chip-user",
            FilterChip::Date => "history-chip-date",
            FilterChip::Paths => "history-chip-paths",
        }
    }
}

/// The Date dropdown's fixed choices. The second element is what git is
/// given for `--since`; `None` clears the filter. Git parses these strings
/// itself, so this table never computes a date — which also means no test
/// of it needs a clock.
pub(super) const DATE_PRESETS: [(&str, Option<&str>); 4] = [
    ("Any time", None),
    ("Today", Some("midnight")),
    ("Last 7 days", Some("7 days ago")),
    ("Last 30 days", Some("30 days ago")),
];

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
    // `caret::bar` and not a `|` appended to the string: the bar always
    // occupies layout, so text does not shift as it blinks. It is this
    // repo's one way to draw a caret. An empty field carries it at the
    // hint's start (`caret::field_placeholder`), a value after its last
    // character.
    let search_caret = || crate::caret::bar(px(14.0), theme.text, caret_visible);
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
                    key_entity.update(cx, |history, cx| history.on_search_key(event, cx));
                })
                .flex_1()
                .min_w(px(0.0))
                .px(px(6.0))
                .py(px(3.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.border)
                .text_size(theme.typography.footnote)
                .text_color(if draft.is_empty() {
                    theme.text_faint
                } else {
                    theme.text
                })
                .flex()
                .items_center()
                // A long query is clipped from the start, so the tail being
                // typed stays in view (`caret::field_value`).
                .overflow_hidden()
                .child(
                    if draft.is_empty() {
                        crate::caret::field_placeholder(
                            "Text or hash".to_owned(),
                            Some(search_caret()),
                        )
                    } else {
                        crate::caret::field_value(draft.to_owned())
                    }
                    .debug_selector(|| "history-search-text".to_owned()),
                )
                .children((!draft.is_empty()).then(search_caret)),
        )
        .child(toggle(
            ".*",
            "history-search-regex",
            regex,
            theme,
            move |cx| {
                regex_entity.update(cx, |history, cx| {
                    history.search_regex = !history.search_regex;
                    history.apply_search_now(cx);
                });
            },
        ))
        .child(toggle(
            "Cc",
            "history-search-case",
            case_sensitive,
            theme,
            move |cx| {
                case_entity.update(cx, |history, cx| {
                    history.search_case_sensitive = !history.search_case_sensitive;
                    history.apply_search_now(cx);
                });
            },
        ))
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
        .text_color(if on { theme.text } else { theme.text_faint })
        .bg(if on {
            theme.element_hover
        } else {
            theme.surface
        })
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(label)
}

/// One dropdown button. `active` is how many options that chip currently
/// selects; it is shown so a collapsed or scrolled-past filter is never
/// silently on.
pub(super) fn render_filter_chip(
    chip: FilterChip,
    active: usize,
    open: bool,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let selector = chip.selector();
    let label = if active == 0 {
        chip.label().to_owned()
    } else {
        format!("{} ({active})", chip.label())
    };
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.0))
        .px(px(6.0))
        .py(px(3.0))
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if active > 0 {
            theme.text
        } else {
            theme.text_faint
        })
        .bg(if open {
            theme.element_hover
        } else {
            theme.surface
        })
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| {
            entity.update(cx, |history, cx| {
                history.open_chip = (history.open_chip != Some(chip)).then_some(chip);
                cx.notify();
            });
        })
        .child(label)
        .child("⌄")
}

/// The option list for an open chip. Multi-select: clicking an option
/// toggles it and leaves the popup open, because choosing two branches
/// should not cost two trips.
pub(super) fn render_chip_popup(
    chip: FilterChip,
    options: &[String],
    selected: &[String],
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let mut list = div()
        .id("history-chip-popup")
        .debug_selector(|| "history-chip-popup".to_owned())
        .absolute()
        .top(px(26.0))
        .w(theme.spacing.menu_width)
        .max_h(px(240.0))
        .overflow_hidden()
        .p(px(4.0))
        .rounded(theme.radii.user_pill)
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_raised)
        .shadow_lg();

    if chip == FilterChip::User {
        list = list.child(
            div()
                .px(px(6.0))
                .py(px(3.0))
                .text_size(theme.typography.caption2)
                .text_color(theme.text_faint)
                .child("Authors in the loaded history"),
        );
    }

    if chip == FilterChip::Date {
        for (label, since) in DATE_PRESETS {
            let value = since.map(ToOwned::to_owned);
            let is_selected = selected.first().map(String::as_str) == since;
            let row_entity = entity.clone();
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("history-date-{label}")))
                    .w_full()
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, cx| {
                        row_entity.update(cx, |history, cx| {
                            history.set_date_preset(value.clone(), cx);
                            history.open_chip = None;
                            cx.notify();
                        });
                    })
                    .child(if is_selected { "✓" } else { " " })
                    .child(label),
            );
        }
        return list;
    }

    if options.is_empty() {
        return list.child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.text_faint)
                .child("Nothing to choose from"),
        );
    }

    for option in options {
        let is_selected = selected.iter().any(|value| value == option);
        let value = option.clone();
        let row_entity = entity.clone();
        list = list.child(
            div()
                .id(gpui::SharedString::from(format!(
                    "history-chip-option-{option}"
                )))
                .w_full()
                .px(px(6.0))
                .py(px(4.0))
                .rounded(theme.radii.control)
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.text)
                .hover(|style| style.bg(theme.element_hover))
                .on_click(move |_, _, cx| {
                    row_entity.update(cx, |history, cx| {
                        history.toggle_chip_option(chip, value.clone(), cx);
                    });
                })
                .child(if is_selected { "✓" } else { " " })
                .child(option.clone()),
        );
    }
    list
}

/// The Paths chip's popup: one free-text row bound to `path_draft`, applied
/// on Enter via `set_path_filter`. Key handling follows the search field's
/// `on_search_key`.
pub(super) fn render_paths_popup(
    draft: &str,
    caret_visible: bool,
    focus: &FocusHandle,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let key_entity = entity;
    // Same `caret::bar` the search row uses: it always occupies layout, so
    // the pathspec does not shift by two pixels every half second as the
    // bar blinks. An empty field carries it at the hint's start.
    let path_caret = || {
        div()
            .debug_selector(|| "history-path-caret".to_owned())
            .child(crate::caret::bar(px(14.0), theme.text, caret_visible))
            .into_any_element()
    };
    div()
        .id("history-paths-popup")
        .debug_selector(|| "history-paths-popup".to_owned())
        .absolute()
        .top(px(26.0))
        .w(theme.spacing.menu_width)
        .p(px(4.0))
        .rounded(theme.radii.user_pill)
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_raised)
        .shadow_lg()
        .child(
            div()
                .id("history-path-field")
                .debug_selector(|| "history-path-field".to_owned())
                .track_focus(focus)
                .on_key_down(move |event, _, cx| {
                    key_entity.update(cx, |history, cx| history.on_path_key(event, cx));
                })
                .w_full()
                .px(px(6.0))
                .py(px(3.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.border)
                .text_size(theme.typography.footnote)
                .text_color(if draft.is_empty() {
                    theme.text_faint
                } else {
                    theme.text
                })
                .flex()
                .items_center()
                .overflow_hidden()
                .child(
                    if draft.is_empty() {
                        crate::caret::field_placeholder(
                            "Path or glob".to_owned(),
                            Some(path_caret()),
                        )
                    } else {
                        crate::caret::field_value(draft.to_owned())
                    }
                    .debug_selector(|| "history-path-text".to_owned()),
                )
                .children((!draft.is_empty()).then(path_caret)),
        )
}

/// IntelliSort: ordering, not filtering. The toolbar's own square-toggle
/// helper, with the double arrow for a sort that is not a filter.
pub(super) fn render_intellisort(
    on: bool,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let click_entity = entity;
    toggle("⇅", "history-intellisort", on, theme, move |cx| {
        click_entity.update(cx, |history, cx| history.toggle_topo_order(cx));
    })
}

/// The single button that stands in for the four chips below ~260px. The
/// count is what keeps a hidden filter from being silently on.
pub(super) fn render_collapsed_chips(
    active: usize,
    open: bool,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let click_entity = entity;
    let label = format!("Filters ({active})");
    div()
        .id("history-chips-collapsed")
        .debug_selector(|| "history-chips-collapsed".to_owned())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.0))
        .px(px(6.0))
        .py(px(3.0))
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if active > 0 {
            theme.text
        } else {
            theme.text_faint
        })
        .bg(if open {
            theme.element_hover
        } else {
            theme.surface
        })
        .hover(|style| style.bg(theme.element_hover))
        .on_click(move |_, _, cx| {
            // ponytail: the combined four-in-one popup is not drawn yet; the
            // click only dismisses any open chip popup until it lands.
            click_entity.update(cx, |history, cx| {
                history.open_chip = None;
                cx.notify();
            });
        })
        .child(label)
}

/// The whole toolbar at the current width.
///
/// `OneRow` puts field, toggles, chips and IntelliSort on one line;
/// `TwoRows` moves the chips to a line of their own; `Collapsed` replaces
/// the four chips with a single `Filters (n)` button whose popup holds them.
/// The count on that button is why a collapsed filter is never silently on.
pub(super) fn render_toolbar(
    layout: ToolbarLayout,
    history: &GitHistory,
    entity: Entity<GitHistory>,
    theme: Theme,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let search = render_search_row(
        &history.search_draft,
        history.search_regex,
        history.search_case_sensitive,
        history.search_caret_visible,
        history.search_focus_handle(),
        entity.clone(),
        theme,
    );
    let chips = [
        FilterChip::Branch,
        FilterChip::User,
        FilterChip::Date,
        FilterChip::Paths,
    ];
    // The count is what keeps a filter from being silently on when its chip
    // is off-screen or folded away.
    let active_of = |chip: FilterChip| -> usize {
        match chip {
            FilterChip::Branch => history.filter.branches.len(),
            FilterChip::User => history.filter.authors.len(),
            FilterChip::Date => usize::from(history.filter.since.is_some()),
            FilterChip::Paths => history.filter.paths.len(),
        }
    };

    let mut chip_row = div().relative().flex().items_center().gap(px(6.0));
    if layout == ToolbarLayout::Collapsed {
        let total: usize = chips.iter().copied().map(active_of).sum();
        chip_row = chip_row.child(render_collapsed_chips(
            total,
            history.open_chip.is_some(),
            entity.clone(),
            theme,
        ));
    } else {
        for chip in chips {
            chip_row = chip_row.child(render_filter_chip(
                chip,
                active_of(chip),
                history.open_chip == Some(chip),
                entity.clone(),
                theme,
            ));
        }
    }
    chip_row = chip_row.child(render_intellisort(
        history.filter.topo_order,
        entity.clone(),
        theme,
    ));
    if let Some(open) = history.open_chip {
        if open == FilterChip::Paths {
            chip_row = chip_row.child(render_paths_popup(
                &history.path_draft,
                history.path_caret_visible,
                history.path_focus_handle(),
                entity.clone(),
                theme,
            ));
        } else {
            chip_row = chip_row.child(render_chip_popup(
                open,
                &history.options_for(open),
                &history.selection_for(open),
                entity.clone(),
                theme,
            ));
        }
    }

    let refreshing = history.is_loading();
    let mut toolbar = div().w_full().flex_none().flex();
    if layout == ToolbarLayout::OneRow {
        toolbar = toolbar.flex_row().items_center().gap(px(6.0));
    } else {
        toolbar = toolbar.flex_col().gap(px(4.0));
    }
    toolbar
        .child(search)
        .child(chip_row)
        .when(refreshing, |this| {
            this.child(
                div()
                    .id("history-refresh")
                    .debug_selector(|| "history-refresh".to_owned())
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(loading::compact("history-refresh-spinner", window, cx)),
            )
        })
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

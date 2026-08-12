//! Reusable controls used by the settings surface.

use gpui::{App, ClickEvent, Div, FontWeight, Rgba, Window, div, prelude::*, px, text};
use std::rc::Rc;
use tiller_theme::Theme;

/// Creates a titled settings section with a card beneath it.
pub fn section(title: &'static str, card: Div, theme: Theme) -> impl IntoElement {
    div()
        .id(format!("settings-section-{title}"))
        .w_full()
        .mb(px(28.0))
        .child(
            div()
                .mb(px(9.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.title)
                .child(text!(id = format!("settings-section-title-{title}"), title)),
        )
        .child(card)
}

/// Creates the rounded surface containing a group of settings rows.
pub fn card(theme: Theme) -> Div {
    div()
        .w_full()
        .rounded(px(12.0))
        .overflow_hidden()
        .bg(theme.card_fill)
}

/// Creates one labelled row. `description` adds the secondary line used by
/// controls such as the font-size steppers.
pub fn row(
    label: &'static str,
    description: Option<String>,
    control: impl IntoElement,
    theme: Theme,
) -> impl IntoElement {
    let mut label_view = div()
        .flex()
        .flex_col()
        .justify_center()
        .flex_1()
        .text_size(px(13.0))
        .text_color(theme.title)
        .child(text!(id = format!("settings-row-label-{label}"), label));

    if let Some(description) = description {
        label_view = label_view.child(
            div()
                .mt(px(2.0))
                .text_size(px(11.0))
                .text_color(theme.subtitle)
                .child(description),
        );
    }

    div()
        .id(format!("settings-row-{label}"))
        .w_full()
        .child(row_view(label_view, control, theme))
}

/// Creates a row from a pre-built label view. This is used when a row needs
/// an icon or a status marker before its text, while retaining the same row
/// geometry as [`row`].
pub fn row_view(label_view: Div, control: impl IntoElement, _theme: Theme) -> Div {
    div()
        .min_h(px(44.0))
        .w_full()
        .px(px(10.0))
        .py(px(7.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(label_view.flex_1())
        .child(control)
}

/// Adds a one-pixel separator between rows in a card.
pub fn separator(theme: Theme) -> Div {
    div().mx(px(10.0)).h(px(1.0)).bg(theme.hairline)
}

/// A compact on/off switch.
pub fn toggle<F>(id: &'static str, on: bool, theme: Theme, callback: F) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .relative()
        .w(px(36.0))
        .h(px(20.0))
        .rounded(px(10.0))
        .bg(if on {
            theme.tab_focus_accent
        } else {
            theme.hairline
        })
        .hover(|style| style.opacity(0.9))
        .on_click(callback)
        .child(
            div()
                .absolute()
                .top(px(3.0))
                .left(px(if on { 19.0 } else { 3.0 }))
                .w(px(14.0))
                .h(px(14.0))
                .rounded(px(7.0))
                .bg(theme.title_selected),
        )
}

/// A segmented control with one selected blue segment.
pub fn segmented(
    id: &'static str,
    options: &'static [&'static str],
    selected: usize,
    theme: Theme,
    callback: impl Fn(usize, &mut App) + 'static,
) -> impl IntoElement {
    let callback: Rc<dyn Fn(usize, &mut App)> = Rc::new(callback);
    let mut control = div()
        .id(id)
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(theme.primary_pill_bg)
        .p(px(2.0));

    for (index, label) in options.iter().enumerate() {
        let callback = callback.clone();
        let active = index == selected;
        control = control.child(
            div()
                .id(format!("{id}-{index}"))
                .h(px(22.0))
                .min_w(px(56.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .text_size(px(12.0))
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if active {
                    theme.title_selected
                } else {
                    theme.subtitle
                })
                .when(active, |this| this.bg(theme.tab_focus_accent))
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| callback(index, cx))
                .child(text!(id = ("segmented-option", index), *label)),
        );
    }

    control
}

/// A numeric stepper with a value label and up/down controls.
pub fn stepper<F>(id: &'static str, value: i32, theme: Theme, callback: F) -> impl IntoElement
where
    F: Fn(i32, &mut App) + Clone + 'static,
{
    stepper_with_unit(id, value, "pt", theme, callback)
}

/// A numeric stepper whose value can use a domain-specific unit.
pub fn stepper_with_unit<F>(
    id: &'static str,
    value: i32,
    unit: &'static str,
    theme: Theme,
    callback: F,
) -> impl IntoElement
where
    F: Fn(i32, &mut App) + Clone + 'static,
{
    let decrement = callback.clone();
    let increment = callback;
    div()
        .id(id)
        .h(px(28.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(theme.primary_pill_bg)
        .child(
            div()
                .id(format!("{id}-decrement"))
                .w(px(28.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| decrement(value - 1, cx))
                .child("⌄"),
        )
        .child(
            div()
                .min_w(px(48.0))
                .px(px(4.0))
                .flex()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.title)
                .child(format!("{value} {unit}")),
        )
        .child(
            div()
                .id(format!("{id}-increment"))
                .w(px(28.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| increment(value + 1, cx))
                .child("⌃"),
        )
}

/// A short status/action badge, matching the pills used by Settings rows.
pub fn badge(label: &'static str, background: Rgba, foreground: Rgba) -> Div {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(7.0))
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(foreground)
        .bg(background)
        .child(text!(id = format!("settings-badge-{label}"), label))
}

/// A heading and supporting copy for a nested subsection inside a card.
pub fn subsection_header(
    title: &'static str,
    description: &'static str,
    action: impl IntoElement,
    theme: Theme,
) -> Div {
    div()
        .w_full()
        .px(px(10.0))
        .pt(px(10.0))
        .pb(px(5.0))
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(13.0))
                        .text_color(theme.title)
                        .child(text!(id = format!("settings-subsection-title-{title}"), title)),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.subtitle)
                        .child(text!(id = format!("settings-subsection-description-{title}"), description)),
                ),
        )
        .child(action)
}

/// A full-width row containing a single left-aligned action button.
pub fn action_row(action: impl IntoElement, theme: Theme) -> Div {
    div()
        .min_h(px(44.0))
        .w_full()
        .px(px(10.0))
        .py(px(7.0))
        .flex()
        .items_center()
        .child(action)
        .bg(theme.card_fill)
}

/// A compact account row with the two status pills used by provider cards.
pub fn account_row(label: &'static str, subtitle: &'static str, active: bool, theme: Theme) -> Div {
    let mut badges = div().flex().items_center().gap(px(5.0)).child(badge(
        "This device",
        theme.primary_pill_bg,
        theme.title,
    ));
    if active {
        badges = badges.child(badge(
            "Active",
            theme.tab_focus_accent,
            theme.title_selected,
        ));
    }

    div()
        .w_full()
        .px(px(10.0))
        .py(px(3.0))
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(13.0))
                                .text_color(theme.title)
                                .child(text!(id = format!("settings-account-label-{label}"), label)),
                        )
                        .child(badges),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.subtitle)
                        .child(text!(id = format!("settings-account-subtitle-{label}"), subtitle)),
                ),
        )
}

/// A plain text action button.
pub fn button<F>(
    id: &'static str,
    label: &'static str,
    theme: Theme,
    callback: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(6.0))
        .text_size(px(12.0))
        .text_color(theme.title)
        .bg(theme.primary_pill_bg)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(callback)
        .child(text!(id = format!("settings-button-{id}"), label))
}

/// A colour swatch pill used for agent accent values.
pub fn color_swatch(id: &'static str, color: Rgba, theme: Theme) -> impl IntoElement {
    div()
        .id(id)
        .w(px(44.0))
        .h(px(22.0))
        .rounded(px(11.0))
        .bg(color)
        .border_2()
        .border_color(theme.hairline)
}

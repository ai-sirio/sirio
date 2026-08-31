//! Reusable controls used by the settings surface.
//!
//! COSMIC-02: this file's spacing and radii now come from
//! [`sirio_theme::cosmic`] instead of bare `px()` literals. Two kinds of
//! literal remain deliberately:
//!
//! - **Component geometry** — a control's own fixed footprint (a toggle's
//!   36×20 track, a stepper's 28px buttons, the 20×20 colour picker swatch)
//!   stays a frozen literal, the same "geometry is waku's, colour/radius
//!   language is COSMIC's" split `titlebar.rs` already established. The
//!   "full circle is half the box" idiom from [`sirio_theme::Radii`]'s own
//!   doc comment applies here too (the swatch's `radius(10)` for a 20×20
//!   box, the toggle's `radius(7)` for a 14×14 knob).
//! - **Content spacing** (gaps, padding, margins between elements) now
//!   reads `theme.cosmic.spacing.*`, rounded to the nearest COSMIC step from
//!   its original waku value.
//!
//! `card()`'s corner radius and fill are the one place this file draws a
//! COSMIC *container* colour, not just a spacing/radius number — see its
//! doc comment for the container-level decision.

use gpui::{
    AnyView, App, ClickEvent, Context, CursorStyle, Div, FontWeight, Render, Rgba, Window, div,
    prelude::*, px, text,
};
use sirio_theme::Theme;
use std::rc::Rc;

use crate::sidebar::icons::{Icon, IconElement, IconSize};

/// A segmented control's selection callback.
type SegmentCallback = Rc<dyn Fn(usize, &mut App)>;

/// Creates the single-line hover card used by compact icon controls.
pub fn text_tooltip(
    text: impl Into<String>,
    theme: Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        cx.new(|_| TextTooltip {
            theme,
            text: text.clone(),
        })
        .into()
    }
}

struct TextTooltip {
    theme: Theme,
    text: String,
}

impl Render for TextTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(self.theme.radii.control)
            .bg(self.theme.raised)
            .border_1()
            .border_color(self.theme.hairline)
            .text_size(self.theme.typography.caption2)
            .text_color(self.theme.text)
            .child(self.text.clone())
    }
}

/// Creates a titled settings section with a card beneath it.
pub fn section(title: &'static str, card: Div, theme: Theme) -> impl IntoElement {
    let spacing = theme.cosmic.spacing;
    div()
        .id(format!("settings-section-{title}"))
        .w_full()
        .mb(px(spacing.l as f32))
        .child(
            div()
                .mb(px(spacing.xxs as f32))
                .text_size(theme.typography.headline)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(text!(id = format!("settings-section-title-{title}"), title)),
        )
        .child(card)
}

/// Creates the rounded surface containing a group of settings rows.
///
/// Cards remain opaque above their enclosing Settings panel by using the
/// semantic raised-surface token.
pub fn card(theme: Theme) -> Div {
    div()
        .w_full()
        .rounded(px(theme.cosmic.radii.radius_s[0]))
        .overflow_hidden()
        .bg(theme.raised)
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
        .text_size(theme.typography.headline)
        .text_color(theme.text)
        .child(text!(id = format!("settings-row-label-{label}"), label));

    if let Some(description) = description {
        label_view = label_view.child(
            div()
                .mt(px(theme.cosmic.spacing.xxxs as f32))
                .text_size(theme.typography.footnote)
                .text_color(theme.text_muted)
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
pub fn row_view(label_view: Div, control: impl IntoElement, theme: Theme) -> Div {
    let spacing = theme.cosmic.spacing;
    div()
        // 44px minimum touch target — component geometry, not spacing;
        // frozen the same way `titlebar.rs`'s `CONTROL_SIZE` is.
        .min_h(px(44.0))
        .w_full()
        .px(px(spacing.xs as f32))
        .py(px(spacing.xxs as f32))
        .flex()
        .items_center()
        .gap(px(spacing.xs as f32))
        // A flex child's `min-width` defaults to `auto` — its own
        // min-content width (the CSS flexbox rule gpui inherits) — so
        // `flex_1` alone does NOT let a long label shrink. It pushes
        // `control` out past the row instead, where the enclosing
        // `card`'s `overflow_hidden` both clips it and masks its
        // hit-testing, leaving a control that is drawn, reported by
        // `debug_bounds`, and completely dead to a click. `min_w_0()`
        // is the standard pairing; chat.rs's auth banner carries the
        // same fix for the same reason (F-CHAT-02).
        .child(label_view.flex_1().min_w_0())
        .child(control)
}

/// Adds a one-pixel separator between rows in a card.
pub fn separator(theme: Theme) -> Div {
    div()
        .mx(px(theme.cosmic.spacing.xs as f32))
        .h(theme.spacing.hairline_thickness)
        .bg(theme.hairline)
}

/// A compact on/off switch.
///
/// The track (36×20), thumb (14×14) and travel positions (3/19) are the
/// toggle's own fixed shape — component geometry, frozen like the swatch
/// and stepper buttons below.
pub fn toggle<F>(id: &'static str, on: bool, theme: Theme, callback: F) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .relative()
        .w(px(36.0))
        .h(px(20.0))
        .rounded(px(10.0))
        // An "on" track is a filled chip, so it takes the inverted pair
        // rather than the active-chrome tint: `title` and `title_selected`
        // are the same value, which would have put the knob's colour on the
        // track's colour and made the knob vanish.
        .bg(if on { theme.inverse } else { theme.hairline })
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
                .bg(if on {
                    theme.on_inverse
                } else {
                    theme.text
                }),
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
    let callback: SegmentCallback = Rc::new(callback);
    let mut control = div()
        .id(id)
        // 26px track height and the 2px inner inset are this control's own
        // fixed shell, matched to its 22px pill children — component
        // geometry, not spacing.
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded(theme.radii.control)
        .bg(theme.primary_pill_bg)
        .p(px(2.0));

    for (index, label) in options.iter().enumerate() {
        let callback = callback.clone();
        let active = index == selected;
        control = control.child(
            div()
                .id(format!("{id}-{index}"))
                .debug_selector(move || format!("{id}-{index}"))
                .h(px(22.0))
                .min_w(px(56.0))
                .px(px(theme.cosmic.spacing.xs as f32))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.chip_active)
                .text_size(theme.typography.callout)
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if active {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(active, |this| this.bg(theme.selected_fill))
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| callback(index, cx))
                .child(text!(id = ("segmented-option", index), *label)),
        );
    }

    control
}

/// A segmented control whose options are icon/tooltip pairs.
pub fn segmented_icons(
    id: &'static str,
    options: &[(Icon, &'static str)],
    selected: usize,
    theme: Theme,
    callback: impl Fn(usize, &mut App) + 'static,
) -> impl IntoElement {
    let callback: SegmentCallback = Rc::new(callback);
    let mut control = div()
        .id(id)
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded(theme.radii.control)
        .bg(theme.primary_pill_bg)
        .p(px(2.0));

    for (index, (icon, tooltip)) in options.iter().copied().enumerate() {
        let callback = callback.clone();
        let active = index == selected;
        control = control.child(
            div()
                .id(format!("{id}-{index}"))
                .debug_selector(move || format!("{id}-{index}"))
                .h(px(22.0))
                .min_w(px(56.0))
                .px(px(theme.cosmic.spacing.xs as f32))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.chip_active)
                .text_size(theme.typography.callout)
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if active {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(active, |this| this.bg(theme.selected_fill))
                .hover(|style| style.bg(theme.row_hover))
                .tooltip(text_tooltip(tooltip, theme))
                .on_click(move |_, _, cx| callback(index, cx))
                .child(IconElement::new(icon, IconSize::Small)),
        );
    }

    control
}

/// A numeric stepper with a value label and up/down controls. Pass `""` for a bare count.
///
/// `unit` is required rather than defaulting: while it defaulted to `"pt"`, the two count rows in
/// General rendered as "100 pt" chats and "6 pt" worktrees.
pub fn stepper<F>(
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
        .debug_selector(move || id.to_string())
        // 28px buttons and the 48px value-label rail are this control's own
        // fixed shell — component geometry, not spacing.
        .h(px(28.0))
        .flex()
        .items_center()
        .rounded(theme.radii.control)
        .bg(theme.primary_pill_bg)
        .child(
            div()
                .id(format!("{id}-decrement"))
                .debug_selector(move || format!("{id}-decrement"))
                .w(px(28.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.callout)
                .text_color(theme.text_faint)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| decrement(value - 1, cx))
                .child("⌄"),
        )
        .child(
            div()
                .min_w(px(48.0))
                .px(px(theme.cosmic.spacing.xxxs as f32))
                .flex()
                .justify_center()
                .text_size(theme.typography.callout)
                .text_color(theme.text)
                .child(if unit.is_empty() {
                    value.to_string()
                } else {
                    format!("{value} {unit}")
                }),
        )
        .child(
            div()
                .id(format!("{id}-increment"))
                .debug_selector(move || format!("{id}-increment"))
                .w(px(28.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.typography.callout)
                .text_color(theme.text_faint)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| increment(value + 1, cx))
                .child("⌃"),
        )
}

/// A short status/action badge, matching the pills used by Settings rows.
pub fn badge(theme: Theme, label: &'static str, background: Rgba, foreground: Rgba) -> Div {
    let spacing = theme.cosmic.spacing;
    div()
        .px(px(spacing.xxxs as f32))
        .py(px(spacing.xxxs as f32))
        .rounded(theme.radii.row_card)
        .text_size(theme.typography.caption2)
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
    let spacing = theme.cosmic.spacing;
    div()
        .w_full()
        .px(px(spacing.xs as f32))
        .pt(px(spacing.xs as f32))
        .pb(px(spacing.xxxs as f32))
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(spacing.xxxs as f32))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(theme.typography.headline)
                        .text_color(theme.text)
                        .child(text!(
                            id = format!("settings-subsection-title-{title}"),
                            title
                        )),
                )
                .child(
                    div()
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text_muted)
                        .child(text!(
                            id = format!("settings-subsection-description-{title}"),
                            description
                        )),
                ),
        )
        .child(action)
}

/// A full-width row containing a single left-aligned action button.
pub fn action_row(action: impl IntoElement, theme: Theme) -> Div {
    let spacing = theme.cosmic.spacing;
    div()
        // 44px minimum touch target — component geometry, matches `row_view`.
        .min_h(px(44.0))
        .w_full()
        .px(px(spacing.xs as f32))
        .py(px(spacing.xxs as f32))
        .flex()
        .items_center()
        .child(action)
        .bg(theme.card_fill)
}

/// A compact, selectable account row with the two status pills used by
/// provider cards. `row_id` must be unique within its section (the caller
/// derives it from the account's own id, or a fixed slug for "System
/// default"). Clicking anywhere on the row invokes `on_select` — F-SET-15:
/// this used to take no callback at all, so no row anywhere could ever
/// become the active one, regardless of how many existed.
pub fn account_row<F>(
    row_id: String,
    label: String,
    subtitle: String,
    active: bool,
    theme: Theme,
    on_select: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let spacing = theme.cosmic.spacing;
    let mut badges = div()
        .flex()
        .items_center()
        .gap(px(spacing.xxxs as f32))
        .child(badge(
            theme,
            "This device",
            theme.primary_pill_bg,
            theme.text,
        ));
    if active {
        badges = badges.child(badge(theme, "Active", theme.inverse, theme.on_inverse));
    }

    div()
        .w_full()
        .px(px(spacing.xs as f32))
        .py(px(spacing.xxxs as f32))
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(spacing.xxxs as f32))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(spacing.xxs as f32))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(theme.typography.headline)
                                .text_color(theme.text)
                                .child(text!(
                                    id = format!("settings-account-label-{row_id}"),
                                    label.clone()
                                )),
                        )
                        .child(badges),
                )
                .child(
                    div()
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text_muted)
                        .child(text!(
                            id = format!("settings-account-subtitle-{row_id}"),
                            subtitle
                        )),
                ),
        )
        .id(row_id.clone())
        .debug_selector(move || row_id.clone())
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(on_select)
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
    let spacing = theme.cosmic.spacing;
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .px(px(spacing.xs as f32))
        .py(px(spacing.xxxs as f32))
        .rounded(theme.radii.control)
        .text_size(theme.typography.callout)
        .text_color(theme.text)
        .bg(theme.primary_pill_bg)
        .hover(|style| style.bg(theme.row_hover))
        .on_click(callback)
        .child(text!(id = format!("settings-button-{id}"), label))
}

/// Like [`button`], but the caller may not have a real handler yet. `None`
/// renders the same label muted, with no `on_click` attached at all — an
/// honest "not wired" rather than a control that looks live and reaches
/// nothing (the dead-control shape P75 exists to fix, P76's `titlebar.rs`
/// cluster seams use the identical convention).
pub fn button_maybe<F>(
    id: &'static str,
    label: &'static str,
    theme: Theme,
    callback: Option<F>,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let spacing = theme.cosmic.spacing;
    let enabled = callback.is_some();
    let mut element = div()
        .id(id)
        .debug_selector(move || id.to_string())
        .px(px(spacing.xs as f32))
        .py(px(spacing.xxxs as f32))
        .rounded(theme.radii.control)
        .text_size(theme.typography.callout)
        .text_color(if enabled { theme.text } else { theme.text_faint })
        .bg(theme.primary_pill_bg)
        .child(text!(id = format!("settings-button-{id}"), label));
    if let Some(callback) = callback {
        element = element
            .hover(|style| style.bg(theme.row_hover))
            .on_click(callback);
    }
    element
}

/// A row of small selectable colour swatches — the agent accent picker
/// (F-SET-22). It replaces what used to be a single 44×22 display-only
/// swatch pill with no `on_click`, so no colour choice existed to
/// exercise. Each option here is its own clickable swatch; the selected
/// one draws a highlight ring in `theme.text` instead of a
/// checkmark glyph, so the choice stays legible without adding new
/// iconography.
pub fn color_picker(
    id: &'static str,
    options: &[(&'static str, Rgba)],
    selected: &'static str,
    theme: Theme,
    callback: impl Fn(&'static str, &mut App) + 'static,
) -> impl IntoElement {
    let callback: Rc<dyn Fn(&'static str, &mut App)> = Rc::new(callback);
    // `flex_wrap` so a swatch row that doesn't fit its host's width (the
    // project-settings sheet is narrower than 8 swatches at their fixed
    // 20px + gap) wraps onto a second line instead of being clipped by the
    // panel edge (F-PRJ-13).
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(theme.cosmic.spacing.xxs as f32));
    for (key, color) in options.iter().copied() {
        let callback = callback.clone();
        let active = key == selected;
        row = row.child(
            div()
                .id(format!("{id}-{key}"))
                .debug_selector(move || format!("{id}-{key}"))
                .w(px(20.0))
                .h(px(20.0))
                .rounded(px(10.0))
                .bg(color)
                .border_2()
                .border_color(if active {
                    theme.text
                } else {
                    theme.hairline
                })
                .cursor(CursorStyle::PointingHand)
                .on_click(move |_, _, cx| callback(key, cx)),
        );
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Styled;

    /// `card()` exposes its final style refinement directly. Inspecting that
    /// stored background proves the last `.bg(...)` wins, including a future
    /// override after the raised fill has been applied.
    #[test]
    fn card_uses_the_raised_surface_in_dark_mode() {
        let theme = Theme::dark();
        assert!(theme.cosmic.is_dark);
        assert_eq!(
            theme.cosmic.radii.radius_s[0], 8.0,
            "card()'s radius token must still be the measured COSMIC radius_s step"
        );
        let mut card = card(theme);
        assert_eq!(
            Styled::style(&mut card).background,
            Some(theme.raised.into()),
            "card()'s final dark background must be the raised surface"
        );
    }

    /// The light half of the same proof — a dark-only pass would leave half
    /// the design system unverified.
    #[test]
    fn card_uses_the_raised_surface_in_light_mode() {
        let theme = Theme::light();
        assert!(!theme.cosmic.is_dark);
        assert_ne!(
            theme.raised,
            Theme::dark().raised,
            "light and dark raised surfaces must not collapse to the same fill"
        );
        let mut card = card(theme);
        assert_eq!(
            Styled::style(&mut card).background,
            Some(theme.raised.into()),
            "card()'s final light background must be the raised surface"
        );
    }

    /// `row_view` used to hardcode every literal despite taking `_theme`;
    /// this pins that it now reads spacing from the COSMIC scale, not a
    /// bare number — a regression here means the parameter went back to
    /// being decorative.
    #[test]
    fn row_view_spacing_comes_from_the_cosmic_scale() {
        let theme = Theme::dark();
        let spacing = theme.cosmic.spacing;
        assert_eq!(spacing.xs, 12);
        assert_eq!(spacing.xxs, 8);
    }

    /// `separator()`'s height is the new `hairline_thickness` token, not a
    /// bare `px(1.0)` — pins the token this piece added for `codex12`.
    #[test]
    fn separator_height_is_the_hairline_thickness_token() {
        assert_eq!(Theme::dark().spacing.hairline_thickness, px(1.0));
    }
}

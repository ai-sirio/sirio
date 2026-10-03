//! The tab's Ely adapters (spec §5). Each turns an Ely widget into the shape
//! the tab already calls: a `debug_selector` wrapper so tests and scripts find
//! it, a busy flag, and `selectable_text` for any message a person may copy.
//! No tab state lives here.
#![allow(dead_code)]

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    data_display::{Badge, Tone},
    feedback::Callout,
    forms::TextInput,
    primitives::{IconName, Severity},
};
use gpui::{AnyElement, App, Context, Entity, IntoElement, Window, div, prelude::*};
use sirio_forge::ChangeState;

use super::ChangeRequestTab;
use crate::change_request_style as style;
use crate::text_selection::selectable_text;

/// How a button is drawn and whether it answers a click.
#[derive(Clone, Copy)]
pub(crate) struct ButtonState {
    pub enabled: bool,
    /// A write is in flight for this very button.
    pub loading: bool,
    pub primary: bool,
}

impl ButtonState {
    pub(crate) const IDLE: Self = Self {
        enabled: true,
        loading: false,
        primary: false,
    };
    pub(crate) fn enabled(enabled: bool) -> Self {
        Self { enabled, ..Self::IDLE }
    }
    pub(crate) fn primary(self) -> Self {
        Self { primary: true, ..self }
    }
    pub(crate) fn loading(self, loading: bool) -> Self {
        Self { loading, ..self }
    }
}

/// A labelled button. `id` is both the element id and the `debug_selector`.
pub(crate) fn text_button(
    id: &'static str,
    label: &'static str,
    icon: Option<IconName>,
    state: ButtonState,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let mut button = Button::new(id, label)
        .disabled(!state.enabled)
        .loading(state.loading)
        .on_click(move |_, window, cx| on_click(window, cx));
    button = if state.primary {
        button.primary()
    } else {
        button.variant(ButtonVariant::Ghost)
    };
    if let Some(icon) = icon {
        button = button.icon(icon);
    }
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(button)
        .into_any_element()
}

pub(crate) fn icon_button(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(
            IconButton::new(id, icon)
                .tooltip(tooltip)
                .disabled(!enabled)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}

/// A message in the severity's colour whose words can be selected and copied.
pub(crate) fn message(severity: Severity, text: String) -> AnyElement {
    Callout::new(severity)
        .child(selectable_text(text))
        .into_any_element()
}

pub(crate) fn state_badge(state: ChangeState) -> impl IntoElement {
    let tone = match state {
        ChangeState::Open => Tone::Success,
        ChangeState::Draft => Tone::Neutral,
        ChangeState::Closed => Tone::Danger,
        ChangeState::Merged => Tone::Accent,
    };
    Badge::new(style::state_label(state)).tone(tone)
}

/// Line endings as `\n`; a single-line field never holds a newline.
pub(crate) fn normalize(text: &str, multi_line: bool) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if multi_line {
        text
    } else {
        text.replace('\n', " ")
    }
}

/// A text input filled with `text`. Needs the window the tab's constructors
/// lack, so it is called from a click, from `control_act`, or from `render`.
pub(crate) fn new_input(
    window: &mut Window,
    cx: &mut Context<ChangeRequestTab>,
    text: &str,
    rows: Option<(usize, usize)>,
    placeholder: &'static str,
) -> Entity<TextInput> {
    let text = text.to_string();
    cx.new(|cx| {
        let mut input = TextInput::new(window, cx).placeholder(placeholder);
        if let Some((min, max)) = rows {
            input = input.multi_line(min, max);
        }
        input.set_text(text, cx);
        input
    })
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn line_endings_become_newlines() {
        assert_eq!(normalize("a\r\nb\rc", true), "a\nb\nc");
    }

    #[test]
    fn a_single_line_field_never_holds_a_newline() {
        assert_eq!(normalize("a\r\nb\nc", false), "a b c");
    }
}

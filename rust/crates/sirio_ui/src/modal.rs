//! A generic modal sheet: a dimmed backdrop with a centered card carrying a
//! title, a body message, an optional single-line text field, and a row of
//! buttons.
//!
//! This is the one primitive behind two uses (`docs/linux-rewrite/tasks/
//! P100-four-defects-the-swift-source-already-answers.md`): the terminal
//! close confirmation (no text field) and the terminal "Set Title" prompt (a
//! text field prefilled with the current title). The Swift original is an
//! `NSAlert` with an optional `NSTextField` accessory view
//! (`App/TerminalContextMenuProvider.swift`'s `showCloseConfirmAlert` and
//! `showSetTitleAlert`) — a title, a message, an optional field, and OK/
//! Cancel-shaped buttons is exactly that shape, drawn in GPUI instead of
//! AppKit.
//!
//! The text field follows the same draft-string-plus-`FocusHandle` idiom
//! already used by `TabRename` (`crates/sirio/src/main.rs`) and
//! `CreateForm`/`CloneForm` (`project_forms.rs`): this module never owns the
//! draft itself, it only draws `ModalTextField::value` and forwards key
//! events to the caller's `on_key_down`, so callers keep their own editing
//! rules (trim-and-no-op-on-empty, in both current uses) in one place.

use bezel::theme::Theme as BezelTheme;
use gpui::{
    AnyElement, App, ClickEvent, FocusHandle, FontWeight, KeyDownEvent, MouseButton, Window, div,
    prelude::*, px,
};
use sirio_theme::Theme;

/// Visual weight of a [`ModalButton`] — which theme fill, and therefore
/// which semantic role, it draws with. Three states rather than a bare
/// `bool` because the two known uses need genuinely different colours for
/// their default action: Set Title's "OK" is affirmative (the inverted
/// chip, `theme.solid`),
/// the close confirm's "Close Anyway" is destructive (`theme.danger`) —
/// collapsing both into one "primary" flag would have painted one of them
/// the wrong colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalButtonTone {
    /// The sheet's non-default action (Cancel).
    Plain,
    /// The sheet's affirmative default action (OK).
    Accent,
    /// The sheet's destructive default action (Close Anyway).
    Destructive,
}

/// One button drawn in a [`ModalSpec`]'s button row, left to right.
pub struct ModalButton {
    /// Selector suffix appended to [`ModalSpec::id`], e.g. `"cancel"`
    /// producing `"pane-close-confirm-cancel"`.
    pub id: &'static str,
    pub label: String,
    pub tone: ModalButtonTone,
    pub on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl ModalButton {
    pub fn new(
        id: &'static str,
        label: impl Into<String>,
        tone: ModalButtonTone,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            tone,
            on_click: Box::new(on_click),
        }
    }
}

/// The optional single-line text field a [`ModalSpec`] carries — the
/// `NSTextField` accessory view on the Set-Title alert.
pub struct ModalTextField {
    pub focus: FocusHandle,
    /// Drawn as-is; the caller owns the draft (same convention as
    /// `TabRename`/`CreateForm`) and applies `on_key_down`'s edits to it.
    pub value: String,
    /// Whether the field's end-of-text insertion caret bar is currently
    /// lit. The caller folds focus + blink phase into this — `render_modal`
    /// is stateless, so it can't know either on its own.
    pub caret_visible: bool,
    pub on_key_down: Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>,
}

impl ModalTextField {
    pub fn new(
        focus: FocusHandle,
        value: impl Into<String>,
        caret_visible: bool,
        on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            focus,
            value: value.into(),
            caret_visible,
            on_key_down: Box::new(on_key_down),
        }
    }
}

/// Backdrop-level focus + key capture for a [`ModalSpec`] variant with no
/// [`ModalTextField`] of its own to hold a `FocusHandle` — the close
/// confirm banner (F-TERM-08's "Close terminal?"), which has two buttons
/// and no text entry. Without this the backdrop never claims real GPUI
/// focus, so a modal opened while a terminal is focused leaves the
/// terminal as the actual keyboard-dispatch target and everything typed
/// while the modal is up reaches its PTY instead of the dialog (see
/// `docs/linux-rewrite/tasks/P103-two-close-confirmation-contracts.md`'s
/// refutation write-up). The Set Title variant doesn't need this: its own
/// `ModalTextField::focus` already tracks focus on the field.
pub struct ModalFocus {
    pub handle: FocusHandle,
    pub on_key_down: Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>,
}

impl ModalFocus {
    pub fn new(
        handle: FocusHandle,
        on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            handle,
            on_key_down: Box::new(on_key_down),
        }
    }
}

/// A modal sheet: title, body, an optional text field, and a button row.
pub struct ModalSpec {
    /// Selector root. The backdrop itself is `"{id}"`, the field (if any) is
    /// `"{id}-field"`, and each button is `"{id}-{button.id}"`.
    pub id: &'static str,
    pub title: String,
    pub body: String,
    pub text_field: Option<ModalTextField>,
    pub buttons: Vec<ModalButton>,
    /// Backdrop-level focus for variants with no `text_field` — see
    /// [`ModalFocus`]. `None` when `text_field` is `Some`, since the field
    /// already tracks its own focus and forwards keys.
    pub focus: Option<ModalFocus>,
}

/// Renders a [`ModalSpec`] as a full-window dimmed backdrop with a centered
/// card. Callers draw this last in their own render tree so it sits above
/// everything else — the same placement `render_pane_close_confirm` already
/// used before it was rebuilt on this primitive.
pub fn render_modal(spec: ModalSpec, theme: Theme) -> AnyElement {
    let backdrop_id = spec.id.to_string();

    let mut sheet = div()
        .flex()
        .flex_col()
        .gap(px(BezelTheme::SPACE_MD))
        .child(
            div()
                .text_size(theme.typography.headline)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(spec.title),
        )
        .child(
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.text_muted)
                .child(spec.body),
        );

    if let Some(field) = spec.text_field {
        let field_id = format!("{}-field", spec.id);
        let focus_for_click = field.focus.clone();
        let on_key_down = field.on_key_down;
        sheet = sheet.child(
            div()
                .id(field_id.clone())
                .debug_selector(move || field_id.clone())
                .track_focus(&field.focus)
                .w_full()
                .h(px(28.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .rounded(theme.radii.control)
                .bg(theme.input_bg)
                .border_1()
                .border_color(theme.text)
                .text_size(theme.typography.footnote)
                .text_color(theme.text)
                .cursor(gpui::CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    focus_for_click.focus(window, cx);
                })
                .on_key_down(move |event, window, cx| on_key_down(event, window, cx))
                // #212: clip inside the field rather than drawing past
                // its border. Must shrink without growing: `flex_1` would
                // push the end-of-text caret below to the far right.
                .overflow_hidden()
                .child(
                    crate::caret::field_value(field.value)
                        .id("modal-field-text")
                        .debug_selector(|| "modal-field-text".to_owned()),
                )
                // End-of-text insertion caret; laid out even when invisible
                // so the bar never shifts the value while blinking.
                .child(crate::caret::bar(px(14.0), theme.text, field.caret_visible)),
        );
    }

    let mut button_row = div().flex().justify_end().gap(px(8.0));
    for button in spec.buttons {
        let button_id = format!("{}-{}", spec.id, button.id);
        let on_click = button.on_click;
        // `gpui::white()` returns `Hsla`; every other branch here is the
        // theme's own `Rgba`, so this converts rather than let the tuple's
        // element type quietly pick whichever arm the compiler saw first.
        let white: gpui::Rgba = gpui::white().into();
        let (bg, text_color) = match button.tone {
            ModalButtonTone::Plain => (theme.surface_raised, theme.text),
            ModalButtonTone::Accent => (theme.solid, theme.on_solid),
            ModalButtonTone::Destructive => (theme.danger, white),
        };
        button_row = button_row.child(
            div()
                .id(button_id.clone())
                .debug_selector(move || button_id.clone())
                .cursor(gpui::CursorStyle::PointingHand)
                .px(px(12.0))
                .py(px(6.0))
                .rounded(theme.radii.control)
                .bg(bg)
                .text_color(text_color)
                .on_click(move |event, window, cx| on_click(event, window, cx))
                .child(button.label),
        );
    }
    sheet = sheet.child(button_row);

    div()
        .id(backdrop_id.clone())
        .debug_selector(move || backdrop_id.clone())
        .when_some(spec.focus, |this, focus| {
            let on_key_down = focus.on_key_down;
            this.track_focus(&focus.handle)
                .on_key_down(move |event, window, cx| on_key_down(event, window, cx))
        })
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.6))
        // The same shape as `controls::card`, on the floating surface rather
        // than the raised one: a sheet asking for input stays opaque when the
        // shell is translucent.
        .child(
            div()
                .w(px(360.0))
                .rounded(px(BezelTheme::BASE_RADIUS))
                .overflow_hidden()
                .bg(theme.floating_surface)
                .p(px(16.0))
                .child(sheet),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, VisualTestContext};
    use sirio_theme::ThemeMode;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A minimal window root drawing exactly one [`ModalSpec`], built by a
    /// closure so each test controls the spec's shape (text field present or
    /// absent, button count).
    struct ModalHarness<F: Fn() -> ModalSpec + 'static> {
        build: F,
    }

    impl<F: Fn() -> ModalSpec + 'static> Render for ModalHarness<F> {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *Theme::get(cx);
            div().size_full().child(render_modal((self.build)(), theme))
        }
    }

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(|cx| Theme::install(ThemeMode::Dark, cx));
    }

    /// The no-text-field variant (F-TERM-08's close confirm): the backdrop
    /// and both buttons draw, and no field selector exists at all.
    #[gpui::test]
    async fn confirm_variant_draws_no_text_field(cx: &mut TestAppContext) {
        install_theme(cx);
        let window = cx.add_window(|_window, _cx| ModalHarness {
            build: || ModalSpec {
                id: "close-confirm-test",
                title: "Close Terminal".into(),
                body: "The running process will be terminated.".into(),
                text_field: None,
                buttons: vec![
                    ModalButton::new("cancel", "Cancel", ModalButtonTone::Plain, |_, _, _| {}),
                    ModalButton::new("close", "Close", ModalButtonTone::Destructive, |_, _, _| {}),
                ],
                focus: None,
            },
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.debug_bounds("close-confirm-test")
            .expect("the backdrop draws");
        cx.debug_bounds("close-confirm-test-cancel")
            .expect("the cancel button draws");
        cx.debug_bounds("close-confirm-test-close")
            .expect("the close button draws");
        assert!(
            cx.debug_bounds("close-confirm-test-field").is_none(),
            "a spec with no text_field must draw no field at all"
        );
    }

    /// The text-field variant (F-TERM-05's Set Title prompt): the field
    /// draws alongside the buttons, distinguishing the two known uses of one
    /// primitive.
    #[gpui::test]
    async fn set_title_variant_draws_a_text_field(cx: &mut TestAppContext) {
        install_theme(cx);
        let window = cx.add_window(|_window, cx| ModalHarness {
            build: {
                let focus = cx.focus_handle();
                move || ModalSpec {
                    id: "set-title-test",
                    title: "Set Title".into(),
                    body: "Enter the new title for \"Terminal\":".into(),
                    text_field: Some(ModalTextField::new(
                        focus.clone(),
                        "Terminal",
                        false,
                        |_, _, _| {},
                    )),
                    buttons: vec![
                        ModalButton::new("ok", "OK", ModalButtonTone::Accent, |_, _, _| {}),
                        ModalButton::new("cancel", "Cancel", ModalButtonTone::Plain, |_, _, _| {}),
                    ],
                    focus: None,
                }
            },
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.debug_bounds("set-title-test-field")
            .expect("a spec with a text_field draws the field");
        cx.debug_bounds("set-title-test-ok")
            .expect("the ok button draws");
        cx.debug_bounds("set-title-test-cancel")
            .expect("the cancel button draws");
    }

    /// Clicking a button invokes exactly that button's `on_click`, not any
    /// other button's — proven by two independent flags rather than one
    /// shared boolean, so a click misrouted to the wrong button would fail
    /// this rather than pass it by coincidence.
    #[gpui::test]
    async fn clicking_a_button_invokes_only_its_own_callback(cx: &mut TestAppContext) {
        install_theme(cx);
        let ok_clicked = Rc::new(Cell::new(false));
        let cancel_clicked = Rc::new(Cell::new(false));
        let ok_flag = ok_clicked.clone();
        let cancel_flag = cancel_clicked.clone();
        let window = cx.add_window(|_window, _cx| ModalHarness {
            build: move || ModalSpec {
                id: "click-test",
                title: "Title".into(),
                body: "Body".into(),
                text_field: None,
                buttons: vec![
                    ModalButton::new("ok", "OK", ModalButtonTone::Accent, {
                        let ok_flag = ok_flag.clone();
                        move |_, _, _| ok_flag.set(true)
                    }),
                    ModalButton::new("cancel", "Cancel", ModalButtonTone::Plain, {
                        let cancel_flag = cancel_flag.clone();
                        move |_, _, _| cancel_flag.set(true)
                    }),
                ],
                focus: None,
            },
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let ok_bounds = cx
            .debug_bounds("click-test-ok")
            .expect("the ok button draws");
        cx.simulate_click(ok_bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();

        assert!(ok_clicked.get(), "clicking OK must invoke OK's callback");
        assert!(
            !cancel_clicked.get(),
            "clicking OK must not invoke Cancel's callback"
        );
    }
}

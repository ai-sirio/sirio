//! Sirio's Ely adapters, shared by the change-request surfaces (spec §5, §7).
//! Each turns an Ely widget into the shape
//! the tab already calls: a `debug_selector` wrapper so tests and scripts find
//! it, a busy flag, and `selectable_text` for any message a person may copy.
//! No tab state lives here.

use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    data_display::{Badge, Tone},
    forms::TextInput,
    primitives::{Icon, IconName, Severity},
    theme::IconSize,
};
use gpui::{AnyElement, App, Context, ElementId, Entity, IntoElement, Window, div, prelude::*, px};
use sirio_forge::ChangeState;
use sirio_theme::Theme;

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
        Self {
            enabled,
            ..Self::IDLE
        }
    }
    pub(crate) fn primary(self) -> Self {
        Self {
            primary: true,
            ..self
        }
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

/// A message in the severity's colour whose words can be selected and copied:
/// a compact line, not Ely's titled `Callout`, whose fixed "Danger" heading
/// would sit above every error.
///
/// `id` names the words. Ids only need to differ among messages drawn together
/// in one window: bezel's accessibility tree rejects a duplicate node id (a
/// panic in debug builds), and alternative arms of a card may share one.
pub(crate) fn message(
    severity: Severity,
    text: String,
    id: impl Into<ElementId>,
    theme: &Theme,
) -> AnyElement {
    let tone = severity.color(&theme.ely);
    div()
        .flex()
        .items_start()
        .gap(px(6.0))
        .text_size(theme.typography.footnote)
        .text_color(tone)
        .child(
            div()
                .flex_none()
                .pt(px(1.0))
                .child(Icon::new(severity.icon()).size(IconSize::Sm).color(tone)),
        )
        .child(div().min_w_0().child(selectable_text(text).id(id)))
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
pub(crate) fn new_input<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    text: &str,
    rows: Option<(usize, usize)>,
    placeholder: &'static str,
) -> Entity<TextInput> {
    // The old field folded line endings on the way in; Ely's keeps them, and a
    // description from GitHub arrives with CRLF, which must not read as an edit.
    let text = normalize(text, rows.is_some());
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
    use ely_gpui_component::forms::{InputEvent, TextInput};
    use gpui::{
        Context, Entity, Focusable, KeyBinding, Modifiers, Render, ScrollDelta, ScrollWheelEvent,
        TestAppContext, TouchPhase, VisualTestContext, actions, div, point, prelude::*, px,
    };
    use sirio_theme::Theme;

    #[test]
    fn line_endings_become_newlines() {
        assert_eq!(normalize("a\r\nb\rc", true), "a\nb\nc");
    }

    #[test]
    fn a_single_line_field_never_holds_a_newline() {
        assert_eq!(normalize("a\r\nb\nc", false), "a b c");
    }

    actions!(ely_ui_probe, [HostEnter, HostUp]);

    /// A host that owns a field and keeps two of its keys. `HostEnter` stops there;
    /// `HostUp` is handed on, so the field's own `Up` runs after it.
    struct Hosted {
        input: Entity<TextInput>,
        events: Vec<InputEvent>,
        entered: usize,
        upped: usize,
    }

    impl Hosted {
        fn new(input: Entity<TextInput>, cx: &mut Context<Self>) -> Self {
            cx.subscribe(&input, |host, _, event: &InputEvent, _| {
                host.events.push(*event);
            })
            .detach();
            Self {
                input,
                events: Vec::new(),
                entered: 0,
                upped: 0,
            }
        }
    }

    impl Render for Hosted {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .on_action(cx.listener(|host, _: &HostEnter, _, _| host.entered += 1))
                .on_action(cx.listener(|host, _: &HostUp, _, cx| {
                    host.upped += 1;
                    cx.propagate();
                }))
                .child(div().w(px(300.0)).child(self.input.clone()))
        }
    }

    /// What the app has when a field is hosted: Sirio's theme and Ely's chat setup.
    fn boot(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(ely_gpui_component::init_chat);
    }

    fn host(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut gpui::Window, &mut Context<Hosted>) -> Entity<TextInput> + 'static,
    ) -> (Entity<Hosted>, &mut VisualTestContext) {
        cx.add_window_view(move |window, cx| {
            let input = build(window, cx);
            Hosted::new(input, cx)
        })
    }

    /// How many times the host's Enter ran, and how many `Submit`s the field emitted.
    fn counts(host: &Entity<Hosted>, cx: &mut VisualTestContext) -> (usize, usize) {
        host.read_with(cx, |host, _| {
            let submits = host.events.iter().filter(|e| **e == InputEvent::Submit);
            (host.entered, submits.count())
        })
    }

    fn text_of(host: &Entity<Hosted>, cx: &mut VisualTestContext) -> String {
        host.read_with(cx, |host, cx| host.input.read(cx).text().to_string())
    }

    #[gpui::test]
    async fn a_host_binding_registered_after_ely_wins_and_can_hand_the_key_on(
        cx: &mut TestAppContext,
    ) {
        boot(cx);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("enter", HostEnter, Some("ProbeHost")),
                KeyBinding::new("up", HostUp, Some("ProbeHost")),
            ]);
        });
        let (host, cx) = host(cx, |window, cx| {
            let input = cx.new(|cx| TextInput::new(window, cx).host_context("ProbeHost"));
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        cx.simulate_input("abc");

        cx.simulate_keystrokes("enter");
        let (entered, submitted) = counts(&host, cx);
        assert_eq!(entered, 1, "the host's Enter ran");
        assert_eq!(submitted, 0, "Ely's own Enter ran as well");
        assert_eq!(text_of(&host, cx), "abc");

        cx.simulate_keystrokes("up");
        let upped = host.read_with(cx, |host, _| host.upped);
        let cursor = host.read_with(cx, |host, cx| host.input.read(cx).cursor());
        assert_eq!(upped, 1, "the host's Up ran");
        assert_eq!(cursor, 0, "the hand-on never reached Ely's Up");

        // A second setup, or a second window, runs Sirio's init after the host bound its keys.
        cx.update(|_, cx| crate::ely::init(cx));
        cx.simulate_keystrokes("enter");
        let (entered, submitted) = counts(&host, cx);
        assert_eq!(entered, 2, "a later init took the host's Enter back");
        assert_eq!(submitted, 0, "a later init put Ely's Enter above the host's");
    }

    #[gpui::test]
    async fn a_detached_field_focused_before_its_first_frame_reports_focus_and_types(
        cx: &mut TestAppContext,
    ) {
        boot(cx);
        let (host, cx) = host(cx, |window, cx| {
            let input = cx.new(|cx| TextInput::detached(cx));
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        cx.run_until_parked();
        let focuses = host.read_with(cx, |host, _| {
            host.events.iter().filter(|e| **e == InputEvent::Focus).count()
        });
        assert_eq!(focuses, 1, "one Focus for a handle focused before the first frame");

        cx.simulate_input("x");
        assert_eq!(text_of(&host, cx), "x");
    }

    /// The edit between typing and `reset_text` is a backspace on purpose: typing in one burst
    /// is a single undo step whose snapshot is the empty text, so after a plain `set_text("")`
    /// an undo would land on `""` as well and the test could not tell the two apart.
    #[gpui::test]
    async fn reset_text_starts_a_fresh_history(cx: &mut TestAppContext) {
        boot(cx);
        let (host, cx) = host(cx, |window, cx| {
            let input = cx.new(|cx| TextInput::new(window, cx));
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        cx.simulate_input("abc");
        cx.simulate_keystrokes("backspace");
        assert_eq!(text_of(&host, cx), "ab");

        host.update(cx, |host, cx| {
            host.input.update(cx, |input, cx| input.reset_text("", cx));
        });
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(
            text_of(&host, cx),
            "",
            "undo brought back what the reset replaced"
        );
    }

    #[gpui::test]
    async fn a_long_multi_line_draft_scrolls_under_the_wheel(cx: &mut TestAppContext) {
        boot(cx);
        let draft = (0..30).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        let (host, cx) = host(cx, move |window, cx| {
            let input = cx.new(|cx| {
                let mut input = TextInput::new(window, cx).multi_line(3, 12);
                input.set_text(draft, cx);
                input
            });
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        // `set_text` leaves the caret at the end, with the view already scrolled to it. Put the
        // caret on the first row so a downward wheel has room, and a view that follows the caret
        // would pull it back.
        host.update(cx, |host, cx| {
            host.input.update(cx, |input, cx| input.select(0..0, cx));
        });
        cx.run_until_parked();
        let top = |host: &Entity<Hosted>, cx: &mut VisualTestContext| {
            host.read_with(cx, |host, cx| host.input.read(cx).bounds_for(0))
                .expect("the field is laid out")
        };
        let before = top(&host, cx);

        let over = before.origin + point(px(10.0), px(10.0));
        cx.simulate_mouse_move(over, None, Modifiers::none());
        cx.simulate_event(ScrollWheelEvent {
            position: over,
            delta: ScrollDelta::Pixels(point(px(0.0), px(-40.0))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        cx.run_until_parked();

        let after = top(&host, cx);
        assert!(
            after.origin.y < before.origin.y,
            "the wheel moved the first row from {:?} to {:?}",
            before.origin.y,
            after.origin.y
        );
    }
}

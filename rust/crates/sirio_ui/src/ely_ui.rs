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
    use ely_gpui_component::forms::{
        DeleteToLineEnd, DeleteToLineStart, DeleteWordRight, InputEvent, MULTI_LINE_CONTEXT,
        TextInput,
    };
    use gpui::{
        Context, Entity, Focusable, KeyBinding, Modifiers, Pixels, Point, Render, ScrollDelta,
        ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, actions, div, point,
        prelude::*, px,
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

    actions!(ely_ui_probe, [HostEnter, HostUp, HostNext]);

    /// A host that owns a field and keeps two of its keys. `HostEnter` stops there;
    /// `HostUp` is handed on, so the field's own `Up` runs after it. The field sits in a page
    /// that counts the wheel events reaching it.
    struct Hosted {
        input: Entity<TextInput>,
        events: Vec<InputEvent>,
        entered: usize,
        upped: usize,
        nexted: usize,
        page_wheels: usize,
        /// Whether the field is in the tree yet.
        shown: bool,
    }

    impl Hosted {
        fn new(input: Entity<TextInput>, shown: bool, cx: &mut Context<Self>) -> Self {
            cx.subscribe(&input, |host, _, event: &InputEvent, _| {
                host.events.push(*event);
            })
            .detach();
            Self {
                input,
                events: Vec::new(),
                entered: 0,
                upped: 0,
                nexted: 0,
                page_wheels: 0,
                shown,
            }
        }

        fn count(&self, event: InputEvent) -> usize {
            self.events.iter().filter(|seen| **seen == event).count()
        }
    }

    impl Render for Hosted {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .on_action(cx.listener(|host, _: &HostEnter, _, _| host.entered += 1))
                .on_action(cx.listener(|host, _: &HostNext, _, _| host.nexted += 1))
                .on_action(cx.listener(|host, _: &HostUp, _, cx| {
                    host.upped += 1;
                    cx.propagate();
                }))
                .on_scroll_wheel(cx.listener(|host, _, _, _| host.page_wheels += 1))
                .when(self.shown, |page| {
                    page.child(div().w(px(300.0)).child(self.input.clone()))
                })
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
            Hosted::new(input, true, cx)
        })
    }

    /// How many times the host's Enter ran, and how many `Submit`s the field emitted.
    fn counts(host: &Entity<Hosted>, cx: &mut VisualTestContext) -> (usize, usize) {
        host.read_with(cx, |host, _| (host.entered, host.count(InputEvent::Submit)))
    }

    fn count(host: &Entity<Hosted>, event: InputEvent, cx: &mut VisualTestContext) -> usize {
        host.read_with(cx, |host, _| host.count(event))
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
        assert_eq!(entered, 1, "the host's Enter must run once");
        assert_eq!(submitted, 0, "Ely's own Enter must not run: the host's stops the key");
        assert_eq!(text_of(&host, cx), "abc", "the text must stay as typed");

        cx.simulate_keystrokes("up");
        let upped = host.read_with(cx, |host, _| host.upped);
        let cursor = host.read_with(cx, |host, cx| host.input.read(cx).cursor());
        assert_eq!(upped, 1, "the host's Up must run once");
        assert_eq!(cursor, 0, "Ely's own Up must run after the host's hands the key on");

        // A second setup, or a second window, runs Sirio's init after the host bound its keys.
        cx.update(|_, cx| crate::ely::init(cx));
        cx.simulate_keystrokes("enter");
        let (entered, submitted) = counts(&host, cx);
        assert_eq!(entered, 2, "after a later init the host's Enter must still run");
        assert_eq!(submitted, 0, "after a later init Ely's Enter must still not run");
    }

    /// Bezel bound Ctrl+N and Ctrl+P, macOS only, in its multi-line context alone: a single-line
    /// field answered neither, and was nested in lists (the combobox, the model search) that bind
    /// Down and Up themselves. Down in a one-line field goes to the end of the text, Up to its
    /// start, so a caret that stays put shows the chord was not taken as one.
    #[gpui::test]
    async fn ctrl_n_and_ctrl_p_do_not_move_a_single_line_field_the_way_down_and_up_do(
        cx: &mut TestAppContext,
    ) {
        boot(cx);
        let (host, cx) = host(cx, |window, cx| {
            let input = cx.new(|cx| TextInput::new(window, cx));
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        let cursor = |host: &Entity<Hosted>, cx: &mut VisualTestContext| {
            host.read_with(cx, |host, cx| host.input.read(cx).cursor())
        };
        cx.simulate_input("abc");
        cx.simulate_keystrokes("home");
        cx.simulate_keystrokes("ctrl-n");
        assert_eq!(cursor(&host, cx), 0, "Ctrl+N must not do what Down does in one line");
        cx.simulate_keystrokes("end");
        cx.simulate_keystrokes("ctrl-p");
        assert_eq!(cursor(&host, cx), 3, "Ctrl+P must not do what Up does in one line");
    }

    /// A focused field holding `text` with `select` selected, in one line or in 3 to 12 rows.
    fn field<'a>(
        cx: &'a mut TestAppContext,
        text: &'static str,
        select: std::ops::Range<usize>,
        rows: bool,
    ) -> (Entity<Hosted>, &'a mut VisualTestContext) {
        boot(cx);
        host(cx, move |window, cx| {
            let input = cx.new(|cx| {
                let mut input = TextInput::new(window, cx);
                if rows {
                    input = input.multi_line(3, 12);
                }
                input.set_text(text, cx);
                input.select(select, cx);
                input
            });
            window.focus(&input.focus_handle(cx), cx);
            input
        })
    }

    fn text_and_selection(
        host: &Entity<Hosted>,
        cx: &mut VisualTestContext,
    ) -> (String, std::ops::Range<usize>) {
        host.read_with(cx, |host, cx| {
            let input = host.input.read(cx);
            (input.text().to_string(), input.selection())
        })
    }

    #[derive(Clone, Copy, Debug)]
    enum Delete {
        ToLineStart,
        ToLineEnd,
        WordRight,
    }

    fn delete(cx: &mut VisualTestContext, which: Delete) {
        match which {
            Delete::ToLineStart => cx.dispatch_action(DeleteToLineStart),
            Delete::ToLineEnd => cx.dispatch_action(DeleteToLineEnd),
            Delete::WordRight => cx.dispatch_action(DeleteWordRight),
        }
        cx.run_until_parked();
    }

    /// What bezel's field did, which the field must keep doing (the macOS and Windows/Linux
    /// chords bound to these are tested where they are pressed). A line is a logical line, ended by
    /// a newline and not by a soft wrap. Whatever is selected is deleted, and nothing more. The
    /// caret ends where the deleted text began.
    #[gpui::test]
    async fn the_delete_to_line_and_word_actions_delete_what_bezels_did(cx: &mut TestAppContext) {
        let wrapped = "word ".repeat(40);
        let wrapped: &'static str = Box::leak(wrapped.into_boxed_str());
        let cases: [(&str, Delete, &str, std::ops::Range<usize>, bool, &str, usize); 10] = [
            ("to the line's end, not past a newline", Delete::ToLineEnd,
                "one two\nthree four", 3..3, true, "one\nthree four", 3),
            ("to the line's start, not past a newline", Delete::ToLineStart,
                "one two\nthree four", 13..13, true, "one two\n four", 8),
            ("a word to the right", Delete::WordRight, "one two", 0..0, true, " two", 0),
            ("the line's end of a wrapped line, all of it", Delete::ToLineEnd,
                wrapped, 5..5, true, "word ", 5),
            ("the line's start of a wrapped line, all of it", Delete::ToLineStart,
                wrapped, 200..200, true, "", 0),
            ("a selection, not the line's end", Delete::ToLineEnd,
                "alpha beta gamma", 6..10, true, "alpha  gamma", 6),
            ("a selection, not the line's start", Delete::ToLineStart,
                "alpha beta gamma", 6..10, true, "alpha  gamma", 6),
            ("a selection, not a word", Delete::WordRight,
                "alpha beta gamma", 6..10, true, "alpha  gamma", 6),
            ("the same in one line", Delete::ToLineEnd, "abc def", 3..3, false, "abc", 3),
            ("nothing past the end", Delete::WordRight, "abc", 3..3, false, "abc", 3),
        ];
        for (what, action, text, select, rows, expect, caret) in cases {
            let (host, cx) = field(cx, text, select, rows);
            delete(cx, action);
            let (got, selection) = text_and_selection(&host, cx);
            assert_eq!(got, expect, "{action:?} deletes {what}");
            assert_eq!(selection, caret..caret, "{action:?} leaves the caret at {caret}: {what}");
        }
    }

    /// One undo step for one action, as for every other edit Ely's field makes.
    #[gpui::test]
    async fn a_delete_to_line_end_is_one_undo_step(cx: &mut TestAppContext) {
        let (host, cx) = field(cx, "one two\nthree four", 3..3, true);
        delete(cx, Delete::ToLineEnd);
        assert_eq!(text_and_selection(&host, cx).0, "one\nthree four");

        cx.simulate_keystrokes("secondary-z");
        assert_eq!(
            text_and_selection(&host, cx).0,
            "one two\nthree four",
            "one undo must give back all the deleted text"
        );
    }

    /// Ctrl+N and Ctrl+P are bound, on macOS, under the multi-line context and not under every
    /// field's, which would hand a chord to the one-line fields nested in lists that bind Down and
    /// Up. Bound here on every platform the way macOS's init binds them: a multi-line field must
    /// hear the chord and a one-line field must not.
    #[gpui::test]
    async fn only_a_multi_line_field_names_the_multi_line_context(cx: &mut TestAppContext) {
        let (one, cx_one) = field(cx, "", 0..0, false);
        cx_one.update(|_, cx| {
            cx.bind_keys([KeyBinding::new("ctrl-n", HostNext, Some(MULTI_LINE_CONTEXT))]);
        });
        cx_one.simulate_keystrokes("ctrl-n");
        assert_eq!(one.read_with(cx_one, |host, _| host.nexted), 0, "a one-line field is not multi-line");

        let (many, cx_many) = field(cx, "", 0..0, true);
        cx_many.update(|_, cx| {
            cx.bind_keys([KeyBinding::new("ctrl-n", HostNext, Some(MULTI_LINE_CONTEXT))]);
        });
        cx_many.simulate_keystrokes("ctrl-n");
        assert_eq!(many.read_with(cx_many, |host, _| host.nexted), 1, "a multi-line field is");
    }

    /// The multi-line identifier is one more on the node and changes nothing else: a host's own
    /// context on a multi-line field still outranks Ely's keys, as it does on a one-line one.
    #[gpui::test]
    async fn a_host_binding_still_wins_in_a_multi_line_field(cx: &mut TestAppContext) {
        boot(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("enter", HostEnter, Some("ProbeHost"))]);
        });
        let (host, cx) = host(cx, |window, cx| {
            let input = cx.new(|cx| {
                TextInput::new(window, cx).multi_line(3, 12).host_context("ProbeHost")
            });
            window.focus(&input.focus_handle(cx), cx);
            input
        });
        cx.simulate_input("abc");
        cx.simulate_keystrokes("enter");

        assert_eq!(counts(&host, cx).0, 1, "the host's Enter must run");
        assert_eq!(text_of(&host, cx), "abc", "Ely's own Enter must not add a newline");
    }

    /// A window whose detached field is not in the tree yet, its handle already focused. The
    /// test windows start inactive, as a window does until the platform activates it.
    fn unshown(cx: &mut TestAppContext) -> (Entity<Hosted>, &mut VisualTestContext) {
        boot(cx);
        cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| TextInput::detached(cx));
            window.focus(&input.focus_handle(cx), cx);
            Hosted::new(input, false, cx)
        })
    }

    /// Puts the field in the tree: its first frame.
    fn show(host: &Entity<Hosted>, cx: &mut VisualTestContext) {
        host.update(cx, |host, cx| {
            host.shown = true;
            cx.notify();
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn a_detached_field_focused_before_its_first_frame_reports_focus_and_types(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = unshown(cx);
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();

        show(&host, cx);
        assert_eq!(
            count(&host, InputEvent::Focus, cx),
            1,
            "a handle focused before the field's first frame must report exactly one Focus"
        );

        cx.simulate_input("x");
        assert_eq!(text_of(&host, cx), "x", "a detached field must take typed text");

        // The subscriptions made at the first frame work: focus leaving is reported once.
        cx.update(|window, cx| window.blur(cx));
        cx.run_until_parked();
        assert_eq!(
            count(&host, InputEvent::Blur, cx),
            1,
            "focus leaving a detached field must report exactly one Blur"
        );
    }

    #[gpui::test]
    async fn a_detached_field_first_drawn_in_an_inactive_window_reports_focus_once_it_activates(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = unshown(cx);
        cx.deactivate_window();

        show(&host, cx);
        assert_eq!(
            count(&host, InputEvent::Focus, cx),
            0,
            "a field first drawn in an inactive window must not report focus yet"
        );

        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert_eq!(
            count(&host, InputEvent::Focus, cx),
            1,
            "the window activating must report the field's focus exactly once"
        );
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
            "undo after reset_text must leave the empty text, not bring back what it replaced"
        );
    }

    /// A 30-line draft in a field that shows 3 to 12 rows, with the caret on the first row.
    /// `set_text` leaves the caret at the end, with the view already at its maximum scroll, so a
    /// downward wheel would have nowhere to go. Returns a point over the field.
    fn draft(cx: &mut TestAppContext) -> (Entity<Hosted>, &mut VisualTestContext, Point<Pixels>) {
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
        host.update(cx, |host, cx| {
            host.input.update(cx, |input, cx| input.select(0..0, cx));
        });
        cx.run_until_parked();
        let over = first_row(&host, cx).origin + point(px(10.0), px(10.0));
        (host, cx, over)
    }

    fn first_row(host: &Entity<Hosted>, cx: &mut VisualTestContext) -> gpui::Bounds<Pixels> {
        host.read_with(cx, |host, cx| host.input.read(cx).bounds_for(0))
            .expect("the field must be laid out")
    }

    /// One wheel event of `by` pixels downward, with the pointer at `over`.
    fn wheel_down(cx: &mut VisualTestContext, over: Point<Pixels>, by: f32) {
        cx.simulate_mouse_move(over, None, Modifiers::none());
        cx.simulate_event(ScrollWheelEvent {
            position: over,
            delta: ScrollDelta::Pixels(point(px(0.0), px(-by))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn a_long_multi_line_draft_scrolls_under_the_wheel(cx: &mut TestAppContext) {
        let (host, cx, over) = draft(cx);
        let before = first_row(&host, cx);

        wheel_down(cx, over, 40.0);

        let after = first_row(&host, cx);
        assert!(
            after.origin.y < before.origin.y,
            "a wheel down must move the first row up: {:?} then {:?}",
            before.origin.y,
            after.origin.y
        );
    }

    /// The caret has not moved when Delete removes a character after it, nor when Left finds it
    /// at the start already, yet each is the user acting on the caret: the view must return to it.
    #[gpui::test]
    async fn a_wheeled_draft_follows_the_caret_again_on_the_next_edit(cx: &mut TestAppContext) {
        let (host, cx, over) = draft(cx);
        let top = first_row(&host, cx).origin.y;
        for key in ["delete", "left"] {
            wheel_down(cx, over, 40.0);
            assert!(
                first_row(&host, cx).origin.y < top,
                "the wheel must have scrolled the draft before {key}"
            );

            cx.simulate_keystrokes(key);

            assert_eq!(
                first_row(&host, cx).origin.y,
                top,
                "{key} must bring the view back to the caret"
            );
        }
    }

    #[gpui::test]
    async fn a_wheel_scrolls_a_draft_alone_and_reaches_the_page_at_its_limit(
        cx: &mut TestAppContext,
    ) {
        let (host, cx, over) = draft(cx);

        wheel_down(cx, over, 40.0);
        let reached = host.read_with(cx, |host, _| host.page_wheels);
        assert_eq!(reached, 0, "a wheel that scrolls the draft must not also scroll the page");

        wheel_down(cx, over, 10_000.0);
        wheel_down(cx, over, 40.0);
        let reached = host.read_with(cx, |host, _| host.page_wheels);
        assert_eq!(reached, 1, "a wheel the draft cannot use must reach the page");
    }
}

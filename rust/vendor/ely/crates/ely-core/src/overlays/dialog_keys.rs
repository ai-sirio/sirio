//! The keys a dialog takes as it opens. A plain test module, so the repository's gate runs it.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AppContext, Context, Entity, IntoElement, KeyUpEvent, Keystroke, ParentElement, Render,
    TestAppContext, VisualTestContext, Window, div,
};

use crate::{forms::TextInput, overlays::PromptDialog};

/// A page that opens a prompt when told to, and counts how often the prompt sent.
struct Host {
    field: Entity<TextInput>,
    sent: Rc<Cell<u32>>,
    open: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let sent = self.sent.clone();
        div().children(self.open.then(|| {
            PromptDialog::new("prompt", "Name", &self.field, |_, _| {})
                .on_submit(move |_, _, _| sent.set(sent.get() + 1))
        }))
    }
}

#[gpui::test]
fn a_dialog_takes_the_first_key_typed_right_after_it_opens(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let sent = Rc::new(Cell::new(0));
    let (host, cx) = cx.add_window_view({
        let sent = sent.clone();
        move |window, cx| Host {
            field: cx.new(|cx| TextInput::new(window, cx)),
            sent,
            open: false,
        }
    });
    cx.run_until_parked();
    // The dialog has just opened: the key arrives before any next-frame work (its focus pass) has run.
    host.update(cx, |host, cx| {
        host.open = true;
        cx.notify();
    });
    cx.simulate_input("a");
    cx.run_until_parked();
    let field = host.read_with(cx, |host, _| host.field.clone());
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        "a",
        "the first key typed after the dialog opened reached its field"
    );
    // The release of an Enter pressed before the dialog opened: no press, so no confirm.
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").expect("a key"),
    });
    cx.run_until_parked();
    assert_eq!(sent.get(), 0, "an Enter the dialog never saw pressed confirmed it");
    // The control: the same dialog does confirm on an Enter it saw pressed, so the zero above means something.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(sent.get(), 1, "an Enter the dialog saw pressed confirms");
}

/// A prompt open with focus in its field, and how many times it has sent. Moved from the old crate.
fn open(
    cx: &mut TestAppContext,
) -> (Entity<TextInput>, Rc<Cell<u32>>, &mut VisualTestContext) {
    cx.update(crate::init_for_tests);
    let sent = Rc::new(Cell::new(0));
    let (host, cx) = cx.add_window_view({
        let sent = sent.clone();
        move |window, cx| Host {
            field: cx.new(|cx| TextInput::new(window, cx)),
            sent,
            open: true,
        }
    });
    cx.run_until_parked();
    let field = host.read_with(cx, |host, _| host.field.clone());
    let focus = field.read_with(cx, |field, _| field.focus().clone());
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "the prompt opens with focus in its field"
    );
    (field, sent, cx)
}

fn release_enter(cx: &mut VisualTestContext) {
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").expect("a key"),
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_release_with_no_press_does_not_confirm(cx: &mut TestAppContext) {
    let (_field, sent, cx) = open(cx);
    // The Enter that opened the dialog went down before it existed.
    release_enter(cx);
    assert_eq!(sent.get(), 0, "an Enter the dialog never saw pressed confirmed it");
}

#[gpui::test]
fn a_press_and_its_release_confirm_once(cx: &mut TestAppContext) {
    let (_field, sent, cx) = open(cx);
    cx.simulate_keystrokes("enter");
    release_enter(cx);
    assert_eq!(sent.get(), 1, "a press the dialog saw, then its release, confirms");
    release_enter(cx);
    assert_eq!(sent.get(), 1, "a second release with no press does not confirm again");
}

#[gpui::test]
fn a_press_while_the_field_is_out_of_focus_arms_nothing(cx: &mut TestAppContext) {
    let (field, sent, cx) = open(cx);
    // Tab leaves the field for the next control in the dialog.
    cx.update(|window, cx| window.focus_next(cx));
    let focus = field.read_with(cx, |field, _| field.focus().clone());
    assert!(
        !cx.update(|window, _| focus.is_focused(window)),
        "focus left the field"
    );
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
    release_enter(cx);
    assert_eq!(sent.get(), 0, "the field was not focused when Enter went down");
}

//! The keys a dialog takes as it opens. A plain test module, so the repository's gate runs it.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AppContext, Context, Entity, IntoElement, KeyUpEvent, Keystroke, ParentElement, Render,
    TestAppContext, Window, div,
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
    // The dialog opens and the first key arrives before a frame has settled.
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

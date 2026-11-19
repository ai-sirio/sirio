//! Host-owned expansion avoids keeping transient state in virtualized widgets.
use gpui::{App, ElementId, Window};
use std::rc::Rc;

/// Asked to set a widget open or closed, with the state it wants.
pub type Toggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;
/// Flips a widget's open state.
pub type Press = Rc<dyn Fn(&mut Window, &mut App)>;

/// The open state of a widget and what a press of its header does: the host's, when it
/// controls the widget, otherwise a keyed state of the window under `id`.
pub fn resolve(
    id: &ElementId,
    controlled: Option<(bool, Toggle)>,
    window: &mut Window,
    cx: &mut App,
) -> (bool, Press) {
    if let Some((open, toggle)) = controlled {
        return (open, Rc::new(move |window, cx| toggle(!open, window, cx)));
    }
    let state = window.use_keyed_state((id.clone(), "open"), cx, |_, _| false);
    let open = *state.read(cx);
    (
        open,
        Rc::new(move |_, cx| {
            state.update(cx, |value, cx| {
                *value = !*value;
                cx.notify();
            })
        }),
    )
}

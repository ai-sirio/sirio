//! Host-owned expansion avoids keeping transient state in virtualized widgets.
use gpui::{App, ElementId, Window};
use std::rc::Rc;
pub(crate) type Toggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;
pub(crate) type Press = Rc<dyn Fn(&mut Window, &mut App)>;
pub(crate) fn resolve(
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

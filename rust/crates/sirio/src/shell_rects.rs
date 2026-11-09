//! Window bounds of the shell's drawn rectangles from their last paint, for
//! the debug `shellRects` entry of `surface.layout.read` that the layout E2E
//! measures. Each cell is written by a non-interactive canvas laid over the
//! measured element, so a probe adds nothing to layout and reports what was
//! actually painted, not what the layout code intended.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, IntoElement, Pixels, Styled, canvas, px};

#[derive(Default)]
pub(crate) struct ShellRects {
    pub(crate) titlebar: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) left: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) center: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) right: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) primary: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) secondary: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) primary_strip: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) secondary_strip: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) secondary_close: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) status: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) divider_handle: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl ShellRects {
    /// Forgets the rects of everything but the title bar and status bar, for
    /// the frames in which the three columns are not drawn at all.
    pub(crate) fn clear_columns(&self) {
        for cell in [
            &self.left,
            &self.center,
            &self.right,
            &self.primary,
            &self.secondary,
            &self.primary_strip,
            &self.secondary_strip,
            &self.secondary_close,
            &self.divider_handle,
        ] {
            cell.set(None);
        }
    }
}

/// An absolute, non-interactive child that records the bounds it is laid out
/// to. The parent must be positioned (`relative()`). An absolute child spans
/// its parent's padding box, inside the borders, so `border` is the parent's
/// border widths (top, right, bottom, left) and the probe reaches over them:
/// what is recorded is the border box the user sees.
pub(crate) fn probe(
    cell: &Rc<Cell<Option<Bounds<Pixels>>>>,
    border: [f32; 4],
) -> impl IntoElement {
    let cell = cell.clone();
    let [top, right, bottom, left] = border;
    canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
        .absolute()
        .top(px(-top))
        .right(px(-right))
        .bottom(px(-bottom))
        .left(px(-left))
}

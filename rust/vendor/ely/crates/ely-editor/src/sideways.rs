//! Where the code column's sideways scroll goes so the caret stays in view.

use gpui::Pixels;

/// The scroll that shows a caret at `caret` (its x from the line's start,
/// unscrolled) in a code column `width` wide, now scrolled by `scroll`,
/// keeping `margin` between the caret and either edge where it can.
///
/// It moves the least it can: a caret already in view leaves the scroll as it
/// is, and one past an edge brings that edge just far enough to show it.
pub(crate) fn follow(scroll: Pixels, caret: Pixels, width: Pixels, margin: Pixels) -> Pixels {
    // A column narrower than two margins keeps the caret as near its middle as it can.
    let margin = margin.min(width / 2.0);
    if caret < scroll + margin {
        (caret - margin).max(Pixels::ZERO)
    } else if caret > scroll + width - margin {
        caret + margin - width
    } else {
        scroll
    }
}

// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The sidebar's arrow keys: the four chords everyone already agrees on, named
//! once, and the one-step cursor move the up and down arrows make.
//!
//! The sidebar cannot hand the move to a library, because applying it means
//! touching its own cursor and expansion state. What lives here is the
//! binding, and the answer to "where does the cursor go".

use gpui::{App, KeyBinding, actions};

actions!(sirio_tree, [SelectPrevious, SelectNext, Collapse, Expand]);

/// The key context a tree claims.
pub const KEY_CONTEXT: &str = "Tree";

/// Bind the arrows. Call once, at startup.
pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, ctx),
        KeyBinding::new("down", SelectNext, ctx),
        KeyBinding::new("left", Collapse, ctx),
        KeyBinding::new("right", Expand, ctx),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Where the cursor goes for an up or down arrow at `cursor` in a list of
/// `len` rows, or `None` when it goes nowhere. Left and right are never a row
/// move here: they walk the pills inside a row.
///
/// Neither end wraps. A menu wraps because it is a ring of choices; a tree is a
/// document, and arriving back at the top because you pressed down once too
/// often loses your place in it.
pub fn step(len: usize, cursor: usize, direction: Direction) -> Option<usize> {
    if cursor >= len {
        return None;
    }
    match direction {
        Direction::Up => cursor.checked_sub(1),
        Direction::Down => (cursor + 1 < len).then_some(cursor + 1),
        Direction::Left | Direction::Right => None,
    }
}

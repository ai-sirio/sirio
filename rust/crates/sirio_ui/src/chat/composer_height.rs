//! The composer field's row arithmetic: how many rows it may grow to, and how
//! many rows a drag of its top edge asks for.

use gpui::Context;

use super::{Chat, ChatEvent};
use crate::composer_prefs::ComposerPrefs;

/// The composer's fixed line height, in px.
pub(crate) const ROW_HEIGHT: f32 = 18.0;
/// The fewest rows the field ever shows.
pub(crate) const MIN_ROWS: usize = 3;
/// The maximum a chat starts with, and the one shown before its height is known.
pub(crate) const DEFAULT_MAX_ROWS: usize = 12;
/// The height of the strip along the composer's top edge that starts a drag.
pub(crate) const EDGE_HEIGHT: f32 = 6.0;

/// The most rows the field may take in a chat `chat_height` px tall: whole rows
/// in half the chat, never fewer than `MIN_ROWS`. A height that is not a
/// positive number gives `MIN_ROWS`.
pub(crate) fn cap_rows(chat_height: f32) -> usize {
    if !chat_height.is_finite() || chat_height <= 0.0 {
        return MIN_ROWS;
    }
    ((chat_height / 2.0 / ROW_HEIGHT).floor() as usize).max(MIN_ROWS)
}

/// The rows the field takes when the pointer moves from `anchor_y` to
/// `pointer_y` (window px, growing downward) while the drag began at
/// `anchor_rows`: dragging up grows it. The row delta is rounded and the result stays within
/// `MIN_ROWS..=cap`; a `cap` under `MIN_ROWS` counts as `MIN_ROWS`. `None` when
/// either position is not a finite number.
pub(crate) fn drag_rows(
    anchor_rows: usize,
    anchor_y: f32,
    pointer_y: f32,
    cap: usize,
) -> Option<usize> {
    if !anchor_y.is_finite() || !pointer_y.is_finite() {
        return None;
    }
    let delta = ((anchor_y - pointer_y) / ROW_HEIGHT).round();
    let rows = (anchor_rows as f32 + delta).clamp(MIN_ROWS as f32, cap.max(MIN_ROWS) as f32);
    Some(rows as usize)
}

/// What a drag of the composer's top edge carries; it paints nothing.
pub(super) struct DraggedComposerEdge;

/// A press on the composer's top edge, from the press until the release.
pub(super) struct ComposerDrag {
    /// The window y the press was at.
    anchor_y: f32,
    /// The field's rows when the press began.
    anchor_rows: usize,
    /// The rows the pointer asks for now.
    rows: usize,
    /// The pointer has moved: the field is pinned to `rows` while this holds.
    pub(super) dragged: bool,
    /// `rows` has differed from `anchor_rows` at some point: the release saves.
    changed: bool,
}

impl Chat {
    /// The most rows the field is drawn to grow to, at rest: the stored maximum
    /// held to the cap, and to at least `MIN_ROWS`. Before the first paint the
    /// cap is unknown and the default holds the stored value instead, so a chat
    /// shown for the first time grows into its cap rather than past it.
    pub(super) fn drawn_max_rows(&self) -> usize {
        let frame = &self.composer_row;
        let stored = frame.stored_max_rows.get();
        match frame.cap_rows.get() {
            Some(cap) => stored.min(cap).max(MIN_ROWS),
            None => stored.min(DEFAULT_MAX_ROWS).max(MIN_ROWS),
        }
    }

    /// Brings the field's rows to the stored maximum and the cap: takes the
    /// stored maximum from the global, then sets the field to `MIN_ROWS` and the
    /// drawn maximum when it is not there already. It runs from the global's
    /// observer and the height probe's deferred update, never from `render`.
    pub(super) fn sync_composer_rows(&mut self, cx: &mut Context<Self>) {
        self.composer_row
            .stored_max_rows
            .set(ComposerPrefs::current(cx).max_rows);
        // A drag that has moved pins the field to its rows; a press that has
        // not moved pins nothing.
        let wanted = match &self.composer_drag {
            Some(drag) if drag.dragged => (drag.rows, drag.rows),
            _ => (MIN_ROWS, self.drawn_max_rows()),
        };
        if self.composer_field.read(cx).rows() != Some(wanted) {
            self.composer_field
                .update(cx, |field, cx| field.set_rows(wanted.0, wanted.1, cx));
        }
    }

    /// A press on the edge at window `pointer_y`: the anchor is the field's
    /// measured rows, or the drawn maximum before it is measured. It replaces
    /// a drag a lost release left behind.
    pub(super) fn begin_composer_drag(&mut self, pointer_y: f32) {
        let anchor_rows = self
            .composer_snapshot()
            .field_rows
            .filter(|rows| *rows >= MIN_ROWS)
            .unwrap_or_else(|| self.drawn_max_rows());
        self.composer_drag = Some(ComposerDrag {
            anchor_y: pointer_y,
            anchor_rows,
            rows: anchor_rows,
            dragged: false,
            changed: false,
        });
    }

    /// The pointer is at window `pointer_y` mid-drag: the field follows it,
    /// drawn but not saved (the release saves).
    pub(super) fn composer_drag_to(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        let cap = self.composer_row.cap_rows.get().unwrap_or(120);
        let Some(drag) = self.composer_drag.as_mut() else {
            return;
        };
        let Some(rows) = drag_rows(drag.anchor_rows, drag.anchor_y, pointer_y, cap) else {
            return;
        };
        drag.dragged = true;
        drag.rows = rows;
        if rows != drag.anchor_rows {
            drag.changed = true;
        }
        self.sync_composer_rows(cx);
    }

    /// The press ended, wherever the pointer was. It runs for each way a drag
    /// can end and does nothing the second time. A drag that changed the rows
    /// publishes them (every chat follows), then the workspace saves them.
    pub(super) fn finish_composer_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.composer_drag.take() else {
            return;
        };
        if drag.changed {
            ComposerPrefs::apply(ComposerPrefs { max_rows: drag.rows }, cx);
            self.sync_composer_rows(cx);
            cx.emit(ChatEvent::ComposerMaxRowsChanged(drag.rows));
        } else {
            self.sync_composer_rows(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_is_half_the_chat_in_whole_rows() {
        assert_eq!(cap_rows(432.0), 12);
        assert_eq!(cap_rows(431.0), 11);
        assert_eq!(cap_rows(1000.0), 27);
    }

    #[test]
    fn a_chat_too_short_or_unmeasurable_still_caps_at_three_rows() {
        for height in [0.0, 50.0, -10.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(cap_rows(height), 3, "height {height}");
        }
    }

    #[test]
    fn the_field_follows_the_pointer_in_whole_rows() {
        assert_eq!(drag_rows(3, 500.0, 464.0, 20), Some(5), "36 px up is two rows");
        assert_eq!(drag_rows(12, 500.0, 536.0, 20), Some(10), "36 px down is two rows fewer");
        assert_eq!(drag_rows(3, 500.0, 492.0, 20), Some(3), "8 px is under half a row");
        assert_eq!(drag_rows(3, 500.0, 490.0, 20), Some(4), "10 px rounds to a row");
    }

    #[test]
    fn the_field_stops_at_three_rows_and_at_the_cap() {
        assert_eq!(drag_rows(5, 500.0, 5000.0, 20), Some(3));
        assert_eq!(drag_rows(5, 500.0, -5000.0, 20), Some(20));
        assert_eq!(drag_rows(5, 500.0, 400.0, 1), Some(3), "a cap under 3 never panics");
    }

    #[test]
    fn a_pointer_that_is_not_a_number_moves_nothing() {
        assert_eq!(drag_rows(5, 500.0, f32::NAN, 20), None);
        assert_eq!(drag_rows(5, f32::INFINITY, 500.0, 20), None);
    }

    #[gpui::test]
    async fn every_open_chat_takes_a_new_maximum(cx: &mut gpui::TestAppContext) {
        use crate::chat::tests::{offline_chat_view, refresh_frame};
        use gpui::AppContext as _;

        let (painted, cx) = offline_chat_view(cx);
        // The window is tall enough that half of it holds 20 rows.
        cx.simulate_resize(gpui::size(gpui::px(900.0), gpui::px(1000.0)));
        refresh_frame(cx);
        refresh_frame(cx);
        let other = cx.new(|cx| {
            crate::chat::Chat::new(
                Some(sirio_chat::LaunchSpec::Acp(sirio_chat::AgentCommand::new(
                    "/definitely/missing/sirio-acp-agent",
                ))),
                std::env::temp_dir(),
                cx,
            )
        });

        cx.update(|_, cx| ComposerPrefs::apply(ComposerPrefs { max_rows: 20 }, cx));
        cx.run_until_parked();
        refresh_frame(cx);

        for chat in [&painted, &other] {
            let stored = chat.read_with(cx, |chat, _| chat.composer_snapshot().stored_max_rows);
            assert_eq!(stored, 20);
        }
        let rows = painted.read_with(cx, |chat, cx| chat.composer_field.read(cx).rows());
        assert_eq!(rows, Some((3, 20)));
    }
}

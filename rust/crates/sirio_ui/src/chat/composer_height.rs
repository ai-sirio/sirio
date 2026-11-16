//! The composer field's row arithmetic: how many rows it may grow to, and how
//! many rows a drag of its top edge asks for.

use gpui::{App, Bounds, Context, EntityId, Pixels};

use super::{Chat, ChatEvent};
use crate::composer_prefs::ComposerPrefs;
use super::composer_view::FIELD_PAD_Y;
use sirio_persistence::settings_ranges;

/// The field's line height at the default interface font size, in px; the
/// field draws `typography.scaled(ROW_HEIGHT)`, so the arithmetic below takes
/// the height it was drawn with.
pub(crate) const ROW_HEIGHT: f32 = 18.0;
/// The fewest rows the field ever shows.
pub(crate) const MIN_ROWS: usize = 3;
/// The maximum a chat starts with, and the one shown before its height is known.
pub(crate) const DEFAULT_MAX_ROWS: usize = 12;
/// The height of the strip along the composer's top edge that starts a drag.
pub(crate) const EDGE_HEIGHT: f32 = 6.0;

/// The band's height minus its field rows' text height, when no chips and no
/// attach error are shown: a fixed allowance, not a measurement. 61 px is the
/// 1px rule, `BAND_TOP_PAD` (10), the field's `FIELD_PAD_Y` above and below
/// (2 x 5), the 8px `gap_2` between the editor and the tool row, the 24px tool
/// row and the band's 8px bottom padding. Three of those parts (the gap, the
/// tool row and the bottom padding) have no named constant, so this is a
/// literal. The E2E readback measured it: `band` 115 px with 3 field rows at the
/// default interface font size, and 61 px plus 23 px rows at font 18.
pub(crate) const BAND_ALLOWANCE: f32 = 61.0;

/// The most rows the field may take in a chat `chat_height` px tall: whole rows
/// in what is left of half the chat after `BAND_ALLOWANCE`, never fewer than
/// `MIN_ROWS`, so half the chat stays above the band. `row_height` is the
/// field's drawn line height; it is positive (`RowFrame::row_height` holds it
/// so). Chips and an attach error come on top of it. A height that is not a
/// positive number gives `MIN_ROWS`.
pub(crate) fn cap_rows(chat_height: f32, row_height: f32) -> usize {
    if !chat_height.is_finite() || chat_height <= 0.0 {
        return MIN_ROWS;
    }
    let room = chat_height / 2.0 - BAND_ALLOWANCE;
    if room <= 0.0 {
        return MIN_ROWS;
    }
    ((room / row_height).floor() as usize).max(MIN_ROWS)
}

/// The rows the field takes when the pointer moves from `anchor_y` to
/// `pointer_y` (window px, growing downward) while the drag began at
/// `anchor_rows`: dragging up grows it. The row delta, in rows of `row_height`
/// px (the field's drawn line height), is rounded and the result stays within
/// `MIN_ROWS..=cap`; a `cap` under `MIN_ROWS` counts as `MIN_ROWS`. `None` when
/// either position is not a finite number. `row_height` is positive
/// (`RowFrame::row_height` holds it so).
pub(crate) fn drag_rows(
    anchor_rows: usize,
    anchor_y: f32,
    pointer_y: f32,
    cap: usize,
    row_height: f32,
) -> Option<usize> {
    if !anchor_y.is_finite() || !pointer_y.is_finite() {
        return None;
    }
    let delta = ((anchor_y - pointer_y) / row_height).round();
    let rows = (anchor_rows as f32 + delta).clamp(MIN_ROWS as f32, cap.max(MIN_ROWS) as f32);
    Some(rows as usize)
}

/// What a drag of the composer's top edge carries: the chat whose edge it is,
/// because GPUI hands a drag to every listener of its type. It paints nothing.
pub(super) struct DraggedComposerEdge(pub(super) EntityId);

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
    /// The global's maximum when the press began: a finish publishes only while
    /// the global still holds it, so a stale drag never overwrites a newer value.
    published: usize,
}

/// The rows a painted field box holds: its height less the padding, over
/// `row_height`, the line height it was drawn with, which is positive
/// (`RowFrame::row_height` holds it so).
pub(super) fn field_rows(bounds: Bounds<Pixels>, row_height: f32) -> usize {
    let height: f32 = bounds.size.height.into();
    ((height - 2.0 * FIELD_PAD_Y) / row_height).round().max(0.0) as usize
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
        // A drag that has moved pins the field to its rows, held to the cap as it
        // stands now (the window or pane may have been resized since the move);
        // a press that has not moved pins nothing.
        self.clamp_drag_rows();
        let wanted = match &self.composer_drag {
            Some(drag) if drag.dragged => (drag.rows, drag.rows),
            _ => (MIN_ROWS, self.drawn_max_rows()),
        };
        if self.composer_field.read(cx).rows() != Some(wanted) {
            self.composer_field
                .update(cx, |field, cx| field.set_rows(wanted.0, wanted.1, cx));
        }
    }

    /// The most rows a drag may ask for now: the cap (the stored range's end
    /// before the first paint), never past the stored range, at least `MIN_ROWS`.
    fn drag_cap(&self) -> usize {
        let stored_most = *settings_ranges::CHAT_COMPOSER_MAX_ROWS.end() as usize;
        self.composer_row
            .cap_rows
            .get()
            .unwrap_or(stored_most)
            .min(stored_most)
            .max(MIN_ROWS)
    }

    /// Holds a moved drag's rows to the current cap. A clamped value that
    /// differs from the anchor counts as a change.
    fn clamp_drag_rows(&mut self) {
        let cap = self.drag_cap();
        if let Some(drag) = self.composer_drag.as_mut() {
            if drag.dragged && drag.rows > cap {
                drag.rows = cap;
                if cap != drag.anchor_rows {
                    drag.changed = true;
                }
            }
        }
    }

    /// A press on the edge at window `pointer_y`: the anchor is the field's
    /// measured rows, or the drawn maximum before it is measured. It replaces
    /// a drag a lost release left behind.
    pub(super) fn begin_composer_drag(&mut self, pointer_y: f32, cx: &App) {
        let anchor_rows = self
            .composer_row
            .field
            .get()
            .map(|b| field_rows(b, self.composer_row.row_height()))
            .filter(|rows| *rows >= MIN_ROWS)
            .unwrap_or_else(|| self.drawn_max_rows());
        self.composer_drag = Some(ComposerDrag {
            anchor_y: pointer_y,
            anchor_rows,
            rows: anchor_rows,
            dragged: false,
            changed: false,
            published: ComposerPrefs::current(cx).max_rows,
        });
    }

    /// The pointer is at window `pointer_y` mid-drag: the field follows it,
    /// drawn but not saved (the release saves).
    pub(super) fn composer_drag_to(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        // Never past the stored range, however tall the chat.
        let cap = self.drag_cap();
        let row_height = self.composer_row.row_height();
        let Some(drag) = self.composer_drag.as_mut() else {
            return;
        };
        let Some(rows) = drag_rows(drag.anchor_rows, drag.anchor_y, pointer_y, cap, row_height)
        else {
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
        // Held to the cap as it stands at the release.
        self.clamp_drag_rows();
        let Some(drag) = self.composer_drag.take() else {
            return;
        };
        // A newer value another chat published since the press wins: this
        // drag's rows are stale, so it publishes nothing.
        if drag.changed && ComposerPrefs::current(cx).max_rows == drag.published {
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
    fn the_cap_leaves_half_the_chat_above_the_band_in_whole_rows() {
        assert_eq!(cap_rows(554.0, 18.0), 12);
        assert_eq!(cap_rows(553.0, 18.0), 11);
        assert_eq!(cap_rows(1000.0, 18.0), 24);
        assert_eq!(cap_rows(894.0, 18.0), 21);
        assert_eq!(cap_rows(294.0, 18.0), 4);
        assert_eq!(cap_rows(266.0, 18.0), 4, "the first height that fits a fourth row");
    }

    #[test]
    fn a_chat_too_short_or_unmeasurable_still_caps_at_three_rows() {
        for height in [265.0, 230.0, 124.0, 122.0, 50.0, 0.0, -10.0] {
            assert_eq!(cap_rows(height, 18.0), 3, "height {height}");
        }
        for height in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(cap_rows(height, 18.0), 3, "height {height}");
        }
    }

    #[test]
    fn the_field_follows_the_pointer_in_whole_rows() {
        assert_eq!(drag_rows(3, 500.0, 464.0, 20, 18.0), Some(5), "36 px up is two rows");
        assert_eq!(drag_rows(12, 500.0, 536.0, 20, 18.0), Some(10), "36 px down is two rows fewer");
        assert_eq!(drag_rows(3, 500.0, 492.0, 20, 18.0), Some(3), "8 px is under half a row");
        assert_eq!(drag_rows(3, 500.0, 490.0, 20, 18.0), Some(4), "10 px rounds to a row");
    }

    #[test]
    fn the_field_stops_at_three_rows_and_at_the_cap() {
        assert_eq!(drag_rows(5, 500.0, 5000.0, 20, 18.0), Some(3));
        assert_eq!(drag_rows(5, 500.0, -5000.0, 20, 18.0), Some(20));
        assert_eq!(drag_rows(5, 500.0, 400.0, 1, 18.0), Some(3), "a cap under 3 never panics");
    }

    #[test]
    fn a_pointer_that_is_not_a_number_moves_nothing() {
        assert_eq!(drag_rows(5, 500.0, f32::NAN, 20, 18.0), None);
        assert_eq!(drag_rows(5, f32::INFINITY, 500.0, 20, 18.0), None);
    }

    #[test]
    fn the_cap_counts_rows_of_the_height_the_field_is_drawn_with() {
        assert_eq!(cap_rows(894.0, 23.0), 16, "at 18 px it would be 21");
        assert_eq!(cap_rows(554.0, 23.0), 9);
        assert_eq!(cap_rows(294.0, 23.0), 3);
    }

    #[test]
    fn a_drag_moves_whole_rows_of_the_drawn_height() {
        assert_eq!(drag_rows(3, 500.0, 454.0, 20, 23.0), Some(5), "46 px up is two 23 px rows");
        assert_eq!(drag_rows(3, 500.0, 489.0, 20, 23.0), Some(3), "11 px is under half a row");
    }

    #[test]
    fn a_painted_field_counts_rows_of_the_drawn_height() {
        let field = gpui::Bounds::new(
            gpui::point(gpui::px(0.0), gpui::px(0.0)),
            gpui::size(gpui::px(400.0), gpui::px(8.0 * 23.0 + 2.0 * FIELD_PAD_Y)),
        );
        assert_eq!(field_rows(field, 23.0), 8, "at 18 px it would be 10");
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

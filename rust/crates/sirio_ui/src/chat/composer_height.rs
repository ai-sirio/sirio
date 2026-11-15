//! The composer field's row arithmetic: how many rows it may grow to, and how
//! many rows a drag of its top edge asks for.

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
/// `anchor_rows`: dragging up grows it. The row count is rounded and stays within
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
}

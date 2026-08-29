/// Linux uses GPUI's `platform` modifier for the Super key. On macOS the same
/// GPUI bit is the Command key, so keeping this decision at the terminal
/// boundary makes the owning pane's routing platform-independent.
pub fn opens_terminal_link(platform_modifier: bool) -> bool {
    platform_modifier
}

/// Maps a click position (window coordinates) to a terminal grid cell,
/// undoing the pane's own on-screen offset first. Every pane but the one
/// flush against the window's top-left corner has a non-zero origin — see
/// `TerminalView::on_left_mouse_down` (F-TERM-UI-02) for the paint-side
/// counterpart this must invert. Pulled out as a pure function so the
/// origin subtraction itself can be unit-tested with a non-zero origin,
/// which a click fired against a window rooted at (0,0) cannot discriminate.
pub fn resolve_click_cell(
    event_x: f32,
    event_y: f32,
    origin_x: f32,
    origin_y: f32,
    cell_width: f32,
    line_height: f32,
) -> (usize, usize) {
    let local_x = (event_x - origin_x).max(0.0);
    let local_y = (event_y - origin_y).max(0.0);
    let column = (local_x / cell_width).floor().max(0.0) as usize;
    let row = (local_y / line_height).floor().max(0.0) as usize;
    (row, column)
}

pub fn url_at_column(line: &str, column: usize) -> Option<String> {
    let starts = ["https://", "http://", "file://"];
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let start = starts
            .iter()
            .find(|prefix| line[index..].starts_with(**prefix))
            .map(|prefix| index + prefix.len());
        let Some(end_start) = start else {
            index += line[index..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let mut end = end_start;
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        if (index..end).contains(&column) {
            let mut url = line[index..end].to_string();
            while matches!(url.chars().last(), Some('.') | Some(',') | Some(';')) {
                url.pop();
            }
            return (!url.is_empty()).then_some(url);
        }
        index = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_link_gesture_is_platform_modifier_only() {
        assert!(opens_terminal_link(true));
        assert!(!opens_terminal_link(false));
    }

    #[test]
    fn click_cell_undoes_a_non_zero_pane_origin() {
        // A pane flush against the window's top-left corner (origin 0,0)
        // cannot distinguish "subtracted the origin" from "ignored it" --
        // both produce the same cell. A second pane, offset by a prior
        // split, cannot: at origin (400, 100) a click at (404, 109) is cell
        // (0, 0) once the offset is undone, but would misresolve to a wild
        // cell (50, 6) if the offset were never subtracted (F-TERM-UI-02).
        assert_eq!(
            resolve_click_cell(404.0, 109.0, 400.0, 100.0, 8.0, 18.0),
            (0, 0)
        );
        assert_ne!(
            resolve_click_cell(404.0, 109.0, 0.0, 0.0, 8.0, 18.0),
            (0, 0)
        );
    }

    #[test]
    fn click_cell_clamps_above_and_left_of_origin() {
        assert_eq!(
            resolve_click_cell(10.0, 10.0, 400.0, 100.0, 8.0, 18.0),
            (0, 0)
        );
    }

    #[test]
    fn plain_urls_are_clickable_without_consuming_trailing_punctuation() {
        let line = "open https://example.test/docs, then";
        assert_eq!(
            url_at_column(line, 10),
            Some("https://example.test/docs".to_string())
        );
        assert_eq!(url_at_column(line, 0), None);
    }
}

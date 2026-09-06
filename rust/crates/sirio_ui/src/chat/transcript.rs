//! The transcript's turn chrome. Every entry of a turn — thought, prose,
//! tool run, closing answer — is drawn where it sits and in one weight:
//! what the model says between its tool calls is an answer like any
//! other, so there is no `Worked · N steps` zone folding interim work
//! behind a header. What is left here is the day heading that marks
//! where the calendar day changes between turns.

use chrono::{DateTime, Local};
use gpui::{AnyElement, SharedString, div, prelude::*, px};
use sirio_theme::Theme;

use super::{Chat, Entry};

/// Sirio's words for a day — bezel carries no clock. `Today`, `Yesterday`,
/// otherwise `Mon 2 Sep`.
pub(crate) fn day_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let day = at.date_naive();
    let today = now.date_naive();
    if day == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(day) {
        "Yesterday".to_string()
    } else {
        at.format("%a %-d %b").to_string()
    }
}

/// The day heading for one transcript index: the label when `entries[index]`
/// is a stamped `User` whose calendar day differs from the previous stamped
/// `User`'s, or is the first stamped one; `None` for undated turns, which
/// never break the sequence either.
pub(crate) fn heading_for(entries: &[Entry], index: usize, now: DateTime<Local>) -> Option<String> {
    let Some(Entry::User { at: Some(at), .. }) = entries.get(index) else {
        return None;
    };
    let previous = entries[..index].iter().rev().find_map(|entry| match entry {
        Entry::User { at: Some(at), .. } => Some(*at),
        _ => None,
    });
    if previous.is_some_and(|previous| previous.date_naive() == at.date_naive()) {
        return None;
    }
    Some(day_label(*at, now))
}

impl Chat {
    /// The day heading above a turn's question: `py 8`, `Subheadline` in
    /// `text_faint`, uppercased with the popover's tracking.
    pub(crate) fn render_day_heading(
        entry_index: usize,
        label: &str,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
    ) -> AnyElement {
        div()
            .debug_selector(move || format!("day-heading-{entry_index}"))
            .py(px(8.0))
            .text_size(theme.typography.footnote)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(bezel_theme.text_faint)
            .child(SharedString::from(bezel::ui::popover::tracked_upper(label)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn day_labels_are_sirios_words() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        assert_eq!(day_label(now, now), "Today");
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 3, 1, 0, 0).unwrap(), now),
            "Yesterday"
        );
        assert_eq!(
            day_label(Local.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(), now),
            "Tue 1 Sep"
        );
    }

    #[test]
    fn a_heading_appears_where_the_day_changes_and_never_for_undated_turns() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        let y = Local.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap();
        let entries = vec![
            Entry::User {
                text: "a".into(),
                at: Some(y),
            },
            Entry::User {
                text: "b".into(),
                at: Some(y),
            },
            Entry::User {
                text: "c".into(),
                at: None,
            },
            Entry::User {
                text: "d".into(),
                at: Some(now),
            },
        ];
        assert_eq!(heading_for(&entries, 0, now).as_deref(), Some("Yesterday"));
        assert_eq!(heading_for(&entries, 1, now), None);
        assert_eq!(
            heading_for(&entries, 2, now),
            None,
            "undated: no heading, no break"
        );
        assert_eq!(heading_for(&entries, 3, now).as_deref(), Some("Today"));
    }
}

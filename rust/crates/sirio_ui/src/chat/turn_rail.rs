//! The turn rail: one hairline tick per user message, along the left edge of
//! the transcript. The tick for the turn under the viewport's top edge is
//! drawn longer and brighter; hovering a tick previews its message in a
//! bezel `HoverCard`, clicking it scrolls the transcript to that turn.
//!
//! Everything that decides *what* the rail shows is pure and lives up here;
//! the element itself is built from bezel's tokens in `Chat::render_turn_rail`.

use bezel::theme::ink;
use bezel::ui::hover_card::HoverCard;
use chrono::{DateTime, Local};
use gpui::{
    AnyElement, Context, Empty, ListOffset, Pixels, SharedString, canvas, div, prelude::*, px,
};

use super::{Chat, Entry};

/// Longest preview a tick offers on hover — the fold row's own clip, so a
/// turn reads the same whether it is named by its fold or by its tick.
pub(crate) const PREVIEW_MAX_CHARS: usize = super::TURN_LABEL_MAX_CHARS;
/// Step between ticks when the transcript has room for it — ChatGPT's rail
/// spaces its ticks about this far apart on a full-height window.
pub(crate) const REST_GAP: Pixels = px(20.0);
/// Tightest the ticks pack before the rail simply runs out of viewport.
pub(crate) const MIN_GAP: Pixels = px(3.0);

/// One tick of the rail: a user message the transcript can jump back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnTick {
    /// Index of the [`Entry::User`] this tick scrolls to.
    pub entry_index: usize,
    /// The message's first line, clipped to [`PREVIEW_MAX_CHARS`].
    pub preview: String,
    /// When the message was sent, for the preview's heading.
    pub at: Option<DateTime<Local>>,
}

/// One tick per user message, in transcript order — ChatGPT's rail counts
/// questions, not the app's `TurnFooter` segments, so a turn the reader is
/// still typing into has a tick from the moment it is sent.
pub(crate) fn turn_ticks(entries: &[Entry]) -> Vec<TurnTick> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(entry_index, entry)| match entry {
            Entry::User { text, at } => Some(TurnTick {
                entry_index,
                preview: text
                    .lines()
                    .next()
                    .unwrap_or(text.as_str())
                    .chars()
                    .take(PREVIEW_MAX_CHARS)
                    .collect(),
                at: *at,
            }),
            _ => None,
        })
        .collect()
}

/// Which tick is "current": the last one whose message sits at or above the
/// viewport's top edge (`ListState::logical_scroll_top().item_ix`). `None`
/// while the viewport is still above the first message.
pub(crate) fn active_tick(ticks: &[TurnTick], top_item_ix: usize) -> Option<usize> {
    ticks
        .iter()
        .rposition(|tick| tick.entry_index <= top_item_ix)
}

/// How much of a row must still be in view for it to count as the one at
/// the top — less than this and the reader is already looking at the next.
/// Above a fold row's bottom padding, below any row worth reading.
pub(crate) const VISIBLE_MIN: Pixels = px(40.0);

/// The first row the reader actually sees at the viewport's top edge.
///
/// `ListState::logical_scroll_top` is not it, for two reasons. F-CHAT-22
/// keeps one list row per entry and draws a folded turn's hidden members at
/// zero height, so the logical top is often the previous turn's hidden
/// footer. And a row scrolled almost entirely off — a fold with only its
/// bottom padding left — still *is* the logical top while the eye reads the
/// row under it. `rows` is the list's `bounds_for_item` as a top..bottom
/// range (rows it did not draw answer `None`); the walk takes the first row
/// with at least [`VISIBLE_MIN`] — or half of itself, for short rows — still
/// below `viewport_top`, and falls back to the logical top when nothing is
/// measured yet.
pub(crate) fn top_visible_item(
    top_item_ix: usize,
    count: usize,
    viewport_top: Pixels,
    rows: impl Fn(usize) -> Option<std::ops::Range<Pixels>>,
) -> usize {
    for ix in top_item_ix..count {
        let Some(row) = rows(ix) else { break };
        let height = row.end - row.start;
        if height <= px(0.0) {
            continue;
        }
        let visible = row.end - row.start.max(viewport_top);
        if visible >= VISIBLE_MIN.min(height / 2.0) {
            return ix;
        }
    }
    top_item_ix
}

/// The step between ticks: [`REST_GAP`] while `count` ticks fit in
/// `available`, packed tighter as they stop fitting, never under
/// [`MIN_GAP`].
pub(crate) fn tick_gap(count: usize, available: Pixels) -> Pixels {
    if count < 2 {
        return REST_GAP;
    }
    let fitted = available / count as f32;
    fitted.clamp(MIN_GAP, REST_GAP)
}

/// Where the rail's left edge sits inside `chat-root`.
const RAIL_LEFT: f32 = 6.0;
/// The transcript's own top padding — the rail starts where the list does.
const RAIL_TOP: f32 = 28.0;
/// A resting tick, and the longer one for the current turn.
const TICK: f32 = 10.0;
const ACTIVE_TICK: f32 = 16.0;
/// Tallest a tick's hit row grows — a hairline alone is nothing to aim at.
const HIT_ROW: Pixels = px(8.0);

impl Chat {
    /// The rail, laid over the left margin of `chat-root` (which is
    /// `relative`) and spanning the transcript list's viewport. Empty for a
    /// transcript with no user message yet.
    ///
    /// The viewport height comes from the list as the last frame left it —
    /// all a render pass can see. The canvas at the end asks for one more
    /// frame when the list's fresh layout disagrees, so the rail is right on
    /// the frame after it first appears rather than after the next scroll.
    pub(crate) fn render_turn_rail(
        &self,
        bezel_theme: &bezel::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ticks = turn_ticks(&self.entries);
        if ticks.is_empty() {
            return Empty.into_any_element();
        }
        let viewport_bounds = self.list_state.viewport_bounds();
        let viewport = viewport_bounds.size.height;
        let top = top_visible_item(
            self.list_state.logical_scroll_top().item_ix,
            self.entries.len(),
            viewport_bounds.top(),
            |ix| {
                self.list_state
                    .bounds_for_item(ix)
                    .map(|bounds| bounds.top()..bounds.bottom())
            },
        );
        let active = active_tick(&ticks, top);
        let gap = tick_gap(ticks.len(), viewport);
        let row = gap.min(HIT_ROW);
        let count = ticks.len();

        let mut rail = div()
            .id("turn-rail")
            .debug_selector(|| "turn-rail".into())
            .absolute()
            .left(px(RAIL_LEFT))
            .top(px(RAIL_TOP))
            .h(viewport)
            .w(px(ACTIVE_TICK))
            .overflow_hidden()
            .flex()
            .flex_col()
            .items_start()
            .justify_center()
            .gap(gap - row);

        for (index, tick) in ticks.into_iter().enumerate() {
            let is_active = active == Some(index);
            let entry_index = tick.entry_index;
            let group = SharedString::from(format!("turn-tick-{index}"));
            let title = SharedString::from(tick.preview);
            let when = match tick.at {
                Some(at) => format!("Turn {} · {}", index + 1, at.format("%H:%M")),
                None => format!("Turn {} of {count}", index + 1),
            };
            let line = div()
                .h(px(1.0))
                .w(px(if is_active { ACTIVE_TICK } else { TICK }))
                .rounded_full()
                .bg(if is_active {
                    bezel_theme.text
                } else {
                    ink(0.28)
                })
                .group_hover(group.clone(), |s| s.bg(ink(0.6)));
            rail = rail.child(
                div()
                    .id(("turn-tick", index))
                    .debug_selector(move || format!("turn-tick-{index}"))
                    .group(group)
                    .h(row)
                    .w_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hoverable_tooltip(move |window, cx| {
                        HoverCard::summary(title.clone(), when.clone(), window, cx)
                    })
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        chat.list_state.scroll_to(ListOffset {
                            item_ix: entry_index,
                            offset_in_item: px(0.0),
                        });
                        cx.notify();
                    }))
                    .child(line),
            );
        }

        let list_state = self.list_state.clone();
        rail.child(
            canvas(
                move |_, window, _| {
                    let fresh = list_state.viewport_bounds().size.height;
                    if (fresh - viewport).abs() > px(0.5) {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Entry;
    use gpui::px;

    fn user(text: &str) -> Entry {
        Entry::User {
            text: text.into(),
            at: None,
        }
    }
    fn prose(text: &str) -> Entry {
        Entry::Assistant {
            text: text.into(),
            document: crate::chat::parse_chat_markdown(text),
        }
    }
    fn tool(id: &str) -> Entry {
        crate::chat::tests::test_tool_call(id)
    }

    #[test]
    fn one_tick_per_user_message() {
        let entries = vec![
            user("a"),
            prose("answer"),
            tool("t1"),
            user("b"),
            Entry::TurnFooter("12:00".into()),
            user("c"),
        ];
        let ticks = turn_ticks(&entries);
        assert_eq!(
            ticks.iter().map(|t| t.entry_index).collect::<Vec<_>>(),
            vec![0, 3, 5]
        );
        assert_eq!(
            ticks.iter().map(|t| t.preview.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn no_ticks_for_a_transcript_without_user_messages() {
        assert!(turn_ticks(&[]).is_empty());
        assert!(turn_ticks(&[prose("hello"), tool("t")]).is_empty());
    }

    #[test]
    fn the_preview_is_the_first_line_clipped() {
        let long = "x".repeat(200);
        let entries = vec![user("first line\nsecond line"), user(&long)];
        let ticks = turn_ticks(&entries);
        assert_eq!(ticks[0].preview, "first line");
        assert_eq!(ticks[1].preview.chars().count(), PREVIEW_MAX_CHARS);
    }

    #[test]
    fn the_active_tick_is_the_last_one_at_or_above_the_viewport_top() {
        let entries = vec![
            user("a"),
            prose("…"),
            prose("…"),
            user("b"),
            prose("…"),
            user("c"),
        ];
        let ticks = turn_ticks(&entries);
        assert_eq!(active_tick(&ticks, 0), Some(0));
        assert_eq!(active_tick(&ticks, 2), Some(0));
        assert_eq!(active_tick(&ticks, 3), Some(1));
        assert_eq!(active_tick(&ticks, 4), Some(1));
        assert_eq!(active_tick(&ticks, 5), Some(2));
        assert_eq!(active_tick(&ticks, 40), Some(2));
    }

    #[test]
    fn no_tick_is_active_above_the_first_message() {
        // A restored transcript can open on a non-user entry (an error, a
        // day heading's prose); nothing is "current" until the first turn.
        let entries = vec![prose("welcome"), prose("…"), user("a")];
        let ticks = turn_ticks(&entries);
        assert_eq!(active_tick(&ticks, 0), None);
        assert_eq!(active_tick(&ticks, 2), Some(0));
        assert_eq!(active_tick(&[], 0), None);
    }

    #[test]
    fn the_top_visible_item_skips_hidden_rows_and_a_row_that_barely_shows() {
        let top = px(100.0);
        let rows = |ix: usize| match ix {
            // Hidden members of a folded turn: F-CHAT-22 keeps one row per
            // entry and draws these at zero height, right at the top edge.
            3 | 4 => Some(top..top),
            // A fold row scrolled so far that only its 12px bottom padding
            // is still in view — the reader sees the next fold as first.
            5 => Some(px(54.0)..px(112.0)),
            // The next fold, fully in view.
            6 => Some(px(112.0)..px(170.0)),
            _ => None,
        };
        assert_eq!(top_visible_item(3, 10, top, rows), 6);
        // A row that is drawn in full is its own answer.
        assert_eq!(top_visible_item(6, 10, top, rows), 6);
        // A tall reply half scrolled off still owns the top edge.
        let tall = |ix: usize| (ix == 2).then(|| px(-400.0)..px(400.0));
        assert_eq!(top_visible_item(2, 10, top, tall), 2);
        // Before layout nothing is measured: fall back to the logical top.
        assert_eq!(top_visible_item(3, 10, top, |_| None), 3);
        // Hidden rows all the way to the end: nothing better than the top.
        assert_eq!(top_visible_item(3, 5, top, |_| Some(top..top)), 3);
    }

    #[test]
    fn the_gap_rests_at_its_step_when_there_is_room() {
        assert_eq!(tick_gap(10, px(600.0)), REST_GAP);
        assert_eq!(tick_gap(0, px(600.0)), REST_GAP);
        assert_eq!(tick_gap(1, px(600.0)), REST_GAP);
    }

    #[test]
    fn the_gap_compresses_to_fit_many_turns() {
        // 60 ticks in 300px cannot keep a 14px step: 5px each fits exactly.
        assert_eq!(tick_gap(60, px(300.0)), px(5.0));
        // …and never below the floor, however many turns there are.
        assert_eq!(tick_gap(1000, px(300.0)), MIN_GAP);
        // A frame before layout (zero height) must not divide to nothing.
        assert_eq!(tick_gap(10, px(0.0)), MIN_GAP);
    }
}

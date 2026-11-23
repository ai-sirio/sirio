//! The usage-limit card (spec §5.4): what a rejected usage limit with a reset time shows in place
//! of the one-line warning, and what it offers once the countdown ends.
//!
//! The card is not a transcript row of its own. The warning notice the limit would have been
//! stays in `entries` (an aside, never persisted) and the card is drawn in its place, so the list
//! keeps one row per entry and a limit with no failed turn still has a row to sit on. The error
//! row of a turn that failed with the limit is hidden while the card covers it.

use chrono::{DateTime, Local};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::feedback::{Alert, Countdown};
use ely_gpui_component::primitives::Severity;
use ely_gpui_component::theme::{ControlSize, TextSize};
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, Styled, Window, div,
};
use sirio_theme::Theme;
use std::time::{Duration, Instant};

/// The card's title.
pub(super) const TITLE: &str = "Usage limit reached";
/// What the card says once the limit is over and no turn failed with it.
pub(super) const LIFTED: &str = "Usage limit lifted";

/// What the limit is doing now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LimitPhase {
    /// The reset is still ahead: the card counts down to it.
    Counting,
    /// The reset has passed and a turn failed with the limit: the card offers Try again.
    Retry,
    /// The reset has passed and no turn failed with it: the card says so, with no button.
    Lifted,
}

impl LimitPhase {
    /// The socket's spelling, a limit card row's `phase`. Debug builds only, like the readout.
    #[cfg(debug_assertions)]
    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::Counting => "counting",
            Self::Retry => "retry",
            Self::Lifted => "lifted",
        }
    }
}

/// A rejected usage limit, held while the chat lives. Not persisted, like the warning it stands
/// in for.
#[derive(Clone, Debug)]
pub(crate) struct LimitCard {
    /// The row it sits on: the warning notice it stands in for.
    pub(super) entry: usize,
    /// When the limit resets, on the wall clock. Read by the debug readout only.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) resets_at: DateTime<Local>,
    /// When the countdown ends, on the clock of the executor that draws it.
    pub(super) until: Instant,
    /// The error row of the turn that failed with this limit, while that row still offers Retry.
    pub(super) failed_entry: Option<usize>,
    /// The warning's sentence: the window, the scope and the reset time.
    pub(super) sentence: String,
}

impl LimitCard {
    /// The card for a limit that resets at `resets_at`, sitting on row `entry`. `wall_now` is the
    /// wall clock and `now` the executor's: the time left is read on the first and counted on the
    /// second. A reset that is already past (clock skew, a late event) ends at `now`, so it never
    /// counts negative. `None` for a reset too far off for the platform's clock to hold.
    pub(super) fn new(
        entry: usize,
        resets_at: DateTime<Local>,
        wall_now: DateTime<Local>,
        now: Instant,
        sentence: String,
    ) -> Option<Self> {
        let left = resets_at
            .signed_duration_since(wall_now)
            .to_std()
            .unwrap_or(Duration::ZERO);
        Some(Self {
            entry,
            resets_at,
            until: now.checked_add(left)?,
            failed_entry: None,
            sentence,
        })
    }

    /// `entries[index]` was removed: the rows behind it moved up one. `false` when it was the
    /// card's own row, which leaves nothing to draw it on. The failed row, once gone, is no
    /// longer one to retry.
    pub(super) fn entry_removed(&mut self, index: usize) -> bool {
        if let Some(failed) = self.failed_entry {
            self.failed_entry = match failed {
                gone if gone == index => None,
                behind if behind > index => Some(behind - 1),
                before => Some(before),
            };
        }
        if self.entry == index {
            return false;
        }
        if self.entry > index {
            self.entry -= 1;
        }
        true
    }
}

/// The phase of `card` at `now`: counting until the reset, then Try again when a turn failed with
/// the limit and the limit lifted when none did.
pub(crate) fn phase(card: &LimitCard, now: Instant) -> LimitPhase {
    if now < card.until {
        LimitPhase::Counting
    } else if card.failed_entry.is_some() {
        LimitPhase::Retry
    } else {
        LimitPhase::Lifted
    }
}

/// The card, drawn: Ely's warning `Alert` with the sentence, and under it the countdown, the Try
/// again button or the words that the limit lifted, by `phase`. `id` is the row's, so the
/// countdown keeps its clock from frame to frame.
pub(super) fn render(
    card: &LimitCard,
    phase: LimitPhase,
    id: ElementId,
    on_retry: impl Fn(&mut Window, &mut App) + 'static,
    theme: &Theme,
) -> AnyElement {
    let alert = Alert::new((id.clone(), "limit-card"), Severity::Warning, TITLE)
        .body(card.sentence.clone());
    let alert = match phase {
        LimitPhase::Counting => alert.action(
            div()
                .debug_selector(|| "limit-card-countdown".into())
                .child(Countdown::new((id, "limit-countdown"), card.until).size(TextSize::Md)),
        ),
        LimitPhase::Retry => alert.action(
            div().debug_selector(|| "limit-card-retry".into()).child(
                Button::new((id, "limit-retry"), "Try again")
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .on_click(move |_, window, cx| on_retry(window, cx)),
            ),
        ),
        LimitPhase::Lifted => alert.action(
            div()
                .debug_selector(|| "limit-card-lifted".into())
                .text_color(theme.ely.fg_muted)
                .child(LIFTED),
        ),
    };
    div()
        .w_full()
        .debug_selector(|| "limit-card".into())
        .child(alert)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    //! The ways the phase can fail, written before the code:
    //! - the card flips early, before the countdown reaches zero, or late, after it;
    //! - a failed turn's card says "lifted" and offers nothing, or a card with no failed turn
    //!   offers a Try again that resends nothing;
    //! - a reset that is already past counts (a negative or a frozen countdown) instead of
    //!   going straight to what follows it.
    use super::*;
    use std::time::Duration;

    fn card(until: Instant, failed_entry: Option<usize>) -> LimitCard {
        LimitCard {
            entry: 2,
            resets_at: Local::now(),
            until,
            failed_entry,
            sentence: "5-hour usage limit reached, resets at 15:15.".into(),
        }
    }

    #[test]
    fn phase_counts_then_offers_retry_after_a_failure() {
        let start = Instant::now();
        let until = start + Duration::from_secs(90);
        let failed = card(until, Some(4));
        assert_eq!(phase(&failed, start), LimitPhase::Counting);
        assert_eq!(
            phase(&failed, until - Duration::from_nanos(1)),
            LimitPhase::Counting,
            "not before zero"
        );
        assert_eq!(phase(&failed, until), LimitPhase::Retry, "at zero");
        assert_eq!(
            phase(&failed, until + Duration::from_secs(3_600)),
            LimitPhase::Retry,
            "and it stays"
        );
    }

    #[test]
    fn phase_counts_then_lifts_without_a_failure() {
        let start = Instant::now();
        let until = start + Duration::from_secs(90);
        let clear = card(until, None);
        assert_eq!(phase(&clear, start), LimitPhase::Counting);
        assert_eq!(
            phase(&clear, until - Duration::from_nanos(1)),
            LimitPhase::Counting,
            "not before zero"
        );
        assert_eq!(phase(&clear, until), LimitPhase::Lifted, "at zero");
        assert_eq!(
            phase(&clear, until + Duration::from_secs(3_600)),
            LimitPhase::Lifted,
            "and it stays"
        );
    }

    #[test]
    fn a_past_reset_skips_the_countdown() {
        let now = Instant::now();
        for until in [now, now - Duration::from_secs(30)] {
            assert_eq!(phase(&card(until, Some(4)), now), LimitPhase::Retry);
            assert_eq!(phase(&card(until, None), now), LimitPhase::Lifted);
        }
    }
}

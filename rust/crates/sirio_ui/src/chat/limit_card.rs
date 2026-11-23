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
use ely_gpui_component::theme::{ActiveTheme as _, ControlSize, TextSize, Theme as ElyTheme};
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, FontWeight, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels, Role,
    StatefulInteractiveElement, Styled, Task, Window, div,
};
use sirio_theme::Theme;
use std::time::{Duration, Instant};

/// The card's title.
pub(super) const TITLE: &str = "Usage limit reached";
/// What the card says once the limit is over and no turn failed with it.
pub(super) const LIFTED: &str = "Usage limit lifted";
/// The longest the chat sleeps before it reads the wall clock again to see whether the limit is
/// over: a machine that slept through the reset wakes at most this late.
pub(super) const WAKE_EVERY: Duration = Duration::from_secs(60);
/// How far the countdown's anchor may drift from the wall clock before it is set again. Every
/// frame reads two clocks a few milliseconds apart; moving the anchor each time would restart
/// Ely's clock on every frame.
const ANCHOR_TOLERANCE: Duration = Duration::from_secs(1);

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
pub(crate) struct LimitCard {
    /// The row it sits on: the warning notice it stands in for.
    pub(super) entry: usize,
    /// When the limit resets, on the wall clock. Read by the debug readout only.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) resets_at: DateTime<Local>,
    /// Where the countdown counts to, on the executor's clock. Only the countdown reads it: the
    /// phase is read off `resets_at` and the wall clock, which run through a sleep. It is set
    /// again when the wall clock disagrees with it (`countdown_until`).
    pub(super) until: Instant,
    /// The error row of the turn that failed with this limit, while that row still offers Retry.
    pub(super) failed_entry: Option<usize>,
    /// The warning's sentence: the window, the scope and the reset time.
    pub(super) sentence: String,
    /// The loop that wakes the chat at the reset. The card owns it: a replaced card, a new
    /// conversation and a closed chat drop it, and a dropped task is cancelled.
    pub(super) wake: Option<Task<()>>,
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
        Some(Self {
            entry,
            resets_at,
            until: now.checked_add(time_left(resets_at, wall_now))?,
            failed_entry: None,
            sentence,
            wake: None,
        })
    }

    /// The instant the countdown counts to, for a frame drawn at `wall_now` and `now`. The wall
    /// clock decides how long is left, so a machine that slept wakes to the right number; the
    /// anchor is only moved when the two clocks disagree by more than a second, and otherwise
    /// stays where it is from frame to frame.
    pub(super) fn countdown_until(&mut self, wall_now: DateTime<Local>, now: Instant) -> Instant {
        if let Some(fresh) = now.checked_add(time_left(self.resets_at, wall_now)) {
            let apart = fresh
                .saturating_duration_since(self.until)
                .max(self.until.saturating_duration_since(fresh));
            if apart > ANCHOR_TOLERANCE {
                self.until = fresh;
            }
        }
        self.until
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

/// What assistive technology calls the card: the title and the sentence, as the Alert names itself
/// from a title and a body. The Alert is given neither (its title is empty and its text is in
/// its content slot), so it would otherwise be nameless.
pub(super) fn accessible_name(sentence: &str) -> String {
    format!("{TITLE}. {sentence}")
}

/// The time from `wall_now` to `resets_at`; none when it is past.
fn time_left(resets_at: DateTime<Local>, wall_now: DateTime<Local>) -> Duration {
    resets_at
        .signed_duration_since(wall_now)
        .to_std()
        .unwrap_or(Duration::ZERO)
}

/// The phase of `card` at `wall_now`: counting until the reset, then Try again when a turn failed
/// with the limit and the limit lifted when none did. The wall clock decides, because the reset
/// is a time of day the sentence names: the monotonic clock stops while the machine sleeps.
pub(crate) fn phase(card: &LimitCard, wall_now: DateTime<Local>) -> LimitPhase {
    if wall_now < card.resets_at {
        LimitPhase::Counting
    } else if card.failed_entry.is_some() {
        LimitPhase::Retry
    } else {
        LimitPhase::Lifted
    }
}

/// Lays out `child` with Ely's reduced-motion flag on, so that what it builds does not animate.
///
/// Ely's `Countdown` rolls the digit of each second for 640 ms (`AnimatedNumber`), and gpui asks
/// for a frame on every frame of a roll: a card that counts for hours would redraw the whole
/// window about two thirds of the time. With the flag on, the digits change in place once a
/// second (the roll is 1 ms), as they do for a user who asked for reduced motion.
///
/// The flag is Ely's theme's, a global that every component reads while it is built, and a
/// component is built when its parent's layout is requested, so setting it around the child's
/// `request_layout` covers the countdown and nothing else: the rest of the window is laid out
/// outside it. It is set and put back in the one call; nothing observes Ely's theme global, so
/// the effect `global_mut` queues wakes no one, and a user who has reduced motion on already is
/// left as they are.
struct StillMotion {
    child: AnyElement,
}

impl IntoElement for StillMotion {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StillMotion {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let already = cx.theme().reduced_motion;
        if !already {
            cx.global_mut::<ElyTheme>().reduced_motion = true;
        }
        let layout = self.child.request_layout(window, cx);
        if !already {
            cx.global_mut::<ElyTheme>().reduced_motion = false;
        }
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// The card, drawn: Ely's warning `Alert` holding the title and `sentence` (the caller draws it
/// through the transcript's selection, as the line it replaces was), and under them the
/// countdown to `until`, the Try again button or the words that the limit lifted, by `phase`.
/// `name` is what assistive technology calls the card (`accessible_name`). `id` is the row's, so
/// the countdown keeps its clock from frame to frame.
pub(super) fn render(
    phase: LimitPhase,
    until: Instant,
    name: String,
    sentence: AnyElement,
    id: ElementId,
    on_retry: impl Fn(&mut Window, &mut App) + 'static,
    theme: &Theme,
) -> AnyElement {
    // Title and sentence both go in the Alert's content slot: `Alert::body` is plain text that
    // cannot be selected, and the Alert draws its content above its own title.
    let words = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.ely.fg)
                .child(TITLE),
        )
        .child(
            div()
                .debug_selector(|| "limit-card-text".into())
                .text_color(theme.ely.fg_muted)
                .child(sentence),
        );
    let name_id = id.clone();
    let alert = Alert::new((id.clone(), "limit-card"), Severity::Warning, "").content(words);
    let alert = match phase {
        LimitPhase::Counting => alert.action(
            div()
                .debug_selector(|| "limit-card-countdown".into())
                .child(StillMotion {
                    child: Countdown::new((id, "limit-countdown"), until)
                        .size(TextSize::Md)
                        .into_any_element(),
                }),
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
        .id((name_id, "limit-card-name"))
        .role(Role::Group)
        .aria_label(name)
        .w_full()
        .debug_selector(|| "limit-card".into())
        .child(alert)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    //! The ways the card's clocks can fail, written before the code:
    //! - the phase flips on the monotonic clock, which does not run while the machine sleeps, so
    //!   a card whose reset the wall clock has passed still counts;
    //! - the card flips early, before the reset, or late, after it;
    //! - a failed turn's card says "lifted" and offers nothing, or a card with no failed turn
    //!   offers a Try again that resends nothing;
    //! - a reset that is already past counts (a negative or a frozen countdown) instead of
    //!   going straight to what follows it;
    //! - the time left is put on the executor's clock wrong (off by the clocks' difference, or
    //!   negative for a past reset);
    //! - the countdown's anchor moves on every frame (it restarts Ely's clock each time), or
    //!   stays put after a sleep (it keeps counting the hours the machine was asleep).
    use super::*;

    fn card(
        resets_at: DateTime<Local>,
        wall_now: DateTime<Local>,
        failed_entry: Option<usize>,
    ) -> LimitCard {
        let mut card = LimitCard::new(
            2,
            resets_at,
            wall_now,
            Instant::now(),
            "5-hour usage limit reached, resets at 15:15.".into(),
        )
        .expect("a reset the clock can hold");
        card.failed_entry = failed_entry;
        card
    }

    fn secs(count: i64) -> chrono::Duration {
        chrono::Duration::seconds(count)
    }

    #[test]
    fn phase_counts_then_offers_retry_after_a_failure() {
        let start = Local::now();
        let resets_at = start + secs(90);
        let failed = card(resets_at, start, Some(4));
        assert_eq!(phase(&failed, start), LimitPhase::Counting);
        assert_eq!(
            phase(&failed, resets_at - chrono::Duration::nanoseconds(1)),
            LimitPhase::Counting,
            "not before the reset"
        );
        assert_eq!(phase(&failed, resets_at), LimitPhase::Retry, "at the reset");
        assert_eq!(
            phase(&failed, resets_at + secs(3_600)),
            LimitPhase::Retry,
            "and it stays"
        );
    }

    #[test]
    fn phase_counts_then_lifts_without_a_failure() {
        let start = Local::now();
        let resets_at = start + secs(90);
        let clear = card(resets_at, start, None);
        assert_eq!(phase(&clear, start), LimitPhase::Counting);
        assert_eq!(
            phase(&clear, resets_at - chrono::Duration::nanoseconds(1)),
            LimitPhase::Counting,
            "not before the reset"
        );
        assert_eq!(phase(&clear, resets_at), LimitPhase::Lifted, "at the reset");
        assert_eq!(
            phase(&clear, resets_at + secs(3_600)),
            LimitPhase::Lifted,
            "and it stays"
        );
    }

    #[test]
    fn a_past_reset_skips_the_countdown() {
        let wall_now = Local::now();
        for resets_at in [wall_now, wall_now - secs(30)] {
            assert_eq!(
                phase(&card(resets_at, wall_now, Some(4)), wall_now),
                LimitPhase::Retry
            );
            assert_eq!(
                phase(&card(resets_at, wall_now, None), wall_now),
                LimitPhase::Lifted
            );
        }
    }

    #[test]
    fn the_accessible_name_holds_the_title_and_the_sentence() {
        let name = accessible_name("5-hour usage limit reached, resets at 15:15.");
        assert!(name.starts_with(TITLE), "{name:?}");
        assert!(
            name.ends_with("5-hour usage limit reached, resets at 15:15."),
            "{name:?}"
        );
    }

    #[test]
    fn new_puts_the_time_left_on_the_executor_clock() {
        let (wall_now, now) = (Local::now(), Instant::now());
        let made = |resets_at| {
            LimitCard::new(0, resets_at, wall_now, now, String::new()).expect("within the clock")
        };
        assert_eq!(made(wall_now + secs(90)).until, now + Duration::from_secs(90));
        assert_eq!(made(wall_now).until, now, "a reset that is now ends now");
        assert_eq!(made(wall_now - secs(30)).until, now, "a past reset never counts negative");
        let far = 200 * 365 * 86_400;
        assert_eq!(
            made(wall_now + secs(far)).until,
            now + Duration::from_secs(far as u64),
            "a reset years away is held, not clamped"
        );
    }

    #[test]
    fn the_countdown_keeps_its_anchor_while_the_clocks_agree() {
        let (wall0, now0) = (Local::now(), Instant::now());
        let mut card =
            LimitCard::new(0, wall0 + secs(3_600), wall0, now0, String::new()).unwrap();
        let anchor = card.until;
        // Ten seconds on both clocks, then a frame a few hundred milliseconds off the second:
        // the anchor must not move, or Ely's clock restarts on every frame.
        assert_eq!(
            card.countdown_until(wall0 + secs(10), now0 + Duration::from_secs(10)),
            anchor
        );
        assert_eq!(
            card.countdown_until(
                wall0 + chrono::Duration::milliseconds(10_400),
                now0 + Duration::from_secs(10)
            ),
            anchor
        );
        assert_eq!(card.until, anchor);
    }

    #[test]
    fn the_countdown_follows_the_wall_clock_after_a_sleep() {
        let (wall0, now0) = (Local::now(), Instant::now());
        let mut card =
            LimitCard::new(0, wall0 + secs(3_600), wall0, now0, String::new()).unwrap();
        // The machine slept 50 minutes: the wall clock moved on, the monotonic one 10 seconds.
        let (wall, now) = (wall0 + secs(3_000), now0 + Duration::from_secs(10));
        let anchor = card.countdown_until(wall, now);
        assert_eq!(anchor, now + Duration::from_secs(600), "ten minutes are left, not fifty");
        assert_eq!(card.until, anchor, "and the card keeps the new anchor");
        // The next frame, a second on, agrees with it.
        assert_eq!(
            card.countdown_until(wall + secs(1), now + Duration::from_secs(1)),
            anchor
        );
    }
}

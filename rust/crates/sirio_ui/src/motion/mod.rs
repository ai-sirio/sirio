// Ported from bezel-motion 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The frame clock, the hover fades and the loaders' maths.
//!
//! One leased drive serves everything that repeats: a view claims a redraw
//! rate with [`Painter::lease`], renews the claim from its render, and the
//! single timer behind it wakes only when some view is owed a frame.
//!
//! Reduced motion is gpui's own flag, `App::reduce_motion`. The clock has no
//! pause switch: it keeps turning while no window of the app is active,
//! because Sirio is watched from elsewhere — an agent runs for minutes while
//! the user works in another window, and the sidebar bloom is what says it is
//! still going.
//!
//! translateY is implemented as a relative-position `top` inset: taffy applies
//! relative insets after layout, so — like a CSS transform — siblings never
//! move.

use std::{collections::HashMap, time::Duration, time::Instant};

use gpui::{
    Animation, AnimationElement, AnimationExt, App, Context, ElementId, EntityId, Global,
    IntoElement, Styled, px,
};

mod curve;
mod hover;
pub mod phase;

pub use curve::{CubicBezier, EASE, EASE_TAILWIND};
pub use hover::{Fade, hover_listener, hover_t};
pub use phase::*;

// ---------------------------------------------------------------------------
// The clock — one leased drive for everything that repeats
// ---------------------------------------------------------------------------

/// Redraw rate for the pulse/spinner loaders.
///
/// 2026-08, M-series laptop: one spinner drawn at 120Hz cost 36% of a core —
/// the window redraw, with the animation math itself at 0.3%. 30fps is
/// visually equivalent for these chunky cell waves at a quarter of the draws,
/// and a window with no spinner mounted schedules nothing at all.
pub const PULSE_FPS: f32 = 30.0;

/// How long a view stays on the tick list after its last spinner paint. One
/// lease outlives a few missed frames; an unmounted spinner stops renewing and
/// the view drops off, letting the clock park.
pub const PULSE_LEASE: Duration = Duration::from_millis(300);

/// Redraw rate for hover fades. The fade is 150ms, so this paints it in
/// nine steps — the drive it replaces ran at the display's rate, whatever that
/// was.
pub const HOVER_FPS: f32 = 60.0;

/// Floor on how long the clock sleeps between wake-ups, so a lease asking for
/// an absurd rate cannot turn the loop into a spin.
const MIN_SLEEP: Duration = Duration::from_millis(1);

/// One view's claim on the clock.
struct Lease {
    /// The fastest rate anything on this view has claimed.
    period: Duration,
    /// When this view is next owed a redraw.
    due: Instant,
    /// A notify is out and the render it provoked has not renewed this lease
    /// yet. Read by [`Painter::woken`] — never by the schedule, because a
    /// claim taken from an *event* is renewed by no render at all: a hover
    /// fade would paint one frame and freeze there.
    in_flight: bool,
    /// When the claim lapses if nothing renews it.
    until: Instant,
}

#[derive(Default)]
struct PulseClock {
    /// Set on first use. Everything here reads the executor's clock rather than
    /// `Instant::now()` — one time source for scheduling and for phase, and the
    /// only way a test can advance a second of animation without waiting one.
    epoch: Option<Instant>,
    leases: HashMap<EntityId, Lease>,
    running: bool,
}

impl PulseClock {
    /// When the loop should next wake: the earliest thing owed to anyone, or
    /// the earliest lapse, whichever comes first.
    ///
    /// Every lease answers with a real time. A lease that fell back to its
    /// *lapse* while waiting on a render is what held every drive to one frame
    /// per lease rather than one per period — a spinner asking for 30fps drew
    /// about 5.
    fn next_wake(&self) -> Option<Instant> {
        self.leases
            .values()
            .map(|lease| lease.due.min(lease.until))
            .min()
    }
}

impl Global for PulseClock {}

/// A component's line back to the view that paints it.
///
/// Component state that outlives a render — a hover fade, a gesture — has to
/// name its view, because event-dispatch context cannot resolve one and the
/// only alternative left is refreshing the whole window. Holding a `Painter`
/// is what makes the two sanctioned frame requests reachable, and it is the
/// one surface to review when asking who can ask for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Painter(EntityId);

impl Painter {
    /// The view this render belongs to. Take it once, where the state is
    /// built, and keep it for as long as the state lives.
    pub fn of<T: 'static>(cx: &Context<T>) -> Self {
        Self(cx.entity_id())
    }

    /// Redraw this view once.
    pub fn notify(self, cx: &mut App) {
        cx.notify(self.0);
    }

    /// Claim `fps` redraws a second, lapsing `until` from now unless something
    /// renews it. One timer serves the whole app: it wakes only when a view is
    /// owed a frame, notifies that view alone, and parks when the last claim
    /// lapses.
    ///
    /// This is the drive for any repeating animation, yours included. gpui's
    /// `with_animation(…).repeat()` asks the *window* for a frame at the
    /// display's rate for as long as it stays mounted, and one such element is
    /// enough to hold the whole window there.
    ///
    /// Renew it from `render` — that is what makes a claim self-cancelling,
    /// since an element that unmounts stops renewing and drops off. The rate is
    /// a per-view minimum of everything claiming it, so a 60fps claim never
    /// drags a 30fps one up with it.
    pub fn lease(self, fps: f32, until: Duration, cx: &mut App) {
        lease(self.0, fps, until, cx);
    }

    /// Whether the clock is what asked for the render running now, rather than
    /// the app. True only in the render a tick provoked: the clock clears what
    /// this view is owed when it notifies, and the render's own [`lease`] is
    /// what puts it back. Read it before renewing.
    pub fn woken(self, cx: &App) -> bool {
        cx.try_global::<PulseClock>()
            .and_then(|clock| clock.leases.get(&self.0))
            .is_some_and(|lease| lease.in_flight)
    }
}

impl From<Painter> for EntityId {
    fn from(painter: Painter) -> Self {
        painter.0
    }
}

impl From<EntityId> for Painter {
    fn from(view: EntityId) -> Self {
        Self(view)
    }
}

fn lease(view: EntityId, fps: f32, until: Duration, cx: &mut App) {
    let now = cx.background_executor().now();
    let period = Duration::from_secs_f32(1.0 / fps.clamp(1.0, 240.0));
    let clock = cx.default_global::<PulseClock>();
    clock
        .leases
        .entry(view)
        .and_modify(|lease| {
            lease.period = lease.period.min(period);
            // The tick already booked the next slot; this render only pulls it
            // earlier if the claim is faster. Re-dating it from *now* would add
            // the frame's own cost to every period — 30fps drew 20.
            lease.due = lease.due.min(now + period);
            lease.in_flight = false;
            lease.until = lease.until.max(now + until);
        })
        .or_insert(Lease {
            period,
            due: now + period,
            in_flight: false,
            until: now + until,
        });
    if clock.running {
        return;
    }
    clock.running = true;
    cx.spawn(async move |cx| {
        loop {
            let sleep = cx.update(|cx| {
                let now = cx.background_executor().now();
                let clock = cx.default_global::<PulseClock>();
                clock
                    .next_wake()
                    .map(|wake| wake.saturating_duration_since(now).max(MIN_SLEEP))
            });
            let Some(sleep) = sleep else { break };
            cx.background_executor().timer(sleep).await;
            let parked = cx.update(|cx| {
                let now = cx.background_executor().now();
                // The clock is the fade store's tick too: it is what advances
                // the frame counter that evicts fades whose elements went away.
                hover::tick_hover_fades();
                let clock = cx.default_global::<PulseClock>();
                clock.leases.retain(|_, lease| lease.until > now);
                if clock.leases.is_empty() {
                    clock.running = false;
                    return true;
                }
                // Owed a frame now. The slot moves on whatever happens next,
                // so the cadence is the period and not the period plus however
                // long the frame took.
                let mut owed = Vec::new();
                for (view, lease) in clock.leases.iter_mut() {
                    if lease.due > now {
                        continue;
                    }
                    lease.due += lease.period;
                    // Frames are costing more than the rate asked for; the next
                    // slot is now rather than a run of slots already missed.
                    if lease.due <= now {
                        lease.due = now + lease.period;
                    }
                    lease.in_flight = true;
                    owed.push(*view);
                }
                for view in owed {
                    cx.notify(view);
                }
                false
            });
            if parked {
                break;
            }
        }
    })
    .detach();
}

/// Current phase `[0,1)` of a repeating `period`, plus a [`lease`] that keeps
/// the calling view re-rendering at [`PULSE_FPS`] while its spinner stays
/// mounted. All cells across all views share one epoch, so multi-instance
/// loaders stay phase-locked. Reduced motion returns a static 0 and schedules
/// nothing.
pub fn pulse_delta(period: Duration, painter: Painter, cx: &mut App) -> f32 {
    if cx.reduce_motion() {
        return 0.0;
    }
    painter.lease(PULSE_FPS, PULSE_LEASE, cx);
    let now = cx.background_executor().now();
    let clock = cx.default_global::<PulseClock>();
    let epoch = *clock.epoch.get_or_insert(now);
    let period = period.as_secs_f32();
    ((now - epoch).as_secs_f32() / period).fract()
}

// ---------------------------------------------------------------------------
// The menu exit
// ---------------------------------------------------------------------------

/// Popover-out: 0.1s — quicker than the entrance (exits should get out of the
/// way; matches the Radix convention of a shorter close than open).
pub const MENU_OUT: Duration = Duration::from_millis(100);

/// Popover exit: fade to 0 + translateY 0→−2 over [`MENU_OUT`]. The eased
/// progress `t` comes from the caller (computed off the popup's closing instant
/// at render time): `with_animation`'s element-id-keyed clock replays from 0 on
/// remount, and a replay mid-exit is a full-opacity flash. The wall-clock
/// progress is monotonic by construction; the animation wrapper here only pumps
/// frames for the exit's span, its own delta unused.
pub fn menu_out<E>(id: impl Into<ElementId>, t: f32, element: E) -> AnimationElement<E>
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(
        id,
        Animation::new(MENU_OUT).with_easing(|delta| EASE.eval(delta)),
        move |el, _| el.relative().opacity(1.0 - t).top(px(-2.0 * t)),
    )
}

// Ported from bezel-motion 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! Hover fades (CSS `transition-colors` parity).
//!
//! gpui `.hover()` styles snap by construction — the style applies the frame
//! the pointer enters. The original CSS puts Tailwind `transition-colors`
//! (150ms, cubic-bezier(0.4, 0, 0.2, 1)) on every interactive wash, so hover
//! states FADE. This is the manual-drive tween for that (never
//! `with_animation`, whose element-id-keyed clock replays on remount): a
//! per-element-key hover progress, advanced from wall time on each evaluation,
//! with the render tail requesting frames while any fade is mid-flight.
//!
//! The store is a main-thread `thread_local` rather than a gpui Global so the
//! many free-function element builders can read a fade without threading `cx`
//! through every signature. All access happens on the UI thread (element
//! builders, mouse listeners, the clock's tick).
//!
//! Staleness: an element that unmounts mid-hover never gets its leave event, so
//! entries are stamped with a frame counter on every read and pruned by the
//! clock's tick when a full tick passes without a read — a reopened menu never
//! inherits a dead entry's wash.

use std::{cell::RefCell, collections::HashMap, time::Duration, time::Instant};

use gpui::{App, SharedString, Window};

use super::{HOVER_FPS, Painter, curve::EASE_TAILWIND, lerp};

/// CSS `transition-colors` default: 150ms over [`EASE_TAILWIND`] — the temporal
/// blend every interactive hover wash rides in the original.
const HOVER_FADE: Duration = Duration::from_millis(150);

/// One element's hover fade: progress runs `origin → target` over
/// [`HOVER_FADE`], re-anchored at `origin` whenever the pointer flips
/// direction mid-flight so the blend is continuous.
#[derive(Debug, Clone, Copy)]
struct FadeEntry {
    origin: f32,
    target: f32,
    started: Instant,
    /// Frame counter at the last read (liveness stamp — see module notes).
    seen: u64,
}

impl FadeEntry {
    fn value(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= HOVER_FADE {
            return self.target;
        }
        let raw = elapsed.as_secs_f32() / HOVER_FADE.as_secs_f32();
        lerp(self.origin, self.target, EASE_TAILWIND.eval(raw))
    }

    fn settled(&self, now: Instant) -> bool {
        self.origin == self.target || now.saturating_duration_since(self.started) >= HOVER_FADE
    }
}

/// Which fade: the view that paints it, and which element inside that view.
///
/// The view is half the identity because the store is one map for the whole
/// app. Keyed on the string alone, two views using `"row-3"` trade each other's
/// wash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fade {
    pub painter: Painter,
    pub key: SharedString,
}

impl Fade {
    pub fn new(painter: Painter, key: impl Into<SharedString>) -> Self {
        Self {
            painter,
            key: key.into(),
        }
    }
}

/// Per-fade progress store. Pure core (explicit `now`); the thread-local
/// wrappers below feed it the clock.
#[derive(Default)]
struct HoverFades {
    entries: HashMap<Fade, FadeEntry>,
    frame: u64,
}

impl HoverFades {
    /// Pointer entered (`hovered`) or left the element behind `fade`. Reduced
    /// motion snaps straight to the endpoint.
    fn set_at(&mut self, fade: &Fade, hovered: bool, reduced: bool, now: Instant) {
        let target = if hovered { 1.0 } else { 0.0 };
        let current = self
            .entries
            .get(fade)
            .map(|e| e.value(now))
            .unwrap_or(0.0);
        if target == 0.0 && !self.entries.contains_key(fade) {
            return; // never-hovered element reporting a leave — nothing to do
        }
        let origin = if reduced { target } else { current };
        let seen = self.frame;
        self.entries.insert(
            fade.clone(),
            FadeEntry {
                origin,
                target,
                started: now,
                seen,
            },
        );
    }

    /// Hover progress (0..1) for `fade` at `now`; stamps liveness.
    fn value_at(&mut self, fade: &Fade, now: Instant) -> f32 {
        let frame = self.frame;
        match self.entries.get_mut(fade) {
            Some(entry) => {
                entry.seen = frame;
                entry.value(now)
            }
            None => 0.0,
        }
    }

    /// Once-per-frame bookkeeping: advance the frame counter, prune entries
    /// that settled back to rest or went a full frame unread (unmounted), and
    /// report whether any fade is still mid-flight (→ keep frames coming).
    fn tick_at(&mut self, now: Instant) -> bool {
        self.frame += 1;
        let frame = self.frame;
        let mut active = false;
        self.entries.retain(|_, entry| {
            // Unread through the whole previous frame: the element unmounted
            // (its leave event will never come) — drop the entry.
            if entry.seen + 1 < frame {
                return false;
            }
            let settled = entry.settled(now);
            if !settled {
                active = true;
            }
            // Settled at rest — steady state, indistinguishable from absent.
            !(settled && entry.target == 0.0)
        });
        active
    }
}

thread_local! {
    static HOVER_FADES: RefCell<HoverFades> = RefCell::new(HoverFades::default());
}

/// Hover progress (0..1) for `fade` this frame.
pub fn hover_t(fade: &Fade) -> f32 {
    HOVER_FADES.with(|fades| fades.borrow_mut().value_at(fade, Instant::now()))
}

/// Record a hover flip for `fade` (reduced motion snaps). Prefer
/// [`hover_listener`], which also asks the clock for the frames to paint it.
fn set_hover(fade: &Fade, hovered: bool, reduced: bool) {
    HOVER_FADES.with(|fades| {
        fades
            .borrow_mut()
            .set_at(fade, hovered, reduced, Instant::now())
    });
}

/// An `.on_hover` listener driving the fade — pair with [`hover_t`] reads of
/// the same [`Fade`] in the same element.
///
/// The view comes in on the `Fade` rather than being resolved here: this runs in
/// event-dispatch context, where `Window::current_view()` asserts.
pub fn hover_listener(fade: Fade) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |hovered, _window, cx| {
        set_hover(&fade, *hovered, cx.reduce_motion());
        fade.painter.lease(HOVER_FPS, HOVER_FADE, cx);
    }
}

/// Once-per-tick bookkeeping for the fade store, driven by the clock.
pub(super) fn tick_hover_fades() {
    HOVER_FADES.with(|fades| {
        fades.borrow_mut().tick_at(Instant::now());
    });
}

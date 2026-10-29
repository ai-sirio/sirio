// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! The state behind a popup menu: its open / closing / closed lifecycle and
//! the searchable list a picker shows.
//!
//! State only. The elements that draw a popup (the anchored card, the rows)
//! are not here; this is the part that decides *whether* one is mounted and
//! which row is active.

use std::time::{Duration, Instant};

use gpui::SharedString;

use crate::motion;

// ---------------------------------------------------------------------------
// Popup — open/closing/closed lifecycle (exit animations)
// ---------------------------------------------------------------------------

/// Popup state with an exit phase. gpui unmounts an element the frame its
/// state drops, so a closing animation needs the state held alive while
/// [`motion::menu_out`] plays: `open` → `begin_close` (render keeps mounting,
/// with the out animation and dead hit-testing) → [`reap_popup`]'s timer
/// `finish_close`es ~[`motion::MENU_OUT`] later. Use [`Self::is_open`] for
/// logic (a closing popup already reads as closed) and [`Self::get`] /
/// [`Self::is_closing`] for rendering.
pub struct Popup<T> {
    /// `Some((state, closing_since))` while mounted; `closing_since` is the
    /// exit-phase start.
    inner: Option<(T, Option<Instant>)>,
    /// Whether the popup was still mounted when the current trigger press
    /// began — see [`Self::note_trigger_press`].
    pressed_while_open: bool,
}

impl<T> Default for Popup<T> {
    fn default() -> Self {
        Self {
            inner: None,
            pressed_while_open: false,
        }
    }
}

impl<T> Popup<T> {
    pub fn open(&mut self, value: T) {
        self.inner = Some((value, None));
    }

    /// Open and interactive (not closing).
    pub fn is_open(&self) -> bool {
        matches!(self.inner, Some((_, None)))
    }

    pub fn is_closing(&self) -> bool {
        matches!(self.inner, Some((_, Some(_))))
    }

    /// When the exit phase began — what the render path hands to the popover
    /// wrappers, which derive the eased exit progress from it each frame.
    pub fn closing_since(&self) -> Option<Instant> {
        match &self.inner {
            Some((_, Some(since))) => Some(*since),
            _ => None,
        }
    }

    /// The state while mounted — open OR playing the exit animation. Render
    /// paths use this; logic paths use [`Self::as_open`]/[`Self::open_mut`].
    pub fn get(&self) -> Option<&T> {
        self.inner.as_ref().map(|(value, _)| value)
    }

    /// The state only while genuinely open — `None` during the exit phase, so
    /// event handlers on a dying popup fall through.
    pub fn as_open(&self) -> Option<&T> {
        match &self.inner {
            Some((value, None)) => Some(value),
            _ => None,
        }
    }

    pub fn open_mut(&mut self) -> Option<&mut T> {
        match &mut self.inner {
            Some((value, None)) => Some(value),
            _ => None,
        }
    }

    /// Unmount now, with no exit phase.
    ///
    /// For a surface that has no exit animation to play — a modal, which
    /// takes no `closing` and paints the same either way. Sending one of those
    /// through [`Self::begin_close`] buys nothing and costs everything: it
    /// stays fully painted for the animation's span, and if the reap never
    /// lands it stays forever, because nothing retries.
    pub fn close(&mut self) {
        self.inner = None;
    }

    /// Enter the exit phase. Returns `true` when this call started it (the
    /// caller then schedules [`reap_popup`]); `false` if already closing or
    /// closed.
    pub fn begin_close(&mut self) -> bool {
        match &mut self.inner {
            Some((_, closing @ None)) => {
                *closing = Some(Instant::now());
                true
            }
            _ => false,
        }
    }

    /// Record, from the trigger's `on_mouse_down`, whether this popup is
    /// still mounted. The anchored card's `on_mouse_down_out` fires on that
    /// same press and begins the close, so by click (mouse-up) time the
    /// popup already reads as closed — the click handler alone cannot tell
    /// "this press dismissed it; stay closed" from "open fresh", and a
    /// plain toggle closes-and-reopens (user report). Both handler orders
    /// work: open and mid-exit each count as mounted. Every trigger click
    /// is preceded by a trigger mouse-down, so the note is never stale.
    pub fn note_trigger_press(&mut self) {
        self.pressed_while_open = self.inner.is_some();
    }

    /// Consume the press note: `true` when the press that produced the
    /// current click found the popup mounted — the click should leave it
    /// closed rather than reopen it.
    pub fn take_press_was_open(&mut self) -> bool {
        std::mem::take(&mut self.pressed_while_open)
    }

    /// Drop the state now the exit phase has run its course. A popup reopened
    /// since the matching [`Self::begin_close`] is left alone — it is `None`
    /// again in the second slot, and the newer phase's own reap handles it.
    ///
    /// It does not re-check the clock. [`reap_popup`] already waited out the
    /// span on the executor's timer, and asking `Instant::elapsed` to agree
    /// makes one deadline depend on two clocks — where they disagree, and a
    /// throttled executor is where, the popup is stranded open with nothing
    /// left to retry.
    fn finish_close(&mut self) {
        if matches!(&self.inner, Some((_, Some(_)))) {
            self.inner = None;
        }
    }
}

/// Schedule the reap for a [`Popup::begin_close`]: after the exit animation's
/// span, drop the popup state and repaint. `popup` re-borrows the field from
/// the view (the state can't be captured — the view owns it).
pub fn reap_popup<V: 'static, T: 'static>(
    cx: &mut gpui::Context<V>,
    popup: impl Fn(&mut V) -> &mut Popup<T> + 'static,
) {
    cx.spawn(async move |view, cx| {
        cx.background_executor()
            .timer(motion::MENU_OUT.saturating_add(Duration::from_millis(20)))
            .await;
        view.update(cx, |view, cx| {
            popup(view).finish_close();
            cx.notify();
        })
        .ok();
    })
    .detach();
}

/// Eased exit progress (0..=1) for a [`Popup`] closing instant, computed from
/// the wall clock at render time. Monotonic by construction — unlike the
/// animation element's own clock, it can never replay from 0 mid-exit.
pub fn exit_progress(since: Instant) -> f32 {
    let raw = (since.elapsed().as_secs_f32() / motion::MENU_OUT.as_secs_f32()).clamp(0.0, 1.0);
    motion::EASE.eval(raw)
}

// ---------------------------------------------------------------------------
// Pure reducers
// ---------------------------------------------------------------------------

/// Step the active row of a menu: wraps at both ends; `None` enters at the
/// edge matching the direction. Empty menus stay `None`.
fn menu_step(active: Option<usize>, count: usize, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let count_i = count as isize;
    let next = match active {
        None => {
            if delta >= 0 {
                0
            } else {
                count_i - 1
            }
        }
        Some(at) => (at as isize + delta).rem_euclid(count_i),
    };
    Some(next as usize)
}

/// Match rank of a label against a query: `0` prefix match, `1` substring,
/// `None` no match. Case-insensitive; an empty query matches everything at
/// rank 1 (input order preserved).
fn match_rank(query: &str, label: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let label = label.to_lowercase();
    if label.starts_with(&query) {
        Some(0)
    } else if label.contains(&query) {
        Some(1)
    } else {
        None
    }
}

/// Filter + rank labels for a search query: prefix matches first, then
/// substring matches, stable within each rank. Returns indices into `labels`.
fn filter_indices<S: AsRef<str>>(query: &str, labels: &[S]) -> Vec<usize> {
    let mut ranked: Vec<(usize, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(ix, label)| match_rank(query, label.as_ref()).map(|rank| (rank, ix)))
        .collect();
    ranked.sort_by_key(|&(rank, ix)| (rank, ix));
    ranked.into_iter().map(|(_, ix)| ix).collect()
}

/// The state behind a searchable list: the items, the ranked view of them, and
/// which row of that view is active. Shared by every picker — the slash and
/// mention menus — so the mapping below is written once.
pub struct Filter {
    items: Vec<SharedString>,
    /// Indices into `items`, ranked by [`filter_indices`].
    filtered: Vec<usize>,
    /// Position within `filtered`, not within `items`.
    active: Option<usize>,
}

impl Filter {
    pub fn new(items: Vec<SharedString>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        let active = (!filtered.is_empty()).then_some(0);
        Self {
            items,
            filtered,
            active,
        }
    }

    pub fn items(&self) -> &[SharedString] {
        &self.items
    }

    /// The ranked view: indices into [`Self::items`], in display order.
    pub fn filtered(&self) -> &[usize] {
        &self.filtered
    }

    /// The highlighted row's position in the FILTERED view — what a renderer
    /// compares each row against.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Re-rank against `query`, re-entering the list at the top: after
    /// narrowing, the best match should be one Enter away.
    pub fn refilter(&mut self, query: &str) {
        self.filtered = filter_indices(query, &self.items);
        self.active = (!self.filtered.is_empty()).then_some(0);
    }

    pub fn step(&mut self, delta: isize) {
        self.active = menu_step(self.active, self.filtered.len(), delta);
    }

    /// The item confirming right now would pick — an index into
    /// [`Self::items`], never into the filtered view. Confusing the two is the
    /// defining bug of a filtered list: it only appears once a query narrows
    /// the rows, and then every selection picks the wrong thing.
    pub fn active_item(&self) -> Option<usize> {
        self.active
            .and_then(|position| self.filtered.get(position))
            .copied()
    }
}

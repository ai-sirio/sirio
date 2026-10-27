// Ported from bezel-ui 0.1.4 (MIT); see THIRD_PARTY_NOTICES.md.
//! A popup menu: the state behind it (its open / closing / closed lifecycle and
//! the searchable list a picker shows) and the pieces every Sirio menu is
//! drawn from.
//!
//! The lifecycle decides *whether* a popup is mounted and which row is active.
//! The drawing is Ely's: [`menu_card`] is its card, [`menu_row`] the frame of
//! a row, [`divider`] its separator, and [`hang_below`] / [`hang_above_at`]
//! float a card the way Ely's `menus::layer` does. The rows keep their own
//! children. What Ely cannot draw, the exit, stays Sirio's: [`leaving`] plays
//! [`motion::menu_out`] while the popup is closing.

use std::time::{Duration, Instant};

use ely_gpui_component::{
    menus::{panel_surface, row_frame},
    motion as ely_motion,
    primitives::{Raised, raise},
    theme::ActiveTheme as _,
};
use gpui::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, Div, ElementId, Pixels, Point,
    SharedString, Stateful, anchored, div, prelude::*, px,
};

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
// Drawing — Ely's card, row frame and separator; Sirio's exit
// ---------------------------------------------------------------------------

/// The frame of one menu row: Ely's [`row_frame`] and, when the row can be
/// pressed, the hover wash. `current` is the row the keys have marked;
/// `disabled` draws the quiet colour and takes neither hover nor a pointer.
/// The hover snaps, as Ely's own rows do. What the row holds, and what a click
/// does, is the caller's.
pub fn menu_row(
    id: impl Into<ElementId>,
    current: bool,
    disabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let hover = cx.theme().colors.hover;
    row_frame(id, current, disabled, cx)
        .when(!disabled, |row| row.hover(move |style| style.bg(hover)))
}

/// The card a menu's rows sit on: Ely's [`panel_surface`], inset by `p_1` and
/// stacking what it holds. Its text is Ely's `fg`, so a plain child that sets
/// no colour of its own is not drawn in gpui's default black.
pub fn menu_card(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    panel_surface(id, cx)
        .p_1()
        .flex()
        .flex_col()
        .text_color(cx.theme().colors.fg)
}

/// The hairline between two groups of rows, Ely's separator.
pub fn divider(cx: &App) -> Div {
    div().h_px().my_1().bg(cx.theme().colors.border)
}

/// `content` while the popup is open, and while it is closing the exit:
/// [`motion::menu_out`] from the wall clock (`since` is
/// [`Popup::closing_since`]), with an occluding overlay on top. The dying
/// menu's rows must not take a click, and nor may whatever is under them.
pub fn leaving(
    id: impl Into<ElementId>,
    closing: Option<Instant>,
    content: impl IntoElement,
) -> AnyElement {
    let Some(since) = closing else {
        return content.into_any_element();
    };
    let dying = div()
        .relative()
        .child(content)
        .child(div().absolute().inset_0().occlude());
    // A name of its own: reusing the entrance's would inherit its finished
    // clock and snap the exit to its end.
    motion::menu_out((id.into(), "out"), exit_progress(since), dying).into_any_element()
}

/// The gap, in pixels, between a trigger and the card [`hang_below`] opens
/// under it.
pub const HANG_GAP: Pixels = px(6.0);

/// Floats `content` under the `relative` element it is a child of, left-aligned
/// to it, `gap` below its bottom edge.
pub fn hang_below(
    id: impl Into<ElementId>,
    gap: Pixels,
    content: impl IntoElement,
    closing: Option<Instant>,
    cx: &App,
) -> AnyElement {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .size_0()
        .child(hang(
            id.into(),
            Anchor::TopLeft,
            None,
            gap,
            content,
            closing,
            cx,
        ))
        .into_any_element()
}

/// Floats `content` over the window point `at`, its bottom-left corner there,
/// [`HANG_GAP`] above it: a completion popup over the caret that opened it.
/// `at` is a window point, as a caret measures, not one relative to a parent.
pub fn hang_above_at(
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    content: impl IntoElement,
    closing: Option<Instant>,
    cx: &App,
) -> AnyElement {
    hang(
        id.into(),
        Anchor::BottomLeft,
        Some(at),
        HANG_GAP,
        content,
        closing,
        cx,
    )
    .into_any_element()
}

/// The layer both hang over the page, drawn as Ely's `menus::layer` draws its
/// own: kept inside the window with a margin, occluding what is under it, its
/// opacity easing in, raised over the page by Ely's [`raise`]. `gap` is the
/// room between the anchor corner and the card, on the side facing it. A press
/// outside the card is the host's to handle, on the card.
fn hang(
    id: ElementId,
    anchor: Anchor,
    at: Option<Point<Pixels>>,
    gap: Pixels,
    content: impl IntoElement,
    closing: Option<Instant>,
    cx: &App,
) -> Raised {
    let enter = Animation::new(ely_motion::duration(ely_motion::FAST, cx))
        .with_easing(ely_motion::ease_out_cubic);
    let body = div()
        .occlude()
        .map(|body| match anchor {
            Anchor::BottomLeft | Anchor::BottomRight => body.pb(gap),
            _ => body.pt(gap),
        })
        .child(leaving(id.clone(), closing, content))
        .with_animation((id.clone(), "in"), enter, |body, t| body.opacity(t));
    let placed = anchored()
        .anchor(anchor)
        .snap_to_window_with_margin(px(8.0))
        .map(|placed| match at {
            Some(at) => placed.position(at),
            None => placed,
        })
        .child(body);
    raise((id, "layer"), placed).with_priority(1)
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

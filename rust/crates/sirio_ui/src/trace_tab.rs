//! The Trace tab: a native Claude chat's whole trace in the Secondary half,
//! one turn at a time, as a tree of agents, model calls and tools on the left
//! and their bars on a shared time axis on the right (spec §5).
//!
//! The tab holds no data of its own. It reads the chat's `TraceStore` through
//! a weak handle and hears of a change through `ChatEvent::TraceChanged`, never
//! by observing the chat, which notifies on every streamed chunk; what it adds
//! is which turn is shown, which spans are folded and which one is selected.
//!
//! - **Turns.** `turn` is the shown turn's 1-based index in the chat. While
//!   `follow` is on it is always the latest; going back turns following off,
//!   and only the turns the chat still keeps can be reached. A trace that
//!   begins again (a new conversation, a history session opened) turns
//!   following back on.
//! - **Keys**, in the `TraceTab` context: Up and Down move the selection, Left
//!   and Right fold and unfold it, Enter reveals it in the transcript. A click
//!   on a row selects and reveals it, as a click on a lane of the session
//!   panel's card does.
//! - **Frames.** A running turn is repainted by the one loop the card shares
//!   (`trace_frame::frame_driver`): only while the shown turn runs and the tab
//!   is drawn, 30 times a second at most while a span runs and once a second
//!   while the turn only waits. Nothing here runs a gpui animation.
//! - **Narrow.** Below 560 px each bar is stacked under its name.

use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ely_gpui_component::{forms::Checkbox, primitives::IconName};
use gpui::{
    AnyElement, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, Render, ScrollStrategy,
    Styled, Subscription, UniformListScrollHandle, WeakEntity, Window,
    actions, canvas, div, prelude::FluentBuilder as _, px, relative, uniform_list,
};
use sirio_acp::SpanKind;
use sirio_theme::Theme;

use crate::chat::trace::{Axis, TraceStore, TreeRow, Turn, TurnState, axis, format_duration};
use crate::chat::{Chat, ChatEvent};
use crate::ely_ui;
use crate::trace_frame::{Pace, Pending, frame_driver};
use crate::trace_style::{dropped_note, turn_color};

mod rows;

use rows::{RowStyle, render_row};

/// Below this width a bar is stacked under its name.
const STACK_BELOW: f32 = 560.0;

actions!(
    trace_tab,
    [SelectPrevious, SelectNext, FoldRow, UnfoldRow, RevealRow]
);

/// Binds the tab's keys, in its own `TraceTab` context so they reach neither a
/// terminal nor a chat, and leave Sirio's `ctrl-alt-up` and `ctrl-alt-down`
/// pane chords alone. The app crate calls through this module.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, Some("TraceTab")),
        KeyBinding::new("down", SelectNext, Some("TraceTab")),
        KeyBinding::new("left", FoldRow, Some("TraceTab")),
        KeyBinding::new("right", UnfoldRow, Some("TraceTab")),
        KeyBinding::new("enter", RevealRow, Some("TraceTab")),
        KeyBinding::new("return", RevealRow, Some("TraceTab")),
    ]);
}

pub enum TraceTabEvent {
    /// The chat the tab shows is gone: the workspace closes the tab.
    Close,
}

/// What `surface.trace.read` reports.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceTabSnapshot {
    /// The shown turn's 1-based index in the chat; 0 with no turn yet.
    pub turn: usize,
    /// The latest turn's index; 0 with no turn yet.
    pub turns: usize,
    pub follow: bool,
    /// The spans past the cap the shown turn did not keep; 0 with none.
    pub dropped: usize,
    /// The rows on screen, folds applied. Each has `id`, `parent` (empty for
    /// the root), `kind` (`root`, `subagent`, `llm` or `tool`), `state`,
    /// `ended` (`0` or `1`), `depth` and `folded` (`0` or `1`).
    pub spans: Vec<BTreeMap<String, String>>,
}

pub struct TraceTab {
    chat: WeakEntity<Chat>,
    chat_id: EntityId,
    focus: FocusHandle,
    /// The shown turn's index in the chat; 0 with no turn yet.
    turn: usize,
    follow: bool,
    /// The spans folded away under themselves, for the shown turn.
    folded: HashSet<String>,
    selected: Option<String>,
    scroll: UniformListScrollHandle,
    /// Shared with the frame loop's timer task, which clears it.
    frame_pending: Pending,
    /// The frames the loop has asked for.
    frames: Rc<Cell<u32>>,
    /// The tab's width as last painted.
    width: Rc<Cell<Option<Pixels>>>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<TraceTabEvent> for TraceTab {}

impl Focusable for TraceTab {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// What a draw of the shown turn needs, read from the store at one instant.
struct Shown {
    index: usize,
    first: usize,
    last: usize,
    title: String,
    summary: String,
    state: TurnState,
    /// Whether some span runs: what decides how often the tab is painted.
    moving: bool,
    /// The spans past the cap the turn did not keep.
    dropped: usize,
    rows: Vec<TreeRow>,
    axis: Axis,
    /// How far into the turn `now` is: the "now" line, and where a running
    /// span's bar ends.
    elapsed: Duration,
    slowest: Option<String>,
}

impl TraceTab {
    pub fn new(chat: Entity<Chat>, cx: &mut Context<Self>) -> Self {
        let subscriptions = [
            cx.subscribe(&chat, |tab, _, event: &ChatEvent, cx| {
                if matches!(event, ChatEvent::TraceChanged) {
                    tab.sync(cx);
                    cx.notify();
                }
            }),
            cx.observe_release(&chat, |_, _, cx| cx.emit(TraceTabEvent::Close)),
        ];
        let mut tab = Self {
            chat_id: chat.entity_id(),
            chat: chat.downgrade(),
            focus: cx.focus_handle(),
            turn: 0,
            follow: true,
            folded: HashSet::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            frame_pending: Rc::default(),
            frames: Rc::default(),
            width: Rc::default(),
            _subscriptions: subscriptions,
        };
        tab.sync(cx);
        tab
    }

    /// The chat this tab shows.
    pub fn chat_id(&self) -> EntityId {
        self.chat_id
    }

    /// What `surface.trace.read` reports: the turn, whether it is followed,
    /// and each row on screen.
    pub fn snapshot(&self, cx: &App) -> TraceTabSnapshot {
        let rows = self.visible_rows(cx, Instant::now());
        let turns = self
            .read(cx, |store| store.latest().map_or(0, Turn::index))
            .unwrap_or(0);
        TraceTabSnapshot {
            turn: self.turn,
            turns,
            follow: self.follow,
            dropped: self
                .read(cx, |store| store.turn(self.turn).map_or(0, Turn::dropped_spans))
                .unwrap_or(0),
            spans: encode(&rows, &self.folded),
        }
    }

    fn read<R>(&self, cx: &App, read: impl FnOnce(&TraceStore) -> R) -> Option<R> {
        let chat = self.chat.upgrade()?;
        Some(read(chat.read(cx).trace()))
    }

    /// The first and last turn the chat keeps. The outer `None` is a chat
    /// that is gone; the inner one a chat with no turn.
    fn kept_by_chat(&self, cx: &App) -> Option<Option<(usize, usize)>> {
        self.read(cx, |store| {
            (!store.is_empty()).then(|| {
                let kept = store.turn_indices();
                (*kept.start(), *kept.end())
            })
        })
    }

    fn kept(&self, cx: &App) -> Option<(usize, usize)> {
        self.kept_by_chat(cx).flatten()
    }

    /// Brings the shown turn in step with the chat's: the latest while
    /// following; else the one the reader chose, or the oldest the chat still
    /// keeps once that is gone. A chat with no turn, or a trace that began
    /// again below the shown turn, starts over with following on.
    fn sync(&mut self, cx: &App) {
        // A chat that is gone is left as it was: the tab is closing.
        let Some(kept) = self.kept_by_chat(cx) else {
            return;
        };
        let Some((first, last)) = kept else {
            self.follow = true;
            self.show(0);
            return;
        };
        if self.turn > last {
            self.follow = true;
        }
        let wanted = if self.follow {
            last
        } else {
            self.turn.clamp(first, last)
        };
        self.show(wanted);
    }

    /// Shows turn `turn`; folds and the selection belong to a turn, so a
    /// different one starts with neither.
    fn show(&mut self, turn: usize) {
        if turn != self.turn {
            self.turn = turn;
            self.folded.clear();
            self.selected = None;
        }
    }

    fn step_back(&mut self, cx: &mut Context<Self>) {
        let Some((first, _)) = self.kept(cx) else {
            return;
        };
        if self.turn <= first {
            return;
        }
        self.follow = false;
        self.show(self.turn - 1);
        cx.notify();
    }

    fn step_forward(&mut self, cx: &mut Context<Self>) {
        let Some((_, last)) = self.kept(cx) else {
            return;
        };
        if self.turn >= last {
            return;
        }
        self.show(self.turn + 1);
        cx.notify();
    }

    fn set_follow(&mut self, follow: bool, cx: &mut Context<Self>) {
        self.follow = follow;
        self.sync(cx);
        cx.notify();
    }

    /// The rows the shown turn draws, folds applied.
    fn visible_rows(&self, cx: &App, now: Instant) -> Vec<TreeRow> {
        self.read(cx, |store| {
            store
                .turn(self.turn)
                .map(|turn| turn.rows(&self.folded, now))
        })
        .flatten()
        .unwrap_or_default()
    }

    fn shown(&self, cx: &App, now: Instant) -> Option<Shown> {
        self.read(cx, |store| {
            let kept = store.turn_indices();
            let turn = store.turn(self.turn)?;
            Some(Shown {
                index: turn.index(),
                first: *kept.start(),
                last: *kept.end(),
                title: turn.title().to_string(),
                summary: turn.summary(now),
                state: turn.state(),
                moving: turn.is_moving(),
                dropped: turn.dropped_spans(),
                rows: turn.rows(&self.folded, now),
                axis: axis(turn.extent(now)),
                elapsed: turn.elapsed(now),
                slowest: turn.slowest_leaf().map(str::to_string),
            })
        })
        .flatten()
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(false, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(true, cx);
    }

    /// Up and Down. With nothing selected the first Down takes the first row
    /// and the first Up the last. The transcript is not touched.
    fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx, Instant::now());
        if rows.is_empty() {
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|span| rows.iter().position(|row| row.span == *span));
        let next = match (current, down) {
            (None, true) => 0,
            (None, false) => rows.len() - 1,
            (Some(at), true) => (at + 1).min(rows.len() - 1),
            (Some(at), false) => at.saturating_sub(1),
        };
        self.selected = Some(rows[next].span.clone());
        self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn fold_row(&mut self, _: &FoldRow, _: &mut Window, cx: &mut Context<Self>) {
        let Some(span) = self.selected.clone() else {
            return;
        };
        self.set_folded(&span, true, cx);
    }

    fn unfold_row(&mut self, _: &UnfoldRow, _: &mut Window, cx: &mut Context<Self>) {
        let Some(span) = self.selected.clone() else {
            return;
        };
        self.set_folded(&span, false, cx);
    }

    /// Folds `span` when it has rows to hide, or opens it.
    fn set_folded(&mut self, span: &str, folded: bool, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx, Instant::now());
        let Some(at) = rows.iter().position(|row| row.span == span) else {
            return;
        };
        let foldable = has_children(&rows, at);
        let changed = if folded {
            foldable && self.folded.insert(span.to_string())
        } else {
            self.folded.remove(span)
        };
        if changed {
            // A fold that hides the selected row leaves the selection on the
            // row that folded, which is on screen; Up and Down go on from it.
            if folded {
                let rows = self.visible_rows(cx, Instant::now());
                let hidden = self
                    .selected
                    .as_ref()
                    .is_some_and(|selected| !rows.iter().any(|row| row.span == *selected));
                if hidden {
                    self.selected = Some(span.to_string());
                }
            }
            cx.notify();
        }
    }

    fn reveal_row(&mut self, _: &RevealRow, _: &mut Window, cx: &mut Context<Self>) {
        self.reveal_selected(cx);
    }

    /// Scrolls the chat's transcript to what the selected span belongs to.
    fn reveal_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(chat), Some(span)) = (self.chat.upgrade(), self.selected.clone()) else {
            return;
        };
        let turn = self.turn;
        chat.update(cx, |chat, cx| chat.reveal_span(turn, &span, cx));
    }

    /// A click on a row: it is selected and revealed, as a lane's click is.
    fn click_row(&mut self, span: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(span);
        window.focus(&self.focus, cx);
        self.reveal_selected(cx);
        cx.notify();
    }
}

/// Whether row `at` has anything under it, drawn or folded away.
fn has_children(rows: &[TreeRow], at: usize) -> bool {
    rows[at].hidden_children > 0
        || rows
            .get(at + 1)
            .is_some_and(|next| next.depth > rows[at].depth)
}

fn kind_word(kind: &Option<SpanKind>) -> &'static str {
    match kind {
        None => "root",
        Some(SpanKind::Subagent) => "subagent",
        Some(SpanKind::Llm) => "llm",
        Some(SpanKind::Tool) => "tool",
    }
}

/// The rows as `surface.trace.read` gives them. A row's parent is the nearest
/// row above it one level shallower.
fn encode(rows: &[TreeRow], folded: &HashSet<String>) -> Vec<BTreeMap<String, String>> {
    let mut above: Vec<(u8, &str)> = Vec::new();
    rows.iter()
        .map(|row| {
            while above.last().is_some_and(|(depth, _)| *depth >= row.depth) {
                above.pop();
            }
            let parent = above.last().map_or("", |(_, span)| *span);
            above.push((row.depth, &row.span));
            let flag = |on: bool| if on { "1" } else { "0" };
            BTreeMap::from(
                [
                    ("id", row.span.as_str()),
                    ("parent", parent),
                    ("kind", kind_word(&row.kind)),
                    ("state", row.state.word()),
                    ("ended", flag(row.end.is_some())),
                    ("depth", row.depth.to_string().as_str()),
                    ("folded", flag(folded.contains(&row.span))),
                ]
                .map(|(key, value)| (key.to_string(), value.to_string())),
            )
        })
        .collect()
}

impl TraceTab {
    /// The canvas that tells the tab how wide it is, so that it can stack.
    /// It redraws the tab only when the width crosses the line, never for a
    /// move within one side of it.
    fn width_probe(&self, cx: &mut Context<Self>) -> AnyElement {
        let width = self.width.clone();
        let entity = cx.entity();
        canvas(
            move |bounds, _, cx| {
                let now = bounds.size.width;
                let before = width.replace(Some(now));
                // Until it has been measured the tab is drawn wide.
                let was_stacked = before.is_some_and(|before| before < px(STACK_BELOW));
                if was_stacked != (now < px(STACK_BELOW)) {
                    let entity = entity.clone();
                    cx.defer(move |cx| entity.update(cx, |_, cx| cx.notify()));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn render_header(&self, shown: &Shown, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        let tone = turn_color(theme, shown.state);
        let back = entity.clone();
        let forward = entity.clone();
        let follow = entity.clone();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .child(ely_ui::icon_button(
                "trace-tab-previous",
                IconName::ChevronLeft,
                "Previous turn",
                shown.index > shown.first,
                move |_, cx| back.update(cx, |tab, cx| tab.step_back(cx)),
            ))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Turn {} of {}", shown.index, shown.last)),
            )
            .child(ely_ui::icon_button(
                "trace-tab-next",
                IconName::ChevronRight,
                "Next turn",
                shown.index < shown.last,
                move |_, cx| forward.update(cx, |tab, cx| tab.step_forward(cx)),
            ))
            .child(
                div()
                    .debug_selector(|| "trace-tab-summary".into())
                    .flex_none()
                    .text_color(tone)
                    .child(shown.summary.clone()),
            )
            .children((shown.dropped > 0).then(|| {
                div()
                    .debug_selector(|| "trace-tab-dropped".into())
                    .flex_none()
                    .text_color(theme.ely.fg_subtle)
                    .child(dropped_note(shown.dropped))
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(80.0))
                    .truncate()
                    .text_color(theme.ely.fg_muted)
                    .child(shown.title.clone()),
            )
            .child(
                div()
                    .id("trace-tab-follow-box")
                    .debug_selector(|| "trace-tab-follow".into())
                    .flex_none()
                    .child(
                        Checkbox::new("trace-tab-follow-check", self.follow)
                            .label("Follow the latest turn")
                            .on_change(move |on, _, cx| {
                                follow.update(cx, |tab, cx| tab.set_follow(on, cx));
                            }),
                    ),
            )
            .into_any_element()
    }

    /// The axis's tick labels, over the bars' column.
    fn render_axis(&self, shown: &Shown, style: &RowStyle) -> AnyElement {
        let theme = &style.theme;
        let last = shown.axis.ticks.len().saturating_sub(1);
        let ticks = div()
            .relative()
            .h_full()
            .when(!style.stacked, |ticks| ticks.flex_1())
            .when(style.stacked, |ticks| ticks.w_full())
            .children(shown.axis.ticks.iter().enumerate().map(|(at, tick)| {
                let label = div()
                    .absolute()
                    .top(px(5.0))
                    .text_size(px(10.0))
                    .text_color(theme.ely.fg_subtle)
                    .whitespace_nowrap()
                    .child(format_duration(*tick));
                if at == last && at > 0 {
                    label.right_0()
                } else {
                    label.left(relative(style.across(*tick)))
                }
            }));
        div()
            .flex()
            .flex_none()
            .h(px(24.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .when(!style.stacked, |axis| axis.child(style.name_column()))
            .child(ticks)
            .into_any_element()
    }
}

impl Render for TraceTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("trace.tab.render", cx.entity_id().as_u64());
        crate::ely::sync_theme_if_changed(cx);
        let theme = Theme::get(cx).clone();
        let entity = cx.entity();
        let now = Instant::now();
        let root = div()
            .id("trace-tab")
            .debug_selector(|| "trace-tab".into())
            .key_context("TraceTab")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::fold_row))
            .on_action(cx.listener(Self::unfold_row))
            .on_action(cx.listener(Self::reveal_row))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.ely.bg)
            .text_color(theme.ely.fg)
            .text_size(theme.typography.footnote)
            .child(self.width_probe(cx));
        let Some(shown) = self.shown(cx, now) else {
            return root.child(
                div()
                    .debug_selector(|| "trace-tab-empty".into())
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .text_color(theme.ely.fg_subtle)
                    .child("No turn yet. The agents of the chat's next turn show here as they work."),
            );
        };
        let running = matches!(shown.state, TurnState::Live | TurnState::Waiting);
        let moving = shown.moving;
        let stacked = self.width.get().is_some_and(|width| width < px(STACK_BELOW));
        // Until the tab has been measured it is drawn wide.
        let track = RowStyle::track_width(self.width.get().map_or(1000.0, f32::from), stacked);
        let style = RowStyle {
            theme: theme.clone(),
            range: shown.axis.range,
            elapsed: shown.elapsed,
            running,
            stacked,
            slowest: shown.slowest.clone(),
            selected: self.selected.clone(),
            track,
            entity: entity.clone(),
        };
        let header = self.render_header(&shown, &theme, &entity);
        let axis_row = self.render_axis(&shown, &style);
        let count = shown.rows.len();
        let rows = Rc::new(shown.rows);
        let list = uniform_list("trace-rows", count, move |range, _, _| {
            range
                .map(|at| render_row(&rows, at, &style))
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.scroll)
        .size_full();
        let handle = self.scroll.0.borrow().base_handle.clone();
        root.child(header)
            .child(axis_row)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(list)
                    .child(crate::file_view::scrollbar("trace-tab-bar", &handle)),
            )
            .child(frame_driver(
                self.frame_pending.clone(),
                self.frames.clone(),
                Pace::of(running, moving, cx.reduce_motion()),
                cx.entity_id().as_u64(),
            ))
    }
}

#[cfg(test)]
mod tests;

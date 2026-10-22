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
//!   is drawn, 30 times a second at most. Nothing here runs a gpui animation.
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
use crate::trace_frame::frame_driver;

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
    frame_pending: Rc<Cell<bool>>,
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
        let tone = match shown.state {
            TurnState::Live => theme.ely.info,
            TurnState::Waiting => theme.ely.warning,
            TurnState::Done => theme.ely.success,
            TurnState::Failed => theme.ely.danger,
            TurnState::Interrupted => theme.ely.fg_muted,
        };
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
        let stacked = self.width.get().is_some_and(|width| width < px(STACK_BELOW));
        let style = RowStyle {
            theme: theme.clone(),
            range: shown.axis.range,
            elapsed: shown.elapsed,
            running,
            stacked,
            slowest: shown.slowest.clone(),
            selected: self.selected.clone(),
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
        root.child(header)
            .child(axis_row)
            .child(div().flex_1().min_h(px(0.0)).child(list))
            .child(frame_driver(
                self.frame_pending.clone(),
                self.frames.clone(),
                running,
                cx.reduce_motion(),
                cx.entity_id().as_u64(),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::rows::tooltip_lines;
    use super::*;
    use crate::chat::testing;
    use crate::chat::trace::SpanState;
    use gpui::{
        AppContext as _, Modifiers, ParentElement, Styled, TestAppContext, VisualTestContext, px,
        size,
    };
    use sirio_acp::{SpanOutcome, TokenCount, TraceEvent, TurnOutcome};
    use std::cell::Cell;
    use std::rc::Rc;

    /// A chat that never connects, with the tab beside it: the chat is drawn
    /// only when a test needs its transcript laid out, the tab only while
    /// `show_tab`, which stands in for the tab being hidden.
    struct Host {
        chat: Option<Entity<Chat>>,
        tab: Entity<TraceTab>,
        show_chat: bool,
        show_tab: bool,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .children(
                    self.chat
                        .clone()
                        .filter(|_| self.show_chat)
                        .map(|chat| div().w(px(420.0)).h_full().child(chat)),
                )
                .children(
                    self.show_tab
                        .then(|| div().flex_1().h_full().child(self.tab.clone())),
                )
        }
    }

    fn host(cx: &mut TestAppContext, show_chat: bool) -> (Entity<Host>, &mut VisualTestContext) {
        testing::prepare(cx);
        cx.update(init);
        let (host, cx) = cx.add_window_view(move |_, cx| {
            let chat = cx.new(testing::offline_chat);
            let tab = cx.new(|cx| TraceTab::new(chat.clone(), cx));
            Host {
                chat: Some(chat),
                tab,
                show_chat,
                show_tab: true,
            }
        });
        cx.simulate_resize(size(px(1100.0), px(500.0)));
        (host, cx)
    }

    fn chat_of(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<Chat> {
        host.read_with(cx, |host, _| host.chat.clone().expect("the chat is alive"))
    }

    fn tab_of(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<TraceTab> {
        host.read_with(cx, |host, _| host.tab.clone())
    }

    fn feed(host: &Entity<Host>, cx: &mut VisualTestContext, event: TraceEvent) {
        let chat = chat_of(host, cx);
        chat.update(cx, |chat, cx| testing::feed(chat, event, cx));
    }

    /// The turn shown, the latest, and whether the tab follows it.
    fn shown(host: &Entity<Host>, cx: &mut VisualTestContext) -> (usize, usize, bool) {
        let tab = tab_of(host, cx);
        let snapshot = tab.read_with(cx, |tab, cx| tab.snapshot(cx));
        (snapshot.turn, snapshot.turns, snapshot.follow)
    }

    fn span_ids(host: &Entity<Host>, cx: &mut VisualTestContext) -> Vec<String> {
        let tab = tab_of(host, cx);
        let snapshot = tab.read_with(cx, |tab, cx| tab.snapshot(cx));
        snapshot.spans.iter().map(|span| span["id"].clone()).collect()
    }

    fn refresh(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    }

    /// A real click at the middle of a drawn element.
    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
        refresh(cx);
    }

    fn selector(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    fn focus_tab(host: &Entity<Host>, cx: &mut VisualTestContext) {
        let tab = tab_of(host, cx);
        cx.update(|window, cx| {
            let focus = tab.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        });
        cx.run_until_parked();
    }

    fn secs(base: Instant, seconds: f64) -> Instant {
        base + Duration::from_secs_f64(seconds)
    }

    fn started(at: Instant) -> TraceEvent {
        TraceEvent::TurnStarted { at }
    }

    fn finished(at: Instant) -> TraceEvent {
        TraceEvent::TurnEnded {
            at,
            outcome: TurnOutcome::Done,
            measured_ms: None,
            cost_usd: None,
            tokens: None,
        }
    }

    fn span(id: &str, parent: Option<&str>, kind: SpanKind, name: &str, at: Instant) -> TraceEvent {
        TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(str::to_string),
            kind,
            name: name.into(),
            detail: String::new(),
            model: None,
            at,
        }
    }

    fn span_ended(id: &str, outcome: SpanOutcome, at: Instant) -> TraceEvent {
        TraceEvent::SpanEnded {
            id: id.into(),
            outcome,
            at,
            measured_ms: None,
        }
    }

    fn done(id: &str, at: Instant) -> TraceEvent {
        span_ended(id, SpanOutcome::Done, at)
    }

    /// The turn the key and click tests read: a model call, a subagent that
    /// read a file, and an edit, all over.
    fn an_audit(host: &Entity<Host>, cx: &mut VisualTestContext, base: Instant) -> usize {
        let chat = chat_of(host, cx);
        chat.update(cx, |chat, cx| {
            testing::submit(chat, "audit the crate", cx);
            let events = [
                started(base),
                span("msg_1", None, SpanKind::Llm, "claude", secs(base, 0.5)),
                done("msg_1", secs(base, 2.0)),
                span("toolu_agent", None, SpanKind::Subagent, "Explore", secs(base, 2.0)),
                span("toolu_read", Some("toolu_agent"), SpanKind::Tool, "Read", secs(base, 3.0)),
                done("toolu_read", secs(base, 5.0)),
                done("toolu_agent", secs(base, 8.0)),
                span("toolu_edit", None, SpanKind::Tool, "Edit", secs(base, 8.0)),
                done("toolu_edit", secs(base, 10.0)),
                finished(secs(base, 12.0)),
            ];
            for event in events {
                testing::feed(chat, event, cx);
            }
            testing::answers(chat, 10);
            let agent = testing::agent_row(chat, "toolu_agent", "Explore");
            testing::answers(chat, 40);
            cx.notify();
            agent
        })
    }

    fn top_entry(host: &Entity<Host>, cx: &mut VisualTestContext) -> usize {
        let chat = chat_of(host, cx);
        chat.read_with(cx, |chat, _| chat.control_snapshot().first_visible_entry)
    }

    #[gpui::test]
    async fn the_tab_follows_the_latest_turn_until_you_go_back(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(100);
        for turn in 0..2 {
            let at = secs(base, f64::from(turn) * 10.0);
            feed(&host, cx, started(at));
            feed(&host, cx, finished(secs(at, 4.0)));
        }
        feed(&host, cx, started(secs(base, 20.0)));
        refresh(cx);
        assert_eq!(shown(&host, cx), (3, 3, true), "a tab on a chat's third turn shows it");

        click(cx, "trace-tab-previous");
        assert_eq!(shown(&host, cx), (2, 3, false), "going back turns following off");

        feed(&host, cx, finished(secs(base, 24.0)));
        feed(&host, cx, started(secs(base, 30.0)));
        refresh(cx);
        assert_eq!(
            shown(&host, cx),
            (2, 4, false),
            "a turn that opens while an old one is read leaves the reader there"
        );

        click(cx, "trace-tab-follow");
        assert_eq!(shown(&host, cx), (4, 4, true), "following again jumps to the latest");
    }

    #[gpui::test]
    async fn the_oldest_turn_the_chat_keeps_is_as_far_back_as_the_tab_goes(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(1000);
        let turn = |n: u32| secs(base, f64::from(n) * 10.0);
        for n in 0..22 {
            feed(&host, cx, started(turn(n)));
            feed(&host, cx, finished(secs(turn(n), 4.0)));
        }
        refresh(cx);
        // The chat keeps the last twenty of its twenty-two turns.
        assert_eq!(shown(&host, cx), (22, 22, true));
        let tab = tab_of(&host, cx);
        for _ in 0..30 {
            tab.update(cx, |tab, cx| tab.step_back(cx));
        }
        assert_eq!(shown(&host, cx), (3, 22, false), "turns 1 and 2 are gone, and so is the way to them");

        // A new turn pushes the one the reader is on out of the chat: the
        // oldest that is left is shown.
        feed(&host, cx, started(turn(22)));
        refresh(cx);
        assert_eq!(shown(&host, cx), (4, 23, false));
    }

    #[gpui::test]
    async fn a_trace_that_begins_again_turns_following_back_on(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        cx.cx.executor().allow_parking();
        let base = Instant::now() - Duration::from_secs(100);
        for n in 0..3 {
            feed(&host, cx, started(secs(base, f64::from(n) * 10.0)));
            feed(&host, cx, finished(secs(base, f64::from(n) * 10.0 + 4.0)));
        }
        click(cx, "trace-tab-previous");
        assert_eq!(shown(&host, cx), (2, 3, false));

        // A new conversation drops the agent that ran these turns, and its
        // trace with it.
        let chat = chat_of(&host, cx);
        chat.update(cx, |chat, cx| testing::new_conversation(chat, cx));
        refresh(cx);
        assert_eq!(shown(&host, cx), (0, 0, true), "nothing to show, and following again");

        feed(&host, cx, started(secs(base, 60.0)));
        refresh(cx);
        assert_eq!(
            shown(&host, cx),
            (1, 1, true),
            "the first turn of the new trace is followed, not left behind at turn 2"
        );
    }

    #[gpui::test]
    async fn the_keys_move_fold_and_reveal(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, true);
        cx.simulate_resize(size(px(1100.0), px(300.0)));
        let base = Instant::now() - Duration::from_secs(60);
        let agent = an_audit(&host, cx, base);
        refresh(cx);
        assert!(top_entry(&host, cx) > agent, "the transcript's tail is far below the agent's row");
        focus_tab(&host, cx);

        // No selection yet: the first Down takes the first row.
        cx.simulate_keystrokes("down down down");
        refresh(cx);
        assert_eq!(
            tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()).as_deref(),
            Some("toolu_agent"),
            "root, the model call, then the subagent"
        );
        assert!(
            top_entry(&host, cx) > agent,
            "moving the selection does not scroll the transcript"
        );

        cx.simulate_keystrokes("left");
        refresh(cx);
        assert_eq!(
            span_ids(&host, cx),
            ["root", "msg_1", "toolu_agent", "toolu_edit"],
            "Left folds the subagent: the read inside it is gone"
        );
        let bar = cx.debug_bounds("trace-bar-toolu_agent").expect("the folded parent's bar is drawn");
        let mark = cx
            .debug_bounds("trace-mark-toolu_agent-0")
            .expect("the folded parent shows where its hidden child began");
        assert!(
            mark.left() >= bar.left() && mark.right() <= bar.right(),
            "the mark {mark:?} sits on the parent's bar {bar:?}"
        );

        cx.simulate_keystrokes("right");
        refresh(cx);
        assert_eq!(
            span_ids(&host, cx),
            ["root", "msg_1", "toolu_agent", "toolu_read", "toolu_edit"],
            "Right unfolds it"
        );
        assert!(cx.debug_bounds("trace-mark-toolu_agent-0").is_none(), "an open parent shows no marks");

        cx.simulate_keystrokes("down");
        refresh(cx);
        assert!(
            top_entry(&host, cx) > agent,
            "Down onto the read did not reveal it either"
        );
        cx.simulate_keystrokes("enter");
        refresh(cx);
        assert_eq!(
            top_entry(&host, cx),
            agent,
            "Enter reveals the Agent row a span inside a subagent belongs to"
        );
    }

    #[gpui::test]
    async fn a_click_on_a_row_selects_it_and_reveals_it(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, true);
        cx.simulate_resize(size(px(1100.0), px(300.0)));
        let agent = an_audit(&host, cx, Instant::now() - Duration::from_secs(60));
        refresh(cx);
        assert!(top_entry(&host, cx) > agent);

        click(cx, "trace-row-toolu_agent");
        assert_eq!(
            tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()).as_deref(),
            Some("toolu_agent")
        );
        assert_eq!(top_entry(&host, cx), agent, "a row reveals what a lane would");
    }

    #[gpui::test]
    async fn a_click_on_a_chevron_folds_without_selecting_or_revealing(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, true);
        cx.simulate_resize(size(px(1100.0), px(300.0)));
        let agent = an_audit(&host, cx, Instant::now() - Duration::from_secs(60));
        refresh(cx);
        assert!(top_entry(&host, cx) > agent);

        click(cx, "trace-chevron-toolu_agent");
        assert_eq!(
            span_ids(&host, cx),
            ["root", "msg_1", "toolu_agent", "toolu_edit"],
            "the chevron folds the subagent"
        );
        assert_eq!(
            tab_of(&host, cx).read_with(cx, |tab, _| tab.selected.clone()),
            None,
            "the row's own click did not also select it"
        );
        assert!(top_entry(&host, cx) > agent, "nor reveal it");

        click(cx, "trace-chevron-toolu_agent");
        assert_eq!(span_ids(&host, cx).len(), 5, "the chevron opens it again");
    }

    fn row(span: &str, kind: Option<SpanKind>, state: SpanState, start: f64, end: Option<f64>) -> TreeRow {
        TreeRow {
            span: span.into(),
            depth: 1,
            kind,
            name: span.into(),
            detail: String::new(),
            state,
            start: Duration::from_secs_f64(start),
            end: end.map(Duration::from_secs_f64),
            hidden_children: 0,
            child_marks: Vec::new(),
            note: None,
            model: None,
            tokens: None,
            progress_tokens: None,
            tool_uses: None,
            waits: Vec::new(),
        }
    }

    #[test]
    fn the_tooltip_names_kind_name_start_duration_model_tokens_and_status() {
        let lines = |row: &TreeRow| -> Vec<(&'static str, String)> {
            tooltip_lines(row, Duration::from_secs(30))
        };
        let pair = |label: &'static str, value: &str| (label, value.to_string());

        let mut edit = row("Edit", Some(SpanKind::Tool), SpanState::Done, 2.5, Some(6.6));
        edit.name = "Edit".into();
        assert_eq!(
            lines(&edit),
            vec![
                pair("Kind", "Tool"),
                pair("Name", "Edit"),
                pair("Start", "+2.5s"),
                pair("Duration", "4.1s"),
                pair("Status", "Done"),
            ]
        );

        let mut call = row("msg_1", Some(SpanKind::Llm), SpanState::Done, 0.5, Some(0.84));
        call.model = Some("opus".into());
        call.tokens = Some(TokenCount { input: 1_240, output: 340 });
        assert_eq!(
            lines(&call),
            vec![
                pair("Kind", "Model call"),
                pair("Name", "msg_1"),
                pair("Start", "+500ms"),
                pair("Duration", "340ms"),
                pair("Model", "opus"),
                pair("Tokens", "1.2k in · 340 out"),
                pair("Status", "Done"),
            ],
            "a call says its model and what it was given and wrote"
        );

        let mut failed = row("toolu_bash", Some(SpanKind::Tool), SpanState::Failed, 70.0, Some(71.0));
        failed.note = Some("exit status 1".into());
        let failed = lines(&failed);
        assert_eq!(&failed[2..], &[pair("Start", "+1m 10s"), pair("Duration", "1.0s"), pair("Status", "Failed"), pair("Error", "exit status 1")]);

        // A span that is still running has taken as long as the turn has gone
        // on since it began; a subagent says what it has used so far.
        let mut agent = row("toolu_agent", Some(SpanKind::Subagent), SpanState::Running, 10.0, None);
        agent.progress_tokens = Some(48_200);
        let agent = lines(&agent);
        assert_eq!(agent[0], pair("Kind", "Subagent"));
        assert_eq!(agent[3], pair("Duration", "20.0s"));
        assert_eq!(agent[4], pair("Tokens", "48.2k so far"));
        assert_eq!(agent[5], pair("Status", "Running"));

        let root = lines(&row("root", None, SpanState::Done, 0.0, Some(12.0)));
        assert_eq!(root[0], pair("Kind", "Turn"));
    }

    #[gpui::test]
    async fn the_spans_say_who_their_parent_is_and_whether_they_ended(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(60);
        for event in [
            started(base),
            span("msg_1", None, SpanKind::Llm, "claude", secs(base, 0.5)),
            done("msg_1", secs(base, 2.0)),
            span("toolu_agent", None, SpanKind::Subagent, "Explore", secs(base, 2.0)),
            span("toolu_read", Some("toolu_agent"), SpanKind::Tool, "Read", secs(base, 3.0)),
            done("toolu_read", secs(base, 5.0)),
            done("toolu_agent", secs(base, 8.0)),
            span("toolu_edit", None, SpanKind::Tool, "Edit", secs(base, 8.0)),
        ] {
            feed(&host, cx, event);
        }
        let tab = tab_of(&host, cx);
        let spans = tab.read_with(cx, |tab, cx| tab.snapshot(cx)).spans;
        let row = |id: &str, parent: &str, kind: &str, state: &str, ended: &str, depth: &str| {
            BTreeMap::from(
                [
                    ("id", id),
                    ("parent", parent),
                    ("kind", kind),
                    ("state", state),
                    ("ended", ended),
                    ("depth", depth),
                    ("folded", "0"),
                ]
                .map(|(key, value)| (key.to_string(), value.to_string())),
            )
        };
        assert_eq!(
            spans,
            vec![
                row("root", "", "root", "running", "0", "0"),
                row("msg_1", "root", "llm", "done", "1", "1"),
                row("toolu_agent", "root", "subagent", "done", "1", "1"),
                row("toolu_read", "toolu_agent", "tool", "done", "1", "2"),
                row("toolu_edit", "root", "tool", "running", "0", "1"),
            ]
        );
    }

    #[gpui::test]
    async fn the_tab_moves_only_while_its_turn_runs(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let frames = |host: &Entity<Host>, cx: &mut VisualTestContext| {
            tab_of(host, cx).read_with(cx, |tab, _| tab.frames.get())
        };
        let next_frame = |cx: &mut VisualTestContext| {
            cx.cx.executor().advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            refresh(cx);
        };
        let base = Instant::now() - Duration::from_secs(5);
        feed(&host, cx, started(base));
        feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
        refresh(cx);
        let first = frames(&host, cx);
        assert!(first > 0, "a drawn tab of a running turn animates");
        next_frame(cx);
        assert!(frames(&host, cx) > first, "and goes on animating");

        // The tab goes out of sight: nothing prepaints, so nothing asks.
        host.update(cx, |host, cx| {
            host.show_tab = false;
            cx.notify();
        });
        refresh(cx);
        let hidden = frames(&host, cx);
        for _ in 0..3 {
            next_frame(cx);
        }
        assert_eq!(frames(&host, cx), hidden, "a tab nobody draws asks for no frame");

        host.update(cx, |host, cx| {
            host.show_tab = true;
            cx.notify();
        });
        refresh(cx);
        next_frame(cx);
        assert!(frames(&host, cx) > hidden, "drawn again, it animates again");

        feed(&host, cx, done("toolu_run", Instant::now()));
        feed(&host, cx, finished(Instant::now()));
        refresh(cx);
        let finished = frames(&host, cx);
        for _ in 0..3 {
            next_frame(cx);
        }
        assert_eq!(frames(&host, cx), finished, "a finished turn requests nothing");
    }

    #[gpui::test]
    async fn the_tab_follows_its_shown_turn_not_the_latest_for_its_frames(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(30);
        feed(&host, cx, started(base));
        feed(&host, cx, finished(secs(base, 4.0)));
        feed(&host, cx, started(secs(base, 10.0)));
        feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 11.0)));
        refresh(cx);
        click(cx, "trace-tab-previous");
        assert_eq!(shown(&host, cx), (1, 2, false));
        let before = tab_of(&host, cx).read_with(cx, |tab, _| tab.frames.get());
        for _ in 0..3 {
            cx.cx.executor().advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            refresh(cx);
        }
        assert_eq!(
            tab_of(&host, cx).read_with(cx, |tab, _| tab.frames.get()),
            before,
            "the turn on screen is over, though the chat's latest runs"
        );
    }

    #[gpui::test]
    async fn the_tab_asks_for_no_frame_but_its_own(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(5);
        feed(&host, cx, started(base));
        feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
        refresh(cx);
        // The test window has no frame loop: the callbacks a draw asks the
        // next frame for wait until they are delivered by hand.
        cx.update(|window, cx| window.simulate_next_frame(cx));
        feed(
            &host,
            cx,
            span("toolu_agent", None, SpanKind::Subagent, "Explore", Instant::now()),
        );
        for _ in 0..6 {
            refresh(cx);
        }
        let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(
            asked, 0,
            "every frame the tab causes comes from its own capped loop"
        );
    }

    #[gpui::test]
    async fn a_tab_asks_to_close_when_its_chat_goes(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let closed = Rc::new(Cell::new(0));
        let seen = closed.clone();
        let tab = tab_of(&host, cx);
        cx.update(|_, cx| {
            cx.subscribe(&tab, move |_, event: &TraceTabEvent, _| {
                if matches!(event, TraceTabEvent::Close) {
                    seen.set(seen.get() + 1);
                }
            })
            .detach();
        });
        refresh(cx);
        assert_eq!(closed.get(), 0);
        host.update(cx, |host, _| host.chat = None);
        cx.run_until_parked();
        assert_eq!(closed.get(), 1, "a tab on a chat that is gone has nothing to show");
    }

    #[gpui::test]
    async fn a_narrow_tab_stacks_each_bar_under_its_name(cx: &mut TestAppContext) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(10);
        feed(&host, cx, started(base));
        feed(&host, cx, span("toolu_run", None, SpanKind::Tool, "Bash", secs(base, 1.0)));
        feed(&host, cx, done("toolu_run", secs(base, 4.0)));
        feed(&host, cx, finished(secs(base, 5.0)));

        cx.simulate_resize(size(px(900.0), px(400.0)));
        refresh(cx);
        let name = cx.debug_bounds("trace-row-name-toolu_run").expect("the name is drawn");
        let track = cx.debug_bounds("trace-row-track-toolu_run").expect("the bar's track is drawn");
        assert!(
            track.left() >= name.right(),
            "wide: the bars sit beside the names, {track:?} beside {name:?}"
        );

        cx.simulate_resize(size(px(500.0), px(400.0)));
        refresh(cx);
        let name = cx.debug_bounds("trace-row-name-toolu_run").expect("the name is drawn");
        let track = cx.debug_bounds("trace-row-track-toolu_run").expect("the bar's track is drawn");
        assert!(
            track.top() >= name.bottom() && track.left() <= name.left() + px(40.0),
            "narrow: the bar is under its name, {track:?} under {name:?}"
        );
    }

    #[gpui::test]
    async fn the_slowest_leaf_wears_a_tag_a_failure_its_error_and_an_interrupted_bar_an_open_end(
        cx: &mut TestAppContext,
    ) {
        let (host, cx) = host(cx, false);
        let base = Instant::now() - Duration::from_secs(60);
        for event in [
            started(base),
            span("toolu_slow", None, SpanKind::Tool, "Bash", secs(base, 1.0)),
            done("toolu_slow", secs(base, 9.0)),
            span("toolu_fail", None, SpanKind::Tool, "Bash", secs(base, 9.0)),
            span_ended(
                "toolu_fail",
                SpanOutcome::Failed { note: "exit status 1".into() },
                secs(base, 10.0),
            ),
            span("toolu_hang", None, SpanKind::Tool, "Read", secs(base, 10.0)),
            finished(secs(base, 11.0)),
        ] {
            feed(&host, cx, event);
        }
        refresh(cx);
        let drawn = |cx: &mut VisualTestContext, id: &str| cx.debug_bounds(selector(id.to_string())).is_some();
        assert!(drawn(cx, "trace-slowest-toolu_slow"), "the slowest leaf is tagged");
        assert!(!drawn(cx, "trace-slowest-toolu_fail"), "and only that one");
        assert!(drawn(cx, "trace-note-toolu_fail"), "a failed bar carries its error");
        assert!(!drawn(cx, "trace-note-toolu_slow"));
        assert!(drawn(cx, "trace-open-end-toolu_hang"), "an interrupted bar has an open end");
        assert!(!drawn(cx, "trace-open-end-toolu_slow"), "a finished one is closed");
    }
}

//! The Trace card: what the native Claude transport reported of the chat's
//! latest turn, as a status line, six numbers and one lane per agent on a
//! shared axis. Shown from the chat's first trace event on and folded until
//! the reader opens it, like the other cards.
//!
//! The snapshot rows (`section`) and the drawing (`render_body`) read the same
//! `Turn` views, so what the control socket reports is what the card shows.
//! The card draws a running turn moving, and moves only while somebody can
//! see it:
//!
//! - **Unfolded**, the body carries a canvas that asks for the next frame
//!   from its own prepaint (`trace_frame::frame_driver`, the loop the Trace tab
//!   shares). A chat that is not painted never prepaints, so a hidden chat asks
//!   for nothing, and a turn that has ended arms nothing.
//!   One delayed frame is pending at a time, which keeps it at 30 frames a
//!   second or fewer, as gpui's own `Animation::with_max_fps` does. While
//!   the turn only waits, with no span running (`Turn::is_moving`), the delay
//!   is a second and the marks hold still: the clock and the "now" line are
//!   all that move.
//! - **Folded**, only the title's seconds move: a timer ticks once a second
//!   and tells the chat while the turn runs, the column is open and a frame
//!   painted it since the last tick. It ends with the turn.
//!
//! Everything that moves is a function of the turn's own clock, painted by
//! that one loop: the pulses, the band of the shimmering "now" text (Ely's
//! `ShimmerText::phase`, which animates nothing itself) and a lane's entry,
//! a short rise over its first 200 ms. Nothing in the card runs a gpui
//! animation or `AnimatePresence`, which would ask for frames of their own,
//! uncapped and uncounted. A turn that ends draws what is there at once: no
//! lane leaves. With gpui's reduced-motion setting the marks hold still and
//! the loop slows to a tick a second, enough for the numbers.

use super::{Row, Section};
use crate::chat::trace::{
    Axis, Lane, ROOT, Segment, SegmentKind, SpanState, TraceStore, Turn, TurnState, TurnStats,
    axis, format_duration,
};
use crate::chat::{Chat, ChatEvent};
use crate::trace_frame::{Pace, TICK, frame_driver};
use crate::trace_style::{compact_count, dropped_note, state_color, turn_color};
use ely_gpui_component::{motion::ease_out_cubic, typography::ShimmerText};
use gpui::{
    AnyElement, App, Context, CursorStyle, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, pattern_slash,
    prelude::FluentBuilder as _, px, relative,
};
use sirio_theme::Theme;
use std::time::{Duration, Instant};

/// A pulse's period, and the least it dims to.
const PULSE: Duration = Duration::from_millis(1200);
const PULSE_LOW: f32 = 0.35;
/// How far a lane steps in for each level it is nested.
const INDENT: f32 = 12.0;
/// How long a lane takes to appear, and how far it rises as it does.
const ENTER: Duration = Duration::from_millis(200);
const ENTER_RISE: f32 = 6.0;
/// One sweep of the "now" text's band of light, as Ely's own shimmer runs it.
const SHIMMER: Duration = Duration::from_millis(1800);

/// The word a turn's tag and pill say.
fn turn_word(state: TurnState) -> &'static str {
    match state {
        TurnState::Live => "live",
        TurnState::Waiting => "waiting",
        TurnState::Done => "done",
        TurnState::Failed => "failed",
        TurnState::Interrupted => "interrupted",
    }
}

/// What a lane says of itself at its end once it has finished.
fn lane_count(lane: &Lane) -> String {
    let calls = if lane.calls == 1 { "call" } else { "calls" };
    format!("{} {calls} · {}", lane.calls, format_duration(lane.duration))
}

/// The six numbers, as `(key, label, value, bad)`. An unknown cost or token
/// count is a dash: the turn's result has not said, or cannot. A turn that
/// left spans out of its trace counts only the ones it kept, so its three
/// counts read as at least: `1998+`.
fn stat_values(stats: &TurnStats) -> [(&'static str, &'static str, String, bool); 6] {
    let count = |count: usize| {
        if stats.dropped > 0 {
            format!("{count}+")
        } else {
            count.to_string()
        }
    };
    // What agents still running may add to is a floor: `≥ $3.28`.
    let mark = if stats.cost_at_least { "≥ " } else { "" };
    [
        ("duration", "Duration", format_duration(stats.duration), false),
        (
            "cost",
            "Cost",
            stats
                .cost_usd
                .map_or_else(|| "—".to_string(), |cost| format!("{mark}${cost:.2}")),
            false,
        ),
        (
            "tokens",
            "Tokens",
            stats.tokens.as_ref().map_or_else(
                || "—".to_string(),
                |tokens| format!("{mark}{}", compact_count(tokens.input + tokens.output)),
            ),
            false,
        ),
        ("llm_calls", "LLM calls", count(stats.llm_calls), false),
        ("tool_calls", "Tool calls", count(stats.tool_calls), false),
        ("errors", "Errors", count(stats.errors), stats.errors > 0),
    ]
}

/// The Trace card's snapshot: the latest turn's line, its numbers, one row
/// per lane with a row after it for each permission that lane waits on, and
/// the row that opens the tab. `None` for a chat that has no turn.
pub(super) fn section(store: &TraceStore, now: Instant) -> Option<Section> {
    let turn = store.latest()?;
    let mut rows = vec![Row {
        tag: turn_word(turn.state()).to_string(),
        ..Row::new("trace/turn", turn.title().to_string(), "")
    }];
    let stats = turn.stats(now);
    for (key, label, value, _) in stat_values(&stats) {
        rows.push(Row::new(&format!("trace/stat/{key}"), label.to_string(), &value));
    }
    if stats.dropped > 0 {
        rows.push(Row {
            muted: true,
            ..Row::new("trace/dropped", dropped_note(stats.dropped), "")
        });
    }
    for lane in turn.lanes(now) {
        rows.push(Row {
            depth: lane.depth,
            tag: lane.state.word().to_string(),
            action: Some("reveal".to_string()),
            note: lane.now.clone().unwrap_or_default(),
            ..Row::new(
                &format!("trace/lane/{}", lane.span),
                lane.name.clone(),
                &lane_count(&lane),
            )
        });
        for wait in &lane.waits {
            rows.push(Row {
                depth: lane.depth.saturating_add(1),
                action: Some("reveal".to_string()),
                ..Row::new(
                    &format!("trace/wait/{}", wait.request_id),
                    "Waiting for you".to_string(),
                    &wait.tool,
                )
            });
        }
    }
    rows.push(Row {
        action: Some("open".to_string()),
        ..Row::new("trace/open", "Open full trace".to_string(), "")
    });
    Some(Section {
        key: "trace",
        title: "Trace",
        summary: turn.summary(now),
        note: String::new(),
        bar: Vec::new(),
        rows,
        custom_body: true,
    })
}

impl Chat {
    /// Runs the action of a Trace row: a lane or a wait scrolls the transcript
    /// to its entry, the last row opens the tab. A click and the control
    /// verb both come here, so a row the snapshot lists is one this does.
    pub(in crate::chat) fn activate_trace(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let now = Instant::now();
        let Some(turn) = self.trace.latest() else {
            return Err("the chat has no trace".to_string());
        };
        if let Some(span) = key.strip_prefix("trace/lane/") {
            let index = turn.index();
            if !turn.lanes(now).iter().any(|lane| lane.span == span) {
                return Err(format!("the latest turn has no lane {span}"));
            }
            self.reveal_span(index, span, cx);
            return Ok(());
        }
        if let Some(request) = key.strip_prefix("trace/wait/") {
            let request_id: u64 = request
                .parse()
                .map_err(|_| format!("{request} is not a request id"))?;
            let waited = turn
                .lanes(now)
                .iter()
                .any(|lane| lane.waits.iter().any(|wait| wait.request_id == request_id));
            if !waited {
                return Err(format!("no lane waits on request {request_id}"));
            }
            self.reveal_wait(request_id, cx);
            return Ok(());
        }
        if key == "trace/open" {
            cx.emit(ChatEvent::OpenTrace);
            return Ok(());
        }
        Err(format!("no row with an action has key {key}"))
    }

    /// A turn started: its folded title moves once a second until it ends.
    /// Called from the event, not from a paint, so a card nobody has drawn
    /// yet is ticking by the time it is. Starting again replaces the timer
    /// of the turn before.
    pub(in crate::chat) fn start_trace_title_tick(&mut self, cx: &mut Context<Self>) {
        self.session_panel.trace_title_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this.update(cx, |chat, cx| {
                    if !chat.trace.is_running() {
                        return false;
                    }
                    let panel = &chat.session_panel;
                    // Only this timer reads the flag: the Change request's
                    // loop takes `painted`, and a second reader of it would
                    // leave each of them short.
                    let painted = panel.frame.trace_painted.replace(false);
                    // An unfolded card has its frames; a closed column has
                    // no title to move.
                    if painted && panel.open && panel.collapsed.contains("trace") {
                        cx.notify();
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        }));
    }
}

/// How bright a pulsing mark is `elapsed` into a turn: full, down to
/// `PULSE_LOW` and back, once a period. Full always under reduced motion.
fn pulse(elapsed: Duration, reduced_motion: bool) -> f32 {
    if reduced_motion {
        return 1.0;
    }
    let phase = elapsed.as_secs_f32() % PULSE.as_secs_f32() / PULSE.as_secs_f32();
    let middle = (1.0 + PULSE_LOW) / 2.0;
    middle + (1.0 - middle) * (phase * std::f32::consts::TAU).cos()
}

/// How far a lane has come in, 0 to 1, `elapsed` into the turn: it takes
/// `ENTER` from the lane's own start and eases out. Read from the turn's
/// clock, so the card's one capped loop paints it and nothing else asks for
/// frames. A turn that has ended draws every lane whole.
fn entry(running: bool, elapsed: Duration, start: Duration) -> f32 {
    if !running {
        return 1.0;
    }
    let share = elapsed.saturating_sub(start).as_secs_f32() / ENTER.as_secs_f32();
    ease_out_cubic(share.min(1.0))
}

/// What the turn's clock says at the frame being drawn.
struct Beat {
    /// The turn has not ended: its clock, its duration and the "now" line
    /// move, on the loop's tick at least.
    running: bool,
    /// Some span is running (`Turn::is_moving`): the loop is at its frames,
    /// and the marks pulse and a lane steps in. A turn that only waits holds
    /// them still, as reduced motion does, and moves on the tick alone.
    moving: bool,
    reduced: bool,
    /// How far into the turn: the "now" line's place, and the clock the
    /// pulses, the shimmer and the lanes' entries run on.
    elapsed: Duration,
}

impl Beat {
    /// How bright a pulsing mark is.
    fn pulse(&self) -> f32 {
        if self.moving {
            pulse(self.elapsed, self.reduced)
        } else {
            1.0
        }
    }

    /// Where the band of the running "now" text is, 0 to 1 across a sweep.
    fn shimmer(&self) -> f32 {
        self.elapsed.as_secs_f32() % SHIMMER.as_secs_f32() / SHIMMER.as_secs_f32()
    }
}

/// The colour and wash of the turn's status pill, and whether it pulses.
fn turn_tone(theme: &Theme, state: TurnState) -> (Hsla, Hsla, bool) {
    let wash = match state {
        TurnState::Live => theme.ely.info_subtle,
        TurnState::Waiting => theme.ely.warning_subtle,
        TurnState::Done => theme.ely.success_subtle,
        TurnState::Failed => theme.ely.danger_subtle,
        TurnState::Interrupted => theme.ely.sunken,
    };
    let pulses = matches!(state, TurnState::Live | TurnState::Waiting);
    (turn_color(theme, state), wash, pulses)
}

/// How bright the title's dot is at `now`. It pulses with the frames the open
/// card asks for, so only while the card is unfolded, and holds still while
/// the turn only waits, as the card's own marks do.
fn title_dot_alpha(turn: &Turn, folded: bool, now: Instant, reduced_motion: bool) -> f32 {
    if !folded && turn.is_moving() {
        pulse(turn.elapsed(now), reduced_motion)
    } else {
        1.0
    }
}

/// The summary beside the card's title: `Live · 34s` in the running colour
/// with a dot, plain and muted once the turn is over. The dot pulses only
/// while the card is open, because a folded card asks for no frames.
pub(super) fn title_summary(
    chat: &Chat,
    summary: &str,
    folded: bool,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let state = chat.trace.latest().map(|turn| turn.state());
    let marked = matches!(state, Some(TurnState::Live | TurnState::Waiting));
    let color = match state {
        Some(TurnState::Live) => theme.ely.info,
        Some(TurnState::Waiting) => theme.ely.warning,
        _ => theme.ely.fg_muted,
    };
    let alpha = chat.trace.latest().map_or(1.0, |turn| {
        title_dot_alpha(turn, folded, Instant::now(), cx.reduce_motion())
    });
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(color)
        .when(marked, |line| {
            line.child(
                div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(color)
                    .opacity(alpha),
            )
        })
        .child(summary.to_string())
        .into_any_element()
}

/// The card's body: the status line, the numbers, the lanes on their axis and
/// the button that opens the tab.
pub(super) fn render_body(chat: &Chat, theme: &Theme, cx: &mut Context<Chat>) -> AnyElement {
    let _perf = sirio_perf::span("trace.card.render", cx.entity_id().as_u64());
    let Some(turn) = chat.trace.latest() else {
        return div().into_any_element();
    };
    let now = Instant::now();
    let running = chat.trace.is_running();
    let reduced = cx.reduce_motion();
    let caption = theme.typography.footnote;
    let state = turn.state();
    let elapsed = turn.elapsed(now);
    let moving = turn.is_moving();
    let beat = Beat {
        running,
        moving,
        reduced,
        elapsed,
    };
    let alpha = beat.pulse();
    let lanes = turn.lanes(now);
    let axis = axis(turn.extent(now));
    let entity = cx.entity();

    // The status pill and the prompt's first line.
    let (tone, wash, pulses) = turn_tone(theme, state);
    let pill_word = match state {
        TurnState::Live => "Live",
        TurnState::Waiting => "Waiting for you",
        TurnState::Done => "Done",
        TurnState::Failed => "Failed",
        TurnState::Interrupted => "Interrupted",
    };
    let top = div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.0))
                .px(px(7.0))
                .py(px(1.0))
                .rounded_full()
                .border_1()
                .border_color(tone.opacity(0.35))
                .bg(wash)
                .text_color(tone)
                .text_size(caption)
                .child(
                    div()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(tone)
                        .opacity(if pulses { alpha } else { 1.0 }),
                )
                .child(pill_word),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(caption)
                .text_color(theme.ely.fg_muted)
                .child(turn.title().to_string()),
        );

    // The numbers, three to a line.
    let turn_stats = turn.stats(now);
    let stats = stat_values(&turn_stats);
    let stat = |(key, label, value, bad): (&str, &str, String, bool)| {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .debug_selector({
                let selector = format!("trace-stat-{key}");
                move || selector.clone()
            })
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(theme.ely.fg_subtle)
                    .child(label.to_uppercase()),
            )
            .child(
                div()
                    .truncate()
                    .text_size(caption)
                    .text_color(if bad {
                        theme.ely.danger
                    } else if value == "—" {
                        theme.ely.fg_subtle
                    } else {
                        theme.ely.fg
                    })
                    .child(value),
            )
    };
    let mut stats = stats.into_iter();
    let first: Vec<_> = stats.by_ref().take(3).map(stat).collect();
    let second: Vec<_> = stats.map(stat).collect();
    let numbers = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .py_2()
        .border_t_1()
        .border_b_1()
        .border_color(theme.ely.border)
        .child(div().flex().gap_2().children(first))
        .child(div().flex().gap_2().children(second));
    // What the cap left out, under the numbers it makes at-least counts.
    let dropped = (turn_stats.dropped > 0).then(|| {
        div()
            .debug_selector(|| "trace-dropped".into())
            .text_size(caption)
            .text_color(theme.ely.fg_subtle)
            .child(dropped_note(turn_stats.dropped))
    });

    // The axis, with the "now" dot on it, and the lanes under it. The grid
    // and the "now" line are drawn inside each lane's track, never across the
    // lane's text.
    let range = axis.range.as_secs_f32().max(0.001);
    let across = |offset: Duration| (offset.as_secs_f32() / range).clamp(0.0, 1.0);
    let last = axis.ticks.len().saturating_sub(1);
    let ticks = div()
        .relative()
        .h(px(17.0))
        .children(axis.ticks.iter().enumerate().map(|(at, tick)| {
            let label = div()
                .absolute()
                .top_0()
                .text_size(px(10.0))
                .line_height(px(12.0))
                .text_color(theme.ely.fg_subtle)
                .whitespace_nowrap()
                .child(format_duration(*tick));
            if at == last && at > 0 {
                label.right_0()
            } else {
                label.left(relative(across(*tick)))
            }
        }))
        .children(running.then(|| {
            div()
                .absolute()
                .bottom(px(1.0))
                .left(relative(across(elapsed)))
                .ml(px(-2.5))
                .size(px(5.0))
                .rounded_full()
                .bg(theme.ely.info)
        }));
    let waterfall = div()
        .flex()
        .flex_col()
        .child(ticks)
        .children(
            lanes
                .iter()
                .enumerate()
                .map(|(at, lane)| render_lane(lane, at == 0, &axis, &beat, theme, &entity)),
        );

    let open = div()
        .id("trace-open")
        .debug_selector(|| "trace-open".into())
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.0))
        .rounded(theme.radii.chip)
        .border_1()
        .border_color(theme.ely.border_strong)
        .bg(theme.ely.sunken)
        .text_size(caption)
        .text_color(theme.ely.fg)
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(theme.ely.hover))
        .on_click({
            let entity = entity.clone();
            move |_, _, cx| {
                entity.update(cx, |chat, cx| {
                    let _ = chat.activate_trace("trace/open", cx);
                });
            }
        })
        .child("Open full trace ↗");

    div()
        .relative()
        .flex()
        .flex_col()
        .gap_2()
        .child(top)
        .child(numbers)
        .children(dropped)
        .child(waterfall)
        .child(open)
        .child(frame_driver(
            chat.session_panel.frame.trace_frame_pending.clone(),
            chat.session_panel.trace_frames.clone(),
            Pace::of(running, moving, reduced),
            cx.entity_id().as_u64(),
        ))
        .into_any_element()
}

/// One agent: its name line, what it is doing, its track, and a line under it
/// for each permission it waits on. The lane's lines are one click that
/// reveals it; each wait is a sibling, never inside that click, so reaching
/// the permission does not also scroll to the lane.
fn render_lane(
    lane: &Lane,
    first: bool,
    axis: &Axis,
    beat: &Beat,
    theme: &Theme,
    entity: &gpui::Entity<Chat>,
) -> AnyElement {
    let caption = theme.typography.footnote;
    let open = matches!(lane.state, SpanState::Running | SpanState::Waiting);
    let color = state_color(theme, lane.state);
    let indent = f32::from(lane.depth) * INDENT;
    let own_waits = !lane.waits.is_empty();
    let header = div()
        .debug_selector({
            let selector = format!("trace-lane-header-{}", lane.span);
            move || selector.clone()
        })
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(indent))
        .child(
            div()
                .flex_none()
                .size(px(7.0))
                .rounded_full()
                .bg(color)
                .opacity(if open { beat.pulse() } else { 1.0 }),
        )
        .child(
            div()
                .flex_none()
                .max_w(relative(0.6))
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.ely.fg)
                .child(lane.name.clone()),
        )
        .when(open && !lane.detail.is_empty(), |line| {
            line.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.ely.fg_subtle)
                    .child(lane.detail.clone()),
            )
        })
        .when(!(open && !lane.detail.is_empty()), |line| line.child(div().flex_1()))
        .child(
            div()
                .flex_none()
                .text_color(theme.ely.fg_muted)
                .child(lane_count(lane)),
        );
    // What it is doing: the running text shimmers, a wait is the next lines.
    let doing = lane
        .now
        .as_ref()
        .filter(|_| open && !(lane.state == SpanState::Waiting && own_waits))
        .map(|text| {
            let line = div()
                .pl(px(indent + 13.0))
                .min_w_0()
                .truncate()
                .text_color(if lane.state == SpanState::Waiting {
                    theme.ely.warning
                } else {
                    theme.ely.fg_muted
                });
            if lane.state == SpanState::Running {
                // The band is where the turn's clock puts it: the card's own
                // loop repaints it, and the text asks for no frames.
                line.child(
                    ShimmerText::new(
                        SharedString::from(format!("trace-now-{}", lane.span)),
                        text.clone(),
                    )
                    .phase(beat.shimmer()),
                )
            } else {
                line.child(text.clone())
            }
        });
    let reveal = {
        let entity = entity.clone();
        let key = format!("trace/lane/{}", lane.span);
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut App| {
            entity.update(cx, |chat, cx| {
                let _ = chat.activate_trace(&key, cx);
            });
        }
    };
    let clickable = div()
        .id(SharedString::from(format!("trace-lane-{}", lane.span)))
        .debug_selector({
            let selector = format!("trace-lane-{}", lane.span);
            move || selector.clone()
        })
        .flex()
        .flex_col()
        .gap(px(3.0))
        .py(px(6.0))
        .when(!first, |lane| lane.border_t_1().border_color(theme.ely.border))
        .text_size(caption)
        .cursor(CursorStyle::PointingHand)
        .on_click(reveal)
        .child(header)
        .children(doing)
        .child(render_track(lane, axis, beat, theme));
    let waits = lane.waits.iter().map(|wait| {
        let request_id = wait.request_id;
        let entity = entity.clone();
        let line = if wait.tool.is_empty() {
            "Waiting for you".to_string()
        } else {
            format!("Waiting for you · {}", wait.tool)
        };
        div()
            .id(SharedString::from(format!("trace-wait-{request_id}")))
            .debug_selector(move || format!("trace-wait-{request_id}"))
            .pl(px(indent + 13.0))
            .pb(px(4.0))
            .min_w_0()
            .truncate()
            .text_size(caption)
            .text_color(theme.ely.warning)
            .cursor(CursorStyle::PointingHand)
            .hover(|style| style.text_color(theme.ely.fg))
            .on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| {
                    let _ = chat.activate_trace(&format!("trace/wait/{request_id}"), cx);
                });
            })
            .child(line)
    });
    // A lane that has just begun rises into place over `ENTER`, by the turn's
    // clock; a still card, a finished turn and reduced motion draw it whole.
    let entered = entry(beat.moving && !beat.reduced, beat.elapsed, lane.start);
    div()
        .relative()
        .flex()
        .flex_col()
        .when(entered < 1.0, |lane| {
            lane.top(px(-(1.0 - entered) * ENTER_RISE)).opacity(entered)
        })
        .child(clickable)
        .children(waits)
        .into_any_element()
}

/// A lane's track: the grid, the faint band of a subagent's lifetime, its
/// model calls, tools and waits on the shared axis, and the "now" line over
/// them while the turn runs. A failed call is drawn in the danger colour.
fn render_track(lane: &Lane, axis: &Axis, beat: &Beat, theme: &Theme) -> AnyElement {
    let range = axis.range.as_secs_f32().max(0.001);
    let across = |offset: Duration| (offset.as_secs_f32() / range).clamp(0.0, 1.0);
    let span = lane.span.clone();
    let grid = axis.ticks.iter().enumerate().map(|(at, tick)| {
        let selector = format!("trace-grid-{span}-{at}");
        div()
            .debug_selector(move || selector.clone())
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(across(*tick)))
            .w(px(1.0))
            .bg(theme.ely.border)
    });
    // The root lives as long as the turn, which its track already shows; a
    // subagent's lifetime is what tells a lane with no calls from an empty one.
    let band = (lane.span != ROOT).then(|| {
        let start = across(lane.start);
        let end = across(lane.end.unwrap_or(beat.elapsed));
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(start))
            .w(relative((end - start).max(0.0)))
            .min_w(px(2.0))
            .bg(theme.ely.fg_subtle.opacity(0.25))
    });
    let now_line = beat.running.then(|| {
        let selector = format!("trace-nowline-{span}");
        div()
            .debug_selector(move || selector.clone())
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(across(beat.elapsed)))
            .w(px(1.0))
            .bg(theme.ely.info.opacity(0.75))
    });
    div()
        .debug_selector({
            let selector = format!("trace-track-{}", lane.span);
            move || selector.clone()
        })
        .relative()
        .h(px(10.0))
        .w_full()
        .rounded(px(3.0))
        .overflow_hidden()
        .bg(theme.sirio.overlay)
        .children(grid)
        .children(band)
        .children(lane.segments.iter().map(|segment: &Segment| {
            let start = across(segment.start);
            let length = segment.end.saturating_sub(segment.start).as_secs_f32() / range;
            let bar = div()
                .absolute()
                .top(px(1.0))
                .bottom(px(1.0))
                .left(relative(start))
                .w(relative(length.clamp(0.0, 1.0 - start)))
                .min_w(px(2.0))
                .rounded(px(2.0));
            match (segment.kind, segment.state) {
                (SegmentKind::Wait, _) => {
                    bar.bg(pattern_slash(theme.ely.warning.opacity(0.75), 3.0, 6.0))
                }
                (_, SpanState::Failed) => bar.bg(theme.ely.danger),
                (SegmentKind::Llm, _) => bar.bg(theme.ely.chart[4].opacity(0.65)),
                (SegmentKind::Tool, _) => bar.bg(theme.ely.chart[1].opacity(0.8)),
            }
        }))
        .children(now_line)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::SessionPanelAction;
    use super::{Beat, entry};
    use crate::chat::tests::{
        lay_out_short, offline_chat_view, open_question_with_free_text, pump_chat_until,
        push_answers, refresh_frame, top_entry,
    };
    use crate::chat::{Chat, ChatEvent, Entry, LaunchSpec};
    use gpui::{TestAppContext, VisualTestContext, px, size};
    use gpui::Entity;
    use sirio_chat::{SpanKind, SpanOutcome, TokenCount, TraceEvent, TurnOutcome};
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const CLAUDE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sirio_chat/tests/fixtures/claude_fixture.py"
    );

    #[test]
    fn a_provisional_cost_reads_at_least() {
        let base = Instant::now();
        let mut store = crate::chat::trace::TraceStore::default();
        for event in [
            TraceEvent::TurnStarted { at: base },
            TraceEvent::SpanStarted {
                id: "toolu_bg".into(),
                parent: None,
                kind: SpanKind::Subagent,
                name: "fork".into(),
                detail: String::new(),
                model: None,
                at: secs(base, 0.1),
            },
            TraceEvent::MainEnded {
                at: secs(base, 0.5),
                outcome: TurnOutcome::Done,
                cost_usd: None,
                tokens: None,
            },
            TraceEvent::AgentsEnded {
                anchor: "toolu_bg".into(),
                at: secs(base, 48.0),
                cost_usd: Some(3.28),
                tokens: Some(TokenCount { input: 40_000, output: 8_200 }),
                at_least: true,
            },
        ] {
            store.apply(event);
        }

        let turn = store.latest().expect("a turn");
        let now = secs(base, 900.0);
        assert_eq!(turn.summary(now), "Done · 48s · ≥ $3.28");
        let stats = turn.stats(now);
        let values = super::stat_values(&stats);
        let cell = |key: &str| {
            values
                .iter()
                .find(|(k, ..)| *k == key)
                .map(|(.., value, _)| value.clone())
                .expect("the stat exists")
        };
        assert_eq!(cell("cost"), "≥ $3.28");
        assert_eq!(cell("tokens"), "≥ 48.2k");
    }

    fn secs(base: Instant, seconds: f64) -> Instant {
        base + Duration::from_secs_f64(seconds)
    }

    fn started(at: Instant) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::TurnStarted { at })
    }

    fn span(
        id: &str,
        parent: Option<&str>,
        kind: SpanKind,
        name: &str,
        detail: &str,
        model: Option<&str>,
        at: Instant,
    ) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(str::to_string),
            kind,
            name: name.into(),
            detail: detail.into(),
            model: model.map(str::to_string),
            at,
        })
    }

    fn span_ended(id: &str, at: Instant) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::SpanEnded {
            id: id.into(),
            outcome: SpanOutcome::Done,
            at,
            measured_ms: None,
        })
    }

    fn span_failed(id: &str, at: Instant) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::SpanEnded {
            id: id.into(),
            outcome: SpanOutcome::Failed {
                note: "boom".into(),
            },
            at,
            measured_ms: None,
        })
    }

    fn wait(span: &str, request_id: u64, at: Instant) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::WaitStarted {
            span: Some(span.into()),
            request_id,
            at,
        })
    }

    fn ended(
        at: Instant,
        measured_ms: Option<u64>,
        cost_usd: Option<f64>,
        tokens: Option<TokenCount>,
    ) -> sirio_chat::ChatEvent {
        sirio_chat::ChatEvent::Trace(TraceEvent::TurnEnded {
            at,
            outcome: TurnOutcome::Done,
            measured_ms,
            cost_usd,
            tokens,
        })
    }

    fn sections(
        chat: &Entity<Chat>,
        cx: &mut VisualTestContext,
    ) -> Vec<crate::chat::SectionSnapshot> {
        chat.read_with(cx, |chat, _| chat.control_snapshot())
            .session_panel
            .sections
    }

    fn trace_section(
        chat: &Entity<Chat>,
        cx: &mut VisualTestContext,
    ) -> Option<crate::chat::SectionSnapshot> {
        sections(chat, cx).into_iter().find(|section| section.key == "trace")
    }

    fn act(
        chat: &Entity<Chat>,
        cx: &mut VisualTestContext,
        action: SessionPanelAction,
    ) -> Result<(), String> {
        chat.update_in(cx, |chat, window, cx| chat.session_panel_act(action, window, cx))
    }

    fn activate(chat: &Entity<Chat>, cx: &mut VisualTestContext, key: &str) -> Result<(), String> {
        act(chat, cx, SessionPanelAction::Activate(key.to_string()))
    }

    fn frames(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> u32 {
        chat.read_with(cx, |chat, _| chat.session_panel.trace_frames.get())
    }

    /// Lets the delayed frame the card scheduled come due, and draws.
    fn next_frame(cx: &mut VisualTestContext) {
        cx.cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        refresh_frame(cx);
    }

    #[gpui::test]
    async fn the_trace_card_appears_only_after_a_trace_event(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        assert!(
            trace_section(&chat, cx).is_none(),
            "a chat that never heard of a trace shows no card"
        );
        chat.update(cx, |chat, cx| chat.handle_event(started(Instant::now()), cx));
        let card = trace_section(&chat, cx).expect("the first turn brings the card");
        assert!(!card.expanded, "the card starts folded");
    }

    #[gpui::test]
    async fn the_trace_card_sits_between_connectors_and_change_request(cx: &mut TestAppContext) {
        use crate::forge_source::testing::{self, CannedForge, FakeSource};
        // The CLI's worker thread wakes the chat's event task, which the
        // test scheduler refuses unless it is told real threads are in play.
        cx.executor().allow_parking();
        cx.update(sirio_theme::Theme::init);
        cx.update(crate::ely::init);
        cx.update(crate::chat::init);
        let forge = std::sync::Arc::new(CannedForge::default());
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        cx.update(|cx| crate::forge_source::set_source(source, cx));
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let launch = sirio_chat::ClaudeLaunch::fixture(
                "python3",
                vec![CLAUDE_FIXTURE.to_string(), "session_panel".to_string()],
            );
            Chat::from_test_command(LaunchSpec::Claude(launch), std::env::temp_dir(), cx)
        });
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        let neighbours = ["context", "connectors", "change_request"];
        pump_chat_until(cx, &chat, |chat| {
            let drawn = chat.control_snapshot().session_panel.sections;
            neighbours
                .iter()
                .all(|key| drawn.iter().any(|section| section.key == *key))
        });
        chat.update(cx, |chat, cx| chat.handle_event(started(Instant::now()), cx));
        let keys: Vec<String> = sections(&chat, cx)
            .into_iter()
            .map(|section| section.key)
            .collect();
        assert_eq!(
            keys,
            ["environment", "context", "connectors", "trace", "change_request"]
        );
    }

    #[gpui::test]
    async fn the_folded_title_carries_the_latest_turns_summary(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.handle_event(started(Instant::now() - Duration::from_secs(3)), cx);
        });
        let card = trace_section(&chat, cx).expect("the card is drawn");
        assert_eq!(card.summary, "Live · 3s");
        assert!(!card.expanded);
    }

    /// The row a snapshot reports, as the facts a reader of it needs.
    type Facts = (String, u8, String, String, String, Option<String>);

    fn facts(card: &crate::chat::SectionSnapshot) -> Vec<Facts> {
        card.rows
            .iter()
            .map(|row| {
                (
                    row.key.clone(),
                    row.depth,
                    row.label.clone(),
                    row.detail.clone(),
                    row.tag.clone(),
                    row.action.clone(),
                )
            })
            .collect()
    }

    fn row(
        key: &str,
        depth: u8,
        label: &str,
        detail: &str,
        tag: &str,
        action: Option<&str>,
    ) -> Facts {
        (
            key.into(),
            depth,
            label.into(),
            detail.into(),
            tag.into(),
            action.map(str::to_string),
        )
    }

    #[gpui::test]
    async fn the_snapshot_lists_turn_stats_lanes_and_open(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        let base = Instant::now();
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.submit_turn(
                "fix the login bug\nand the logout".into(),
                Vec::new(),
                Vec::new(),
                cx,
            );
            chat.handle_event(started(base), cx);
            chat.handle_event(
                span("msg_1", None, SpanKind::Llm, "claude", "", Some("opus"), secs(base, 0.5)),
                cx,
            );
            chat.handle_event(span_ended("msg_1", secs(base, 2.0)), cx);
            chat.handle_event(
                span(
                    "toolu_agent",
                    None,
                    SpanKind::Subagent,
                    "Explore",
                    "scan the repo",
                    None,
                    secs(base, 2.0),
                ),
                cx,
            );
            chat.handle_event(
                span(
                    "toolu_read",
                    Some("toolu_agent"),
                    SpanKind::Tool,
                    "Read",
                    "",
                    None,
                    secs(base, 3.0),
                ),
                cx,
            );
            chat.handle_event(span_ended("toolu_read", secs(base, 5.0)), cx);
            chat.handle_event(span_ended("toolu_agent", secs(base, 8.0)), cx);
            chat.handle_event(
                span("toolu_edit", None, SpanKind::Tool, "Edit", "", None, secs(base, 8.0)),
                cx,
            );
            chat.handle_event(span_ended("toolu_edit", secs(base, 10.0)), cx);
            chat.handle_event(
                ended(
                    secs(base, 12.0),
                    Some(12_000),
                    None,
                    Some(TokenCount { input: 40_000, output: 8_200 }),
                ),
                cx,
            );
        });
        let card = trace_section(&chat, cx).expect("the card is drawn");
        assert_eq!(
            facts(&card),
            vec![
                row("trace/turn", 0, "fix the login bug", "", "done", None),
                row("trace/stat/duration", 0, "Duration", "12s", "", None),
                row("trace/stat/cost", 0, "Cost", "—", "", None),
                row("trace/stat/tokens", 0, "Tokens", "48.2k", "", None),
                row("trace/stat/llm_calls", 0, "LLM calls", "1", "", None),
                row("trace/stat/tool_calls", 0, "Tool calls", "2", "", None),
                row("trace/stat/errors", 0, "Errors", "0", "", None),
                row("trace/lane/root", 0, "Claude Code", "2 calls · 12s", "done", Some("reveal")),
                row("trace/lane/toolu_agent", 1, "Explore", "1 call · 6s", "done", Some("reveal")),
                row("trace/open", 0, "Open full trace", "", "", Some("open")),
            ]
        );

        // The next turn's result names its spend: it is shown, where the
        // first showed a dash.
        chat.update(cx, |chat, cx| {
            chat.submit_turn("again".into(), Vec::new(), Vec::new(), cx);
            chat.handle_event(started(base + Duration::from_secs(30)), cx);
            chat.handle_event(
                ended(base + Duration::from_secs(34), Some(4_000), Some(0.31), None),
                cx,
            );
        });
        let card = trace_section(&chat, cx).expect("the card follows the latest turn");
        let detail = |key: &str| {
            card.rows
                .iter()
                .find(|row| row.key == key)
                .map(|row| row.detail.clone())
        };
        assert_eq!(card.rows[0].label, "again");
        assert_eq!(detail("trace/stat/cost").as_deref(), Some("$0.31"));
        assert_eq!(detail("trace/stat/tokens").as_deref(), Some("—"));
        assert_eq!(card.summary, "Done · 4s · $0.31");
    }

    #[gpui::test]
    async fn activating_a_lane_reveals_its_entry(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        let agent = chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.submit_turn("audit the crate".into(), Vec::new(), Vec::new(), cx);
            chat.handle_event(started(Instant::now()), cx);
            chat.handle_event(
                span("toolu_agent", None, SpanKind::Subagent, "Explore", "", None, Instant::now()),
                cx,
            );
            push_answers(chat, 10);
            chat.push_entry(Entry::SubagentTask {
                id: "toolu_agent".into(),
                title: "Explore".into(),
                status: "Completed".into(),
                tool_calls: Vec::new(),
                expanded: false,
            });
            let agent = chat.entries.len() - 1;
            push_answers(chat, 40);
            cx.notify();
            agent
        });
        lay_out_short(cx);
        assert!(top_entry(&chat, cx) > agent, "the tail is far below the agent's row");

        activate(&chat, cx, "trace/lane/toolu_agent").expect("a lane reveals its entry");
        refresh_frame(cx);
        assert_eq!(top_entry(&chat, cx), agent);
        assert!(
            !chat.read_with(cx, |chat, _| chat.list_state.is_following_tail()),
            "a reveal must not be undone by the next line"
        );
        assert!(
            activate(&chat, cx, "trace/lane/toolu_nobody").is_err(),
            "a lane the turn does not hold has nothing to reveal"
        );
    }

    #[gpui::test]
    async fn activating_the_wait_reveals_the_permission(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        let (user, permission) = chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.submit_turn("deploy it".into(), Vec::new(), Vec::new(), cx);
            let user = chat.entries.len() - 1;
            let now = Instant::now();
            chat.handle_event(started(now), cx);
            chat.handle_event(
                span("toolu_deploy", None, SpanKind::Tool, "Deploy", "", None, now),
                cx,
            );
            chat.handle_event(wait("toolu_deploy", 42, now), cx);
            push_answers(chat, 12);
            chat.push_entry(open_question_with_free_text(42));
            let permission = chat.entries.len() - 1;
            push_answers(chat, 30);
            cx.notify();
            (user, permission)
        });
        lay_out_short(cx);

        let card = trace_section(&chat, cx).expect("the card is drawn");
        let lane = card.rows.iter().position(|row| row.key == "trace/lane/root");
        let wait_row = card.rows.iter().position(|row| row.key == "trace/wait/42");
        assert_eq!(
            wait_row,
            lane.map(|lane| lane + 1),
            "the wait is listed right after its lane: {:?}",
            facts(&card)
        );
        let lane = &card.rows[lane.unwrap()];
        assert_eq!(lane.tag, "waiting");
        let wait_row = &card.rows[wait_row.unwrap()];
        assert_eq!(
            (wait_row.depth, wait_row.label.as_str(), wait_row.detail.as_str()),
            (1, "Waiting for you", "Deploy")
        );
        assert_eq!(wait_row.action.as_deref(), Some("reveal"));

        activate(&chat, cx, "trace/wait/42").expect("the wait reveals its permission");
        refresh_frame(cx);
        assert_eq!(top_entry(&chat, cx), permission);
        assert_ne!(permission, user);
        assert!(
            activate(&chat, cx, "trace/wait/7").is_err(),
            "a wait nobody is in has no row to act on"
        );
    }

    #[gpui::test]
    async fn activating_open_emits_open_trace(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.handle_event(started(Instant::now()), cx);
        });
        let opened = Rc::new(Cell::new(0));
        let seen = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                if matches!(event, ChatEvent::OpenTrace) {
                    seen.set(seen.get() + 1);
                }
            })
            .detach();
        });
        activate(&chat, cx, "trace/open").expect("the card opens the trace");
        assert_eq!(opened.get(), 1);
        assert!(
            activate(&chat, cx, "trace/stat/cost").is_err(),
            "a stat is read, not clicked"
        );
        assert_eq!(opened.get(), 1);
    }

    #[gpui::test]
    async fn no_frame_is_requested_once_the_turn_ends(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        chat.update(cx, |chat, cx| {
            let now = Instant::now();
            chat.handle_event(started(now), cx);
            chat.handle_event(span("toolu_run", None, SpanKind::Tool, "Bash", "", None, now), cx);
        });
        for _ in 0..3 {
            next_frame(cx);
        }
        assert_eq!(
            frames(&chat, cx),
            0,
            "a folded card asks for no frame, though its turn runs"
        );

        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        refresh_frame(cx);
        let first = frames(&chat, cx);
        assert!(first > 0, "an unfolded card of a running turn animates");
        next_frame(cx);
        assert!(frames(&chat, cx) > first, "and goes on animating");

        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card folds");
        refresh_frame(cx);
        let folded = frames(&chat, cx);
        for _ in 0..3 {
            next_frame(cx);
        }
        assert_eq!(frames(&chat, cx), folded, "folding the card stops its frames");

        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds again");
        refresh_frame(cx);
        next_frame(cx);
        assert!(frames(&chat, cx) > folded, "unfolded again, it animates again");

        chat.update(cx, |chat, cx| {
            chat.handle_event(span_ended("toolu_run", Instant::now()), cx);
            chat.handle_event(ended(Instant::now(), None, None, None), cx);
        });
        refresh_frame(cx);
        let finished = frames(&chat, cx);
        for _ in 0..3 {
            next_frame(cx);
        }
        assert_eq!(frames(&chat, cx), finished, "a finished turn requests nothing");
    }

    /// A real click at the middle of a drawn element.
    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        for _ in 0..3 {
            refresh_frame(cx);
        }
    }

    /// A selector built at run time, which `debug_bounds` wants as `'static`.
    fn selector(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    /// A chat with a running turn whose card is unfolded and painted: the
    /// root runs a tool, so its "now" line is drawn.
    fn running_card(cx: &mut TestAppContext) -> (Entity<Chat>, &mut VisualTestContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            let now = Instant::now() - Duration::from_secs(5);
            chat.handle_event(started(now), cx);
            chat.handle_event(
                span("toolu_run", None, SpanKind::Tool, "Bash", "", None, secs(now, 1.0)),
                cx,
            );
        });
        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        for _ in 0..4 {
            refresh_frame(cx);
        }
        (chat, cx)
    }

    #[gpui::test]
    async fn the_card_asks_for_no_frame_but_its_own(cx: &mut TestAppContext) {
        let (chat, cx) = running_card(cx);
        // The test window has no frame loop: the callbacks a draw asks the
        // next frame for wait until they are delivered by hand.
        cx.update(|window, cx| window.simulate_next_frame(cx));
        // A lane arrives: whatever draws it in must not ask for frames of its
        // own, and neither may the shimmering "now" text of the running lane.
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                span(
                    "toolu_agent",
                    None,
                    SpanKind::Subagent,
                    "Explore",
                    "scan",
                    None,
                    Instant::now(),
                ),
                cx,
            );
        });
        for _ in 0..6 {
            refresh_frame(cx);
        }
        let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(
            asked, 0,
            "every frame the trace causes comes from its own capped loop"
        );
    }

    #[gpui::test]
    async fn frames_are_capped_at_thirty_a_second(cx: &mut TestAppContext) {
        let (chat, cx) = running_card(cx);
        let before = frames(&chat, cx);
        for _ in 0..100 {
            cx.cx.executor().advance_clock(Duration::from_millis(10));
            cx.run_until_parked();
            refresh_frame(cx);
        }
        let asked = frames(&chat, cx) - before;
        assert!(asked >= 1, "a running card keeps moving");
        assert!(asked <= 31, "{asked} frames in a second is past 30 a second");
    }

    #[gpui::test]
    async fn a_wait_rows_click_does_not_reveal_its_lane(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        let (user, permission) = chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.submit_turn("deploy it".into(), Vec::new(), Vec::new(), cx);
            let user = chat.entries.len() - 1;
            let now = Instant::now() - Duration::from_secs(5);
            chat.handle_event(started(now), cx);
            chat.handle_event(
                span("toolu_deploy", None, SpanKind::Tool, "Deploy", "", None, secs(now, 1.0)),
                cx,
            );
            chat.handle_event(wait("toolu_deploy", 42, secs(now, 2.0)), cx);
            push_answers(chat, 12);
            chat.push_entry(open_question_with_free_text(42));
            let permission = chat.entries.len() - 1;
            push_answers(chat, 60);
            cx.notify();
            (user, permission)
        });
        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        for _ in 0..4 {
            refresh_frame(cx);
        }
        assert!(top_entry(&chat, cx) > permission, "the tail is far below the request");

        click(cx, "trace-wait-42");
        assert_eq!(
            top_entry(&chat, cx),
            permission,
            "the yellow row reaches the request, not the lane under which it sits"
        );
        assert_ne!(top_entry(&chat, cx), user);
    }

    #[gpui::test]
    async fn the_axis_lines_stay_inside_the_tracks(cx: &mut TestAppContext) {
        let (chat, cx) = running_card(cx);
        chat.update(cx, |chat, cx| {
            let now = Instant::now() - Duration::from_secs(3);
            chat.handle_event(
                span("toolu_agent", None, SpanKind::Subagent, "Explore", "scan", None, now),
                cx,
            );
        });
        for _ in 0..4 {
            refresh_frame(cx);
        }
        for lane in ["root", "toolu_agent"] {
            let header = cx
                .debug_bounds(selector(format!("trace-lane-header-{lane}")))
                .unwrap_or_else(|| panic!("the header of {lane} is drawn"));
            let track = cx
                .debug_bounds(selector(format!("trace-track-{lane}")))
                .unwrap_or_else(|| panic!("the track of {lane} is drawn"));
            for line in [
                selector(format!("trace-nowline-{lane}")),
                selector(format!("trace-grid-{lane}-1")),
            ] {
                let bounds = cx
                    .debug_bounds(line)
                    .unwrap_or_else(|| panic!("{line} is drawn"));
                assert!(
                    bounds.top() >= track.top() && bounds.bottom() <= track.bottom(),
                    "{line} {bounds:?} leaves its track {track:?}"
                );
                assert!(
                    !bounds.intersects(&header),
                    "{line} {bounds:?} crosses the header {header:?}"
                );
            }
        }
    }

    #[test]
    fn a_lane_enters_over_two_hundred_milliseconds_from_its_start() {
        let at = |millis: u64| entry(true, Duration::from_millis(millis), Duration::from_secs(10));
        // Not there yet, which a clock a little behind the lane's own
        // instants can say, must not underflow.
        assert_eq!(entry(true, Duration::from_secs(9), Duration::from_secs(10)), 0.0);
        assert_eq!(at(10_000), 0.0);
        assert!(at(10_100) > 0.5 && at(10_100) < 1.0, "an ease-out: past half at half time");
        assert_eq!(at(10_200), 1.0);
        assert_eq!(at(60_000), 1.0);
        // A turn that ended draws every lane whole, whenever it began.
        assert_eq!(
            entry(false, Duration::from_millis(10_050), Duration::from_secs(10)),
            1.0
        );
    }

    /// A running turn whose card is unfolded and painted, and whose one open
    /// call waits on the user: the turn is open and nothing in it runs. With
    /// `agent` a subagent works beside the wait.
    fn waiting_card(
        cx: &mut TestAppContext,
        agent: bool,
    ) -> (Entity<Chat>, &mut VisualTestContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            let now = Instant::now() - Duration::from_secs(5);
            chat.handle_event(started(now), cx);
            chat.handle_event(
                span("toolu_edit", None, SpanKind::Tool, "Edit", "", None, secs(now, 1.0)),
                cx,
            );
            chat.handle_event(wait("toolu_edit", 1, secs(now, 2.0)), cx);
            if agent {
                chat.handle_event(
                    span(
                        "toolu_agent",
                        None,
                        SpanKind::Subagent,
                        "Explore",
                        "scan",
                        None,
                        secs(now, 3.0),
                    ),
                    cx,
                );
            }
        });
        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        for _ in 0..4 {
            refresh_frame(cx);
        }
        (chat, cx)
    }

    /// The frames the card asks for over `steps` clock steps of 10 ms. The
    /// window is drawn once a frame comes due, which is when a drawn card
    /// would be, and not on every step: nothing else would repaint it, and a
    /// draw while a frame is pending asks for nothing.
    fn frames_over(chat: &Entity<Chat>, cx: &mut VisualTestContext, steps: u32) -> u32 {
        let before = frames(chat, cx);
        for _ in 0..steps {
            cx.cx.executor().advance_clock(Duration::from_millis(10));
            cx.run_until_parked();
            let pending = chat.read_with(cx, |chat, _| {
                crate::trace_frame::is_pending(&chat.session_panel.frame.trace_frame_pending)
            });
            if !pending {
                refresh_frame(cx);
            }
        }
        frames(chat, cx) - before
    }

    #[gpui::test]
    async fn a_card_that_only_waits_ticks_once_a_second(cx: &mut TestAppContext) {
        let (chat, cx) = waiting_card(cx, false);
        let asked = frames_over(&chat, cx, 300);
        assert!(asked >= 1, "an open turn's clock keeps moving");
        assert!(
            asked <= 4,
            "{asked} frames in 3 s of waiting for the user is past a tick a second"
        );
    }

    #[gpui::test]
    async fn an_agent_running_beside_the_wait_brings_the_capped_frames_back(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = waiting_card(cx, true);
        let asked = frames_over(&chat, cx, 100);
        assert!(asked >= 10, "{asked} frames in a second is a tick, not an animation");
        assert!(asked <= 31, "{asked} frames in a second is past 30 a second");
    }

    #[gpui::test]
    async fn a_span_that_starts_while_a_tick_is_on_its_way_gets_its_frames_at_once(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.handle_event(started(Instant::now() - Duration::from_secs(1)), cx);
        });
        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        for _ in 0..4 {
            refresh_frame(cx);
        }
        // The turn is open and nothing in it runs, so the next paint is a
        // second away. Then a call starts: it is not held for that second.
        assert_eq!(frames(&chat, cx), 1, "one frame is on its way, a tick");
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                span("toolu_run", None, SpanKind::Tool, "Bash", "", None, Instant::now()),
                cx,
            );
        });
        refresh_frame(cx);
        let asked = frames_over(&chat, cx, 20);
        assert!(asked >= 3, "{asked} frames in 200 ms after a call started: the tick held them");
    }

    #[test]
    fn a_mark_pulses_only_while_the_turn_is_moving() {
        let beat = |moving| Beat {
            running: true,
            moving,
            reduced: false,
            // 600 ms is the bottom of a 1.2 s pulse.
            elapsed: Duration::from_millis(600),
        };
        assert!(beat(true).pulse() < 0.4, "a moving turn's mark dims");
        assert_eq!(beat(false).pulse(), 1.0, "a turn that only waits holds its marks still");
    }

    #[test]
    fn the_titles_dot_pulses_only_while_the_card_is_open_and_the_turn_moves() {
        use crate::chat::trace::TraceStore;
        let base = Instant::now();
        let mut store = TraceStore::default();
        let mut feed = |event| {
            store.apply(event);
        };
        feed(TraceEvent::TurnStarted { at: base });
        feed(TraceEvent::SpanStarted {
            id: "toolu_edit".into(),
            parent: None,
            kind: SpanKind::Tool,
            name: "Edit".into(),
            detail: String::new(),
            model: None,
            at: secs(base, 0.1),
        });
        // 600 ms is the bottom of a 1.2 s pulse.
        let at = secs(base, 0.6);
        let alpha = |store: &TraceStore, folded| {
            super::title_dot_alpha(store.latest().expect("a turn"), folded, at, false)
        };
        assert!(alpha(&store, false) < 0.4, "a running turn's dot pulses while the card is open");
        assert_eq!(alpha(&store, true), 1.0, "a folded card asks for no frames to pulse on");

        store.apply(TraceEvent::WaitStarted {
            span: Some("toolu_edit".into()),
            request_id: 1,
            at: secs(base, 0.2),
        });
        assert_eq!(alpha(&store, false), 1.0, "a turn that only waits holds the dot still");
    }

    #[gpui::test]
    async fn a_turn_past_the_span_cap_says_what_it_did_not_draw(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        let base = Instant::now() - Duration::from_secs(5);
        // Exactly the cap: a call, 1 998 reads and one failed read.
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            chat.handle_event(started(base), cx);
            chat.handle_event(
                span("msg_1", None, SpanKind::Llm, "claude", "", Some("opus"), base),
                cx,
            );
            for n in 0..1998 {
                let id = format!("toolu_{n}");
                chat.handle_event(
                    span(&id, None, SpanKind::Tool, "Read", "", None, secs(base, 1.0)),
                    cx,
                );
            }
            chat.handle_event(
                span("toolu_bad", None, SpanKind::Tool, "Bash", "", None, secs(base, 1.5)),
                cx,
            );
            chat.handle_event(span_failed("toolu_bad", secs(base, 2.0)), cx);
        });
        let detail = |card: &crate::chat::SectionSnapshot, key: &str| {
            card.rows
                .iter()
                .find(|row| row.key == key)
                .map(|row| row.detail.clone())
                .unwrap_or_else(|| panic!("no row {key}"))
        };
        let counts = |card: &crate::chat::SectionSnapshot| {
            [
                detail(card, "trace/stat/llm_calls"),
                detail(card, "trace/stat/tool_calls"),
                detail(card, "trace/stat/errors"),
            ]
        };
        let card = trace_section(&chat, cx).expect("the card is drawn");
        assert_eq!(counts(&card), ["1", "1999", "1"], "at the cap nothing was left out");
        assert!(
            card.rows.iter().all(|row| row.key != "trace/dropped"),
            "and the card says nothing of it"
        );

        // One span too many.
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                span("toolu_past_0", None, SpanKind::Tool, "Read", "", None, secs(base, 3.0)),
                cx,
            );
        });
        let card = trace_section(&chat, cx).expect("the card is drawn");
        assert_eq!(
            counts(&card),
            ["1+", "1999+", "1+"],
            "a count that leaves spans out is at least what it says"
        );
        assert_eq!(
            card.rows.iter().position(|row| row.key == "trace/dropped"),
            Some(7),
            "the row sits under the six numbers"
        );
        assert_eq!(card.rows[7].label, "1 span past the 2000 cap not drawn");

        // And two more.
        chat.update(cx, |chat, cx| {
            for n in 1..3 {
                let id = format!("toolu_past_{n}");
                chat.handle_event(
                    span(&id, None, SpanKind::Tool, "Read", "", None, secs(base, 3.0)),
                    cx,
                );
            }
        });
        let card = trace_section(&chat, cx).expect("the card is drawn");
        assert_eq!(card.rows[7].label, "3 spans past the 2000 cap not drawn");

        // Unfolded, the card draws the same row. Drawing it is the slow
        // part: a lane of 2 000 calls.
        act(&chat, cx, SessionPanelAction::Toggle("trace".into())).expect("the card unfolds");
        for _ in 0..2 {
            next_frame(cx);
        }
        assert!(cx.debug_bounds("trace-dropped").is_some(), "and draws it in the card");
    }

    #[gpui::test]
    async fn the_folded_title_ticks_while_the_turn_runs_and_only_while_painted(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        let notified = Rc::new(Cell::new(0u32));
        let seen = notified.clone();
        cx.update(|_, cx| {
            cx.observe(&chat, move |_, _| seen.set(seen.get() + 1)).detach();
        });
        chat.update(cx, |chat, cx| chat.handle_event(started(Instant::now()), cx));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let tick = |cx: &mut VisualTestContext| {
            let before = notified.get();
            cx.cx.executor().advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
            notified.get() - before
        };
        assert_eq!(tick(cx), 1, "a painted chat is told once a second");
        // The test window draws what a notification dirtied; a chat in a
        // background tab is not drawn at all, which this stands in for.
        chat.read_with(cx, |chat, _| chat.session_panel.frame.trace_painted.set(false));
        assert_eq!(
            tick(cx),
            0,
            "a chat nothing painted since the last tick is left alone"
        );
        refresh_frame(cx);
        assert_eq!(tick(cx), 1, "painted again, it is told again");

        chat.update(cx, |chat, cx| chat.handle_event(ended(Instant::now(), None, None, None), cx));
        refresh_frame(cx);
        assert_eq!(tick(cx), 0, "the tick ends with the turn");
        refresh_frame(cx);
        assert_eq!(tick(cx), 0);
    }
}

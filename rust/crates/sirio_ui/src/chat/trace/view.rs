//! The views a [`Turn`] answers: its stats, lanes, tree rows, reveal target
//! and summary, and the axis the waterfall draws on.
//!
//! Every view is computed from the spans as they are at `now`. A turn that
//! has ended no longer reads `now`: each of its spans has an end, and the
//! agent's own clock wins over the instants wherever it reported one.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use sirio_acp::{SpanKind, SpanOutcome, TokenCount, TurnOutcome};

use super::{ROOT, ROOT_NAME, Span, SpanEnd, Spend, Turn, TurnEnd};

/// What an axis is divided by, smallest first, in seconds: 1, 2, 5, 10, 15
/// and 30 s, then 1, 2, 5, 10, 15 and 30 min. Past the last the step doubles.
const AXIS_STEPS: [u64; 12] = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800];
/// The most intervals an axis is divided into.
const AXIS_INTERVALS: u64 = 4;
/// The least an axis spans, in milliseconds.
const AXIS_MINIMUM_MS: u64 = 10_000;

/// Where a turn is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnState {
    Live,
    Waiting,
    Done,
    Failed,
    Interrupted,
}

/// Where a span, or the agent a span runs, is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanState {
    Running,
    Waiting,
    Done,
    Failed,
    Interrupted,
}

impl SpanState {
    /// The word the control socket and the cards say it with.
    pub fn word(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

/// The six numbers of a turn's stats row.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnStats {
    pub duration: Duration,
    /// `None` until the turn's `result` says, and when it cannot say.
    pub cost_usd: Option<f64>,
    /// On the same terms as `cost_usd`.
    pub tokens: Option<TokenCount>,
    /// `cost_usd` is a floor: background agents still run, or some of their
    /// spend could not be told.
    pub cost_at_least: bool,
    pub llm_calls: usize,
    pub tool_calls: usize,
    pub errors: usize,
    /// The spans past the cap the turn did not keep. The three counts above
    /// cover only the spans it kept, so each is at least what it says.
    pub dropped: usize,
}

/// A permission a lane is waiting on.
#[derive(Clone, Debug, PartialEq)]
pub struct LaneWait {
    pub request_id: u64,
    /// The waited span's name when it is a tool, else empty.
    pub tool: String,
}

/// One agent of a turn: the root, or a subagent.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    /// The span of the agent, [`ROOT`] for the root.
    pub span: String,
    pub depth: u8,
    pub name: String,
    pub detail: String,
    pub state: SpanState,
    /// Its own tool and model calls. A subagent is a lane, not a call.
    pub calls: usize,
    pub duration: Duration,
    /// What a running lane is doing; `None` for one that is not running.
    pub now: Option<String>,
    /// The permissions this agent's own calls wait on, in arrival order. A
    /// wait is listed on one lane only, though every lane above it waits.
    pub waits: Vec<LaneWait>,
    pub segments: Vec<Segment>,
    /// When the agent's lifetime began, as an offset from the turn's start.
    pub start: Duration,
    /// When it ended, on the same terms; `None` while it runs.
    pub end: Option<Duration>,
}

/// A stretch of a lane's track, as offsets from the turn's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub kind: SegmentKind,
    pub start: Duration,
    pub end: Duration,
    /// How the call, or the wait, stands: what colours the bar.
    pub state: SpanState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Llm,
    Tool,
    Wait,
}

/// One row of the tab's tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub span: String,
    pub depth: u8,
    /// `None` for the root.
    pub kind: Option<SpanKind>,
    pub name: String,
    pub detail: String,
    pub state: SpanState,
    pub start: Duration,
    /// `None` while the span runs.
    pub end: Option<Duration>,
    /// Every row folded away under this one.
    pub hidden_children: usize,
    /// Where each of those rows starts, in order: the marks a folded parent
    /// draws on its own bar.
    pub child_marks: Vec<Duration>,
    /// A failed span's error line, a failed turn's reason on the root.
    pub note: Option<String>,
    /// The model of a model call, and of the root its latest call's.
    pub model: Option<String>,
    /// A model call's tokens; on the root, the turn's own once known.
    pub tokens: Option<TokenCount>,
    /// A subagent's running total from the agent's progress reports, to show
    /// when no model call has reported `tokens`.
    pub progress_tokens: Option<u64>,
    /// How many tools a subagent has used, from its progress reports.
    pub tool_uses: Option<u32>,
    /// The waits on this span alone, as offsets from the turn's start; the
    /// end is `None` while the wait is open. The root's are those on the
    /// root.
    pub waits: Vec<(Duration, Option<Duration>)>,
}

/// The transcript entry a span reveals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevealTarget {
    /// The tool row, or the `Agent` row, with this id.
    ToolRow(String),
    /// The permission request with this id.
    Permission(u64),
    /// The turn's user message.
    UserMessage,
}

/// The waterfall's time axis, from 0 to `range`.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub range: Duration,
    pub ticks: Vec<Duration>,
}

/// How a turn's spans nest, worked out once per view.
struct Shape {
    /// How deep each span sits in the tree, the root's children at 1.
    depth: Vec<u8>,
    /// How deep each span's agent lane sits, the first subagents at 1.
    lane_depth: Vec<u8>,
    /// Whether the span, or something under it, has an open wait.
    waiting: Vec<bool>,
    root_waiting: bool,
    /// What each agent owns, gathered in one pass: slot 0 is the root and
    /// slot `i + 1` the subagent that is span `i`.
    lanes: Vec<LaneFacts>,
}

/// What one agent owns.
#[derive(Clone, Default)]
struct LaneFacts {
    /// Its own tool and model-call spans, in arrival order.
    calls: Vec<usize>,
    /// The subagents directly under it that are still open.
    running_agents: usize,
    /// Its latest-started open tool and open model call.
    running_tool: Option<usize>,
    running_call: Option<usize>,
    /// Its latest model call that names a model.
    latest_model: Option<usize>,
    /// Its waits, open or over, in arrival order: indices into `Turn::waits`.
    waits: Vec<usize>,
}

/// An agent's slot in [`Shape::lanes`]; `None` is the root.
fn slot(lane: Option<usize>) -> usize {
    lane.map_or(0, |index| index + 1)
}

impl Span {
    /// How a lane's track draws it. A subagent is a lane of its own, never
    /// a segment of its parent's.
    fn segment_kind(&self) -> Option<SegmentKind> {
        match self.kind {
            SpanKind::Llm => Some(SegmentKind::Llm),
            SpanKind::Tool => Some(SegmentKind::Tool),
            SpanKind::Subagent => None,
        }
    }

    /// How long an ended span took: the agent's own clock when it reported
    /// one.
    fn ended_duration(&self) -> Option<Duration> {
        self.end.as_ref().map(|end| {
            end.measured
                .unwrap_or_else(|| end.at.saturating_duration_since(self.start))
        })
    }

    /// How long the span has taken as of `now`.
    fn duration(&self, now: Instant) -> Duration {
        self.ended_duration()
            .unwrap_or_else(|| now.saturating_duration_since(self.start))
    }

    fn failure(&self) -> Option<&str> {
        match &self.end {
            Some(SpanEnd {
                outcome: SpanOutcome::Failed { note },
                ..
            }) => Some(note),
            _ => None,
        }
    }
}

impl Turn {
    pub fn stats(&self, now: Instant) -> TurnStats {
        let known = self.known_spend();
        let mut stats = TurnStats {
            duration: self.elapsed(now),
            cost_usd: known.and_then(|spend| spend.cost_usd),
            tokens: known.and_then(|spend| spend.tokens.clone()),
            cost_at_least: known.is_some_and(|spend| spend.at_least),
            llm_calls: 0,
            tool_calls: 0,
            errors: 0,
            dropped: self.dropped,
        };
        for span in &self.spans {
            match span.kind {
                SpanKind::Llm => stats.llm_calls += 1,
                SpanKind::Tool => stats.tool_calls += 1,
                SpanKind::Subagent => {}
            }
            if span.failure().is_some() {
                stats.errors += 1;
            }
        }
        stats
    }

    /// Whether the turn is drawn moving: it has not ended and some span is
    /// running, a model call streaming, a tool running or a subagent working,
    /// not waiting on the user. A turn that only waits, or sits between two
    /// calls with nothing open, is still open and its clock goes on, but
    /// nothing in it moves: the card and the tab ask for a frame a second,
    /// not thirty.
    pub fn is_moving(&self) -> bool {
        if self.ended.is_some() {
            return false;
        }
        let shape = self.shape();
        (0..self.spans.len()).any(|index| self.span_state(index, &shape) == SpanState::Running)
    }

    /// The root first, then each subagent in tree order, depth-first and
    /// each level in the order its spans started.
    pub fn lanes(&self, now: Instant) -> Vec<Lane> {
        let shape = self.shape();
        let root = Lane {
            span: ROOT.to_string(),
            depth: 0,
            name: ROOT_NAME.to_string(),
            detail: self.latest_model(None, &shape).unwrap_or_default(),
            state: self.root_state(),
            calls: shape.lanes[slot(None)].calls.len(),
            duration: self.elapsed(now),
            now: self.now_row(None, &shape, now),
            waits: self.waits_of(None, &shape),
            segments: self.segments_of(None, &shape, now),
            start: Duration::ZERO,
            end: self.ended.as_ref().map(|_| self.elapsed(now)),
        };
        let agents = self
            .preorder()
            .into_iter()
            .filter(|&index| self.spans[index].kind == SpanKind::Subagent)
            .map(|index| {
                let span = &self.spans[index];
                let lane = Some(index);
                let start = self.offset(span.start);
                Lane {
                    span: span.id.clone(),
                    depth: shape.lane_depth[index],
                    name: span.name.clone(),
                    detail: self.detail_of(index, &shape),
                    state: self.span_state(index, &shape),
                    calls: shape.lanes[slot(lane)].calls.len(),
                    duration: span.duration(now),
                    now: self.now_row(lane, &shape, now),
                    waits: self.waits_of(lane, &shape),
                    segments: self.segments_of(lane, &shape, now),
                    start,
                    end: span.end.as_ref().map(|_| start + span.duration(now)),
                }
            });
        std::iter::once(root).chain(agents).collect()
    }

    /// The tree: the root, then its spans depth-first, each level in the
    /// order its spans started. A span in `folded` hides everything under it.
    pub fn rows(&self, folded: &HashSet<String>, now: Instant) -> Vec<TreeRow> {
        let shape = self.shape();
        // Each span's own waits, as offsets from the turn's start. `None` is
        // the root's, and holds those on a span the turn did not keep.
        let mut waits: HashMap<Option<usize>, Vec<(Duration, Option<Duration>)>> = HashMap::new();
        for wait in &self.waits {
            waits.entry(wait.span).or_default().push((
                self.offset(wait.start),
                wait.end.map(|end| self.offset(end)),
            ));
        }
        let mut rows = Vec::new();
        let everything: Vec<usize> = (0..self.spans.len()).collect();
        let root_folded = folded.contains(ROOT);
        let hidden = if root_folded { &everything[..] } else { &[] };
        rows.push(TreeRow {
            span: ROOT.to_string(),
            depth: 0,
            kind: None,
            name: ROOT_NAME.to_string(),
            detail: self.latest_model(None, &shape).unwrap_or_default(),
            state: self.root_state(),
            start: Duration::ZERO,
            end: self.ended.as_ref().map(|_| self.elapsed(now)),
            hidden_children: hidden.len(),
            child_marks: self.marks_of(hidden),
            note: match &self.ended {
                Some(TurnEnd {
                    outcome: TurnOutcome::Failed { reason },
                    ..
                }) if !reason.is_empty() => Some(reason.clone()),
                _ => None,
            },
            model: self.latest_model(None, &shape),
            tokens: self
                .known_spend()
                .and_then(|spend| spend.tokens.clone()),
            progress_tokens: None,
            tool_uses: None,
            waits: waits.remove(&None).unwrap_or_default(),
        });
        if root_folded {
            return rows;
        }
        let mut pending: Vec<usize> = self.children_of(None).into_iter().rev().collect();
        while let Some(index) = pending.pop() {
            let span = &self.spans[index];
            let hidden = if folded.contains(&span.id) {
                self.descendants_of(index)
            } else {
                Vec::new()
            };
            let start = self.offset(span.start);
            rows.push(TreeRow {
                span: span.id.clone(),
                depth: shape.depth[index],
                kind: Some(span.kind.clone()),
                name: span.name.clone(),
                detail: self.detail_of(index, &shape),
                state: self.span_state(index, &shape),
                start,
                end: span.end.as_ref().map(|_| start + span.duration(now)),
                hidden_children: hidden.len(),
                child_marks: self.marks_of(&hidden),
                note: span
                    .failure()
                    .filter(|note| !note.is_empty())
                    .map(str::to_string),
                model: span.model.clone(),
                tokens: span.tokens.clone(),
                progress_tokens: span.progress_tokens,
                tool_uses: span.tool_uses,
                waits: waits.remove(&Some(index)).unwrap_or_default(),
            });
            if hidden.is_empty() {
                pending.extend(self.children_of(Some(index)).into_iter().rev());
            }
        }
        rows
    }

    /// The transcript entry `span` reaches: the permission it waits on, else
    /// the `Agent` row of the outermost subagent at or above it, else its own
    /// row when it is a main-thread tool. The root, a main-thread model call
    /// and a span this turn does not hold have only the user's message.
    pub fn reveal_target(&self, span: &str) -> RevealTarget {
        let Some(&index) = self.by_id.get(span) else {
            return RevealTarget::UserMessage;
        };
        let waited = self
            .waits
            .iter()
            .find(|wait| wait.end.is_none() && wait.span == Some(index));
        if let Some(wait) = waited {
            return RevealTarget::Permission(wait.request_id);
        }
        let mut outermost = None;
        let mut cursor = Some(index);
        while let Some(at) = cursor {
            if self.spans[at].kind == SpanKind::Subagent {
                outermost = Some(at);
            }
            cursor = self.spans[at].parent;
        }
        match outermost {
            Some(agent) => RevealTarget::ToolRow(self.spans[agent].id.clone()),
            None if self.spans[index].kind == SpanKind::Tool => {
                RevealTarget::ToolRow(self.spans[index].id.clone())
            }
            None => RevealTarget::UserMessage,
        }
    }

    /// The span that took longest among those that have ended and have
    /// nothing under them; the first one on a tie.
    pub fn slowest_leaf(&self) -> Option<&str> {
        let mut slowest: Option<(&Span, Duration)> = None;
        for span in &self.spans {
            if !span.children.is_empty() {
                continue;
            }
            let Some(took) = span.ended_duration() else {
                continue;
            };
            if slowest.is_none_or(|(_, longest)| took > longest) {
                slowest = Some((span, took));
            }
        }
        slowest.map(|(span, _)| span.id.as_str())
    }

    /// How far the waterfall reaches from the turn's start: at least the
    /// turn's own duration, and past it when a bar ends later. The agent's
    /// clock for the turn can be shorter than the wall time its spans
    /// were stamped in, so an axis sized from [`Turn::elapsed`] alone could
    /// leave the last bar off its end.
    pub fn extent(&self, now: Instant) -> Duration {
        let spans = self
            .spans
            .iter()
            .map(|span| self.offset(span.start) + span.duration(now));
        let waits = self
            .waits
            .iter()
            .map(|wait| self.offset(wait.end.unwrap_or(now)));
        spans.chain(waits).fold(self.elapsed(now), Duration::max)
    }

    /// The folded Trace card's title, `Live · 34s`.
    pub fn summary(&self, now: Instant) -> String {
        let elapsed = format_duration(self.elapsed(now));
        let cost = || {
            self.known_spend()
                .and_then(|spend| spend.cost_usd)
                .map(|cost| format!(" · ${cost:.2}"))
                .unwrap_or_default()
        };
        match self.state() {
            TurnState::Live => format!("Live · {elapsed}"),
            TurnState::Waiting => format!("Waiting for you · {elapsed}"),
            TurnState::Interrupted => format!("Interrupted · {elapsed}"),
            TurnState::Failed => format!("Failed · {elapsed}{}", cost()),
            TurnState::Done => format!("Done · {elapsed}{}", cost()),
        }
    }

    /// What the turn cost, when it is worth showing: an interrupted turn has
    /// none, whatever its `TurnEnded` carried, nor does a waiting turn whose
    /// main thread was interrupted.
    fn known_spend(&self) -> Option<&Spend> {
        let outcome = match &self.ended {
            Some(end) => Some(&end.outcome),
            None => self.main_outcome.as_ref(),
        };
        if matches!(outcome, Some(TurnOutcome::Interrupted)) {
            return None;
        }
        self.spend.as_ref()
    }

    fn offset(&self, at: Instant) -> Duration {
        at.saturating_duration_since(self.started)
    }

    /// Everything the views ask of how the spans nest and who owns what,
    /// in one pass over the spans and one over the waits.
    fn shape(&self) -> Shape {
        let count = self.spans.len();
        // The nearest subagent above each span, the agent it belongs to.
        // `None` is the root.
        let mut owner: Vec<Option<usize>> = Vec::with_capacity(count);
        let mut depth: Vec<u8> = Vec::with_capacity(count);
        let mut lane_depth: Vec<u8> = Vec::with_capacity(count);
        let mut lanes = vec![LaneFacts::default(); count + 1];
        // A span's parent was opened before it, so is already here.
        for (index, span) in self.spans.iter().enumerate() {
            let (above, deep) = match span.parent {
                None => (None, 1),
                Some(parent) => {
                    let above = if self.spans[parent].kind == SpanKind::Subagent {
                        Some(parent)
                    } else {
                        owner[parent]
                    };
                    (above, depth[parent].saturating_add(1))
                }
            };
            lane_depth.push(above.map_or(1, |above| lane_depth[above].saturating_add(1)));
            owner.push(above);
            depth.push(deep);

            let facts = &mut lanes[slot(above)];
            let open = span.end.is_none();
            // On a tie the later span is the latest.
            let newer = |best: Option<usize>| {
                best.is_none_or(|best| span.start >= self.spans[best].start)
            };
            match span.kind {
                SpanKind::Subagent => facts.running_agents += usize::from(open),
                SpanKind::Tool => {
                    facts.calls.push(index);
                    if open && newer(facts.running_tool) {
                        facts.running_tool = Some(index);
                    }
                }
                SpanKind::Llm => {
                    facts.calls.push(index);
                    if open && newer(facts.running_call) {
                        facts.running_call = Some(index);
                    }
                    if span.model.is_some() && newer(facts.latest_model) {
                        facts.latest_model = Some(index);
                    }
                }
            }
        }
        let mut waiting = vec![false; count];
        let mut root_waiting = false;
        for (index, wait) in self.waits.iter().enumerate() {
            lanes[slot(self.wait_lane(wait.span, &owner))].waits.push(index);
            if wait.end.is_some() {
                continue;
            }
            root_waiting = true;
            let mut cursor = wait.span;
            while let Some(span) = cursor {
                if waiting[span] {
                    break;
                }
                waiting[span] = true;
                cursor = self.spans[span].parent;
            }
        }
        Shape {
            depth,
            lane_depth,
            waiting,
            root_waiting,
            lanes,
        }
    }

    /// The agent a wait is listed on: the span's own when it is a subagent,
    /// else the one its call belongs to; the root when it waits on the root.
    fn wait_lane(&self, span: Option<usize>, owner: &[Option<usize>]) -> Option<usize> {
        let span = span?;
        if self.spans[span].kind == SpanKind::Subagent {
            Some(span)
        } else {
            owner[span]
        }
    }

    fn span_state(&self, index: usize, shape: &Shape) -> SpanState {
        match &self.spans[index].end {
            None if shape.waiting[index] => SpanState::Waiting,
            None => SpanState::Running,
            Some(end) => match end.outcome {
                SpanOutcome::Done => SpanState::Done,
                SpanOutcome::Failed { .. } => SpanState::Failed,
                SpanOutcome::Interrupted => SpanState::Interrupted,
            },
        }
    }

    fn root_state(&self) -> SpanState {
        match self.state() {
            TurnState::Live => SpanState::Running,
            TurnState::Waiting => SpanState::Waiting,
            TurnState::Done => SpanState::Done,
            TurnState::Failed => SpanState::Failed,
            TurnState::Interrupted => SpanState::Interrupted,
        }
    }

    /// The children of a span, or of the root, in the order they started.
    fn children_of(&self, parent: Option<usize>) -> Vec<usize> {
        let mut children = match parent {
            Some(parent) => self.spans[parent].children.clone(),
            None => self.root_children.clone(),
        };
        children.sort_by_key(|&child| (self.spans[child].start, child));
        children
    }

    fn preorder(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.spans.len());
        let mut pending: Vec<usize> = self.children_of(None).into_iter().rev().collect();
        while let Some(index) = pending.pop() {
            order.push(index);
            pending.extend(self.children_of(Some(index)).into_iter().rev());
        }
        order
    }

    /// Everything under a span, nearest first.
    fn descendants_of(&self, index: usize) -> Vec<usize> {
        let mut found = Vec::new();
        let mut pending = self.spans[index].children.clone();
        while let Some(next) = pending.pop() {
            found.push(next);
            pending.extend(self.spans[next].children.iter().copied());
        }
        found
    }

    fn marks_of(&self, hidden: &[usize]) -> Vec<Duration> {
        let mut marks: Vec<Duration> = hidden
            .iter()
            .map(|&index| self.offset(self.spans[index].start))
            .collect();
        marks.sort();
        marks
    }

    /// The model of the agent's latest model call that names one.
    fn latest_model(&self, lane: Option<usize>, shape: &Shape) -> Option<String> {
        let call = shape.lanes[slot(lane)].latest_model?;
        self.spans[call].model.clone()
    }

    /// What a row says beside its name: what the agent reported, else the
    /// model.
    fn detail_of(&self, index: usize, shape: &Shape) -> String {
        let span = &self.spans[index];
        if !span.detail.is_empty() {
            return span.detail.clone();
        }
        let model = match span.kind {
            SpanKind::Subagent => self.latest_model(Some(index), shape),
            SpanKind::Llm | SpanKind::Tool => span.model.clone(),
        };
        model.unwrap_or_default()
    }

    /// What a running agent is doing, the first of: a permission waits,
    /// subagents run, a tool runs, the model writes, and for a subagent the
    /// last thing it reported. The latest tool or call of its kind when
    /// several are open.
    fn now_row(&self, lane: Option<usize>, shape: &Shape, now: Instant) -> Option<String> {
        let (open, waiting) = match lane {
            None => (self.ended.is_none(), shape.root_waiting),
            Some(index) => (self.spans[index].end.is_none(), shape.waiting[index]),
        };
        if !open {
            return None;
        }
        if waiting {
            return Some("Waiting for you".to_string());
        }
        let facts = &shape.lanes[slot(lane)];
        if facts.running_agents > 0 {
            let agents = facts.running_agents;
            let s = if agents == 1 { "" } else { "s" };
            return Some(format!("{agents} subagent{s} running"));
        }
        let age = |span: &Span| format_elapsed(now.saturating_duration_since(span.start));
        if let Some(tool) = facts.running_tool.map(|tool| &self.spans[tool]) {
            return Some(format!("{} · {}", tool.name, age(tool)));
        }
        if let Some(call) = facts.running_call.map(|call| &self.spans[call]) {
            return Some(match &call.model {
                Some(model) => format!("Thinking · {model} · {}", age(call)),
                None => format!("Thinking · {}", age(call)),
            });
        }
        // A subagent between two calls has no open span; what it last
        // reported is the best that is known.
        lane.and_then(|index| self.spans[index].activity.clone())
    }

    fn waits_of(&self, lane: Option<usize>, shape: &Shape) -> Vec<LaneWait> {
        shape.lanes[slot(lane)]
            .waits
            .iter()
            .map(|&wait| &self.waits[wait])
            .filter(|wait| wait.end.is_none())
            .map(|wait| LaneWait {
                request_id: wait.request_id,
                tool: match wait.span {
                    Some(span) if self.spans[span].kind == SpanKind::Tool => {
                        self.spans[span].name.clone()
                    }
                    _ => String::new(),
                },
            })
            .collect()
    }

    /// The agent's calls in the order they started, then its waits, so a
    /// wait is drawn over the call it holds up.
    fn segments_of(&self, lane: Option<usize>, shape: &Shape, now: Instant) -> Vec<Segment> {
        let facts = &shape.lanes[slot(lane)];
        let mut segments: Vec<Segment> = facts
            .calls
            .iter()
            .filter_map(|&index| {
                let span = &self.spans[index];
                let start = self.offset(span.start);
                Some(Segment {
                    kind: span.segment_kind()?,
                    start,
                    end: start + span.duration(now),
                    state: self.span_state(index, shape),
                })
            })
            .collect();
        segments.sort_by_key(|segment| segment.start);
        let waits = facts.waits.iter().map(|&index| {
            let wait = &self.waits[index];
            let start = self.offset(wait.start);
            let end = self.offset(wait.end.unwrap_or(now));
            Segment {
                kind: SegmentKind::Wait,
                start,
                end: end.max(start),
                state: if wait.end.is_some() {
                    SpanState::Done
                } else {
                    SpanState::Waiting
                },
            }
        });
        segments.extend(waits);
        segments
    }
}

/// An axis from 0 over at least ten seconds, in the smallest round step that
/// divides it into at most four intervals, and as long as a whole number of
/// those steps.
pub fn axis(range: Duration) -> Axis {
    let wanted = u64::try_from(range.as_millis())
        .unwrap_or(u64::MAX / 2)
        .clamp(AXIS_MINIMUM_MS, u64::MAX / 2);
    let last = AXIS_STEPS[AXIS_STEPS.len() - 1] * 1000;
    let step = AXIS_STEPS
        .iter()
        .map(|seconds| seconds * 1000)
        .chain(std::iter::successors(Some(last * 2), |step| step.checked_mul(2)))
        .find(|step| wanted.div_ceil(*step) <= AXIS_INTERVALS)
        .unwrap_or(wanted);
    let intervals = wanted.div_ceil(step);
    Axis {
        range: Duration::from_millis(intervals * step),
        ticks: (0..=intervals)
            .map(|tick| Duration::from_millis(tick * step))
            .collect(),
    }
}

/// `34s` below a minute, `1m 02s` from a minute up. Whole seconds, rounded
/// down.
pub fn format_duration(d: Duration) -> String {
    let seconds = d.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

/// [`format_duration`] with a tenth of a second below a minute: `3.2s`.
fn format_elapsed(d: Duration) -> String {
    if d >= Duration::from_secs(60) {
        return format_duration(d);
    }
    let tenths = d.as_millis() / 100;
    format!("{}.{}s", tenths / 10, tenths % 10)
}

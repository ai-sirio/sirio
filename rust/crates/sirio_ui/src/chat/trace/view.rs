//! The views a [`Turn`] answers: its stats, lanes, tree rows, reveal target
//! and summary, and the axis the waterfall draws on.
//!
//! Every view is computed from the spans as they are at `now`. A turn that
//! has ended no longer reads `now`: each of its spans has an end, and the
//! agent's own clock wins over the instants wherever it reported one.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use sirio_acp::{SpanKind, SpanOutcome, TokenCount, TurnOutcome};

use super::{ROOT, ROOT_NAME, Span, SpanEnd, Turn, TurnEnd};

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

/// The six numbers of a turn's stats row.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnStats {
    pub duration: Duration,
    /// `None` until the turn's `result` says, and when it cannot say.
    pub cost_usd: Option<f64>,
    /// On the same terms as `cost_usd`.
    pub tokens: Option<TokenCount>,
    pub llm_calls: usize,
    pub tool_calls: usize,
    pub errors: usize,
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
}

/// A stretch of a lane's track, as offsets from the turn's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub kind: SegmentKind,
    pub start: Duration,
    pub end: Duration,
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
    /// The nearest subagent above each span, the agent it belongs to.
    /// `None` is the root.
    owner: Vec<Option<usize>>,
    /// How deep each span sits in the tree, the root's children at 1.
    depth: Vec<u8>,
    /// How deep each span's agent lane sits, the first subagents at 1.
    lane_depth: Vec<u8>,
    /// Whether the span, or something under it, has an open wait.
    waiting: Vec<bool>,
    root_waiting: bool,
}

impl Span {
    fn is_call(&self) -> bool {
        matches!(self.kind, SpanKind::Tool | SpanKind::Llm)
    }

    /// The agent's own clock when it reported one.
    fn duration(&self, now: Instant) -> Duration {
        match &self.end {
            Some(end) => end
                .measured
                .unwrap_or_else(|| end.at.saturating_duration_since(self.start)),
            None => now.saturating_duration_since(self.start),
        }
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
        let mut stats = TurnStats {
            duration: self.elapsed(now),
            cost_usd: self.ended.as_ref().and_then(|end| end.cost_usd),
            tokens: self.ended.as_ref().and_then(|end| end.tokens.clone()),
            llm_calls: 0,
            tool_calls: 0,
            errors: 0,
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
            calls: self.calls_of(None, &shape),
            duration: self.elapsed(now),
            now: self.now_row(None, &shape, now),
            waits: self.waits_of(None, &shape),
            segments: self.segments_of(None, &shape, now),
        };
        let agents = self
            .preorder()
            .into_iter()
            .filter(|&index| self.spans[index].kind == SpanKind::Subagent)
            .map(|index| {
                let span = &self.spans[index];
                Lane {
                    span: span.id.clone(),
                    depth: shape.lane_depth[index],
                    name: span.name.clone(),
                    detail: self.detail_of(index, &shape),
                    state: self.span_state(index, &shape),
                    calls: self.calls_of(Some(index), &shape),
                    duration: span.duration(now),
                    now: self.now_row(Some(index), &shape, now),
                    waits: self.waits_of(Some(index), &shape),
                    segments: self.segments_of(Some(index), &shape, now),
                }
            });
        std::iter::once(root).chain(agents).collect()
    }

    /// The tree: the root, then its spans depth-first, each level in the
    /// order its spans started. A span in `folded` hides everything under it.
    pub fn rows(&self, folded: &HashSet<String>, now: Instant) -> Vec<TreeRow> {
        let shape = self.shape();
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
            if !span.children.is_empty() || span.end.is_none() {
                continue;
            }
            // `now` is not read: an ended span has its end.
            let took = span.duration(self.started);
            if slowest.is_none_or(|(_, longest)| took > longest) {
                slowest = Some((span, took));
            }
        }
        slowest.map(|(span, _)| span.id.as_str())
    }

    /// The folded Trace card's title, `Live · 34s`.
    pub fn summary(&self, now: Instant) -> String {
        let elapsed = format_duration(self.elapsed(now));
        let cost = || {
            self.ended
                .as_ref()
                .and_then(|end| end.cost_usd)
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

    fn offset(&self, at: Instant) -> Duration {
        at.saturating_duration_since(self.started)
    }

    fn shape(&self) -> Shape {
        let mut owner: Vec<Option<usize>> = Vec::with_capacity(self.spans.len());
        let mut depth: Vec<u8> = Vec::with_capacity(self.spans.len());
        let mut lane_depth: Vec<u8> = Vec::with_capacity(self.spans.len());
        // A span's parent was opened before it, so is already here.
        for span in &self.spans {
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
        }
        let mut waiting = vec![false; self.spans.len()];
        let mut root_waiting = false;
        for wait in self.waits.iter().filter(|wait| wait.end.is_none()) {
            root_waiting = true;
            let mut cursor = wait.span;
            while let Some(index) = cursor {
                if waiting[index] {
                    break;
                }
                waiting[index] = true;
                cursor = self.spans[index].parent;
            }
        }
        Shape {
            owner,
            depth,
            lane_depth,
            waiting,
            root_waiting,
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
        self.spans
            .iter()
            .enumerate()
            .filter(|(index, span)| span.kind == SpanKind::Llm && shape.owner[*index] == lane)
            .filter_map(|(_, span)| Some((span.start, span.model.as_ref()?)))
            .max_by_key(|(start, _)| *start)
            .map(|(_, model)| model.clone())
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

    fn calls_of(&self, lane: Option<usize>, shape: &Shape) -> usize {
        self.spans
            .iter()
            .enumerate()
            .filter(|(index, span)| span.is_call() && shape.owner[*index] == lane)
            .count()
    }

    /// The agent's open spans of one kind.
    fn running<'a>(
        &'a self,
        lane: Option<usize>,
        kind: SpanKind,
        shape: &'a Shape,
    ) -> impl Iterator<Item = &'a Span> + 'a {
        self.spans
            .iter()
            .enumerate()
            .filter(move |(index, span)| {
                span.end.is_none() && span.kind == kind && shape.owner[*index] == lane
            })
            .map(|(_, span)| span)
    }

    /// What a running agent is doing, the first of: a permission waits,
    /// subagents run, a tool runs, the model writes. The latest tool or
    /// call of its kind when several are open.
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
        let agents = self.running(lane, SpanKind::Subagent, shape).count();
        if agents > 0 {
            let s = if agents == 1 { "" } else { "s" };
            return Some(format!("{agents} subagent{s} running"));
        }
        let age = |span: &Span| format_elapsed(now.saturating_duration_since(span.start));
        if let Some(tool) = self
            .running(lane, SpanKind::Tool, shape)
            .max_by_key(|span| span.start)
        {
            return Some(format!("{} · {}", tool.name, age(tool)));
        }
        let call = self
            .running(lane, SpanKind::Llm, shape)
            .max_by_key(|span| span.start)?;
        Some(match &call.model {
            Some(model) => format!("Thinking · {model} · {}", age(call)),
            None => format!("Thinking · {}", age(call)),
        })
    }

    /// The agent a wait is listed on: the span's own when it is a subagent,
    /// else the one its call belongs to; the root when it waits on the root.
    fn wait_lane(&self, span: Option<usize>, shape: &Shape) -> Option<usize> {
        let span = span?;
        if self.spans[span].kind == SpanKind::Subagent {
            Some(span)
        } else {
            shape.owner[span]
        }
    }

    fn waits_of(&self, lane: Option<usize>, shape: &Shape) -> Vec<LaneWait> {
        self.waits
            .iter()
            .filter(|wait| wait.end.is_none() && self.wait_lane(wait.span, shape) == lane)
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
        let mut segments: Vec<Segment> = self
            .spans
            .iter()
            .enumerate()
            .filter(|(index, span)| span.is_call() && shape.owner[*index] == lane)
            .map(|(_, span)| {
                let start = self.offset(span.start);
                Segment {
                    kind: match span.kind {
                        SpanKind::Llm => SegmentKind::Llm,
                        SpanKind::Tool | SpanKind::Subagent => SegmentKind::Tool,
                    },
                    start,
                    end: start + span.duration(now),
                }
            })
            .collect();
        segments.sort_by_key(|segment| segment.start);
        let waits = self
            .waits
            .iter()
            .filter(|wait| self.wait_lane(wait.span, shape) == lane)
            .map(|wait| {
                let start = self.offset(wait.start);
                let end = self.offset(wait.end.unwrap_or(now));
                Segment {
                    kind: SegmentKind::Wait,
                    start,
                    end: end.max(start),
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

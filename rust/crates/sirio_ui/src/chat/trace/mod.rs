//! A chat's trace: the turns and spans its native Claude transport reported,
//! and every view the Trace card and tab draw from them.
//!
//! Pure, with no gpui: every instant comes in through an event or an
//! argument, never from a clock, so what a turn looks like is a function of
//! what it was told. `sirio_acp::TraceEvent` documents what the producer
//! closes on its own; what it leaves to this store is:
//! - a span still open when its turn ends becomes `Interrupted`, never `Done`,
//!   whatever the turn's outcome;
//! - a turn with no `TurnEnded` (a transport error, the CLI's death, the
//!   prompt timeout) is ended by [`TraceStore::fail_open_turn`];
//! - span, wait and `TurnEnded` events that arrive with no turn open belong to
//!   a turn Sirio did not start, and are ignored.
//!
//! Protocol data degrades, it never rejects: a span whose parent is unknown
//! hangs under the root, and an event that names a span nobody opened is
//! ignored.

use std::collections::{HashMap, VecDeque};
use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use sirio_acp::{SpanKind, SpanOutcome, TokenCount, TraceEvent, TurnOutcome};

mod view;

pub use view::{
    Axis, Lane, LaneWait, RevealTarget, Segment, SegmentKind, SpanState, TreeRow, TurnState,
    TurnStats, axis, format_duration,
};

/// The id of every turn's root span, the main Claude Code thread. Tool ids
/// start `toolu_` and message ids `msg_`, so no span of the agent's can
/// collide with it.
pub const ROOT: &str = "root";
/// The root lane's name.
pub const ROOT_NAME: &str = "Claude Code";
/// The turns a chat keeps; the oldest is let go.
const MAX_TURNS: usize = 20;
/// The spans a turn keeps. Past this a span is counted, not kept.
const MAX_SPANS: usize = 2000;
/// The longest a turn's title runs, in characters.
const TITLE_LIMIT: usize = 200;
/// Why a turn that was still open when the next one started ended.
const ABANDONED: &str = "The turn ended without a result";

/// The turns of one chat, oldest first.
#[derive(Clone, Debug)]
pub struct TraceStore {
    turns: VecDeque<Turn>,
    /// The 1-based index of the oldest kept turn.
    first: usize,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self {
            turns: VecDeque::new(),
            first: 1,
        }
    }
}

impl TraceStore {
    /// Folds one event into the latest turn, or opens a turn.
    pub fn apply(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::TurnStarted { at } => self.start_turn(at),
            TraceEvent::TurnEnded {
                at,
                outcome,
                measured_ms,
                cost_usd,
                tokens,
            } => {
                if let Some(turn) = self.open_turn() {
                    let measured = measured_ms.map(Duration::from_millis);
                    turn.close(at, outcome, measured, cost_usd, tokens);
                }
            }
            event => {
                if let Some(turn) = self.open_turn() {
                    turn.apply(event);
                }
            }
        }
    }

    /// Ends the open turn `Failed` with `reason`, as a transport error, the
    /// CLI's death or the prompt timeout does: none of them brings a
    /// `TurnEnded`. Nothing happens when no turn is open.
    pub fn fail_open_turn(&mut self, reason: &str, at: Instant) {
        if let Some(turn) = self.open_turn() {
            let outcome = TurnOutcome::Failed {
                reason: reason.to_string(),
            };
            turn.close(at, outcome, None, None, None);
        }
    }

    /// Names the latest turn after the prompt that started it: the first line
    /// that says anything.
    pub fn set_latest_title(&mut self, title: &str) {
        if let Some(turn) = self.turns.back_mut() {
            turn.title = title
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .chars()
                .take(TITLE_LIMIT)
                .collect();
        }
    }

    pub fn latest(&self) -> Option<&Turn> {
        self.turns.back()
    }

    /// A kept turn by its 1-based index in the chat.
    pub fn turn(&self, index: usize) -> Option<&Turn> {
        self.turns.get(index.checked_sub(self.first)?)
    }

    /// The indices [`TraceStore::turn`] answers for. Empty when there is no
    /// turn.
    pub fn turn_indices(&self) -> RangeInclusive<usize> {
        self.first..=(self.first + self.turns.len()).saturating_sub(1)
    }

    /// Whether the latest turn has not ended.
    pub fn is_running(&self) -> bool {
        self.latest().is_some_and(|turn| turn.ended.is_none())
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    fn open_turn(&mut self) -> Option<&mut Turn> {
        self.turns.back_mut().filter(|turn| turn.ended.is_none())
    }

    /// A prompt was written. A turn still open when the next starts never
    /// got its `result`: it ends failed, then the new one opens.
    fn start_turn(&mut self, at: Instant) {
        if let Some(turn) = self.open_turn() {
            let outcome = TurnOutcome::Failed {
                reason: ABANDONED.to_string(),
            };
            turn.close(at, outcome, None, None, None);
        }
        let index = self.first + self.turns.len();
        self.turns.push_back(Turn::new(index, at));
        if self.turns.len() > MAX_TURNS {
            self.turns.pop_front();
            self.first += 1;
        }
    }
}

/// One turn: the spans between a prompt and its end.
#[derive(Clone, Debug)]
pub struct Turn {
    index: usize,
    title: String,
    started: Instant,
    ended: Option<TurnEnd>,
    /// In arrival order, so a span's parent is always before it.
    spans: Vec<Span>,
    by_id: HashMap<String, usize>,
    /// The spans that hang under the root.
    root_children: Vec<usize>,
    /// Every wait of the turn, in arrival order.
    waits: Vec<Wait>,
    dropped: usize,
}

#[derive(Clone, Debug)]
struct TurnEnd {
    at: Instant,
    outcome: TurnOutcome,
    /// The agent's own clock for the turn.
    measured: Option<Duration>,
    cost_usd: Option<f64>,
    tokens: Option<TokenCount>,
}

#[derive(Clone, Debug)]
struct Span {
    id: String,
    parent: Option<usize>,
    children: Vec<usize>,
    kind: SpanKind,
    name: String,
    detail: String,
    model: Option<String>,
    start: Instant,
    end: Option<SpanEnd>,
}

#[derive(Clone, Debug)]
struct SpanEnd {
    at: Instant,
    outcome: SpanOutcome,
    /// The agent's own clock for the span, which wins over the instants.
    measured: Option<Duration>,
}

/// A permission asked of the user.
#[derive(Clone, Debug)]
struct Wait {
    request_id: u64,
    /// The span it waits on, the root when `None`.
    span: Option<usize>,
    start: Instant,
    end: Option<Instant>,
}

impl Turn {
    fn new(index: usize, started: Instant) -> Self {
        Self {
            index,
            title: String::new(),
            started,
            ended: None,
            spans: Vec::new(),
            by_id: HashMap::new(),
            root_children: Vec::new(),
            waits: Vec::new(),
            dropped: 0,
        }
    }

    /// 1-based, per chat.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The first line of the prompt, or empty before the chat names it.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Waiting while any wait is open, whatever else runs.
    pub fn state(&self) -> TurnState {
        match &self.ended {
            None if self.waits.iter().any(|wait| wait.end.is_none()) => TurnState::Waiting,
            None => TurnState::Live,
            Some(end) => match end.outcome {
                TurnOutcome::Done => TurnState::Done,
                TurnOutcome::Failed { .. } => TurnState::Failed,
                TurnOutcome::Interrupted => TurnState::Interrupted,
            },
        }
    }

    /// How long the turn has run: the agent's own clock once it has ended
    /// and reported one, else the instants.
    pub fn elapsed(&self, now: Instant) -> Duration {
        match &self.ended {
            Some(end) => end
                .measured
                .unwrap_or_else(|| end.at.saturating_duration_since(self.started)),
            None => now.saturating_duration_since(self.started),
        }
    }

    /// The spans past the cap this turn did not keep.
    pub fn dropped_spans(&self) -> usize {
        self.dropped
    }

    fn apply(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::SpanStarted {
                id,
                parent,
                kind,
                name,
                detail,
                model,
                at,
            } => self.insert_span(
                parent.as_deref(),
                Span {
                    id,
                    parent: None,
                    children: Vec::new(),
                    kind,
                    name,
                    detail,
                    model,
                    start: at,
                    end: None,
                },
            ),
            TraceEvent::SpanNamed { id, name, detail } => {
                if let Some(&span) = self.by_id.get(&id) {
                    let span = &mut self.spans[span];
                    if !name.is_empty() {
                        span.name = name;
                    }
                    if !detail.is_empty() {
                        span.detail = detail;
                    }
                }
            }
            TraceEvent::WaitStarted {
                span,
                request_id,
                at,
            } => self.open_wait(span, request_id, at),
            TraceEvent::WaitEnded { request_id, at } => {
                let open = self
                    .waits
                    .iter_mut()
                    .find(|wait| wait.request_id == request_id && wait.end.is_none());
                if let Some(wait) = open {
                    wait.end = Some(at);
                }
            }
            TraceEvent::SpanEnded {
                id,
                outcome,
                at,
                measured_ms,
            } => self.end_span(&id, outcome, at, measured_ms.map(Duration::from_millis)),
            // The views read what a span holds, not the totals the agent
            // reports while it runs: a model call's tokens and a subagent's
            // progress are not drawn.
            TraceEvent::SpanUsage { .. } | TraceEvent::SpanProgress { .. } => {}
            // The store itself opens and ends turns.
            TraceEvent::TurnStarted { .. } | TraceEvent::TurnEnded { .. } => {}
        }
    }

    /// Keeps `span` under `parent`, or counts it when the turn is full.
    fn insert_span(&mut self, parent: Option<&str>, mut span: Span) {
        if self.by_id.contains_key(&span.id) {
            return;
        }
        if self.spans.len() >= MAX_SPANS {
            self.dropped += 1;
            return;
        }
        // A parent nobody opened leaves the span under the root.
        span.parent = parent.and_then(|parent| self.by_id.get(parent).copied());
        let index = self.spans.len();
        match span.parent {
            Some(parent) => self.spans[parent].children.push(index),
            None => self.root_children.push(index),
        }
        self.by_id.insert(span.id.clone(), index);
        self.spans.push(span);
    }

    fn open_wait(&mut self, span: Option<String>, request_id: u64, at: Instant) {
        if self.waits.iter().any(|wait| wait.request_id == request_id) {
            return;
        }
        // A span nobody kept is waited on at the root: the user still has
        // to answer.
        let span = span
            .as_deref()
            .and_then(|span| self.by_id.get(span).copied());
        self.waits.push(Wait {
            request_id,
            span,
            start: at,
            end: None,
        });
    }

    fn end_span(
        &mut self,
        id: &str,
        outcome: SpanOutcome,
        at: Instant,
        measured: Option<Duration>,
    ) {
        let Some(&index) = self.by_id.get(id) else {
            return;
        };
        if self.spans[index].end.is_some() {
            return;
        }
        self.spans[index].end = Some(SpanEnd {
            at,
            outcome,
            measured,
        });
        // A span that has ended is waiting on nothing, whether or not the
        // wait's own end ever comes.
        for wait in &mut self.waits {
            if wait.span == Some(index) && wait.end.is_none() {
                wait.end = Some(at);
            }
        }
    }

    /// Ends the turn at `at`. Whatever is still open at that moment was
    /// abandoned by it: its spans are interrupted and its waits end.
    fn close(
        &mut self,
        at: Instant,
        outcome: TurnOutcome,
        measured: Option<Duration>,
        cost_usd: Option<f64>,
        tokens: Option<TokenCount>,
    ) {
        for span in &mut self.spans {
            if span.end.is_none() {
                span.end = Some(SpanEnd {
                    at,
                    outcome: SpanOutcome::Interrupted,
                    measured: None,
                });
            }
        }
        for wait in &mut self.waits {
            if wait.end.is_none() {
                wait.end = Some(at);
            }
        }
        self.ended = Some(TurnEnd {
            at,
            outcome,
            measured,
            cost_usd,
            tokens,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every instant a test names is an offset from one origin: the store
    /// never reads a clock, so neither do these.
    struct Clock(Instant);

    impl Clock {
        fn at(&self, ms: u64) -> Instant {
            self.0 + Duration::from_millis(ms)
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// A store whose first turn started at offset 0 and is still open.
    fn open_turn() -> (TraceStore, Clock) {
        let clock = Clock(Instant::now());
        let mut store = TraceStore::default();
        store.apply(TraceEvent::TurnStarted { at: clock.at(0) });
        (store, clock)
    }

    fn start(
        store: &mut TraceStore,
        c: &Clock,
        id: &str,
        parent: Option<&str>,
        kind: SpanKind,
        name: &str,
        at: u64,
    ) {
        store.apply(TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(Into::into),
            kind,
            name: name.into(),
            detail: String::new(),
            model: None,
            at: c.at(at),
        });
    }

    fn tool(
        store: &mut TraceStore,
        c: &Clock,
        id: &str,
        parent: Option<&str>,
        name: &str,
        at: u64,
    ) {
        start(store, c, id, parent, SpanKind::Tool, name, at);
    }

    fn agent(
        store: &mut TraceStore,
        c: &Clock,
        id: &str,
        parent: Option<&str>,
        name: &str,
        detail: &str,
        at: u64,
    ) {
        store.apply(TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(Into::into),
            kind: SpanKind::Subagent,
            name: name.into(),
            detail: detail.into(),
            model: None,
            at: c.at(at),
        });
    }

    fn call(
        store: &mut TraceStore,
        c: &Clock,
        id: &str,
        parent: Option<&str>,
        model: &str,
        at: u64,
    ) {
        store.apply(TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(Into::into),
            kind: SpanKind::Llm,
            name: "Model call".into(),
            detail: String::new(),
            model: Some(model.into()),
            at: c.at(at),
        });
    }

    fn end(store: &mut TraceStore, c: &Clock, id: &str, at: u64) {
        end_as(store, c, id, SpanOutcome::Done, at);
    }

    fn end_as(store: &mut TraceStore, c: &Clock, id: &str, outcome: SpanOutcome, at: u64) {
        store.apply(TraceEvent::SpanEnded {
            id: id.into(),
            outcome,
            at: c.at(at),
            measured_ms: None,
        });
    }

    fn wait(store: &mut TraceStore, c: &Clock, span: Option<&str>, request_id: u64, at: u64) {
        store.apply(TraceEvent::WaitStarted {
            span: span.map(Into::into),
            request_id,
            at: c.at(at),
        });
    }

    fn unwait(store: &mut TraceStore, c: &Clock, request_id: u64, at: u64) {
        store.apply(TraceEvent::WaitEnded {
            request_id,
            at: c.at(at),
        });
    }

    fn finish_with(
        store: &mut TraceStore,
        c: &Clock,
        at: u64,
        outcome: TurnOutcome,
        measured_ms: Option<u64>,
        cost_usd: Option<f64>,
        tokens: Option<TokenCount>,
    ) {
        store.apply(TraceEvent::TurnEnded {
            at: c.at(at),
            outcome,
            measured_ms,
            cost_usd,
            tokens,
        });
    }

    fn finish(store: &mut TraceStore, c: &Clock, at: u64, outcome: TurnOutcome) {
        finish_with(store, c, at, outcome, None, None, None);
    }

    fn rows_of(store: &TraceStore, now: Instant) -> Vec<TreeRow> {
        store
            .latest()
            .expect("a turn")
            .rows(&HashSet::new(), now)
    }

    fn row<'a>(rows: &'a [TreeRow], id: &str) -> &'a TreeRow {
        rows.iter()
            .find(|row| row.span == id)
            .unwrap_or_else(|| panic!("no row for {id}: {:?}", ids(rows)))
    }

    fn ids(rows: &[TreeRow]) -> Vec<&str> {
        rows.iter().map(|row| row.span.as_str()).collect()
    }

    fn reveal(store: &TraceStore, span: &str) -> RevealTarget {
        store.latest().expect("a turn").reveal_target(span)
    }

    fn lanes_of(store: &TraceStore, now: Instant) -> Vec<Lane> {
        store.latest().expect("a turn").lanes(now)
    }

    fn lane<'a>(lanes: &'a [Lane], id: &str) -> &'a Lane {
        lanes
            .iter()
            .find(|lane| lane.span == id)
            .unwrap_or_else(|| panic!("no lane for {id}"))
    }

    #[test]
    fn an_open_span_at_turn_end_is_interrupted_not_done() {
        let outcomes = [
            TurnOutcome::Done,
            TurnOutcome::Failed {
                reason: "boom".into(),
            },
            TurnOutcome::Interrupted,
        ];
        for outcome in outcomes {
            let (mut store, c) = open_turn();
            tool(&mut store, &c, "toolu_done", None, "Read", 50);
            end(&mut store, &c, "toolu_done", 90);
            tool(&mut store, &c, "toolu_a", None, "Bash", 100);
            agent(&mut store, &c, "toolu_x", None, "reviewer", "", 200);
            tool(&mut store, &c, "toolu_b", Some("toolu_x"), "Read", 300);
            call(&mut store, &c, "msg_1", None, "claude-haiku-5-5", 400);
            finish(&mut store, &c, 1000, outcome.clone());

            let found = rows_of(&store, c.at(5000));
            for id in ["toolu_a", "toolu_x", "toolu_b", "msg_1"] {
                let open = row(&found, id);
                assert_eq!(open.state, SpanState::Interrupted, "{id} under {outcome:?}");
                assert_eq!(open.end, Some(ms(1000)), "{id} ends with the turn");
            }
            let done = row(&found, "toolu_done");
            assert_eq!(done.state, SpanState::Done, "a closed span keeps its outcome");
            assert_eq!(done.end, Some(ms(90)));
            let lanes = lanes_of(&store, c.at(5000));
            assert_eq!(lane(&lanes, "toolu_x").state, SpanState::Interrupted);
        }
    }

    #[test]
    fn a_turn_with_no_result_ends_failed_on_transport_error() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        call(&mut store, &c, "msg_1", None, "claude-haiku-5-5", 200);
        wait(&mut store, &c, Some("toolu_a"), 7, 300);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Waiting);

        store.fail_open_turn("Agent disconnected", c.at(900));

        let turn = store.latest().expect("a turn");
        assert_eq!(turn.state(), TurnState::Failed);
        assert!(!store.is_running(), "a failed turn must stop running");
        assert_eq!(turn.elapsed(c.at(60_000)), ms(900));
        let found = rows_of(&store, c.at(60_000));
        assert_eq!(row(&found, "toolu_a").state, SpanState::Interrupted);
        assert_eq!(row(&found, "msg_1").state, SpanState::Interrupted);
        assert_eq!(
            row(&found, ROOT).note.as_deref(),
            Some("Agent disconnected"),
            "the root row carries the transcript's reason"
        );
        let lanes = lanes_of(&store, c.at(60_000));
        assert!(lanes[0].waits.is_empty(), "no wait outlives the turn");
        assert_eq!(lanes[0].state, SpanState::Failed);

        // The turn is over: a second failure changes nothing.
        store.fail_open_turn("Agent timed out", c.at(1500));
        assert_eq!(turn_reason(&store), Some("Agent disconnected".to_string()));
        assert_eq!(store.latest().expect("a turn").elapsed(c.at(9000)), ms(900));

        // And with no turn at all there is nothing to fail.
        let mut empty = TraceStore::default();
        empty.fail_open_turn("Agent disconnected", c.at(0));
        assert!(empty.is_empty());
    }

    fn turn_reason(store: &TraceStore) -> Option<String> {
        let now = Instant::now();
        rows_of(store, now)
            .into_iter()
            .find(|row| row.span == ROOT)
            .and_then(|row| row.note)
    }

    #[test]
    fn the_turn_waits_while_any_span_waits() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        tool(&mut store, &c, "toolu_b", None, "Write", 150);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Live);

        wait(&mut store, &c, Some("toolu_a"), 1, 200);
        let turn = store.latest().expect("a turn");
        assert_eq!(turn.state(), TurnState::Waiting);
        assert_eq!(turn.summary(c.at(300)), "Waiting for you · 0s");
        let found = rows_of(&store, c.at(300));
        assert_eq!(row(&found, "toolu_a").state, SpanState::Waiting);
        assert_eq!(row(&found, "toolu_b").state, SpanState::Running);
        assert_eq!(row(&found, ROOT).state, SpanState::Waiting);

        wait(&mut store, &c, Some("toolu_b"), 2, 250);
        unwait(&mut store, &c, 1, 400);
        assert_eq!(
            store.latest().expect("a turn").state(),
            TurnState::Waiting,
            "one wait is still open"
        );

        unwait(&mut store, &c, 2, 500);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Live);

        // A span that ends under an open wait is not waiting any more, even
        // if the wait's own end never came.
        wait(&mut store, &c, Some("toolu_a"), 3, 600);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Waiting);
        end(&mut store, &c, "toolu_a", 700);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Live);
    }

    #[test]
    fn lanes_list_the_root_then_subagents_in_tree_order_with_depth() {
        let (mut store, c) = open_turn();
        call(&mut store, &c, "msg_1", None, "claude-opus-5-5", 0);
        end(&mut store, &c, "msg_1", 500);
        tool(&mut store, &c, "toolu_r", None, "Read", 500);
        end(&mut store, &c, "toolu_r", 800);
        agent(&mut store, &c, "toolu_a", None, "reviewer", "Review the diff", 800);
        agent(&mut store, &c, "toolu_b", None, "tester", "", 810);
        call(&mut store, &c, "msg_b1", Some("toolu_b"), "claude-sonnet-5-5", 820);
        end(&mut store, &c, "msg_b1", 900);
        call(&mut store, &c, "msg_a1", Some("toolu_a"), "claude-haiku-5-5", 800);
        end(&mut store, &c, "msg_a1", 900);
        tool(&mut store, &c, "toolu_a1", Some("toolu_a"), "Read", 900);
        end(&mut store, &c, "toolu_a1", 1000);
        // Opened after toolu_b, yet it belongs to toolu_a and is listed
        // before toolu_b: the tree's order is the tree's, not arrival's.
        agent(&mut store, &c, "toolu_n", Some("toolu_a"), "explorer", "Find callers", 950);
        tool(&mut store, &c, "toolu_n1", Some("toolu_n"), "Grep", 960);
        store.apply(TraceEvent::SpanEnded {
            id: "toolu_a".into(),
            outcome: SpanOutcome::Done,
            at: c.at(3000),
            measured_ms: Some(1234),
        });

        let lanes = lanes_of(&store, c.at(4000));
        let spans: Vec<&str> = lanes.iter().map(|lane| lane.span.as_str()).collect();
        assert_eq!(spans, [ROOT, "toolu_a", "toolu_n", "toolu_b"]);
        let depths: Vec<u8> = lanes.iter().map(|lane| lane.depth).collect();
        assert_eq!(depths, [0, 1, 2, 1]);
        let names: Vec<&str> = lanes.iter().map(|lane| lane.name.as_str()).collect();
        assert_eq!(names, ["Claude Code", "reviewer", "explorer", "tester"]);
        // The root shows its model, a subagent its description, and one
        // with no description its model.
        let details: Vec<&str> = lanes.iter().map(|lane| lane.detail.as_str()).collect();
        assert_eq!(
            details,
            [
                "claude-opus-5-5",
                "Review the diff",
                "Find callers",
                "claude-sonnet-5-5"
            ]
        );
        // Calls are the lane's own tools and model calls: a subagent is its
        // own lane, not a call of its parent's.
        let calls: Vec<usize> = lanes.iter().map(|lane| lane.calls).collect();
        assert_eq!(calls, [2, 2, 1, 1]);
        // The agent's own clock wins over the instants for a subagent's
        // duration.
        assert_eq!(lane(&lanes, "toolu_a").duration, ms(1234));
        assert_eq!(lane(&lanes, "toolu_b").duration, ms(4000 - 810));
        assert_eq!(lanes[0].duration, ms(4000));
        // A lane's track holds its own calls, as offsets from the turn's start.
        assert_eq!(
            lane(&lanes, "toolu_a").segments,
            vec![
                Segment {
                    kind: SegmentKind::Llm,
                    start: ms(800),
                    end: ms(900)
                },
                Segment {
                    kind: SegmentKind::Tool,
                    start: ms(900),
                    end: ms(1000)
                },
            ]
        );
    }

    #[test]
    fn the_now_row_prefers_the_wait_then_subagents_then_tool_then_model() {
        let (mut store, c) = open_turn();
        call(&mut store, &c, "msg_1", None, "claude-haiku-5-5", 1000);
        tool(&mut store, &c, "toolu_bash", None, "Bash", 1900);
        let now = c.at(5100);
        let root_now = |store: &TraceStore| lanes_of(store, now)[0].now.clone();

        assert_eq!(root_now(&store).as_deref(), Some("Bash · 3.2s"));

        agent(&mut store, &c, "toolu_a", None, "reviewer", "", 2500);
        agent(&mut store, &c, "toolu_b", None, "tester", "", 2600);
        assert_eq!(root_now(&store).as_deref(), Some("2 subagents running"));

        // A subagent's own lane names what that agent is doing.
        tool(&mut store, &c, "toolu_read", Some("toolu_a"), "Read", 4000);
        call(&mut store, &c, "msg_b1", Some("toolu_b"), "claude-sonnet-5-5", 3100);
        let lanes = lanes_of(&store, now);
        assert_eq!(lane(&lanes, "toolu_a").now.as_deref(), Some("Read · 1.1s"));
        assert_eq!(
            lane(&lanes, "toolu_b").now.as_deref(),
            Some("Thinking · claude-sonnet-5-5 · 2.0s")
        );

        wait(&mut store, &c, Some("toolu_bash"), 1, 3000);
        assert_eq!(root_now(&store).as_deref(), Some("Waiting for you"));

        unwait(&mut store, &c, 1, 3500);
        assert_eq!(root_now(&store).as_deref(), Some("2 subagents running"));

        end(&mut store, &c, "toolu_read", 4500);
        end(&mut store, &c, "toolu_a", 4600);
        assert_eq!(root_now(&store).as_deref(), Some("1 subagent running"));
        let lanes = lanes_of(&store, now);
        assert_eq!(lane(&lanes, "toolu_a").now, None, "a finished lane says nothing");

        end(&mut store, &c, "msg_b1", 4700);
        end(&mut store, &c, "toolu_b", 4800);
        assert_eq!(root_now(&store).as_deref(), Some("Bash · 3.2s"));

        end(&mut store, &c, "toolu_bash", 4900);
        assert_eq!(
            root_now(&store).as_deref(),
            Some("Thinking · claude-haiku-5-5 · 4.1s")
        );

        end(&mut store, &c, "msg_1", 5000);
        assert_eq!(root_now(&store), None, "nothing is running between calls");

        // A call with no model named still reads as a call.
        start(&mut store, &c, "msg_2", None, SpanKind::Llm, "Model call", 1000);
        assert_eq!(root_now(&store).as_deref(), Some("Thinking · 4.1s"));

        // From a minute up a call's age reads like the summary's.
        tool(&mut store, &c, "toolu_slow", None, "Bash", 0);
        assert_eq!(
            lanes_of(&store, c.at(65_000))[0].now.as_deref(),
            Some("Bash · 1m 05s")
        );

        // Once the turn is over no lane is doing anything.
        finish(&mut store, &c, 6000, TurnOutcome::Done);
        assert!(lanes_of(&store, now).iter().all(|lane| lane.now.is_none()));
    }

    #[test]
    fn stats_show_no_cost_or_tokens_until_known() {
        let (mut store, c) = open_turn();
        call(&mut store, &c, "msg_1", None, "claude-haiku-5-5", 0);
        end(&mut store, &c, "msg_1", 500);
        tool(&mut store, &c, "toolu_a", None, "Read", 500);
        end(&mut store, &c, "toolu_a", 600);
        tool(&mut store, &c, "toolu_b", None, "Bash", 600);
        end_as(
            &mut store,
            &c,
            "toolu_b",
            SpanOutcome::Failed {
                note: "exit 1".into(),
            },
            700,
        );
        agent(&mut store, &c, "toolu_x", None, "reviewer", "", 700);
        call(&mut store, &c, "msg_x1", Some("toolu_x"), "claude-haiku-5-5", 700);
        end(&mut store, &c, "msg_x1", 800);
        tool(&mut store, &c, "toolu_x1", Some("toolu_x"), "Grep", 800);
        end(&mut store, &c, "toolu_x1", 900);

        let live = store.latest().expect("a turn").stats(c.at(5000));
        assert_eq!(live.duration, secs(5));
        assert_eq!(live.cost_usd, None);
        assert_eq!(live.tokens, None);
        assert_eq!(live.llm_calls, 2);
        assert_eq!(live.tool_calls, 3, "a subagent is not a tool call");
        assert_eq!(live.errors, 1);

        // A turn whose spend cannot be told apart still shows none.
        finish(&mut store, &c, 6000, TurnOutcome::Done);
        let unknown = store.latest().expect("a turn").stats(c.at(9000));
        assert_eq!(unknown.duration, secs(6));
        assert_eq!(unknown.cost_usd, None);
        assert_eq!(unknown.tokens, None);

        store.apply(TraceEvent::TurnStarted { at: c.at(7000) });
        finish_with(
            &mut store,
            &c,
            9000,
            TurnOutcome::Done,
            Some(1800),
            Some(0.31),
            Some(TokenCount {
                input: 1200,
                output: 80,
            }),
        );
        let known = store.latest().expect("a turn").stats(c.at(9000));
        assert_eq!(known.duration, ms(1800), "the agent's own clock wins");
        assert_eq!(known.cost_usd, Some(0.31));
        assert_eq!(
            known.tokens,
            Some(TokenCount {
                input: 1200,
                output: 80
            })
        );
    }

    #[test]
    fn only_twenty_turns_are_kept() {
        let c = Clock(Instant::now());
        let mut store = TraceStore::default();
        assert!(store.turn_indices().is_empty(), "no turn, no index");
        for i in 1..=21u64 {
            store.apply(TraceEvent::TurnStarted { at: c.at(i * 100) });
            store.set_latest_title(&format!("\n  turn {i}  \nsecond line"));
            finish(&mut store, &c, i * 100 + 50, TurnOutcome::Done);
        }

        assert!(store.turn(1).is_none(), "the oldest turn was let go");
        assert!(store.turn(0).is_none());
        assert!(store.turn(22).is_none());
        assert_eq!(store.turn(2).expect("kept").title(), "turn 2");
        assert_eq!(store.turn(21).expect("kept").index(), 21);
        assert_eq!(store.turn_indices(), 2..=21);
        assert_eq!(store.latest().expect("a turn").index(), 21);
        assert!(!store.is_empty());
        assert!(!store.is_running());
    }

    #[test]
    fn a_turn_past_the_span_cap_counts_what_it_did_not_keep() {
        let (mut store, c) = open_turn();
        for i in 0..2000 {
            tool(&mut store, &c, &format!("toolu_{i}"), None, "Read", i);
        }
        assert_eq!(store.latest().expect("a turn").dropped_spans(), 0);

        tool(&mut store, &c, "toolu_2000", None, "Read", 2000);
        let turn = store.latest().expect("a turn");
        assert_eq!(turn.dropped_spans(), 1);
        assert_eq!(rows_of(&store, c.at(3000)).len(), 2001, "the root and 2000");
        assert_eq!(turn.stats(c.at(3000)).tool_calls, 2000);

        // What happens to a span that was not kept is not an error.
        end(&mut store, &c, "toolu_2000", 2100);
        wait(&mut store, &c, Some("toolu_2000"), 1, 2200);
        let turn = store.latest().expect("a turn");
        assert_eq!(turn.dropped_spans(), 1, "an event is not a span");
        assert_eq!(turn.state(), TurnState::Waiting, "its wait still waits");
    }

    #[test]
    fn reveal_reaches_the_nearest_transcript_entry() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_m", None, "Read", 100);
        call(&mut store, &c, "msg_m", None, "claude-haiku-5-5", 150);
        agent(&mut store, &c, "toolu_x", None, "reviewer", "", 200);
        tool(&mut store, &c, "toolu_x1", Some("toolu_x"), "Grep", 250);
        call(&mut store, &c, "msg_x1", Some("toolu_x"), "claude-haiku-5-5", 260);
        agent(&mut store, &c, "toolu_y", Some("toolu_x"), "explorer", "", 300);
        tool(&mut store, &c, "toolu_z", Some("toolu_y"), "Read", 350);
        let target = |span: &str| reveal(&store, span);
        // A main-thread tool is its own row.
        assert_eq!(target("toolu_m"), RevealTarget::ToolRow("toolu_m".into()));
        // A subagent is its Agent row.
        assert_eq!(target("toolu_x"), RevealTarget::ToolRow("toolu_x".into()));
        // Anything inside a subagent is the outermost Agent row above it, at
        // any depth.
        assert_eq!(target("toolu_x1"), RevealTarget::ToolRow("toolu_x".into()));
        assert_eq!(target("msg_x1"), RevealTarget::ToolRow("toolu_x".into()));
        assert_eq!(target("toolu_y"), RevealTarget::ToolRow("toolu_x".into()));
        assert_eq!(target("toolu_z"), RevealTarget::ToolRow("toolu_x".into()));
        // A main-thread model call, the root, and what the store does not
        // know have only the user's message to reach.
        assert_eq!(target("msg_m"), RevealTarget::UserMessage);
        assert_eq!(target(ROOT), RevealTarget::UserMessage);
        assert_eq!(target("toolu_nobody"), RevealTarget::UserMessage);

        // A span that waits reaches its permission, wherever it sits.
        wait(&mut store, &c, Some("toolu_m"), 42, 400);
        wait(&mut store, &c, Some("toolu_z"), 43, 410);
        wait(&mut store, &c, Some("toolu_y"), 44, 420);
        let target = |span: &str| reveal(&store, span);
        assert_eq!(target("toolu_m"), RevealTarget::Permission(42));
        assert_eq!(target("toolu_z"), RevealTarget::Permission(43));
        assert_eq!(target("toolu_y"), RevealTarget::Permission(44));
        // A subagent that only holds a waiting call is not waiting itself.
        assert_eq!(target("toolu_x"), RevealTarget::ToolRow("toolu_x".into()));

        unwait(&mut store, &c, 42, 500);
        assert_eq!(
            reveal(&store, "toolu_m"),
            RevealTarget::ToolRow("toolu_m".into()),
            "an answered wait reaches the row again"
        );
    }

    #[test]
    fn axis_has_at_least_ten_seconds_and_round_steps() {
        let short = axis(secs(3));
        assert_eq!(short.range, secs(10));
        assert_eq!(short.ticks, [Duration::ZERO, secs(5), secs(10)]);
        assert_eq!(axis(Duration::ZERO).range, secs(10));

        // The steps the approved mockup draws.
        for (range, step) in [(20, 5), (30, 10), (60, 15), (120, 30)] {
            let axis = axis(secs(range));
            assert_eq!(axis.range, secs(range), "{range} s fits whole steps");
            let want: Vec<Duration> = (0..=range / step).map(|i| secs(i * step)).collect();
            assert_eq!(axis.ticks, want, "{range} s is read in {step} s");
        }

        // A range that is not a whole number of steps is rounded up to one.
        let odd = axis(secs(21));
        assert_eq!(odd.range, secs(30));
        assert_eq!(odd.ticks, [0, 10, 20, 30].map(secs));
        let tenth = axis(ms(10_300));
        assert_eq!(tenth.range, secs(15));
        assert_eq!(tenth.ticks, [0, 5, 10, 15].map(secs));
    }

    #[test]
    fn axis_for_an_hour_uses_round_steps() {
        let hour = axis(secs(3600));
        assert_eq!(hour.range, secs(3600));
        assert_eq!(hour.ticks, [0, 15, 30, 45, 60].map(|m| secs(m * 60)));

        let past = axis(secs(61 * 60));
        assert_eq!(past.range, secs(90 * 60));
        assert_eq!(past.ticks, [0, 30, 60, 90].map(|m| secs(m * 60)));

        // However long the turn, the axis stays readable and starts at zero.
        for total in (0..=100_000u64).step_by(37) {
            let axis = axis(secs(total));
            let step = axis.ticks[1] - axis.ticks[0];
            assert!(axis.ticks.len() <= 5, "{total} s gave {:?}", axis.ticks);
            assert_eq!(axis.ticks[0], Duration::ZERO);
            assert_eq!(axis.ticks.last(), Some(&axis.range), "{total} s");
            assert!(axis.range >= secs(total.max(10)), "{total} s is covered");
            assert!(axis.range < secs(total.max(10)) + step, "{total} s is tight");
        }
    }

    #[test]
    fn summary_strings() {
        let ended = |outcome: TurnOutcome, wall: u64, measured: Option<u64>, cost: Option<f64>| {
            let (mut store, c) = open_turn();
            finish_with(&mut store, &c, wall, outcome, measured, cost, None);
            (store, c)
        };
        let failed = || TurnOutcome::Failed {
            reason: "boom".into(),
        };

        let (live, c) = open_turn();
        assert_eq!(
            live.latest().expect("a turn").summary(c.at(34_900)),
            "Live · 34s"
        );

        let (mut waiting, c) = open_turn();
        tool(&mut waiting, &c, "toolu_a", None, "Bash", 100);
        wait(&mut waiting, &c, Some("toolu_a"), 1, 200);
        assert_eq!(
            waiting.latest().expect("a turn").summary(c.at(46_200)),
            "Waiting for you · 46s"
        );

        let summary = |(store, c): (TraceStore, Clock)| {
            store.latest().expect("a turn").summary(c.at(900_000))
        };
        assert_eq!(
            summary(ended(failed(), 63_400, Some(62_000), Some(0.12))),
            "Failed · 1m 02s · $0.12"
        );
        assert_eq!(
            summary(ended(failed(), 63_400, Some(62_000), None)),
            "Failed · 1m 02s"
        );
        assert_eq!(
            summary(ended(TurnOutcome::Interrupted, 12_300, None, None)),
            "Interrupted · 12s"
        );
        assert_eq!(
            summary(ended(TurnOutcome::Interrupted, 12_300, None, Some(0.05))),
            "Interrupted · 12s",
            "an interrupted turn never prints a cost"
        );
        assert_eq!(
            summary(ended(TurnOutcome::Done, 77_400, Some(76_000), Some(0.31))),
            "Done · 1m 16s · $0.31"
        );
        assert_eq!(
            summary(ended(TurnOutcome::Done, 77_400, Some(76_000), None)),
            "Done · 1m 16s"
        );
        assert_eq!(
            summary(ended(TurnOutcome::Done, 76_400, None, Some(0.004))),
            "Done · 1m 16s · $0.00",
            "no clock of the agent's own: the instants say"
        );

        assert_eq!(format_duration(Duration::ZERO), "0s");
        assert_eq!(format_duration(ms(59_999)), "59s");
        assert_eq!(format_duration(secs(60)), "1m 00s");
        assert_eq!(format_duration(secs(62)), "1m 02s");
        assert_eq!(format_duration(secs(3725)), "62m 05s");
    }

    #[test]
    fn a_span_event_for_an_unknown_span_is_ignored() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        let rows_before = rows_of(&store, c.at(500));
        let lanes_before = lanes_of(&store, c.at(500));

        end(&mut store, &c, "toolu_nobody", 200);
        store.apply(TraceEvent::SpanNamed {
            id: "toolu_nobody".into(),
            name: "ghost".into(),
            detail: "ghost".into(),
        });
        store.apply(TraceEvent::SpanUsage {
            id: "toolu_nobody".into(),
            tokens: TokenCount {
                input: 1,
                output: 1,
            },
        });
        store.apply(TraceEvent::SpanProgress {
            id: "toolu_nobody".into(),
            tokens: Some(1),
            tool_uses: Some(1),
            activity: Some("ghost".into()),
        });
        unwait(&mut store, &c, 99, 300);

        assert_eq!(rows_of(&store, c.at(500)), rows_before);
        assert_eq!(lanes_of(&store, c.at(500)), lanes_before);
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Live);

        // What cannot be placed is shown, not dropped: a span of a parent
        // nobody opened hangs under the root, and a wait on a span nobody
        // opened waits on the root.
        tool(&mut store, &c, "toolu_orphan", Some("toolu_ghost"), "Read", 400);
        wait(&mut store, &c, Some("toolu_nobody"), 5, 450);
        let found = rows_of(&store, c.at(500));
        assert_eq!(row(&found, "toolu_orphan").depth, 1);
        let lanes = lanes_of(&store, c.at(500));
        assert_eq!(
            lanes[0].waits,
            vec![LaneWait {
                request_id: 5,
                tool: String::new()
            }]
        );
        assert_eq!(store.latest().expect("a turn").state(), TurnState::Waiting);
    }

    #[test]
    fn slowest_leaf_is_the_longest_ended_leaf() {
        let (mut store, c) = open_turn();
        assert_eq!(store.latest().expect("a turn").slowest_leaf(), None);

        tool(&mut store, &c, "toolu_t1", None, "Read", 0);
        end(&mut store, &c, "toolu_t1", 1000);
        tool(&mut store, &c, "toolu_t2", None, "Bash", 1000);
        end(&mut store, &c, "toolu_t2", 4000);
        // Longer than every leaf, but it has children: not a leaf.
        agent(&mut store, &c, "toolu_x", None, "reviewer", "", 0);
        tool(&mut store, &c, "toolu_x1", Some("toolu_x"), "Grep", 100);
        end(&mut store, &c, "toolu_x1", 2500);
        end(&mut store, &c, "toolu_x", 9000);
        // Longer than every leaf, but still running: not ended.
        tool(&mut store, &c, "toolu_run", None, "Bash", 100);
        assert_eq!(
            store.latest().expect("a turn").slowest_leaf(),
            Some("toolu_t2")
        );

        // A failed call took the time it took.
        tool(&mut store, &c, "toolu_f", None, "Write", 4000);
        end_as(
            &mut store,
            &c,
            "toolu_f",
            SpanOutcome::Failed {
                note: "denied".into(),
            },
            8000,
        );
        assert_eq!(
            store.latest().expect("a turn").slowest_leaf(),
            Some("toolu_f")
        );

        // The agent's own clock wins over the instants.
        tool(&mut store, &c, "toolu_m", None, "Task", 8000);
        store.apply(TraceEvent::SpanEnded {
            id: "toolu_m".into(),
            outcome: SpanOutcome::Done,
            at: c.at(8100),
            measured_ms: Some(5000),
        });
        assert_eq!(
            store.latest().expect("a turn").slowest_leaf(),
            Some("toolu_m")
        );
    }

    #[test]
    fn a_folded_parent_shows_its_childrens_marks() {
        let (mut store, c) = open_turn();
        agent(&mut store, &c, "toolu_x", None, "reviewer", "Review", 1000);
        tool(&mut store, &c, "toolu_x1", Some("toolu_x"), "Grep", 1500);
        call(&mut store, &c, "msg_x2", Some("toolu_x"), "claude-haiku-5-5", 2000);
        agent(&mut store, &c, "toolu_y", Some("toolu_x"), "explorer", "", 2500);
        tool(&mut store, &c, "toolu_y1", Some("toolu_y"), "Read", 3000);
        tool(&mut store, &c, "toolu_t", None, "Bash", 4000);
        // Arrives last, started first: a subagent's model call is reported
        // when it ends. The tree is in start order.
        call(&mut store, &c, "msg_early", Some("toolu_x"), "claude-haiku-5-5", 1200);
        let now = c.at(5000);
        let turn = store.latest().expect("a turn");

        let open = turn.rows(&HashSet::new(), now);
        assert_eq!(
            ids(&open),
            [
                ROOT,
                "toolu_x",
                "msg_early",
                "toolu_x1",
                "msg_x2",
                "toolu_y",
                "toolu_y1",
                "toolu_t"
            ]
        );
        let depths: Vec<u8> = open.iter().map(|row| row.depth).collect();
        assert_eq!(depths, [0, 1, 2, 2, 2, 2, 3, 1]);
        assert_eq!(row(&open, ROOT).kind, None, "the root is no span's kind");
        assert_eq!(row(&open, "toolu_x").kind, Some(SpanKind::Subagent));
        assert_eq!(row(&open, "toolu_x1").kind, Some(SpanKind::Tool));
        assert_eq!(row(&open, "msg_x2").kind, Some(SpanKind::Llm));
        assert_eq!(row(&open, "toolu_x").detail, "Review");
        assert_eq!(row(&open, "toolu_x").start, ms(1000));
        assert_eq!(row(&open, "toolu_x").end, None, "still running");
        assert!(open.iter().all(|row| row.hidden_children == 0));
        assert!(open.iter().all(|row| row.child_marks.is_empty()));

        let folded: HashSet<String> = ["toolu_x".to_string()].into();
        let rows = turn.rows(&folded, now);
        assert_eq!(ids(&rows), [ROOT, "toolu_x", "toolu_t"]);
        let parent = row(&rows, "toolu_x");
        assert_eq!(parent.hidden_children, 5, "every row folded away");
        assert_eq!(
            parent.child_marks,
            [1200, 1500, 2000, 2500, 3000].map(ms),
            "one mark where each hidden row starts"
        );
        assert_eq!(row(&rows, ROOT).hidden_children, 0);

        // The root folds like any parent, and an id that holds nothing
        // folds nothing.
        let everything: HashSet<String> = [ROOT.to_string(), "toolu_t".to_string()].into();
        let rows = turn.rows(&everything, now);
        assert_eq!(ids(&rows), [ROOT]);
        assert_eq!(rows[0].hidden_children, 7);
        assert_eq!(rows[0].child_marks.len(), 7);
    }

    #[test]
    fn a_lane_lists_its_open_waits() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_bash", None, "Bash", 100);
        agent(&mut store, &c, "toolu_x", None, "reviewer", "", 200);
        tool(&mut store, &c, "toolu_w", Some("toolu_x"), "Write", 300);
        let now = c.at(1000);

        wait(&mut store, &c, Some("toolu_w"), 2, 400);
        let lanes = lanes_of(&store, now);
        assert_eq!(
            lane(&lanes, "toolu_x").waits,
            vec![LaneWait {
                request_id: 2,
                tool: "Write".into()
            }],
            "the wait sits on the lane of the agent whose tool asked"
        );
        assert!(lane(&lanes, ROOT).waits.is_empty(), "and on no other lane");
        // The whole path to the root shows the wait as a state.
        assert_eq!(lane(&lanes, "toolu_x").state, SpanState::Waiting);
        assert_eq!(lane(&lanes, ROOT).state, SpanState::Waiting);

        wait(&mut store, &c, Some("toolu_bash"), 3, 450);
        wait(&mut store, &c, None, 4, 460);
        wait(&mut store, &c, Some("toolu_x"), 5, 470);
        let lanes = lanes_of(&store, now);
        let tools: Vec<(u64, &str)> = lane(&lanes, ROOT)
            .waits
            .iter()
            .map(|wait| (wait.request_id, wait.tool.as_str()))
            .collect();
        assert_eq!(tools, [(3, "Bash"), (4, "")], "in the order they came");
        let on_agent: Vec<(u64, &str)> = lane(&lanes, "toolu_x")
            .waits
            .iter()
            .map(|wait| (wait.request_id, wait.tool.as_str()))
            .collect();
        assert_eq!(
            on_agent,
            [(2, "Write"), (5, "")],
            "a wait on the agent itself names no tool"
        );

        for request_id in 2..=5 {
            unwait(&mut store, &c, request_id, 500);
        }
        let lanes = lanes_of(&store, now);
        assert!(lanes.iter().all(|lane| lane.waits.is_empty()));
        assert_eq!(lane(&lanes, "toolu_x").state, SpanState::Running);
        assert_eq!(lane(&lanes, ROOT).state, SpanState::Running);
    }

    #[test]
    fn events_outside_a_turn_are_ignored() {
        let c = Clock(Instant::now());
        let mut store = TraceStore::default();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        wait(&mut store, &c, Some("toolu_a"), 1, 150);
        finish(&mut store, &c, 200, TurnOutcome::Done);
        assert!(store.is_empty(), "a turn Sirio did not start is no turn");
        assert!(store.latest().is_none());

        // The same after a turn has ended: nothing is open to receive them.
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        finish(&mut store, &c, 200, TurnOutcome::Done);
        let rows_before = rows_of(&store, c.at(900));
        tool(&mut store, &c, "toolu_late", None, "Read", 300);
        wait(&mut store, &c, Some("toolu_a"), 2, 310);
        end(&mut store, &c, "toolu_a", 320);
        finish(&mut store, &c, 400, TurnOutcome::Failed { reason: "late".into() });

        assert_eq!(rows_of(&store, c.at(900)), rows_before);
        let turn = store.latest().expect("a turn");
        assert_eq!(turn.state(), TurnState::Done);
        assert_eq!(turn.elapsed(c.at(900)), ms(200));
        assert!(lanes_of(&store, c.at(900))[0].waits.is_empty());
    }

    #[test]
    fn a_turn_start_while_one_is_open_ends_the_open_turn_failed() {
        let (mut store, c) = open_turn();
        tool(&mut store, &c, "toolu_a", None, "Bash", 100);
        wait(&mut store, &c, Some("toolu_a"), 1, 150);

        store.apply(TraceEvent::TurnStarted { at: c.at(500) });

        let first = store.turn(1).expect("the first turn is kept");
        assert_eq!(first.state(), TurnState::Failed);
        assert_eq!(first.elapsed(c.at(9000)), ms(500));
        let found = first.rows(&HashSet::new(), c.at(9000));
        assert_eq!(row(&found, "toolu_a").state, SpanState::Interrupted);
        assert_eq!(
            row(&found, ROOT).note.as_deref(),
            Some("The turn ended without a result")
        );
        assert!(first.lanes(c.at(9000))[0].waits.is_empty());

        let second = store.turn(2).expect("the new turn opened");
        assert_eq!(second.state(), TurnState::Live);
        assert_eq!(second.index(), 2);
        assert_eq!(store.turn_indices(), 1..=2);
        assert!(store.is_running());
        assert_eq!(rows_of(&store, c.at(9000)).len(), 1, "it starts empty");
    }
}

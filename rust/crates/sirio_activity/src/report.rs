//! The turn tracker: what an agent's OSC 7501 reports say about its turns.
//!
//! One [`TurnTracker`] per pane turns the reports of the program on it into the
//! status the tab shows. It is pure: the caller says what was reported and when the
//! host saw it, and reads the status back. Time is the host's, in milliseconds
//! since the session started, so a stalled app or a relaunch cannot change how long
//! a turn lasted.
//!
//! The rules (spec section 3.3):
//!
//! - A **turn** opens on `working`, at the report's time, and ends at the first
//!   `idle`, `done` or `error`. A `blocked` spell inside it does not end it.
//! - `working` is Running. `blocked` is NeedsInput, a first one included. `done` is
//!   Done and `error` is Error, and both close the turn.
//! - `idle` is Done when a turn is open and either it lasted at least
//!   [`MIN_TURN_MS`] or a `blocked` came inside it (a rejected permission).
//!   Otherwise it shows no status: a startup, the 10 ms blip Codex sends, a startup
//!   dialog that was answered. Either way the turn closes.
//! - A report older than the one the tracker holds is a replay and is ignored; the
//!   same report again changes nothing.

use std::time::Duration;

use crate::status::AgentStatus;

/// How long a turn must last for its `idle` to show as finished. It separates a
/// real turn from Codex's startup blip.
pub const MIN_TURN_MS: u64 = 1000;

/// How long after an agent is recognised its title and screen set no status: what
/// the CLI writes while it starts says nothing about a turn.
pub const TITLE_GRACE: Duration = Duration::from_secs(2);

/// The state an agent reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportedState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

/// One pane's reported state, its open turn and the status that follows from them.
#[derive(Clone, Debug)]
pub(crate) struct TurnTracker {
    /// The last report applied: its state and the host time it carries.
    last: Option<(ReportedState, u64)>,
    /// The host time of the `working` that opened the turn, while it is open.
    turn_start: Option<u64>,
    /// A `blocked` came inside the open turn.
    blocked_in_turn: bool,
    status: Option<AgentStatus>,
}

impl TurnTracker {
    /// A tracker that has seen no report.
    pub(crate) fn new() -> Self {
        Self {
            last: None,
            turn_start: None,
            blocked_in_turn: false,
            status: None,
        }
    }

    /// The status the reports show: `None` for no status.
    pub(crate) fn status(&self) -> Option<AgentStatus> {
        self.status
    }

    /// Applies one report seen by the host at `at_ms`. A report older than the
    /// last applied one is a replay and is dropped, and the same state at the same
    /// time is a repeat that changes nothing.
    pub(crate) fn report(&mut self, state: ReportedState, at_ms: u64) {
        if let Some((last_state, last_at)) = self.last {
            if at_ms < last_at || (at_ms == last_at && state == last_state) {
                return;
            }
        }
        self.last = Some((state, at_ms));
        match state {
            ReportedState::Working => {
                if self.turn_start.is_none() {
                    self.turn_start = Some(at_ms);
                    self.blocked_in_turn = false;
                }
                self.status = Some(AgentStatus::Running);
            }
            ReportedState::Blocked => {
                if self.turn_start.is_some() {
                    self.blocked_in_turn = true;
                }
                self.status = Some(AgentStatus::NeedsInput);
            }
            ReportedState::Done => {
                self.close_turn();
                self.status = Some(AgentStatus::Done);
            }
            ReportedState::Error => {
                self.close_turn();
                self.status = Some(AgentStatus::Error);
            }
            ReportedState::Idle => {
                let finished = self
                    .turn_start
                    .is_some_and(|start| self.blocked_in_turn || at_ms.saturating_sub(start) >= MIN_TURN_MS);
                self.close_turn();
                self.status = finished.then_some(AgentStatus::Done);
            }
        }
    }

    /// The agent left while a turn was running: what it was doing is finished.
    /// Only a pane that is Running settles; the turn closes.
    pub(crate) fn exited(&mut self) {
        if self.status == Some(AgentStatus::Running) {
            self.status = Some(AgentStatus::Done);
        }
        self.close_turn();
    }

    fn close_turn(&mut self) {
        self.turn_start = None;
        self.blocked_in_turn = false;
    }
}

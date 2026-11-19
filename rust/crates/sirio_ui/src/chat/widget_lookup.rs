//! Which MCP tool calls get a widget, decided from plain data.
//!
//! A call is looked up in the servers' tool list (`ui_tool_for`). The list `system/init`
//! names carries no tools, so the first widget call of a session, and the first after each
//! reconnect, toggle or reload, waits for a status answer that takes up to ten seconds.
//! While it waits, the call's own `Returned` and `Cancelled` must not be lost. This module
//! holds that state machine and nothing else: no GPUI, no CLI, so each step is a plain
//! function of its inputs.
//!
//! How it can fail, written before the code:
//!
//! 1. A `Returned` that arrives before the refresh answers is lost, or reaches the widget
//!    ahead of the input.
//! 2. A `Cancelled` during the lookup is lost.
//! 3. A miss leaves a record or a slot behind, so a late `Returned` for a call without a
//!    widget is held forever.
//! 4. A second miss in the same turn refreshes again.
//! 5. A lookup that arrives while a refresh is in flight starts a second refresh instead of
//!    joining it.
//! 6. A reconnect, toggle or reload leaves the stale list in use.
//! 7. A refresh that was asked before a reconnect answers after it, and its list is kept as
//!    if it were current; or a call made after the reconnect is resolved from it.
//! 8. A refresh that fails leaves its callers waiting, or is asked again for every call of
//!    the turn.
//! 9. A late answer that belongs to a session that was replaced resolves the new session's
//!    calls.
//! 10. A hit on a list that already knows the tool still waits for a refresh, or a seeded
//!     list is ignored.
//! 11. The same call announced twice makes two records.
//! 12. A refresh asked before a reconnect fails after it, and a call made after the reconnect
//!     is ended with it, or the failure is held against the turn so later calls get no ask.
//! 13. A transcript that was replaced leaves lookups behind: a refresh asked for the old rows
//!     answers into the new ones and opens a slot for a call with no row.

use serde_json::Value;

/// What a call's lookup is waiting for, or has the answer to.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Update {
    Returned(Value),
    Cancelled,
}

/// What a call said while it had no widget yet.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Held {
    pub input: Value,
    pub result: Option<Value>,
    pub cancelled: bool,
}

/// One thing for a widget, in the order the protocol wants it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Delivery {
    Input(Value),
    Result(Value),
    Cancelled,
}

impl Held {
    pub(super) fn new(input: Value) -> Self {
        Self { input, result: None, cancelled: false }
    }

    /// The first result is the call's; later ones are ignored, as the bridge does.
    pub(super) fn take(&mut self, update: Update) {
        match update {
            Update::Returned(result) => {
                if self.result.is_none() {
                    self.result = Some(result);
                }
            }
            Update::Cancelled => self.cancelled = true,
        }
    }

    /// Everything held, input first, then the result, then a cancellation.
    pub(super) fn deliveries(self) -> Vec<Delivery> {
        let mut out = vec![Delivery::Input(self.input)];
        out.extend(self.result.map(Delivery::Result));
        if self.cancelled {
            out.push(Delivery::Cancelled);
        }
        out
    }
}

/// What the lookup has decided about one call.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Step<R> {
    /// Ask the CLI for the status, off the UI thread, and answer with
    /// [`Lookups::refreshed`] and this ticket.
    Refresh { ticket: u64 },
    /// The call's tool has a UI: make its slot.
    Hit { id: String, turn: u64, hit: R, held: Held },
    /// It has none: nothing was kept.
    Miss { id: String },
}

enum List<L> {
    /// Never read for this session: the first lookup seeds it from the client.
    Unseeded,
    /// Dropped by a reconnect, toggle or reload: the next lookup asks.
    Stale,
    Have(L),
}

struct Flight {
    ticket: u64,
    generation: u64,
    turn: u64,
}

struct Pending {
    id: String,
    name: String,
    turn: u64,
    held: Held,
    /// Set once the call waits on the refresh in flight: the generation it joined at.
    joined: Option<u64>,
}

pub(super) struct Lookups<L, R> {
    list: List<L>,
    /// Counts the drops of the list, so an answer that was asked for before a drop is known.
    generation: u64,
    next_ticket: u64,
    flight: Option<Flight>,
    /// The turn a refresh was last started for.
    spent: Option<u64>,
    /// The turn a refresh last failed in.
    failed: Option<u64>,
    pending: Vec<Pending>,
    _hit: std::marker::PhantomData<R>,
}

impl<L, R> Default for Lookups<L, R> {
    fn default() -> Self {
        Self {
            list: List::Unseeded,
            generation: 0,
            next_ticket: 1,
            flight: None,
            spent: None,
            failed: None,
            pending: Vec::new(),
            _hit: std::marker::PhantomData,
        }
    }
}

impl<L, R> Lookups<L, R> {
    /// A call was announced. `seed` reads the client's own list, once per session.
    pub(super) fn called(
        &mut self,
        id: &str,
        name: &str,
        input: Value,
        turn: u64,
        seed: impl FnOnce() -> Option<L>,
        find: impl Fn(&L, &str) -> Option<R>,
    ) -> Vec<Step<R>> {
        if self.pending.iter().any(|pending| pending.id == id) {
            return Vec::new();
        }
        if matches!(self.list, List::Unseeded) {
            self.list = seed().map_or(List::Stale, List::Have);
        }
        let mut pending = Pending {
            id: id.to_owned(),
            name: name.to_owned(),
            turn,
            held: Held::new(input),
            joined: None,
        };
        let known = match &self.list {
            List::Have(list) => Some(find(list, name)),
            _ => None,
        };
        match known {
            Some(Some(hit)) => return vec![Step::Hit {
                id: pending.id,
                turn,
                hit,
                held: pending.held,
            }],
            // A list that lacks the tool: the server may have changed, so one refresh per
            // turn, and none while one is already asked (the call joins that one).
            Some(None) if self.flight.is_none() && self.spent == Some(turn) => {
                return vec![Step::Miss { id: pending.id }];
            }
            None if self.flight.is_none() && self.failed == Some(turn) => {
                return vec![Step::Miss { id: pending.id }];
            }
            _ => {}
        }
        let mut steps = Vec::new();
        if self.flight.is_none() {
            steps.push(self.start(turn));
        }
        pending.joined = Some(self.generation);
        self.pending.push(pending);
        steps
    }

    fn start(&mut self, turn: u64) -> Step<R> {
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.spent = Some(turn);
        self.flight = Some(Flight { ticket, generation: self.generation, turn });
        Step::Refresh { ticket }
    }

    /// The call's result or cancellation. True when the lookup still holds the call and kept
    /// it; false when the call has no record here.
    pub(super) fn absorb(&mut self, id: &str, update: Update) -> bool {
        let Some(pending) = self.pending.iter_mut().find(|pending| pending.id == id) else {
            return false;
        };
        pending.held.take(update);
        true
    }

    /// The answer to a `Refresh` step. `None` is a refresh that failed.
    pub(super) fn refreshed(
        &mut self,
        ticket: u64,
        answer: Option<L>,
        find: impl Fn(&L, &str) -> Option<R>,
    ) -> Vec<Step<R>> {
        if self.flight.as_ref().map(|flight| flight.ticket) != Some(ticket) {
            return Vec::new();
        }
        let Some(flight) = self.flight.take() else { return Vec::new() };
        let current = flight.generation == self.generation;
        let Some(list) = answer else {
            // A failure that predates a reconnect says nothing of the servers as they are now:
            // it is not held against the turn, and a call made after the reconnect asks again.
            if current {
                self.failed = Some(flight.turn);
            }
            let mut rest = Vec::new();
            let mut steps = Vec::new();
            for pending in std::mem::take(&mut self.pending) {
                if !current && pending.joined == Some(self.generation) {
                    rest.push(pending);
                } else {
                    steps.push(Step::Miss { id: pending.id });
                }
            }
            self.restart_for(rest, flight.turn, &mut steps);
            return steps;
        };
        let mut steps = Vec::new();
        let mut rest = Vec::new();
        for pending in std::mem::take(&mut self.pending) {
            // A call made after a drop of the list must not be resolved from an answer
            // that was asked for before it.
            if !current && pending.joined == Some(self.generation) {
                rest.push(pending);
                continue;
            }
            match find(&list, &pending.name) {
                Some(hit) => steps.push(Step::Hit {
                    id: pending.id,
                    turn: pending.turn,
                    hit,
                    held: pending.held,
                }),
                None => steps.push(Step::Miss { id: pending.id }),
            }
        }
        if current {
            self.list = List::Have(list);
        }
        self.restart_for(rest, flight.turn, &mut steps);
        steps
    }

    /// Calls that joined a flight asked before a reconnect get a flight of their own.
    fn restart_for(&mut self, rest: Vec<Pending>, turn: u64, steps: &mut Vec<Step<R>>) {
        if rest.is_empty() {
            return;
        }
        let turn = rest.iter().map(|pending| pending.turn).min().unwrap_or(turn);
        steps.push(self.start(turn));
        self.pending = rest;
    }

    /// A reconnect, toggle or reload: the list is old, the next lookup asks again.
    pub(super) fn invalidate(&mut self) {
        self.list = List::Stale;
        self.generation += 1;
        self.failed = None;
    }

    /// The transcript was replaced: every call being looked up has no row any more, and the
    /// answer to a refresh asked for them lands on nothing. The stored list stays.
    pub(super) fn discard(&mut self) {
        self.pending.clear();
        self.flight = None;
    }

    /// A new session: nothing of the old one stays.
    pub(super) fn reset(&mut self) {
        self.list = List::Unseeded;
        self.generation += 1;
        self.flight = None;
        self.spent = None;
        self.failed = None;
        self.pending.clear();
    }

    /// The server list the lookups last kept, when there is a current one.
    pub(super) fn list(&self) -> Option<&L> {
        match &self.list {
            List::Have(list) => Some(list),
            _ => None,
        }
    }

    /// A list for a test, with no client to read it from.
    #[cfg(test)]
    pub(super) fn seed(&mut self, list: L) {
        self.list = List::Have(list);
    }

    /// Whether the lookup still holds the call: it has no slot yet and may get one.
    pub(super) fn holds(&self, id: &str) -> bool {
        self.pending.iter().any(|pending| pending.id == id)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    type Machine = Lookups<Vec<&'static str>, String>;

    fn find(list: &Vec<&'static str>, name: &str) -> Option<String> {
        list.iter().find(|known| **known == name).map(|known| (*known).to_string())
    }

    fn call(machine: &mut Machine, id: &str, name: &str, turn: u64) -> Vec<Step<String>> {
        machine.called(id, name, json!({ "q": id }), turn, || None, find)
    }

    fn call_seeded(
        machine: &mut Machine,
        id: &str,
        name: &str,
        seed: Vec<&'static str>,
    ) -> Vec<Step<String>> {
        machine.called(id, name, json!({ "q": id }), 1, || Some(seed), find)
    }

    fn ticket_of(steps: &[Step<String>]) -> u64 {
        steps
            .iter()
            .find_map(|step| match step {
                Step::Refresh { ticket } => Some(*ticket),
                _ => None,
            })
            .expect("a refresh was asked for")
    }

    fn refreshes(steps: &[Step<String>]) -> usize {
        steps.iter().filter(|step| matches!(step, Step::Refresh { .. })).count()
    }

    fn hits(steps: &[Step<String>]) -> Vec<(&str, &Held)> {
        steps
            .iter()
            .filter_map(|step| match step {
                Step::Hit { id, held, .. } => Some((id.as_str(), held)),
                _ => None,
            })
            .collect()
    }

    fn misses(steps: &[Step<String>]) -> Vec<&str> {
        steps
            .iter()
            .filter_map(|step| match step {
                Step::Miss { id } => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_call_of_a_session_asks_and_keeps_what_arrives_meanwhile() {
        let mut machine = Machine::default();
        let steps = call(&mut machine, "a", "mcp__s__show", 1);
        let ticket = ticket_of(&steps);
        assert!(machine.absorb("a", Update::Returned(json!({ "content": [] }))));
        assert!(machine.absorb("a", Update::Cancelled));
        let steps = machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        let found = hits(&steps);
        assert_eq!(found.len(), 1);
        let (_, held) = found[0];
        assert_eq!(held.input, json!({ "q": "a" }));
        assert_eq!(held.result, Some(json!({ "content": [] })));
        assert!(held.cancelled);
        assert!(!machine.holds("a"));
    }

    #[test]
    fn what_a_call_held_reaches_the_widget_input_first() {
        let mut held = Held::new(json!(1));
        held.take(Update::Cancelled);
        held.take(Update::Returned(json!(2)));
        held.take(Update::Returned(json!(3)));
        assert_eq!(
            held.deliveries(),
            vec![Delivery::Input(json!(1)), Delivery::Result(json!(2)), Delivery::Cancelled]
        );
    }

    #[test]
    fn a_miss_leaves_nothing_and_a_late_update_is_not_kept() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__plain", 1));
        assert!(machine.absorb("a", Update::Cancelled));
        let steps = machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        assert_eq!(misses(&steps), vec!["a"]);
        assert!(!machine.holds("a"));
        assert!(!machine.absorb("a", Update::Returned(json!(1))));
    }

    #[test]
    fn a_second_miss_in_the_same_turn_does_not_refresh_and_a_later_turn_does() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__plain", 4));
        machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        let steps = call(&mut machine, "b", "mcp__s__plain", 4);
        assert_eq!(refreshes(&steps), 0);
        assert_eq!(misses(&steps), vec!["b"]);
        assert!(!machine.holds("b"));
        let steps = call(&mut machine, "c", "mcp__s__plain", 5);
        assert_eq!(refreshes(&steps), 1);
    }

    #[test]
    fn a_known_tool_needs_no_refresh_even_after_a_miss_this_turn() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__plain", 4));
        machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        let steps = call(&mut machine, "b", "mcp__s__show", 4);
        assert_eq!(refreshes(&steps), 0);
        assert_eq!(hits(&steps).len(), 1);
    }

    #[test]
    fn a_call_during_a_refresh_joins_it() {
        let mut machine = Machine::default();
        let first = call(&mut machine, "a", "mcp__s__show", 1);
        let ticket = ticket_of(&first);
        let second = call(&mut machine, "b", "mcp__s__other", 1);
        assert_eq!(refreshes(&second), 0);
        assert!(second.is_empty());
        let steps = machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        assert_eq!(refreshes(&steps), 0);
        assert_eq!(hits(&steps).len(), 1);
        assert_eq!(misses(&steps), vec!["b"]);
    }

    #[test]
    fn a_seeded_list_that_knows_the_tool_needs_no_refresh() {
        let mut machine = Machine::default();
        let steps = call_seeded(&mut machine, "a", "mcp__s__show", vec!["mcp__s__show"]);
        assert_eq!(refreshes(&steps), 0);
        assert_eq!(hits(&steps).len(), 1);
        let steps = call_seeded(&mut machine, "b", "mcp__s__show", Vec::new());
        assert_eq!(hits(&steps).len(), 1, "the seed is read once");
    }

    #[test]
    fn a_reconnect_drops_the_list_so_the_next_call_asks() {
        let mut machine = Machine::default();
        let steps = call_seeded(&mut machine, "a", "mcp__s__show", vec!["mcp__s__show"]);
        assert_eq!(hits(&steps).len(), 1);
        machine.invalidate();
        let steps = call(&mut machine, "b", "mcp__s__show", 1);
        assert_eq!(refreshes(&steps), 1);
        assert!(hits(&steps).is_empty());
        assert!(machine.holds("b"));
    }

    #[test]
    fn an_answer_asked_before_a_reconnect_is_not_kept_and_a_later_call_gets_its_own() {
        let mut machine = Machine::default();
        let old = ticket_of(&call(&mut machine, "a", "mcp__s__show", 1));
        machine.invalidate();
        assert!(call(&mut machine, "b", "mcp__s__show", 1).is_empty());
        let steps = machine.refreshed(old, Some(vec!["mcp__s__show"]), find);
        let found = hits(&steps);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "a");
        let fresh = ticket_of(&steps);
        assert_ne!(fresh, old);
        assert!(machine.holds("b"));
        let steps = machine.refreshed(fresh, Some(vec!["mcp__s__show"]), find);
        assert_eq!(hits(&steps).len(), 1);
        let steps = call(&mut machine, "c", "mcp__s__show", 1);
        assert_eq!(hits(&steps).len(), 1, "the fresh answer is the list now");
    }

    #[test]
    fn an_answer_that_predates_a_reconnect_leaves_the_list_dropped() {
        let mut machine = Machine::default();
        let old = ticket_of(&call(&mut machine, "a", "mcp__s__show", 1));
        machine.invalidate();
        machine.refreshed(old, Some(vec!["mcp__s__show"]), find);
        let steps = call(&mut machine, "b", "mcp__s__show", 1);
        assert_eq!(refreshes(&steps), 1);
    }

    #[test]
    fn a_failed_refresh_ends_its_callers_and_waits_for_the_next_turn() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__show", 7));
        assert!(call(&mut machine, "b", "mcp__s__show", 7).is_empty());
        let steps = machine.refreshed(ticket, None, find);
        assert_eq!(misses(&steps), vec!["a", "b"]);
        assert!(!machine.holds("a") && !machine.holds("b"));
        let steps = call(&mut machine, "c", "mcp__s__show", 7);
        assert_eq!(refreshes(&steps), 0);
        assert_eq!(misses(&steps), vec!["c"]);
        let steps = call(&mut machine, "d", "mcp__s__show", 8);
        assert_eq!(refreshes(&steps), 1);
    }

    #[test]
    fn a_failure_that_predates_a_reconnect_does_not_end_a_later_call_or_block_the_turn() {
        let mut machine = Machine::default();
        let old = ticket_of(&call(&mut machine, "a", "mcp__s__show", 7));
        machine.invalidate();
        assert!(call(&mut machine, "b", "mcp__s__show", 7).is_empty());
        let steps = machine.refreshed(old, None, find);
        assert_eq!(misses(&steps), vec!["a"]);
        let fresh = ticket_of(&steps);
        assert!(machine.holds("b"));
        let steps = machine.refreshed(fresh, Some(vec!["mcp__s__show"]), find);
        assert_eq!(hits(&steps).len(), 1);
        let steps = call(&mut machine, "c", "mcp__s__show", 7);
        assert_eq!(hits(&steps).len(), 1);
    }

    #[test]
    fn a_failure_that_predates_a_reconnect_is_not_held_against_the_turn() {
        let mut machine = Machine::default();
        let old = ticket_of(&call(&mut machine, "a", "mcp__s__show", 7));
        machine.invalidate();
        machine.refreshed(old, None, find);
        let steps = call(&mut machine, "b", "mcp__s__show", 7);
        assert_eq!(refreshes(&steps), 1);
    }

    #[test]
    fn a_replaced_transcript_drops_the_lookups_and_their_answers() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__show", 1));
        assert!(machine.absorb("a", Update::Cancelled));
        machine.discard();
        assert!(!machine.holds("a"));
        assert!(machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find).is_empty());
        let steps = call(&mut machine, "b", "mcp__s__show", 1);
        assert_eq!(refreshes(&steps), 1);
    }

    #[test]
    fn discarding_keeps_the_list_the_session_still_has() {
        let mut machine = Machine::default();
        call_seeded(&mut machine, "a", "mcp__s__show", vec!["mcp__s__show"]);
        machine.discard();
        let steps = call(&mut machine, "b", "mcp__s__show", 1);
        assert_eq!(hits(&steps).len(), 1);
    }

    #[test]
    fn a_new_session_ignores_the_old_sessions_answer() {
        let mut machine = Machine::default();
        let old = ticket_of(&call(&mut machine, "a", "mcp__s__show", 1));
        machine.reset();
        assert!(!machine.holds("a"));
        let fresh = ticket_of(&call(&mut machine, "b", "mcp__s__show", 1));
        let steps = machine.refreshed(old, Some(vec!["mcp__s__show"]), find);
        assert!(steps.is_empty());
        assert!(machine.holds("b"));
        let steps = machine.refreshed(fresh, Some(vec!["mcp__s__show"]), find);
        assert_eq!(hits(&steps).len(), 1);
    }

    #[test]
    fn the_same_call_announced_twice_is_one_record() {
        let mut machine = Machine::default();
        let ticket = ticket_of(&call(&mut machine, "a", "mcp__s__show", 1));
        assert!(call(&mut machine, "a", "mcp__s__show", 1).is_empty());
        let steps = machine.refreshed(ticket, Some(vec!["mcp__s__show"]), find);
        assert_eq!(hits(&steps).len(), 1);
    }
}

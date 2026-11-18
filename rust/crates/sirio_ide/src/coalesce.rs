//! Per-key trailing-edge coalescing (spec 2026-11-15, section 4.4): at most one send per key per
//! [`SEND_AFTER`], always carrying the latest state. Pure bookkeeping, no clock and no threads:
//! the caller starts a timer when [`Coalescer::change`] says so and asks for the state when it
//! fires.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

/// How long after a key's first change its send is due.
pub const SEND_AFTER: Duration = Duration::from_millis(100);

/// What is waiting to be sent, per key.
pub struct Coalescer<K, S> {
    waiting: HashMap<K, S>,
}

impl<K: Hash + Eq, S> Default for Coalescer<K, S> {
    fn default() -> Self {
        Self { waiting: HashMap::new() }
    }
}

impl<K: Hash + Eq, S> Coalescer<K, S> {
    /// Notes `latest` as `key`'s state. Returns the delay of the send to schedule when none is
    /// waiting for `key` yet, and `None` when one already is (it will carry `latest`).
    pub fn change(&mut self, key: K, latest: S) -> Option<Duration> {
        self.waiting.insert(key, latest).is_none().then_some(SEND_AFTER)
    }

    /// The send for `key` is due: its latest state, if any is waiting. The next change schedules
    /// again.
    pub fn fire(&mut self, key: &K) -> Option<S> {
        self.waiting.remove(key)
    }

    /// `key` is gone: whatever waited for it is dropped.
    pub fn forget(&mut self, key: &K) {
        self.waiting.remove(key);
    }

    /// Drops everything.
    pub fn clear(&mut self) {
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Failure modes, written before the code:
    // 1. the first change schedules nothing, or schedules at the wrong delay;
    // 2. a change inside the window schedules a second send (a burst becomes a flood);
    // 3. the send carries the first state, not the latest;
    // 4. after a send the next change schedules nothing (stuck: the key never sends again);
    // 5. connections share a window (one key's change swallows another's, or its state leaks);
    // 6. a firing with nothing waiting invents a send;
    // 7. forgetting a key leaves it stuck waiting (a reconnect under the same key never sends).

    #[test]
    fn first_change_schedules_one_send_at_100_ms() {
        let mut c = Coalescer::default();
        assert_eq!(c.change(1, "a"), Some(Duration::from_millis(100)));
    }

    #[test]
    fn changes_inside_the_window_schedule_nothing() {
        let mut c = Coalescer::default();
        c.change(1, "a");
        assert_eq!(c.change(1, "b"), None);
        assert_eq!(c.change(1, "c"), None);
    }

    #[test]
    fn the_send_carries_the_latest_state() {
        let mut c = Coalescer::default();
        c.change(1, "a");
        c.change(1, "b");
        c.change(1, "c");
        assert_eq!(c.fire(&1), Some("c"));
    }

    #[test]
    fn after_a_send_the_next_change_schedules_again() {
        let mut c = Coalescer::default();
        c.change(1, "a");
        assert_eq!(c.fire(&1), Some("a"));
        assert_eq!(c.change(1, "b"), Some(SEND_AFTER));
        assert_eq!(c.fire(&1), Some("b"));
    }

    #[test]
    fn connections_are_independent() {
        let mut c = Coalescer::default();
        assert_eq!(c.change(1, "a1"), Some(SEND_AFTER));
        assert_eq!(c.change(2, "b1"), Some(SEND_AFTER));
        c.change(1, "a2");
        assert_eq!(c.fire(&2), Some("b1"));
        assert_eq!(c.fire(&1), Some("a2"));
    }

    #[test]
    fn firing_with_nothing_waiting_sends_nothing() {
        let mut c: Coalescer<i32, &str> = Coalescer::default();
        assert_eq!(c.fire(&1), None);
        c.change(1, "a");
        c.fire(&1);
        assert_eq!(c.fire(&1), None);
    }

    #[test]
    fn a_forgotten_key_schedules_afresh() {
        let mut c = Coalescer::default();
        c.change(1, "a");
        c.forget(&1);
        assert_eq!(c.fire(&1), None);
        assert_eq!(c.change(1, "b"), Some(SEND_AFTER));
    }
}

//! When the worktree's errors call for the drawer.
//!
//! The rule (spec §4.4): the drawer opens when the error count rises above a
//! baseline. `observe` keeps two numbers, the `baseline` (the count at the
//! last moment the user looked at Problems or closed the drawer) and `last`
//! (the count the previous observation saw).
//!
//! - While `looking`, both follow the count and nothing opens.
//! - Otherwise a `Publish` lowers the baseline to the new count only when the
//!   count actually fell (`errors < last`): that is a fix. A count that only
//!   rises, however low it still is under the baseline, is a refill and
//!   moves nothing, which is what keeps a restored baseline from reading the
//!   store's refill after a relaunch as new errors.
//! - `ServerGone` records the count but never lowers the baseline: entries
//!   that vanish and return are not a fix.
//! - It returns whether `errors > baseline`; the baseline then waits for
//!   `seen` to move it.
//!
//! Ways this can fail, written before the code (spec §4.4):
//! - the first error of a worktree whose baseline is 0 does not open it;
//! - the server republishing the count the user already saw opens it;
//! - a rise above the baseline does not open it;
//! - fixes do not lower the baseline, so an agent that fixes 3 errors and
//!   breaks one stays unnoticed, or the baseline rises with the count so a
//!   later error is judged against a higher bar than the user saw;
//! - a server that exits or restarts counts as a fix and lowers the baseline,
//!   so its refilling entries read as new errors;
//! - while the user looks at Problems, the baseline stays behind the count
//!   and the drawer pops open the moment they leave;
//! - after a relaunch the store refills from zero, and the refill (which only
//!   rises) opens a drawer the user had closed.

/// What changed the count the alarm is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmCause {
    /// A server published diagnostics.
    Publish,
    /// A server exited or restarted, so its entries vanished.
    ServerGone,
}

#[derive(Debug, Clone, Copy)]
pub struct ErrorAlarm {
    baseline: usize,
    last: usize,
}

impl ErrorAlarm {
    /// An alarm whose baseline is the persisted one (0 for a worktree that
    /// has never been seen). The store refills from zero, so `last` is 0.
    pub fn new(baseline: usize) -> Self {
        Self { baseline, last: 0 }
    }

    pub fn baseline(&self) -> usize {
        self.baseline
    }

    /// The user looked at Problems or closed the drawer with `errors` listed.
    pub fn seen(&mut self, errors: usize) {
        self.baseline = errors;
        self.last = errors;
    }

    /// Whether the drawer should open now that the count is `errors`.
    pub fn observe(&mut self, errors: usize, cause: AlarmCause, looking: bool) -> bool {
        if looking {
            self.seen(errors);
            return false;
        }
        if cause == AlarmCause::Publish && errors < self.last {
            self.baseline = self.baseline.min(errors);
        }
        self.last = errors;
        errors > self.baseline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen_at(errors: usize) -> ErrorAlarm {
        let mut alarm = ErrorAlarm::new(0);
        alarm.seen(errors);
        alarm
    }

    #[test]
    fn first_error_from_zero_opens() {
        let mut alarm = ErrorAlarm::new(0);
        assert!(alarm.observe(1, AlarmCause::Publish, false));
    }

    #[test]
    fn same_count_republished_stays_closed() {
        let mut alarm = seen_at(3);
        assert!(!alarm.observe(3, AlarmCause::Publish, false));
    }

    #[test]
    fn a_rise_above_the_baseline_opens() {
        let mut alarm = seen_at(3);
        assert!(alarm.observe(4, AlarmCause::Publish, false));
    }

    #[test]
    fn fixes_lower_the_baseline_and_a_new_error_opens() {
        let mut alarm = seen_at(3);
        assert!(!alarm.observe(0, AlarmCause::Publish, false));
        assert_eq!(alarm.baseline(), 0);
        assert!(alarm.observe(1, AlarmCause::Publish, false));
    }

    #[test]
    fn server_gone_never_lowers_the_baseline() {
        let mut alarm = seen_at(3);
        assert!(!alarm.observe(0, AlarmCause::ServerGone, false));
        assert_eq!(alarm.baseline(), 3);
        assert!(!alarm.observe(3, AlarmCause::Publish, false));
    }

    #[test]
    fn while_looking_the_baseline_follows_the_count() {
        let mut alarm = seen_at(3);
        assert!(!alarm.observe(7, AlarmCause::Publish, true));
        assert_eq!(alarm.baseline(), 7);
        assert!(!alarm.observe(2, AlarmCause::Publish, true));
        assert_eq!(alarm.baseline(), 2);
        // Leaving with 2 errors, the same 2 do not open it.
        assert!(!alarm.observe(2, AlarmCause::Publish, false));
    }

    #[test]
    fn a_restored_baseline_suppresses_the_refill() {
        let mut alarm = ErrorAlarm::new(3);
        assert!(!alarm.observe(1, AlarmCause::Publish, false));
        assert!(!alarm.observe(2, AlarmCause::Publish, false));
        assert!(!alarm.observe(3, AlarmCause::Publish, false));
        assert_eq!(alarm.baseline(), 3);
    }
}

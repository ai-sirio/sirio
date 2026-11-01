//! The stop policy: one pure decision for the unread mark and the desktop
//! notification.

use crate::status::AgentStatus;

/// What is known about a pane's status record when its status goes from
/// nothing to something. Read only when the old status is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstStatus {
    /// No status is recorded for the tab: a tab created in this run.
    Unrecorded,
    /// The tab's status was recorded earlier in this run.
    Recorded { was_running: bool },
    /// The pane's first status since launch, against what an earlier run recorded.
    Restored { was_running: bool, chat: bool },
}

/// What a status change asks of the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StopDecision {
    /// Mark the session unread.
    pub unread: bool,
    /// Post a desktop notification.
    pub notify: bool,
}

/// The one rule for when an agent has stopped and somebody should hear of it.
pub struct StopPolicy;

impl StopPolicy {
    /// Decides the unread mark and the notification for one resolved status
    /// change. `first` is read only when `old` is `None`; `on_screen` is
    /// whether the pane is in front of the user right now.
    ///
    /// A stop is `Running` to a settled status, a settled status to `Error`,
    /// or a first settled status that follows `first`. Starting work, a
    /// status reasserted and a move between settled statuses other than into
    /// `Error` are never a stop. A terminal that stopped while Sirio was
    /// closed is a restore stop: marked, never notified.
    pub fn decide(
        old: Option<AgentStatus>,
        new: AgentStatus,
        first: FirstStatus,
        on_screen: bool,
    ) -> StopDecision {
        if new == AgentStatus::Running || old == Some(new) {
            return StopDecision::default();
        }
        let (stop, restore) = match old {
            Some(AgentStatus::Running) => (true, false),
            Some(_) => (new == AgentStatus::Error, false),
            None => match first {
                FirstStatus::Unrecorded => (true, false),
                FirstStatus::Recorded { was_running } => {
                    (was_running || new == AgentStatus::Error, false)
                }
                FirstStatus::Restored {
                    was_running: true,
                    chat: false,
                } => (true, true),
                FirstStatus::Restored { .. } => (false, false),
            },
        };
        if !stop {
            return StopDecision::default();
        }
        StopDecision {
            unread: !on_screen,
            notify: !on_screen && !restore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::AgentStatus::{Done, Error, NeedsInput, Running};

    const BOTH: StopDecision = StopDecision {
        unread: true,
        notify: true,
    };

    fn all_firsts() -> [FirstStatus; 7] {
        [
            FirstStatus::Unrecorded,
            FirstStatus::Recorded { was_running: false },
            FirstStatus::Recorded { was_running: true },
            FirstStatus::Restored {
                was_running: true,
                chat: false,
            },
            FirstStatus::Restored {
                was_running: false,
                chat: true,
            },
            FirstStatus::Restored {
                was_running: false,
                chat: false,
            },
            FirstStatus::Restored {
                was_running: true,
                chat: true,
            },
        ]
    }

    #[test]
    fn running_to_settled_marks_and_notifies() {
        for new in [NeedsInput, Done, Error] {
            assert_eq!(
                StopPolicy::decide(Some(Running), new, FirstStatus::Unrecorded, false),
                BOTH,
                "Running -> {new:?}"
            );
        }
    }

    #[test]
    fn a_stop_on_screen_does_neither() {
        assert_eq!(
            StopPolicy::decide(Some(Running), Done, FirstStatus::Unrecorded, true),
            StopDecision::default()
        );
    }

    #[test]
    fn starting_work_is_never_a_stop() {
        for old in [None, Some(Done), Some(NeedsInput), Some(Error)] {
            for first in all_firsts() {
                assert_eq!(
                    StopPolicy::decide(old, Running, first, false),
                    StopDecision::default(),
                    "{old:?} -> Running with {first:?}"
                );
            }
        }
    }

    #[test]
    fn reasserting_a_status_is_not_a_stop() {
        for status in [Done, Error] {
            for first in all_firsts() {
                assert_eq!(
                    StopPolicy::decide(Some(status), status, first, false),
                    StopDecision::default(),
                    "{status:?} -> {status:?} with {first:?}"
                );
            }
        }
    }

    #[test]
    fn ending_a_turn_then_exiting_stops_once() {
        for first in all_firsts() {
            assert_eq!(
                StopPolicy::decide(Some(NeedsInput), Done, first, false),
                StopDecision::default(),
                "NeedsInput -> Done with {first:?}"
            );
        }
    }

    #[test]
    fn failing_after_settling_is_a_stop() {
        for old in [Done, NeedsInput] {
            assert_eq!(
                StopPolicy::decide(Some(old), Error, FirstStatus::Unrecorded, false),
                BOTH,
                "{old:?} -> Error"
            );
        }
    }

    #[test]
    fn a_first_settled_status_of_a_new_tab_is_a_stop() {
        assert_eq!(
            StopPolicy::decide(None, NeedsInput, FirstStatus::Unrecorded, false),
            BOTH
        );
    }

    #[test]
    fn a_first_status_after_a_clear_follows_the_record() {
        assert_eq!(
            StopPolicy::decide(
                None,
                Done,
                FirstStatus::Recorded { was_running: false },
                false
            ),
            StopDecision::default()
        );
        assert_eq!(
            StopPolicy::decide(
                None,
                Done,
                FirstStatus::Recorded { was_running: true },
                false
            ),
            BOTH
        );
        assert_eq!(
            StopPolicy::decide(
                None,
                Error,
                FirstStatus::Recorded { was_running: false },
                false
            ),
            BOTH
        );
    }

    #[test]
    fn a_terminal_that_stopped_while_closed_is_marked_quietly() {
        let restored = FirstStatus::Restored {
            was_running: true,
            chat: false,
        };
        assert_eq!(
            StopPolicy::decide(None, Done, restored, false),
            StopDecision {
                unread: true,
                notify: false
            }
        );
        // On screen it is not marked either.
        assert_eq!(
            StopPolicy::decide(None, Done, restored, true),
            StopDecision::default()
        );
    }

    #[test]
    fn a_restored_chat_is_never_marked() {
        assert_eq!(
            StopPolicy::decide(
                None,
                Done,
                FirstStatus::Restored {
                    was_running: true,
                    chat: true
                },
                false
            ),
            StopDecision::default()
        );
    }

    #[test]
    fn a_restored_idle_tab_is_not_marked() {
        for new in [NeedsInput, Error] {
            assert_eq!(
                StopPolicy::decide(
                    None,
                    new,
                    FirstStatus::Restored {
                        was_running: false,
                        chat: false
                    },
                    false
                ),
                StopDecision::default(),
                "restored idle -> {new:?}"
            );
        }
    }
}

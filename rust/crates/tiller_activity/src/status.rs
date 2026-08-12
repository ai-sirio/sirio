//! The agent lifecycle status enum and the transition type, ported from
//! `TillerCore/Models.swift` (`AgentStatus`, `AgentTransition`).

/// The lifecycle status of an agent pane, driving the sidebar badge.
///
/// Mirrors Swift's `AgentStatus` raw values and priority order: error >
/// needs-input > running > done.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentStatus {
    Running,
    NeedsInput,
    Done,
    Error,
}

impl AgentStatus {
    /// The status a process exit code maps to: 0 → done, anything else →
    /// error. This is the only path to `.done` for agents without native
    /// hooks.
    pub fn from_exit_code(code: i32) -> Self {
        if code == 0 {
            AgentStatus::Done
        } else {
            AgentStatus::Error
        }
    }

    /// Human-readable label, matching the Swift `humanLabel`.
    pub fn human_label(self) -> &'static str {
        match self {
            AgentStatus::Running => "running",
            AgentStatus::NeedsInput => "needs input",
            AgentStatus::Done => "finished",
            AgentStatus::Error => "failed",
        }
    }

    /// Priority for aggregation: error > needs-input > running > done.
    /// Lower value = higher priority, mirroring `AgentStatus.priorityOrder`.
    pub fn priority(self) -> u8 {
        match self {
            AgentStatus::Error => 0,
            AgentStatus::NeedsInput => 1,
            AgentStatus::Running => 2,
            AgentStatus::Done => 3,
        }
    }

    /// The highest-priority status in the collection, or `None` if empty.
    pub fn highest_priority<'a>(
        statuses: impl IntoIterator<Item = &'a AgentStatus>,
    ) -> Option<AgentStatus> {
        statuses
            .into_iter()
            .min_by_key(|status| status.priority())
            .copied()
    }
}

/// A status transition for one pane, returned by the model so the caller
/// can decide whether to post a notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub pane_id: String,
    pub old: Option<AgentStatus>,
    pub new: AgentStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_map_to_done_and_error() {
        assert_eq!(AgentStatus::from_exit_code(0), AgentStatus::Done);
        assert_eq!(AgentStatus::from_exit_code(1), AgentStatus::Error);
        assert_eq!(AgentStatus::from_exit_code(-9), AgentStatus::Error);
    }

    #[test]
    fn priority_order_is_error_needs_input_running_done() {
        assert!(AgentStatus::Error.priority() < AgentStatus::NeedsInput.priority());
        assert!(AgentStatus::NeedsInput.priority() < AgentStatus::Running.priority());
        assert!(AgentStatus::Running.priority() < AgentStatus::Done.priority());
    }

    #[test]
    fn highest_priority_picks_the_winner() {
        assert_eq!(
            AgentStatus::highest_priority(
                [
                    AgentStatus::Running,
                    AgentStatus::Done,
                    AgentStatus::NeedsInput
                ]
                .iter()
            ),
            Some(AgentStatus::NeedsInput)
        );
        assert_eq!(
            AgentStatus::highest_priority([AgentStatus::Running, AgentStatus::Error].iter()),
            Some(AgentStatus::Error)
        );
        assert_eq!(AgentStatus::highest_priority(std::iter::empty()), None);
    }

    #[test]
    fn human_labels_match_swift() {
        assert_eq!(AgentStatus::Running.human_label(), "running");
        assert_eq!(AgentStatus::NeedsInput.human_label(), "needs input");
        assert_eq!(AgentStatus::Done.human_label(), "finished");
        assert_eq!(AgentStatus::Error.human_label(), "failed");
    }
}

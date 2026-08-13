//! Display-facing activity state derived from an optional agent status.

use crate::status::AgentStatus;

/// Activity-panel state. `Idle` represents a live shell or chat with no
/// agent status, which is distinct from an agent's four lifecycle states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActivityStatus {
    Running,
    NeedsInput,
    Done,
    Error,
    Idle,
}

impl ActivityStatus {
    /// Maps the model's optional agent status to a display state.
    pub fn from_agent_status(status: Option<AgentStatus>) -> Self {
        match status {
            Some(AgentStatus::Running) => Self::Running,
            Some(AgentStatus::NeedsInput) => Self::NeedsInput,
            Some(AgentStatus::Done) => Self::Done,
            Some(AgentStatus::Error) => Self::Error,
            None => Self::Idle,
        }
    }

    /// Whether closing this activity can terminate live work and therefore
    /// needs confirmation first.
    pub fn requires_close_confirmation(self) -> bool {
        matches!(self, Self::Running | Self::NeedsInput | Self::Error)
    }
}

impl From<Option<AgentStatus>> for ActivityStatus {
    fn from(status: Option<AgentStatus>) -> Self {
        Self::from_agent_status(status)
    }
}

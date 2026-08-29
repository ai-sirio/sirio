//! Stable activity urgency sorting.

use crate::status::AgentStatus;

/// Stable ordering helpers for activity-like values.
pub struct AttentionSort;

impl AttentionSort {
    /// Sorts by error, needs-input, running, done, then idle while preserving
    /// input order for equal ranks.
    pub fn sorted<T: Clone>(items: &[T], status_of: impl Fn(&T) -> Option<AgentStatus>) -> Vec<T> {
        let mut sorted = items.to_vec();
        sorted.sort_by_key(|item| rank(status_of(item)));
        sorted
    }

    /// Moves only error and needs-input rows ahead of the caller's manual
    /// order. Relative order among urgent rows follows lifecycle priority;
    /// all non-urgent rows retain their manual order.
    pub fn urgent_first<T: Clone>(
        items: &[T],
        status_of: impl Fn(&T) -> Option<AgentStatus>,
    ) -> Vec<T> {
        let mut sorted = items.to_vec();
        sorted.sort_by_key(|item| urgency_rank(status_of(item)));
        sorted
    }
}

fn rank(status: Option<AgentStatus>) -> u8 {
    status.map_or(4, AgentStatus::priority)
}

fn urgency_rank(status: Option<AgentStatus>) -> u8 {
    match status {
        Some(AgentStatus::Error) => 0,
        Some(AgentStatus::NeedsInput) => 1,
        Some(AgentStatus::Running | AgentStatus::Done) | None => 2,
    }
}

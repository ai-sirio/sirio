//! Pure notification decisions and payload formatting.

use crate::status::AgentStatus;

/// Data passed to a platform notification backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationPayload {
    pub pane_id: String,
    pub worktree_id: String,
    pub title: String,
    pub body: String,
}

/// Policy for suppressing noisy or already-visible activity transitions.
pub struct NotificationPolicy;

impl NotificationPolicy {
    /// Returns true when a transition deserves a desktop notification.
    pub fn should_notify(
        old: Option<AgentStatus>,
        new: AgentStatus,
        app_active: bool,
        pane_visible: bool,
    ) -> bool {
        if new == AgentStatus::Running || old == Some(new) {
            return false;
        }
        !(app_active && pane_visible)
    }
}

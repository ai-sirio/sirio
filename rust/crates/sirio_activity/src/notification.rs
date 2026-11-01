//! Notification payload formatting.

/// Data passed to a platform notification backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationPayload {
    pub pane_id: String,
    pub worktree_id: String,
    pub title: String,
    pub body: String,
}

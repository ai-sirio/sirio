//! Notification payload.

/// Data passed to a platform notification backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationPayload {
    /// The persistence id of the session the notification is about; empty for
    /// one that belongs to no session (`sirioctl notify`).
    pub session_id: String,
    pub worktree_id: String,
    pub title: String,
    pub body: String,
}

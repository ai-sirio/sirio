//! The [`Tab`] value type: a surface open inside a worktree.

use crate::{TabId, WorktreeId};

/// What kind of surface a [`Tab`] hosts.
///
/// Mirrors the `LegacyTabContent` cases of the Swift app that survive in the
/// Rust rewrite: terminal splits, agent chat panes, and the file surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    /// A terminal (possibly split into multiple panes).
    Terminal,
    /// A chat pane with an AI coding agent.
    AgentChat,
    /// A browser surface.
    Browser,
    /// A file editor.
    Editor,
    /// A diff view.
    Diff,
}

/// A tab open inside a worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    /// Stable identity within a [`crate::Workspace`].
    pub id: TabId,
    /// The worktree this tab lives in.
    pub worktree_id: WorktreeId,
    /// The tab's title as shown in the sidebar and tab bar.
    pub title: String,
    /// The kind of surface this tab hosts.
    pub kind: TabKind,
}

impl Tab {
    /// Returns a new tab with the given fields.
    pub fn new(
        id: TabId,
        worktree_id: WorktreeId,
        title: impl Into<String>,
        kind: TabKind,
    ) -> Self {
        Self {
            id,
            worktree_id,
            title: title.into(),
            kind,
        }
    }
}

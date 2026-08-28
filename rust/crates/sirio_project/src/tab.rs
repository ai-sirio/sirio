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

impl TabKind {
    /// Whether a tab of this kind is listed under its worktree in the
    /// sidebar (#125, decided in #130).
    ///
    /// A Browser tab is a tab like any other in the tab bar, and **never a
    /// sidebar row**: the sidebar lists the work a worktree contains, and a
    /// web page is not that. It appeared there anyway, drawn with the chat
    /// icon through the sidebar's icon catch-all, which is what made the
    /// omission look deliberate.
    ///
    /// Exhaustive on purpose — no `_` arm. A new surface kind must state
    /// which side of this line it falls on rather than inheriting an answer
    /// from whichever branch happened to be last.
    pub fn appears_in_sidebar(self) -> bool {
        match self {
            Self::Terminal | Self::AgentChat | Self::Editor | Self::Diff => true,
            Self::Browser => false,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::TabKind;

    /// #125/#130: the sidebar lists the work a worktree contains, and a web
    /// page is not that. A Browser tab appeared there anyway, drawn with the
    /// chat icon through the sidebar's icon catch-all — which is exactly why
    /// it read as intentional rather than as the omission it was.
    #[test]
    fn a_browser_tab_is_never_a_sidebar_row() {
        assert!(!TabKind::Browser.appears_in_sidebar());
    }

    /// The other four stay. Spelled out one by one rather than as a loop, so
    /// adding a surface kind fails to compile here until someone decides
    /// which side of the line it belongs on.
    #[test]
    fn every_other_surface_kind_still_appears() {
        assert!(TabKind::Terminal.appears_in_sidebar());
        assert!(TabKind::AgentChat.appears_in_sidebar());
        assert!(TabKind::Editor.appears_in_sidebar());
        assert!(TabKind::Diff.appears_in_sidebar());
    }
}

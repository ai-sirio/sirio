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

/// Which half of the [center split] a surface belongs to.
///
/// The two halves have fixed roles: which pane a tab is drawn in is
/// *derived* from what the tab is ([`TabKind::pane_role`]), never chosen
/// and never stored. Primary holds what the user talks to — terminals and
/// agent chats; Secondary holds what the user looks at — a browser, an
/// editor, a diff. Despite the name neither is a [pane](crate) in the
/// glossary's reserved sense (the split tree inside one tab); see
/// `CONTEXT.md`, "Primary pane role" / "Secondary pane role".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneRole {
    /// Always shown; takes the whole work area when Secondary is not.
    Primary,
    /// Absent by default; opens with the first Secondary tab and auto-
    /// closes with the last.
    Secondary,
}

impl TabKind {
    /// The center-split half this surface is drawn in. Rigid by design —
    /// there is no move-between-panes gesture, because membership and
    /// surface kind are the same fact read twice.
    ///
    /// Exhaustive on purpose — no `_` arm. A new surface kind must state
    /// which half it belongs to rather than inheriting an answer from
    /// whichever branch happened to be last.
    pub fn pane_role(self) -> PaneRole {
        match self {
            Self::Terminal | Self::AgentChat => PaneRole::Primary,
            Self::Browser | Self::Editor | Self::Diff => PaneRole::Secondary,
        }
    }

    /// Whether a tab of this kind is listed under its worktree in the
    /// sidebar (#125, decided in #130; narrowed by #319/#322).
    ///
    /// Deliberately *derived* from [`Self::pane_role`] rather than kept as
    /// a second list: the sidebar lists the work a worktree contains —
    /// exactly the Primary pane's tabs — and one fact gets one definition.
    /// A browser tab was never a sidebar row; editor and diff tabs joined
    /// it when they moved to the Secondary half, where the right sidebar's
    /// own Changes and Files listings already cover them.
    pub fn appears_in_sidebar(self) -> bool {
        self.pane_role() == PaneRole::Primary
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
    use super::{PaneRole, TabKind};

    /// #125/#130: the sidebar lists the work a worktree contains, and a web
    /// page is not that. A Browser tab appeared there anyway, drawn with the
    /// chat icon through the sidebar's icon catch-all — which is exactly why
    /// it read as intentional rather than as the omission it was.
    #[test]
    fn a_browser_tab_is_never_a_sidebar_row() {
        assert!(!TabKind::Browser.appears_in_sidebar());
    }

    /// #319: the routing rule is the spine of the center split. Terminal
    /// and chat are what the user talks to (Primary); browser, editor and
    /// diff are what the user looks at (Secondary).
    #[test]
    fn pane_role_routes_by_surface_kind() {
        assert_eq!(TabKind::Terminal.pane_role(), PaneRole::Primary);
        assert_eq!(TabKind::AgentChat.pane_role(), PaneRole::Primary);
        assert_eq!(TabKind::Browser.pane_role(), PaneRole::Secondary);
        assert_eq!(TabKind::Editor.pane_role(), PaneRole::Secondary);
        assert_eq!(TabKind::Diff.pane_role(), PaneRole::Secondary);
    }

    /// The sidebar predicate is *derived* from the routing rule, not a
    /// second list that could drift from it: exactly the Primary kinds are
    /// listed. Spelled out one by one rather than as a loop, so adding a
    /// surface kind fails to compile here until someone decides which side
    /// of the line it belongs on.
    #[test]
    fn the_sidebar_lists_exactly_the_primary_kinds() {
        assert!(TabKind::Terminal.appears_in_sidebar());
        assert!(TabKind::AgentChat.appears_in_sidebar());
        assert!(!TabKind::Browser.appears_in_sidebar());
        assert!(!TabKind::Editor.appears_in_sidebar());
        assert!(!TabKind::Diff.appears_in_sidebar());
    }
}

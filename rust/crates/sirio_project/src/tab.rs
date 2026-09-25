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
    /// A project's settings form. Like the other Secondary surfaces it is
    /// something the user looks at rather than talks to, so it lives in
    /// the Secondary half of the center split.
    ProjectSettings,
}

/// Which half of the [center split] a tab is drawn in.
///
/// Every kind has a *home* half ([`TabKind::default_pane`]) that a new tab
/// opens in. Terminals and agent chats may be moved to the other half
/// ([`TabKind::can_move_between_panes`]); every other kind always stays
/// home. Where a live tab actually is lives on the tab, not here. Despite the
/// name neither half is a [pane](crate) in the glossary's reserved sense (the
/// split tree inside one tab); see `CONTEXT.md`, "Primary pane role" /
/// "Secondary pane role".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneRole {
    /// The left half. Always shown; takes the whole work area when
    /// Secondary is not.
    Primary,
    /// The right half. Drawn from the first frame, even empty (it then shows
    /// a launcher); the keyboard toggle hides it and the choice is stored
    /// per worktree.
    Secondary,
}

impl PaneRole {
    /// The half across the divider.
    pub fn other(self) -> Self {
        match self {
            Self::Primary => Self::Secondary,
            Self::Secondary => Self::Primary,
        }
    }
}

impl TabKind {
    /// The half a new tab of this kind opens in: Primary for what the user
    /// talks to, Secondary for what the user looks at.
    ///
    /// Exhaustive on purpose — no `_` arm. A new surface kind must state its
    /// home rather than inheriting an answer from whichever branch happened
    /// to be last.
    pub fn default_pane(self) -> PaneRole {
        match self {
            Self::Terminal | Self::AgentChat => PaneRole::Primary,
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings => {
                PaneRole::Secondary
            }
        }
    }

    /// Whether a tab of this kind may be moved to the other half (spec
    /// 2026-09-24). Only what the user talks to moves; a browser, editor,
    /// diff or settings form always stays in the Secondary half.
    /// Exhaustive on purpose.
    pub fn can_move_between_panes(self) -> bool {
        match self {
            Self::Terminal | Self::AgentChat => true,
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings => false,
        }
    }

    /// Whether a tab of this kind is listed under its worktree in the
    /// sidebar (#125, decided in #130; narrowed by #319/#322).
    ///
    /// The sidebar lists the work a worktree contains — its terminals and
    /// chats — wherever those are drawn. It is no longer derived from the
    /// half: a terminal moved to the right half is still that work. A browser
    /// tab was never a sidebar row; editor and diff tabs are covered by the
    /// right sidebar's own Changes and Files listings. Exhaustive on purpose.
    pub fn appears_in_sidebar(self) -> bool {
        match self {
            Self::Terminal | Self::AgentChat => true,
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings => false,
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
    use super::{PaneRole, TabKind};

    /// #125/#130: the sidebar lists the work a worktree contains, and a web
    /// page is not that. A Browser tab appeared there anyway, drawn with the
    /// chat icon through the sidebar's icon catch-all — which is exactly why
    /// it read as intentional rather than as the omission it was.
    #[test]
    fn a_browser_tab_is_never_a_sidebar_row() {
        assert!(!TabKind::Browser.appears_in_sidebar());
    }

    /// #319: every kind has a home half. Terminal and chat are what the user
    /// talks to (Primary); browser, editor, diff and settings are what the
    /// user looks at (Secondary). A new tab opens in its home half.
    #[test]
    fn default_pane_routes_by_surface_kind() {
        assert_eq!(TabKind::Terminal.default_pane(), PaneRole::Primary);
        assert_eq!(TabKind::AgentChat.default_pane(), PaneRole::Primary);
        assert_eq!(TabKind::Browser.default_pane(), PaneRole::Secondary);
        assert_eq!(TabKind::Editor.default_pane(), PaneRole::Secondary);
        assert_eq!(TabKind::Diff.default_pane(), PaneRole::Secondary);
        assert_eq!(TabKind::ProjectSettings.default_pane(), PaneRole::Secondary);
    }

    /// Spec 2026-09-24 §1: only what the user talks to may leave its home
    /// half. Spelled out kind by kind, not looped, so a new kind does not
    /// compile until someone decides which side of the line it is on.
    #[test]
    fn only_terminals_and_chats_move_between_panes() {
        assert!(TabKind::Terminal.can_move_between_panes());
        assert!(TabKind::AgentChat.can_move_between_panes());
        assert!(!TabKind::Browser.can_move_between_panes());
        assert!(!TabKind::Editor.can_move_between_panes());
        assert!(!TabKind::Diff.can_move_between_panes());
        assert!(!TabKind::ProjectSettings.can_move_between_panes());
    }

    /// The sidebar lists a worktree's terminals and chats *wherever they are
    /// drawn*: a terminal moved to the right half is still work the worktree
    /// contains. Spelled out one by one for the same reason as above.
    #[test]
    fn the_sidebar_lists_terminals_and_chats() {
        assert!(TabKind::Terminal.appears_in_sidebar());
        assert!(TabKind::AgentChat.appears_in_sidebar());
        assert!(!TabKind::Browser.appears_in_sidebar());
        assert!(!TabKind::Editor.appears_in_sidebar());
        assert!(!TabKind::Diff.appears_in_sidebar());
        assert!(!TabKind::ProjectSettings.appears_in_sidebar());
    }

    #[test]
    fn other_is_the_half_across_the_divider() {
        assert_eq!(PaneRole::Primary.other(), PaneRole::Secondary);
        assert_eq!(PaneRole::Secondary.other(), PaneRole::Primary);
    }
}

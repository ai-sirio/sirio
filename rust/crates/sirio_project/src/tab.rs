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
    /// A pull or merge request, read-only (spec 2026-09-27). Something the
    /// user looks at, so it lives in the Secondary half; the right panel
    /// lists change requests itself, so the left sidebar does not.
    ChangeRequest,
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
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings | Self::ChangeRequest => {
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
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings | Self::ChangeRequest => false,
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
            Self::Browser | Self::Editor | Self::Diff | Self::ProjectSettings | Self::ChangeRequest => false,
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

//! The project/worktree/tab sidebar.
//!
//! This first Rust implementation deliberately owns a small fixture model. The
//! real project store will be connected by the integrator; keeping the view
//! model flat here makes filtering and variable-height rendering deterministic.
//!
//! Worktree creation and removal are real: the "+ New Worktree..." row opens
//! a branch-name prompt, creates the worktree with `sirio_git` on the
//! background executor (never the render thread), and inserts the new row
//! without a refresh; a hover "×" on a worktree row removes it. The fixture
//! project "sirio" is backed by the repository given in
//! `SIRIO_SIDEBAR_REPO`; projects without a repository path are not offered
//! a New Worktree row, exactly like non-git projects.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bezel::motion::{Fade, Painter};
use bezel::ui::popover::{self, Popup};
use bezel::ui::tree;
use gpui::{
    App, Context, DragMoveEvent, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    MouseButton, MouseDownEvent, PathPromptOptions, Point, PromptLevel, Render, Rgba,
    StyleRefinement, Window, div, img, prelude::*, px, rgb,
};
use sirio_git::{create_worktree, derive_worktree_path, remove_worktree, resolve_parent_directory};
use sirio_project::{TabKind, display_absolute_path, display_path};
use sirio_theme::{AgentBrandColor, Theme};

use crate::caret;
use crate::loading;
use crate::project_forms::{CloneForm, CloneFormEvent, CreateForm, CreateFormEvent};
use crate::project_identity::{AvatarSource, ProjectIcon, ProjectIconPicker, ProjectIconValue};
use crate::row_reorder::{ReorderScope, RowDrag, accepts_drop, insertion_index};
use crate::tab_bar::NewTabAction;

#[path = "icons.rs"]
pub mod icons;

use self::icons::{Icon, IconElement, IconSize};
use crate::right_panel::ActivityStatus;

/// One agent's brand mark: the silhouette **and** the colour it is drawn in,
/// carried together so the two can never disagree about which agent a row is
/// showing.
///
/// The reference has a single `AgentIcon` view used by the worktree badge,
/// the sidebar tab row and the tab bar alike, so a mark looks the same
/// wherever it appears. This port had drifted into three different tints for
/// the same mark — `theme.text` in the tab bar, `theme.warning` on
/// sidebar tab rows, `theme.text` in the worktree badge — and the
/// last of those is Claude's own brand coral, so every agent's mark was
/// wearing Claude's colour. Pairing the icon with its brand at the type level
/// is what makes one rule enforceable across all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentMark {
    /// The silhouette.
    pub icon: Icon,
    /// The brand it is drawn in. Ignored for a chromatic asset that paints
    /// its own colours (`Icon::is_chromatic`, currently only omp's gradient),
    /// exactly as the reference's `OmpShape` ignores any inherited tint.
    pub brand: AgentBrandColor,
}

impl AgentMark {
    /// The mark for an `AgentCatalog` id, or `None` for an id with no brand
    /// silhouette (a plain shell, or an adapter this port has no asset for).
    pub fn for_agent_id(agent_id: &str) -> Option<Self> {
        Some(Self {
            icon: Icon::for_agent_id(agent_id)?,
            brand: AgentBrandColor::for_agent_id(agent_id),
        })
    }
}

/// What the leading status column of a worktree (or collapsed project) row
/// draws — a port of `TillerCore/SidebarGlyph.swift`'s `SidebarGlyphKind`,
/// with the extra `Idle` case the Rust `ActivityStatus` carries folded onto
/// the same `None` the Swift `nil` status maps to.
///
/// The Swift table is the contract, and two of its rows had been inverted
/// here: `Idle` drew the amber needs-input dot, so a worktree with nothing
/// happening was pixel-identical to one waiting on an answer, and `Running`
/// drew nothing at all, so a busy worktree looked empty. Both now follow
/// `SidebarGlyphKind.forStatus`: nothing for no status, the loader for
/// running, a lifecycle dot for the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RowStatusGlyph {
    /// No glyph. The column keeps its width so rows stay aligned.
    None,
    /// The running indicator, tinted with the agent's **brand** — Swift's
    /// `RunningDots(color: AgentIcon.color(for: agentId))`, whose whole
    /// purpose is to say *whose* work is in progress.
    Running(Rgba),
    /// A static lifecycle dot: amber needs-input, green done, red error.
    Dot(Rgba),
}

impl RowStatusGlyph {
    fn for_status(
        status: Option<ActivityStatus>,
        brand: Option<AgentBrandColor>,
        theme: Theme,
    ) -> Self {
        match status {
            None | Some(ActivityStatus::Idle) => Self::None,
            // The tint is the agent's brand, never a `Theme` status token.
            // Routed through the eight-token settings palette it used to be
            // one — Claude resolved to `Amber`, i.e. to `tab_needs_input` —
            // so a *running* Claude worktree and one that *needed input*
            // painted the same `#E0B36A` and differed only by dot geometry.
            // An unidentified agent gets the neutral fallback, matching
            // `AgentIcon.color(for: agentId ?? "")`'s `.gray`.
            Some(ActivityStatus::Running) => {
                Self::Running(brand.unwrap_or(AgentBrandColor::Unknown).color())
            }
            Some(ActivityStatus::NeedsInput) => Self::Dot(theme.warning),
            Some(ActivityStatus::Done) => Self::Dot(theme.success),
            Some(ActivityStatus::Error) => Self::Dot(theme.danger),
        }
    }
}

/// `SidebarRow::id` for a tab row built from real, host-owned tab data is
/// this offset plus the tab's own id. Real tab ids and the fixture/catalog's
/// hand- and index-assigned ids both start low, so without an offset a tab
/// row could collide with — and be toggled by clicking — an unrelated row.
/// The offset is far past anything a session will reach.
pub const TAB_ROW_ID_OFFSET: usize = 1_000_000;

/// Row-id base for parked tab rows, kept clear of both the worktree row ids
/// (`project_index * 1000 + worktree_index + 1`) and the live tab rows above.
/// A parked tab has no `OpenTab::id`, so its row is identified by the
/// worktree it hangs under and its position in that worktree's strip — see
/// [`parked_tab_row_id`].
pub const PARKED_TAB_ROW_ID_OFFSET: usize = 100_000_000;

/// The row id of the `index`-th parked tab under worktree row `worktree_id`.
/// Unique across worktrees, unlike an `OpenTab::id`, which every worktree
/// numbers from zero.
pub fn parked_tab_row_id(worktree_id: usize, index: usize) -> usize {
    PARKED_TAB_ROW_ID_OFFSET + worktree_id * 1000 + index
}

/// What a sidebar tab row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarTabRef {
    /// A tab in the host's live list: its `OpenTab::id`, the id
    /// [`SidebarEvent::SelectTab`] and [`SidebarEvent::CloseTab`] carry.
    Open(usize),
    /// A tab of a worktree the host has switched away from: it is no longer
    /// mounted, but the host still lists it from the worktree's persisted
    /// strip so the sidebar keeps showing what that worktree holds. The
    /// value is its 0-based position in that strip; a click reports it back
    /// as [`SidebarEvent::SelectParkedTab`].
    Parked(usize),
}

/// One tab, as the host (`main.rs`) knows it. For a live tab this is the
/// same fact the tab bar and the Activity panel already render — the
/// sidebar's tab rows are a third view of it, not a second copy. For a
/// parked one it is the persisted strip the host would restore on selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarTab {
    /// Which tab this is: live, or parked with its worktree.
    pub tab: SidebarTabRef,
    /// The tab's title, e.g. "Chat" or "Terminal".
    pub title: String,
    /// Whether this is the workspace's active tab.
    pub selected: bool,
    /// The tab's semantic kind. Icons are derived from this value rather
    /// than guessed from the user-visible title.
    pub kind: TabKind,
    /// The agent's brand mark, when this tab belongs to an agent. A plain
    /// terminal or chat keeps the surface icon instead.
    ///
    /// **This is a live fact, not a spawn-time one.** The host recomputes it
    /// every `sync_activity` from `AgentActivityModel`'s `pane_agents`, so a
    /// pane whose agent is identified *after* it started — by Layer B's OSC
    /// title or Layer D's process walk, which is how every agent Sirio did
    /// not itself spawn gets identified — grows its brand mark here as soon
    /// as it is known. That is exactly what `WorkspaceTabIcon` does in the
    /// reference: it reads `model.agentActivity.paneAgents[paneId]` at
    /// render time and never caches. Taking it from a field fixed at spawn
    /// is what left every tab row under an identified worktree still drawing
    /// the generic terminal glyph.
    pub agent: Option<AgentMark>,
}

/// Width the sidebar draws itself at until the host says otherwise, and the
/// same number as `AppSettings::sidebar_width`'s default. Every fixture in
/// this file's tests, and the demo sidebar, keep it: only the running shell
/// pushes a different one, through [`Sidebar::set_panel_width`].
const DEFAULT_SIDEBAR_WIDTH: f32 = 325.0;
const FILTER_LEFT_INSET: f32 = 20.0;
const ROW_RIGHT_INSET: f32 = 7.0;

pub(crate) const ROW_HEIGHT: f32 = 32.0;
/// Single-line row title line height (13.5px at waku's row ratio).
pub(crate) const ROW_TITLE_LINE_HEIGHT: f32 = 18.0;
/// Two-line card context line height (11.5px).
pub(crate) const ROW_SUB_LINE_HEIGHT: f32 = 15.0;
/// Legacy content rhythm retained for the sidebar conformance inventory.
#[cfg(test)]
pub(crate) const ROW_V_PADDING: f32 = 7.0;
/// Gap between a card's title and context lines.
pub(crate) const ROW_GAP: f32 = 4.0;
/// Vertical seam between row cards in the tree. The hover and selection
/// fills paint across a card's whole box, so cards that touch merge into
/// one highlight block; this keeps them reading as separate cards.
pub(crate) const ROW_V_GAP: f32 = 2.0;
/// Two-line card height: 7 + 18 + 4 + 15 + 7 — waku's session-card math.
pub(crate) const CARD_TWO_LINE_HEIGHT: f32 = 51.0;
/// A visible row in the flattened sidebar tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    /// Stable fixture identity used by click handlers.
    pub id: usize,
    /// The row's semantic kind.
    pub kind: RowKind,
    /// Tree depth, where projects are depth zero.
    pub depth: usize,
    /// Display title.
    pub title: String,
    /// Whether this row is selected.
    pub selected: bool,
    /// Whether this project or worktree row's disclosure is open. A project
    /// hides its whole section when closed; a worktree hides its tab rows.
    /// Meaningless for the other kinds.
    pub expanded: bool,
    /// The application-level primary marker for a worktree row.
    pub is_primary: bool,
    /// The worktree's live agent status, driving the small status dot next
    /// to its glyph. Only meaningful for `RowKind::Worktree`; host-pushed,
    /// the same way `RightPanel::set_activity` is — this crate never
    /// resolves status itself.
    pub agent_status: Option<ActivityStatus>,
    /// Whether this project row is a git repository. Only meaningful for
    /// `RowKind::Project`; the New Worktree row is offered only for git
    /// projects with a repository path.
    pub is_git: bool,
    /// The repository root for a project row, or the checkout path for a
    /// worktree row — the path git operations run against.
    pub path: Option<PathBuf>,
    /// The real `OpenTab::id` this row was built from, for a `RowKind::Tab`
    /// row sourced from [`Sidebar::set_worktree_tabs`] with a
    /// [`SidebarTabRef::Open`]. `None` for every other row, including the
    /// decorative fixture rows: those keep the old local-selection click
    /// behaviour, this doesn't.
    pub tab_id: Option<usize>,
    /// The strip position of a parked tab, for a `RowKind::Tab` row sourced
    /// from a [`SidebarTabRef::Parked`]. Such a row carries its worktree's
    /// checkout path in `path` (the only tab row that does), so its click
    /// can report [`SidebarEvent::SelectParkedTab`] without a lookup.
    /// Exclusive with `tab_id`.
    pub parked_tab: Option<usize>,
    /// The semantic kind for a real tab row. `None` for non-tab rows and
    /// legacy fixture rows that do not represent a host-owned tab.
    pub tab_kind: Option<TabKind>,
    /// The running agent's brand mark for a real tab row, if any. This is
    /// the **tab** row's mark, matching the Swift original, where the agent
    /// brand belongs to the tab and the worktree row keeps its branch
    /// glyph. A worktree row never sets it.
    pub agent_icon: Option<Icon>,
    /// The brand of the agent **this row is about** — one fact with one
    /// drawing job per row kind:
    ///
    /// * `RowKind::Worktree` — F-CORE-ACT-17: the tint of the running
    ///   indicator, and nothing else, mirroring
    ///   `WorktreeStatusGlyph(status:agentId:)`, whose `agentId` argument
    ///   reaches exactly one thing: `RunningDots(color:)`.
    /// * `RowKind::Tab` — the colour of [`Self::agent_icon`]'s brand mark,
    ///   mirroring the `AgentIcon` that `WorkspaceTabIcon` draws.
    pub agent_brand: Option<AgentBrandColor>,
    /// F-SID-11: the worktree's durable comment annotation, carried through
    /// from `SidebarWorktree::comment`. Only meaningful for
    /// `RowKind::Worktree`.
    pub comment: Option<String>,
    /// F-CORE-ACT-18: one brand mark per *distinct agent currently running*
    /// in this worktree, already de-duplicated and in `AgentCatalog` order
    /// by `AgentActivityModel::running_agent_ids`. Only meaningful for
    /// `RowKind::Worktree`; drawn as the row's trailing badge. Empty when
    /// nothing is running — this is strictly the `.running` set, never
    /// done/error/needs-input (those are the leading status dot's job).
    pub running_agents: Vec<AgentMark>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarProject {
    pub id: String,
    pub name: String,
    /// Whether the project is a git repository — a non-git project has no
    /// worktrees and is not offered the New Worktree row.
    pub is_git: bool,
    /// The project's root on disk, needed to create a worktree from its row.
    pub root_path: PathBuf,
    pub worktrees: Vec<SidebarWorktree>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarWorktree {
    pub branch: String,
    pub path: PathBuf,
    pub is_primary: bool,
    /// F-SID-11: the durable `worktree.comment` annotation (`worktree.set`
    /// over the control socket), already persisted and read by the status
    /// bar — the worktree row itself never rendered it. `None` when no
    /// comment has ever been set for this path.
    pub comment: Option<String>,
}

/// Stable target carried by a sidebar context action. Paths and catalog ids
/// are the host's source of truth; row numbers are only drawing identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarContextTarget {
    Project {
        id: String,
        path: PathBuf,
        is_git: bool,
    },
    Worktree {
        path: PathBuf,
        is_primary: bool,
    },
}

/// The transitions exposed by a project/worktree context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarContextAction {
    ProjectSettings,
    RefreshProject,
    InitializeGit,
    RevealInFileManager,
    RemoveProject,
    SetPrimary,
    UnsetPrimary,
    /// F-SID-15: the context menu's confirm-gated counterpart to the
    /// hover-x button, which used to call `remove_worktree_row` (a
    /// real on-disk deletion) directly with no confirmation at all.
    RemoveWorktree,
    NewTab(NewTabAction),
}

/// Typed explanation for an unavailable context-menu command. The reason is
/// rendered beside the disabled item instead of leaving a grey mystery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarDisabledReason {
    AlreadyGitProject,
    /// #372: the primary checkout cannot be `git worktree remove`d —
    /// deleting its directory would destroy the repository itself.
    PrimaryWorktree,
}

impl std::fmt::Display for SidebarDisabledReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyGitProject => formatter.write_str("Git is already initialized"),
            Self::PrimaryWorktree => {
                formatter.write_str("The primary worktree cannot be removed")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarContextItem {
    pub label: &'static str,
    pub action: SidebarContextAction,
    pub enabled: bool,
    pub disabled_reason: Option<SidebarDisabledReason>,
}

#[derive(Clone)]
struct OpenContextMenu {
    target: SidebarContextTarget,
    position: Point<gpui::Pixels>,
}

#[derive(Clone)]
struct ProjectSettingsCard {
    id: String,
    name: String,
    display_name: Rc<RefCell<String>>,
    display_name_focus: FocusHandle,
    path: PathBuf,
    is_git: bool,
    icon: Rc<RefCell<ProjectIcon>>,
    icon_picker: gpui::Entity<ProjectIconPicker>,
    /// F-PRJ-17: the pinned base branch draft; empty means "follow the
    /// primary worktree" (`primary_branch`), matching the Swift
    /// `WorktreeBaseSection`'s `effectiveBase` fallback chain.
    default_worktree_base: Rc<RefCell<String>>,
    worktree_base_focus: FocusHandle,
    /// The primary worktree's branch, snapshotted when the sheet opens —
    /// display-only, used for the "Following primary (…)" subtitle.
    primary_branch: Option<String>,
    /// F-PRJ-18: the checkout-location override draft; empty means "the
    /// project's sibling directory" (`card.path`'s parent).
    worktree_location_override: Rc<RefCell<String>>,
    worktree_location_focus: FocusHandle,
}

#[derive(Clone)]
enum ProjectFormSurface {
    Clone(gpui::Entity<CloneForm>),
    Create(gpui::Entity<CreateForm>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    AddProject(PathBuf),
    RemoveProject(String),
    /// Select the open tab with this id (an `OpenTab::id`, not a row id).
    SelectTab(usize),
    /// The user clicked a worktree row. The host owns which worktree is
    /// selected — the sidebar only reports the click, carrying the
    /// worktree's checkout path — and answers with
    /// [`Sidebar::set_selected_worktree`] so the highlight follows the
    /// host's decision, not the other way around.
    SelectWorktree(PathBuf),
    /// The user clicked a parked tab row ([`SidebarTabRef::Parked`]): the
    /// `index`-th tab of the persisted strip of the worktree at `path`,
    /// which is not the selected worktree. The host selects that worktree
    /// (restoring its strip) and then activates the tab at that position;
    /// there is no live tab id to name.
    SelectParkedTab { path: PathBuf, index: usize },
    /// A worktree was created successfully on disk. The host must refresh
    /// the owning catalog before accepting the new path as selectable.
    WorktreeCreated { project_id: String, path: PathBuf },
    /// A worktree was removed successfully on disk. The host must refresh
    /// the owning catalog and control state before rebuilding its rows.
    WorktreeRemoved { project_id: String, path: PathBuf },
    /// Close the open tab with this id.
    CloseTab(usize),
    /// Open the project settings sheet for a catalog project.
    OpenProjectSettings(String),
    /// A project-settings edit has changed the durable project identity.
    ProjectSettingsChanged(ProjectSettingsUpdate),
    /// A typed project/worktree context-menu transition for the shell.
    ContextAction {
        target: SidebarContextTarget,
        action: SidebarContextAction,
    },
    /// The row order is already updated locally while dragging. The host
    /// receives only the final gesture so it can persist the same order.
    Reorder {
        drag: RowDrag,
        target_id: usize,
        before: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSettingsUpdate {
    pub id: String,
    pub display_name: Option<String>,
    pub is_git: bool,
    pub icon: ProjectIcon,
    /// F-PRJ-17: the pinned base branch, or `None` to follow the primary
    /// worktree.
    pub default_worktree_base: Option<String>,
    /// F-PRJ-18: the checkout-location override, or `None` for the
    /// project's sibling directory.
    pub worktree_location_override: Option<String>,
}

/// The kinds of rows rendered by [`Sidebar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A top-level project.
    Project,
    /// A git worktree or plain folder.
    Worktree,
    /// An agent or terminal tab.
    Tab,
    /// The action row below an expanded project.
    NewWorktree,
}

/// Which field of the [`WorktreePrompt`] is receiving keystrokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorktreePromptField {
    /// The (required) branch-name field.
    Branch,
    /// The optional base-branch override (F-PRJ-17); empty falls back to
    /// the project's pinned default and only then to HEAD (F-CORE-DOM-02).
    Base,
    /// The optional checkout-location override (F-PRJ-18); empty falls back
    /// to the project's pinned default and only then to the sibling
    /// directory `resolve_parent_directory` uses on its own (F-CORE-DOM-02).
    Location,
}

/// The open branch-name prompt for creating a worktree.
#[derive(Clone)]
struct WorktreePrompt {
    /// The project row the new worktree belongs to.
    project_row_id: usize,
    /// The project's display name, used in the derived checkout folder.
    project_name: String,
    /// The repository root to create the worktree in.
    repo_root: PathBuf,
    /// The branch name being typed.
    draft: String,
    /// The optional base-branch override being typed (F-PRJ-17).
    base_draft: String,
    /// The optional checkout-location override being typed (F-PRJ-18).
    location_draft: String,
    /// F-CORE-DOM-02: the project's pinned "Default Worktree Base", snapshot
    /// from `project_worktree_defaults` when the prompt opened. Falls back
    /// to this — not straight to HEAD — when `base_draft` is left blank,
    /// mirroring the Swift app's `WorktreeDefaults.resolveBase` (an explicit
    /// per-project override wins; there is no per-dialog field in Swift at
    /// all, so `base_draft` is this Rust port's own additive one-off
    /// override on top of it).
    default_base: Option<String>,
    /// F-CORE-DOM-02: the project's pinned "Worktree Location", snapshot the
    /// same way. Falls back to this — not straight to the sibling directory
    /// — when `location_draft` is left blank, mirroring
    /// `WorktreeDefaults.resolveParentDirectory`.
    default_location_override: Option<String>,
    /// Which of the three fields above Tab/keystrokes currently target.
    focused_field: WorktreePromptField,
    /// A creation error to show under the field, if the last attempt failed.
    error: Option<String>,
    /// Focus for the prompt's text field.
    focus: FocusHandle,
}

impl WorktreePrompt {
    /// The draft string that keystrokes currently target.
    fn focused_draft_mut(&mut self) -> &mut String {
        match self.focused_field {
            WorktreePromptField::Branch => &mut self.draft,
            WorktreePromptField::Base => &mut self.base_draft,
            WorktreePromptField::Location => &mut self.location_draft,
        }
    }
}

/// A fixture-backed project sidebar.
pub struct Sidebar {
    rows: Vec<SidebarRow>,
    project_ids: std::collections::HashMap<usize, String>,
    project_names: std::collections::HashMap<String, String>,
    project_identities: std::collections::HashMap<String, ProjectIcon>,
    /// F-PRJ-17/F-PRJ-18: persisted per-project worktree defaults
    /// (`default_worktree_base`, `worktree_location_override`), pushed in by
    /// the host the same way `project_identities` is — reset on every
    /// `set_projects` and refilled by `set_project_worktree_defaults`.
    project_worktree_defaults: std::collections::HashMap<String, (Option<String>, Option<String>)>,
    filter: String,
    filter_focus: FocusHandle,
    tree_cursor: usize,
    tree_focus: FocusHandle,
    /// Shared blink state for every sidebar text field's insertion caret
    /// (filter, project-settings card, worktree prompt). One is enough:
    /// window focus is unique. Visibility is recomputed each render.
    field_blink: caret::Blink,
    /// The open worktree-creation prompt, if any.
    prompt: Option<WorktreePrompt>,
    /// Checkout paths of the worktrees whose disclosure the user closed.
    /// Keyed by path rather than row id because rows are rebuilt from the
    /// catalog on every `set_projects`; see [`Self::set_row_expanded`].
    /// Session-scoped: not persisted.
    collapsed_worktrees: std::collections::HashSet<PathBuf>,
    /// A transient error message (failed creation/removal) shown at the
    /// bottom of the sidebar.
    notice: Option<String>,
    context_menu: Popup<OpenContextMenu>,
    context_menu_focus: FocusHandle,
    project_settings: Option<ProjectSettingsCard>,
    add_project_menu: Popup<()>,
    project_form: Option<ProjectFormSurface>,
    pending_reorder: Option<(RowDrag, usize, bool)>,
    /// The panel's current width, pushed in by the host each render — the
    /// same arrangement `RightPanel::set_panel_width` uses, and for the same
    /// reason: a view cannot measure its own container, and reading the last
    /// drawn frame would lag a frame behind every drag.
    panel_width: f32,
    /// One view per visible row, keyed by row id, so each row is a cached
    /// subtree of its own: the spinner lease of a running worktree notifies
    /// only that row's view, and the sidebar's own layout becomes a list of
    /// fixed-height leaves gpui replays. Pruned to the visible rows on every
    /// render.
    row_views: std::collections::HashMap<usize, gpui::Entity<RowView>>,
    /// Whether rows are mounted through `Entity::cached`. On in the app; the
    /// host turns it off for drawn tests, whose `debug_bounds` probes are not
    /// carried through a replayed subtree. Only pays off when the host mounts
    /// the sidebar itself *uncached*: gpui re-renders a cached view's whole
    /// subtree with `window.refreshing` set, so a cached sidebar — dirty on
    /// every spinner frame as the spinner's ancestor — would drag every row
    /// along with it.
    cache_rows: bool,
}

/// What one row renders from — a copy the sidebar pushes in, compared before
/// it notifies, so an unchanged row stays a replayed subtree.
#[derive(Clone, PartialEq)]
struct RowInputs {
    row: SidebarRow,
    index: usize,
    cursor: bool,
    /// Whether the row has rows under it in the full tree — a worktree's
    /// tab rows, which may be hidden by its own disclosure. Decides the
    /// tree shape (`Sidebar::tree_row`), so it is an input like the rest.
    has_children: bool,
    project_id: Option<String>,
    project_icon: Option<ProjectIcon>,
    drag: Option<RowDrag>,
}

/// One sidebar row as its own view. It owns nothing but its inputs; every
/// handler still targets the sidebar entity it holds, exactly as the row did
/// when the sidebar rendered it inline. Its render is where the running
/// spinner's lease lands, so a running worktree re-renders one row.
struct RowView {
    sidebar: gpui::Entity<Sidebar>,
    inputs: RowInputs,
    /// How many times gpui asked this row to render. Test-observable only.
    render_count: u64,
}

impl Render for RowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_count = self.render_count.wrapping_add(1);
        let theme = *Theme::get(cx);
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
        let inputs = self.inputs.clone();
        let row_shape = Sidebar::tree_row(&inputs.row, inputs.has_children);
        // Boxed here: the opaque return type of `render_row` captures the
        // borrow of `bezel_theme`, which ends with this frame's render.
        Sidebar::render_row(
            inputs.row,
            inputs.index,
            row_shape,
            inputs.cursor,
            inputs.project_id,
            inputs.project_icon,
            inputs.drag,
            self.sidebar.clone(),
            theme,
            &bezel_theme,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl Sidebar {
    /// Text for the worktree-location field, which is *not* a label: what it
    /// holds is persisted as the project's worktree base and handed to
    /// `resolve_parent_directory`, which takes it verbatim. So it strips
    /// Windows' verbatim prefix like every other user-facing path, but keeps
    /// the home directory spelled out — `Path::join` would treat a collapsed
    /// "~" as a directory of that name.
    fn worktree_location_text(path: &Path) -> String {
        display_absolute_path(path)
    }

    /// The host pushes the resolved sidebar width every render; same
    /// every-render push as `RightPanel::set_panel_width`, no-op when
    /// unchanged so a drag does not notify more than it must.
    pub fn set_panel_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.panel_width == width {
            return;
        }
        self.panel_width = width;
        cx.notify();
    }

    /// Creates the expanded fixture shown by the reference sidebar capture.
    pub fn new_for_demo(cx: &mut Context<Self>) -> Self {
        // The fixture's "sirio" project is backed by the repository the app
        // runs from when provided; without one, no New Worktree row is
        // offered (a project with no repository path behaves like a non-git
        // project).
        let sirio_repo = std::env::var_os("SIRIO_SIDEBAR_REPO").map(PathBuf::from);
        Self::new_with_repo(cx, sirio_repo)
    }

    /// The fixture with an explicit repository path for the "sirio"
    /// project — the test entry point (tests pass their scratch repo
    /// directly instead of racing a process-global env var).
    fn new_with_repo(cx: &mut Context<Self>, sirio_repo: Option<PathBuf>) -> Self {
        fn row(
            id: usize,
            kind: RowKind,
            depth: usize,
            title: &str,
            selected: bool,
            expanded: bool,
            path: Option<PathBuf>,
        ) -> SidebarRow {
            SidebarRow {
                id,
                kind,
                depth,
                title: title.to_string(),
                selected,
                expanded,
                is_primary: false,
                agent_status: None,
                is_git: true,
                path,
                tab_id: None,
                parked_tab: None,
                tab_kind: None,
                agent_icon: None,
                agent_brand: None,
                comment: None,
                running_agents: Vec::new(),
            }
        }

        let worktree_path = sirio_repo.clone();
        let mut rows = vec![
            row(0, RowKind::Project, 0, "sirio", true, true, sirio_repo),
            row(1, RowKind::Worktree, 1, "main", true, true, worktree_path),
            row(2, RowKind::Tab, 2, "Chat", true, false, None),
            row(
                3,
                RowKind::NewWorktree,
                1,
                "New Worktree...",
                false,
                false,
                None,
            ),
            row(
                4,
                RowKind::Project,
                0,
                "cricchetto-firma-e-digitalizzazione-bff-app",
                false,
                false,
                None,
            ),
            row(5, RowKind::Worktree, 1, "main", false, true, None),
            row(6, RowKind::Tab, 2, "Terminal", false, false, None),
            row(
                7,
                RowKind::Project,
                0,
                "Project-Tracker",
                false,
                false,
                None,
            ),
            row(8, RowKind::Project, 0, "source", false, false, None),
        ];
        rows[2].tab_kind = Some(TabKind::AgentChat);
        rows[6].tab_kind = Some(TabKind::Terminal);

        Self {
            rows,
            project_ids: std::collections::HashMap::new(),
            project_names: std::collections::HashMap::new(),
            project_identities: std::collections::HashMap::new(),
            project_worktree_defaults: std::collections::HashMap::new(),
            filter: String::new(),
            filter_focus: cx.focus_handle().tab_stop(true),
            tree_cursor: 0,
            tree_focus: cx.focus_handle().tab_stop(true),
            field_blink: caret::Blink::new(),
            prompt: None,
            collapsed_worktrees: std::collections::HashSet::new(),
            notice: None,
            context_menu: Popup::default(),
            context_menu_focus: cx.focus_handle().tab_stop(true),
            project_settings: None,
            add_project_menu: Popup::default(),
            project_form: None,
            pending_reorder: None,
            panel_width: DEFAULT_SIDEBAR_WIDTH,
            row_views: std::collections::HashMap::new(),
            cache_rows: !cfg!(test),
        }
    }

    pub fn from_projects(projects: Vec<SidebarProject>, cx: &mut Context<Self>) -> Self {
        let mut rows = Vec::new();
        let mut project_ids = std::collections::HashMap::new();
        let mut project_names = std::collections::HashMap::new();
        let mut project_identities = std::collections::HashMap::new();
        for (project_index, project) in projects.into_iter().enumerate() {
            let project_row_id = project_index * 1000;
            project_ids.insert(project_row_id, project.id.clone());
            project_names.insert(project.id.clone(), project.name.clone());
            project_identities.insert(project.id, ProjectIcon::default());
            let project_is_git = project.is_git;
            let project_path = project.root_path.clone();
            rows.push(SidebarRow {
                id: project_row_id,
                kind: RowKind::Project,
                depth: 0,
                title: project.name,
                // Only the first worktree of the first project starts
                // selected: the selected row is one fact, and a project row
                // is a container, not a selection.
                selected: false,
                expanded: true,
                is_primary: false,
                agent_status: None,
                is_git: project_is_git,
                path: Some(project_path),
                tab_id: None,
                parked_tab: None,
                tab_kind: None,
                agent_icon: None,
                agent_brand: None,
                comment: None,
                running_agents: Vec::new(),
            });
            let worktree_count = project.worktrees.len();
            for (worktree_index, worktree) in project.worktrees.into_iter().enumerate() {
                rows.push(SidebarRow {
                    id: project_row_id + worktree_index + 1,
                    kind: RowKind::Worktree,
                    depth: 1,
                    title: worktree.branch,
                    selected: project_index == 0 && worktree_index == 0,
                    expanded: true,
                    is_primary: worktree.is_primary,
                    agent_status: None,
                    // Worktree rows inherit the project's repo-ness; the
                    // worktree actions hang off the project row.
                    is_git: project_is_git,
                    path: Some(worktree.path),
                    tab_id: None,
                    parked_tab: None,
                    tab_kind: None,
                    agent_icon: None,
                    agent_brand: None,
                    comment: worktree.comment,
                    running_agents: Vec::new(),
                });
            }
            if project_is_git {
                rows.push(SidebarRow {
                    id: project_row_id + worktree_count + 1,
                    kind: RowKind::NewWorktree,
                    depth: 1,
                    title: "New Worktree...".to_string(),
                    selected: false,
                    expanded: false,
                    is_primary: false,
                    agent_status: None,
                    is_git: true,
                    path: None,
                    tab_id: None,
                    parked_tab: None,
                    tab_kind: None,
                    agent_icon: None,
                    agent_brand: None,
                    comment: None,
                    running_agents: Vec::new(),
                });
            }
        }
        Self {
            rows,
            project_ids,
            project_names,
            project_identities,
            project_worktree_defaults: std::collections::HashMap::new(),
            filter: String::new(),
            filter_focus: cx.focus_handle().tab_stop(true),
            tree_cursor: 0,
            tree_focus: cx.focus_handle().tab_stop(true),
            field_blink: caret::Blink::new(),
            prompt: None,
            collapsed_worktrees: std::collections::HashSet::new(),
            notice: None,
            context_menu: Popup::default(),
            context_menu_focus: cx.focus_handle().tab_stop(true),
            project_settings: None,
            add_project_menu: Popup::default(),
            project_form: None,
            pending_reorder: None,
            panel_width: DEFAULT_SIDEBAR_WIDTH,
            row_views: std::collections::HashMap::new(),
            cache_rows: !cfg!(test),
        }
    }

    pub fn set_projects(&mut self, projects: Vec<SidebarProject>, cx: &mut Context<Self>) {
        let filter = std::mem::take(&mut self.filter);
        let replacement = Self::from_projects(projects, cx);
        self.rows = replacement.rows;
        self.project_ids = replacement.project_ids;
        self.project_names = replacement.project_names;
        self.project_identities = replacement.project_identities;
        self.project_worktree_defaults = replacement.project_worktree_defaults;
        self.filter = filter;
        self.pending_reorder = None;
        self.apply_collapsed_worktrees();
        // F-PRJ-12: an already-open Project Settings card snapshots
        // is_git/path once, when it's opened (open_project_settings). If
        // the rebuilt rows above changed that same project -- e.g.
        // "Initialize Git" flipped it from a folder to a repo -- patch the
        // live card in place so the open sheet doesn't keep showing the
        // stale repo type/path underneath the (still correct) display-name
        // field.
        if let Some(card) = self.project_settings.as_ref() {
            let fresh = self
                .project_ids
                .iter()
                .find_map(|(row_id, id)| (id == &card.id).then_some(*row_id))
                .and_then(|row_id| self.rows.iter().find(|row| row.id == row_id))
                .map(|row| (row.is_git, row.path.clone()));
            if let Some((is_git, path)) = fresh
                && let Some(card) = self.project_settings.as_mut()
            {
                card.is_git = is_git;
                if let Some(path) = path {
                    card.path = path;
                }
            }
        }
        cx.notify();
    }

    fn reorder_group_for_row(&self, row_id: usize, kind: RowKind) -> Option<usize> {
        let row_index = self.rows.iter().position(|row| row.id == row_id)?;
        match kind {
            RowKind::Project => None,
            RowKind::Worktree => self.rows[..=row_index]
                .iter()
                .rposition(|row| row.kind == RowKind::Project)
                .map(|index| self.rows[index].id),
            RowKind::Tab => self.rows[..=row_index]
                .iter()
                .rposition(|row| row.kind == RowKind::Worktree)
                .map(|index| self.rows[index].id),
            RowKind::NewWorktree => None,
        }
    }

    fn row_drag(&self, row: &SidebarRow) -> Option<RowDrag> {
        let scope = match row.kind {
            RowKind::Project => ReorderScope::Projects,
            RowKind::Worktree => ReorderScope::Worktrees,
            RowKind::Tab => ReorderScope::Tabs,
            RowKind::NewWorktree => return None,
        };
        Some(RowDrag {
            scope,
            id: row.id,
            group: self.reorder_group_for_row(row.id, row.kind),
        })
    }

    fn reorder_rows(&mut self, drag: RowDrag, target_id: usize, before: bool) -> bool {
        let Some(target_index) = self.rows.iter().position(|row| row.id == target_id) else {
            return false;
        };
        let Some(source_index) = self.rows.iter().position(|row| row.id == drag.id) else {
            return false;
        };
        let target_kind = self.rows[target_index].kind;
        if !accepts_drop(
            drag,
            match target_kind {
                RowKind::Project => ReorderScope::Projects,
                RowKind::Worktree => ReorderScope::Worktrees,
                RowKind::Tab => ReorderScope::Tabs,
                RowKind::NewWorktree => return false,
            },
            self.reorder_group_for_row(target_id, target_kind),
        ) {
            return false;
        }
        if drag.scope == ReorderScope::Projects {
            if drag.id == target_id {
                return false;
            }
            let source_end = self.rows[source_index + 1..]
                .iter()
                .position(|row| row.kind == RowKind::Project)
                .map_or(self.rows.len(), |offset| source_index + 1 + offset);
            let block: Vec<_> = self.rows.drain(source_index..source_end).collect();
            let Some(target_index) = self.rows.iter().position(|row| row.id == target_id) else {
                return false;
            };
            let target_end = self.rows[target_index + 1..]
                .iter()
                .position(|row| row.kind == RowKind::Project)
                .map_or(self.rows.len(), |offset| target_index + 1 + offset);
            let insert_at = if before { target_index } else { target_end };
            self.rows.splice(insert_at..insert_at, block);
            return true;
        }

        let current_indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.kind == target_kind
                    && self.reorder_group_for_row(row.id, row.kind)
                        == self.reorder_group_for_row(target_id, target_kind)
            })
            .map(|(index, _)| index)
            .collect();
        let Some(source_position) = current_indices
            .iter()
            .position(|index| *index == source_index)
        else {
            return false;
        };
        let Some(target_position) = current_indices
            .iter()
            .position(|index| *index == target_index)
        else {
            return false;
        };
        let Some(insert_position) = insertion_index(
            current_indices.len(),
            source_position,
            target_position,
            before,
        ) else {
            return false;
        };
        let row = self.rows.remove(source_index);
        let remaining_indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.kind == target_kind
                    && self.reorder_group_for_row(candidate.id, candidate.kind)
                        == self.reorder_group_for_row(target_id, target_kind)
            })
            .map(|(index, _)| index)
            .collect();
        // `insert_position == remaining_indices.len()` means "insert after
        // the group's last remaining sibling" -- NOT "insert at the end of
        // `self.rows`". A worktree group is always followed by its
        // project's `NewWorktree` affordance row (and a tab group is
        // followed by whatever row comes after that worktree), so falling
        // back to `self.rows.len()` here used to drop the dragged row past
        // that trailing row instead of right after its new sibling.
        let insert_at = match remaining_indices.get(insert_position) {
            Some(&index) => index,
            None => remaining_indices
                .last()
                .map_or(self.rows.len(), |last| last + 1),
        };
        self.rows.insert(insert_at, row);
        true
    }

    fn preview_reorder(
        &mut self,
        drag: RowDrag,
        target_id: usize,
        before: bool,
        cx: &mut Context<Self>,
    ) {
        if self.reorder_rows(drag, target_id, before) {
            self.pending_reorder = Some((drag, target_id, before));
            cx.notify();
        }
    }

    fn confirm_reorder(&mut self, cx: &mut Context<Self>) {
        let Some((drag, target_id, before)) = self.pending_reorder.take() else {
            return;
        };
        cx.emit(SidebarEvent::Reorder {
            drag,
            target_id,
            before,
        });
    }

    /// Returns the complete context menu contract for a project or worktree.
    /// Disabled rows stay visible with their typed reason so the user can
    /// distinguish an unavailable transition from a missing affordance.
    pub fn context_menu_items(target: &SidebarContextTarget) -> Vec<SidebarContextItem> {
        let item = |label, action, enabled, disabled_reason| SidebarContextItem {
            label,
            action,
            enabled,
            disabled_reason,
        };
        match target {
            SidebarContextTarget::Project { is_git, .. } => vec![
                item(
                    "Project Settings",
                    SidebarContextAction::ProjectSettings,
                    true,
                    None,
                ),
                item(
                    "Refresh Project",
                    SidebarContextAction::RefreshProject,
                    true,
                    None,
                ),
                item(
                    "Initialize Git repository",
                    SidebarContextAction::InitializeGit,
                    !is_git,
                    is_git.then_some(SidebarDisabledReason::AlreadyGitProject),
                ),
                item(
                    "Show in File Manager",
                    SidebarContextAction::RevealInFileManager,
                    true,
                    None,
                ),
                item(
                    "Remove Project",
                    SidebarContextAction::RemoveProject,
                    true,
                    None,
                ),
            ],
            SidebarContextTarget::Worktree { is_primary, .. } => {
                let mut items = vec![item(
                    if *is_primary {
                        "Unset Primary"
                    } else {
                        "Set Primary"
                    },
                    if *is_primary {
                        SidebarContextAction::UnsetPrimary
                    } else {
                        SidebarContextAction::SetPrimary
                    },
                    true,
                    None,
                )];
                items.extend([
                    item(
                        "New Terminal",
                        SidebarContextAction::NewTab(NewTabAction::NewTerminal),
                        true,
                        None,
                    ),
                    item(
                        "Claude Code",
                        SidebarContextAction::NewTab(NewTabAction::ClaudeCode),
                        true,
                        None,
                    ),
                    item(
                        "Codex",
                        SidebarContextAction::NewTab(NewTabAction::Codex),
                        true,
                        None,
                    ),
                    item(
                        "OpenCode",
                        SidebarContextAction::NewTab(NewTabAction::OpenCode),
                        true,
                        None,
                    ),
                    item(
                        "Pi",
                        SidebarContextAction::NewTab(NewTabAction::Pi),
                        true,
                        None,
                    ),
                    item(
                        "Oh-My-Pi",
                        SidebarContextAction::NewTab(NewTabAction::OhMyPi),
                        true,
                        None,
                    ),
                    item(
                        "New Chat",
                        SidebarContextAction::NewTab(NewTabAction::NewChat),
                        true,
                        None,
                    ),
                    // F-SID-15: the only other removal path was the row's
                    // hover-x button, which had no confirmation state at
                    // all and deleted the on-disk worktree immediately.
                    // The context menu route is confirm-gated in
                    // dispatch_context_action; the hover-x button now goes
                    // through the same gate instead of bypassing it.
                    // #372: the primary checkout cannot be removed —
                    // `git worktree remove` refuses the main worktree and
                    // deleting its directory would destroy the repository.
                    item(
                        "Remove Worktree",
                        SidebarContextAction::RemoveWorktree,
                        !is_primary,
                        is_primary.then_some(SidebarDisabledReason::PrimaryWorktree),
                    ),
                ]);
                items
            }
        }
    }

    fn context_target(&self, row_id: usize) -> Option<SidebarContextTarget> {
        let row = self.rows.iter().find(|row| row.id == row_id)?;
        match row.kind {
            RowKind::Project => Some(SidebarContextTarget::Project {
                id: self.project_ids.get(&row_id)?.clone(),
                path: row.path.clone()?,
                is_git: row.is_git,
            }),
            RowKind::Worktree => Some(SidebarContextTarget::Worktree {
                path: row.path.clone()?,
                is_primary: row.is_primary,
            }),
            RowKind::Tab | RowKind::NewWorktree => None,
        }
    }

    fn open_context_menu(
        &mut self,
        row_id: usize,
        position: Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.context_target(row_id) {
            self.context_menu.open(OpenContextMenu { target, position });
            self.project_settings = None;
            self.context_menu_focus.focus(window, cx);
            cx.notify();
        }
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.begin_close() {
            popover::reap_popup(cx, |sidebar| &mut sidebar.context_menu);
            cx.notify();
        }
    }

    fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.get().is_some() {
            self.context_menu.close();
            cx.notify();
        }
    }

    /// Applies the host's persisted project identity to the live sidebar.
    /// This is called after initial construction and after catalog refreshes.
    pub fn set_project_identity(
        &mut self,
        project_id: &str,
        display_name: Option<String>,
        icon: ProjectIcon,
        cx: &mut Context<Self>,
    ) {
        self.project_identities.insert(project_id.to_string(), icon);
        let Some(row_id) = self
            .project_ids
            .iter()
            .find_map(|(row_id, id)| (id == project_id).then_some(*row_id))
        else {
            return;
        };
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == row_id) {
            let base_name = self
                .project_names
                .get(project_id)
                .cloned()
                .unwrap_or_else(|| row.title.clone());
            row.title = display_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(base_name);
        }
        cx.notify();
    }

    /// F-PRJ-17/F-PRJ-18: applies the host's persisted worktree-base and
    /// location-override for one project. Called the same way and at the
    /// same call sites as [`Self::set_project_identity`] — after initial
    /// construction and after catalog refreshes — so an already-open
    /// settings sheet's drafts stay in sync with what was actually saved.
    pub fn set_project_worktree_defaults(
        &mut self,
        project_id: &str,
        default_worktree_base: Option<String>,
        worktree_location_override: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.project_worktree_defaults.insert(
            project_id.to_string(),
            (default_worktree_base, worktree_location_override),
        );
        cx.notify();
    }

    fn project_settings_update(card: &ProjectSettingsCard) -> ProjectSettingsUpdate {
        ProjectSettingsUpdate {
            id: card.id.clone(),
            display_name: {
                let value = card.display_name.borrow().trim().to_string();
                (!value.is_empty()).then_some(value)
            },
            is_git: card.is_git,
            icon: card.icon.borrow().clone(),
            default_worktree_base: {
                let value = card.default_worktree_base.borrow().trim().to_string();
                (!value.is_empty()).then_some(value)
            },
            worktree_location_override: {
                let value = card.worktree_location_override.borrow().trim().to_string();
                (!value.is_empty()).then_some(value)
            },
        }
    }

    fn emit_project_settings_changed(&self, cx: &mut Context<Self>) {
        if let Some(card) = &self.project_settings {
            cx.emit(SidebarEvent::ProjectSettingsChanged(
                Self::project_settings_update(card),
            ));
        }
    }

    fn apply_icon_change(&mut self, project_id: String, icon: ProjectIcon, cx: &mut Context<Self>) {
        let Some(card) = self
            .project_settings
            .as_ref()
            .filter(|card| card.id == project_id)
        else {
            return;
        };
        *card.icon.borrow_mut() = icon.clone();
        self.project_identities.insert(project_id, icon);
        self.emit_project_settings_changed(cx);
        cx.notify();
    }

    fn on_display_name_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let Some(card) = &self.project_settings else {
            return;
        };
        let mut draft = card.display_name.borrow_mut();
        match event.keystroke.key.as_str() {
            "backspace" | "delete" => {
                draft.pop();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    draft.push_str(character);
                }
            }
        }
        drop(draft);
        let update = Self::project_settings_update(card);
        self.set_project_identity(
            &update.id,
            update.display_name.clone(),
            update.icon.clone(),
            cx,
        );
        cx.emit(SidebarEvent::ProjectSettingsChanged(update));
        cx.notify();
    }

    /// F-PRJ-17: keystrokes typed into the "Default Worktree Base" field.
    fn on_worktree_base_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let Some(card) = &self.project_settings else {
            return;
        };
        let mut draft = card.default_worktree_base.borrow_mut();
        match event.keystroke.key.as_str() {
            "backspace" | "delete" => {
                draft.pop();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    draft.push_str(character);
                }
            }
        }
        drop(draft);
        self.emit_project_settings_changed(cx);
        cx.notify();
    }

    /// F-PRJ-17: "Use Primary" clears the pin, restoring the "follow the
    /// primary worktree" fallback — mirrors the Swift `Button("Use Primary")`
    /// in `WorktreeBaseSection`, which calls
    /// `setProjectWorktreeBase(project, branch: nil)`.
    fn use_primary_worktree_base(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &self.project_settings else {
            return;
        };
        card.default_worktree_base.borrow_mut().clear();
        self.emit_project_settings_changed(cx);
        cx.notify();
    }

    /// F-PRJ-18: keystrokes typed into the "Worktree Location" field.
    fn on_worktree_location_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let Some(card) = &self.project_settings else {
            return;
        };
        let mut draft = card.worktree_location_override.borrow_mut();
        match event.keystroke.key.as_str() {
            "backspace" | "delete" => {
                draft.pop();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    draft.push_str(character);
                }
            }
        }
        drop(draft);
        self.emit_project_settings_changed(cx);
        cx.notify();
    }

    /// F-PRJ-18: "Restore Default" clears the override, restoring the
    /// project's sibling directory as the parent for new worktrees.
    fn restore_default_worktree_location(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &self.project_settings else {
            return;
        };
        card.worktree_location_override.borrow_mut().clear();
        self.emit_project_settings_changed(cx);
        cx.notify();
    }
}

/// What a platform path prompt came back with, reduced to the three cases the
/// surface actually distinguishes.
#[derive(Debug, PartialEq, Eq)]
enum PickedPath {
    /// The user chose a path.
    Chosen(PathBuf),
    /// Nothing to do, and nothing to say: the user cancelled, chose an empty
    /// selection, or the window went away with the prompt still open.
    Nothing,
    /// No chooser could be opened at all. Carries the reason, which must reach
    /// the user.
    Unavailable(String),
}

impl PickedPath {
    /// Classifies a raw prompt outcome.
    ///
    /// The raw type is `Result<Result<Option<Vec<PathBuf>>>, Canceled>`, which
    /// stacks three unrelated failures — the window went away, the platform
    /// could not open a chooser, the user cancelled — into one shape. The terse
    /// way to read it, `let Ok(Ok(Some(paths))) = outcome else { return }`, is
    /// correct for two of those and wrong for the third, and reads as though it
    /// handled all three. That is how `choose_worktree_location` came to
    /// swallow a missing XDG portal in silence while its two siblings reported
    /// it, and it is why this lives in one place now instead of three.
    fn from_prompt<E: std::fmt::Display, C>(
        outcome: Result<Result<Option<Vec<PathBuf>>, E>, C>,
    ) -> Self {
        match outcome {
            // A real selection. `prompt_for_paths` is asked for one directory,
            // so take the last and ignore any surprise extras.
            Ok(Ok(Some(mut paths))) => match paths.pop() {
                Some(path) => Self::Chosen(path),
                // An empty vector is a selection of nothing: same as cancel.
                None => Self::Nothing,
            },
            // Cancelled.
            Ok(Ok(None)) => Self::Nothing,
            // The platform could not open a chooser at all — no XDG portal on
            // this session, for example. Never silent: a control that is drawn,
            // clicked, and then does nothing is indistinguishable from a broken
            // app.
            Ok(Err(error)) => Self::Unavailable(error.to_string()),
            // The prompt was dropped with the window. Nobody is left to tell.
            Err(_) => Self::Nothing,
        }
    }
}

impl Sidebar {
    /// F-PRJ-18: "Choose…" opens the same platform folder picker
    /// `start_open_project` uses (the XDG portal on Linux, the system
    /// open-panel on macOS) and writes the chosen path into the draft.
    fn choose_worktree_location(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card_id) = self.project_settings.as_ref().map(|card| card.id.clone()) else {
            return;
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder for new worktrees".into()),
        });
        cx.spawn_in(window, async move |sidebar, cx| {
            let path = match PickedPath::from_prompt(receiver.await) {
                PickedPath::Chosen(path) => path,
                PickedPath::Nothing => return,
                PickedPath::Unavailable(reason) => {
                    let _ = sidebar.update(cx, |sidebar, cx| {
                        sidebar.notice =
                            Some(format!("could not open the folder picker: {reason}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = sidebar.update(cx, |sidebar, cx| {
                let Some(card) = sidebar.project_settings.as_ref() else {
                    return;
                };
                if card.id != card_id {
                    // The sheet was closed/reopened on a different project
                    // while the portal dialog was up.
                    return;
                }
                *card.worktree_location_override.borrow_mut() = Self::worktree_location_text(&path);
                sidebar.emit_project_settings_changed(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Host entry point for the gear affordance.
    pub fn open_project_settings(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(row_id) = self
            .project_ids
            .iter()
            .find_map(|(row_id, id)| (id == project_id).then_some(*row_id))
        else {
            return;
        };
        let Some(row) = self.rows.iter().find(|row| row.id == row_id) else {
            return;
        };
        let Some(path) = row.path.clone() else {
            return;
        };
        let row_title = row.title.clone();
        let row_is_git = row.is_git;
        self.close_context_menu(cx);
        let icon = Rc::new(RefCell::new(
            self.project_identities
                .get(project_id)
                .cloned()
                .unwrap_or_default(),
        ));
        let sidebar_entity = cx.entity();
        let picker_project_id = project_id.to_string();
        let icon_picker = cx.new(|cx| {
            ProjectIconPicker::with_value_and_repo(icon.borrow().clone(), &path, cx)
                .on_change_with_context(move |value, cx| {
                    sidebar_entity.update(cx, |sidebar, cx| {
                        sidebar.apply_icon_change(picker_project_id.clone(), value, cx)
                    });
                })
        });
        let base_name = self
            .project_names
            .get(project_id)
            .cloned()
            .unwrap_or_else(|| row_title.clone());
        let display_name = if row_title != base_name {
            row_title
        } else {
            Default::default()
        };
        // F-PRJ-17: the primary worktree's branch, for the "Following
        // primary (…)" subtitle — scanned from this project's own child
        // rows, the same `rows[project_index+1..]` traversal
        // `insert_worktree_row` already uses.
        let primary_branch = self
            .rows
            .iter()
            .position(|candidate| candidate.id == row_id)
            .and_then(|project_index| {
                self.rows[project_index + 1..]
                    .iter()
                    .take_while(|candidate| candidate.depth > 0)
                    .find(|candidate| candidate.kind == RowKind::Worktree && candidate.is_primary)
                    .map(|candidate| candidate.title.clone())
            });
        let (default_worktree_base, worktree_location_override) = self
            .project_worktree_defaults
            .get(project_id)
            .cloned()
            .unwrap_or_default();
        self.project_form = None;
        self.project_settings = Some(ProjectSettingsCard {
            id: project_id.to_string(),
            name: base_name,
            display_name: Rc::new(RefCell::new(display_name)),
            display_name_focus: cx.focus_handle(),
            path,
            is_git: row_is_git,
            icon,
            icon_picker,
            default_worktree_base: Rc::new(RefCell::new(default_worktree_base.unwrap_or_default())),
            worktree_base_focus: cx.focus_handle(),
            primary_branch,
            worktree_location_override: Rc::new(RefCell::new(
                worktree_location_override.unwrap_or_default(),
            )),
            worktree_location_focus: cx.focus_handle(),
        });
        cx.notify();
    }

    /// Whether one of the mutually-exclusive project-entry surfaces is open.
    /// The shell uses this for Escape handling even when keyboard focus still
    /// belongs to a terminal outside the sidebar.
    pub fn has_open_project_surface(&self) -> bool {
        self.project_settings.is_some() || self.project_form.is_some()
    }

    /// Dismisses whichever project-entry surface is open.
    pub fn close_project_surface(&mut self, cx: &mut Context<Self>) {
        let closed_settings = self.project_settings.take().is_some();
        let closed_form = self.project_form.take().is_some();
        if closed_settings || closed_form {
            cx.notify();
        }
    }

    pub fn set_notice(&mut self, notice: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(notice.into());
        cx.notify();
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    fn dispatch_context_action(
        &mut self,
        target: SidebarContextTarget,
        action: SidebarContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A chosen command is a completed transition, so unmount immediately;
        // keeping the exit overlay alive would occlude an immediate follow-up
        // right-click on the same row. Pointer dismissal still animates via
        // `close_context_menu`.
        self.context_menu.close();
        cx.notify();
        if action == SidebarContextAction::RemoveProject {
            if let SidebarContextTarget::Project { id, .. } = target {
                self.request_remove_project(id, window, cx);
            }
            return;
        }
        if action == SidebarContextAction::RemoveWorktree {
            if let SidebarContextTarget::Worktree { path, .. } = &target
                && let Some(row_id) = self
                    .rows
                    .iter()
                    .find(|row| {
                        row.kind == RowKind::Worktree && row.path.as_deref() == Some(path.as_path())
                    })
                    .map(|row| row.id)
            {
                self.request_remove_worktree_row(row_id, window, cx);
            }
            return;
        }
        cx.emit(SidebarEvent::ContextAction { target, action });
    }

    fn start_add_project(&mut self, cx: &mut Context<Self>) {
        if !self.add_project_menu.take_press_was_open() {
            self.add_project_menu.open(());
        }
        self.close_context_menu(cx);
        cx.notify();
    }

    fn close_add_project_menu(&mut self, cx: &mut Context<Self>) {
        if self.add_project_menu.begin_close() {
            popover::reap_popup(cx, |sidebar| &mut sidebar.add_project_menu);
            cx.notify();
        }
    }

    fn start_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_project_menu.close();
        // GPUI's platform path prompt is the one mechanism this codebase
        // opens a chooser with: it routes to the XDG portal on Linux and
        // the system open-panel on macOS. The sidebar only turns the picked
        // path into an event; the shell owns what happens next.
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        cx.spawn_in(window, async move |sidebar, cx| {
            match PickedPath::from_prompt(receiver.await) {
                // A real selection: a non-git folder gets a confirmation
                // prompt (F-PRJ-03) before it silently becomes a project —
                // adding a plain folder as a project when the user most
                // likely meant to pick their repo checkout is surprising,
                // and a project with no git backing loses worktrees,
                // branches, and every git-driven sidebar affordance.
                PickedPath::Chosen(path) => {
                    let _ = sidebar.update_in(cx, |sidebar, window, cx| {
                        sidebar.confirm_add_project(path, window, cx);
                    });
                }
                PickedPath::Nothing => {}
                PickedPath::Unavailable(reason) => {
                    let _ = sidebar.update(cx, |sidebar, cx| {
                        sidebar.notice =
                            Some(format!("could not open the folder picker: {reason}"));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// F-PRJ-03: a folder with no `.git` gets a three-way prompt — Initialize
    /// Git, Add without Git, Cancel — instead of silently becoming a
    /// project. A folder that is already a git checkout (the common case)
    /// is added immediately with no extra click.
    fn confirm_add_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path.join(".git").exists() {
            cx.emit(SidebarEvent::AddProject(path));
            return;
        }
        let receiver = window.prompt(
            PromptLevel::Info,
            "This folder is not a git repository",
            Some(
                "Sirio's worktrees, branches, and git-driven sidebar features need a git \
                 repository. You can initialize one here, or add the folder as-is.",
            ),
            &["Initialize Git", "Add without Git", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |sidebar, cx| {
            let choice = receiver.await.unwrap_or(2);
            let _ = sidebar.update(cx, |sidebar, cx| match choice {
                0 => {
                    if let Err(error) = std::process::Command::new("git")
                        .arg("init")
                        .arg("--quiet")
                        .current_dir(&path)
                        .status()
                    {
                        sidebar.notice = Some(format!("could not run git init: {error}"));
                        cx.notify();
                        return;
                    }
                    cx.emit(SidebarEvent::AddProject(path));
                }
                1 => cx.emit(SidebarEvent::AddProject(path)),
                _ => {}
            });
        })
        .detach();
    }

    /// F-CORE-DOM-03: the Clone/Create forms' starting `parent` is the
    /// deterministic default-project-location proposal — mirrors Swift's
    /// `ProjectDefaults.defaultProjectsRoot()` seeding `CreateNewProjectView`'s
    /// `parentDir` (`App/AddProjectSheet.swift:291`). A user with no
    /// configured default sees `$SIRIO_PROJECTS_DIR`, else
    /// `$XDG_DATA_HOME/Sirio/projects`, else `$HOME/Sirio/projects` —
    /// `sirio_project::default_project_base()` is the single source of
    /// truth for that proposal; forms remain free to override it via
    /// `set_parent` (the folder-picker "Change" affordance).
    fn project_form_parent() -> PathBuf {
        sirio_project::default_project_base()
    }

    /// Whether the Clone popover should stay open once a clone finishes.
    ///
    /// A truncated clone is still a success — the project is added to the
    /// sidebar either way (see `start_clone_project`) — but closing the
    /// popover in the same frame `CloneStatus::Complete`'s truncation notice
    /// appears would mean nobody ever reads it. An ordinary (non-truncated)
    /// completion keeps closing automatically, matching the pre-existing
    /// behavior; only a truncated one now leaves the form open until the
    /// user dismisses it via the existing Cancel affordance.
    fn clone_form_stays_open_after(truncated: bool) -> bool {
        truncated
    }

    fn start_clone_project(&mut self, cx: &mut Context<Self>) {
        self.add_project_menu.close();
        self.project_settings = None;
        let form = cx.new(|cx| CloneForm::new(Self::project_form_parent(), cx));
        cx.subscribe(
            &form,
            |sidebar, _, event: &CloneFormEvent, cx| match event {
                CloneFormEvent::Cloned {
                    destination,
                    truncated,
                } => {
                    if !Self::clone_form_stays_open_after(*truncated) {
                        sidebar.project_form = None;
                    }
                    cx.emit(SidebarEvent::AddProject(destination.clone()));
                    cx.notify();
                }
            },
        )
        .detach();
        self.project_form = Some(ProjectFormSurface::Clone(form));
        cx.notify();
    }

    fn start_create_project(&mut self, cx: &mut Context<Self>) {
        self.add_project_menu.close();
        self.project_settings = None;
        let form = cx.new(|cx| CreateForm::new(Self::project_form_parent(), cx));
        cx.subscribe(
            &form,
            |sidebar, _, event: &CreateFormEvent, cx| match event {
                CreateFormEvent::Created(path) => {
                    sidebar.project_form = None;
                    cx.emit(SidebarEvent::AddProject(path.clone()));
                    cx.notify();
                }
            },
        )
        .detach();
        self.project_form = Some(ProjectFormSurface::Create(form));
        cx.notify();
    }

    fn request_remove_project(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = window.prompt(
            PromptLevel::Warning,
            "Remove project from Sirio?",
            Some("This only removes the project from Sirio's sidebar. Files on disk will not be deleted."),
            &["Remove from Sirio", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |sidebar, cx| {
            if receiver.await.unwrap_or(1) == 0 {
                let _ =
                    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::RemoveProject(project_id)));
            }
        })
        .detach();
    }

    /// Mirrors SidebarView.swift: an explicit project colour wins when present;
    /// the fixture has no persisted override, so its stable fallback is the
    /// DJB2 hash of the project name modulo the source palette.
    fn project_color(name: &str) -> Rgba {
        let mut hash = 5381_i64;
        for byte in name.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(i64::from(byte));
        }
        // TODO(theme): expose the Swift project's semantic accent palette from
        // sirio_theme. These are the source-defined system colours used by
        // SidebarView's stable fallback.
        match hash.unsigned_abs() as usize % 8 {
            0 => rgb(0x007AFF), // blue
            1 => rgb(0xFF9500), // orange
            2 => rgb(0x34C759), // green
            3 => rgb(0xAF52DE), // purple
            4 => rgb(0xFF2D55), // pink
            5 => rgb(0x30B0C7), // teal
            6 => rgb(0x5856D6), // indigo
            _ => rgb(0xFFCC00), // yellow
        }
    }

    /// The minimum row rhythm: a single-line row is 32px (13.5px title at
    /// an 18px line height plus 7px of vertical padding — the action-row
    /// math); a card with a context line is 51px (7 + 18 + 4 + 15 + 7 —
    /// the session-card math). Content-sized titles grow beyond this floor.
    /// Whether a row draws a second line at all.
    ///
    /// The sub-line used to lead with the checkout path, which every project
    /// and worktree row had, so "is a card" and "has a path" were the same
    /// question. #151 dropped the path — it was almost always truncated,
    /// repeated the project prefix on every child, and bought its second
    /// line for every row in the tree. What remains on that line is the
    /// `Primary` pill and the F-SID-11 worktree comment, either of which may
    /// be absent, so both the sub-line and the taller height that pays for
    /// it now follow whether there is anything left to put there.
    ///
    /// The render and the height read this one predicate, so they cannot
    /// drift into disagreeing about whether a row has two lines.
    fn has_sub_line(row: &SidebarRow) -> bool {
        matches!(row.kind, RowKind::Project | RowKind::Worktree)
            && (row.is_primary
                || row
                    .comment
                    .as_ref()
                    .is_some_and(|comment| !comment.is_empty()))
    }

    fn row_min_height(row: &SidebarRow) -> f32 {
        if Self::has_sub_line(row) {
            CARD_TWO_LINE_HEIGHT
        } else {
            ROW_HEIGHT
        }
    }

    /// Blink timer tick shared by every sidebar text field's caret.
    fn flip_field_blink(&mut self, cx: &mut Context<Self>) {
        self.field_blink.flip();
        cx.notify();
    }

    fn on_filter_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.filter.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.filter.push_str(character);
        }
        cx.notify();
    }

    /// The structural row handed to bezel. Sirio keeps the data and content;
    /// bezel owns branch/leaf identity, indentation, disclosure and chrome.
    /// Whether rows are mounted as cached views. The host turns this off for
    /// drawn tests (see the field's doc); the app leaves it on.
    pub fn set_cache_rows(&mut self, cache: bool) {
        self.cache_rows = cache;
    }

    /// `(row id, kind, render count)` for every row view alive. Test-only:
    /// it lets the host prove that a spinner frame re-renders the running
    /// row and replays the others.
    #[doc(hidden)]
    pub fn row_render_counts(&self, cx: &App) -> Vec<(usize, RowKind, u64)> {
        let mut counts = self
            .row_views
            .iter()
            .map(|(id, view)| {
                let view = view.read(cx);
                (*id, view.inputs.row.kind, view.render_count)
            })
            .collect::<Vec<_>>();
        counts.sort_by_key(|(id, _, _)| *id);
        counts
    }

    /// The bezel tree shape of one row. A project is a container even when
    /// empty and always carries a chevron; a worktree earns one only while
    /// it has tab rows to hide (`has_children`), so an idle worktree with
    /// nothing under it does not grow a disclosure that opens onto nothing.
    fn tree_row(row: &SidebarRow, has_children: bool) -> tree::Row {
        match row.kind {
            RowKind::Project => tree::Row::branch(0, row.expanded),
            RowKind::Worktree if has_children => tree::Row::branch(1, row.expanded),
            RowKind::Worktree | RowKind::NewWorktree => tree::Row::leaf(1),
            RowKind::Tab => tree::Row::leaf(2),
        }
    }

    /// Whether the worktree row `worktree_id` has tab rows directly under it
    /// in the full (unfiltered, uncollapsed) row list.
    fn worktree_has_tab_rows(rows: &[SidebarRow], worktree_id: usize) -> bool {
        rows.iter()
            .position(|row| row.id == worktree_id && row.kind == RowKind::Worktree)
            .is_some_and(|index| {
                rows.get(index + 1)
                    .is_some_and(|next| next.kind == RowKind::Tab)
            })
    }

    /// `has_children` for [`Self::tree_row`], resolved against the full row
    /// list rather than the visible one: a collapsed worktree's tab rows are
    /// exactly the ones `visible_rows` leaves out.
    fn row_has_children(&self, row: &SidebarRow) -> bool {
        match row.kind {
            RowKind::Project => true,
            RowKind::Worktree => Self::worktree_has_tab_rows(&self.rows, row.id),
            RowKind::Tab | RowKind::NewWorktree => false,
        }
    }

    /// Apply one of bezel's standard tree directions to the currently
    /// visible, depth-annotated rows. Expansion remains Sirio state; bezel
    /// reports only the intent.
    fn tree_step(&mut self, direction: tree::Direction, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            self.tree_cursor = 0;
            return;
        }

        let cursor = self.tree_cursor.min(rows.len() - 1);
        let shape = rows
            .iter()
            .map(|row| Self::tree_row(row, self.row_has_children(row)))
            .collect::<Vec<_>>();
        match tree::step(&shape, cursor, direction) {
            Some(tree::Move::To(index)) => self.tree_cursor = index,
            Some(tree::Move::Expand(index)) => {
                self.set_row_expanded(rows[index].id, rows[index].kind, true);
                self.tree_cursor = index;
            }
            Some(tree::Move::Collapse(index)) => {
                self.set_row_expanded(rows[index].id, rows[index].kind, false);
                self.tree_cursor = index;
            }
            None => {
                self.tree_cursor = cursor;
            }
        }
        cx.notify();
    }

    fn focus_tree_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tree_cursor = index;
        self.tree_focus.focus(window, cx);
        cx.notify();
    }

    fn select_row(&mut self, id: usize, cx: &mut Context<Self>) {
        for row in &mut self.rows {
            row.selected = row.id == id;
        }
        cx.notify();
    }

    /// Host-driven selection: the shell answers a [`SidebarEvent::SelectWorktree`]
    /// by calling this, so the highlight always agrees with what the shell
    /// actually treats as the current worktree.
    pub fn set_selected_worktree(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let mut changed = false;
        for row in &mut self.rows {
            let selected = row.kind == RowKind::Worktree && row.path.as_deref() == Some(path);
            if row.selected != selected {
                row.selected = selected;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Sets everything a worktree row draws about its live agents. The row
    /// carries three facts and draws them as three distinct things, in the
    /// Swift original's own division of labour:
    ///
    /// * **this is a worktree** — the branch glyph, always drawn beside the
    ///   branch name (`App/SidebarView.swift:361`). Not settable here: it
    ///   is a property of the row, not of the agents in it.
    /// * **what it is doing, and whose** — one leading indicator carrying
    ///   both: `status` picks the shape (a running indicator, a lifecycle
    ///   dot, or nothing), `agent_brand` tints it. That is precisely what
    ///   `WorktreeStatusGlyph(status:agentId:)` does; `agentId` reaches
    ///   nothing but `RunningDots(color:)` there, and reaches nothing but
    ///   the tint here.
    /// * **which agents are running** — the trailing badge
    ///   (`running_agents`, from
    ///   `AgentActivityModel::running_agent_ids`), the only place a brand
    ///   mark appears on a worktree row.
    ///
    /// All of it is host-resolved. This crate deliberately does not depend
    /// on `sirio_activity`: status resolution, urgency ranking and catalog
    /// order live there and are applied by the host, so the sidebar can
    /// never grow a second, disagreeing copy of those rules.
    ///
    /// Called every render (`SirioWorkspace::sync_activity`), so it diffs
    /// before touching a row.
    pub fn set_worktree_activity(
        &mut self,
        id: usize,
        status: Option<ActivityStatus>,
        agent_brand: Option<AgentBrandColor>,
        running_agents: Vec<AgentMark>,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.kind == RowKind::Worktree)
        {
            if row.agent_status == status
                && row.agent_brand == agent_brand
                && row.running_agents == running_agents
            {
                return;
            }
            row.agent_status = status;
            row.agent_brand = agent_brand;
            row.running_agents = running_agents;
            cx.notify();
        }
    }

    /// F-CORE-ACT-22: applies a host-computed display order to the worktree
    /// rows under `project_row_id`. The host produces `order` (worktree row
    /// ids) by running the project's current row order through
    /// `sirio_activity::AttentionSort::urgent_first`, so a worktree whose
    /// agent is in `error`/`needs-input` floats above its siblings while
    /// every other row keeps exactly the position the user dragged it to.
    ///
    /// A worktree row moves together with the tab rows underneath it. Row
    /// ids are untouched — they are the host's catalog identity, not a
    /// position — so a later drag still resolves to the right worktree.
    /// Returns whether anything actually moved; a no-op when the order is
    /// already correct, which is what makes this safe to call every frame.
    pub fn set_worktree_order(
        &mut self,
        project_row_id: usize,
        order: &[usize],
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project_index) = self
            .rows
            .iter()
            .position(|row| row.id == project_row_id && row.kind == RowKind::Project)
        else {
            return false;
        };
        let section_end = self.rows[project_index + 1..]
            .iter()
            .position(|row| row.kind == RowKind::Project)
            .map_or(self.rows.len(), |offset| project_index + 1 + offset);

        // Split the project's section into [worktree row + its tab rows]
        // blocks, keeping whatever trailing rows (the New Worktree
        // affordance) follow the last block exactly where they are.
        let mut blocks: Vec<(usize, Vec<SidebarRow>)> = Vec::new();
        let mut trailing: Vec<SidebarRow> = Vec::new();
        for row in &self.rows[project_index + 1..section_end] {
            match row.kind {
                RowKind::Worktree => blocks.push((row.id, vec![row.clone()])),
                RowKind::Tab if !blocks.is_empty() => {
                    blocks
                        .last_mut()
                        .expect("checked non-empty")
                        .1
                        .push(row.clone());
                }
                _ => trailing.push(row.clone()),
            }
        }
        if blocks.len() < 2 {
            return false;
        }
        let current: Vec<usize> = blocks.iter().map(|(id, _)| *id).collect();
        // Only a permutation of exactly this project's worktree rows is a
        // legal order; anything else is a stale snapshot and is ignored.
        let mut wanted = order.to_vec();
        let mut sorted_current = current.clone();
        wanted.sort_unstable();
        sorted_current.sort_unstable();
        if wanted != sorted_current || current.as_slice() == order {
            return false;
        }

        let mut reordered: Vec<SidebarRow> = Vec::new();
        for id in order {
            if let Some(position) = blocks.iter().position(|(block_id, _)| block_id == id) {
                reordered.extend(blocks.remove(position).1);
            }
        }
        reordered.extend(trailing);
        self.rows.splice(project_index + 1..section_end, reordered);
        cx.notify();
        true
    }

    /// Replaces the tab rows under a worktree with the host's real, current
    /// tab list — the same list the tab bar and the Activity panel already
    /// render. Called every render (see `SirioWorkspace::sync_activity`),
    /// so this must diff before touching `self.rows` or it would `notify()`
    /// every frame forever.
    ///
    /// A row built here carries `tab_id: Some(tab.id)` and, unlike a fixture
    /// or catalog row, is never locally toggled by a click: `selected`
    /// always comes from the host's `tabs` argument, because the host's
    /// `active_tab` is the one fact "which tab is selected" is allowed to
    /// have.
    pub fn set_worktree_tabs(
        &mut self,
        worktree_id: usize,
        tabs: Vec<SidebarTab>,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree_index) = self
            .rows
            .iter()
            .position(|row| row.id == worktree_id && row.kind == RowKind::Worktree)
        else {
            return;
        };
        let insert_at = worktree_index + 1;
        // Host-sourced rows, live or parked, are the ones this call owns;
        // the decorative fixture tab rows carry neither and are left alone.
        let existing_end = insert_at
            + self.rows[insert_at..]
                .iter()
                .take_while(|row| {
                    row.kind == RowKind::Tab && (row.tab_id.is_some() || row.parked_tab.is_some())
                })
                .count();

        // The diff includes the agent mark, and must: it is the only field
        // here that changes without the tab list itself changing. A pane
        // identified after spawn keeps its id, kind, title and selection and
        // only grows a brand — comparing everything but the mark would make
        // this an unconditional early return for exactly the case the mark
        // exists to show.
        let unchanged = self.rows[insert_at..existing_end]
            .iter()
            .map(|row| {
                (
                    row.tab_id,
                    row.parked_tab,
                    row.tab_kind,
                    row.agent_icon,
                    row.agent_brand,
                    row.title.as_str(),
                    row.selected,
                )
            })
            .eq(tabs.iter().map(|tab| {
                let (tab_id, parked_tab) = match tab.tab {
                    SidebarTabRef::Open(id) => (Some(id), None),
                    SidebarTabRef::Parked(index) => (None, Some(index)),
                };
                (
                    tab_id,
                    parked_tab,
                    Some(tab.kind),
                    tab.agent.map(|agent| agent.icon),
                    tab.agent.map(|agent| agent.brand),
                    tab.title.as_str(),
                    tab.selected,
                )
            }));
        if unchanged {
            return;
        }

        let depth = self.rows[worktree_index].depth + 1;
        let worktree_path = self.rows[worktree_index].path.clone();
        let new_rows = tabs.into_iter().map(|tab| {
            let (id, tab_id, parked_tab, path) = match tab.tab {
                SidebarTabRef::Open(tab_id) => {
                    (TAB_ROW_ID_OFFSET + tab_id, Some(tab_id), None, None)
                }
                SidebarTabRef::Parked(index) => (
                    parked_tab_row_id(worktree_id, index),
                    None,
                    Some(index),
                    worktree_path.clone(),
                ),
            };
            SidebarRow {
                id,
                kind: RowKind::Tab,
                depth,
                title: tab.title,
                selected: tab.selected,
                expanded: false,
                is_primary: false,
                agent_status: None,
                is_git: false,
                path,
                tab_id,
                parked_tab,
                tab_kind: Some(tab.kind),
                agent_icon: tab.agent.map(|agent| agent.icon),
                agent_brand: tab.agent.map(|agent| agent.brand),
                comment: None,
                running_agents: Vec::new(),
            }
        });
        self.rows.splice(insert_at..existing_end, new_rows);
        cx.notify();
    }

    fn toggle_project(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(project) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.kind == RowKind::Project)
        {
            project.expanded = !project.expanded;
        }
        self.select_row(id, cx);
    }

    /// Opens or closes the disclosure of the worktree row `id`. Unlike
    /// [`Self::toggle_project`] this never touches the selection: the chevron
    /// is its own control, and which worktree is selected stays the host's
    /// decision (`SidebarEvent::SelectWorktree`).
    fn toggle_worktree(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(expanded) = self
            .rows
            .iter()
            .find(|row| row.id == id && row.kind == RowKind::Worktree)
            .map(|row| row.expanded)
        else {
            return;
        };
        self.set_row_expanded(id, RowKind::Worktree, !expanded);
        cx.notify();
    }

    /// The one place a row's `expanded` flag is written. For a worktree the
    /// same fact is mirrored into `collapsed_worktrees`, keyed by checkout
    /// path, so it outlives the row: `set_projects` rebuilds every row from
    /// the catalog and re-applies the set, and a selection change never
    /// consults it at all — a worktree the user closed stays closed, one
    /// they left open stays open, whichever worktree is current.
    fn set_row_expanded(&mut self, id: usize, kind: RowKind, expanded: bool) {
        if !matches!(kind, RowKind::Project | RowKind::Worktree) {
            return;
        }
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.kind == kind)
        else {
            return;
        };
        row.expanded = expanded;
        if kind == RowKind::Worktree
            && let Some(path) = row.path.clone()
        {
            if expanded {
                self.collapsed_worktrees.remove(&path);
            } else {
                self.collapsed_worktrees.insert(path);
            }
        }
    }

    /// Re-applies `collapsed_worktrees` to freshly built worktree rows.
    fn apply_collapsed_worktrees(&mut self) {
        for row in &mut self.rows {
            if row.kind == RowKind::Worktree {
                row.expanded = !row
                    .path
                    .as_ref()
                    .is_some_and(|path| self.collapsed_worktrees.contains(path));
            }
        }
    }

    // ------------------------------------------------------------------
    // Worktree creation and removal
    // ------------------------------------------------------------------

    /// The index of the project row enclosing `row_id`, if any.
    fn enclosing_project_index(&self, row_id: usize) -> Option<usize> {
        let row_index = self.rows.iter().position(|row| row.id == row_id)?;
        self.rows[..=row_index]
            .iter()
            .rposition(|row| row.kind == RowKind::Project)
    }

    /// The repository root of the project enclosing `row_id`, if it has one.
    fn project_root(&self, row_id: usize) -> Option<PathBuf> {
        let project_index = self.enclosing_project_index(row_id)?;
        let project = &self.rows[project_index];
        if project.is_git {
            project.path.clone()
        } else {
            None
        }
    }

    /// Opens the branch-name prompt for the project whose New Worktree row
    /// was clicked.
    fn begin_worktree_prompt(
        &mut self,
        row_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_index) = self.enclosing_project_index(row_id) else {
            return;
        };
        let project = &self.rows[project_index];
        let Some(repo_root) = project.path.clone() else {
            // A project with no repository path (like a non-git project) is
            // never offered the row in the first place.
            return;
        };
        let project_row_id = project.id;
        let project_name = project.title.clone();
        // F-CORE-DOM-02: snapshot the project's pinned base/location so a
        // blank dialog field falls back to them instead of straight to
        // HEAD/the sibling directory — `project_worktree_defaults` is keyed
        // by the durable project id, not this row's own numeric id.
        let (default_base, default_location_override) = self
            .project_ids
            .get(&project_row_id)
            .and_then(|id| self.project_worktree_defaults.get(id))
            .cloned()
            .unwrap_or_default();
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        self.prompt = Some(WorktreePrompt {
            project_row_id,
            project_name,
            repo_root,
            draft: String::new(),
            base_draft: String::new(),
            location_draft: String::new(),
            default_base,
            default_location_override,
            focused_field: WorktreePromptField::Branch,
            error: None,
            focus,
        });
        cx.notify();
    }

    /// Cancels the open prompt, if any.
    fn cancel_worktree_prompt(&mut self, cx: &mut Context<Self>) {
        if self.prompt.take().is_some() {
            cx.notify();
        }
    }

    /// Confirms the open prompt: creates the worktree on the background
    /// executor and inserts its row on success.
    fn confirm_worktree_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let branch = prompt.draft.trim().to_string();
        if branch.is_empty() {
            // Re-open with the reason; the draft is kept for editing.
            self.prompt = Some(WorktreePrompt {
                error: Some("enter a branch name".to_string()),
                ..prompt
            });
            cx.notify();
            return;
        }

        let repo_root = prompt.repo_root.clone();
        let project_name = prompt.project_name.clone();
        let project_row_id = prompt.project_row_id;
        let project_id = self.project_ids.get(&project_row_id).cloned();
        // F-PRJ-17/F-CORE-DOM-02: an explicit base branch, when typed, wins
        // outright (threaded through to `create_worktree`'s `base`
        // argument); left blank, it falls back to the project's *pinned*
        // default (`WorktreeDefaults.resolveBase` in the Swift app) rather
        // than straight to `None` — git's own HEAD fallback only takes over
        // once neither is set.
        let base = {
            let trimmed = prompt.base_draft.trim();
            if !trimmed.is_empty() {
                Some(trimmed.to_string())
            } else {
                prompt.default_base.clone()
            }
        };
        // F-PRJ-18/F-CORE-DOM-02: same layering for the checkout location —
        // an explicit override typed here wins; blank falls back to the
        // project's pinned override (`WorktreeDefaults.resolveParentDirectory`)
        // before `resolve_parent_directory` reaches for the sibling
        // directory default.
        let location_override = {
            let trimmed = prompt.location_draft.trim();
            if !trimmed.is_empty() {
                Some(PathBuf::from(trimmed))
            } else {
                prompt.default_location_override.clone().map(PathBuf::from)
            }
        };
        // The worktree lives next to the project (or at the override
        // above), named `{project}-{branch}` — mirroring the Swift app.
        let path = derive_worktree_path(
            &resolve_parent_directory(&repo_root, location_override.as_deref()),
            &project_name,
            &branch,
        );

        let repo_root_for_task = repo_root.clone();
        let branch_for_task = branch.clone();
        let path_for_task = path.clone();
        let base_for_task = base.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    create_worktree(
                        &repo_root_for_task,
                        &branch_for_task,
                        &path_for_task,
                        base_for_task.as_deref(),
                    )
                })
                .await;
            this.update(cx, |sidebar, cx| match result {
                Ok(()) => {
                    sidebar.insert_worktree_row(project_row_id, &branch, path.clone(), cx);
                    if let Some(project_id) = project_id {
                        cx.emit(SidebarEvent::WorktreeCreated { project_id, path });
                    }
                }
                Err(error) => {
                    sidebar.notice = Some(error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Inserts a worktree row for `branch` under the project, before its
    /// New Worktree row, and selects it.
    fn insert_worktree_row(
        &mut self,
        project_row_id: usize,
        branch: &str,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let project_index = self
            .rows
            .iter()
            .position(|row| row.id == project_row_id)
            .expect("project row still present");
        let insert_at = self.rows[project_index + 1..]
            .iter()
            .position(|row| row.kind == RowKind::NewWorktree)
            .map_or(self.rows.len(), |offset| project_index + 1 + offset);
        // Excludes tab rows: their ids live at `TAB_ROW_ID_OFFSET` and up
        // (see that constant's doc comment), a separate namespace from
        // hand- and index-assigned rows. Taking the max across both would
        // hand a brand-new worktree row an id inside the tab-row range as
        // soon as any tab was open.
        let id = self
            .rows
            .iter()
            .filter(|row| row.tab_id.is_none())
            .map(|row| row.id)
            .max()
            .unwrap_or(0)
            + 1;

        self.rows.insert(
            insert_at,
            SidebarRow {
                id,
                kind: RowKind::Worktree,
                depth: 1,
                title: branch.to_string(),
                selected: false,
                expanded: false,
                is_primary: false,
                agent_status: None,
                is_git: true,
                path: Some(path),
                tab_id: None,
                parked_tab: None,
                tab_kind: None,
                agent_icon: None,
                agent_brand: None,
                comment: None,
                running_agents: Vec::new(),
            },
        );
        self.select_row(id, cx);
        self.notice = None;
    }

    /// #372: the confirm dialog names its target — branch and checkout
    /// path — like the Changes panel's "Discard changes?" names its file,
    /// so a reorder between right-click and confirm cannot silently retarget
    /// a destructive, irreversible deletion.
    fn remove_worktree_prompt(branch: &str, path: &Path) -> (String, String) {
        (
            format!("Remove worktree `{branch}`?"),
            format!(
                "This permanently deletes the worktree at `{}` and its branch \
                 `{branch}` on disk. This cannot be undone.",
                path.display()
            ),
        )
    }

    /// F-SID-15: confirm-gated entry point for worktree removal. Both the
    /// context menu's "Remove Worktree" and the row's hover-x button route
    /// through this instead of calling `remove_worktree_row` (a real
    /// on-disk deletion, spawned immediately) with no safety confirmation.
    fn request_remove_worktree_row(
        &mut self,
        row_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((branch, worktree_path, is_primary)) = self
            .rows
            .iter()
            .find(|row| row.id == row_id && row.kind == RowKind::Worktree)
            .map(|row| {
                (
                    row.title.clone(),
                    row.path.clone().unwrap_or_default(),
                    row.is_primary,
                )
            })
        else {
            return;
        };
        // #372: the primary checkout is not removable — the menu item and
        // the hover-x button already hide it, so reaching here means a
        // stale row id; do nothing rather than prompt for the repository
        // itself.
        if is_primary {
            return;
        }
        let (title, detail) = Self::remove_worktree_prompt(&branch, &worktree_path);
        let receiver = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(&detail),
            &["Remove Worktree", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |sidebar, cx| {
            if receiver.await.unwrap_or(1) == 0 {
                let _ = sidebar.update(cx, |sidebar, cx| sidebar.remove_worktree_row(row_id, cx));
            }
        })
        .detach();
    }

    /// Removes a worktree on the background executor and drops its rows.
    fn remove_worktree_row(&mut self, row_id: usize, cx: &mut Context<Self>) {
        let Some(repo_root) = self.project_root(row_id) else {
            return;
        };
        let Some((worktree_path, branch, is_primary)) = self
            .rows
            .iter()
            .find(|row| row.id == row_id)
            .and_then(|row| {
                row.path
                    .clone()
                    .map(|path| (path, row.title.clone(), row.is_primary))
            })
        else {
            return;
        };
        // #372: defensive — the primary checkout must never reach
        // `git worktree remove`; see `request_remove_worktree_row`.
        if is_primary {
            return;
        }
        let project_id = self
            .enclosing_project_index(row_id)
            .and_then(|index| self.project_ids.get(&self.rows[index].id).cloned());

        let repo_root_for_task = repo_root.clone();
        let worktree_path_for_task = worktree_path.clone();
        let branch_for_task = branch.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    remove_worktree(
                        &repo_root_for_task,
                        &worktree_path_for_task,
                        &branch_for_task,
                    )
                })
                .await;
            this.update(cx, |sidebar, cx| match result {
                Ok(()) => {
                    sidebar.drop_worktree_rows(row_id);
                    if let Some(project_id) = project_id {
                        cx.emit(SidebarEvent::WorktreeRemoved {
                            project_id,
                            path: worktree_path,
                        });
                    }
                    cx.notify();
                }
                Err(error) => {
                    // git refused (typically uncommitted changes); surface
                    // its reason instead of forcing through.
                    sidebar.notice = Some(error.to_string());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Drops a worktree row and its tab rows from the tree.
    fn drop_worktree_rows(&mut self, row_id: usize) {
        let Some(index) = self.rows.iter().position(|row| row.id == row_id) else {
            return;
        };
        let end = self.rows[index + 1..]
            .iter()
            .position(|row| row.kind != RowKind::Tab)
            .map_or(self.rows.len(), |offset| index + 1 + offset);
        self.rows.drain(index..end);
        self.notice = None;
    }

    // ------------------------------------------------------------------
    // The branch-name prompt
    // ------------------------------------------------------------------

    fn on_prompt_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.field_blink.wake();
        match event.keystroke.key.as_str() {
            "enter" | "return" => self.confirm_worktree_prompt(cx),
            "escape" => self.cancel_worktree_prompt(cx),
            "tab" => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.focused_field = match prompt.focused_field {
                        WorktreePromptField::Branch => WorktreePromptField::Base,
                        WorktreePromptField::Base => WorktreePromptField::Location,
                        WorktreePromptField::Location => WorktreePromptField::Branch,
                    };
                }
                cx.notify();
            }
            "backspace" | "delete" => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.focused_draft_mut().pop();
                }
                cx.notify();
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && character != "\n"
                {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.focused_draft_mut().push_str(character);
                    }
                    cx.notify();
                }
            }
        }
    }

    fn visible_rows(&self) -> Vec<SidebarRow> {
        let query = self.filter.trim().to_lowercase();
        if query.is_empty() {
            return self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    if row.kind == RowKind::Project {
                        Some((index, row))
                    } else {
                        None
                    }
                })
                .flat_map(|(project_index, project)| {
                    let next_project = self.rows[project_index + 1..]
                        .iter()
                        .position(|row| row.kind == RowKind::Project)
                        .map_or(self.rows.len(), |offset| project_index + 1 + offset);
                    let children = &self.rows[project_index + 1..next_project];
                    let mut project_row = project.clone();
                    if !project.expanded {
                        project_row.agent_status = Self::collapsed_project_status(children);
                    }
                    let mut section = vec![project_row];
                    if project.expanded {
                        // A collapsed worktree hides the tab rows under it,
                        // the way a collapsed project hides its section.
                        let mut worktree_expanded = true;
                        section.extend(
                            children
                                .iter()
                                .filter(|row| match row.kind {
                                    RowKind::Worktree => {
                                        worktree_expanded = row.expanded;
                                        true
                                    }
                                    RowKind::Tab => worktree_expanded,
                                    // The New Worktree row is only offered
                                    // for git projects with a repository
                                    // path.
                                    RowKind::NewWorktree => {
                                        project.is_git && project.path.is_some()
                                    }
                                    RowKind::Project => true,
                                })
                                .cloned(),
                        );
                    }
                    section
                })
                .collect();
        }

        let mut filtered = Vec::new();
        let mut project_index = 0;
        while project_index < self.rows.len() {
            let project = &self.rows[project_index];
            if project.kind != RowKind::Project {
                project_index += 1;
                continue;
            }
            let next_project = self.rows[project_index + 1..]
                .iter()
                .position(|row| row.kind == RowKind::Project)
                .map_or(self.rows.len(), |offset| project_index + 1 + offset);
            let section = &self.rows[project_index + 1..next_project];
            let project_matches = project.title.to_lowercase().contains(&query);
            let section_matches = section
                .iter()
                .any(|row| row.title.to_lowercase().contains(&query));

            if project_matches || section_matches {
                let mut project_row = project.clone();
                if !project.expanded {
                    project_row.agent_status = Self::collapsed_project_status(section);
                }
                filtered.push(project_row);
                if project.expanded || section_matches {
                    let mut worktree: Option<SidebarRow> = None;
                    let mut tabs = Vec::new();
                    let append_worktree =
                        |filtered: &mut Vec<SidebarRow>,
                         worktree: &mut Option<SidebarRow>,
                         tabs: &mut Vec<SidebarRow>| {
                            let Some(worktree_row) = worktree.take() else {
                                return;
                            };
                            let worktree_matches =
                                worktree_row.title.to_lowercase().contains(&query);
                            let tab_matches = tabs
                                .iter()
                                .any(|tab: &SidebarRow| tab.title.to_lowercase().contains(&query));
                            if worktree_matches || tab_matches {
                                // Same rule as a collapsed project: its own
                                // match reveals the row, not the rows it
                                // hides — those need a match of their own.
                                let worktree_expanded = worktree_row.expanded;
                                filtered.push(worktree_row);
                                if worktree_matches && worktree_expanded {
                                    filtered.append(tabs);
                                } else {
                                    filtered.extend(
                                        tabs.drain(..).filter(|tab| {
                                            tab.title.to_lowercase().contains(&query)
                                        }),
                                    );
                                }
                            } else {
                                tabs.clear();
                            }
                        };
                    for row in section {
                        match row.kind {
                            RowKind::Worktree => {
                                append_worktree(&mut filtered, &mut worktree, &mut tabs);
                                worktree = Some(row.clone());
                            }
                            RowKind::Tab => tabs.push(row.clone()),
                            RowKind::NewWorktree => {
                                append_worktree(&mut filtered, &mut worktree, &mut tabs);
                                if project_matches
                                    && project.expanded
                                    && project.is_git
                                    && project.path.is_some()
                                {
                                    filtered.push(row.clone());
                                }
                            }
                            RowKind::Project => {}
                        }
                    }
                    append_worktree(&mut filtered, &mut worktree, &mut tabs);
                }
            }
            project_index = next_project;
        }
        filtered
    }

    /// F-SID-06: a collapsed project hides its worktree rows, so the status
    /// dot they'd otherwise show has nowhere to draw. This picks the single
    /// most urgent status among a project's worktree children so the
    /// collapsed project row can badge it instead — the same "a glyph only
    /// appears for a notable status" rule `render_row` already applies to
    /// an expanded worktree row, just aggregated up one level.
    ///
    /// Only statuses that would actually draw something are candidates:
    /// `Idle` draws nothing (`RowStatusGlyph::for_status`), so aggregating it
    /// would let an idle worktree hide a finished sibling behind a row with
    /// no glyph on it at all.
    fn collapsed_project_status(children: &[SidebarRow]) -> Option<ActivityStatus> {
        fn urgency(status: ActivityStatus) -> u8 {
            match status {
                ActivityStatus::Idle => 0,
                ActivityStatus::Done => 1,
                ActivityStatus::Running => 2,
                ActivityStatus::NeedsInput => 3,
                ActivityStatus::Error => 4,
            }
        }
        children
            .iter()
            .filter(|row| row.kind == RowKind::Worktree)
            .filter_map(|row| row.agent_status)
            .filter(|status| urgency(*status) > 0)
            .max_by_key(|status| urgency(*status))
    }

    /// Stable semantic debug/test names, independent of vendored filenames.
    fn icon_selector_name(icon: Icon) -> &'static str {
        match icon {
            Icon::FolderFill => "folder",
            Icon::GitBranch => "git-branch",
            Icon::MessageSquare => "chat-round-line",
            Icon::SquareTerminal => "terminal",
            Icon::Close => "close",
            Icon::ChevronDown => "alt-arrow-down",
            Icon::ChevronUp => "alt-arrow-up",
            Icon::ChevronRight => "alt-arrow-right",
            Icon::ChevronLeft => "alt-arrow-left",
            Icon::Settings => "settings-minimalistic",
            Icon::RefreshCw => "refresh",
            Icon::Plus => "plus",
            Icon::File => "document",
            Icon::Sparkles => "sparkle-thin",
            Icon::Shield => "shield-thin",
            Icon::SunMoon => "sun-dim-thin",
            Icon::Globe => "global",
            Icon::ClaudeCode => "claude-mark",
            Icon::Codex => "openai-mark",
            Icon::OpenCode => "agent-opencode",
            Icon::Pi => "pi-mark",
            Icon::OhMyPi => "agent-omp",
            Icon::SidebarLeft => "sidebar-minimalistic-left",
            Icon::PanelRight => "sidebar-minimalistic",
            Icon::Archive => "archive-minimalistic",
            Icon::Lock => "key-minimalistic",
            Icon::FileTree => "file-tree",
            Icon::Thread => "thread",
            Icon::Diff => "diff",
            Icon::DiffUnified => "diff-unified",
            Icon::DiffSplit => "diff-split",
            Icon::ExpandVertical => "expand-vertical",
            Icon::FoldVertical => "fold-vertical",
            Icon::SquarePlus => "square-plus",
            Icon::SquareMinus => "square-minus",
            Icon::Undo => "undo",
            Icon::GitGraph => "git-graph",
            Icon::FileType(_) => "file-type",
        }
    }

    /// The tint of a tab row's icon.
    ///
    /// A branded agent mark is drawn in its brand, exactly as the reference
    /// draws one `AgentIcon` wherever a tab is listed. A tab with no agent —
    /// an unstarted chat, a plain terminal — used to fall back to
    /// `tab_needs_input`, spending the "answer me" amber as a decorative
    /// tint, so an idle terminal wore the colour of an agent genuinely
    /// waiting on the reader. It takes `meta` instead: the same grey the
    /// worktree rows it sits under already use.
    ///
    /// Lifted out of the row body because a colour chosen inline is a colour
    /// no test can reach — which is exactly how the amber survived the first
    /// pass at this collision.
    fn tab_row_icon_color(
        agent_brand: Option<AgentBrandColor>,
        has_agent_icon: bool,
        theme: Theme,
    ) -> Rgba {
        agent_brand
            .filter(|_| has_agent_icon)
            .map_or(theme.text_faint, AgentBrandColor::color)
    }

    fn row_icon(row: &SidebarRow) -> Icon {
        match row.kind {
            RowKind::Project => Icon::FolderFill,
            // A worktree row's mark says what the row *is*, not what is
            // running in it: `App/SidebarView.swift:361` draws
            // `arrow.triangle.branch` beside the branch name unconditionally
            // and never puts an agent mark there. The agent reaches this row
            // as the tint of the status indicator and as the trailing badge
            // — see `set_worktree_activity`.
            RowKind::Worktree => Icon::GitBranch,
            RowKind::Tab => row.agent_icon.unwrap_or(match row.tab_kind {
                Some(TabKind::Terminal) => Icon::SquareTerminal,
                Some(TabKind::Editor | TabKind::Diff) => Icon::File,
                _ => Icon::MessageSquare,
            }),
            RowKind::NewWorktree => Icon::Plus,
        }
    }

    fn context_action_selector(action: SidebarContextAction) -> &'static str {
        match action {
            SidebarContextAction::ProjectSettings => "project-settings",
            SidebarContextAction::RefreshProject => "refresh-project",
            SidebarContextAction::InitializeGit => "initialize-git",
            SidebarContextAction::RevealInFileManager => "reveal-in-file-manager",
            SidebarContextAction::RemoveProject => "remove-project",
            SidebarContextAction::SetPrimary => "set-primary",
            SidebarContextAction::UnsetPrimary => "unset-primary",
            SidebarContextAction::RemoveWorktree => "remove-worktree-context",
            SidebarContextAction::NewTab(NewTabAction::NewTerminal) => "new-terminal",
            SidebarContextAction::NewTab(NewTabAction::ClaudeCode) => "claude-code",
            SidebarContextAction::NewTab(NewTabAction::Codex) => "codex",
            SidebarContextAction::NewTab(NewTabAction::OpenCode) => "opencode",
            SidebarContextAction::NewTab(NewTabAction::Pi) => "pi",
            SidebarContextAction::NewTab(NewTabAction::OhMyPi) => "oh-my-pi",
            SidebarContextAction::NewTab(NewTabAction::NewChat) => "new-chat",
            SidebarContextAction::NewTab(NewTabAction::NewChanges)
            | SidebarContextAction::NewTab(NewTabAction::NewBrowser)
            | SidebarContextAction::NewTab(NewTabAction::SplitClaudeCode) => "unsupported",
        }
    }

    /// One text field in the New Worktree prompt: branch, base branch
    /// (F-PRJ-17), or checkout location (F-PRJ-18). The focused field draws
    /// the selection-ring border the single branch field used to own alone;
    /// unfocused fields fall back to a hairline. Both Tab (`on_prompt_key`)
    /// and a direct click switch which field keystrokes target — Tab alone
    /// is not trusted here because GPUI's own tab-stop focus traversal
    /// (every field's `focus` handle is `tab_stop(true)`, matching the
    /// convention `chat.rs`'s composer already flags as host-dependent) can
    /// consume the keypress before `on_prompt_key` ever sees it, stranding
    /// the prompt with no live field at all.
    fn render_worktree_prompt_field(
        id: &'static str,
        placeholder: &str,
        value: &str,
        field: WorktreePromptField,
        focused: bool,
        caret_shown: bool,
        entity: &gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let click_entity = entity.clone();
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .w_full()
            .h(px(26.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded(theme.radii.control)
            .bg(theme.input_bg)
            .border_1()
            .border_color(if focused { theme.ring } else { theme.border })
            .cursor(gpui::CursorStyle::IBeam)
            .text_size(theme.typography.footnote)
            .text_color(if value.is_empty() {
                theme.text_faint
            } else {
                theme.text
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_entity.update(cx, |sidebar, cx| {
                    if let Some(prompt) = sidebar.prompt.as_mut() {
                        prompt.focused_field = field;
                        prompt.focus.focus(window, cx);
                    }
                    cx.notify();
                });
            })
            // #208: the text is clipped rather than laying out at its
            // natural width and spilling past the field's own border. It must shrink *without* growing -- `flex_1` here
            // would stretch a short value to the full width and push the
            // end-of-text caret below to the far right, which is the one
            // thing this row's geometry means.
            //
            // These placeholders are not fixed strings: a pinned location
            // interpolates a filesystem path into
            // "optional -- defaults to the pinned location ({location})",
            // so the overflow is bounded only by how deep the path is.
            .overflow_hidden()
            .child(
                caret::field_value(if value.is_empty() {
                    placeholder.to_owned()
                } else {
                    value.to_owned()
                })
                .id("worktree-prompt-field-text")
                .debug_selector(move || format!("{id}-text")),
            )
            // End-of-text insertion caret; these compact single-line fields
            // always append. `caret_shown` already folds in the field being
            // focused and the blink phase.
            .when(focused, |this| {
                this.child(caret::bar(px(14.0), theme.text, caret_shown))
            })
    }

    fn render_context_menu(
        popup: &Popup<OpenContextMenu>,
        entity: gpui::Entity<Self>,
        theme: Theme,
        painter: Painter,
    ) -> impl IntoElement {
        let menu = popup.get().expect("mounted context menu").clone();
        let closing = popup.closing_since();
        let target = menu.target;
        let position = menu.position;
        let bezel_theme = theme.to_bezel_theme();
        let mut card = popover::popover_card(&bezel_theme)
            .id("sidebar-context-menu")
            .debug_selector(|| "sidebar-context-menu".to_owned())
            .w(px(240.0));

        for item in Self::context_menu_items(&target) {
            let selector = format!(
                "sidebar-context-item-{}",
                Self::context_action_selector(item.action)
            );
            let enabled = item.enabled;
            let action = item.action;
            let item_target = target.clone();
            let item_entity = entity.clone();
            let mut row = popover::menu_row(
                &bezel_theme,
                false,
                Fade::new(painter, selector.clone()),
            )
                .id(selector.clone())
                .debug_selector(move || selector.clone())
                .w_full()
                .min_h(px(29.0))
                .justify_between()
                .text_color(if enabled {
                    bezel_theme.text
                } else {
                    bezel_theme.text_faint
                })
                .when(!enabled, |this| {
                    this.cursor_default().bg(gpui::transparent_black())
                });
            if enabled {
                row = row.on_click(move |_, window, cx| {
                    item_entity.update(cx, |sidebar, cx| {
                        sidebar.dispatch_context_action(item_target.clone(), action, window, cx)
                    });
                });
            }
            row = row.child(item.label);
            if let Some(reason) = item.disabled_reason {
                row = row.child(
                    div()
                        .text_size(theme.typography.scaled(11.0))
                        .text_color(bezel_theme.text_faint)
                        .child(reason.to_string()),
                );
            }
            card = card.child(row);
        }
        // F-SID-15: `menu_at` owns the deferred priority-1 layer, so later
        // sidebar siblings cannot paint over the menu or intercept its rows.
        // Dismissal stays on the card because bezel intentionally leaves that
        // listener to the caller.
        let card = card.on_mouse_down_out(move |_, _, cx| {
            entity.update(cx, |sidebar, cx| sidebar.close_context_menu(cx));
        });
        popover::menu_at(
            "sidebar-context-menu-layer",
            position,
            card.into_any_element(),
            closing,
        )
    }

    fn render_add_project_menu(
        popup: &Popup<()>,
        entity: gpui::Entity<Self>,
        theme: Theme,
        painter: Painter,
    ) -> impl IntoElement {
        let open_entity = entity.clone();
        let clone_entity = entity.clone();
        let create_entity = entity.clone();
        let bezel_theme = theme.to_bezel_theme();
        let card = popover::popover_card(&bezel_theme)
            .id("add-project-menu")
            .debug_selector(|| "add-project-menu".to_owned())
            .w(px(190.0))
            .child(Self::render_add_project_item(
                entity.clone(),
                "Open Project…",
                "add-project-open",
                &bezel_theme,
                painter,
                move |_, window, cx| {
                    open_entity.update(cx, |sidebar, cx| sidebar.start_open_project(window, cx))
                },
            ))
            .child(Self::render_add_project_item(
                entity.clone(),
                "Clone Repository…",
                "add-project-clone",
                &bezel_theme,
                painter,
                move |_, _, cx| {
                    clone_entity.update(cx, |sidebar, cx| sidebar.start_clone_project(cx))
                },
            ))
            .child(Self::render_add_project_item(
                entity.clone(),
                "Create Project…",
                "add-project-create",
                &bezel_theme,
                painter,
                move |_, _, cx| {
                    create_entity.update(cx, |sidebar, cx| sidebar.start_create_project(cx))
                },
            ))
            .on_mouse_down_out(move |_, _, cx| {
                entity.update(cx, |sidebar, cx| sidebar.close_add_project_menu(cx));
            });
        popover::anchored_menu_below(
            "add-project-menu-layer",
            card.into_any_element(),
            popup.closing_since(),
        )
    }

    fn render_add_project_item(
        entity: gpui::Entity<Self>,
        label: &'static str,
        selector: &'static str,
        theme: &bezel::theme::Theme,
        painter: Painter,
        action: impl Fn(gpui::Entity<Self>, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        popover::menu_row(theme, false, Fade::new(painter, selector))
            .id(selector)
            .debug_selector(|| selector.to_owned())
            .w_full()
            .min_h(px(29.0))
            .text_color(theme.text)
            .on_click(move |_, window, cx| action(entity.clone(), window, cx))
            .child(label)
    }

    fn render_project_form(
        form: ProjectFormSurface,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let form_view = match form {
            ProjectFormSurface::Clone(form) => div().child(form).into_any_element(),
            ProjectFormSurface::Create(form) => div().child(form).into_any_element(),
        };
        let close_entity = entity.clone();
        let backdrop_close_entity = entity.clone();
        div()
            .id("project-form-overlay")
            .debug_selector(|| "project-form-overlay".to_owned())
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(0.0))
            .bottom(px(0.0))
            .p(px(12.0))
            .bg(rgb(0x000000).alpha(0.35))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("project-form-card")
                    .debug_selector(|| "project-form-card".to_owned())
                    .w(px(300.0))
                    .rounded(theme.radii.toast)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .on_mouse_down_out(move |_, _, cx| {
                        backdrop_close_entity.update(cx, |sidebar, cx| {
                            sidebar.close_project_surface(cx);
                        });
                    })
                    .child(form_view)
                    .child(
                        div()
                            .id("close-project-form")
                            .debug_selector(|| "close-project-form".to_owned())
                            .ml(px(16.0))
                            .mb(px(12.0))
                            .w(px(80.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.element_hover))
                            .on_click(move |_, _, cx| {
                                close_entity.update(cx, |sidebar, cx| {
                                    sidebar.project_form = None;
                                    cx.notify();
                                });
                            })
                            .child("Cancel"),
                    ),
            )
    }

    fn render_project_settings(
        card: ProjectSettingsCard,
        entity: gpui::Entity<Self>,
        theme: Theme,
        focused_fields: [bool; 3],
        caret_visible: bool,
    ) -> impl IntoElement {
        let [name_focused, base_focused, location_focused] = focused_fields;
        let display_name = card.display_name.borrow().clone();
        let heading_name = if display_name.trim().is_empty() {
            card.name.clone()
        } else {
            display_name.clone()
        };
        let close_entity = entity.clone();
        let backdrop_close_entity = entity.clone();
        let name_entity = entity.clone();
        let focus_entity = entity.clone();
        let display_name_focus = card.display_name_focus.clone();
        let initialize_entity = entity.clone();
        let remove_entity = entity.clone();
        let remove_project_id = card.id.clone();
        let project_target = SidebarContextTarget::Project {
            id: card.id.clone(),
            path: card.path.clone(),
            is_git: card.is_git,
        };
        div()
            .id("project-settings-sheet")
            .debug_selector(|| "project-settings-sheet".to_owned())
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(0.0))
            .bottom(px(0.0))
            .on_mouse_down_out(move |_, _, cx| {
                backdrop_close_entity.update(cx, |sidebar, cx| {
                    sidebar.close_project_surface(cx);
                });
            })
            // F-PRJ-13: without this, GPUI's hit test (`Frame::hit_test`)
            // walks every hitbox under the pointer back-to-front and only
            // stops at one with `HitboxBehavior::BlockMouse` -- absent that,
            // a click here ALSO reaches whatever `sidebar-tree` row this
            // opaque full-sheet overlay happens to be painted over, firing
            // that row's own `on_click` (`SidebarEvent::SelectWorktree`) in
            // the same gesture. `occlude()` installs that blocking hitbox,
            // so every control in this sheet -- Reset included -- is
            // finally the only thing a click on it can reach.
            .occlude()
            .p(px(16.0))
            .bg(theme.surface)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(format!("Project Settings · {heading_name}")),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(display_path(&card.path)),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .child(if card.is_git {
                        "Repository: Git"
                    } else {
                        "Repository: Folder"
                    }),
            )
            .child(
                div()
                    .id("project-display-name-field")
                    .debug_selector(|| "project-display-name-field".to_owned())
                    .track_focus(&display_name_focus)
                    .w_full()
                    .h(px(32.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(theme.typography.footnote)
                    .text_color(if display_name.trim().is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_entity.update(cx, |sidebar, cx| {
                            if let Some(card) = &sidebar.project_settings {
                                card.display_name_focus.focus(window, cx);
                            }
                        });
                    })
                    .on_key_down(move |event, window, cx| {
                        name_entity.update(cx, |sidebar, cx| {
                            sidebar.on_display_name_key(event, window, cx);
                        });
                    })
                    // #212: clip inside the field; must not grow, or the caret leaves the text.
                    .overflow_hidden()
                    .child(
                        caret::field_value(if display_name.trim().is_empty() {
                            "Display name".to_owned()
                        } else {
                            display_name
                        })
                        .id("sidebar-display-name-text")
                        .debug_selector(|| "sidebar-display-name-text".to_owned()),
                    )
                    .when(name_focused, |this| {
                        this.child(caret::bar(px(14.0), theme.text, caret_visible))
                    }),
            )
            .when(!card.is_git, |this| {
                let target = project_target.clone();
                this.child(
                    div()
                        .id("project-settings-initialize-git")
                        .debug_selector(|| "project-settings-initialize-git".to_owned())
                        .h(px(30.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radii.control)
                        .bg(theme.surface_raised)
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text)
                        .on_click(move |_, _, cx| {
                            initialize_entity.update(cx, |_, cx| {
                                cx.emit(SidebarEvent::ContextAction {
                                    target: target.clone(),
                                    action: SidebarContextAction::InitializeGit,
                                });
                            });
                        })
                        .child("Initialize Git"),
                )
            })
            .child(
                div()
                    .id("project-icon-picker")
                    .debug_selector(|| "project-icon-picker".to_owned())
                    .w_full()
                    .p(px(8.0))
                    .rounded(theme.radii.control)
                    .bg(theme.surface)
                    .child(card.icon_picker.clone()),
            )
            .when(card.is_git, |this| {
                this.child(Self::render_worktree_base_section(
                    &card,
                    &entity,
                    &theme,
                    base_focused,
                    caret_visible,
                ))
                .child(Self::render_worktree_location_section(
                    &card,
                    &entity,
                    &theme,
                    location_focused,
                    caret_visible,
                ))
            })
            .child(
                div()
                    .id("project-settings-remove")
                    .debug_selector(|| "project-settings-remove".to_owned())
                    .cursor(gpui::CursorStyle::PointingHand)
                    .mt(px(4.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.diff_del)
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, window, cx| {
                        remove_entity.update(cx, |sidebar, cx| {
                            sidebar.request_remove_project(remove_project_id.clone(), window, cx);
                        });
                    })
                    .child(
                        IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.diff_del),
                    )
                    .child("Remove Project"),
            )
            .child(
                div()
                    .id("close-project-settings")
                    .debug_selector(|| "close-project-settings".to_owned())
                    .mt(px(4.0))
                    .w(px(80.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(theme.radii.control)
                    .text_color(theme.text)
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(move |_, _, cx| {
                        close_entity.update(cx, |sidebar, cx| {
                            sidebar.project_settings = None;
                            cx.notify();
                        });
                    })
                    .child("Close"),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.0))
                    .text_color(theme.text_faint)
                    .child(card.id),
            )
    }

    /// F-PRJ-17: "Default Worktree Base" — mirrors the Swift
    /// `WorktreeBaseSection`'s effective-value/subtitle pair. Typing a
    /// branch name pins it; "Use Primary" clears the pin.
    fn render_worktree_base_section(
        card: &ProjectSettingsCard,
        entity: &gpui::Entity<Self>,
        theme: &Theme,
        focused: bool,
        caret_visible: bool,
    ) -> impl IntoElement {
        let draft = card.default_worktree_base.borrow().clone();
        let effective_base = if !draft.trim().is_empty() {
            draft.clone()
        } else {
            card.primary_branch
                .clone()
                .unwrap_or_else(|| "—".to_string())
        };
        let subtitle = if !draft.trim().is_empty() {
            "Pinned".to_string()
        } else if let Some(branch) = &card.primary_branch {
            format!("Following primary branch ({branch})")
        } else {
            "No primary worktree set".to_string()
        };
        let base_focus = card.worktree_base_focus.clone();
        let focus_entity = entity.clone();
        let key_entity = entity.clone();
        let primary_entity = entity.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Default Worktree Base"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.text)
                                    .child(effective_base),
                            )
                            .child(
                                div()
                                    .text_size(theme.typography.scaled(11.0))
                                    .text_color(theme.text_faint)
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .id("project-worktree-base-use-primary")
                            .debug_selector(|| "project-worktree-base-use-primary".to_owned())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_faint)
                            .hover(|style| style.text_color(theme.text))
                            .on_click(move |_, _, cx| {
                                primary_entity.update(cx, |sidebar, cx| {
                                    sidebar.use_primary_worktree_base(cx);
                                });
                            })
                            .child("Use Primary"),
                    ),
            )
            .child(
                div()
                    .id("project-worktree-base-field")
                    .debug_selector(|| "project-worktree-base-field".to_owned())
                    .track_focus(&base_focus)
                    .w_full()
                    .h(px(28.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(theme.typography.footnote)
                    .text_color(if draft.trim().is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_entity.update(cx, |sidebar, cx| {
                            if let Some(card) = &sidebar.project_settings {
                                card.worktree_base_focus.focus(window, cx);
                            }
                        });
                    })
                    .on_key_down(move |event, window, cx| {
                        key_entity.update(cx, |sidebar, cx| {
                            sidebar.on_worktree_base_key(event, window, cx);
                        });
                    })
                    // #212: see the field above.
                    .overflow_hidden()
                    .child(
                        caret::field_value(if draft.trim().is_empty() {
                            "Search branches by name…".to_owned()
                        } else {
                            draft
                        })
                        .id("sidebar-branch-search-text")
                        .debug_selector(|| "sidebar-branch-search-text".to_owned()),
                    )
                    .when(focused, |this| {
                        this.child(caret::bar(px(14.0), theme.text, caret_visible))
                    }),
            )
    }

    /// F-PRJ-18: "Worktree Location" — mirrors the Swift
    /// `WorktreeLocationSection`. Typing a path or using "Choose…" sets an
    /// override; "Restore Default" clears it back to the project's sibling
    /// directory.
    fn render_worktree_location_section(
        card: &ProjectSettingsCard,
        entity: &gpui::Entity<Self>,
        theme: &Theme,
        focused: bool,
        caret_visible: bool,
    ) -> impl IntoElement {
        let draft = card.worktree_location_override.borrow().clone();
        let default_location = card
            .path
            .parent()
            .map(Self::worktree_location_text)
            .unwrap_or_else(|| Self::worktree_location_text(&card.path));
        let location_focus = card.worktree_location_focus.clone();
        let focus_entity = entity.clone();
        let key_entity = entity.clone();
        let choose_entity = entity.clone();
        let restore_entity = entity.clone();
        let has_override = !draft.trim().is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Worktree Location"),
            )
            .child(
                div()
                    .text_size(theme.typography.scaled(11.0))
                    .text_color(theme.text_faint)
                    .child(format!(
                        "Parent folder for new worktrees. Empty uses the default: {default_location}"
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("project-worktree-location-field")
                            .debug_selector(|| "project-worktree-location-field".to_owned())
                            .track_focus(&location_focus)
                            .flex_1()
                            .h(px(28.0))
                            .px(px(9.0))
                            .flex()
                            .items_center()
                            .rounded(theme.radii.control)
                            .bg(theme.input_bg)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.typography.footnote)
                            .text_color(if has_override { theme.text } else { theme.text_faint })
                            .cursor(gpui::CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus_entity.update(cx, |sidebar, cx| {
                                    if let Some(card) = &sidebar.project_settings {
                                        card.worktree_location_focus.focus(window, cx);
                                    }
                                });
                            })
                            .on_key_down(move |event, window, cx| {
                                key_entity.update(cx, |sidebar, cx| {
                                    sidebar.on_worktree_location_key(event, window, cx);
                                });
                            })
                            // #212: this one renders a filesystem path, so it is the likeliest to overflow.
                            .overflow_hidden()
                            .child(
                                caret::field_value(if has_override {
                                    draft
                                } else {
                                    default_location.clone()
                                })
                                .id("sidebar-location-override-text")
                                .debug_selector(|| "sidebar-location-override-text".to_owned()),
                            )
                            .when(focused, |this| {
                                this.child(caret::bar(px(14.0), theme.text, caret_visible))
                            }),
                    )
                    .child(
                        div()
                            .id("project-worktree-location-choose")
                            .debug_selector(|| "project-worktree-location-choose".to_owned())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .px(px(8.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme.radii.control)
                            .bg(theme.surface_raised)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text)
                            .on_click(move |_, window, cx| {
                                choose_entity.update(cx, |sidebar, cx| {
                                    sidebar.choose_worktree_location(window, cx);
                                });
                            })
                            .child("Choose…"),
                    ),
            )
            .when(has_override, |this| {
                this.child(
                    div()
                        .id("project-worktree-location-restore")
                        .debug_selector(|| "project-worktree-location-restore".to_owned())
                        .cursor(gpui::CursorStyle::PointingHand)
                        .text_size(theme.typography.scaled(11.0))
                        .text_color(theme.text_faint)
                        .hover(|style| style.text_color(theme.text))
                        .on_click(move |_, _, cx| {
                            restore_entity.update(cx, |sidebar, cx| {
                                sidebar.restore_default_worktree_location(cx);
                            });
                        })
                        .child("Restore Default"),
                )
            })
    }

    fn render_row(
        row: SidebarRow,
        row_index: usize,
        row_shape: tree::Row,
        cursor: bool,
        project_id: Option<String>,
        project_icon: Option<ProjectIcon>,
        drag: Option<RowDrag>,
        entity: gpui::Entity<Self>,
        theme: Theme,
        bezel_theme: &bezel::theme::Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let row_id = row.id;
        let selected = row.selected;
        let kind = row.kind;
        let title = row.title.clone();
        let path = row.path.clone();
        // The click closure reports the checkout path for worktree rows.
        let worktree_path = path.clone();
        let is_project = kind == RowKind::Project;
        let is_worktree = kind == RowKind::Worktree;
        // #372: the primary checkout offers no hover-x — like its disabled
        // context-menu item, it cannot be `git worktree remove`d.
        let is_removable_worktree = is_worktree && !row.is_primary;
        // waku's card rhythm: a project or worktree row becomes a two-line
        // card (13.5px title over an 11.5px context line) only when it has
        // something for that second line; every other row is single-line at
        // the 32px action-row height. See `has_sub_line`.
        let has_sub_line = Self::has_sub_line(&row);
        let row_min_height = Self::row_min_height(&row);
        // F-CORE-ACT-18: the trailing running-agents badge is one 12px mark
        // per distinct running agent, 3px apart, 7px clear of the title. It
        // takes its width out of the title's, so a busy worktree truncates
        // its branch name instead of pushing the hover controls off the row.
        let running_agents: Vec<AgentMark> = if kind == RowKind::Worktree {
            row.running_agents.clone()
        } else {
            Vec::new()
        };
        // A glyph only appears for a notable status — matching the
        // reference. A collapsed project also gets one (F-SID-06): its
        // worktree rows are hidden, so `row.agent_status` was pre-aggregated
        // onto the project row itself in `visible_rows`.
        let status_glyph =
            if kind == RowKind::Worktree || (kind == RowKind::Project && !row.expanded) {
                RowStatusGlyph::for_status(row.agent_status, row.agent_brand, theme)
            } else {
                RowStatusGlyph::None
            };
        let glyph = project_icon
            .as_ref()
            .and_then(|icon| match &icon.value {
                ProjectIconValue::Symbol(glyph) => Some(glyph.icon()),
                ProjectIconValue::Avatar(_) => Some(Icon::Globe),
                ProjectIconValue::Emoji(_) => None,
            })
            .unwrap_or_else(|| Self::row_icon(&row));
        let glyph_color = match kind {
            RowKind::Project => project_icon
                .as_ref()
                .map(|icon| icon.tint.resolve(theme))
                .unwrap_or_else(|| Self::project_color(&title)),
            RowKind::Tab => {
                Self::tab_row_icon_color(row.agent_brand, row.agent_icon.is_some(), theme)
            }
            RowKind::Worktree | RowKind::NewWorktree => theme.text_faint,
        };
        let entity = entity.clone();
        let remove_entity = entity.clone();
        let click_entity = entity.clone();
        let tab_close_entity = entity.clone();
        let context_entity = entity.clone();
        let hover_group = format!("sidebar-project-{row_id}");
        let tab_id = row.tab_id;
        let parked_tab = row.parked_tab;
        let mark_size = theme.typography.headline;
        let icon_size = IconSize::Small;
        let project_mark = match project_icon.as_ref().map(|icon| &icon.value) {
            Some(ProjectIconValue::Emoji(emoji)) => div()
                .text_size(px(15.0))
                .child(emoji.clone())
                .into_any_element(),
            // A locally chosen PNG is real file content already on disk — no
            // network fetch needed, so it can render as an actual image
            // instead of the generic globe glyph every other avatar source
            // still falls back to (F-PRJ-14: GitHub/Favicon need an HTTP
            // client this app doesn't have yet; see project_identity.rs).
            Some(ProjectIconValue::Avatar(AvatarSource::LocalPng(path))) => img(path.clone())
                .w(mark_size)
                .h(mark_size)
                .rounded(theme.radii.control)
                .into_any_element(),
            _ => IconElement::new(glyph, icon_size)
                .text_color(glyph_color)
                .into_any_element(),
        };

        let row_debug_selector = if kind == RowKind::NewWorktree {
            "new-worktree-row".to_string()
        } else {
            format!("sidebar-row-{row_id}")
        };
        let mut row_view = tree::tree_row(bezel_theme, &row_shape, selected, cursor)
            .text_size(theme.typography.scaled(12.5))
            .id(row_id)
            .debug_selector(move || row_debug_selector)
            .group(hover_group.clone())
            .relative()
            .min_h(px(row_min_height))
            .on_click(move |_, window, cx| {
                click_entity.update(cx, |sidebar, cx| {
                    sidebar.focus_tree_row(row_index, window, cx);
                    if let Some(tab_id) = tab_id {
                        // A host-driven row: the host owns which tab is
                        // selected, so report the click rather than
                        // flipping `selected` locally.
                        cx.emit(SidebarEvent::SelectTab(tab_id));
                        return;
                    }
                    if let Some(index) = parked_tab
                        && let Some(path) = worktree_path.as_ref()
                    {
                        // A parked tab has no live id: ask the host to bring
                        // its worktree back with this strip position active.
                        cx.emit(SidebarEvent::SelectParkedTab {
                            path: path.clone(),
                            index,
                        });
                        return;
                    }
                    match kind {
                        RowKind::Project => sidebar.toggle_project(row_id, cx),
                        RowKind::NewWorktree => {
                            sidebar.begin_worktree_prompt(row_id, window, cx);
                        }
                        RowKind::Tab => {}
                        RowKind::Worktree => {
                            // Report the click to the host; the host decides
                            // what actually becomes selected and confirms by
                            // calling back `set_selected_worktree`.
                            if let Some(path) = worktree_path.as_ref() {
                                cx.emit(SidebarEvent::SelectWorktree(path.clone()));
                                sidebar.select_row(row_id, cx);
                            }
                        }
                    }
                });
            });

        row_view = row_view.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                context_entity.update(cx, |sidebar, cx| {
                    sidebar.open_context_menu(row_id, event.position, window, cx)
                });
            },
        );

        if let Some(drag) = drag {
            let drag_entity = entity.clone();
            let move_entity = entity.clone();
            let drop_entity = entity.clone();
            row_view = row_view
                .on_drag(drag, move |_, _, _, cx| {
                    drag_entity.update(cx, |sidebar, _| sidebar.pending_reorder = None);
                    cx.new(|_| gpui::Empty)
                })
                .on_drag_move::<RowDrag>(move |event: &DragMoveEvent<RowDrag>, _, cx| {
                    let drag = *event.drag(cx);
                    let before = event.event.position.y < event.bounds.center().y;
                    move_entity.update(cx, |sidebar, cx| {
                        sidebar.preview_reorder(drag, row_id, before, cx);
                    });
                })
                .on_drop::<RowDrag>(move |_, _, cx| {
                    drop_entity.update(cx, |sidebar, cx| sidebar.confirm_reorder(cx));
                });
        }

        // Sirio owns the row's content; bezel's tree row already supplied
        // disclosure, indentation, cursor/selection paint and hover chrome.
        let main_line = div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .min_h(px(ROW_TITLE_LINE_HEIGHT))
            .child(
                div()
                    .w(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(theme.text_faint)
                    .child(match status_glyph {
                        // Swift's `RunningDots`, tinted by the agent: a
                        // different *shape* from a lifecycle dot, so a
                        // running worktree can never be mistaken for a
                        // finished one at a glance, and a different tint per
                        // agent, so the one glyph carries both facts.
                        RowStatusGlyph::Running(_color) => div()
                            .id(("sidebar-status-running", row_id))
                            .debug_selector(move || format!("sidebar-status-running-{row_id}"))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(loading::compact("sidebar-running-spinner", window, cx))
                            .into_any_element(),
                        RowStatusGlyph::Dot(color) => div()
                            .id(("sidebar-status-dot", row_id))
                            .debug_selector(move || format!("sidebar-status-dot-{row_id}"))
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(color)
                            .into_any_element(),
                        RowStatusGlyph::None => div().into_any_element(),
                    }),
            )
            .child({
                let slot = div().w(px(16.0)).flex().items_center().justify_center();
                // F-CORE-ACT-17: a worktree row's mark is its agent's brand
                // when one owns the worktree, and the branch glyph
                // otherwise. The selector carries which, so the identity is
                // assertable from a drawn test.
                if is_worktree {
                    let name = Self::icon_selector_name(glyph);
                    slot.id(("sidebar-worktree-mark", row_id))
                        .debug_selector(move || format!("sidebar-worktree-mark-{row_id}-{name}"))
                        .child(project_mark)
                        .into_any_element()
                } else if kind == RowKind::Tab {
                    // A tab row names its glyph the same way, so a drawn test
                    // can assert that a pane identified after spawn actually
                    // changed the mark on screen rather than only in a field.
                    let name = Self::icon_selector_name(glyph);
                    slot.id(("sidebar-tab-mark", row_id))
                        .debug_selector(move || format!("sidebar-tab-mark-{row_id}-{name}"))
                        .child(project_mark)
                        .into_any_element()
                } else {
                    slot.child(project_mark).into_any_element()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .debug_selector(move || format!("sidebar-row-title-{row_id}"))
                    .text_ellipsis()
                    .line_height(px(ROW_TITLE_LINE_HEIGHT))
                    .font_weight(if is_project {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    // A parked tab is not live: its title reads as a record
                    // of what the worktree holds, not as an open surface.
                    .when(parked_tab.is_some(), |this| {
                        this.text_color(theme.text_faint)
                    })
                    .child(title),
            )
            .when(is_project, |this| {
                this.child(
                    div()
                        .id(("project-settings", row_id))
                        .debug_selector(move || format!("project-settings-{row_id}"))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(theme.text_faint)
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .child(
                            IconElement::new(Icon::Settings, IconSize::XSmall)
                                .text_color(theme.text),
                        )
                        .on_click(move |_, _window, cx| {
                            if let Some(project_id) = project_id.clone() {
                                remove_entity.update(cx, |_, cx| {
                                    cx.emit(SidebarEvent::OpenProjectSettings(project_id));
                                });
                            }
                        }),
                )
            })
            // F-CORE-ACT-18: `AgentActivityModel::running_agent_ids` already
            // de-duplicated these and put them in `AgentCatalog` order, so
            // the badge draws them left to right exactly as handed over —
            // it never re-sorts and never de-duplicates again.
            .when(!running_agents.is_empty(), |this| {
                this.child(
                    div()
                        .id(("sidebar-running-agents", row_id))
                        .debug_selector(move || format!("sidebar-running-agents-{row_id}"))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(3.0))
                        .children(running_agents.iter().enumerate().map(|(index, mark)| {
                            div()
                                .id(("sidebar-running-agent", row_id * 16 + index))
                                .debug_selector({
                                    let name = Self::icon_selector_name(mark.icon);
                                    move || format!("sidebar-running-agent-{row_id}-{name}")
                                })
                                .flex()
                                .flex_none()
                                .items_center()
                                // Each mark in its own brand. Every mark used
                                // to be tinted `theme.text`, a
                                // coral near enough to Claude's brand to read
                                // as it, so a Codex or Pi mark was drawn in
                                // Claude's colour. Shape carried identity;
                                // colour actively contradicted it. Codex is the
                                // one exception: its mark is drawn in
                                // `theme.text` (white) like everywhere else
                                // in the app — tab bar and status bar never
                                // use its blue brand hex, so the badge must
                                // not be the only blue Codex mark on screen.
                                .child(IconElement::new(mark.icon, IconSize::Small).text_color(
                                    if matches!(mark.icon, Icon::Codex) {
                                        theme.text
                                    } else {
                                        mark.brand.color()
                                    },
                                ))
                        })),
                )
            })
            .when(is_removable_worktree, |this| {
                let remove_entity = entity.clone();
                this.child(
                    div()
                        .id(("remove-worktree", row_id))
                        .debug_selector(move || format!("remove-worktree-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(theme.text_faint)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.element_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            remove_entity.update(cx, |sidebar, cx| {
                                sidebar.request_remove_worktree_row(row_id, window, cx);
                            });
                        })
                        .child(
                            IconElement::new(Icon::Close, IconSize::XSmall)
                                .text_color(theme.text_faint),
                        ),
                )
            })
            .when_some(tab_id, |this, tab_id| {
                this.child(
                    div()
                        .id(("sidebar-tab-close", row_id))
                        .debug_selector(move || format!("sidebar-tab-close-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(14.0))
                        .text_color(theme.text_muted)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.element_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            tab_close_entity.update(cx, |_, cx| {
                                cx.emit(SidebarEvent::CloseTab(tab_id));
                            });
                        })
                        .child(
                            IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.text),
                        ),
                )
            });

        let content = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(ROW_GAP))
            .child(main_line)
            .when(has_sub_line, |this| {
                this.child(
                    div()
                        // Aligned under the title: 12px leading slot + 7px gap
                        // + 16px glyph + 7px gap.
                        .pl(px(42.0))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(12.5))
                        .line_height(px(ROW_SUB_LINE_HEIGHT))
                        .text_color(theme.text_faint)
                        .when(row.is_primary, |this| {
                            this.child(
                                div()
                                    .id(("sidebar-primary-pill", row_id))
                                    .debug_selector(move || {
                                        format!("sidebar-primary-pill-{row_id}")
                                    })
                                    .px(px(5.0))
                                    .rounded(theme.radii.chip)
                                    .bg(theme.surface_raised)
                                    .text_color(theme.text)
                                    .text_size(theme.typography.scaled(11.0))
                                    .child("Primary"),
                            )
                        })
                        // F-SID-11: the durable `worktree.comment` annotation
                        // (`worktree.set` over the control socket) was already
                        // persisted and read by the status bar; the worktree
                        // row itself never rendered it.
                        .when_some(
                            row.comment.filter(|comment| !comment.is_empty()),
                            |this, comment| {
                                this.child(
                                    div()
                                        .id(("sidebar-worktree-comment", row_id))
                                        .debug_selector(move || {
                                            format!("sidebar-worktree-comment-{row_id}")
                                        })
                                        .min_w_0()
                                        .truncate()
                                        .text_color(theme.text_faint)
                                        .child(comment),
                                )
                            },
                        ),
                )
            });

        // A worktree's disclosure is its own control, unlike a project's,
        // whose whole row toggles: the row body must keep meaning "select
        // this worktree". bezel draws the chevron with no handler of its
        // own, so a hit target the size of its column sits over it and
        // stops the click before the row's selection handler sees it.
        let chevron_entity = entity.clone();
        let worktree_chevron = is_worktree && row_shape.expanded.is_some();
        let chevron_left = tree::INDENT * row_shape.depth as f32;
        row_view.child(content).when(worktree_chevron, |this| {
            this.child(
                div()
                    .id(("sidebar-worktree-chevron", row_id))
                    .debug_selector(move || format!("sidebar-worktree-chevron-{row_id}"))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(chevron_left))
                    .w(px(16.0))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        chevron_entity.update(cx, |sidebar, cx| {
                            sidebar.focus_tree_row(row_index, window, cx);
                            sidebar.toggle_worktree(row_id, cx);
                        });
                    }),
            )
        })
    }
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.filter_focus.clone()
    }
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        // Production installs bezel alongside Sirio's theme. Some isolated
        // sidebar fixtures set only the Sirio global, so establish the same
        // invariant lazily before the tree or a popup reads bezel's palette.
        if cx.try_global::<bezel::theme::Theme>().is_none() {
            theme.install_into_bezel(cx);
        }
        let rows = self.visible_rows();
        let entity = cx.entity();
        // The row list consumes one; the worktree prompt below needs another.
        let prompt_owner = entity.clone();
        let project_ids = self.project_ids.clone();
        let project_identities = self.project_identities.clone();
        let row_drags = rows
            .iter()
            .filter_map(|row| self.row_drag(row).map(|drag| (row.id, drag)))
            .collect::<std::collections::HashMap<_, _>>();
        let filter_focus = self.filter_focus.clone();
        let tree_focus = self.tree_focus.clone();
        let filter_is_focused = filter_focus.is_focused(window);
        // Sidebar text fields (filter, project-settings card, worktree
        // prompt) share one blink: window focus is unique, so at most one
        // caret is ever visible.
        let settings_name_focused = self
            .project_settings
            .as_ref()
            .is_some_and(|card| card.display_name_focus.is_focused(window));
        let settings_base_focused = self
            .project_settings
            .as_ref()
            .is_some_and(|card| card.worktree_base_focus.is_focused(window));
        let settings_location_focused = self
            .project_settings
            .as_ref()
            .is_some_and(|card| card.worktree_location_focus.is_focused(window));
        let prompt_focus_focused = self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.focus.is_focused(window));
        let field_focused = filter_is_focused
            || settings_name_focused
            || settings_base_focused
            || settings_location_focused
            || prompt_focus_focused;
        caret::schedule(
            &mut self.field_blink,
            field_focused,
            Self::flip_field_blink,
            cx,
        );
        let field_caret_visible = field_focused && self.field_blink.visible();
        let filter_text = self.filter.clone();
        let filter_is_empty = filter_text.is_empty();
        let prompt = self.prompt.clone();
        let notice = self.notice.clone();
        let context_menu = self.context_menu.get().map(|_| {
            Self::render_context_menu(&self.context_menu, entity.clone(), theme, Painter::of(cx))
                .into_any_element()
        });
        let project_settings = self.project_settings.clone();
        let add_project_menu = self.add_project_menu.get().map(|_| {
            Self::render_add_project_menu(
                &self.add_project_menu,
                entity.clone(),
                theme,
                Painter::of(cx),
            )
            .into_any_element()
        });
        let project_form = self.project_form.clone();
        let reorder_drop_entity = entity.clone();
        let panel_width = self.panel_width;
        let tree_cursor = self.tree_cursor.min(rows.len().saturating_sub(1));
        // Each row is its own view (see `RowView`): push this render's inputs
        // in, notify only on change, and mount it cached at the height the
        // row will take, so a still row is replayed rather than laid out
        // again. Views of rows that are no longer visible are dropped.
        let cache_rows = self.cache_rows;
        let mut row_views = std::mem::take(&mut self.row_views);
        let mut next_views = std::collections::HashMap::with_capacity(rows.len());
        let mut rendered_rows = Vec::with_capacity(rows.len());
        for (index, row) in rows.into_iter().enumerate() {
            let project_id = project_ids.get(&row.id).cloned();
            let project_icon = project_id
                .as_ref()
                .and_then(|id| project_identities.get(id).cloned());
            let inputs = RowInputs {
                drag: row_drags.get(&row.id).copied(),
                cursor: index == tree_cursor,
                has_children: self.row_has_children(&row),
                index,
                project_id,
                project_icon,
                row,
            };
            let row_id = inputs.row.id;
            let row_height = Self::row_min_height(&inputs.row);
            let view = match row_views.remove(&row_id) {
                Some(view) => {
                    view.update(cx, |view, cx| {
                        if view.inputs != inputs {
                            view.inputs = inputs;
                            cx.notify();
                        }
                    });
                    view
                }
                None => cx.new(|_| RowView {
                    sidebar: entity.clone(),
                    inputs,
                    render_count: 0,
                }),
            };
            rendered_rows.push(if cache_rows {
                view.clone()
                    .cached(StyleRefinement::default().w_full().h(px(row_height)))
                    .into_any_element()
            } else {
                view.clone().into_any_element()
            });
            next_views.insert(row_id, view);
        }
        self.row_views = next_views;
        let context_menu_entity = entity.clone();
        let project_surface_entity = entity.clone();
        div()
            .track_focus(&self.context_menu_focus)
            .capture_key_down(move |event, _, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                let mut closed = false;
                project_surface_entity.update(cx, |sidebar, cx| {
                    if sidebar.has_open_project_surface() {
                        sidebar.close_project_surface(cx);
                        closed = true;
                    }
                });
                if closed {
                    cx.stop_propagation();
                }
            })
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    context_menu_entity
                        .update(cx, |sidebar, cx| sidebar.dismiss_context_menu(cx));
                }
            })
            .relative()
            .flex()
            .flex_col()
            .w(px(panel_width))
            .h_full()
            .overflow_hidden()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border_opaque)
            .pt(px(8.0))
            .child(
                div()
                    .h(px(20.0))
                    .w_full()
                    .px(px(FILTER_LEFT_INSET))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(theme.typography.scaled(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_faint)
                    .child("Projects")
                    .child(
                        div()
                            .id("add-project")
                            .debug_selector(|| "add-project".to_string())
                            .w(px(20.0))
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(17.0))
                            .text_color(theme.text_faint)
                            .hover(|style| style.bg(theme.element_hover).rounded(theme.radii.control))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, _| {
                                this.add_project_menu.note_trigger_press();
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_add_project(cx);
                            }))
                            .child("+")
                            .when_some(add_project_menu, |this, menu| this.child(menu)),
                    ),
            )
            .child(
                div()
                    .id("filter-field")
                    .debug_selector(|| "filter-field".to_string())
                    .track_focus(&filter_focus)
                    .relative()
                    .ml(px(FILTER_LEFT_INSET))
                    .mt(px(6.0))
                    .w(px((panel_width - FILTER_LEFT_INSET - ROW_RIGHT_INSET).max(0.0)))
                    .h(px(28.0))
                    .px(px(9.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(if filter_is_focused {
                        theme.ring
                    } else {
                        theme.border
                    })
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.filter_focus.focus(window, cx);
                        }),
                    )
                    .on_key_down(cx.listener(Self::on_filter_key))
                    .child(div().text_size(px(12.5)).text_color(theme.text_faint).child("⌕"))
                            .min_w_0()
                    .child(
                        div()
                            // A long filter is clipped from the start so the
                            // tail being typed stays in view, and the caret
                            // stays inside the field (`caret::field_value`).
                            .overflow_hidden()
                            .flex_1()
                            .flex()
                            .items_center()
                            .text_size(theme.typography.scaled(12.5))
                            .text_color(if filter_is_empty {
                                theme.text_faint
                            } else {
                                theme.text
                            })
                            // Keep the placeholder as its own conditional
                            // element. Its absence is then the renderer's
                            // unambiguous representation of a non-empty
                            // filter, instead of replacing the contents of
                            // the same text child.
                            .when(filter_is_empty, |this| {
                                this.child(
                                    div()
                                        .id("filter-placeholder")
                                        .debug_selector(|| "filter-placeholder".to_owned())
                                        .child("Filter"),
                                )
                            })
                            .when(!filter_is_empty, |this| {
                                this.child(
                                    caret::field_value(filter_text)
                                        .debug_selector(|| "filter-text".to_owned()),
                                )
                            })
                            .when(filter_is_focused, |this| {
                                this.child(caret::bar(px(12.0), theme.text, field_caret_visible))
                            }),
                    ),
            )
            .child(
                div()
                    .id("sidebar-tree")
                    .debug_selector(|| "sidebar-tree".to_owned())
                    .key_context(tree::KEY_CONTEXT)
                    .track_focus(&tree_focus)
                    .on_action(cx.listener(|sidebar, _: &tree::SelectPrevious, _, cx| {
                        sidebar.tree_step(tree::Direction::Up, cx);
                    }))
                    .on_action(cx.listener(|sidebar, _: &tree::SelectNext, _, cx| {
                        sidebar.tree_step(tree::Direction::Down, cx);
                    }))
                    .on_action(cx.listener(|sidebar, _: &tree::Collapse, _, cx| {
                        sidebar.tree_step(tree::Direction::Left, cx);
                    }))
                    .on_action(cx.listener(|sidebar, _: &tree::Expand, _, cx| {
                        sidebar.tree_step(tree::Direction::Right, cx);
                    }))
                    .mt(px(11.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .h_full()
                    .overflow_y_scroll()
                    // Rows reorder during the drag, so the row originally
                    // under the pointer may be a different entity by
                    // mouse-up. Commit against this stable drop surface;
                    // `pending_reorder` still contains the typed target
                    // selected by the last drag-move event.
                    .on_drop::<RowDrag>(move |_, _, cx| {
                        reorder_drop_entity.update(cx, |sidebar, cx| sidebar.confirm_reorder(cx));
                    })
                    .child(
                        tree::tree()
                            .flex_none()
                            .gap(px(ROW_V_GAP))
                            .children(rendered_rows),
                    ),
            )
            .when(notice.is_some(), |this| {
                this.child(
                    div()
                        .id("sidebar-notice")
                        .w_full()
                        .px(px(FILTER_LEFT_INSET))
                        .py(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.diff_del)
                        .child(notice.unwrap_or_default()),
                )
            })
            .when(prompt.is_some(), |this| {
                let prompt = prompt.expect("checked above");
                let prompt_entity = prompt_owner.clone();
                this.child(
                    div()
                        .id("worktree-prompt")
                        .absolute()
                        .left(px(0.0))
                        .right(px(0.0))
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x000000).alpha(0.35))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                if let Some(prompt) = &this.prompt {
                                    prompt.focus.focus(window, cx);
                                }
                            }),
                        )
                        .child(
                            div()
                                .id("worktree-prompt-card")
                                .debug_selector(|| "worktree-prompt".to_string())
                                .track_focus(&prompt.focus)
                                .w(px(260.0))
                                .rounded(theme.radii.toast)
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.border)
                                .px(px(14.0))
                                .py(px(12.0))
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .on_key_down(cx.listener(Self::on_prompt_key))
                                .on_click(move |_, window, cx| {
                                    // Keep the card focused when clicked.
                                    let focus = prompt_entity
                                        .read(cx)
                                        .prompt
                                        .as_ref()
                                        .map(|prompt| prompt.focus.clone());
                                    if let Some(focus) = focus {
                                        focus.focus(window, cx);
                                    }
                                })
                                .child(
                                    div()
                                        .text_size(theme.typography.headline)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text)
                                        .child(format!("New worktree in {}", prompt.project_name)),
                                )
                                .child(Self::render_worktree_prompt_field(
                                    "worktree-prompt-branch",
                                    "branch name",
                                    &prompt.draft,
                                    WorktreePromptField::Branch,
                                    prompt.focused_field == WorktreePromptField::Branch,
                                    prompt.focused_field == WorktreePromptField::Branch
                                        && prompt_focus_focused
                                        && field_caret_visible,
                                    &prompt_owner,
                                    theme,
                                ))
                                .child(Self::render_worktree_prompt_field(
                                    "worktree-prompt-base",
                                    // F-CORE-DOM-02: the placeholder describes
                                    // what blank actually resolves to, which
                                    // is the pinned base once one is set —
                                    // not unconditionally HEAD.
                                    &match &prompt.default_base {
                                        Some(base) => {
                                            format!("optional — defaults to the pinned base ({base})")
                                        }
                                        None => "base branch (optional, defaults to HEAD)".to_string(),
                                    },
                                    &prompt.base_draft,
                                    WorktreePromptField::Base,
                                    prompt.focused_field == WorktreePromptField::Base,
                                    prompt.focused_field == WorktreePromptField::Base
                                        && prompt_focus_focused
                                        && field_caret_visible,
                                    &prompt_owner,
                                    theme,
                                ))
                                .child(Self::render_worktree_prompt_field(
                                    "worktree-prompt-location",
                                    &match &prompt.default_location_override {
                                        Some(location) => {
                                            format!("optional — defaults to the pinned location ({location})")
                                        }
                                        None => {
                                            "location (optional, defaults next to project)".to_string()
                                        }
                                    },
                                    &prompt.location_draft,
                                    WorktreePromptField::Location,
                                    prompt.focused_field == WorktreePromptField::Location,
                                    prompt.focused_field == WorktreePromptField::Location
                                        && prompt_focus_focused
                                        && field_caret_visible,
                                    &prompt_owner,
                                    theme,
                                ))
                                .when(prompt.error.is_some(), |this| {
                                    this.child(
                                        div()
                                            .text_size(theme.typography.footnote)
                                            .text_color(theme.diff_del)
                                            .child(prompt.error.clone().unwrap_or_default()),
                                    )
                                })
                                .child(
                                    div()
                                        .text_size(theme.typography.caption2)
                                        .text_color(theme.text_faint)
                                        .child(
                                            "Tab to switch field · Enter to create · Esc to cancel",
                                        ),
                                ),
                        ),
                )
            })
            .when_some(context_menu, |this, menu| this.child(menu))
            .when_some(project_settings, |this, card| {
                this.child(Self::render_project_settings(
                    card,
                    entity.clone(),
                    theme,
                    [
                        settings_name_focused,
                        settings_base_focused,
                        settings_location_focused,
                    ],
                    field_caret_visible,
                ))
            })
            .when_some(project_form, |this, form| {
                this.child(Self::render_project_form(form, entity.clone(), theme))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_identity::ProjectGlyph;

    /// A path picker that cannot open must say so, not fail silently.
    ///
    /// The three call sites — add-project, worktree-location, and the icon
    /// chooser — all take the same `Result<Result<Option<Vec<PathBuf>>>, _>`,
    /// and one of them used to read it as `let Ok(Ok(Some(paths))) = outcome
    /// else { return }`. That collapses "no XDG portal on this session" into
    /// the same branch as "the user pressed Cancel", so clicking the control
    /// on a portal-less desktop did nothing at all and explained nothing.
    /// Found by a critic driving the real dialog, not by a test — hence this
    /// one.
    #[test]
    fn a_picker_that_cannot_open_is_never_silent() {
        let unavailable: Result<Result<Option<Vec<PathBuf>>, String>, ()> =
            Ok(Err("no portal".into()));
        assert_eq!(
            PickedPath::from_prompt(unavailable),
            PickedPath::Unavailable("no portal".into()),
            "a platform failure must carry a reason to the surface"
        );
    }

    /// The other half of the clone-truncation finding: even once
    /// `GitClone::clone`'s flag survives the call chain, a `Cloned` event
    /// that always closes the popover would delete the notice before
    /// anyone reads it (`CloneFormEvent::Cloned` and `Complete`'s auto-close
    /// used to run unconditionally on every successful clone). A clean
    /// clone must still close automatically — that part of the existing UX
    /// stays — but a truncated one must not.
    #[test]
    fn only_a_truncated_clone_keeps_the_popover_open() {
        assert!(
            !Sidebar::clone_form_stays_open_after(false),
            "an ordinary completion must keep auto-closing, as it always has"
        );
        assert!(
            Sidebar::clone_form_stays_open_after(true),
            "a truncated completion must not close before its notice is seen"
        );
    }

    /// ...and the three ways of choosing nothing stay silent, so the notice
    /// means something when it does appear.
    #[test]
    fn cancelling_a_picker_says_nothing() {
        let cancelled: Result<Result<Option<Vec<PathBuf>>, String>, ()> = Ok(Ok(None));
        let empty: Result<Result<Option<Vec<PathBuf>>, String>, ()> = Ok(Ok(Some(Vec::new())));
        let window_gone: Result<Result<Option<Vec<PathBuf>>, String>, ()> = Err(());
        for (label, outcome) in [
            ("cancelled", cancelled),
            ("empty selection", empty),
            ("window dropped", window_gone),
        ] {
            assert_eq!(
                PickedPath::from_prompt(outcome),
                PickedPath::Nothing,
                "{label} should not raise a notice"
            );
        }
    }

    #[test]
    fn choosing_a_folder_yields_that_folder() {
        let chosen: Result<Result<Option<Vec<PathBuf>>, String>, ()> =
            Ok(Ok(Some(vec![PathBuf::from("/tmp/somewhere")])));
        assert_eq!(
            PickedPath::from_prompt(chosen),
            PickedPath::Chosen(PathBuf::from("/tmp/somewhere"))
        );
    }

    /// The location field's text round-trips: it is persisted as the
    /// project's worktree base and later joined onto. `display_path` would
    /// collapse a home-relative choice to "~", which `Path::join` treats as a
    /// directory of that name — so this seam must stay absolute.
    #[test]
    fn the_worktree_location_field_keeps_the_home_directory_spelled_out() {
        let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        else {
            return;
        };
        let text = Sidebar::worktree_location_text(&home.join("code"));
        assert!(
            !text.starts_with('~'),
            "the location field must not collapse the home directory, got {text}"
        );
        assert!(
            text.ends_with("code"),
            "the location field must still name the chosen directory, got {text}"
        );
    }

    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta,
        ScrollWheelEvent, TouchPhase, VisualTestContext, point, size,
    };
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Creates a throwaway git repo with one commit.
    fn scratch_repo(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "sirio-sidebar-test-{tag}-{}-{unique}",
            std::process::id()
        ));
        let repo = parent.join("repo");
        std::fs::create_dir_all(&repo).expect("create repo dir");
        // Canonicalize so derived paths match what porcelain reports
        // (macOS /var is a symlink to /private/var).
        let repo = std::fs::canonicalize(&repo).expect("canonicalize repo");
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "test@sirio.dev"],
            vec!["config", "user.name", "Sirio Test"],
        ] {
            assert!(
                Command::new("git")
                    .args(&args)
                    .current_dir(&repo)
                    .status()
                    .expect("git")
                    .success()
            );
        }
        std::fs::write(repo.join("file.txt"), "one\ntwo\nthree\n").expect("write");
        assert!(
            Command::new("git")
                .args(["add", "-A"])
                .current_dir(&repo)
                .status()
                .expect("git")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "root"])
                .current_dir(&repo)
                .status()
                .expect("git")
                .success()
        );
        repo
    }

    fn porcelain(repo: &std::path::Path) -> String {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo)
            .output()
            .expect("git");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[gpui::test]
    async fn multi_project_worktree_fixture_preserves_project_root_hierarchy(
        cx: &mut gpui::TestAppContext,
    ) {
        let projects = vec![
            SidebarProject {
                id: "first".to_string(),
                name: "First Project".to_string(),
                is_git: true,
                root_path: PathBuf::from("/tmp/first"),
                worktrees: vec![
                    SidebarWorktree {
                        branch: "main".to_string(),
                        path: PathBuf::from("/tmp/first-main"),
                        is_primary: true,
                        comment: None,
                    },
                    SidebarWorktree {
                        branch: "feature".to_string(),
                        path: PathBuf::from("/tmp/first-feature"),
                        is_primary: false,
                        comment: None,
                    },
                ],
            },
            SidebarProject {
                id: "second".to_string(),
                name: "Second Project".to_string(),
                is_git: true,
                root_path: PathBuf::from("/tmp/second"),
                worktrees: vec![SidebarWorktree {
                    branch: "main".to_string(),
                    path: PathBuf::from("/tmp/second-main"),
                    is_primary: true,
                    comment: None,
                }],
            },
        ];

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(projects, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Exercise the exact row-building boundary that feeds rendering.
        let actual = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .update(cx, |sidebar, _| {
                    sidebar
                        .visible_rows()
                        .into_iter()
                        .map(|row| (row.kind, row.depth, row.title))
                        .collect::<Vec<_>>()
                })
        });
        assert_eq!(
            actual,
            vec![
                (RowKind::Project, 0, "First Project".to_string()),
                (RowKind::Worktree, 1, "main".to_string()),
                (RowKind::Worktree, 1, "feature".to_string()),
                (RowKind::NewWorktree, 1, "New Worktree...".to_string()),
                (RowKind::Project, 0, "Second Project".to_string()),
                (RowKind::Worktree, 1, "main".to_string()),
                (RowKind::NewWorktree, 1, "New Worktree...".to_string()),
            ],
            "row data already contains project roots and depth-one worktree children"
        );
    }

    #[gpui::test]
    async fn a_long_worktree_list_scrolls_inside_the_sidebar_viewport(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let project_root = PathBuf::from("/tmp/sidebar-scroll-project");
        let worktrees = (0..20)
            .map(|index| SidebarWorktree {
                branch: format!("worktree-{index}"),
                path: project_root.join(format!("worktree-{index}")),
                is_primary: index == 0,
                comment: None,
            })
            .collect();
        let window = cx.open_window(size(px(320.0), px(240.0)), |_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "scroll-project".into(),
                    name: "Scroll Project".into(),
                    is_git: true,
                    root_path: project_root.clone(),
                    worktrees,
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let last_row_before = cx
            .debug_bounds("new-worktree-row")
            .expect("the last sidebar row is rendered");
        let tree = cx
            .debug_bounds("sidebar-tree")
            .expect("the sidebar tree viewport is rendered");
        assert!(
            tree.bottom() <= px(240.0),
            "the sidebar tree viewport must stay inside the short window: tree={tree:?}"
        );

        cx.simulate_event(ScrollWheelEvent {
            position: point(px(160.0), px(180.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-1000.0))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
        cx.run_until_parked();

        let last_row_after = cx
            .debug_bounds("new-worktree-row")
            .expect("the last sidebar row remains in the scrollable tree");
        assert!(
            last_row_after.top() < last_row_before.top(),
            "scrolling down must move the last row into view: before={last_row_before:?}, \
             after={last_row_after:?}, tree={tree:?}"
        );
        assert!(
            last_row_after.bottom() <= px(240.0),
            "the last row must be reachable inside the short window: row={last_row_after:?}"
        );
        assert!(
            last_row_after.top() >= tree.top() && last_row_after.bottom() <= tree.bottom(),
            "the last row must be visible inside the sidebar viewport: row={last_row_after:?}, \
             tree={tree:?}"
        );
    }

    #[gpui::test]
    async fn narrow_sidebar_truncates_long_row_labels_without_overlap(
        cx: &mut gpui::TestAppContext,
    ) {
        let project_root = PathBuf::from("/tmp/sidebar-narrow-labels");
        cx.update(Theme::init);
        let window = cx.open_window(size(px(320.0), px(240.0)), |_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "narrow-labels".into(),
                    name: "Project".into(),
                    is_git: true,
                    root_path: project_root.clone(),
                    worktrees: vec![
                        SidebarWorktree {
                            branch: "worktree/green-meadow-592b".into(),
                            path: project_root.join("green-meadow"),
                            is_primary: false,
                            comment: None,
                        },
                        SidebarWorktree {
                            branch: "worktree/green-valley-a9fb".into(),
                            path: project_root.join("green-valley"),
                            is_primary: false,
                            comment: None,
                        },
                    ],
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.set_panel_width(180.0, cx));
        });
        cx.run_until_parked();

        let first_row = cx
            .debug_bounds("sidebar-row-1")
            .expect("the first worktree row is drawn");
        let second_row = cx
            .debug_bounds("sidebar-row-2")
            .expect("the second worktree row is drawn");
        let first_title = cx
            .debug_bounds("sidebar-row-title-1")
            .expect("the first worktree title is drawn");
        let second_title = cx
            .debug_bounds("sidebar-row-title-2")
            .expect("the second worktree title is drawn");

        assert!(
            first_title.size.height <= px(ROW_TITLE_LINE_HEIGHT),
            "a narrow row title must stay one line: title={first_title:?}"
        );
        assert!(
            second_title.size.height <= px(ROW_TITLE_LINE_HEIGHT),
            "a narrow row title must stay one line: title={second_title:?}"
        );
        assert!(
            first_row.bottom() <= second_row.top(),
            "long labels must not paint into the next row: first={first_row:?}, second={second_row:?}"
        );
    }

    fn structural_row(kind: RowKind, depth: usize, expanded: bool) -> SidebarRow {
        SidebarRow {
            id: depth,
            kind,
            depth,
            title: "row".to_string(),
            selected: false,
            expanded,
            is_primary: false,
            agent_status: None,
            is_git: true,
            path: None,
            tab_id: None,
            parked_tab: None,
            tab_kind: None,
            agent_icon: None,
            agent_brand: None,
            comment: None,
            running_agents: Vec::new(),
        }
    }

    #[test]
    fn project_and_worktree_rows_map_to_bezel_tree_levels() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Project, 0, true), true),
            tree::Row::branch(0, true)
        );
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Worktree, 1, false), false),
            tree::Row::leaf(1)
        );
    }

    #[test]
    fn project_tree_rows_keep_the_sidebar_expansion_state() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Project, 0, false), true),
            tree::Row::branch(0, false)
        );
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Project, 0, true), true),
            tree::Row::branch(0, true)
        );
    }

    /// A project is a container even when empty, so it always carries a
    /// chevron; a worktree only earns one once it has tab rows to hide.
    #[test]
    fn a_project_is_a_branch_even_without_children() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Project, 0, true), false),
            tree::Row::branch(0, true)
        );
    }

    #[test]
    fn a_worktree_without_tab_rows_is_a_leaf() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Worktree, 1, true), false),
            tree::Row::leaf(1)
        );
    }

    #[test]
    fn a_worktree_with_tab_rows_is_a_branch_keeping_its_expansion_state() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Worktree, 1, true), true),
            tree::Row::branch(1, true)
        );
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Worktree, 1, false), true),
            tree::Row::branch(1, false)
        );
    }

    #[test]
    fn the_new_worktree_action_is_a_depth_one_leaf() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::NewWorktree, 1, false), false),
            tree::Row::leaf(1)
        );
    }

    #[test]
    fn tab_rows_are_depth_two_leaves() {
        assert_eq!(
            Sidebar::tree_row(&structural_row(RowKind::Tab, 2, false), false),
            tree::Row::leaf(2)
        );
    }

    #[test]
    fn bezel_parent_navigation_matches_the_sidebar_hierarchy() {
        let shape = [
            Sidebar::tree_row(&structural_row(RowKind::Project, 0, true), true),
            Sidebar::tree_row(&structural_row(RowKind::Worktree, 1, true), true),
            Sidebar::tree_row(&structural_row(RowKind::Tab, 2, false), false),
            Sidebar::tree_row(&structural_row(RowKind::NewWorktree, 1, false), false),
        ];
        assert_eq!(tree::parent_of(&shape, 1), Some(0));
        assert_eq!(tree::parent_of(&shape, 2), Some(1));
        assert_eq!(tree::parent_of(&shape, 3), Some(0));
    }

    /// The fixture's first worktree (row 1) owns a tab row (row 2) and is
    /// followed by the New Worktree action (row 3). Collapsing the worktree
    /// hides only what hangs under it.
    #[gpui::test]
    async fn collapsing_a_worktree_hides_its_tab_rows_but_not_its_siblings(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let sidebar = cx.new(|cx| Sidebar::new_with_repo(cx, Some(PathBuf::from("fixture-repo"))));
        let visible_ids = |sidebar: &Sidebar| {
            sidebar
                .visible_rows()
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        };

        sidebar.read_with(cx, |sidebar, _| {
            assert!(
                visible_ids(sidebar).contains(&2),
                "a worktree starts expanded: its tab row is visible"
            );
        });

        sidebar.update(cx, |sidebar, cx| sidebar.toggle_worktree(1, cx));
        sidebar.read_with(cx, |sidebar, _| {
            let ids = visible_ids(sidebar);
            assert!(ids.contains(&1), "the collapsed worktree row itself stays");
            assert!(!ids.contains(&2), "its tab row is hidden");
            assert!(ids.contains(&3), "the New Worktree sibling is untouched");
        });

        sidebar.update(cx, |sidebar, cx| sidebar.toggle_worktree(1, cx));
        sidebar.read_with(cx, |sidebar, _| {
            assert!(
                visible_ids(sidebar).contains(&2),
                "toggling again restores the tab row"
            );
        });
    }

    /// The filter follows the project rule one level down: a collapsed
    /// worktree's tab rows stay hidden unless the query matches one of them.
    #[gpui::test]
    async fn filter_reveals_a_collapsed_worktrees_matching_tab_rows_only(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let sidebar = cx.new(|cx| Sidebar::new_with_repo(cx, None));
        sidebar.update(cx, |sidebar, cx| {
            sidebar.toggle_worktree(1, cx);
            sidebar.filter = "main".to_string();
        });
        sidebar.read_with(cx, |sidebar, _| {
            let ids = sidebar
                .visible_rows()
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>();
            assert!(ids.contains(&1), "the worktree itself matches");
            assert!(
                !ids.contains(&2),
                "a non-matching tab row under a collapsed worktree stays hidden"
            );
        });

        sidebar.update(cx, |sidebar, _| sidebar.filter = "chat".to_string());
        sidebar.read_with(cx, |sidebar, _| {
            let ids = sidebar
                .visible_rows()
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>();
            assert!(
                ids.contains(&2),
                "a matching tab row is shown even under a collapsed worktree"
            );
        });
    }

    /// The worktree chevron is its own control: clicking it folds the tab
    /// rows without reporting a selection, clicking the row still selects,
    /// and bezel's ←/→ fold and unfold the same row from the keyboard.
    #[gpui::test]
    async fn worktree_chevron_toggles_tab_rows_without_selecting(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::tree::init);
        let repo = PathBuf::from("fixture-repo");
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        assert!(
            cx.debug_bounds("sidebar-row-2").is_some(),
            "the worktree starts open: its tab row is drawn"
        );

        // The chevron rides in bezel's 16px disclosure column, after one
        // level of indent guide.
        let row1 = cx
            .debug_bounds("sidebar-row-1")
            .expect("the worktree row is drawn");
        let chevron = point(row1.origin.x + px(tree::INDENT) + px(8.0), row1.center().y);
        cx.simulate_click(chevron, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-2").is_none(),
            "the chevron click folds the worktree's tab row"
        );
        assert!(
            events.borrow().is_empty(),
            "the chevron is not a selection: nothing is reported, got {:?}",
            events.borrow()
        );

        cx.simulate_click(chevron, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-2").is_some(),
            "a second chevron click unfolds it again"
        );

        // The keyboard path folds the same row: the chevron click left the
        // tree cursor on it.
        cx.simulate_keystrokes("left");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-2").is_none(),
            "← on a worktree row folds its tab rows"
        );
        cx.simulate_keystrokes("right");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-2").is_some(),
            "→ on a worktree row unfolds them"
        );

        // The row body is still the selection control it always was.
        let row1 = cx.debug_bounds("sidebar-row-1").expect("worktree row");
        cx.simulate_click(row1.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            events
                .borrow()
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectWorktree(path) if *path == repo)),
            "clicking the row body reports SelectWorktree, got {:?}",
            events.borrow()
        );
    }

    /// A parked tab is one the host no longer holds live (its worktree was
    /// switched away from) but still lists from the persisted strip. Its
    /// row is drawn under the worktree, offers no ✕ (there is no live tab
    /// to close), and a click asks the host to bring the worktree back with
    /// that tab active rather than naming a tab id that does not exist.
    #[gpui::test]
    async fn parked_tab_rows_report_a_parked_selection_and_offer_no_close(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let repo = PathBuf::from("fixture-repo");
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        sidebar.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                1,
                vec![
                    SidebarTab {
                        tab: SidebarTabRef::Parked(0),
                        title: "Old Terminal".into(),
                        selected: false,
                        kind: TabKind::Terminal,
                        agent: None,
                    },
                    SidebarTab {
                        tab: SidebarTabRef::Parked(1),
                        title: "Old Chat".into(),
                        selected: false,
                        kind: TabKind::AgentChat,
                        agent: AgentMark::for_agent_id("claude"),
                    },
                ],
                cx,
            );
        });
        cx.run_until_parked();

        let second_id = parked_tab_row_id(1, 1);
        let row_selector: &'static str =
            Box::leak(format!("sidebar-row-{second_id}").into_boxed_str());
        let close_selector: &'static str =
            Box::leak(format!("sidebar-tab-close-{second_id}").into_boxed_str());
        let mark_selector: &'static str =
            Box::leak(format!("sidebar-tab-mark-{second_id}-claude-mark").into_boxed_str());
        let row = cx
            .debug_bounds(row_selector)
            .expect("the parked tab row is drawn under its worktree");
        assert!(
            cx.debug_bounds(mark_selector).is_some(),
            "a parked agent tab keeps its brand mark"
        );
        cx.simulate_mouse_move(row.center(), None, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds(close_selector).is_none(),
            "a parked tab has no live tab to close, so no ✕ even on hover"
        );

        cx.simulate_click(row.center(), Modifiers::none());
        cx.run_until_parked();
        let emitted = events.borrow();
        assert!(
            emitted.iter().any(|event| matches!(
                event,
                SidebarEvent::SelectParkedTab { path, index: 1 } if *path == repo
            )),
            "clicking a parked tab row reports the worktree path and the tab's index, got {emitted:?}"
        );
        assert!(
            !emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectTab(_))),
            "and never a live tab id it does not have"
        );
    }

    /// The host re-pushes a worktree's list on every sync: live tabs
    /// replace parked rows in place, and an empty list clears either.
    #[gpui::test]
    async fn live_tabs_replace_parked_rows_and_an_empty_list_clears_them(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let sidebar = cx.new(|cx| Sidebar::new_with_repo(cx, Some(PathBuf::from("fixture-repo"))));
        let tab_rows_under_1 = |sidebar: &Sidebar| {
            let start = sidebar
                .rows
                .iter()
                .position(|row| row.id == 1)
                .expect("worktree row 1");
            sidebar.rows[start + 1..]
                .iter()
                // The fixture's decorative "Chat" row (id 2) is not
                // host-sourced and stays put; only live and parked rows are
                // this call's.
                .take_while(|row| {
                    row.kind == RowKind::Tab && (row.tab_id.is_some() || row.parked_tab.is_some())
                })
                .map(|row| row.id)
                .collect::<Vec<_>>()
        };
        let parked = |index: usize| SidebarTab {
            tab: SidebarTabRef::Parked(index),
            title: format!("Parked {index}"),
            selected: false,
            kind: TabKind::Terminal,
            agent: None,
        };
        let live = |id: usize| SidebarTab {
            tab: SidebarTabRef::Open(id),
            title: format!("Live {id}"),
            selected: false,
            kind: TabKind::Terminal,
            agent: None,
        };

        sidebar.update(cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, vec![parked(0), parked(1)], cx);
        });
        sidebar.read_with(cx, |sidebar, _| {
            assert_eq!(
                tab_rows_under_1(sidebar),
                vec![parked_tab_row_id(1, 0), parked_tab_row_id(1, 1)]
            );
        });

        sidebar.update(cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, vec![live(7)], cx);
        });
        sidebar.read_with(cx, |sidebar, _| {
            assert_eq!(
                tab_rows_under_1(sidebar),
                vec![TAB_ROW_ID_OFFSET + 7],
                "live tabs replace the parked rows rather than stacking under them"
            );
        });

        sidebar.update(cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, Vec::new(), cx);
        });
        sidebar.read_with(cx, |sidebar, _| {
            assert!(tab_rows_under_1(sidebar).is_empty());
        });
    }

    /// The collapsed set is keyed by checkout path, so it survives the host
    /// rebuilding the rows (`set_projects`) and moving the selection to a
    /// different worktree — the point of the feature: a worktree the user
    /// closed stays closed, one they left open stays open.
    #[gpui::test]
    async fn a_collapsed_worktree_stays_collapsed_across_rebuilds_and_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let root = std::path::PathBuf::from("/tmp/sirio-collapse-fixture");
        let projects = || {
            vec![SidebarProject {
                id: "proj".into(),
                name: "proj".into(),
                is_git: true,
                root_path: root.clone(),
                worktrees: vec![
                    SidebarWorktree {
                        branch: "main".into(),
                        path: root.join("main"),
                        is_primary: true,
                        comment: None,
                    },
                    SidebarWorktree {
                        branch: "feature".into(),
                        path: root.join("feature"),
                        is_primary: false,
                        comment: None,
                    },
                ],
            }]
        };
        let tab = || SidebarTab {
            tab: SidebarTabRef::Open(0),
            title: "Terminal".into(),
            selected: false,
            kind: TabKind::Terminal,
            agent: None,
        };
        let sidebar = cx.new(|cx| Sidebar::from_projects(projects(), cx));
        // Row ids follow `from_projects`: project 0, worktrees 1 and 2.
        sidebar.update(cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, vec![tab()], cx);
            sidebar.set_worktree_tabs(2, vec![tab()], cx);
            sidebar.toggle_worktree(1, cx);
        });

        // The host rebuilds the rows and re-pushes the tabs, then selects
        // the other worktree.
        sidebar.update(cx, |sidebar, cx| {
            sidebar.set_projects(projects(), cx);
            sidebar.set_worktree_tabs(1, vec![tab()], cx);
            sidebar.set_worktree_tabs(2, vec![tab()], cx);
            sidebar.set_selected_worktree(&root.join("feature"), cx);
        });

        sidebar.read_with(cx, |sidebar, _| {
            let visible = sidebar.visible_rows();
            let tab_rows_under = |worktree_id: usize| {
                let start = visible
                    .iter()
                    .position(|row| row.id == worktree_id)
                    .expect("worktree row visible");
                visible[start + 1..]
                    .iter()
                    .take_while(|row| row.kind == RowKind::Tab)
                    .count()
            };
            assert_eq!(
                tab_rows_under(1),
                0,
                "the worktree the user closed stays closed after a rebuild and a selection change"
            );
            assert_eq!(
                tab_rows_under(2),
                1,
                "the worktree the user left open stays open"
            );
            let main_row = sidebar
                .rows
                .iter()
                .find(|row| row.id == 1)
                .expect("main row");
            assert!(
                !main_row.expanded,
                "the rebuilt row carries the collapsed state"
            );
        });
    }

    /// The row's rhythm is a minimum, not a prediction: a row keeps the
    /// height its own line count needs, whatever title the content layout
    /// asks for above that floor.
    ///
    /// #151 changed which rows have two lines. A project or worktree row is
    /// a two-line card only when something occupies its sub-line — before,
    /// the checkout path put something there unconditionally, so every such
    /// row was 51px whether or not it had anything else to say.
    #[test]
    fn a_rows_height_uses_only_the_row_kind_minimum() {
        let mut row = SidebarRow {
            id: 0,
            kind: RowKind::Project,
            depth: 0,
            title: "sirio".to_owned(),
            selected: false,
            expanded: true,
            is_primary: false,
            agent_status: None,
            is_git: true,
            path: Some(PathBuf::from("/tmp/sirio")),
            tab_id: None,
            parked_tab: None,
            tab_kind: None,
            agent_icon: None,
            agent_brand: None,
            comment: None,
            running_agents: Vec::new(),
        };
        assert_eq!(
            Sidebar::row_min_height(&row),
            ROW_HEIGHT,
            "a project row with no pill and no comment has one line, so it must \r
             collapse to the action-row height rather than keep paying for the \r
             sub-line the path used to occupy"
        );

        row.title = "a".repeat(40);
        assert_eq!(
            Sidebar::row_min_height(&row),
            ROW_HEIGHT,
            "a long title must grow from its content, not from a character estimate"
        );

        row.kind = RowKind::Worktree;
        row.is_primary = true;
        assert_eq!(
            Sidebar::row_min_height(&row),
            CARD_TWO_LINE_HEIGHT,
            "a primary worktree still draws its pill on a sub-line"
        );

        row.is_primary = false;
        row.comment = Some("release branch".to_owned());
        assert_eq!(
            Sidebar::row_min_height(&row),
            CARD_TWO_LINE_HEIGHT,
            "a worktree comment still draws on a sub-line"
        );

        row.comment = Some(String::new());
        assert_eq!(
            Sidebar::row_min_height(&row),
            ROW_HEIGHT,
            "an empty comment is not content, so it must not buy a second line"
        );

        row.comment = None;
        row.kind = RowKind::Tab;
        row.path = None;
        assert_eq!(
            Sidebar::row_min_height(&row),
            ROW_HEIGHT,
            "a long leaf title must keep the action-row height as its minimum"
        );
    }

    /// #372: the confirm dialog must name its target — branch and checkout
    /// path — so a reorder between right-click and confirm cannot silently
    /// retarget a destructive, irreversible deletion.
    #[test]
    fn remove_worktree_prompt_names_the_branch_and_path() {
        let (title, detail) = Sidebar::remove_worktree_prompt(
            "qa-test-wt",
            &PathBuf::from("/tmp/sirio-qa-test-wt"),
        );
        assert!(
            title.contains("qa-test-wt"),
            "the title must name the branch, got {title:?}"
        );
        assert!(
            detail.contains("qa-test-wt"),
            "the detail must name the branch, got {detail:?}"
        );
        assert!(
            detail.contains("/tmp/sirio-qa-test-wt"),
            "the detail must name the checkout path, got {detail:?}"
        );
    }

    /// #372: the primary checkout cannot be `git worktree remove`d, so its
    /// context-menu entry stays visible but disabled with a reason instead
    /// of offering the destructive dialog; any other worktree stays enabled.
    #[test]
    fn only_a_non_primary_worktree_offers_removal() {
        let primary = Sidebar::context_menu_items(&SidebarContextTarget::Worktree {
            path: PathBuf::from("/tmp/sirio"),
            is_primary: true,
        });
        let primary_item = primary
            .iter()
            .find(|item| item.action == SidebarContextAction::RemoveWorktree)
            .expect("the primary worktree still exposes Remove Worktree");
        assert!(
            !primary_item.enabled,
            "Remove Worktree must be disabled on the primary checkout"
        );
        assert_eq!(
            primary_item.disabled_reason,
            Some(SidebarDisabledReason::PrimaryWorktree),
            "the disabled primary entry must say why"
        );

        let secondary = Sidebar::context_menu_items(&SidebarContextTarget::Worktree {
            path: PathBuf::from("/tmp/sirio-qa-test-wt"),
            is_primary: false,
        });
        let secondary_item = secondary
            .iter()
            .find(|item| item.action == SidebarContextAction::RemoveWorktree)
            .expect("a secondary worktree exposes Remove Worktree");
        assert!(
            secondary_item.enabled,
            "Remove Worktree stays enabled off the primary checkout"
        );
        assert_eq!(
            secondary_item.disabled_reason, None,
            "an enabled entry carries no disabled reason"
        );
    }

    #[test]
    fn terminal_tab_icon_ignores_title() {
        let row = SidebarRow {
            id: 1,
            kind: RowKind::Tab,
            depth: 2,
            title: "foo".to_string(),
            selected: false,
            expanded: false,
            agent_status: None,
            is_primary: false,
            is_git: false,
            path: None,
            tab_id: Some(1),
            parked_tab: None,
            tab_kind: Some(TabKind::Terminal),
            agent_icon: None,
            agent_brand: None,
            comment: None,
            running_agents: Vec::new(),
        };

        assert_eq!(Sidebar::row_icon(&row), Icon::SquareTerminal);
    }

    #[test]
    fn agent_tab_icon_ignores_title() {
        let row = SidebarRow {
            id: 2,
            kind: RowKind::Tab,
            depth: 2,
            title: "renamed agent".to_string(),
            selected: false,
            expanded: false,
            agent_status: None,
            is_primary: false,
            is_git: false,
            path: None,
            tab_id: Some(2),
            parked_tab: None,
            tab_kind: Some(TabKind::Terminal),
            agent_icon: Some(Icon::ClaudeCode),
            agent_brand: None,
            comment: None,
            running_agents: Vec::new(),
        };

        assert_eq!(Sidebar::row_icon(&row), Icon::ClaudeCode);
    }

    /// F-CORE-DOM-03: the Clone/Create forms must propose
    /// `sirio_project::default_project_base()`'s deterministic default —
    /// not some independent HOME/temp_dir guess — and the proposal must
    /// track a Linux-specific override (`SIRIO_PROJECTS_DIR`), matching
    /// the VERIFY clause's "observe the proposed project location ...
    /// repeat with a Linux replacement root and confirm it is
    /// deterministic". Serialized on `env_lock` because it mutates process
    /// environment shared with every other `#[test]` in this binary.
    #[test]
    fn project_form_parent_proposes_the_deterministic_default_base() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());

        let saved_projects_dir = std::env::var_os("SIRIO_PROJECTS_DIR");
        let saved_xdg = std::env::var_os("XDG_DATA_HOME");

        // No overrides configured: falls back through to $HOME/Sirio/projects,
        // exactly what `sirio_project::default_project_base()` returns.
        unsafe {
            std::env::remove_var("SIRIO_PROJECTS_DIR");
            std::env::remove_var("XDG_DATA_HOME");
        }
        assert_eq!(
            Sidebar::project_form_parent(),
            sirio_project::default_project_base(),
            "with no overrides, the sidebar's proposed parent must equal the deterministic default"
        );

        // A Linux replacement root (SIRIO_PROJECTS_DIR) changes the
        // proposal deterministically, and the sidebar must track it rather
        // than proposing a fixed HOME-derived path of its own.
        let replacement = std::env::temp_dir().join("sirio-f-core-dom-03-replacement-root");
        unsafe {
            std::env::set_var("SIRIO_PROJECTS_DIR", &replacement);
        }
        assert_eq!(
            Sidebar::project_form_parent(),
            replacement,
            "SIRIO_PROJECTS_DIR must be honored as the proposed location"
        );

        unsafe {
            std::env::remove_var("SIRIO_PROJECTS_DIR");
            match saved_projects_dir {
                Some(value) => std::env::set_var("SIRIO_PROJECTS_DIR", value),
                None => std::env::remove_var("SIRIO_PROJECTS_DIR"),
            }
            match saved_xdg {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
    }

    #[test]
    fn sidebar_context_items_explain_git_eligibility_and_list_every_new_surface() {
        let git_project = SidebarContextTarget::Project {
            id: "git".to_string(),
            path: PathBuf::from("/tmp/git"),
            is_git: true,
        };
        let items = Sidebar::context_menu_items(&git_project);
        let initialize = items
            .iter()
            .find(|item| item.action == SidebarContextAction::InitializeGit)
            .expect("Git initialization remains visible as a disabled command");
        assert!(!initialize.enabled);
        assert_eq!(
            initialize.disabled_reason,
            Some(SidebarDisabledReason::AlreadyGitProject)
        );

        let worktree = SidebarContextTarget::Worktree {
            path: PathBuf::from("/tmp/git-main"),
            is_primary: false,
        };
        let worktree_items = Sidebar::context_menu_items(&worktree);
        assert!(
            worktree_items
                .iter()
                .any(|item| { item.action == SidebarContextAction::SetPrimary && item.enabled })
        );
        for action in [
            NewTabAction::NewTerminal,
            NewTabAction::ClaudeCode,
            NewTabAction::Codex,
            NewTabAction::OpenCode,
            NewTabAction::Pi,
            NewTabAction::OhMyPi,
            NewTabAction::NewChat,
        ] {
            assert!(
                worktree_items
                    .iter()
                    .any(|item| item.action == SidebarContextAction::NewTab(action)),
                "context menu lists {action:?}"
            );
        }
    }

    #[gpui::test]
    async fn right_click_context_menu_dispatches_a_typed_worktree_action(
        cx: &mut gpui::TestAppContext,
    ) {
        let repo = scratch_repo("context-menu");
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let row = cx
            .debug_bounds("sidebar-row-1")
            .expect("the worktree row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-context-menu").is_some(),
            "right-click draws the context menu"
        );

        let terminal = cx
            .debug_bounds("sidebar-context-item-new-terminal")
            .expect("the context menu exposes New Terminal");
        cx.simulate_click(terminal.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(events.borrow().iter().any(|event| matches!(
            event,
            SidebarEvent::ContextAction {
                target: SidebarContextTarget::Worktree { .. },
                action: SidebarContextAction::NewTab(NewTabAction::NewTerminal),
            }
        )));
    }

    #[gpui::test]
    async fn drawn_project_context_menu_offers_refresh_project(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("context-menu-refresh-project");
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "refresh-project".into(),
                    name: "refresh-project".into(),
                    is_git: true,
                    root_path: repo.clone(),
                    worktrees: vec![SidebarWorktree {
                        branch: "main".into(),
                        path: repo.clone(),
                        is_primary: true,
                        comment: None,
                    }],
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let row = cx
            .debug_bounds("sidebar-row-0")
            .expect("the project row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-context-item-refresh-project")
                .is_some(),
            "the project context menu exposes Refresh Project"
        );
    }

    #[gpui::test]
    async fn right_click_context_menu_follows_pointer_and_escape_dismisses(
        cx: &mut gpui::TestAppContext,
    ) {
        let repo = scratch_repo("context-menu-pointer");
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let row = cx
            .debug_bounds("sidebar-row-1")
            .expect("the worktree row is drawn");
        let click = point(row.origin.x + px(100.0), row.origin.y + px(10.0));
        cx.simulate_event(MouseDownEvent {
            position: click,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: click,
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let menu = cx
            .debug_bounds("sidebar-context-menu")
            .expect("right-click draws the context menu");
        assert!(
            menu.left() >= click.x - px(8.0) && menu.left() <= click.x + px(8.0),
            "context menu left edge should follow the pointer: menu={:?}, click={click:?}",
            menu
        );
        assert!(
            menu.top() >= click.y - px(8.0) && menu.top() <= click.y + px(8.0),
            "context menu top edge should follow the pointer: menu={:?}, click={click:?}",
            menu
        );
        let context_menu_focus = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx)
                .context_menu_focus
                .clone()
        });
        let focused = cx.update(|window, cx| window.focused(cx));
        assert_eq!(
            focused,
            Some(context_menu_focus),
            "the context menu focus handle owns keyboard focus"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-context-menu").is_none(),
            "Escape closes the context menu"
        );
    }

    /// F-SID-15: the context menu's "Remove Worktree" is confirm-gated the
    /// same way the hover-x button is -- nothing is deleted until the user
    /// answers the prompt, and it routes to a real removal (not just an
    /// event nobody outside sidebar.rs would act on) once they do.
    #[gpui::test]
    async fn right_click_context_menu_remove_worktree_confirms_before_removing(
        cx: &mut gpui::TestAppContext,
    ) {
        // See remove_button_removes_the_worktree's identical comment: widen
        // the git-call timeout so a loaded machine can't turn this into a
        // flake.
        // SAFETY: test process; the only reader is the crate's per-call
        // `SIRIO_GIT_TIMEOUT_MS` lookup.
        unsafe { std::env::set_var("SIRIO_GIT_TIMEOUT_MS", "120000") };
        let repo = scratch_repo("context-menu-remove");
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The repo's sole (primary) worktree can't be git-worktree-removed;
        // create a second one through the prompt, matching
        // remove_button_removes_the_worktree's setup, and remove that one.
        let new_worktree_row = cx.debug_bounds("new-worktree-row").expect("row rendered");
        cx.simulate_click(new_worktree_row.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("to-remove");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let row_id = sidebar_entity
            .read_with(&cx, |sidebar, _| {
                sidebar
                    .rows
                    .iter()
                    .find(|row| row.kind == RowKind::Worktree && row.title == "to-remove")
                    .map(|row| row.id)
            })
            .expect("the new worktree row exists");
        let row_selector: &'static str =
            Box::leak(format!("sidebar-row-{row_id}").into_boxed_str());

        let row = cx
            .debug_bounds(row_selector)
            .expect("the new worktree row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let remove = cx
            .debug_bounds("sidebar-context-item-remove-worktree-context")
            .expect("the context menu exposes Remove Worktree");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.has_pending_prompt(), "removal asks for confirmation");

        assert!(
            sidebar_entity.read_with(&cx, |sidebar, _| sidebar
                .rows
                .iter()
                .any(|row| row.id == row_id)),
            "nothing is removed before the user answers"
        );

        cx.simulate_prompt_answer("Remove Worktree");
        cx.condition(&sidebar_entity, |sidebar, _cx| {
            !sidebar
                .rows
                .iter()
                .any(|row| row.kind == RowKind::Worktree && row.title == "to-remove")
        })
        .await;
    }

    /// #208: a prompt field's text must stay inside the field.
    ///
    /// The row is a flex line holding the text and, after it, the
    /// end-of-text caret. The text was a bare string with no `min_w_0` and
    /// no ellipsis, so it laid out at its natural width and drew straight
    /// past the field's own rounded border onto the popover behind it --
    /// "location (optional, defaults next to project)" spilled its last
    /// word at the default 13pt, and interface font size is a user setting
    /// that goes up from there.
    ///
    /// These placeholders are not fixed strings either: a pinned location
    /// interpolates a filesystem path, so the overflow is bounded only by
    /// how deep that path happens to be.
    ///
    /// Geometry, not text, is the assertion -- it is what the defect was.
    #[gpui::test]
    async fn prompt_field_text_stays_inside_its_field(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("field-overflow");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let row_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        // The longest of the three placeholders, and the one that spilled.
        let field = cx
            .debug_bounds("worktree-prompt-location")
            .expect("the location field is drawn");
        let text = cx
            .debug_bounds("worktree-prompt-location-text")
            .expect("the location field's text is drawn");

        assert!(
            text.right() <= field.right(),
            "the field's text must not draw past the field's own right edge:              text={text:?} field={field:?}"
        );
        assert!(
            text.left() >= field.left(),
            "nor past its left edge: text={text:?} field={field:?}"
        );
    }

    #[gpui::test]
    async fn new_worktree_prompt_creates_a_real_worktree(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("create");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The New Worktree row is offered for the git project.
        let row_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        // The branch-name prompt opens.
        assert!(
            cx.debug_bounds("worktree-prompt").is_some(),
            "clicking New Worktree opens the prompt"
        );

        // Type the branch (a slash, exercising the nested-directory case)
        // and confirm with Enter.
        cx.simulate_input("feature/login");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The worktree row appears in the tree without a refresh.
        let created = cx
            .update(|window, cx| {
                let sidebar = window
                    .root::<Sidebar>()
                    .flatten()
                    .expect("sidebar root")
                    .read(cx);
                sidebar
                    .rows
                    .iter()
                    .find(|row| row.kind == RowKind::Worktree && row.title == "feature/login")
                    .cloned()
            })
            .expect("the new worktree row was inserted");
        assert!(created.selected, "the new row is selected");

        // And the real repository agrees: porcelain reports the worktree on
        // the new branch, at the derived path.
        let derived = derive_worktree_path(
            &resolve_parent_directory(&repo, None),
            "sirio",
            "feature/login",
        );
        let porcelain = porcelain(&repo);
        assert!(
            porcelain.contains(&format!("worktree {}", derived.display())),
            "porcelain reports the created worktree at the derived path:\n{porcelain}"
        );
        assert!(
            porcelain.contains("branch refs/heads/feature/login"),
            "porcelain reports the new branch:\n{porcelain}"
        );
    }

    /// F-CORE-DOM-02: a project's pinned "Default Worktree Base" and
    /// "Worktree Location" (pushed in the same way a live host applies them
    /// after a restore — via `set_project_worktree_defaults`, exactly what
    /// `refresh_sidebar` does) must be honoured by "New Worktree…" when its
    /// own one-off Base/Location fields are left blank, mirroring the Swift
    /// app's `WorktreeDefaults.resolveBase`/`resolveParentDirectory`
    /// (explicit per-project override wins; there is no separate per-dialog
    /// field in Swift at all). Before this fix, `begin_worktree_prompt`
    /// never read `project_worktree_defaults`, so the pin had zero effect on
    /// worktree creation: leaving the dialog fields blank always cut the new
    /// branch from HEAD and placed it next to the project, no matter what
    /// was pinned in Project Settings.
    #[gpui::test]
    async fn new_worktree_honours_the_pinned_base_and_location_when_the_dialog_is_left_blank(
        cx: &mut gpui::TestAppContext,
    ) {
        let repo = scratch_repo("pin-base");
        // A second branch carrying a marker file `main` never gets, so the
        // created worktree's contents prove which branch it was actually
        // cut from.
        assert!(
            Command::new("git")
                .args(["checkout", "-b", "release"])
                .current_dir(&repo)
                .status()
                .expect("git")
                .success()
        );
        std::fs::write(repo.join("release-marker.txt"), "release\n").expect("write marker");
        for args in [
            vec!["add", "-A"],
            vec!["commit", "-m", "release marker"],
            vec!["checkout", "main"],
        ] {
            assert!(
                Command::new("git")
                    .args(&args)
                    .current_dir(&repo)
                    .status()
                    .expect("git")
                    .success()
            );
        }

        let location_dir = repo
            .parent()
            .expect("scratch repo has a parent directory")
            .join("pinned-location");
        std::fs::create_dir_all(&location_dir).expect("create pinned location dir");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "pin-project".into(),
                    name: "pin-project".into(),
                    is_git: true,
                    root_path: repo.clone(),
                    worktrees: vec![SidebarWorktree {
                        branch: "main".into(),
                        path: repo.clone(),
                        is_primary: true,
                        comment: None,
                    }],
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        // Simulate the host re-applying the persisted pin, the same call
        // `refresh_sidebar` makes after a restore or a settings save.
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.set_project_worktree_defaults(
                    "pin-project",
                    Some("release".to_string()),
                    Some(location_dir.to_string_lossy().into_owned()),
                    cx,
                );
            });
        });
        cx.run_until_parked();

        let row_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        // Type only the branch name; leave the dialog's own Base and
        // Location fields untouched (its placeholder describes them as
        // optional).
        cx.simulate_input("cleanbase1");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        let expected_path = location_dir.join("pin-project-cleanbase1");
        assert!(
            expected_path.is_dir(),
            "the pinned Worktree Location override placed the new worktree at {} \
             instead of the project's sibling directory",
            expected_path.display()
        );
        assert!(
            expected_path.join("release-marker.txt").is_file(),
            "the new worktree was cut from the pinned Default Worktree Base \
             ('release'), not from HEAD -- the release-only marker file must \
             be present at {}",
            expected_path.display()
        );
    }

    #[gpui::test]
    async fn remove_button_removes_the_worktree(cx: &mut gpui::TestAppContext) {
        // The sidebar's create/remove go through `sirio_git`, whose runner
        // bounds every git invocation with a 10 s deadline so a hung git can
        // never freeze the UI. On a loaded machine (parallel compiles, high
        // load average) a single `git worktree` call on a fixture repo can
        // legitimately exceed that budget, which turns a healthy test into a
        // flake. Widen the budget for this process — the production default
        // is untouched; the override only applies while this variable is set.
        // SAFETY: test process; the only reader is the crate's per-call
        // `SIRIO_GIT_TIMEOUT_MS` lookup, and a wider budget can only turn a
        // would-be timeout into a pass.
        unsafe { std::env::set_var("SIRIO_GIT_TIMEOUT_MS", "120000") };
        let repo = scratch_repo("remove");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Create the worktree through the prompt so its row exists.
        let row_bounds = cx.debug_bounds("new-worktree-row").expect("row rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("to-remove");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        // The remove button sits at the row's right edge, on its title
        // line. Locate the created worktree's own row and click against
        // *its* bounds rather than deriving a point from a neighbour plus a
        // constant row height: #151 made a row's height depend on whether it
        // has a sub-line at all, so any hardcoded offset here silently rots
        // the next time that changes — which is exactly how this test broke.
        let new_worktree_bounds = cx
            .debug_bounds("new-worktree-row")
            .expect("the New Worktree row's bounds are known");
        let created_row = (0..64)
            .filter_map(|row_id| {
                let selector: &'static str =
                    Box::leak(format!("sidebar-row-{row_id}").into_boxed_str());
                cx.debug_bounds(selector)
            })
            .filter(|bounds| bounds.origin.y < new_worktree_bounds.origin.y)
            .max_by(|a, b| a.origin.y.partial_cmp(&b.origin.y).expect("finite y"))
            .expect("the created worktree's row is drawn above the New Worktree row");
        // The × is 16px wide, inset 8px from the row's right edge.
        let remove_button = point(
            created_row.origin.x + created_row.size.width - px(16.0),
            created_row.center().y,
        );
        // The remove button is hover-revealed: move the mouse over the row
        // first so the × is visible and clickable.
        cx.simulate_mouse_move(remove_button, None, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(remove_button, Modifiers::none());
        cx.run_until_parked();

        // F-SID-15: the hover-x button is confirm-gated now instead of
        // deleting the on-disk worktree immediately on click.
        assert!(cx.has_pending_prompt(), "removal asks for confirmation");
        cx.simulate_prompt_answer("Remove Worktree");

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.condition(&sidebar_entity, |sidebar, _cx| {
            !sidebar
                .rows
                .iter()
                .any(|row| row.kind == RowKind::Worktree && row.title == "to-remove")
        })
        .await;

        // The row is gone from the tree and the repository agrees.
        let remaining = cx.update(|window, cx| {
            let sidebar = window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx);
            sidebar
                .rows
                .iter()
                .filter(|row| row.kind == RowKind::Worktree)
                .map(|row| row.title.clone())
                .collect::<Vec<_>>()
        });
        assert!(
            !remaining.iter().any(|title| title == "to-remove"),
            "the removed worktree row is gone, remaining: {remaining:?}"
        );
        let porcelain = porcelain(&repo);
        assert!(
            !porcelain.contains("to-remove"),
            "porcelain no longer reports the removed worktree:\n{porcelain}"
        );
    }

    /// The shell lets the panel be dragged across its whole range, and the
    /// sidebar did not notice: every row was laid out against a fixed 325
    /// and the panel's `overflow_hidden` cut off the difference. At the
    /// floor of the day — 160, before the rows themselves set it — that read
    /// as a worktree name ending mid-word with no ellipsis: the text was not
    /// overflowing its row, the row was overflowing the panel.
    #[gpui::test]
    async fn rows_follow_the_panel_width(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("panel-width");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo)));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        let wide = cx
            .debug_bounds("sidebar-row-0")
            .expect("the first project row")
            .size
            .width;

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.set_panel_width(DEFAULT_SIDEBAR_WIDTH - 125.0, cx);
            });
        });
        cx.run_until_parked();

        let narrow = cx
            .debug_bounds("sidebar-row-0")
            .expect("the first project row")
            .size
            .width;

        assert_eq!(
            f32::from(wide) - f32::from(narrow),
            125.0,
            "the row must give back exactly what the panel took"
        );
    }

    #[gpui::test]
    async fn clicking_a_worktree_row_reports_selection_to_the_host(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("select");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // The fixture's worktree rows carry no path; give the first one a
        // real checkout path so the click has something to report.
        let worktree_path = repo.join("sirio-main");
        cx.update(|_, cx| {
            sidebar_entity.update(cx, |sidebar, cx| {
                if let Some(row) = sidebar
                    .rows
                    .iter_mut()
                    .find(|row| row.kind == RowKind::Worktree)
                {
                    row.path = Some(worktree_path.clone());
                }
                cx.notify();
            })
        });
        let row_bounds = cx
            .debug_bounds("sidebar-row-1")
            .expect("the first worktree row is rendered");
        cx.simulate_click(row_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectWorktree(path) if *path == worktree_path)),
            "clicking a worktree row must emit SelectWorktree with its path, got {emitted:?}"
        );

        // The host answers by confirming the selection; the highlight agrees.
        sidebar_entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_selected_worktree(&worktree_path, cx);
        });
        let selected = cx.read(|cx| {
            sidebar_entity
                .read(cx)
                .rows
                .iter()
                .filter(|r| r.selected)
                .count()
        });
        assert_eq!(selected, 1, "exactly one row stays selected");
    }

    /// F-PRJ-03: picking a non-git folder through Open Project prompts
    /// before adding it. "Initialize Git" must leave a real `.git` behind
    /// (not just claim to) and then still emit AddProject.
    #[gpui::test]
    async fn open_project_initialize_git_creates_a_real_repo_then_adds(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let picked = std::env::temp_dir().join(format!(
            "sirio-sidebar-test-init-git-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&picked);
        std::fs::create_dir_all(&picked).expect("create the plain (non-git) folder");
        assert!(
            !picked.join(".git").exists(),
            "the fixture folder starts with no .git"
        );

        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let events = Rc::new(RefCell::new(Vec::<SidebarEvent>::new()));
        let collected = events.clone();
        let entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            cx.subscribe(&entity, move |_, event: &SidebarEvent, _cx| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let plus_bounds = cx
            .debug_bounds("add-project")
            .expect("the + add-project control is rendered");
        cx.simulate_click(plus_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        let open = cx
            .debug_bounds("add-project-open")
            .expect("open project choice");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_path_prompt_response(|_| Some(vec![picked.clone()]));
        cx.run_until_parked();

        cx.simulate_prompt_answer("Initialize Git");
        cx.run_until_parked();

        assert!(
            picked.join(".git").is_dir(),
            "Initialize Git must leave a real .git behind, not just claim to"
        );
        let emitted = events.borrow();
        assert!(
            emitted.iter().any(|event| matches!(
                event,
                SidebarEvent::AddProject(path) if *path == picked
            )),
            "after initializing git the folder is still added, got {emitted:?}"
        );

        let _ = std::fs::remove_dir_all(&picked);
    }

    #[gpui::test]
    async fn add_project_menu_open_choice_reports_the_chosen_directory(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let picked = std::env::temp_dir().join("sirio-picked-project");
        let plus_bounds = cx
            .debug_bounds("add-project")
            .expect("the + add-project control is rendered");
        cx.simulate_click(plus_bounds.center(), Modifiers::none());
        cx.run_until_parked();

        for selector in [
            "add-project-open",
            "add-project-clone",
            "add-project-create",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "the + menu offers {selector}"
            );
        }
        let open = cx
            .debug_bounds("add-project-open")
            .expect("open project choice");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.did_prompt_for_paths(),
            "Open Project must open the platform folder picker"
        );

        // The picker is a directory chooser, not a file chooser.
        cx.simulate_path_prompt_response(|options| {
            assert!(
                options.directories && !options.files && !options.multiple,
                "the add-project picker asks for one directory"
            );
            Some(vec![picked.clone()])
        });
        cx.run_until_parked();

        // F-PRJ-03: the picked directory has no `.git`, so it gets the
        // confirmation prompt before being added — answer "Add without Git"
        // to reach the same AddProject outcome this test asserts.
        cx.simulate_prompt_answer("Add without Git");
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted.iter().any(|event| matches!(
                event,
                SidebarEvent::AddProject(path) if *path == picked
            )),
            "the chosen directory must be emitted as AddProject, got {emitted:?}"
        );
    }

    #[gpui::test]
    async fn add_project_menu_open_cancel_is_silent(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        let plus_bounds = cx
            .debug_bounds("add-project")
            .expect("the + add-project control is rendered");
        cx.simulate_click(plus_bounds.center(), Modifiers::none());
        cx.run_until_parked();
        let open = cx
            .debug_bounds("add-project-open")
            .expect("open project choice");
        cx.simulate_click(open.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.did_prompt_for_paths(),
            "Open Project opens the platform folder picker"
        );

        // The user cancels: the platform answers None and nothing happens.
        cx.simulate_path_prompt_response(|_options| None);
        cx.run_until_parked();

        assert!(
            events.borrow().is_empty(),
            "cancelling the picker must emit nothing, got {:?}",
            events.borrow()
        );
    }

    #[gpui::test]
    async fn dragging_project_rows_reorders_the_live_sidebar_block(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let projects = vec![
            SidebarProject {
                id: "first".into(),
                name: "First".into(),
                is_git: false,
                root_path: PathBuf::from("/repo/first"),
                worktrees: vec![SidebarWorktree {
                    branch: "main".into(),
                    path: PathBuf::from("/repo/first"),
                    is_primary: true,
                    comment: None,
                }],
            },
            SidebarProject {
                id: "second".into(),
                name: "Second".into(),
                is_git: false,
                root_path: PathBuf::from("/repo/second"),
                worktrees: vec![SidebarWorktree {
                    branch: "main".into(),
                    path: PathBuf::from("/repo/second"),
                    is_primary: true,
                    comment: None,
                }],
            },
        ];
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(projects, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let row_ids = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                window
                    .root::<Sidebar>()
                    .flatten()
                    .expect("sidebar root")
                    .read(cx)
                    .rows
                    .iter()
                    .map(|row| row.id)
                    .collect::<Vec<_>>()
            })
        };
        let first = cx.debug_bounds("sidebar-row-0").expect("first project row");
        let second = cx
            .debug_bounds("sidebar-row-1000")
            .expect("second project row");
        cx.simulate_event(MouseDownEvent {
            position: first.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(first.center().x + px(30.0), first.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseMoveEvent {
            position: second.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: second.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let after = row_ids(&mut cx);
        assert_eq!(after.first().copied(), Some(1000));
        assert_eq!(after.get(2).copied(), Some(0));
    }

    /// F-CORE-ACT-17 + F-CORE-ACT-18, drawn: the three facts a worktree row
    /// has to carry read as three separate things, the way the Swift
    /// original divides them.
    ///
    /// * The **branch glyph** stays put whatever the agents are doing —
    ///   `App/SidebarView.swift:361` draws `arrow.triangle.branch` beside
    ///   the branch name unconditionally. The port had been replacing it
    ///   with the agent's brand mark, which erased the git-ness of the row
    ///   and stated the agent twice.
    /// * The **status indicator** is the only thing the agent identity
    ///   touches, and only as a tint — `WorktreeStatusGlyph(status:agentId:)`
    ///   passes `agentId` to nothing but `RunningDots(color:)`.
    /// * The **trailing badge** is the one place a brand mark appears, one
    ///   per running agent, in the order handed over (catalog order).
    #[gpui::test]
    async fn drawn_worktree_row_keeps_its_branch_glyph_and_tints_one_status_indicator(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-worktree-mark-1-git-branch")
                .is_some(),
            "an agent-less worktree row draws the branch glyph"
        );
        assert!(
            cx.debug_bounds("sidebar-running-agents-1").is_none(),
            "no running agents means no trailing badge at all"
        );

        let entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_activity(
                1,
                Some(ActivityStatus::Running),
                Some(AgentBrandColor::Claude),
                vec![
                    AgentMark {
                        icon: Icon::ClaudeCode,
                        brand: AgentBrandColor::Claude,
                    },
                    AgentMark {
                        icon: Icon::Codex,
                        brand: AgentBrandColor::Codex,
                    },
                ],
                cx,
            );
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-worktree-mark-1-git-branch")
                .is_some(),
            "the branch glyph is a property of the row, not of the agents in it"
        );
        assert!(
            cx.debug_bounds("sidebar-worktree-mark-1-claude-mark")
                .is_none(),
            "the agent's brand mark never takes the branch glyph's place"
        );
        assert!(
            cx.debug_bounds("sidebar-status-running-1").is_some(),
            "a running worktree draws the running indicator, not nothing"
        );
        assert!(cx.debug_bounds("sidebar-running-agents-1").is_some());
        let claude = cx
            .debug_bounds("sidebar-running-agent-1-claude-mark")
            .expect("claude is badged as running");
        let codex = cx
            .debug_bounds("sidebar-running-agent-1-openai-mark")
            .expect("codex is badged as running");
        assert!(
            claude.origin.x < codex.origin.x,
            "badge marks are drawn in the catalog order they were handed over"
        );

        // The badge is strictly the `.running` set: a worktree that goes
        // quiet loses it, and the running indicator gives way to a dot.
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_activity(1, Some(ActivityStatus::Done), None, Vec::new(), cx);
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("sidebar-running-agents-1").is_none());
        assert!(cx.debug_bounds("sidebar-status-running-1").is_none());
        assert!(cx.debug_bounds("sidebar-status-dot-1").is_some());
        assert!(
            cx.debug_bounds("sidebar-worktree-mark-1-git-branch")
                .is_some()
        );
    }

    /// A row card paints its hover and selection fill across its whole box,
    /// so adjacent cards that touch read as one merged highlight block (the
    /// selected worktree's fill ran straight into the next row's hover
    /// fill). The tree must leave a visible seam between row boxes.
    #[gpui::test]
    async fn adjacent_row_highlight_boxes_do_not_touch(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let above = cx
            .debug_bounds("sidebar-row-1")
            .expect("the selected worktree row is drawn");
        let below = cx
            .debug_bounds("sidebar-row-2")
            .expect("its tab row is drawn");
        let gap = below.origin.y - (above.origin.y + above.size.height);
        assert!(
            gap >= px(ROW_V_GAP),
            "adjacent row highlight boxes must be separated by {ROW_V_GAP}px, got {gap:.1}px"
        );
    }

    /// The seam this port had one level down from the worktree row: a pane
    /// whose agent is identified **after** it started must change the *tab*
    /// row's mark, not only the worktree row's.
    ///
    /// The reference's `WorkspaceTabIcon` reads
    /// `model.agentActivity.paneAgents[paneId]` at render time, so a Claude
    /// the user launched by hand in a plain terminal — identified by Layer B
    /// from its OSC title, or by Layer D from its process name, which is how
    /// every agent Sirio did not spawn gets identified — shows its brand
    /// mark as soon as it is known. This port took the mark from a field
    /// fixed at spawn, so those tab rows kept the generic terminal glyph
    /// forever while the worktree row above them already showed the brand.
    #[gpui::test]
    async fn drawn_tab_row_takes_the_mark_an_agent_earns_after_spawn(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let plain_terminal = || SidebarTab {
            tab: SidebarTabRef::Open(7),
            title: "Terminal".into(),
            selected: false,
            kind: TabKind::Terminal,
            agent: None,
        };
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, vec![plain_terminal()], cx);
        });
        cx.run_until_parked();

        // `TAB_ROW_ID_OFFSET + 7`, spelled out: `debug_bounds` wants a
        // `&'static str`, and a leaked format! per assertion reads worse
        // than the two constants this test actually needs.
        assert_eq!(TAB_ROW_ID_OFFSET + 7, 1_000_007);
        const GENERIC: &str = "sidebar-tab-mark-1000007-terminal";
        const CLAUDE: &str = "sidebar-tab-mark-1000007-claude-mark";

        assert!(
            cx.debug_bounds(GENERIC).is_some(),
            "an unidentified terminal keeps the generic surface glyph"
        );
        assert!(cx.debug_bounds(CLAUDE).is_none(), "and nothing else");

        // Layer B lands. Everything else about the tab is unchanged — same
        // id, same kind, same title, same selection — which is exactly the
        // case a diff that ignored the mark would swallow.
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                1,
                vec![SidebarTab {
                    agent: AgentMark::for_agent_id("claude"),
                    ..plain_terminal()
                }],
                cx,
            );
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds(CLAUDE).is_some(),
            "an agent identified after spawn changes the tab row's mark on screen"
        );
        assert!(
            cx.debug_bounds(GENERIC).is_none(),
            "the generic glyph gives way rather than being drawn alongside"
        );

        // ...and it goes away again when the identity does, so this is a
        // live read and not a one-way latch.
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(1, vec![plain_terminal()], cx);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds(GENERIC).is_some(),
            "losing the identity restores the surface glyph"
        );
    }

    /// The two collisions a user would have to measure pixels to resolve.
    ///
    /// 1. A **running** Claude worktree resolved its tint through the
    ///    eight-token settings palette, where Claude was `Amber` — that is
    ///    `theme.warning` itself. Running and needs-input painted the
    ///    same `#E0B36A`, leaving a 3x3 dot cluster versus a 6x6 dot as the
    ///    only difference. The reference has no such collision: needs-input
    ///    is `.dot(.amber)` and Claude-running is `RunningDots` in Claude's
    ///    own colour.
    /// 2. Every badge mark was tinted `theme.text`, a coral near
    ///    enough to Claude's brand to read as it, so a Codex or Pi mark was
    ///    drawn in Claude's colour.
    #[test]
    fn running_tint_never_equals_a_status_colour_and_names_the_agent() {
        for theme in [Theme::dark(), Theme::light()] {
            let needs_input =
                RowStatusGlyph::for_status(Some(ActivityStatus::NeedsInput), None, theme);
            for (agent, brand) in [
                ("claude", AgentBrandColor::Claude),
                ("codex", AgentBrandColor::Codex),
                ("opencode", AgentBrandColor::OpenCode),
                ("pi", AgentBrandColor::Pi),
                ("omp", AgentBrandColor::Omp),
            ] {
                let running =
                    RowStatusGlyph::for_status(Some(ActivityStatus::Running), Some(brand), theme);
                assert_eq!(running, RowStatusGlyph::Running(brand.color()));
                assert_ne!(
                    running,
                    RowStatusGlyph::Running(match needs_input {
                        RowStatusGlyph::Dot(color) => color,
                        other => panic!("needs-input must be a dot, got {other:?}"),
                    }),
                    "{agent} running must not paint the needs-input colour"
                );
            }
        }
    }

    /// The third face of the same collision, and the one that outlived the
    /// first fix: a tab row with **no** agent.
    ///
    /// `running_tint_never_equals_a_status_colour_and_names_the_agent` pins
    /// the branded half. The unbranded half fell back to `tab_needs_input`,
    /// so a plain terminal row and an agent waiting on an answer were the
    /// same amber — the very thing that test exists to forbid, one branch
    /// over.
    #[test]
    fn a_tab_row_without_an_agent_never_borrows_the_needs_input_amber() {
        for theme in [Theme::dark(), Theme::light()] {
            let plain = Sidebar::tab_row_icon_color(None, false, theme);
            assert_eq!(
                plain, theme.text_faint,
                "a tab with no agent takes the row grey"
            );
            assert_ne!(
                plain, theme.warning,
                "an idle tab must not wear the colour of one waiting on an answer"
            );

            // A brand only reaches the tint when there is a mark to draw it on.
            assert_eq!(
                Sidebar::tab_row_icon_color(Some(AgentBrandColor::Codex), true, theme),
                AgentBrandColor::Codex.color()
            );
            assert_eq!(
                Sidebar::tab_row_icon_color(Some(AgentBrandColor::Codex), false, theme),
                theme.text_faint,
                "a brand with no mark to paint falls back like any other tab"
            );
        }
    }

    /// A worktree whose agent is unknown still gets a running indicator, in
    /// the neutral grey `AgentIcon.color(for: agentId ?? "")` resolves to —
    /// never the brand accent, which would name an agent nobody identified.
    #[test]
    fn an_unidentified_running_agent_gets_the_neutral_fallback() {
        let theme = Theme::dark();
        assert_eq!(
            RowStatusGlyph::for_status(Some(ActivityStatus::Running), None, theme),
            RowStatusGlyph::Running(AgentBrandColor::Unknown.color())
        );
    }

    /// The status table itself, against `SidebarGlyphKind.forStatus` in
    /// `Packages/TillerCore/Sources/TillerCore/SidebarGlyph.swift`. Two rows
    /// of it had been inverted: `Idle` drew the amber needs-input dot, so an
    /// idle worktree was pixel-identical to one waiting on an answer, and
    /// `Running` drew nothing, so a busy worktree looked empty.
    #[test]
    fn status_glyph_table_matches_the_swift_original() {
        let theme = Theme::light();
        assert_eq!(
            RowStatusGlyph::for_status(None, None, theme),
            RowStatusGlyph::None
        );
        assert_eq!(
            RowStatusGlyph::for_status(Some(ActivityStatus::Idle), None, theme),
            RowStatusGlyph::None,
            "nil status draws no glyph -- and Idle is the Rust name for it"
        );
        assert_eq!(
            RowStatusGlyph::for_status(Some(ActivityStatus::NeedsInput), None, theme),
            RowStatusGlyph::Dot(theme.warning)
        );
        assert_eq!(
            RowStatusGlyph::for_status(Some(ActivityStatus::Done), None, theme),
            RowStatusGlyph::Dot(theme.success)
        );
        assert_eq!(
            RowStatusGlyph::for_status(Some(ActivityStatus::Error), None, theme),
            RowStatusGlyph::Dot(theme.danger)
        );
        // Running is a different *shape*, and the agent id reaches the row
        // only as its tint.
        assert_eq!(
            RowStatusGlyph::for_status(
                Some(ActivityStatus::Running),
                Some(AgentBrandColor::Codex),
                theme
            ),
            RowStatusGlyph::Running(AgentBrandColor::Codex.color())
        );
        assert_ne!(
            RowStatusGlyph::for_status(Some(ActivityStatus::Idle), None, theme),
            RowStatusGlyph::for_status(Some(ActivityStatus::NeedsInput), None, theme),
            "an idle worktree must not look like one that needs input"
        );
    }

    /// F-CORE-ACT-22, drawn: the host's urgency order actually moves the
    /// rows on screen, and a worktree takes its own tab rows with it rather
    /// than leaving them orphaned under whatever row lands in its place.
    #[gpui::test]
    async fn drawn_worktree_order_moves_a_row_with_its_tab_rows(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let project = SidebarProject {
            id: "project".into(),
            name: "Project".into(),
            is_git: true,
            root_path: PathBuf::from("/repo/project"),
            worktrees: (0..3)
                .map(|index| SidebarWorktree {
                    branch: format!("branch-{index}"),
                    path: PathBuf::from(format!("/repo/project-{index}")),
                    is_primary: index == 0,
                    comment: None,
                })
                .collect(),
        };
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(vec![project], cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                3,
                vec![SidebarTab {
                    tab: SidebarTabRef::Open(7),
                    title: "Claude Code".into(),
                    selected: true,
                    kind: TabKind::Terminal,
                    agent: AgentMark::for_agent_id("claude"),
                }],
                cx,
            );
        });
        cx.run_until_parked();

        let before = cx
            .debug_bounds("sidebar-row-3")
            .expect("third worktree row");
        let first_before = cx
            .debug_bounds("sidebar-row-1")
            .expect("first worktree row");
        assert!(before.origin.y > first_before.origin.y);

        let moved = entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_order(0, &[3, 1, 2], cx)
        });
        cx.run_until_parked();
        assert!(moved, "a genuine permutation reorders the rows");

        let urgent = cx
            .debug_bounds("sidebar-row-3")
            .expect("third worktree row");
        let first = cx
            .debug_bounds("sidebar-row-1")
            .expect("first worktree row");
        let second = cx
            .debug_bounds("sidebar-row-2")
            .expect("second worktree row");
        let tab_selector: &'static str =
            Box::leak(format!("sidebar-row-{}", TAB_ROW_ID_OFFSET + 7).into_boxed_str());
        let tab = cx
            .debug_bounds(tab_selector)
            .expect("the moved worktree's tab row");
        assert!(
            urgent.origin.y < first.origin.y && urgent.origin.y < second.origin.y,
            "the urgent worktree is drawn above both siblings"
        );
        assert!(
            first.origin.y < second.origin.y,
            "the siblings keep their manual order relative to each other"
        );
        assert!(
            tab.origin.y > urgent.origin.y && tab.origin.y < first.origin.y,
            "the worktree's tab row travelled with it"
        );
        assert!(
            cx.debug_bounds("new-worktree-row")
                .expect("the New Worktree affordance stays drawn")
                .origin
                .y
                > second.origin.y,
            "the New Worktree affordance stays at the end of the project"
        );

        // Idempotent: pushing the same order again changes nothing, which is
        // what makes this safe on every frame.
        let again = entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_order(0, &[3, 1, 2], cx)
        });
        assert!(!again, "re-pushing the standing order is a no-op");
        // A stale or foreign order is rejected rather than half-applied.
        let stale = entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_order(0, &[3, 1, 2, 99], cx)
        });
        assert!(
            !stale,
            "an order that is not a permutation of the rows is ignored"
        );
    }

    #[gpui::test]
    async fn dragging_worktree_rows_reorders_only_their_project_group(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let project = SidebarProject {
            id: "project".into(),
            name: "Project".into(),
            is_git: false,
            root_path: PathBuf::from("/repo/project"),
            worktrees: (0..3)
                .map(|index| SidebarWorktree {
                    branch: format!("branch-{index}"),
                    path: PathBuf::from(format!("/repo/project-{index}")),
                    is_primary: index == 0,
                    comment: None,
                })
                .collect(),
        };
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(vec![project], cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let source = cx
            .debug_bounds("sidebar-row-1")
            .expect("first worktree row");
        let target = cx
            .debug_bounds("sidebar-row-3")
            .expect("third worktree row");
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(30.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let worktrees = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx)
                .rows
                .iter()
                .filter(|row| row.kind == RowKind::Worktree)
                .map(|row| row.title.clone())
                .collect::<Vec<_>>()
        });
        assert_eq!(worktrees, vec!["branch-1", "branch-2", "branch-0"]);
    }

    #[gpui::test]
    async fn dragging_worktree_onto_last_sibling_lands_before_new_worktree_row(
        cx: &mut gpui::TestAppContext,
    ) {
        // F-SID-17: a real git project always carries a trailing
        // `RowKind::NewWorktree` affordance row after its last worktree.
        // Dropping a dragged worktree row onto the last sibling in its
        // group used to fall back to `self.rows.len()`, which lands the
        // row *after* that affordance instead of after its new sibling.
        cx.update(Theme::init);
        let project = SidebarProject {
            id: "project".into(),
            name: "Project".into(),
            is_git: true,
            root_path: PathBuf::from("/repo/project"),
            worktrees: (0..2)
                .map(|index| SidebarWorktree {
                    branch: format!("branch-{index}"),
                    path: PathBuf::from(format!("/repo/project-{index}")),
                    is_primary: index == 0,
                    comment: None,
                })
                .collect(),
        };
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(vec![project], cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let source = cx
            .debug_bounds("sidebar-row-1")
            .expect("first worktree row");
        let target = cx
            .debug_bounds("sidebar-row-2")
            .expect("second worktree row");
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(30.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let kinds_and_titles = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx)
                .rows
                .iter()
                .map(|row| (row.kind, row.title.clone()))
                .collect::<Vec<_>>()
        });
        // branch-0 must land right after branch-1, and the New Worktree
        // affordance must stay last in the project's block — not have
        // branch-0 dumped after it.
        assert_eq!(
            kinds_and_titles,
            vec![
                (RowKind::Project, "Project".to_string()),
                (RowKind::Worktree, "branch-1".to_string()),
                (RowKind::Worktree, "branch-0".to_string()),
                (RowKind::NewWorktree, "New Worktree...".to_string()),
            ]
        );
    }

    #[gpui::test]
    async fn dragging_tab_rows_reorders_only_their_worktree_group(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let project = SidebarProject {
            id: "project".into(),
            name: "Project".into(),
            is_git: false,
            root_path: PathBuf::from("/repo/project"),
            worktrees: vec![SidebarWorktree {
                branch: "main".into(),
                path: PathBuf::from("/repo/project"),
                is_primary: true,
                comment: None,
            }],
        };
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(vec![project], cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        sidebar.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                1,
                vec![
                    SidebarTab {
                        tab: SidebarTabRef::Open(42),
                        title: "First".into(),
                        selected: true,
                        kind: TabKind::Terminal,
                        agent: None,
                    },
                    SidebarTab {
                        tab: SidebarTabRef::Open(43),
                        title: "Second".into(),
                        selected: false,
                        kind: TabKind::Terminal,
                        agent: None,
                    },
                ],
                cx,
            );
        });
        cx.run_until_parked();

        let source_selector: &'static str =
            Box::leak(format!("sidebar-row-{}", TAB_ROW_ID_OFFSET + 42).into_boxed_str());
        let target_selector: &'static str =
            Box::leak(format!("sidebar-row-{}", TAB_ROW_ID_OFFSET + 43).into_boxed_str());
        let source = cx.debug_bounds(&source_selector).expect("first tab row");
        let target = cx.debug_bounds(&target_selector).expect("second tab row");
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: point(source.center().x + px(30.0), source.center().y),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseMoveEvent {
            position: target.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        cx.simulate_event(MouseUpEvent {
            position: target.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();

        let tab_ids = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx)
                .rows
                .iter()
                .filter_map(|row| row.tab_id)
                .collect::<Vec<_>>()
        });
        assert_eq!(tab_ids, vec![43, 42]);
    }

    /// F-TAB-15: a host-owned tab row's close control is drawn, hover-
    /// revealed, and clicking it reports CloseTab with the real tab id —
    /// the tab strip's ✕, exercised from the sidebar's view of the same
    /// tabs the strip renders.
    #[gpui::test]
    async fn the_drawn_tab_close_control_reports_closeta_tab(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("tab-close");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, Some(repo.clone())));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // The host pushes one open tab under the worktree row (id 1).
        let tab_id = 42usize;
        sidebar_entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_tabs(
                1,
                vec![SidebarTab {
                    tab: SidebarTabRef::Open(tab_id),
                    title: "Chat".into(),
                    selected: true,
                    kind: TabKind::AgentChat,
                    agent: None,
                }],
                cx,
            );
        });
        cx.run_until_parked();
        let row_id = TAB_ROW_ID_OFFSET + tab_id;

        // The close control is hover-revealed: move over the row, then the
        // ✕ is visible and clickable at its own drawn bounds.
        // `debug_bounds` takes a static selector; the row ids are dynamic,
        // so leak one string per lookup — a bounded, test-only cost.
        let row_selector: &'static str =
            Box::leak(format!("sidebar-row-{row_id}").into_boxed_str());
        let close_selector: &'static str =
            Box::leak(format!("sidebar-tab-close-{row_id}").into_boxed_str());
        let row_bounds = cx
            .debug_bounds(row_selector)
            .expect("the host-driven tab row is drawn");
        cx.simulate_mouse_move(row_bounds.center(), None, Modifiers::none());
        cx.run_until_parked();
        let close = cx
            .debug_bounds(close_selector)
            .expect("the tab close control is drawn after hovering the row");
        cx.simulate_click(close.center(), Modifiers::none());
        cx.run_until_parked();

        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::CloseTab(id) if *id == tab_id)),
            "clicking the drawn ✕ must emit CloseTab for the real tab id, got {emitted:?}"
        );
        assert!(
            !emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectTab(_))),
            "the close control must not also select the tab"
        );
    }

    /// F-SID-01: the sidebar draws its Projects header's Add Project control
    /// and every project row, and the control is a real control — clicking it
    /// opens the three-choice project menu.
    #[gpui::test]
    async fn projects_header_add_menu_and_project_rows_render(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The header's + control is drawn.
        let plus = cx
            .debug_bounds("add-project")
            .expect("the + Add Project control is drawn");

        // All four fixture project rows are drawn (ids 0, 4, 7, 8).
        for id in [0, 4, 7, 8] {
            let selector: &'static str = Box::leak(format!("sidebar-row-{id}").into_boxed_str());
            assert!(
                cx.debug_bounds(selector).is_some(),
                "project row {id} is drawn"
            );
        }

        // And it dispatches the real event: the picker opens.
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("add-project-open").is_some()
                && cx.debug_bounds("add-project-clone").is_some()
                && cx.debug_bounds("add-project-create").is_some(),
            "clicking + must open the three-choice project menu"
        );
    }

    #[gpui::test]
    async fn project_menu_mounts_clone_and_create_forms(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("add-project").expect("add project control");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let clone = cx
            .debug_bounds("add-project-clone")
            .expect("clone project choice");
        cx.simulate_click(clone.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("clone-url-field").is_some(),
            "clone form is mounted"
        );

        let cancel = cx
            .debug_bounds("close-project-form")
            .expect("project form cancel");
        cx.simulate_click(cancel.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let create = cx
            .debug_bounds("add-project-create")
            .expect("create project choice");
        cx.simulate_click(create.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("create-name-field").is_some(),
            "create form is mounted"
        );
    }

    #[gpui::test]
    async fn escape_closes_project_settings_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "Project".into(),
                    is_git: false,
                    root_path: PathBuf::from("/tmp/project"),
                    worktrees: Vec::new(),
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.open_project_settings("project", cx));
        });
        cx.run_until_parked();

        let field = cx
            .debug_bounds("project-display-name-field")
            .expect("project settings field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-settings-sheet").is_none(),
            "Escape closes the Project Settings card"
        );
    }

    #[gpui::test]
    async fn escape_closes_clone_repository_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("add-project").expect("add project control");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let clone = cx
            .debug_bounds("add-project-clone")
            .expect("clone project choice");
        cx.simulate_click(clone.center(), Modifiers::none());
        cx.run_until_parked();
        let field = cx.debug_bounds("clone-url-field").expect("clone URL field");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-form-overlay").is_none(),
            "Escape closes the Clone Repository card"
        );
    }

    #[gpui::test]
    async fn escape_closes_create_project_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("add-project").expect("add project control");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let create = cx
            .debug_bounds("add-project-create")
            .expect("create project choice");
        cx.simulate_click(create.center(), Modifiers::none());
        cx.run_until_parked();
        let field = cx
            .debug_bounds("create-name-field")
            .expect("create project name field");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-form-overlay").is_none(),
            "Escape closes the Create Project card"
        );
    }

    #[gpui::test]
    async fn backdrop_click_closes_project_settings_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "Project".into(),
                    is_git: false,
                    root_path: PathBuf::from("/tmp/project"),
                    worktrees: Vec::new(),
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.open_project_settings("project", cx));
        });
        cx.run_until_parked();

        let sheet = cx
            .debug_bounds("project-settings-sheet")
            .expect("project settings card is drawn");
        cx.simulate_click(
            point(sheet.right() + px(40.0), sheet.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-settings-sheet").is_none(),
            "clicking outside Project Settings closes the card"
        );
    }

    #[gpui::test]
    async fn backdrop_click_closes_clone_repository_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("add-project").expect("add project control");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let clone = cx
            .debug_bounds("add-project-clone")
            .expect("clone project choice");
        cx.simulate_click(clone.center(), Modifiers::none());
        cx.run_until_parked();
        let card = cx
            .debug_bounds("project-form-card")
            .expect("clone card is drawn");
        cx.simulate_click(
            point(card.left() - px(4.0), card.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-form-overlay").is_none(),
            "clicking outside Clone Repository closes the card"
        );
    }

    #[gpui::test]
    async fn backdrop_click_closes_create_project_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("add-project").expect("add project control");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        let create = cx
            .debug_bounds("add-project-create")
            .expect("create project choice");
        cx.simulate_click(create.center(), Modifiers::none());
        cx.run_until_parked();
        let card = cx
            .debug_bounds("project-form-card")
            .expect("create card is drawn");
        cx.simulate_click(
            point(card.left() - px(4.0), card.center().y),
            Modifiers::none(),
        );
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-form-overlay").is_none(),
            "clicking outside Create Project closes the card"
        );
    }

    #[gpui::test]
    async fn opening_project_cards_replaces_the_existing_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "Project".into(),
                    is_git: false,
                    root_path: PathBuf::from("/tmp/project"),
                    worktrees: Vec::new(),
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.open_project_settings("project", cx));
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("project-settings-sheet").is_some());

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.start_clone_project(cx));
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-form-overlay").is_some()
                && cx.debug_bounds("clone-url-field").is_some(),
            "Clone Repository replaces Project Settings instead of stacking"
        );

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.open_project_settings("project", cx));
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-settings-sheet").is_some()
                && cx.debug_bounds("project-form-overlay").is_none(),
            "Project Settings replaces Clone Repository instead of stacking"
        );

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| sidebar.start_create_project(cx));
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-settings-sheet").is_none()
                && cx.debug_bounds("create-name-field").is_some(),
            "Create Project replaces Project Settings instead of stacking"
        );
    }

    #[gpui::test]
    async fn project_settings_mounts_the_icon_picker(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "Project".into(),
                    is_git: false,
                    root_path: PathBuf::from("/tmp/project"),
                    worktrees: Vec::new(),
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-icon-picker").is_some(),
            "project settings mounts the icon picker"
        );
        assert!(
            cx.debug_bounds("project-icon-glyph-folder").is_some(),
            "the mounted picker renders its glyph choices"
        );
    }

    /// F-PRJ-13: wave F live-drove Reset and found it appeared to fail --
    /// the sidebar's active worktree silently jumped to a different,
    /// not-visible-in-sheet project the instant Reset was clicked. Root
    /// cause: `render_project_settings`'s full-sheet overlay div is a plain
    /// `.absolute()` sibling of `sidebar-tree`, never `.occlude()`d, so
    /// GPUI's hit test (`Frame::hit_test`, which walks hitboxes back-to-
    /// front and only stops at a `HitboxBehavior::BlockMouse` hitbox)
    /// collects every interactive hitbox under the click, sheet AND row
    /// both -- both `on_click` handlers fire for one physical click. This
    /// builds two projects sized so a real worktree row of the *second*
    /// renders directly under the Reset button of the *first*'s open sheet
    /// (confirmed, not assumed: the test fails outright if no row's bounds
    /// intersect Reset's before asserting anything about the click), then
    /// clicks Reset and asserts no `SelectWorktree` reached the host --
    /// only the icon-reset update should.
    #[gpui::test]
    async fn reset_button_click_does_not_leak_through_to_the_row_underneath(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        // Many single-line worktree rows on the decoy project push its rows
        // down the sidebar until one lands under the settings sheet's fixed
        // Reset position -- the sheet is a full-height overlay starting at
        // the very top, so Reset's own y is constant regardless of which
        // project opened it.
        //
        // #151 shortened these rows: a worktree with no Primary pill and no
        // comment has no sub-line, so it is 32px rather than 51px. Twelve of
        // them no longer reach Reset, so the count is raised until they do.
        // The invariant assertion below is what actually guards this — it
        // fails loudly rather than letting the test quietly stop exercising
        // the occlusion it exists to prove.
        let decoy_worktrees: Vec<SidebarWorktree> = (0..24)
            .map(|i| SidebarWorktree {
                branch: format!("decoy-{i}"),
                path: PathBuf::from(format!("/tmp/prj13-decoy/wt-{i}")),
                is_primary: i == 0,
                comment: None,
            })
            .collect();
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![
                    SidebarProject {
                        id: "settings-project".into(),
                        name: "settings-project".into(),
                        is_git: true,
                        root_path: PathBuf::from("/tmp/prj13-settings"),
                        worktrees: Vec::new(),
                    },
                    SidebarProject {
                        id: "decoy-project".into(),
                        name: "decoy-project".into(),
                        is_git: true,
                        root_path: PathBuf::from("/tmp/prj13-decoy"),
                        worktrees: decoy_worktrees,
                    },
                ],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("settings-project", cx)
            });
        });
        cx.run_until_parked();

        let reset = cx
            .debug_bounds("project-icon-reset")
            .expect("the reset control is drawn");

        // Confirm the overlap this fix depends on actually exists in this
        // fixture, rather than assuming geometry: without it, a click at
        // Reset's centre proves nothing about occlusion either way.
        let overlapping_row = std::iter::once(0)
            .chain(1000..1040)
            .filter_map(|row_id| {
                let selector: &'static str =
                    Box::leak(format!("sidebar-row-{row_id}").into_boxed_str());
                cx.debug_bounds(selector)
            })
            .find(|bounds| bounds.intersects(&reset));
        assert!(
            overlapping_row.is_some(),
            "fixture invariant: a sidebar row must render under the Reset \
             button for this test to exercise the click-through bug"
        );

        cx.simulate_click(reset.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            !events
                .borrow()
                .iter()
                .any(|event| matches!(event, SidebarEvent::SelectWorktree(_))),
            "clicking Reset must not also select whichever decoy worktree \
             row is rendered underneath it -- got {:?}",
            events.borrow()
        );
    }

    #[gpui::test]
    async fn project_settings_changes_update_the_row_and_emit_a_durable_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "Project".into(),
                    is_git: true,
                    root_path: PathBuf::from("/tmp/project"),
                    worktrees: Vec::new(),
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();

        let field = cx
            .debug_bounds("project-display-name-field")
            .expect("display-name field is drawn");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("Renamed");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.display_name.as_deref() == Some("Renamed")
            )),
            "typing a display name emits a durable project update"
        );

        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let glyph = cx
            .debug_bounds("project-icon-glyph-git-branch")
            .expect("the glyph picker remains mounted");
        cx.simulate_click(glyph.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.icon.value == ProjectIconValue::Symbol(ProjectGlyph::GitBranch)
            )),
            "choosing a glyph emits the selected project icon"
        );
        let row_title = cx.update(|window, cx| {
            window
                .root::<Sidebar>()
                .flatten()
                .expect("sidebar root")
                .read(cx)
                .rows
                .first()
                .expect("project row")
                .title
                .clone()
        });
        assert_eq!(row_title, "Renamed", "the sidebar reflects the edited name");
    }

    /// F-PRJ-17/F-PRJ-18: wave F found `grep -rn "worktree_base|default_worktree_base|
    /// WorktreeBase" rust/crates/sirio_ui/src/*.rs` returned zero hits, and a
    /// live top-to-bottom read of the Project Settings sheet found no
    /// default-base or worktree-location control anywhere in it. This test
    /// fails to compile on the unfixed tree (no such ids are ever drawn, no
    /// such fields exist on `ProjectSettingsUpdate`) and passes once the
    /// controls exist and are gated on `card.is_git` the same way the New
    /// Worktree row itself is (a project with no worktrees has no base or
    /// location to set).
    #[gpui::test]
    async fn worktree_base_and_location_fields_are_drawn_only_for_git_projects(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![
                    SidebarProject {
                        id: "git-project".into(),
                        name: "git-project".into(),
                        is_git: true,
                        root_path: PathBuf::from("/tmp/prj1718-git"),
                        worktrees: vec![SidebarWorktree {
                            branch: "main".into(),
                            path: PathBuf::from("/tmp/prj1718-git-main"),
                            is_primary: true,
                            comment: None,
                        }],
                    },
                    SidebarProject {
                        id: "folder-project".into(),
                        name: "folder-project".into(),
                        is_git: false,
                        root_path: PathBuf::from("/tmp/prj1718-folder"),
                        worktrees: Vec::new(),
                    },
                ],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("git-project", cx)
            });
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-worktree-base-field").is_some(),
            "a git project's settings sheet draws the Default Worktree Base field"
        );
        assert!(
            cx.debug_bounds("project-worktree-location-field").is_some(),
            "a git project's settings sheet draws the Worktree Location field"
        );

        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("folder-project", cx)
            });
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-worktree-base-field").is_none(),
            "a non-git project has no worktrees, so it offers no base/location controls"
        );
    }

    /// F-PRJ-17/F-PRJ-18: typing into either field, and the two clearing
    /// controls ("Use Primary", "Restore Default"), each emit a durable
    /// `ProjectSettingsChanged` carrying the new value.
    #[gpui::test]
    async fn typing_worktree_base_and_location_emits_a_durable_update(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "project".into(),
                    is_git: true,
                    root_path: PathBuf::from("/tmp/prj1718"),
                    worktrees: vec![SidebarWorktree {
                        branch: "main".into(),
                        path: PathBuf::from("/tmp/prj1718-main"),
                        is_primary: true,
                        comment: None,
                    }],
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                captured.borrow_mut().push(event.clone());
            })
            .detach();
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();

        let base_field = cx
            .debug_bounds("project-worktree-base-field")
            .expect("worktree-base field is drawn");
        cx.simulate_click(base_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("develop");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.default_worktree_base.as_deref() == Some("develop")
            )),
            "typing a base branch emits it on the durable update"
        );

        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let location_field = cx
            .debug_bounds("project-worktree-location-field")
            .expect("worktree-location field is drawn");
        cx.simulate_click(location_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("/srv/worktrees");
        cx.run_until_parked();

        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.worktree_location_override.as_deref() == Some("/srv/worktrees")
            )),
            "typing a location override emits it on the durable update"
        );

        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let use_primary = cx
            .debug_bounds("project-worktree-base-use-primary")
            .expect("Use Primary is drawn");
        cx.simulate_click(use_primary.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.default_worktree_base.is_none()
            )),
            "Use Primary clears the pinned base"
        );

        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let restore = cx
            .debug_bounds("project-worktree-location-restore")
            .expect("Restore Default is drawn once an override is set");
        cx.simulate_click(restore.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            events.borrow().iter().any(|event| matches!(
                event,
                SidebarEvent::ProjectSettingsChanged(update)
                    if update.worktree_location_override.is_none()
            )),
            "Restore Default clears the location override"
        );
    }

    /// F-PRJ-17/F-PRJ-18's VERIFY clause, verbatim: "reopen the sheet, and
    /// confirm the selected option persists." A real host applies
    /// `set_project_worktree_defaults` after every `ProjectSettingsChanged`
    /// (the same loop `refresh_sidebar` already drives for icons through
    /// `set_project_identity`) -- this test drives exactly that host round
    /// trip without a live `sirio` process, closes the sheet, reopens it,
    /// and reads the freshly-built card's own drafts.
    #[gpui::test]
    async fn worktree_base_and_location_persist_across_reopen(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            Sidebar::from_projects(
                vec![SidebarProject {
                    id: "project".into(),
                    name: "project".into(),
                    is_git: true,
                    root_path: PathBuf::from("/tmp/prj1718-persist"),
                    worktrees: vec![SidebarWorktree {
                        branch: "main".into(),
                        path: PathBuf::from("/tmp/prj1718-persist-main"),
                        is_primary: true,
                        comment: None,
                    }],
                }],
                cx,
            )
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        let saved = Rc::new(RefCell::new(None));
        let captured = saved.clone();
        let host_sidebar = sidebar.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, cx| {
                if let SidebarEvent::ProjectSettingsChanged(update) = event {
                    *captured.borrow_mut() = Some((
                        update.default_worktree_base.clone(),
                        update.worktree_location_override.clone(),
                    ));
                    host_sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_project_worktree_defaults(
                            &update.id,
                            update.default_worktree_base.clone(),
                            update.worktree_location_override.clone(),
                            cx,
                        );
                    });
                }
            })
            .detach();
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();

        let base_field = cx
            .debug_bounds("project-worktree-base-field")
            .expect("worktree-base field is drawn");
        cx.simulate_click(base_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("release");
        cx.run_until_parked();

        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let location_field = cx
            .debug_bounds("project-worktree-location-field")
            .expect("worktree-location field is drawn");
        cx.simulate_click(location_field.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("/srv/worktrees");
        cx.run_until_parked();

        assert_eq!(
            saved.borrow().clone(),
            Some((
                Some("release".to_string()),
                Some("/srv/worktrees".to_string())
            )),
            "both edits reached the host round trip"
        );

        // Close, then reopen: a fresh card is built from
        // `project_worktree_defaults`, which now holds what the host saved.
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.project_settings = None;
                cx.notify();
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();

        let reopened = cx.update(|_, cx| {
            sidebar.read(cx).project_settings.as_ref().map(|card| {
                (
                    card.default_worktree_base.borrow().clone(),
                    card.worktree_location_override.borrow().clone(),
                )
            })
        });
        assert_eq!(
            reopened,
            Some(("release".to_string(), "/srv/worktrees".to_string())),
            "reopening the sheet shows the persisted base and location -- \
             the clause's exact requirement"
        );
    }

    /// F-PRJ-12: open_project_settings snapshots is_git once; set_projects
    /// (how the host reports "Initialize Git" completing) must patch an
    /// already-open card in place rather than leaving it stale.
    #[gpui::test]
    async fn set_projects_refreshes_an_open_project_settings_card(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let make_project = |is_git: bool| SidebarProject {
            id: "project".into(),
            name: "Project".into(),
            is_git,
            root_path: PathBuf::from("/tmp/project"),
            worktrees: Vec::new(),
        };
        let window =
            cx.add_window(|_window, cx| Sidebar::from_projects(vec![make_project(false)], cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("project", cx)
            });
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("project-settings-initialize-git").is_some(),
            "a folder project's open sheet offers Initialize Git"
        );

        // The host reports the project is now a git repo the same way it
        // reports any project-list change: a fresh set_projects call --
        // the sheet is still open the whole time.
        cx.update(|_, cx| {
            sidebar.update(cx, |sidebar, cx| {
                sidebar.set_projects(vec![make_project(true)], cx);
            });
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("project-settings-initialize-git").is_none(),
            "the open sheet must drop Initialize Git once the project is a repo, \
             without being closed and reopened"
        );
        let is_git = cx.update(|_, cx| {
            sidebar
                .read(cx)
                .project_settings
                .as_ref()
                .expect("sheet stays open across set_projects")
                .is_git
        });
        assert!(is_git, "the open card's is_git field itself was patched");
    }

    /// F-SID-02: typing in the Filter field narrows the drawn rows to the
    /// matching project, and clearing the filter restores every row.
    #[gpui::test]
    async fn filter_narrows_rows_and_clearing_restores_them(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Focus the filter field the way a user does: click it.
        let filter = cx
            .debug_bounds("filter-field")
            .expect("the Filter field is drawn");
        cx.simulate_click(filter.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("filter-placeholder").is_some(),
            "an empty filter shows its placeholder"
        );

        // The first character must remove the placeholder immediately.
        cx.simulate_input("t");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("filter-placeholder").is_none(),
            "the placeholder disappears as soon as the filter receives text"
        );

        // Finish a query matching exactly one project of the four.
        cx.simulate_input("racker");
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let typed = cx.read(|cx| sidebar_entity.read(cx).filter.clone());
        assert_eq!(typed, "tracker", "the keystrokes reached the filter state");

        // Only Project-Tracker's row remains drawn; every other project row
        // and the first project's expanded children disappear.
        for id in [0, 1, 2, 3, 4, 5, 6, 8] {
            let selector: &'static str = Box::leak(format!("sidebar-row-{id}").into_boxed_str());
            assert!(
                cx.debug_bounds(selector).is_none(),
                "row {id} must be filtered out while the filter reads 'tracker'"
            );
        }
        assert!(
            cx.debug_bounds("sidebar-row-7").is_some(),
            "the matching project row stays drawn"
        );

        // Clearing the filter restores every project row.
        cx.simulate_keystrokes(
            "backspace backspace backspace backspace backspace backspace backspace",
        );
        cx.run_until_parked();
        for id in [0, 4, 7, 8] {
            let selector: &'static str = Box::leak(format!("sidebar-row-{id}").into_boxed_str());
            assert!(
                cx.debug_bounds(selector).is_some(),
                "project row {id} returns after the filter is cleared"
            );
        }
    }

    /// F-SID-06: a collapsed project has no visible worktree row to carry
    /// the status dot, so the project row itself badges the most urgent
    /// status among its (hidden) worktree children; expanding the project
    /// hands the dot back to the worktree row and clears the badge.
    #[gpui::test]
    async fn collapsed_project_badges_the_worst_child_worktree_status(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));

        // Project row 4 (the long fixture name) starts collapsed with
        // worktree row 5 as its only child.
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.set_worktree_activity(5, Some(ActivityStatus::Error), None, Vec::new(), cx);
        });
        cx.run_until_parked();

        let project_status = entity.update(&mut cx, |sidebar, _cx| {
            sidebar
                .visible_rows()
                .into_iter()
                .find(|row| row.id == 4)
                .and_then(|row| row.agent_status)
        });
        assert_eq!(
            project_status,
            Some(ActivityStatus::Error),
            "a collapsed project badges its most urgent child worktree status"
        );

        // Expanding the project stops the aggregate badge; the now-visible
        // worktree row carries its own dot instead (render_row's existing
        // RowKind::Worktree gate).
        entity.update(&mut cx, |sidebar, cx| {
            sidebar.toggle_project(4, cx);
        });
        cx.run_until_parked();
        let expanded_status = entity.update(&mut cx, |sidebar, _cx| {
            sidebar
                .visible_rows()
                .into_iter()
                .find(|row| row.id == 4)
                .and_then(|row| row.agent_status)
        });
        assert_eq!(
            expanded_status, None,
            "an expanded project row does not carry the aggregated badge"
        );
    }

    /// F-SID-04: clicking a project row's chevron reveals its children;
    /// bezel's arrow actions then walk, collapse and re-expand the same rows.
    #[gpui::test]
    async fn project_chevron_hides_and_restores_children(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::tree::init);
        let window = cx.add_window(|_window, cx| Sidebar::new_with_repo(cx, None));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Project row 4 (the long fixture name) starts collapsed: its
        // worktree row (5) and tab row (6) are not drawn.
        assert!(
            cx.debug_bounds("sidebar-row-5").is_none(),
            "a collapsed project's worktree row is hidden"
        );
        assert!(
            cx.debug_bounds("sidebar-row-6").is_none(),
            "a collapsed project's tab row is hidden"
        );

        // Click the disclosure chevron: it rides in the row's leading 12px
        // slot (8px row padding + 6px into the slot).
        let row4 = cx
            .debug_bounds("sidebar-row-4")
            .expect("the collapsed project row is drawn");
        let chevron = point(row4.origin.x + px(8.0) + px(6.0), row4.center().y);
        cx.simulate_click(chevron, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-5").is_some(),
            "the chevron click reveals the project's worktree row"
        );
        assert!(
            cx.debug_bounds("sidebar-row-6").is_some(),
            "the chevron click reveals the project's tab row"
        );

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let project_cursor = sidebar.read_with(&cx.cx, |sidebar, _| sidebar.tree_cursor);
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            sidebar.read_with(&cx.cx, |sidebar, _| sidebar.tree_cursor),
            project_cursor + 1,
            "down moves the bezel cursor to the first worktree"
        );
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(
            sidebar.read_with(&cx.cx, |sidebar, _| sidebar.tree_cursor),
            project_cursor,
            "up returns the bezel cursor to the project"
        );

        // The click also places the bezel tree cursor on the project. From
        // there the standard tree actions collapse and re-expand it without
        // changing the sidebar's project/worktree semantics.
        cx.simulate_keystrokes("left");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-5").is_none(),
            "left collapses the project and hides its worktree row"
        );
        assert!(
            cx.debug_bounds("sidebar-row-6").is_none(),
            "left collapses the project and hides its tab row"
        );

        cx.simulate_keystrokes("right");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-row-5").is_some(),
            "right expands the project and restores its worktree row"
        );
        assert!(
            cx.debug_bounds("sidebar-row-6").is_some(),
            "right expands the project and restores its tab row"
        );
    }

    /// F-SID-10: the context menu's Remove Project asks the platform for
    /// confirmation before emitting anything — accepting emits RemoveProject
    /// with the project's id; cancelling emits nothing.
    #[gpui::test]
    async fn remove_project_context_item_confirms_before_emitting(cx: &mut gpui::TestAppContext) {
        let repo = scratch_repo("remove-project");

        cx.update(Theme::init);
        let projects = vec![SidebarProject {
            id: "proj-1".to_string(),
            name: "scratch".to_string(),
            is_git: true,
            root_path: repo.clone(),
            worktrees: vec![],
        }];
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(projects, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
        });

        // Open the project's context menu and choose Remove Project.
        let remove_item = cx
            .debug_bounds("sidebar-row-0")
            .expect("the project row is drawn");
        cx.simulate_event(MouseDownEvent {
            position: remove_item.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: remove_item.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        let item = cx
            .debug_bounds("sidebar-context-item-remove-project")
            .expect("the context menu exposes Remove Project");
        cx.simulate_click(item.center(), Modifiers::none());
        cx.run_until_parked();

        // The platform prompt is up and nothing has been emitted yet.
        assert!(cx.has_pending_prompt(), "removal asks for confirmation");
        assert!(
            events.borrow().is_empty(),
            "nothing may be emitted before the user answers"
        );

        // Cancelling emits nothing.
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(
            events.borrow().is_empty(),
            "cancelling the prompt emits nothing, got {:?}",
            events.borrow()
        );

        // Accepting emits RemoveProject with the project's id.
        cx.simulate_event(MouseDownEvent {
            position: remove_item.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: remove_item.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        let item = cx
            .debug_bounds("sidebar-context-item-remove-project")
            .expect("the context menu exposes Remove Project again");
        cx.simulate_click(item.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.has_pending_prompt(), "removal asks for confirmation");
        cx.simulate_prompt_answer("Remove from Sirio");
        cx.run_until_parked();
        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::RemoveProject(id) if id == "proj-1")),
            "accepting the prompt emits RemoveProject with the project's id, got {emitted:?}"
        );
    }

    /// F-PRJ-11: the removal logic (`request_remove_project`) was already
    /// correct but only reachable from the context menu one level up --
    /// Project Settings itself had no removal control at all. The sheet's
    /// own Remove Project must ask for confirmation and emit the same
    /// event the context-menu path does.
    #[gpui::test]
    async fn project_settings_remove_project_confirms_before_emitting(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let projects = vec![SidebarProject {
            id: "proj-1".to_string(),
            name: "scratch".to_string(),
            is_git: true,
            root_path: PathBuf::from("/tmp/proj-1"),
            worktrees: vec![],
        }];
        let window = cx.add_window(|_window, cx| Sidebar::from_projects(projects, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar_entity =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let collected = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&sidebar_entity, move |_, event: &SidebarEvent, _| {
                collected.borrow_mut().push(event.clone());
            })
            .detach();
            sidebar_entity.update(cx, |sidebar, cx| {
                sidebar.open_project_settings("proj-1", cx)
            });
        });
        cx.run_until_parked();

        let remove = cx
            .debug_bounds("project-settings-remove")
            .expect("Project Settings draws a Remove Project control");
        cx.simulate_click(remove.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.has_pending_prompt(), "removal asks for confirmation");
        assert!(
            events.borrow().is_empty(),
            "nothing may be emitted before the user answers"
        );

        cx.simulate_prompt_answer("Remove from Sirio");
        cx.run_until_parked();
        let emitted = events.borrow();
        assert!(
            emitted
                .iter()
                .any(|event| matches!(event, SidebarEvent::RemoveProject(id) if id == "proj-1")),
            "accepting the prompt emits RemoveProject with the project's id, got {emitted:?}"
        );
    }
}

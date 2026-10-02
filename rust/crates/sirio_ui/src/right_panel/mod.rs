//! The checkout right panel: filesystem browsing and git changes.
//!
//! `mod.rs` owns the panel's lifecycle, its header, and the choice of
//! surface below it; the Files surface lives in its sibling module (`files`).
//!
//! The data model deliberately stays local to this panel. Git operations are
//! delegated to `sirio_git`; the host application can later replace the
//! refresh callbacks with its project store without changing the row layout.

mod change_requests;
mod files;
mod history;
mod history_toolbar;
mod references;

use gpui::{
    App, Context, EventEmitter, FocusHandle, MouseButton, Render, Task, Window, div, prelude::*, px,
};
use sirio_theme::Theme;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::changes::{ChangesTabActionEvent, ChangesTabEvent};
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use change_requests::{ChangeRequestList, ChangeRequestListEvent};
use history::{GitHistory, GitHistoryEvent};

pub use change_requests::{filter_word, parse_filter};
pub use references::{GroupedRow, ReferenceRow, ReferencesState, group_by_file, summary};
use references::{ReferencesEvent, ReferencesList};

const HEADER_HEIGHT: f32 = 40.0;
/// User actions originating from the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPanelEvent {
    /// Open a file from the Files tree in the host application's tab strip.
    OpenFile(PathBuf),
}

/// Actions that need a host-owned surface beyond the existing file-open door.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPanelActionEvent {
    /// Open a modified file in the host application's Diff tab.
    OpenDiff(PathBuf),
    /// Open a terminal prepared to resolve this conflicted path.
    ResolveInTerminal(PathBuf),
    /// Open one commit's diff in a host tab, by full object name. The History
    /// view raises this; the host owns the tab it lands in.
    OpenCommit(String),
    /// Open a file at a line — a references result. The panel names the
    /// place; the app owns the tab.
    OpenAtLine { path: PathBuf, line: usize },
    /// Open a change request in a host tab. The title rides along so the
    /// tab is named before its first load.
    OpenChangeRequest {
        reference: sirio_forge::ChangeRef,
        title: String,
    },
}

/// Which view the right panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelView {
    #[default]
    Files,
    Diff,
    History,
    References,
    ChangeRequests,
}

/// A settled Files tree, handed to the host on the way out of a worktree so
/// the way back in draws it at once. `expanded` is kept beside the tree
/// because the nodes carry their own `expanded` flag: a snapshot taken while
/// a walk was replacing part of the tree would otherwise lose which folders
/// the user had open.
#[derive(Clone, Debug)]
pub struct FilesSnapshot {
    file_tree: Vec<files::FileNode>,
    flattened_file_rows: Vec<files::FileRow>,
    git_markers: files::GitMarkers,
    expanded: Vec<PathBuf>,
    /// Whether the snapshot was invalidated by a filesystem change.
    dirty: bool,
    /// When this snapshot was produced.
    captured_at: SystemTime,
}

impl FilesSnapshot {
    /// A clean snapshot young enough to draw without making the Files panel
    /// wait for a walk first.
    ///
    /// The age bound is about what a *restored* tree may claim on arrival,
    /// so it belongs to the display decision only. It is deliberately not a
    /// "needs rewalking" test: it expires on its own, and a caller that
    /// rewalks whatever is not fresh never reaches a resting state. Use
    /// `is_dirty` for that -- it moves only on evidence.
    #[must_use]
    pub fn is_fresh(&self, now: SystemTime) -> bool {
        !self.dirty
            && now.duration_since(self.captured_at).unwrap_or_default() <= Duration::from_secs(2)
    }

    /// Whether a filesystem change has been seen under this tree since the
    /// walk that produced it.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether the worktree-relative `relative` was excluded by the ignore
    /// rules when this snapshot was walked.
    ///
    /// The walk already pays for `git ls-files --ignored --directory`, so a
    /// watcher can reuse the answer instead of running git per event. That
    /// matters because a recursive watch on a checkout sees its build output
    /// too: without this, one `cargo build` under the tree reports thousands
    /// of `target/` writes, each of which would mark the tree dirty and buy
    /// another repository-sized walk.
    #[must_use]
    pub fn ignores(&self, relative: &std::path::Path) -> bool {
        self.git_markers.ignores(relative)
    }

    /// Marks cached data for a silent background refresh while retaining it
    /// as the immediately visible tree.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Builds a settled Files snapshot without creating a visible panel.
pub fn build_files_snapshot(repo_root: impl Into<PathBuf>) -> Result<FilesSnapshot, String> {
    files::build_snapshot(&repo_root.into())
}

/// App-wide selection. A GPUI global rather than a field, for the same
/// reason `DiffViewMode` is one: `select_worktree` throws the whole
/// `RightPanel` entity away and builds a fresh one, so a field would snap
/// back to Files on every worktree switch.
struct PanelViewSetting(PanelView);

impl gpui::Global for PanelViewSetting {}

/// Session-wide Files preference. It deliberately stays out of persisted
/// settings: a new app launch starts with dotfiles hidden again.
struct ShowHiddenFilesSetting(bool);

impl gpui::Global for ShowHiddenFilesSetting {}

impl ShowHiddenFilesSetting {
    fn get(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|setting| setting.0)
    }

    fn set(show: bool, cx: &mut App) {
        cx.set_global(Self(show));
    }
}

impl PanelView {
    /// Rail order, left to right.
    const ORDER: [PanelView; 5] = [
        PanelView::Files,
        PanelView::Diff,
        PanelView::History,
        PanelView::References,
        PanelView::ChangeRequests,
    ];

    fn icon(self) -> Icon {
        match self {
            PanelView::Files => Icon::FileTree,
            PanelView::Diff => Icon::Diff,
            PanelView::History => Icon::GitGraph,
            PanelView::References => Icon::MagnifyingGlass,
            PanelView::ChangeRequests => Icon::PullRequest,
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            PanelView::Files => "right-panel-tab-files",
            PanelView::Diff => "right-panel-tab-diff",
            PanelView::History => "right-panel-tab-history",
            PanelView::References => "right-panel-tab-references",
            PanelView::ChangeRequests => "right-panel-tab-change-requests",
        }
    }

    pub fn get(cx: &App) -> Self {
        if cx.has_global::<PanelViewSetting>() {
            cx.global::<PanelViewSetting>().0
        } else {
            Self::default()
        }
    }

    pub fn set(view: Self, cx: &mut App) {
        cx.set_global(PanelViewSetting(view));
    }
}

/// The GPUI right panel: the filesystem tree, changes, history, and references.
pub struct RightPanel {
    repo_root: PathBuf,
    project_is_git: bool,
    /// Filesystem roots the panel may inspect. Startup supplies the project
    /// roots and linked worktrees; a panel created for one runtime selection
    /// uses that worktree as its only root.
    allowed_roots: Vec<PathBuf>,
    /// A panel is constructed for a selected checkout. Closing that checkout
    /// must explicitly revoke the binding; retaining its path would make the
    /// Files/Changes surface look current while serving stale data.
    worktree_selected: bool,
    file_tree: Vec<files::FileNode>,
    flattened_file_rows: Vec<files::FileRow>,
    git_markers: files::GitMarkers,
    /// The in-flight folder-expansion walk, if any. Replaced (never
    /// queued) on every new expansion request.
    walk_task: Option<Task<()>>,
    /// Whether the top-level tree refresh is currently in flight. This is
    /// the single-flight guard. It is deliberately *not* the loading state
    /// rendered to users — see `settled`.
    refresh_started: bool,
    /// The in-flight root refresh. It is separate from `walk_task` so a user
    /// can expand a directory while the root refresh is still loading.
    refresh_task: Option<Task<()>>,
    /// Whether any top-level walk has ever finished, successfully or not.
    ///
    /// This, and not `refresh_started`, is what "Loading files…" is about.
    /// `ensure_tree_refresh` re-walks every second, so gating the
    /// placeholder on "a walk is in flight" put a full-panel placeholder
    /// over a perfectly good tree once a second — on this 4-core box the
    /// tree was visible roughly one frame in three, for minutes. Gating it
    /// on "no walk has ever finished" also covers the two cases an
    /// `is_empty()` test gets wrong: a directory that is genuinely empty
    /// (which would say "Loading files…" forever) and a root that failed to
    /// read (whose error panel would be replaced by the placeholder on every
    /// tick, hiding the Retry the user is trying to click).
    settled: bool,
    /// Bumped whenever a root refresh is superseded. A completed refresh must
    /// not publish data for an earlier checkout or walk.
    refresh_generation: u64,
    refresh_error: Option<String>,
    selected_path: Option<PathBuf>,
    file_focus: Option<FocusHandle>,
    /// Bumped on every walk request (and on collapse): a walk that
    /// completes after a newer one was requested must not apply its
    /// result late.
    walk_generation: u64,
    /// Whether the first Files render has requested a refresh.
    refresh_loop_started: bool,
    /// Coalesces refresh requests until the 300ms debounce expires.
    refresh_debounce_task: Option<Task<()>>,
    refresh_dirty: bool,
    /// Backs off a refresh that keeps failing (F-CHG-03), and is
    /// dropped the moment one succeeds.
    refresh_retry_task: Option<Task<()>>,
    refresh_failures: u32,
    /// Detects leaving and re-entering the Files view without polling.
    files_view_active: bool,
    /// The panel view the last render drew, so a switch to a lazily built
    /// surface can schedule the frame that builds it.
    last_panel_view: Option<PanelView>,
    file_context_menu: Option<files::FileContextMenu>,
    /// Built on first selection of the Diff view, dropped when the checkout
    /// changes. A user who never opens Diff never pays for a git status here.
    changes: Option<gpui::Entity<crate::changes::ChangesTab>>,
    /// Kept alive so the child's events keep reaching `re_emit`.
    changes_subscriptions: Vec<gpui::Subscription>,
    /// Built on first selection of the History view, dropped when the
    /// checkout changes. A user who never opens History never runs `git log`.
    history: Option<gpui::Entity<GitHistory>>,
    history_subscription: Option<gpui::Subscription>,
    /// Built when References is first shown, dropped when the checkout
    /// changes. A user who never asks for references never pays for it.
    references: Option<gpui::Entity<ReferencesList>>,
    references_subscription: Option<gpui::Subscription>,
    /// Built the first time the change request view is shown, dropped when
    /// the checkout changes — which also cancels its in-flight loads, so a
    /// late answer can never paint another worktree's rows.
    change_requests: Option<gpui::Entity<ChangeRequestList>>,
    change_requests_subscription: Option<gpui::Subscription>,
    /// The resolved right-panel width, pushed in by the host every render.
    /// The History toolbar shapes itself from it; see [`GitHistory::panel_width`].
    panel_width: f32,
    is_stale: bool,
    updating: bool,
}

impl RightPanel {
    /// Creates the panel for one checkout.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        Self {
            repo_root: repo_root.clone(),
            project_is_git: true,
            allowed_roots: vec![repo_root],
            worktree_selected: true,
            file_tree: Vec::new(),
            flattened_file_rows: Vec::new(),
            git_markers: files::GitMarkers::default(),
            walk_task: None,
            refresh_started: false,
            refresh_task: None,
            settled: false,
            refresh_generation: 0,
            refresh_error: None,
            selected_path: None,
            file_focus: None,
            walk_generation: 0,
            refresh_loop_started: false,
            refresh_debounce_task: None,
            refresh_dirty: false,
            refresh_retry_task: None,
            refresh_failures: 0,
            files_view_active: false,
            last_panel_view: None,
            file_context_menu: None,
            changes: None,
            changes_subscriptions: Vec::new(),
            history: None,
            history_subscription: None,
            references: None,
            references_subscription: None,
            change_requests: None,
            change_requests_subscription: None,
            panel_width: 320.0,
            is_stale: false,
            updating: false,
        }
    }

    /// Creates the startup panel with the project catalog's filesystem
    /// allowlist. A stale restored directory is left unselected until the
    /// host reconciles it with a current catalog worktree.
    pub fn with_roots(repo_root: impl Into<PathBuf>, allowed_roots: Vec<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        let worktree_selected = allowed_roots.iter().any(|root| root == &repo_root);
        let mut panel = Self::new(repo_root);
        panel.allowed_roots = allowed_roots;
        panel.worktree_selected = worktree_selected;
        panel
    }

    /// Creates the panel for a worktree the host already holds a settled
    /// Files tree for. `select_worktree` throws the whole entity away on
    /// every switch, so without this the tree replays its walk each time and
    /// the user watches "Loading files..." on a directory they were reading
    /// a second ago. The restored tree is marked stale on arrival: it is
    /// drawn immediately, and a silent refresh replaces it with what is on
    /// disk only when the snapshot is dirty or old.
    pub fn with_snapshot(
        repo_root: impl Into<PathBuf>,
        snapshot: Option<FilesSnapshot>,
    ) -> Self {
        let mut panel = Self::new(repo_root);
        if let Some(snapshot) = snapshot {
            let snapshot_is_fresh = snapshot.is_fresh(SystemTime::now());
            panel.file_tree = snapshot.file_tree;
            files::restore_expanded(&mut panel.file_tree, &snapshot.expanded);
            panel.flattened_file_rows = snapshot.flattened_file_rows;
            panel.git_markers = snapshot.git_markers;
            panel.settled = true;
            panel.is_stale = snapshot.dirty;
            panel.updating = false;
            panel.refresh_loop_started = snapshot_is_fresh;
        }
        panel
    }

    /// Creates the startup panel while carrying the catalog's Git capability.
    pub fn with_snapshot_for_project(
        repo_root: impl Into<PathBuf>,
        project_is_git: bool,
        snapshot: Option<FilesSnapshot>,
    ) -> Self {
        let mut panel = Self::with_snapshot(repo_root, snapshot);
        panel.project_is_git = project_is_git;
        panel
    }

    /// The tree the host caches for this worktree, or `None` when there is
    /// nothing worth caching yet. A tree that is still loading is not useful,
    /// but a settled tree remains the best visible fallback while refreshing.
    pub fn files_snapshot(&self) -> Option<FilesSnapshot> {
        if !self.settled || (self.is_stale && self.file_tree.is_empty()) {
            return None;
        }
        Some(FilesSnapshot {
            file_tree: self.file_tree.clone(),
            flattened_file_rows: self.flattened_file_rows.clone(),
            git_markers: self.git_markers.clone(),
            expanded: files::collect_expanded(&self.file_tree),
            dirty: self.is_stale,
            captured_at: SystemTime::now(),
        })
    }

    /// The host pushes the resolved right-panel width every render; no-op
    /// when unchanged so a drag does not notify more than it must.
    pub fn set_panel_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.panel_width == width {
            return;
        }
        self.panel_width = width;
        self.sync_change_requests_visibility(cx);
        cx.notify();
    }

    /// Keeps the visible tree and asks for a silent debounced refresh.
    pub fn mark_files_dirty(&mut self, cx: &mut Context<Self>) {
        if !self.worktree_selected {
            return;
        }
        self.is_stale = self.settled;
        self.request_refresh(cx);
    }

    /// Remove the panel's checkout binding after its selected worktree
    /// closes. This clears both already drawn rows and any in-flight result's
    /// visible destination; a later worktree selection replaces the panel
    /// with a new bound instance.
    pub fn clear_worktree(&mut self, cx: &mut Context<Self>) {
        if !self.worktree_selected {
            return;
        }
        self.worktree_selected = false;
        self.file_tree.clear();
        self.flattened_file_rows.clear();
        self.git_markers = files::GitMarkers::default();
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        self.changes = None;
        self.changes_subscriptions.clear();
        self.history = None;
        self.history_subscription = None;
        self.references = None;
        self.references_subscription = None;
        self.change_requests = None;
        self.change_requests_subscription = None;
        self.is_stale = false;
        self.updating = false;
        self.refresh_started = false;
        self.refresh_task = None;
        self.refresh_debounce_task = None;
        self.refresh_dirty = false;
        self.refresh_retry_task = None;
        self.refresh_failures = 0;
        self.files_view_active = false;
        self.refresh_generation += 1;
        self.walk_task = None;
        self.walk_generation += 1;
        cx.notify();
    }

    /// The mirror of [`Self::clear_worktree`]: (re-)bind the panel to a
    /// genuinely selected checkout. F-CHG-02: `select_worktree` already
    /// covers a real, explicit worktree switch by throwing this whole entity
    /// away and building a fresh one via [`Self::with_snapshot_for_project`]
    /// -- but a worktree can also become the *current* one passively, e.g.
    /// `sync_control_state` re-matching `working_directory` against a
    /// project the user just added over the control socket, with no dedicated
    /// switch call in between. `SirioWorkspace::sync_activity` -- already
    /// the app's one continuous reconciliation point, run after essentially
    /// every state-changing action -- calls this every time so the panel
    /// cannot drift from `has_current_worktree()`'s answer no matter which
    /// path changed it. Idempotent when neither the selection state nor the
    /// bound path actually changed, so a tree the user has been expanding is
    /// left alone on the other 44-and-counting call sites that were already
    /// unrelated to worktree selection.
    pub fn bind_worktree(
        &mut self,
        repo_root: impl Into<PathBuf>,
        project_is_git: bool,
        cx: &mut Context<Self>,
    ) {
        let repo_root = repo_root.into();
        let capability_changed = self.project_is_git != project_is_git;
        self.project_is_git = project_is_git;
        if self.worktree_selected && self.repo_root == repo_root {
            if capability_changed {
                self.changes = None;
                self.changes_subscriptions.clear();
                cx.notify();
            }
            return;
        }
        if !self.allowed_roots.iter().any(|root| root == &repo_root) {
            self.allowed_roots.push(repo_root.clone());
        }
        self.worktree_selected = true;
        self.repo_root = repo_root;
        self.file_tree.clear();
        self.flattened_file_rows.clear();
        self.git_markers = files::GitMarkers::default();
        self.settled = false;
        self.is_stale = false;
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        self.changes = None;
        self.changes_subscriptions.clear();
        self.history = None;
        self.history_subscription = None;
        self.references = None;
        self.references_subscription = None;
        self.change_requests = None;
        self.change_requests_subscription = None;
        self.updating = false;
        self.refresh_started = false;
        self.refresh_task = None;
        self.refresh_debounce_task = None;
        self.refresh_dirty = false;
        self.refresh_retry_task = None;
        self.refresh_failures = 0;
        self.files_view_active = false;
        self.refresh_generation += 1;
        self.walk_task = None;
        self.walk_generation += 1;
        cx.notify();
    }
}

impl RightPanel {
    fn render_header(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = PanelView::get(cx);
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .children(PanelView::ORDER.into_iter().filter(|view| *view != PanelView::ChangeRequests || self.project_is_git).map(|view| {
                let is_active = view == active;
                div()
                    .id(view.element_id())
                    .debug_selector(move || view.element_id().to_owned())
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .when(is_active, |this| this.bg(theme.ely.hover))
                    .hover(|style| style.bg(theme.ely.hover))
                    .child(IconElement::new(view.icon(), IconSize::Small).text_color(
                        if is_active {
                            theme.ely.fg
                        } else {
                            theme.ely.fg_muted
                        },
                    ))
                    .on_mouse_down(MouseButton::Left, {
                        let entity = entity.clone();
                        move |_, _, cx| {
                            PanelView::set(view, cx);
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    })
            }))
    }

    fn render_no_worktree(&self, theme: Theme) -> impl IntoElement {
        div()
            .id("right-panel-no-worktree")
            .debug_selector(|| "right-panel-no-worktree".to_owned())
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme.spacing.card_gap)
            .p(theme.spacing.card_gap)
            .text_size(theme.typography.headline)
            .text_color(theme.ely.fg)
            .child(
                IconElement::new(
                    Icon::PanelRight,
                    IconSize::Custom(theme.typography.large_title),
                )
                .text_color(theme.ely.fg),
            )
            .child("No worktree selected")
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_subtle)
                    .child("Select a worktree to inspect its files and changes."),
            )
    }

    /// Builds the Diff view's `ChangesTab` if it is not built yet, and wires
    /// its events onto the channels the host already handles.
    fn ensure_changes(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<crate::changes::ChangesTab> {
        if let Some(changes) = self.changes.clone() {
            return changes;
        }
        let repo_root = self.repo_root.clone();
        let changes = cx.new(|cx| {
            crate::changes::ChangesTab::in_right_panel_with_git_capability(
                repo_root,
                self.project_is_git,
                cx,
            )
        });
        self.changes_subscriptions = vec![
            cx.subscribe(&changes, |_, _, event: &ChangesTabEvent, cx| match event {
                ChangesTabEvent::OpenFile(path) => cx.emit(RightPanelEvent::OpenFile(path.clone())),
            }),
            cx.subscribe(
                &changes,
                |_, _, event: &ChangesTabActionEvent, cx| match event {
                    ChangesTabActionEvent::OpenDiff(path) => {
                        cx.emit(RightPanelActionEvent::OpenDiff(path.clone()))
                    }
                    ChangesTabActionEvent::ResolveInTerminal(path) => {
                        cx.emit(RightPanelActionEvent::ResolveInTerminal(path.clone()))
                    }
                },
            ),
        ];
        self.changes = Some(changes.clone());
        changes
    }

    fn render_diff(&mut self, _theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex_1().min_h(px(0.0)).child(self.ensure_changes(cx))
    }

    fn ensure_history(&mut self, cx: &mut Context<Self>) -> gpui::Entity<GitHistory> {
        if let Some(history) = self.history.clone() {
            return history;
        }
        let history = cx.new(|cx| GitHistory::new(self.repo_root.clone(), cx));
        self.history_subscription = Some(cx.subscribe(
            &history,
            |_, _, event: &GitHistoryEvent, cx| match event {
                GitHistoryEvent::OpenCommit(sha) => {
                    cx.emit(RightPanelActionEvent::OpenCommit(sha.clone()))
                }
            },
        ));
        self.history = Some(history.clone());
        history
    }

    fn render_history(&mut self, _theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        // The view cannot measure its own container, so the host's resolved
        // width is handed down here, each render, with a no-op guard.
        let panel_width = self.panel_width;
        let history = self.ensure_history(cx);
        history.update(cx, |history, cx| {
            if history.panel_width != panel_width {
                history.panel_width = panel_width;
                cx.notify();
            }
        });
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(history)
    }

    fn ensure_references(&mut self, cx: &mut Context<Self>) -> gpui::Entity<ReferencesList> {
        if let Some(list) = self.references.clone() {
            return list;
        }
        let list = cx.new(|_| ReferencesList::new());
        self.references_subscription = Some(cx.subscribe(
            &list,
            |_, _, event: &ReferencesEvent, cx| match event {
                ReferencesEvent::Open { path, line } => {
                    cx.emit(RightPanelActionEvent::OpenAtLine {
                        path: path.clone(),
                        line: *line,
                    })
                }
            },
        ));
        self.references = Some(list.clone());
        list
    }

    fn render_references(&mut self, _theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.ensure_references(cx);
        div().flex_1().min_h(px(0.0)).flex().flex_col().child(list)
    }

    fn ensure_change_requests(&mut self, cx: &mut Context<Self>) -> gpui::Entity<ChangeRequestList> {
        if let Some(list) = self.change_requests.clone() {
            return list;
        }
        let list = cx.new(|cx| ChangeRequestList::new(self.repo_root.clone(), cx));
        self.change_requests_subscription = Some(cx.subscribe(
            &list,
            |_, _, event: &ChangeRequestListEvent, cx| match event {
                ChangeRequestListEvent::Open { reference, title } => {
                    cx.emit(RightPanelActionEvent::OpenChangeRequest {
                        reference: reference.clone(),
                        title: title.clone(),
                    })
                }
            },
        ));
        self.change_requests = Some(list.clone());
        list
    }

    fn render_change_requests(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.ensure_change_requests(cx);
        div().flex_1().min_h(px(0.0)).flex().flex_col().child(list)
    }

    /// On screen: the panel is drawn (the host pushes a width of 0 when it
    /// hides it), a worktree is selected, and the view is chosen.
    fn sync_change_requests_visibility(&mut self, cx: &mut Context<Self>) {
        let visible = self.panel_width > 0.0
            && self.worktree_selected
            && self.project_is_git
            && PanelView::get(cx) == PanelView::ChangeRequests;
        if let Some(list) = self.change_requests.clone() {
            list.update(cx, |list, cx| list.set_visible(visible, cx));
        }
    }

    /// Selects the view and makes it load now, drawn or not: the socket's
    /// `show` verb calls this, and a headless instance draws no frame for
    /// `sync_change_requests_visibility` to run in. A later frame with the
    /// panel hidden still hides it.
    pub fn show_change_requests(&mut self, cx: &mut Context<Self>) {
        PanelView::set(PanelView::ChangeRequests, cx);
        let list = self.ensure_change_requests(cx);
        list.update(cx, |list, cx| list.set_visible(true, cx));
        cx.notify();
    }

    pub fn change_requests_report(&self, cx: &App) -> Option<Vec<(String, String)>> {
        self.change_requests.as_ref().map(|list| list.read(cx).report(cx))
    }

    pub fn set_change_request_filter(&mut self, filter: sirio_forge::Filter, cx: &mut Context<Self>) -> bool {
        let Some(list) = self.change_requests.clone() else {
            return false;
        };
        list.update(cx, |list, cx| list.set_filter(filter, cx));
        true
    }

    pub fn change_request_for(&self, number: u64, cx: &App) -> Option<(sirio_forge::ChangeRef, String)> {
        self.change_requests.as_ref().and_then(|list| list.read(cx).reference_for(number))
    }

    /// Reads the list again now: a write just changed what it shows.
    pub fn refresh_change_requests(&mut self, cx: &mut Context<Self>) {
        if let Some(list) = self.change_requests.clone() {
            list.update(cx, |list, cx| list.refresh(cx));
        }
    }

    pub fn reconnect_change_requests(&mut self, cx: &mut Context<Self>) {
        if let Some(list) = self.change_requests.clone() {
            list.update(cx, |list, cx| list.reconnect(cx));
        }
    }

    /// The host's way in. Building the list if it does not exist yet is
    /// deliberate: the answer can arrive before the user has ever looked at
    /// this surface, and dropping it then would lose the search they asked
    /// for.
    pub fn set_references(&mut self, state: ReferencesState, cx: &mut Context<Self>) {
        let list = self.ensure_references(cx);
        list.update(cx, |list, cx| list.set_state(state, cx));
        cx.notify();
    }
}

impl EventEmitter<RightPanelEvent> for RightPanel {}
impl EventEmitter<RightPanelActionEvent> for RightPanel {}

impl Render for RightPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("RightPanel.render", cx.entity_id().as_u64());
        let theme = Theme::get(cx).with_sidebar_typography();
        let view = PanelView::get(cx);
        // The selected view is a global, and nothing here observes it, so
        // switching surfaces schedules no frame of its own. Each surface
        // other than Files is built lazily *during* a render in that view
        // (`ensure_history`, `ensure_diff`), so without this the newly
        // selected one is never constructed and the panel draws the old
        // surface until something unrelated happens to notify. It used to
        // survive on the Files refresh's own `notify` arriving a moment
        // later, which stopped being true once the refresh became
        // Files-only. Guarded by the change, so it costs one extra frame
        // per switch and cannot re-arm itself.
        if self.last_panel_view != Some(view) {
            self.last_panel_view = Some(view);
            cx.notify();
        }
        if self.worktree_selected {
            let showing_files = view == PanelView::Files;
            if showing_files {
                if !self.files_view_active && (self.is_stale || !self.settled) {
                    self.request_refresh(cx);
                }
                self.ensure_tree_refresh(cx);
            }
            self.files_view_active = showing_files;
        }
        if self.worktree_selected && self.file_focus.is_none() {
            self.file_focus = Some(cx.focus_handle().tab_stop(true));
        }
        let entity = cx.entity();
        let body = if !self.worktree_selected {
            self.render_no_worktree(theme).into_any_element()
        } else {
            match PanelView::get(cx) {
                PanelView::Files => self
                    .render_files(entity.clone(), theme, window, cx)
                    .into_any_element(),
                PanelView::Diff => self.render_diff(theme, cx).into_any_element(),
                PanelView::History => self.render_history(theme, cx).into_any_element(),
                PanelView::References => self.render_references(theme, cx).into_any_element(),
                PanelView::ChangeRequests if self.project_is_git => {
                    self.render_change_requests(cx).into_any_element()
                }
                PanelView::ChangeRequests => self.render_files(entity.clone(), theme, window, cx).into_any_element(),
            }
        };
        self.sync_change_requests_visibility(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .text_size(theme.typography.scaled(16.0))
            .overflow_hidden()
            .bg(theme.ely.bg)
            .child(self.render_header(entity.clone(), theme, cx))
            .child(body)
            .when(self.worktree_selected, |this| {
                this.when_some(self.file_context_menu.clone(), |this, menu| {
                    this.child(Self::render_file_context_menu(menu, entity.clone(), theme))
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-right-panel-module-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[gpui::test]
    fn the_history_view_renders_commit_rows_in_the_drawn_frame(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        git(&dir.0, &["init", "-q", "-b", "main"]);
        git(&dir.0, &["config", "user.email", "probe@sirio.test"]);
        git(&dir.0, &["config", "user.name", "probe"]);
        std::fs::write(dir.0.join("a.txt"), "a").expect("write file");
        git(&dir.0, &["add", "a.txt"]);
        git(&dir.0, &["commit", "-q", "-m", "initial"]);
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel =
            cx.update(|window, _cx| window.root::<RightPanel>().flatten().expect("panel root"));
        panel.update(&mut cx, |panel, cx| {
            panel.bind_worktree(dir.0.clone(), true, cx);
        });
        cx.update(|_window, cx| PanelView::set(PanelView::History, cx));
        cx.run_until_parked();

        let list = cx
            .debug_bounds("right-panel-history")
            .expect("the history list is drawn");
        assert!(
            f32::from(list.size.height) > 0.0,
            "the list must have real height, or no row is ever visible"
        );
        let row = cx
            .debug_bounds("history-row")
            .expect("commit rows render in the History view");
        assert!(
            f32::from(row.size.height) > 0.0,
            "a commit row must be visible in the drawn frame"
        );
    }

    /// The Files rail icon never carries a spinner. It used to overlay
    /// `loading::compact` for as long as `walk_task` was in flight, and
    /// `ensure_tree_refresh` puts a walk in flight every second, so the icon
    /// blinked once a second for the duration of every `git status`. A walk
    /// over a settled tree is silent: the tree stays and the new listing
    /// lands in place.
    ///
    /// The in-flight state is faked with a task that never completes: a
    /// real walk finishes inside `run_until_parked`, so the frame drawn
    /// afterwards would be the settled one and prove nothing.
    #[gpui::test]
    fn a_walk_in_flight_draws_no_spinner_on_the_files_rail_icon(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        std::fs::write(dir.0.join("a.txt"), "a").expect("write file");
        let window = cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel =
            cx.update(|window, _cx| window.root::<RightPanel>().flatten().expect("panel root"));
        panel.update(&mut cx, |panel, cx| {
            panel.bind_worktree(dir.0.clone(), true, cx);
        });
        cx.update(|_window, cx| PanelView::set(PanelView::Files, cx));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("files-refresh-spinner").is_none(),
            "a settled rail carries no spinner"
        );

        panel.update(&mut cx, |panel, cx| {
            panel.walk_task = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            panel.read_with(&cx, |panel, _| panel.walk_task.is_some()),
            "the walk is still in flight in the drawn frame"
        );
        assert!(
            cx.debug_bounds("files-refresh-spinner").is_none(),
            "a walk in flight must not draw a spinner on the rail icon"
        );
        assert!(
            cx.debug_bounds(PanelView::Files.element_id()).is_some(),
            "the rail icon itself stays drawn"
        );
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    fn rebinding_a_worktree_drops_the_changes_entity(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let other = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

        panel.update(cx, |panel, cx| {
            panel.ensure_changes(cx);
            panel.bind_worktree(other.0.clone(), true, cx);
            assert!(
                panel.changes.is_none(),
                "a stale checkout's diff must not survive"
            );
        });
    }

    #[gpui::test]
    fn rebinding_same_path_with_new_git_capability_rebuilds_changes(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let panel = cx.new(|_| {
            RightPanel::with_snapshot_for_project(dir.0.clone(), false, None)
        });

        let old_changes = panel.update(cx, |panel, cx| panel.ensure_changes(cx));
        assert!(!old_changes.read_with(cx, |changes, _| changes.allows_staging()));

        panel.update(cx, |panel, cx| {
            panel.bind_worktree(dir.0.clone(), true, cx);
            assert!(
                panel.changes.is_none(),
                "a capability change must drop the cached Diff surface"
            );
            assert!(
                panel.changes_subscriptions.is_empty(),
                "a dropped Diff surface must release its subscriptions"
            );
        });

        let new_changes = panel.update(cx, |panel, cx| panel.ensure_changes(cx));
        assert_ne!(old_changes.entity_id(), new_changes.entity_id());
        assert!(new_changes.read_with(cx, |changes, _| changes.allows_staging()));
    }

    #[gpui::test]
    fn switching_away_during_a_refresh_does_not_show_blocking_files_loading(
        cx: &mut TestAppContext,
    ) {
        cx.update(sirio_theme::Theme::init);
        let outgoing = TempDir::new();
        let incoming = TempDir::new();
        let source = cx.new(|_| RightPanel::new(outgoing.0.clone()));

        let outgoing_snapshot = source.update(cx, |panel, _| {
            panel.settled = true;
            panel.updating = true;
            panel.files_snapshot()
        });
        assert!(
            outgoing_snapshot.is_some(),
            "a settled tree remains cacheable while it refreshes"
        );

        let window = cx.add_window(|_window, _cx| {
            let mut panel = RightPanel::with_snapshot(incoming.0.clone(), outgoing_snapshot);
            panel.refresh_started = true;
            panel
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("files-loading").is_none(),
            "switching worktrees must never show the blocking first-load placeholder"
        );
    }

    #[gpui::test]
    fn an_empty_stale_tree_is_not_cached_as_a_visible_files_snapshot(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let directory = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(directory.0.clone()));

        panel.update(cx, |panel, _| {
            panel.settled = true;
            panel.is_stale = true;
            panel.updating = true;
            assert!(panel.files_snapshot().is_none());
        });
    }
}

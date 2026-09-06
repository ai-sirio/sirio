//! The checkout right panel: filesystem browsing, git changes, and activity.
//!
//! `mod.rs` owns the panel's lifecycle, its header, and the choice of
//! surface below it; the Files and Activity surfaces live in their
//! sibling modules (`files`, `activity`).
//!
//! The data model deliberately stays local to this panel. Git operations are
//! delegated to `sirio_git`; the host application can later replace the
//! refresh callbacks with its project store without changing the row layout.

mod activity;
mod files;
mod history;
mod history_toolbar;

use gpui::{
    App, Context, EventEmitter, FocusHandle, MouseButton, Render, Task, Window, div, prelude::*, px,
};
use sirio_theme::Theme;
use std::path::PathBuf;

use crate::changes::{ChangesTabActionEvent, ChangesTabEvent};
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use history::{GitHistory, GitHistoryEvent};

// ROW_HEIGHT stays reachable at the module root for the conformance
// suite (`crate::right_panel::ROW_HEIGHT`) even though the file-tree
// rows that use it live in `files`. Only that test module reads it.
#[cfg(test)]
pub(crate) use files::ROW_HEIGHT;

const HEADER_HEIGHT: f32 = 40.0;
/// Two-line activity row: 5 + 18 + 2 + 15 + 5, waku's card math.
const ACTIVITY_ROW_HEIGHT: f32 = 48.0;
/// Status shown at the trailing edge of an activity row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStatus {
    /// The surface is waiting for input.
    Idle,
    /// The surface is currently running.
    Running,
    /// The agent is blocked on a user decision or answer.
    NeedsInput,
    /// The surface completed successfully.
    Done,
    /// The surface reported an error.
    Error,
}

/// The colour an [`ActivityStatus`] is painted in, wherever it is drawn.
///
/// The right panel's activity rows and the tab strip's status glyphs show the
/// same five states, and used to carry two copies of this table — one here and
/// one in the app crate. Two copies of a colour table is one edit away from two
/// different colours for the same state, so there is now one.
///
/// Only the states that want something from the reader are coloured. Running is
/// the ordinary case and reads as the bright text neutral; idle is the same
/// neutral turned down. Amber, green and red are kept for "answer me", "this
/// finished" and "this broke".
pub fn status_color(status: ActivityStatus, theme: Theme) -> gpui::Rgba {
    match status {
        ActivityStatus::Idle => theme.text_faint,
        ActivityStatus::Running => theme.text,
        ActivityStatus::NeedsInput => theme.warning,
        ActivityStatus::Done => theme.success,
        ActivityStatus::Error => theme.danger,
    }
}

/// User actions originating from an activity row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPanelEvent {
    /// Select the open tab at this activity index.
    SelectActivity(usize),
    /// Close the open tab at this activity index.
    CloseActivity(usize),
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
}

/// A surface shown in the Activity section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySurface {
    /// The typed surface icon from the shared embedded icon set.
    pub icon: Icon,
    /// Surface title.
    pub title: String,
    /// Project/worktree subtitle.
    pub location: String,
    /// Current surface status.
    pub status: ActivityStatus,
}

impl ActivitySurface {
    /// Build one activity row.
    #[must_use]
    pub fn new(
        icon: Icon,
        title: impl Into<String>,
        location: impl Into<String>,
        status: ActivityStatus,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            location: location.into(),
            status,
        }
    }
}

/// Which view the right panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelView {
    #[default]
    Files,
    Activity,
    Diff,
    History,
}

/// App-wide selection. A GPUI global rather than a field, for the same
/// reason `DiffViewMode` is one: `select_worktree` throws the whole
/// `RightPanel` entity away and builds a fresh one, so a field would snap
/// back to Files on every worktree switch.
struct PanelViewSetting(PanelView);

impl gpui::Global for PanelViewSetting {}

impl PanelView {
    /// Rail order, left to right.
    const ORDER: [PanelView; 4] = [
        PanelView::Files,
        PanelView::Activity,
        PanelView::Diff,
        PanelView::History,
    ];

    fn icon(self) -> Icon {
        match self {
            PanelView::Files => Icon::FileTree,
            PanelView::Activity => Icon::Thread,
            PanelView::Diff => Icon::Diff,
            PanelView::History => Icon::GitGraph,
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            PanelView::Files => "right-panel-tab-files",
            PanelView::Activity => "right-panel-tab-activity",
            PanelView::Diff => "right-panel-tab-diff",
            PanelView::History => "right-panel-tab-history",
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

/// The GPUI right panel: the filesystem tree and the activity section.
pub struct RightPanel {
    repo_root: PathBuf,
    /// Filesystem roots the panel may inspect. Startup supplies the project
    /// roots and linked worktrees; a panel created for one runtime selection
    /// uses that worktree as its only root.
    allowed_roots: Vec<PathBuf>,
    /// A panel is constructed for a selected checkout. Closing that checkout
    /// must explicitly revoke the binding; retaining its path would make the
    /// Files/Changes surface look current while serving stale data.
    worktree_selected: bool,
    file_tree: Vec<files::FileNode>,
    git_markers: files::GitMarkers,
    activity: Vec<ActivitySurface>,
    /// The in-flight folder-expansion walk, if any. Replaced (never
    /// queued) on every new expansion request.
    walk_task: Option<Task<()>>,
    /// Whether the top-level tree refresh is currently in flight. This is
    /// the single-flight guard. It is deliberately *not* the loading state
    /// rendered to users — see `settled`.
    refresh_started: bool,
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
    refresh_error: Option<String>,
    selected_path: Option<PathBuf>,
    file_focus: Option<FocusHandle>,
    /// Bumped on every walk request (and on collapse): a walk that
    /// completes after a newer one was requested must not apply its
    /// result late.
    walk_generation: u64,
    /// One polling loop per panel, armed on first render.
    refresh_loop_started: bool,
    /// Counts frames this panel has actually been drawn in (#189).
    ///
    /// `ensure_tree_refresh`'s tick does two walks that both scale with the
    /// repository -- git's `--untracked-files=all` scan and `read_tree`'s
    /// recursive descent -- and neither is worth doing when nothing is
    /// drawing the panel. This counter is what "nothing is drawing it"
    /// means, and it is deliberately not `Window::is_window_active()`:
    /// measured on Windows, that reads `true` for a *minimised* window,
    /// because `render` stops being called and the last polled value simply
    /// goes stale. A count of real draws cannot go stale that way.
    ///
    /// It cannot deadlock either, which a naive "have we drawn lately"
    /// guard would: while minimised, the walk's own `cx.notify()` was
    /// observed to produce no draw at all, so suspending the walk removes
    /// no draw that would otherwise have happened. When the window comes
    /// back, GPUI resumes drawing on its own and the next tick sees the
    /// count move.
    renders: u64,
    /// The `renders` value the previous tick saw. Equal means no draw
    /// happened in between, so this tick skips both walks.
    renders_at_last_tick: u64,
    /// Set when a tick skipped the walks, so the resumed panel refreshes at
    /// once rather than showing a stale tree for up to a second.
    refresh_suspended: bool,
    /// Consecutive ticks skipped because nothing drew this panel (#193).
    ///
    /// A gate that can only be released by a draw can suspend *forever*
    /// when no draw is ever coming -- a panel driven outside a window, or
    /// any future path that stops drawing without dropping the entity. That
    /// is a liveness bug, not a saving, and the sibling gate on the Changes
    /// surface hit exactly it: a test driving that loop with no window at
    /// all never refreshed again. After `SUSPENDED_TICK_BUDGET` skipped
    /// ticks one walk runs regardless, bounding staleness for anything
    /// still live.
    suspended_ticks: u32,
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
    /// The resolved right-panel width, pushed in by the host every render.
    /// The History toolbar shapes itself from it; see [`GitHistory::panel_width`].
    panel_width: f32,
}

impl RightPanel {
    /// Creates the panel for one checkout.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        Self {
            repo_root: repo_root.clone(),
            allowed_roots: vec![repo_root],
            worktree_selected: true,
            file_tree: Vec::new(),
            git_markers: files::GitMarkers::default(),
            activity: Vec::new(),
            walk_task: None,
            refresh_started: false,
            settled: false,
            refresh_error: None,
            selected_path: None,
            file_focus: None,
            walk_generation: 0,
            refresh_loop_started: false,
            renders: 0,
            renders_at_last_tick: 0,
            refresh_suspended: false,
            suspended_ticks: 0,
            file_context_menu: None,
            changes: None,
            changes_subscriptions: Vec::new(),
            history: None,
            history_subscription: None,
            panel_width: 405.0,
        }
    }

    /// Creates the panel with an initial activity section.
    pub fn with_activity(repo_root: impl Into<PathBuf>, activity: Vec<ActivitySurface>) -> Self {
        Self {
            activity,
            ..Self::new(repo_root)
        }
    }

    /// Creates the startup panel with the project catalog's filesystem
    /// allowlist. A stale restored directory is left unselected until the
    /// host reconciles it with a current catalog worktree.
    pub fn with_activity_and_roots(
        repo_root: impl Into<PathBuf>,
        allowed_roots: Vec<PathBuf>,
        activity: Vec<ActivitySurface>,
    ) -> Self {
        let repo_root = repo_root.into();
        let worktree_selected = allowed_roots.iter().any(|root| root == &repo_root);
        let mut panel = Self::with_activity(repo_root, activity);
        panel.allowed_roots = allowed_roots;
        panel.worktree_selected = worktree_selected;
        panel
    }

    /// Replace the host-provided activity rows. The host (`main.rs`) calls
    /// this every render to stay live with `Chat`'s own state; it no-ops on
    /// an unchanged value to avoid notifying every render.
    pub fn set_activity(&mut self, activity: Vec<ActivitySurface>, cx: &mut Context<Self>) {
        if self.activity == activity {
            return;
        }
        self.activity = activity;
        cx.notify();
    }

    /// The host pushes the resolved right-panel width every render; same
    /// every-render push as [`RightPanel::set_activity`], no-op when
    /// unchanged so a drag does not notify more than it must.
    pub fn set_panel_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.panel_width == width {
            return;
        }
        self.panel_width = width;
        cx.notify();
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
        self.git_markers = files::GitMarkers::default();
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        self.changes = None;
        self.changes_subscriptions.clear();
        self.history = None;
        self.history_subscription = None;
        cx.notify();
    }

    /// The mirror of [`Self::clear_worktree`]: (re-)bind the panel to a
    /// genuinely selected checkout. F-CHG-02: `select_worktree` already
    /// covers a real, explicit worktree switch by throwing this whole entity
    /// away and building a fresh one via [`Self::with_activity`] -- but a
    /// worktree can also become the *current* one passively, e.g.
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
    pub fn bind_worktree(&mut self, repo_root: impl Into<PathBuf>, cx: &mut Context<Self>) {
        let repo_root = repo_root.into();
        if self.worktree_selected && self.repo_root == repo_root {
            return;
        }
        if !self.allowed_roots.iter().any(|root| root == &repo_root) {
            self.allowed_roots.push(repo_root.clone());
        }
        self.worktree_selected = true;
        self.repo_root = repo_root;
        self.file_tree.clear();
        self.git_markers = files::GitMarkers::default();
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        self.changes = None;
        self.changes_subscriptions.clear();
        self.history = None;
        self.history_subscription = None;
        self.settled = false;
        self.walk_generation += 1;
        cx.notify();
    }
}

impl RightPanel {
    /// The colour of the Activity rail badge, or `None` when nothing wants
    /// attention. Error outranks NeedsInput: one failed surface is the more
    /// urgent fact.
    pub(crate) fn activity_badge(&self) -> Option<ActivityStatus> {
        if self
            .activity
            .iter()
            .any(|row| row.status == ActivityStatus::Error)
        {
            return Some(ActivityStatus::Error);
        }
        if self
            .activity
            .iter()
            .any(|row| row.status == ActivityStatus::NeedsInput)
        {
            return Some(ActivityStatus::NeedsInput);
        }
        None
    }

    fn render_header(
        &self,
        entity: gpui::Entity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = PanelView::get(cx);
        let badge = self.activity_badge();
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .border_b_1()
            .border_color(theme.border)
            .children(PanelView::ORDER.map(|view| {
                let is_active = view == active;
                let badge_color = (view == PanelView::Activity)
                    .then_some(badge)
                    .flatten()
                    .map(|status| match status {
                        ActivityStatus::Error => theme.danger,
                        _ => theme.warning,
                    });
                div()
                    .id(view.element_id())
                    .debug_selector(move || view.element_id().to_owned())
                    .relative()
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .when(is_active, |this| this.bg(theme.element_hover))
                    .hover(|style| style.bg(theme.element_hover))
                    .child(IconElement::new(view.icon(), IconSize::Small).text_color(
                        if is_active {
                            theme.text
                        } else {
                            theme.text_muted
                        },
                    ))
                    // The Files icon never carries a spinner. It used to
                    // overlay `loading::compact` for as long as `walk_task`
                    // was in flight, and `ensure_tree_refresh` puts a walk
                    // in flight every second, so the icon blinked once a
                    // second for the duration of every `git status`. A walk
                    // over a settled tree is silent — the same rule the
                    // `settled` field applies to the tree itself.
                    .when_some(badge_color, |this, color| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(4.0))
                                .right(px(4.0))
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(color),
                        )
                    })
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
            .text_color(theme.text)
            .child(
                IconElement::new(
                    Icon::PanelRight,
                    IconSize::Custom(theme.typography.large_title),
                )
                .text_color(theme.text),
            )
            .child("No worktree selected")
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
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
        let changes = cx.new(|cx| crate::changes::ChangesTab::in_right_panel(repo_root, cx));
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
}

impl EventEmitter<RightPanelEvent> for RightPanel {}
impl EventEmitter<RightPanelActionEvent> for RightPanel {}

impl Render for RightPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        // #189: incremented here and nowhere else -- being *in* a drawn
        // frame is the whole signal.
        self.renders = self.renders.wrapping_add(1);
        if self.worktree_selected {
            self.ensure_tree_refresh(cx);
            // #191: resume only into the view the tree is actually for.
            // Without this clause the gate below would be undone on the
            // very next frame, because a panel showing History is still a
            // panel being drawn.
            if self.refresh_suspended && PanelView::get(cx) == PanelView::Files {
                self.refresh_suspended = false;
                self.refresh(cx);
            }
        }
        if self.worktree_selected && self.file_focus.is_none() {
            self.file_focus = Some(cx.focus_handle().tab_stop(true));
        }
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .overflow_hidden()
            .bg(theme.surface)
            .child(self.render_header(entity.clone(), theme, cx))
            .child(if !self.worktree_selected {
                self.render_no_worktree(theme).into_any_element()
            } else {
                match PanelView::get(cx) {
                    PanelView::Files => self
                        .render_files(entity.clone(), theme, window, cx)
                        .into_any_element(),
                    PanelView::Activity => self
                        .render_activity(entity.clone(), theme, window, cx)
                        .into_any_element(),
                    PanelView::Diff => self.render_diff(theme, cx).into_any_element(),
                    PanelView::History => self.render_history(theme, cx).into_any_element(),
                }
            })
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
    use gpui::{
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext,
        VisualTestContext,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    // ── F-EDIT-12 harness capability: payload drags ─────────────────────
    //
    // The inventory's drag entry needs a drop target in a *pane* — that
    // lives in the shell (`sirio/src/main.rs`), so the product half is
    // routed to codex12 (a file row becomes the drag source, a pane the
    // drop target). What this crate owns is the proof that the harness can
    // express a payload drag at all: a source element with `on_drag`, a
    // target with `on_drop`, and the real mouse-down / move / up sequence
    // between them. The shell's F-EDIT-12 test then uses exactly this
    // recipe against its workspace.

    /// The drag preview view GPUI requires from `on_drag`.
    struct EmptyDragPreview;

    impl Render for EmptyDragPreview {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    struct DragFixture {
        dropped: Rc<RefCell<Vec<PathBuf>>>,
    }

    impl DragFixture {
        fn new() -> Self {
            Self {
                dropped: Default::default(),
            }
        }
    }

    impl Render for DragFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let dropped = self.dropped.clone();
            div()
                .size_full()
                .flex()
                .child(
                    div()
                        .id("drag-source")
                        .w(px(100.0))
                        .h(px(100.0))
                        .debug_selector(|| "drag-source".into())
                        .on_drag(PathBuf::from("/repo/file.txt"), |_, _, _, cx| {
                            cx.new(|_| EmptyDragPreview)
                        }),
                )
                .child(
                    div()
                        .id("drag-target")
                        .w(px(100.0))
                        .h(px(100.0))
                        .debug_selector(|| "drag-target".into())
                        .on_drag_move(|_event: &gpui::DragMoveEvent<PathBuf>, _, _| {})
                        .on_drop(move |path: &PathBuf, _, _| {
                            dropped.borrow_mut().push(path.clone());
                        }),
                )
        }
    }

    #[gpui::test]
    async fn a_payload_drag_reaches_the_drop_target_through_real_mouse_events(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| DragFixture::new());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let fixture = cx.update(|window, _| {
            window
                .root::<DragFixture>()
                .flatten()
                .expect("fixture root")
        });

        let source = cx
            .debug_bounds("drag-source")
            .expect("the drag source is in the drawn frame");
        let target = cx
            .debug_bounds("drag-target")
            .expect("the drop target is in the drawn frame");

        // The real gesture: press on the source, move past the 2px drag
        // threshold (which starts the payload drag), move over the target,
        // release. No drag convenience method exists — this is the mouse
        // sequence GPUI itself uses, dispatched through the same window
        // event path as production input.
        cx.simulate_event(MouseDownEvent {
            position: source.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: gpui::point(source.center().x + px(30.0), source.center().y),
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

        let dropped = fixture.read_with(&cx.cx, |fixture, _| fixture.dropped.borrow().clone());
        assert_eq!(
            dropped,
            vec![PathBuf::from("/repo/file.txt")],
            "the drop target receives the drag payload through the real event path"
        );
    }

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
    fn the_rail_switches_the_selected_view(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);

        cx.update(|cx| {
            assert_eq!(PanelView::get(cx), PanelView::Files, "Files is the default");
            PanelView::set(PanelView::History, cx);
            assert_eq!(PanelView::get(cx), PanelView::History);
        });
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
            panel.bind_worktree(dir.0.clone(), cx);
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
            panel.bind_worktree(dir.0.clone(), cx);
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
    fn the_selection_survives_rebinding_to_another_worktree(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let other = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

        cx.update(|cx| PanelView::set(PanelView::Diff, cx));
        panel.update(cx, |panel, cx| panel.bind_worktree(other.0.clone(), cx));

        cx.update(|cx| assert_eq!(PanelView::get(cx), PanelView::Diff));
    }

    #[gpui::test]
    fn the_activity_badge_appears_only_for_attention_states(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

        let idle = ActivitySurface::new(Icon::SquareTerminal, "one", "", ActivityStatus::Idle);
        let waiting =
            ActivitySurface::new(Icon::SquareTerminal, "two", "", ActivityStatus::NeedsInput);

        panel.update(cx, |panel, cx| {
            panel.set_activity(vec![idle.clone()], cx);
            assert!(panel.activity_badge().is_none(), "idle rows raise no badge");
            panel.set_activity(vec![idle, waiting], cx);
            assert!(
                panel.activity_badge().is_some(),
                "a waiting agent raises a badge"
            );
        });
    }

    #[gpui::test]
    fn the_changes_entity_is_built_only_when_the_diff_view_is_selected(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

        panel.update(cx, |panel, _| {
            assert!(panel.changes.is_none(), "nothing built up front");
        });

        cx.update(|cx| PanelView::set(PanelView::Diff, cx));
        panel.update(cx, |panel, cx| {
            panel.ensure_changes(cx);
            assert!(panel.changes.is_some(), "selecting Diff builds it");
        });
    }

    #[gpui::test]
    fn rebinding_a_worktree_drops_the_changes_entity(cx: &mut TestAppContext) {
        cx.update(sirio_theme::Theme::init);
        let dir = TempDir::new();
        let other = TempDir::new();
        let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

        panel.update(cx, |panel, cx| {
            panel.ensure_changes(cx);
            panel.bind_worktree(other.0.clone(), cx);
            assert!(
                panel.changes.is_none(),
                "a stale checkout's diff must not survive"
            );
        });
    }
}

//! The checkout right panel: filesystem browsing, git changes, and activity.
//!
//! `mod.rs` owns the panel's lifecycle, its header, and the choice of
//! surface below it; the Files and Activity surfaces live in their
//! sibling modules (`files`, `activity`).
//!
//! The data model deliberately stays local to this panel. Git operations are
//! delegated to `tiller_git`; the host application can later replace the
//! refresh callbacks with its project store without changing the row layout.

mod activity;
mod files;

use gpui::{
    Context, EventEmitter, FocusHandle, FontWeight, Render, Task, Window, div, prelude::*,
    px,
};
use std::path::PathBuf;
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement, IconSize};

// ROW_HEIGHT stays reachable at the module root for the conformance
// suite (`crate::right_panel::ROW_HEIGHT`) even though the file-tree
// rows that use it live in `files`. Only that test module reads it.
#[cfg(test)]
pub(crate) use files::ROW_HEIGHT;

const PANEL_WIDTH: f32 = 405.0;
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



/// The GPUI right panel: the filesystem tree and the activity section.
pub struct RightPanel {
    repo_root: PathBuf,
    /// A panel is constructed for a selected checkout. Closing that checkout
    /// must explicitly revoke the binding; retaining its path would make the
    /// Files/Changes surface look current while serving stale data.
    worktree_selected: bool,
    file_tree: Vec<files::FileNode>,
    git_markers: files::GitMarkers,
    activity_expanded: bool,
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
    file_context_menu: Option<files::FileContextMenu>,
}


impl RightPanel {

    /// Creates the panel for one checkout.
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            worktree_selected: true,
            file_tree: Vec::new(),
            git_markers: files::GitMarkers::default(),
            activity_expanded: false,
            activity: Vec::new(),
            walk_task: None,
            refresh_started: false,
            settled: false,
            refresh_error: None,
            selected_path: None,
            file_focus: None,
            walk_generation: 0,
            refresh_loop_started: false,
            file_context_menu: None,
        }
    }

    /// Creates the panel with an initial activity section.
    pub fn with_activity(repo_root: impl Into<PathBuf>, activity: Vec<ActivitySurface>) -> Self {
        Self {
            activity,
            ..Self::new(repo_root)
        }
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
        cx.notify();
    }

    /// The mirror of [`Self::clear_worktree`]: (re-)bind the panel to a
    /// genuinely selected checkout. F-CHG-02: `select_worktree` already
    /// covers a real, explicit worktree switch by throwing this whole entity
    /// away and building a fresh one via [`Self::with_activity`] -- but a
    /// worktree can also become the *current* one passively, e.g.
    /// `sync_control_state` re-matching `working_directory` against a
    /// project the user just added over the control socket, with no dedicated
    /// switch call in between. `TillerWorkspace::sync_activity` -- already
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
        self.worktree_selected = true;
        self.repo_root = repo_root;
        self.file_tree.clear();
        self.git_markers = files::GitMarkers::default();
        self.selected_path = None;
        self.refresh_error = None;
        self.file_context_menu = None;
        self.settled = false;
        self.walk_generation += 1;
        cx.notify();
    }

}


impl RightPanel {

    fn render_header(&self, theme: Theme) -> impl IntoElement {
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .border_b_1()
            .border_color(theme.hairline)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.5))
                    .text_color(theme.title)
                    .child("Files"),
            )
            .child(
                div()
                    .id("close-right-panel")
                    .w(px(20.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(16.0))
                    .text_color(theme.subtitle)
                    .hover(|style| style.bg(theme.row_hover).rounded(px(4.0)))
                    .child(IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.title)),
            )
    }

}


impl EventEmitter<RightPanelEvent> for RightPanel {}
impl EventEmitter<RightPanelActionEvent> for RightPanel {}


impl Render for RightPanel {

    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        if self.worktree_selected {
            self.ensure_tree_refresh(cx);
        }
        if self.worktree_selected && self.file_focus.is_none() {
            self.file_focus = Some(cx.focus_handle().tab_stop(true));
        }
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .flex_col()
            .w(px(PANEL_WIDTH))
            .h_full()
            .overflow_hidden()
            .bg(theme.background)
            .child(self.render_header(theme))
            .child(if self.worktree_selected {
                self.render_files(entity.clone(), theme, cx)
                    .into_any_element()
            } else {
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
                    .text_color(theme.title)
                    .child(
                        IconElement::new(
                            Icon::PanelRight,
                            IconSize::Custom(theme.typography.large_title),
                        )
                        .text_color(theme.title),
                    )
                    .child("No worktree selected")
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.meta)
                            .child("Select a worktree to inspect its files and changes."),
                    )
                    .into_any_element()
            })
            .when(self.worktree_selected, |this| {
                this.when_some(self.file_context_menu.clone(), |this, menu| {
                    this.child(Self::render_file_context_menu(menu, entity.clone(), theme))
                })
            })
            .child(self.render_activity(entity, theme))
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

    // ── F-EDIT-12 harness capability: payload drags ─────────────────────
    //
    // The inventory's drag entry needs a drop target in a *pane* — that
    // lives in the shell (`tiller/src/main.rs`), so the product half is
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


}


//! The drawer under a file tab: its header, its height and what is open.
//!
//! One `WorktreeDrawer` belongs to a worktree and is drawn by every file tab
//! of that worktree, so switching tabs never changes it. The panel under the
//! header is an empty shell here: Problems and Search draw into it.
//!
//! Ways this can fail, written before the code:
//! - a height the user never chose reaches the database: a NaN or an infinity
//!   (the column is NOT NULL, and a NaN is stored as NULL), or a value outside
//!   the floor and the ceiling;
//! - a window shorter than the ceiling's arithmetic allows (tab height minus
//!   160 px is below the 96 px floor) panics `f32::clamp`, which refuses an
//!   inverted range; the floor must win;
//! - a tab name the database holds that this build does not know leaves the
//!   drawer on nothing, instead of Problems;
//! - a drag saves on every move, which is one write per frame;
//! - the drawer is saved from a drop the pointer never delivers (released
//!   outside the drawer), so the dragged height is lost at the next relaunch;
//! - a closed drawer draws a panel, or keeps its drag handle, so the code
//!   loses rows to a drawer that is only a header.

use std::cell::Cell;
use std::ops::Range;
use std::path::PathBuf;

use ely_gpui_component::forms::Choice;
use ely_gpui_component::navigation::Tabs;
use ely_gpui_component::primitives::IconName;
use gpui::{Context, DragMoveEvent, EventEmitter, MouseButton, Pixels, Render, Window, div, prelude::*, px};
use sirio_persistence::EditorDrawerRecord;
use sirio_theme::Theme;

use super::{ErrorAlarm, ProblemCounts, ProblemStore};
use crate::ely_ui;

/// The height a worktree's drawer opens to the first time.
const DEFAULT_HEIGHT: f32 = 240.0;
/// The least an open drawer is tall: its header and three rows.
const MIN_HEIGHT: f32 = 96.0;
/// The room the code keeps above an open drawer.
const CODE_KEEPS: f32 = 160.0;
/// A closed drawer is its header row only.
const HEADER_HEIGHT: f32 = 30.0;
/// The strip at the drawer's top edge that takes the resize drag.
const EDGE_HEIGHT: f32 = 6.0;

/// What the drawer's panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawerTab {
    Problems,
    Search,
}

impl DrawerTab {
    /// The word the database and the control verbs use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Problems => "problems",
            Self::Search => "search",
        }
    }

    /// The tab a stored or chosen word names; a word this build does not know
    /// is Problems.
    pub fn parse(word: &str) -> Self {
        match word {
            "search" => Self::Search,
            _ => Self::Problems,
        }
    }
}

/// What the drawer tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreeDrawerEvent {
    /// The state to keep changed: save it.
    Changed,
    /// A row of Problems was chosen: open `path` there (0-based line, UTF-16
    /// column, as the protocol counts).
    OpenProblem { path: PathBuf, line: u32, column: u32 },
    /// A hit of Search was chosen: open `path` with `range` (bytes) selected.
    OpenHit { path: PathBuf, range: Range<usize> },
}

/// The payload of a drag of the drawer's top edge.
#[derive(Clone, Copy)]
struct DraggedDrawerEdge;

/// Where a drag took hold: the pointer's y and the height then.
#[derive(Clone, Copy)]
struct DragAnchor {
    pointer_y: f32,
    height: f32,
}

pub struct WorktreeDrawer {
    worktree: PathBuf,
    open: bool,
    height: Pixels,
    tab: DrawerTab,
    alarm: ErrorAlarm,
    problems: ProblemStore,
    /// The file tab's height as it was last drawn, which caps the drawer. Written
    /// while the tab is laid out, so it is a `Cell`; unknown until then.
    tab_height: Cell<Pixels>,
    drag: Option<DragAnchor>,
    drag_moved: bool,
}

/// `height` held between the floor and the ceiling for a tab `tab_height`
/// tall. A tab too short for both leaves the floor, which wins; a height that
/// is not a number is the default.
fn clamp_height(height: Pixels, tab_height: Pixels) -> Pixels {
    let height = f32::from(height);
    let height = if height.is_finite() { height } else { DEFAULT_HEIGHT };
    let ceiling = f32::from(tab_height) - CODE_KEEPS;
    // `min` then `max`, never `clamp`: an inverted range must not panic.
    px(height.min(ceiling).max(MIN_HEIGHT))
}

impl WorktreeDrawer {
    /// The drawer of `worktree`, as `record` left it or closed on Problems at
    /// the default height.
    pub fn new(worktree: PathBuf, record: Option<&EditorDrawerRecord>) -> Self {
        let unknown = px(f32::INFINITY);
        let (open, height, tab, alarm) = match record {
            Some(record) => (
                record.open,
                clamp_height(px(record.height), unknown),
                DrawerTab::parse(&record.tab),
                ErrorAlarm::from_json(&record.seen_errors),
            ),
            None => (
                false,
                px(DEFAULT_HEIGHT),
                DrawerTab::Problems,
                ErrorAlarm::new(Default::default()),
            ),
        };
        Self {
            worktree,
            open,
            height,
            tab,
            alarm,
            problems: ProblemStore::default(),
            tab_height: Cell::new(unknown),
            drag: None,
            drag_moved: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn tab(&self) -> DrawerTab {
        self.tab
    }

    /// The height the drawer keeps, which is what is saved.
    pub fn height(&self) -> Pixels {
        self.height
    }

    /// The file tab's height as last drawn; unbounded before the first frame.
    pub fn tab_height(&self) -> Pixels {
        self.tab_height.get()
    }

    /// Records the file tab's height; true when it changed, so the caller asks
    /// for the frame that draws the drawer at the new cap.
    pub fn note_tab_height(&self, height: Pixels) -> bool {
        let changed = self.tab_height.get() != height;
        self.tab_height.set(height);
        changed
    }

    /// The height the drawer is drawn at: the kept height under the cap of the
    /// tab as it is now, or the header alone when closed.
    pub fn drawn_height(&self) -> Pixels {
        if self.open {
            clamp_height(self.height, self.tab_height.get())
        } else {
            px(HEADER_HEIGHT)
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        self.changed(cx);
    }

    /// Opens the drawer on `tab`.
    pub fn show(&mut self, tab: DrawerTab, cx: &mut Context<Self>) {
        if self.open && self.tab == tab {
            return;
        }
        self.open = true;
        self.tab = tab;
        self.changed(cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.changed(cx);
    }

    /// Keeps `px` as the height, between the floor and `tab_height` less the
    /// room the code keeps.
    pub fn set_height(&mut self, px: Pixels, tab_height: Pixels, cx: &mut Context<Self>) {
        let next = clamp_height(px, tab_height);
        if next == self.height {
            return;
        }
        self.height = next;
        self.changed(cx);
    }

    /// What is saved for this worktree.
    pub fn record(&self) -> EditorDrawerRecord {
        EditorDrawerRecord {
            worktree_path: self.worktree.to_string_lossy().into_owned(),
            open: self.open,
            height: f32::from(self.height),
            tab: self.tab.as_str().to_owned(),
            seen_errors: self.alarm.to_json(),
        }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(WorktreeDrawerEvent::Changed);
        cx.notify();
    }

    fn begin_drag(&mut self, pointer_y: f32) {
        self.drag = Some(DragAnchor {
            pointer_y,
            height: f32::from(self.drawn_height()),
        });
        self.drag_moved = false;
    }

    /// The pointer is at `pointer_y` mid-drag: the height follows it, drawn
    /// but not saved (the drop saves).
    fn drag_to(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        let Some(anchor) = self.drag else {
            return;
        };
        let wanted = anchor.height - (pointer_y - anchor.pointer_y);
        if !wanted.is_finite() {
            return;
        }
        // Whole pixels: a pointer's fractions are noise, and the saved height
        // reads the same after a relaunch.
        let next = clamp_height(px(wanted.round()), self.tab_height.get());
        // Sub-pixel jitter would redraw every frame for nothing.
        if (f32::from(next) - f32::from(self.height)).abs() < 0.5 {
            return;
        }
        self.height = next;
        self.drag_moved = true;
        cx.notify();
    }

    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        self.drag = None;
        if std::mem::take(&mut self.drag_moved) {
            cx.emit(WorktreeDrawerEvent::Changed);
        }
    }
}

impl EventEmitter<WorktreeDrawerEvent> for WorktreeDrawer {}

/// `Problems`, then the counts that are not zero: `⊘ 3 ⚠ 5 ⓘ 2`.
fn problems_label(counts: &ProblemCounts) -> String {
    let mut label = String::from("Problems");
    for (mark, count) in [("⊘", counts.errors), ("⚠", counts.warnings), ("ⓘ", counts.notes)] {
        if count > 0 {
            label.push_str(&format!(" {mark} {count}"));
        }
    }
    label
}

impl Render for WorktreeDrawer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _render = sirio_perf::span("WorktreeDrawer.render", cx.entity_id().as_u64());
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let open = self.open;
        let tabs = Tabs::new(
            "drawer-tabs",
            [
                Choice::new("problems", problems_label(&self.problems.counts())),
                Choice::new("search", "Search"),
            ],
            self.tab.as_str(),
        )
        .on_change({
            let entity = entity.clone();
            move |value, _, cx| {
                let tab = DrawerTab::parse(value);
                entity.update(cx, |drawer, cx| drawer.show(tab, cx));
            }
        });
        let hint = if cfg!(target_os = "macos") { "⌘J" } else { "Ctrl+J" };
        let header = div()
            .id("file-drawer-header")
            .debug_selector(|| "file-drawer-header".to_owned())
            .flex_none()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(tabs))
            .child(
                div()
                    .flex_none()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_subtle)
                    .child(hint),
            )
            .when(open, |this| {
                let entity = entity.clone();
                this.child(ely_ui::icon_button(
                    "file-drawer-close",
                    IconName::X,
                    "Close",
                    true,
                    move |_, cx| entity.update(cx, |drawer, cx| drawer.close(cx)),
                ))
            });
        // Empty until Problems and Search draw into it.
        let panel = div()
            .id("file-drawer-panel")
            .debug_selector(|| "file-drawer-panel".to_owned())
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_hidden()
            .bg(theme.ely.bg);
        let edge = div()
            .id("file-drawer-edge")
            .debug_selector(|| "file-drawer-edge".to_owned())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(EDGE_HEIGHT))
            .cursor_row_resize()
            .on_mouse_down(MouseButton::Left, {
                let entity = entity.clone();
                move |event, _, cx| {
                    let pointer_y = f32::from(event.position.y);
                    entity.update(cx, |drawer, _| drawer.begin_drag(pointer_y));
                }
            })
            // A press on the edge released with no drag is not a drop: this ends
            // it, so the anchor never outlives its press.
            .on_mouse_up(MouseButton::Left, {
                let entity = entity.clone();
                move |_, _, cx| entity.update(cx, |drawer, cx| drawer.finish_drag(cx))
            })
            .on_drag(DraggedDrawerEdge, |_, _, _, cx| cx.new(|_| gpui::Empty));
        div()
            .id("file-drawer")
            .debug_selector(|| "file-drawer".to_owned())
            .relative()
            .flex_none()
            .w_full()
            .h(self.drawn_height())
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme.ely.border)
            .bg(theme.ely.surface)
            // The file editor around the drawer takes the focus on a left press
            // and opens the code's menu on a right one. A press in the drawer is
            // not the code's: the children (the edge, the tabs) have run by the
            // time it bubbles here.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_drag_move::<DraggedDrawerEdge>(cx.listener(
                |drawer, event: &DragMoveEvent<DraggedDrawerEdge>, _, cx| {
                    drawer.drag_to(f32::from(event.event.position.y), cx);
                },
            ))
            // A release over the drawer drops; one outside it never reaches
            // `on_drop`, so it ends the drag here too.
            .on_drop::<DraggedDrawerEdge>(cx.listener(|drawer, _, _, cx| drawer.finish_drag(cx)))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|drawer, _, _, cx| drawer.finish_drag(cx)),
            )
            .child(header)
            .when(open, |this| this.child(panel).child(edge))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_wins_in_a_window_too_short_for_both_bounds() {
        assert_eq!(clamp_height(px(300.0), px(200.0)), px(MIN_HEIGHT));
    }

    #[test]
    fn a_height_that_is_not_a_number_is_the_default() {
        assert_eq!(clamp_height(px(f32::NAN), px(2000.0)), px(DEFAULT_HEIGHT));
        assert_eq!(clamp_height(px(f32::INFINITY), px(2000.0)), px(DEFAULT_HEIGHT));
    }

    #[test]
    fn the_height_stays_between_the_floor_and_the_room_the_code_keeps() {
        assert_eq!(clamp_height(px(10.0), px(800.0)), px(MIN_HEIGHT));
        assert_eq!(clamp_height(px(700.0), px(800.0)), px(640.0));
        assert_eq!(clamp_height(px(300.0), px(800.0)), px(300.0));
    }

    #[test]
    fn a_tab_not_measured_yet_caps_nothing() {
        assert_eq!(clamp_height(px(5000.0), px(f32::INFINITY)), px(5000.0));
    }

    #[test]
    fn a_tab_name_this_build_does_not_know_is_problems() {
        assert_eq!(DrawerTab::parse("output"), DrawerTab::Problems);
        assert_eq!(DrawerTab::parse(DrawerTab::Search.as_str()), DrawerTab::Search);
    }

    #[test]
    fn the_title_names_the_counts_that_are_not_zero() {
        let counts = ProblemCounts { errors: 3, warnings: 0, notes: 2 };
        assert_eq!(problems_label(&counts), "Problems ⊘ 3 ⓘ 2");
        assert_eq!(problems_label(&ProblemCounts::default()), "Problems");
    }
}

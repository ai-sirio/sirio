//! What the debug verb `surface.files.read` reports about the Files tree.
//!
//! Everything that reads the rows goes through [`RightPanel::files_facts`]:
//! the row builder behind it can change without the verb's shape changing.

use std::cell::{Cell, RefCell};
#[cfg(debug_assertions)]
use std::path::Path;
use std::rc::Rc;

use gpui::{App, Bounds, Pixels};
#[cfg(debug_assertions)]
use gpui::{canvas, prelude::*};

use super::super::{PanelView, RightPanel, ShowHiddenFilesSetting};

pub(in super::super) type BoundsCell = Rc<Cell<Option<Bounds<Pixels>>>>;
/// The rows the last frame prepainted, in drawn order, each with the bounds
/// it was prepainted at, so a row's label and its coordinates always come
/// from one frame.
pub(in super::super) type RowBounds = Rc<RefCell<Vec<FileRowFacts>>>;

/// What the debug verb `surface.files.read` reports. A bounds field is
/// `None` while that element is not drawn.
#[derive(Clone, Debug, Default)]
pub struct FilesFacts {
    /// The Files view is the panel's current view, and the panel is drawn.
    pub shown: bool,
    /// The first walk has landed.
    pub settled: bool,
    /// The first walk is still running.
    pub updating: bool,
    /// The tree on screen may be older than the disk (a change was seen, or
    /// the last walk failed).
    pub stale: bool,
    pub show_hidden: bool,
    /// How many refreshes have landed, whatever their result.
    pub refreshes: u64,
    /// The `refreshes` count the last frame that drew the rows had seen.
    pub drawn_refreshes: u64,
    /// The clickable hidden-files toggle, not its footer container.
    pub hidden_switch_bounds: Option<Bounds<Pixels>>,
    /// Every row label built since the previous read.
    pub seen: Vec<String>,
    /// The rows on screen, in drawn order.
    pub rows: Vec<FileRowFacts>,
}

#[derive(Clone, Debug)]
pub struct FileRowFacts {
    pub label: String,
    /// The row's leaf, relative to the worktree, with `/` separators.
    pub path: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub selected: bool,
    pub status: Option<&'static str>,
    pub bounds: Option<Bounds<Pixels>>,
}

/// A row's own absolute path, spelled relative to the worktree with `/`.
#[cfg(debug_assertions)]
pub(super) fn relative_slash(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

impl RightPanel {
    /// What the debug verb reports about the Files tree. It takes the
    /// built-label record: each read returns what was built since the last.
    pub fn files_facts(&self, cx: &App) -> FilesFacts {
        let show_hidden = ShowHiddenFilesSetting::get(cx);
        let shown = self.panel_width > 0.0 && self.last_panel_view == Some(PanelView::Files);
        #[cfg(debug_assertions)]
        let seen: Vec<String> = std::mem::take(&mut *self.built_labels.borrow_mut())
            .into_iter()
            .collect();
        #[cfg(not(debug_assertions))]
        let seen = Vec::new();
        let rows = if shown {
            self.file_row_bounds.borrow().clone()
        } else {
            Vec::new()
        };
        FilesFacts {
            shown,
            settled: self.settled,
            updating: self.updating,
            stale: self.is_stale,
            show_hidden,
            refreshes: self.refreshes_landed,
            drawn_refreshes: self.drawn_refreshes.get(),
            hidden_switch_bounds: if shown {
                self.hidden_switch_bounds.get()
            } else {
                None
            },
            seen,
            rows,
        }
    }

    /// Records the label of every row a rebuild produced.
    pub(super) fn record_built_labels(&self) {
        #[cfg(debug_assertions)]
        {
            let mut seen = self.built_labels.borrow_mut();
            for row in &self.flattened_file_rows {
                seen.insert(row.node.name.clone());
            }
        }
    }
}

/// A probe that, at prepaint, records `facts` with its bounds. Only rows
/// that are really drawn reach prepaint (the list builds row 0 extra times
/// just to measure it), so the record holds exactly the drawn rows, top to
/// bottom. Debug builds only.
#[cfg(debug_assertions)]
pub(super) fn row_probe(rows: &RowBounds, mut facts: FileRowFacts) -> impl gpui::IntoElement {
    let rows = rows.clone();
    canvas(
        move |bounds, _, _| {
            facts.bounds = Some(bounds);
            rows.borrow_mut().push(facts);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The same, into one cell. Debug builds only.
pub(super) fn cell_probe(cell: &BoundsCell) -> Option<impl gpui::IntoElement> {
    #[cfg(debug_assertions)]
    {
        Some(super::super::prompts::bounds_probe(cell))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = cell;
        None::<gpui::Empty>
    }
}

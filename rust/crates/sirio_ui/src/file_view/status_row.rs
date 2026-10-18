//! The row under a file: the branch and the language server on the left; the
//! caret's place, the indent, the encoding, the line ending, the language and
//! the bell on the right, each one an Ely status item.
//!
//! The row only shows, for now: no item takes a press. Three of its facts come
//! from the file (the caret, the line ending, the language) and are read as the
//! row is drawn. The rest are the app's to know (`StatusFacts`: the branch, the
//! worktree's state, how far it is from its upstream, the language server) and
//! are pushed into the view, which draws them as they were last pushed.
//!
//! What the row shows when there is less to show:
//! - A snapshot has no branch of its own (it is a revision) and no server, so
//!   those two items are absent; the rest describe the text and are drawn.
//! - Markdown Preview hides the code editor, so a caret there is not what the
//!   reader looks at: the cursor item is absent. The indent, the line ending
//!   and the language still describe the file and stay.
//! - A language with no running server draws no server item, ever: a missing
//!   server is not volunteered (a row with a "no server" mark would sit on
//!   nearly every file).
//! - Until the file has loaded and drawn, there is no row.
//! - A row too narrow for every item draws the ones that matter at a glance and
//!   leaves out the indent, the encoding and the line ending; the state still
//!   reads them. The row measures itself (`StatusLayout`): it is full while the
//!   left group's natural width and the right group's natural width fit side by
//!   side, and compact from the frame after they stop fitting. A file beside a
//!   terminal and the right panel is that narrow.
//! - A branch name longer than `BRANCH_CHARS` is drawn shortened in the middle
//!   (`feature/a-ve…the-status-row`), so the server item after it is never
//!   pushed out of the row by a long name. The state reads the whole name.

use ely_gpui_component::editor::{
    BranchIndicator, CursorPosition, Encoding, IndentSettings, LanguageMode,
    LineEnding as ElyLineEnding, LspStatus, NotificationBell,
};
use gpui::{AnyElement, App, Context, Pixels, SharedString, canvas, prelude::*, px};
use sirio_theme::Theme;
use std::cell::Cell;

use super::{FileView, LspState, MarkdownMode};
use crate::editor::LoadStatus;
use crate::editor_prefs::EditorPrefs;
use sirio_markdown::LineEnding;

/// The longest branch name the row draws whole, in characters; a longer one is
/// shortened in the middle to this many, the ellipsis included.
const BRANCH_CHARS: usize = 24;

/// The row's side padding and the gap between its two groups, which the fit
/// test adds to the groups' widths.
const ROW_PADDING: Pixels = px(8.0);
const GROUP_GAP: Pixels = px(2.0);

/// What the row has measured of itself, written as it is laid out and read by
/// the next frame's render. Widths start at nothing, so the first frame draws
/// every item and the measure that follows decides.
#[derive(Default)]
pub(super) struct StatusLayout {
    /// The whole row.
    row: Cell<Pixels>,
    /// The left group at its natural width (the group itself shrinks and clips).
    left: Cell<Pixels>,
    /// The right group as drawn.
    right: Cell<Pixels>,
    /// The indent, encoding and line ending items together, as they were last
    /// drawn; what a compact row would need to grow back to the full set.
    extras: Cell<Pixels>,
    /// Whether the last decision was the compact set.
    compact: Cell<bool>,
}

impl StatusLayout {
    /// Decides from the measures whether the next frame draws the compact set,
    /// and whether that is a change.
    fn settle(&self) -> bool {
        let regrown = if self.compact.get() { self.extras.get() + GROUP_GAP } else { px(0.0) };
        let needed = self.left.get() + self.right.get() + regrown + GROUP_GAP + ROW_PADDING * 2.0;
        let compact = self.row.get() < needed;
        self.compact.replace(compact) != compact
    }
}

/// What only the workspace knows about the file's place, pushed by the app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusFacts {
    /// The branch the file's worktree has checked out (or a short commit id
    /// when its HEAD is detached); `None` outside a repository.
    pub branch: Option<String>,
    /// Whether the worktree has changes git lists.
    pub worktree_dirty: bool,
    /// `(ahead, behind)` of the branch against its upstream, as
    /// `sirio_git::ahead_behind` counts them; `None` when it has no upstream.
    /// (An `Option` rather than two numbers, so "level with the upstream" and
    /// "no upstream" read differently in the state.)
    pub sync: Option<(usize, usize)>,
    /// The language server for this file and where it stands; `None` when
    /// there is none to name.
    pub lsp: Option<(String, LspState)>,
}

/// What the row draws, read once per frame (and once per state read).
pub(super) struct StatusItems {
    pub(super) branch: Option<String>,
    pub(super) dirty: bool,
    pub(super) sync: Option<(usize, usize)>,
    pub(super) lsp: Option<(String, LspState)>,
    /// Line and column of the primary caret, from one, and how many characters
    /// all the selections cover together. A character is a Unicode scalar:
    /// a tab, a letter and an emoji each count one, and a combining mark counts
    /// by itself.
    pub(super) cursor: Option<(usize, usize, usize)>,
    pub(super) spaces: bool,
    pub(super) indent: usize,
    pub(super) crlf: bool,
    pub(super) language: &'static str,
    pub(super) bell: usize,
}

impl StatusItems {
    /// The row as the control socket reads it: `branch=…;ahead=…;behind=…;
    /// lsp=<name>:<starting|ready|failed>|none;cursor=Ln a, Col b (n selected);
    /// indent=Spaces: 4;encoding=UTF-8;eol=LF;language=Rust;bell=0`. A field the
    /// row does not draw reads `none`; `(n selected)` is there only while
    /// something is selected. The fields are separated by `;` without escaping,
    /// so a branch name that contains one makes the line ambiguous; a reader of
    /// the line splits on the keys it knows, and the E2E's branch names have
    /// none.
    pub(super) fn text(&self) -> String {
        let number = |value: Option<usize>| value.map_or("none".to_owned(), |n| n.to_string());
        let lsp = match &self.lsp {
            Some((name, state)) => {
                let state = match state {
                    LspState::Starting => "starting",
                    LspState::Ready => "ready",
                    LspState::Busy(_) => "busy",
                    LspState::Failed(_) => "failed",
                };
                format!("{name}:{state}")
            }
            None => "none".to_owned(),
        };
        let cursor = match self.cursor {
            Some((line, column, 0)) => format!("Ln {line}, Col {column}"),
            Some((line, column, selected)) => {
                format!("Ln {line}, Col {column} ({selected} selected)")
            }
            None => "none".to_owned(),
        };
        // The same words Ely's item draws.
        let indent = if self.spaces {
            format!("Spaces: {}", self.indent)
        } else {
            format!("Tab Size: {}", self.indent)
        };
        format!(
            "branch={};ahead={};behind={};lsp={lsp};cursor={cursor};indent={indent};\
             encoding=UTF-8;eol={};language={};bell={}",
            self.branch.as_deref().unwrap_or("none"),
            number(self.sync.map(|(ahead, _)| ahead)),
            number(self.sync.map(|(_, behind)| behind)),
            if self.crlf { "CRLF" } else { "LF" },
            self.language,
            self.bell,
        )
    }
}

impl FileView {
    /// The facts the app last pushed.
    pub fn status_facts(&self) -> &StatusFacts {
        &self.status_facts
    }

    /// Replaces the facts the row draws from the app. Notifies only when they
    /// differ: the app pushes on every load, save and focus.
    pub fn set_status_facts(&mut self, facts: StatusFacts, cx: &mut Context<Self>) {
        if self.status_facts != facts {
            self.status_facts = facts;
            cx.notify();
        }
    }

    /// The row as the control socket reads it (`StatusItems::text`); empty
    /// while the row is not drawn.
    pub fn status_text(&self, cx: &App) -> String {
        self.status_items(cx).map(|items| items.text()).unwrap_or_default()
    }

    /// Which set of items the row draws now, `compact` or `full`; empty while
    /// the row is not drawn.
    pub fn status_set(&self, cx: &App) -> &'static str {
        match self.status_items(cx) {
            None => "",
            Some(_) if self.status_layout.compact.get() => "compact",
            Some(_) => "full",
        }
    }

    /// What the row draws, or `None` while it is not drawn: the file has not
    /// loaded, or the code editor has not been made yet.
    pub(super) fn status_items(&self, cx: &App) -> Option<StatusItems> {
        let editor = self.editor()?;
        let code = self.code_editor.as_ref()?;
        if editor.status() != &LoadStatus::Loaded {
            return None;
        }
        let snapshot = self.is_snapshot();
        let code = code.read(cx);
        let cursor = (self.effective_mode() == MarkdownMode::Code).then(|| {
            let (line, column) = code.position();
            let text = code.text();
            let selected = code
                .selections()
                .iter()
                .map(|selection| text[selection.range()].chars().count())
                .sum();
            (line, column, selected)
        });
        // Until the settings screen can set an indent per file, every file
        // indents with spaces as wide as the Editor setting says.
        let facts = &self.status_facts;
        Some(StatusItems {
            branch: facts.branch.clone().filter(|_| !snapshot),
            dirty: facts.worktree_dirty,
            sync: facts.sync.filter(|_| !snapshot),
            lsp: facts.lsp.clone().filter(|_| !snapshot),
            cursor,
            spaces: true,
            indent: EditorPrefs::current(cx).tab_size,
            crlf: editor.line_ending() == LineEnding::Crlf,
            language: editor.language().name(),
            bell: 0,
        })
    }

    /// The row under the file, or `None` while there is nothing to show.
    pub(super) fn render_status_row(&self, theme: Theme, cx: &App) -> Option<AnyElement> {
        let items = self.status_items(cx)?;
        let layout = self.status_layout.clone();
        let compact = layout.compact.get();
        let mut left: Vec<AnyElement> = Vec::new();
        if let Some(branch) = items.branch {
            let mut indicator = BranchIndicator::new(
                "status-branch",
                SharedString::from(shorten_branch(&branch)),
            )
            .dirty(items.dirty);
            if let Some((ahead, behind)) = items.sync {
                indicator = indicator.sync(ahead, behind);
            }
            left.push(indicator.into_any_element());
        }
        if let Some((name, state)) = items.lsp {
            left.push(
                LspStatus::new("status-lsp", SharedString::from(name), state).into_any_element(),
            );
        }
        let mut right: Vec<AnyElement> = Vec::new();
        if let Some((line, column, selected)) = items.cursor {
            right.push(
                CursorPosition::new("status-cursor", line, column)
                    .selected(selected)
                    .into_any_element(),
            );
        }
        if !compact {
            let measure_extras = measure(layout.clone(), |layout, size| layout.extras.set(size));
            right.push(
                gpui::div()
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.0))
                    .child(measure_extras)
                    .child(
                        IndentSettings::new("status-indent", items.spaces, items.indent)
                            .into_any_element(),
                    )
                    .child(Encoding::new("status-encoding", "UTF-8").into_any_element())
                    .child(ElyLineEnding::new("status-line-ending", items.crlf).into_any_element())
                    .into_any_element(),
            );
        }
        right.push(LanguageMode::new("status-language", items.language).into_any_element());
        right.push(NotificationBell::new("status-bell", items.bell).into_any_element());
        // Each group reports the width it takes; the row's own measure is the
        // last to run, so it decides with this frame's numbers, and a change
        // asks for the frame that draws the other set.
        let measure_left = measure(layout.clone(), |layout, size| layout.left.set(size));
        let measure_right = measure(layout.clone(), |layout, size| layout.right.set(size));
        let decide = canvas(
            {
                let layout = layout.clone();
                move |bounds, window, _| {
                    layout.row.set(bounds.size.width);
                    if layout.settle() {
                        window.request_animation_frame();
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        Some(
            gpui::div()
                .id("file-status-row")
                .relative()
                .debug_selector(|| "file-status-row".to_owned())
                .w_full()
                .h(px(26.0))
                .px(ROW_PADDING)
                .flex()
                .flex_none()
                .items_center()
                .gap(GROUP_GAP)
                .overflow_hidden()
                .whitespace_nowrap()
                .border_t_1()
                .border_color(theme.ely.border)
                .bg(theme.ely.surface)
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(
                    // The group shrinks and clips; its content keeps its
                    // natural width, which is what the left measure reads.
                    gpui::div()
                        .flex()
                        .items_center()
                        .min_w(px(0.0))
                        .flex_shrink(1.0)
                        .overflow_hidden()
                        .child(
                            gpui::div()
                                .relative()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(2.0))
                                .child(measure_left)
                                .children(left),
                        ),
                )
                .child(gpui::div().flex_1())
                .child(
                    gpui::div()
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .flex_none()
                        .child(measure_right)
                        .children(right),
                )
                .child(decide)
                .into_any_element(),
        )
    }
}

/// A canvas that fills its parent (which must be `relative`) and hands the
/// layout the parent's size as it is laid out.
fn measure(
    layout: std::rc::Rc<StatusLayout>,
    record: impl Fn(&StatusLayout, Pixels) + 'static,
) -> impl IntoElement {
    canvas(
        move |bounds, _, _| record(&layout, bounds.size.width),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// `branch` as the row draws it: whole up to `BRANCH_CHARS` characters, else its
/// beginning and its end around an ellipsis.
fn shorten_branch(branch: &str) -> String {
    let length = branch.chars().count();
    if length <= BRANCH_CHARS {
        return branch.to_owned();
    }
    let kept = BRANCH_CHARS - 1;
    let head = kept / 2 + kept % 2;
    let tail = kept - head;
    let start: String = branch.chars().take(head).collect();
    let end: String = branch.chars().skip(length - tail).collect();
    format!("{start}…{end}")
}

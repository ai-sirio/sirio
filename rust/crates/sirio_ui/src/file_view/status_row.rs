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

use ely_gpui_component::editor::{
    BranchIndicator, CursorPosition, Encoding, IndentSettings, LanguageMode,
    LineEnding as ElyLineEnding, LspStatus, NotificationBell,
};
use ely_gpui_component::theme::Theme as ElyTheme;
use gpui::{AnyElement, App, Context, SharedString, prelude::*, px};
use sirio_theme::Theme;

use super::{FileView, LspState, MarkdownMode};
use crate::editor::LoadStatus;
use crate::editor_prefs::EditorPrefs;
use sirio_markdown::LineEnding;

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
    /// something is selected.
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
            sync: facts.sync,
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
        // Ely's items read Ely's theme, which the shell installs beside Sirio's
        // (`ely::init`); a fixture that sets only Sirio's has nothing to draw
        // them with, and draws no row.
        if !cx.has_global::<ElyTheme>() {
            return None;
        }
        let items = self.status_items(cx)?;
        let mut left: Vec<AnyElement> = Vec::new();
        if let Some(branch) = items.branch {
            let mut indicator =
                BranchIndicator::new("status-branch", SharedString::from(branch)).dirty(items.dirty);
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
        right.push(IndentSettings::new("status-indent", items.spaces, items.indent).into_any_element());
        right.push(Encoding::new("status-encoding", "UTF-8").into_any_element());
        right.push(ElyLineEnding::new("status-line-ending", items.crlf).into_any_element());
        right.push(LanguageMode::new("status-language", items.language).into_any_element());
        right.push(NotificationBell::new("status-bell", items.bell).into_any_element());
        Some(
            gpui::div()
                .id("file-status-row")
                .debug_selector(|| "file-status-row".to_owned())
                .w_full()
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(2.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .border_t_1()
                .border_color(theme.ely.border)
                .bg(theme.ely.surface)
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .children(left)
                .child(gpui::div().flex_1())
                .children(right)
                .into_any_element(),
        )
    }
}

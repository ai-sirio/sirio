//! The References view: where a symbol is used, grouped by file.
//!
//! The rows arrive already converted — a path, a display string, a
//! zero-based line and a preview — because the crate that owns the buffers
//! and the protocol does the converting. Nothing here knows what a UTF-16
//! column is, and nothing here reads a file.

use std::path::PathBuf;

use gpui::{Context, EventEmitter, Render, Window, div, prelude::*, px, uniform_list};
use sirio_theme::Theme;

/// Height of one row, header or result.
const ROW_HEIGHT: f32 = 24.0;

/// One place a symbol is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRow {
    pub path: PathBuf,
    /// Worktree-relative, for display.
    pub display: String,
    /// Zero-based, as the protocol counts and as `open_at_line` expects.
    /// The row draws `line + 1`.
    pub line: usize,
    /// The source line, trimmed. Empty when it could not be read — a file
    /// outside the preview budget, or one that no longer exists.
    pub preview: String,
}

/// What the surface is showing.
///
/// Four states rather than "rows or no rows", because a surface that says
/// nothing while it waits and a surface that found nothing are the same
/// picture, and the user who clicked deserves to tell them apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferencesState {
    /// Nothing has been asked yet.
    Idle,
    /// A question is out and the answer has not arrived.
    Searching { symbol: String },
    /// The answer arrived. `rows` may be empty: a symbol used nowhere is a
    /// real result.
    Found { symbol: String, rows: Vec<ReferenceRow> },
    /// The answer was a failure, already phrased for a reader.
    Failed(String),
}

/// A row of the drawn list: either a file's header or one of its results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupedRow {
    Header { display: String, count: usize },
    Reference(ReferenceRow),
}

/// One header per run of rows sharing a file, its results beneath.
///
/// A run, not a set: rows arrive ordered by (display, line), and if they
/// ever do not, a second run of the same file stays visible instead of
/// being folded into a header far above it.
pub fn group_by_file(rows: &[ReferenceRow]) -> Vec<GroupedRow> {
    let mut out: Vec<GroupedRow> = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        let display = rows[index].display.clone();
        let count = rows[index..]
            .iter()
            .take_while(|row| row.display == display)
            .count();
        out.push(GroupedRow::Header { display, count });
        for row in &rows[index..index + count] {
            out.push(GroupedRow::Reference(row.clone()));
        }
        index += count;
    }
    out
}

/// A click on a result row. The panel re-emits it; the app owns the tab it
/// lands in.
pub enum ReferencesEvent {
    Open { path: PathBuf, line: usize },
}

pub(crate) struct ReferencesList {
    state: ReferencesState,
}

impl ReferencesList {
    pub(crate) fn new() -> Self {
        Self { state: ReferencesState::Idle }
    }

    pub(crate) fn set_state(&mut self, state: ReferencesState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }
}

impl EventEmitter<ReferencesEvent> for ReferencesList {}

impl Render for ReferencesList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);

        let (heading, body) = summary(&self.state);

        let grouped = match &self.state {
            ReferencesState::Found { rows, .. } => group_by_file(rows),
            _ => Vec::new(),
        };

        let mut root = div()
            .id("right-panel-references")
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "references-heading".to_owned())
                    .h(px(ROW_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .child(heading),
            );

        if let Some(message) = body {
            return root.child(
                div()
                    .debug_selector(|| "references-message".to_owned())
                    .px(px(10.0))
                    .py(px(6.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(message),
            );
        }

        let count = grouped.len();
        root = root.child(
            uniform_list(
                "right-panel-references-rows",
                count,
                cx.processor(move |_list, range: std::ops::Range<usize>, _window, cx| {
                    let theme = *Theme::get(cx);
                    range
                        .filter_map(|index| {
                            grouped.get(index).cloned().map(|entry| (index, entry))
                        })
                        .map(|(index, entry)| match entry {
                            GroupedRow::Header { display, count } => div()
                                .h(px(ROW_HEIGHT))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_faint)
                                .child(display)
                                .child(format!("({count})"))
                                .into_any_element(),
                            GroupedRow::Reference(row) => {
                                let path = row.path.clone();
                                let line = row.line;
                                div()
                                    .id(("reference-row", index))
                                    .h(px(ROW_HEIGHT))
                                    .pl(px(22.0))
                                    .pr(px(10.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(theme.typography.footnote)
                                    .text_color(theme.text)
                                    .hover(|style| style.bg(theme.element_hover))
                                    .on_click(cx.listener(move |_list, _event, _window, cx| {
                                        cx.emit(ReferencesEvent::Open {
                                            path: path.clone(),
                                            line,
                                        });
                                    }))
                                    .child(
                                        div()
                                            .w(px(44.0))
                                            .flex_none()
                                            .text_color(theme.text_faint)
                                            .child(format!("{}", row.line + 1)),
                                    )
                                    .child(row.preview.clone())
                                    .into_any_element()
                            }
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1(),
        );
        root
    }
}

/// The heading, and the message that replaces the list when there is no
/// list to draw.
///
/// Pure, and public to the crate's tests, because this is the decision that
/// keeps "still looking" and "found nothing" from being the same picture.
/// The user clicked something; they must be able to tell the two apart.
pub fn summary(state: &ReferencesState) -> (String, Option<String>) {
    match state {
        ReferencesState::Idle => (
            "References".to_owned(),
            Some("Use Find References on a symbol.".to_owned()),
        ),
        ReferencesState::Searching { symbol } => {
            (headline(symbol, None), Some("Searching…".to_owned()))
        }
        ReferencesState::Failed(message) => ("References".to_owned(), Some(message.clone())),
        ReferencesState::Found { symbol, rows } if rows.is_empty() => (
            headline(symbol, Some(0)),
            Some("No references found".to_owned()),
        ),
        ReferencesState::Found { symbol, rows } => (headline(symbol, Some(rows.len())), None),
    }
}

/// `"12 references to `spawn`"`, or `"12 references"` when the buffer gave
/// no identifier to name. Never an invented one.
fn headline(symbol: &str, count: Option<usize>) -> String {
    match (count, symbol.is_empty()) {
        (Some(count), false) => format!("{count} references to {symbol}"),
        (Some(count), true) => format!("{count} references"),
        (None, false) => format!("References to {symbol}"),
        (None, true) => "References".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(display: &str, line: usize) -> ReferenceRow {
        ReferenceRow {
            path: std::path::PathBuf::from(format!("/repo/{display}")),
            display: display.to_owned(),
            line,
            preview: String::new(),
        }
    }

    #[test]
    fn rows_are_grouped_under_one_header_per_file() {
        let rows = vec![row("main.rs", 10), row("main.rs", 42), row("lsp.rs", 7)];
        let grouped = group_by_file(&rows);

        assert_eq!(grouped.len(), 5, "three rows plus two headers");
        assert_eq!(
            grouped[0],
            GroupedRow::Header { display: "main.rs".to_owned(), count: 2 }
        );
        assert!(matches!(&grouped[1], GroupedRow::Reference(r) if r.line == 10));
        assert!(matches!(&grouped[2], GroupedRow::Reference(r) if r.line == 42));
        assert_eq!(
            grouped[3],
            GroupedRow::Header { display: "lsp.rs".to_owned(), count: 1 }
        );
    }

    #[test]
    fn a_file_that_comes_back_later_gets_its_own_header() {
        // Rows arrive ordered by (display, line). If they ever do not, a
        // second run of the same file must still be legible rather than
        // silently folded into the first header's count.
        let rows = vec![row("a.rs", 1), row("b.rs", 2), row("a.rs", 3)];
        let grouped = group_by_file(&rows);
        let headers = grouped
            .iter()
            .filter(|entry| matches!(entry, GroupedRow::Header { .. }))
            .count();
        assert_eq!(headers, 3);
    }

    #[test]
    fn no_rows_is_no_groups() {
        assert!(group_by_file(&[]).is_empty());
    }

    #[test]
    fn waiting_and_finding_nothing_do_not_look_alike() {
        // The whole reason this surface has four states. A panel that says
        // nothing while it waits and a panel that found nothing are the
        // same picture, and the user who clicked cannot tell which happened.
        let (waiting_heading, waiting_body) = summary(&ReferencesState::Searching {
            symbol: "spawn".to_owned(),
        });
        let (empty_heading, empty_body) = summary(&ReferencesState::Found {
            symbol: "spawn".to_owned(),
            rows: Vec::new(),
        });

        assert_eq!(waiting_body.as_deref(), Some("Searching…"));
        assert_eq!(empty_body.as_deref(), Some("No references found"));
        assert_ne!(waiting_body, empty_body);
        assert_eq!(waiting_heading, "References to spawn");
        assert_eq!(empty_heading, "0 references to spawn");
    }

    #[test]
    fn results_replace_the_message_rather_than_joining_it() {
        // With rows to draw there must be no message: a list under
        // "Searching…" would say the search is still running.
        let (heading, body) = summary(&ReferencesState::Found {
            symbol: "spawn".to_owned(),
            rows: vec![row("main.rs", 3)],
        });
        assert_eq!(heading, "1 references to spawn");
        assert_eq!(body, None);
    }

    #[test]
    fn a_failure_is_shown_rather_than_swallowed() {
        // LspError's Display phrases the still-indexing case; the surface
        // shows whatever it is given rather than flattening it to "none".
        let (_, body) = summary(&ReferencesState::Failed(
            "the language server is still indexing this project".to_owned(),
        ));
        assert_eq!(
            body.as_deref(),
            Some("the language server is still indexing this project")
        );
    }

    #[test]
    fn a_symbol_the_buffer_could_not_name_is_not_invented() {
        // symbol_at returns empty when the caret is not on an identifier.
        // The heading then counts, and says nothing it does not know.
        let (heading, _) = summary(&ReferencesState::Found {
            symbol: String::new(),
            rows: vec![row("main.rs", 1), row("main.rs", 2)],
        });
        assert_eq!(heading, "2 references");
    }
}

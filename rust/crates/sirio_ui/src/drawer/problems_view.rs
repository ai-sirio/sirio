//! Problems as the drawer draws it: Ely's `ProblemsPanel` over the store, the
//! order its rows come in, and the line it says when there is nothing to list.
//!
//! Ways this can fail, written before the code:
//! - the order the debug verb counts rows in is not the order the panel draws
//!   them in, so `--open=N` opens a different row than the N-th drawn (the
//!   panel groups by file, puts the file with the worst problem first and the
//!   worse problem first within a file, each stably, and the current tab's file
//!   leads among equals);
//! - a file whose rows the severity filter hides is still counted as drawn;
//! - Information and Hint are drawn as errors or as nothing, instead of notes;
//! - the empty line names a server twice (two servers of one program), joins
//!   them with something else than ` · `, or says "no problems" with no server
//!   at all, which is a different thing to say;
//! - a path is drawn with the platform's separator, so Windows paths and the
//!   debug state disagree with the panel's own grouping of file and folder; or
//!   a backslash in a Unix file name is turned into a separator, so the row
//!   opens a different file.

use std::path::{Path, PathBuf};

use ely_gpui_component::editor::{Problem, ProblemsPanel};
use ely_gpui_component::primitives::Severity;
use gpui::{Entity, IntoElement, ParentElement, SharedString, Styled, div, px};
use sirio_theme::Theme;

use super::{StoredProblem, WorktreeDrawer};

/// Every severity the panel can filter by.
pub(super) const ALL_SEVERITIES: [Severity; 3] =
    [Severity::Danger, Severity::Warning, Severity::Info];

/// Ely's severity for the protocol's: Information and Hint are notes.
pub(super) fn ely_severity(severity: sirio_lsp::Severity) -> Severity {
    match severity {
        sirio_lsp::Severity::Error => Severity::Danger,
        sirio_lsp::Severity::Warning => Severity::Warning,
        sirio_lsp::Severity::Information | sirio_lsp::Severity::Hint => Severity::Info,
    }
}

/// How bad a severity is, worst highest (Ely's own ranking).
fn weight(severity: Severity) -> u8 {
    match severity {
        Severity::Danger => 3,
        Severity::Warning => 2,
        Severity::Info => 1,
        Severity::Success => 0,
    }
}

/// A path as the panel and the debug state name it: `/` between its parts. Only
/// Windows' separator is rewritten; a backslash in a Unix file name is part of it.
pub(super) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) { text.replace('\\', "/") } else { text.into_owned() }
}

/// One row as drawn: its file and the finding.
pub(super) struct DrawnRow {
    pub path: PathBuf,
    pub problem: StoredProblem,
}

/// The rows the panel draws for `files` with only `shown` severities, in the
/// order it draws them: files by their worst row (stable, so `files` order
/// breaks ties, and the caller puts the current tab's file first), rows within
/// a file by severity, worst first, then by line.
pub(super) fn drawn_rows(
    files: &[(PathBuf, Vec<StoredProblem>)],
    shown: &[Severity],
) -> Vec<DrawnRow> {
    let mut groups: Vec<(&PathBuf, Vec<&StoredProblem>)> = Vec::new();
    for (path, problems) in files {
        let mut kept: Vec<&StoredProblem> = problems
            .iter()
            .filter(|problem| shown.contains(&ely_severity(problem.severity)))
            .collect();
        if kept.is_empty() {
            continue;
        }
        kept.sort_by(|a, b| {
            weight(ely_severity(b.severity))
                .cmp(&weight(ely_severity(a.severity)))
                .then(a.line.cmp(&b.line))
        });
        groups.push((path, kept));
    }
    groups.sort_by_key(|(_, rows)| {
        std::cmp::Reverse(rows.first().map_or(0, |row| weight(ely_severity(row.severity))))
    });
    groups
        .into_iter()
        .flat_map(|(path, rows)| {
            rows.into_iter().map(|problem| DrawnRow {
                path: path.clone(),
                problem: problem.clone(),
            })
        })
        .collect()
}

/// What Problems says when it lists nothing: which servers are running and
/// that they report nothing, or that none is.
pub(super) fn empty_note(servers: &[String]) -> String {
    let mut named: Vec<&str> = Vec::new();
    for name in servers {
        if !named.contains(&name.as_str()) {
            named.push(name);
        }
    }
    if named.is_empty() {
        return "No language server is running in this worktree.".to_owned();
    }
    named
        .iter()
        .map(|name| format!("{name}: no problems"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The panel under the drawer's header, or the line that says why it is empty.
pub(super) fn render(
    files: &[(PathBuf, Vec<StoredProblem>)],
    shown: &[Severity],
    note: &str,
    drawer: Entity<WorktreeDrawer>,
    theme: &Theme,
) -> impl IntoElement {
    if files.iter().all(|(_, problems)| problems.is_empty()) {
        return div()
            .px(px(12.0))
            .py(px(10.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.ely.fg_subtle)
            .child(note.to_owned())
            .into_any_element();
    }
    // In the store's order, the current file first, and not sorted: Ely groups by
    // file and sorts by severity itself, stably, which is what `drawn_rows` does
    // to the same list, so the verb and the panel count the same rows.
    let problems = files.iter().flat_map(|(path, problems)| {
        problems.iter().map(move |problem| Problem {
            path: SharedString::from(display_path(path)),
            // Ely draws `line + 1` and `column + 1`, so the protocol's 0-based
            // numbers go in as they are.
            line: problem.line as usize,
            column: problem.column as usize,
            severity: ely_severity(problem.severity),
            message: SharedString::from(problem.message.clone()),
            source: problem.source.clone().map(SharedString::from),
        })
    });
    let on_open = drawer.clone();
    let on_shown = drawer;
    div()
        .px(px(8.0))
        .py(px(4.0))
        .child(
            ProblemsPanel::new("drawer-problems", problems)
                .shown(shown.iter().copied())
                .on_open(move |problem, _, cx| {
                    let (path, line, column) =
                        (problem.path.to_string(), problem.line, problem.column);
                    on_open.update(cx, |drawer, cx| drawer.open_problem(&path, line, column, cx));
                })
                .on_shown(move |next, _, cx| {
                    on_shown.update(cx, |drawer, cx| drawer.set_shown(next, cx));
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(line: u32, severity: sirio_lsp::Severity) -> StoredProblem {
        StoredProblem {
            line,
            column: 0,
            end_line: line,
            end_column: 0,
            severity,
            message: format!("m{line}"),
            source: None,
        }
    }

    fn files() -> Vec<(PathBuf, Vec<StoredProblem>)> {
        use sirio_lsp::Severity::{Error, Hint, Warning};
        vec![
            (PathBuf::from("a.rs"), vec![found(9, Warning), found(2, Hint)]),
            (PathBuf::from("b.rs"), vec![found(7, Warning), found(3, Error)]),
            (PathBuf::from("c.rs"), vec![found(1, Warning)]),
        ]
    }

    fn lines(rows: &[DrawnRow]) -> Vec<(String, u32)> {
        rows.iter().map(|row| (display_path(&row.path), row.problem.line)).collect()
    }

    #[test]
    fn the_worst_file_leads_the_worst_row_leads_it_and_ties_keep_the_given_order() {
        let rows = drawn_rows(&files(), &ALL_SEVERITIES);
        assert_eq!(
            lines(&rows),
            vec![
                ("b.rs".to_owned(), 3),
                ("b.rs".to_owned(), 7),
                ("a.rs".to_owned(), 9),
                ("a.rs".to_owned(), 2),
                ("c.rs".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn a_file_whose_rows_are_all_filtered_out_is_not_drawn() {
        let rows = drawn_rows(&files(), &[Severity::Info]);
        assert_eq!(lines(&rows), vec![("a.rs".to_owned(), 2)]);
    }

    #[test]
    fn information_and_hint_are_notes() {
        assert_eq!(ely_severity(sirio_lsp::Severity::Information), Severity::Info);
        assert_eq!(ely_severity(sirio_lsp::Severity::Hint), Severity::Info);
        assert_eq!(ely_severity(sirio_lsp::Severity::Error), Severity::Danger);
        assert_eq!(ely_severity(sirio_lsp::Severity::Warning), Severity::Warning);
    }

    #[test]
    fn the_note_names_each_server_once_and_says_so_when_there_is_none() {
        assert_eq!(
            empty_note(&["rust-analyzer".to_owned(), "pyright".to_owned(), "pyright".to_owned()]),
            "rust-analyzer: no problems · pyright: no problems"
        );
        assert_eq!(empty_note(&[]), "No language server is running in this worktree.");
    }

    #[test]
    fn a_path_is_drawn_with_forward_slashes_and_a_unix_backslash_is_kept() {
        assert_eq!(display_path(&Path::new("src").join("lib.rs")), "src/lib.rs");
        if !cfg!(windows) {
            assert_eq!(display_path(Path::new("a\\b.rs")), "a\\b.rs");
        }
    }
}

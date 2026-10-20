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
//!   opens a different file;
//! - the panel is given every problem a server published, and builds a row for
//!   each (it has no `uniform_list`): thousands of rows redrawn on every frame
//!   (the panel is given at most `DRAW_ROWS`, and says how many are not drawn);
//! - the toggles' counts become those of the drawn rows, so a capped list reads
//!   "1 500 errors" for 4 000 (they are given the store's counts);
//! - the list is rebuilt, and every stored problem cloned, on every render
//!   (`DrawnProblems` is built once and kept until the store, the current file or
//!   the severities change);
//! - every severity switched off draws an empty panel and no word on why.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use ely_gpui_component::editor::{Problem, ProblemsPanel};
use ely_gpui_component::primitives::Severity;
use gpui::{Entity, IntoElement, ParentElement, SharedString, Styled, div, px, prelude::*};
use sirio_theme::Theme;

use super::search_view::plural;
use super::{ProblemCounts, StoredProblem, WorktreeDrawer};

/// The most rows (a file's header, a problem) Problems draws at once; what is
/// left is counted in a line under them.
pub(super) const DRAW_ROWS: usize = 1500;

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
/// a file by severity, worst first, then by line. At most `limit` are returned;
/// the second number is how many more there were.
pub(super) fn drawn_rows(
    files: &[(PathBuf, Vec<StoredProblem>)],
    shown: &[Severity],
    limit: usize,
) -> (Vec<DrawnRow>, usize) {
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
    let total: usize = groups.iter().map(|(_, rows)| rows.len()).sum();
    let rows = groups
        .into_iter()
        .flat_map(|(path, rows)| {
            rows.into_iter().map(|problem| DrawnRow {
                path: path.clone(),
                problem: problem.clone(),
            })
        })
        .take(limit)
        .collect::<Vec<_>>();
    let omitted = total - rows.len();
    (rows, omitted)
}

/// What Problems draws, built once from the store: the panel's rows (at most
/// `DRAW_ROWS`, in the order the panel sorts them to), how many more there were,
/// and the store's exact counts for the toggles.
pub(super) struct DrawnProblems {
    pub rows: Vec<DrawnRow>,
    pub panel: Vec<Problem>,
    pub omitted: usize,
    /// No server has anything to say about any file.
    pub nothing_stored: bool,
    pub counts: ProblemCounts,
}

impl DrawnProblems {
    pub(super) fn build(
        files: &[(PathBuf, Vec<StoredProblem>)],
        shown: &[Severity],
        counts: ProblemCounts,
    ) -> Rc<Self> {
        let nothing_stored = files.iter().all(|(_, problems)| problems.is_empty());
        let (rows, omitted) = drawn_rows(files, shown, DRAW_ROWS);
        // Already in the panel's own order, which it sorts to stably: the verb
        // and the panel count the same rows.
        let panel = rows
            .iter()
            .map(|row| Problem {
                path: SharedString::from(display_path(&row.path)),
                // Ely draws `line + 1` and `column + 1`, so the protocol's 0-based
                // numbers go in as they are.
                line: row.problem.line as usize,
                column: row.problem.column as usize,
                severity: ely_severity(row.problem.severity),
                message: SharedString::from(row.problem.message.clone()),
                source: row.problem.source.clone().map(SharedString::from),
            })
            .collect();
        Rc::new(Self { rows, panel, omitted, nothing_stored, counts })
    }
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

/// What Problems says under the panel when it draws fewer than there are.
pub(super) fn omitted_note(omitted: usize) -> String {
    format!(
        "{} {} not drawn.",
        plural(omitted, "more problem"),
        if omitted == 1 { "is" } else { "are" }
    )
}

/// What Problems says when the user has switched every severity off.
pub(super) const ALL_HIDDEN: &str = "All severities are hidden.";

/// The panel under the drawer's header, or the line that says why it is empty.
pub(super) fn render(
    drawn: &DrawnProblems,
    shown: &[Severity],
    note: &str,
    drawer: Entity<WorktreeDrawer>,
    theme: &Theme,
) -> impl IntoElement {
    let line = |text: String| {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.ely.fg_subtle)
            .child(text)
    };
    if drawn.nothing_stored {
        return line(note.to_owned()).py(px(10.0)).into_any_element();
    }
    let on_open = drawer.clone();
    let on_shown = drawer;
    let counts = [drawn.counts.errors, drawn.counts.warnings, drawn.counts.notes];
    div()
        .px(px(8.0))
        .py(px(4.0))
        .child(
            ProblemsPanel::new("drawer-problems", drawn.panel.iter().cloned())
                .shown(shown.iter().copied())
                .counts(counts)
                .on_open(move |problem, _, cx| {
                    let (path, line, column) =
                        (problem.path.to_string(), problem.line, problem.column);
                    on_open.update(cx, |drawer, cx| drawer.open_problem(&path, line, column, cx));
                })
                .on_shown(move |next, _, cx| {
                    on_shown.update(cx, |drawer, cx| drawer.set_shown(next, cx));
                }),
        )
        .when(shown.is_empty(), |this| this.child(line(ALL_HIDDEN.to_owned())))
        .when(drawn.omitted > 0, |this| this.child(line(omitted_note(drawn.omitted))))
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
        let (rows, omitted) = drawn_rows(&files(), &ALL_SEVERITIES, usize::MAX);
        assert_eq!(omitted, 0);
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
        let (rows, _) = drawn_rows(&files(), &[Severity::Info], usize::MAX);
        assert_eq!(lines(&rows), vec![("a.rs".to_owned(), 2)]);
    }

    #[test]
    fn a_limit_keeps_the_first_rows_in_drawn_order_and_counts_the_rest() {
        let (rows, omitted) = drawn_rows(&files(), &ALL_SEVERITIES, 2);
        assert_eq!(lines(&rows), vec![("b.rs".to_owned(), 3), ("b.rs".to_owned(), 7)]);
        assert_eq!(omitted, 3);
        // Rows hidden by the severity filter are not "not drawn".
        let (rows, omitted) = drawn_rows(&files(), &[Severity::Info], 1);
        assert_eq!((rows.len(), omitted), (1, 0));
    }

    #[test]
    fn a_store_over_the_budget_gives_the_panel_the_budget_and_the_toggles_the_stores_counts() {
        let many: Vec<StoredProblem> = (0..DRAW_ROWS as u32 + 100)
            .map(|line| found(line, sirio_lsp::Severity::Error))
            .collect();
        let counts = ProblemCounts { errors: many.len(), warnings: 0, notes: 0 };
        let drawn = DrawnProblems::build(&[(PathBuf::from("a.rs"), many)], &ALL_SEVERITIES, counts);

        let drawn_counts = (drawn.rows.len(), drawn.panel.len(), drawn.omitted);
        assert_eq!(drawn_counts, (DRAW_ROWS, DRAW_ROWS, 100));
        assert_eq!(drawn.counts.errors, DRAW_ROWS + 100);
        assert!(!drawn.nothing_stored);
        let empty = [(PathBuf::from("a.rs"), vec![])];
        assert!(DrawnProblems::build(&empty, &ALL_SEVERITIES, counts).nothing_stored);
    }

    #[test]
    fn the_budget_is_one_line_with_grouped_counts() {
        assert_eq!(omitted_note(1), "1 more problem is not drawn.");
        assert_eq!(omitted_note(3_500), "3 500 more problems are not drawn.");
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

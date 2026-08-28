//! Data transformation from unified diffs to side-by-side rows.

use crate::{DiffLine, DiffOrigin, FileDiff};

/// One line occupying one side of a side-by-side row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffSideBySideLine {
    pub origin: DiffOrigin,
    pub old_line_number: Option<usize>,
    pub new_line_number: Option<usize>,
    pub content: String,
    /// Hunk headers are represented as full-width rows, stored on the left.
    pub is_hunk: bool,
}

impl DiffSideBySideLine {
    fn from_diff_line(line: &DiffLine) -> Self {
        Self {
            origin: line.origin,
            old_line_number: line.old_line_number,
            new_line_number: line.new_line_number,
            content: line.content.clone(),
            is_hunk: false,
        }
    }

    fn hunk(header: &str) -> Self {
        Self {
            origin: DiffOrigin::Context,
            old_line_number: None,
            new_line_number: None,
            content: header.to_string(),
            is_hunk: true,
        }
    }
}

/// One visual row: old content on the left, new content on the right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffSideBySideRow {
    pub id: usize,
    pub left: Option<DiffSideBySideLine>,
    pub right: Option<DiffSideBySideLine>,
    /// Hunk headers span the full width rather than being paired.
    pub is_hunk: bool,
}

/// Compatibility aliases matching the source package's vocabulary.
pub type GitDiffSideBySideLine = DiffSideBySideLine;
pub type GitDiffSideBySideRow = DiffSideBySideRow;

/// Namespace for side-by-side transformations.
pub struct GitDiffSideBySide;

impl GitDiffSideBySide {
    /// Converts all hunks, including their headers, to side-by-side rows.
    pub fn rows(diff: &FileDiff) -> Vec<DiffSideBySideRow> {
        let mut rows = Vec::new();
        for hunk in &diff.hunks {
            rows.push(DiffSideBySideRow {
                id: rows.len(),
                left: Some(DiffSideBySideLine::hunk(&hunk.header)),
                right: None,
                is_hunk: true,
            });
            append_lines(&mut rows, &hunk.lines);
        }
        rows
    }

    /// Converts a flat sequence of unified lines. Metadata is not a member of
    /// [`DiffLine`] and is therefore naturally absent; callers that start
    /// from raw git output should parse it through [`crate::parse_diff`].
    pub fn rows_from_lines(lines: &[DiffLine]) -> Vec<DiffSideBySideRow> {
        let mut rows = Vec::new();
        append_lines(&mut rows, lines);
        rows
    }
}

fn append_lines(rows: &mut Vec<DiffSideBySideRow>, lines: &[DiffLine]) {
    let mut deletions = Vec::new();
    let mut additions = Vec::new();
    for line in lines {
        match line.origin {
            DiffOrigin::Deletion => deletions.push(DiffSideBySideLine::from_diff_line(line)),
            DiffOrigin::Addition => additions.push(DiffSideBySideLine::from_diff_line(line)),
            DiffOrigin::Context => {
                flush(rows, &mut deletions, &mut additions);
                let line = DiffSideBySideLine::from_diff_line(line);
                rows.push(DiffSideBySideRow {
                    id: rows.len(),
                    left: Some(line.clone()),
                    right: Some(line),
                    is_hunk: false,
                });
            }
        }
    }
    flush(rows, &mut deletions, &mut additions);
}

fn flush(
    rows: &mut Vec<DiffSideBySideRow>,
    deletions: &mut Vec<DiffSideBySideLine>,
    additions: &mut Vec<DiffSideBySideLine>,
) {
    for index in 0..deletions.len().max(additions.len()) {
        rows.push(DiffSideBySideRow {
            id: rows.len(),
            left: deletions.get(index).cloned(),
            right: additions.get(index).cloned(),
            is_hunk: false,
        });
    }
    deletions.clear();
    additions.clear();
}

/// Free-function spelling for side-by-side conversion.
pub fn side_by_side_rows(diff: &FileDiff) -> Vec<DiffSideBySideRow> {
    GitDiffSideBySide::rows(diff)
}

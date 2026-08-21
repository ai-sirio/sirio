# Right Panel Views Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the right panel's fixed `Files` header with a rail of four icons that switch the whole panel between Files, Activity, Diff and a new commit-graph History.

**Architecture:** `right_panel.rs` becomes a module directory whose `mod.rs` owns the icon rail and a `PanelView` selection held in a GPUI global; Files and Activity move out unchanged, Diff mounts the existing `ChangesTab`, and History is new — a pure `git log` + lane-layout pair in `tiller_git` rendered by a `uniform_list` with a canvas-painted graph column.

**Tech Stack:** Rust 2024, gpui (Zed pinned rev), `tiller_git` shelling out to the `git` binary through `GitRunner`, no new dependencies in any crate.

**Spec:** `docs/superpowers/specs/2026-08-21-right-panel-views-design.md`

## Global Constraints

- No new crate dependencies. `tiller_git` depends only on `libc`; keep it that way.
- `tiller_git` and its two new modules stay UI-free: no gpui import, plain value types.
- Every colour comes from `Theme::get(cx)`. No hardcoded colour anywhere in a renderer.
- New SVGs come byte-for-byte from Zed commit `875e2a1c458ffcf2bfe90be841eb9893f399b4ff`, the commit already recorded in `rust/assets/icons/zed/ATTRIBUTION.md`. Do not refresh the catalog and do not edit icon geometry.
- No GitComet or Zed source code is copied, adapted, or transcribed. GitComet is AGPL-3.0, Zed is GPL-3.0, Tiller is MIT.
- Tests first: write the failing test, run it, then implement.
- Conventional Commits, lower-case imperative subject.
- Gate: `Scripts/ci.sh` prints `CI OK`.
- Run per-crate (`cargo test -p <crate>`) while iterating; the workspace run has two timing-sensitive tests.

## Track layout

Track A (Tasks 1-4) touches only `rust/crates/tiller_git/`. Track B (Tasks 5-8) touches only `rust/crates/tiller_ui/`, `rust/assets/`, and `rust/crates/tiller/src/main.rs`. They share no file and may run in parallel checkouts. Track C (Tasks 9-12) needs both and runs after they merge.

---

### Task 1: `CommitRecord` and the log parser

**Files:**
- Create: `rust/crates/tiller_git/src/log.rs`
- Modify: `rust/crates/tiller_git/src/lib.rs` (add `mod log;` and the `pub use`)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct CommitRecord { pub sha: String, pub parents: Vec<String>, pub refs: Vec<String>, pub author: String, pub timestamp: i64, pub subject: String }` and `pub fn parse_log(output: &str) -> Vec<CommitRecord>`. Task 2 calls `parse_log`; Task 3 consumes `&[CommitRecord]`.

- [ ] **Step 1: Write the failing tests**

Create `rust/crates/tiller_git/src/log.rs` containing only the test module for now:

```rust
//! `git log` reading: commit records for the History view.

#[cfg(test)]
mod tests {
    use super::*;

    /// One record per commit, fields split on US (0x1f), records on RS (0x1e).
    fn fixture(records: &[&str]) -> String {
        records
            .iter()
            .map(|record| format!("\u{1e}{record}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_a_single_commit() {
        let output = fixture(&["abc123\u{1f}\u{1f}HEAD -> main\u{1f}Ada\u{1f}1700000000\u{1f}initial commit"]);

        let commits = parse_log(&output);

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "abc123");
        assert!(commits[0].parents.is_empty());
        assert_eq!(commits[0].refs, vec!["HEAD -> main".to_owned()]);
        assert_eq!(commits[0].author, "Ada");
        assert_eq!(commits[0].timestamp, 1_700_000_000);
        assert_eq!(commits[0].subject, "initial commit");
    }

    #[test]
    fn parses_multiple_parents_of_a_merge() {
        let output = fixture(&["m1\u{1f}p1 p2\u{1f}\u{1f}Ada\u{1f}1700000001\u{1f}merge branch 'x'"]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].parents, vec!["p1".to_owned(), "p2".to_owned()]);
    }

    #[test]
    fn an_empty_ref_field_yields_no_refs() {
        let output = fixture(&["c1\u{1f}p1\u{1f}\u{1f}Ada\u{1f}1700000002\u{1f}fix: thing"]);

        let commits = parse_log(&output);

        assert!(commits[0].refs.is_empty());
    }

    #[test]
    fn splits_multiple_refs_on_comma() {
        let output = fixture(&["c1\u{1f}p1\u{1f}HEAD -> main, origin/main, tag: v1\u{1f}Ada\u{1f}1\u{1f}s"]);

        let commits = parse_log(&output);

        assert_eq!(
            commits[0].refs,
            vec![
                "HEAD -> main".to_owned(),
                "origin/main".to_owned(),
                "tag: v1".to_owned(),
            ]
        );
    }

    #[test]
    fn keeps_spaces_and_unicode_in_a_subject() {
        let output = fixture(&["c1\u{1f}p1\u{1f}\u{1f}Ada Lovelace\u{1f}3\u{1f}feat: aggiunge il pannello — con trattino"]);

        let commits = parse_log(&output);

        assert_eq!(commits[0].author, "Ada Lovelace");
        assert_eq!(commits[0].subject, "feat: aggiunge il pannello — con trattino");
    }

    #[test]
    fn empty_output_yields_no_commits() {
        assert!(parse_log("").is_empty());
    }

    #[test]
    fn a_record_with_missing_fields_is_skipped_not_panicked_on() {
        let output = fixture(&["truncated\u{1f}p1"]);

        assert!(parse_log(&output).is_empty());
    }
}
```

Register the module in `rust/crates/tiller_git/src/lib.rs`: add `mod log;` to the `mod` block (alphabetical, between `git` and `remote`) and `pub use log::{CommitRecord, GitLog, parse_log};` next to the other `pub use` lines.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_git log::`
Expected: FAIL to compile — `cannot find function parse_log`, `cannot find type CommitRecord`.

- [ ] **Step 3: Write the implementation**

Above the test module in `rust/crates/tiller_git/src/log.rs`:

```rust
/// Field separator inside one record: ASCII US.
const FIELD: char = '\u{1f}';
/// Record separator between commits: ASCII RS. Deliberately not `NUL`, which
/// collides with `-z` and which git would also use as its own record
/// delimiter; a subject can contain neither of these two control characters.
const RECORD: char = '\u{1e}';

/// The commit fields the History view needs, in `git log` order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitRecord {
    /// Full 40-character object name.
    pub sha: String,
    /// Full object names of the parents, in git's order. Empty for a root
    /// commit; two or more for a merge.
    pub parents: Vec<String>,
    /// Decorations from `%D`, already split: `HEAD -> main`, `tag: v1`, ...
    pub refs: Vec<String>,
    /// Author name (`%an`), not the committer.
    pub author: String,
    /// Author date as a Unix timestamp (`%at`).
    pub timestamp: i64,
    /// One-line subject (`%s`).
    pub subject: String,
}

/// Parses the output of the `--format` this module sends. A record whose
/// field count is short is dropped rather than partially filled: a truncated
/// capture (see `GitCommandResult::truncated`) must not produce a commit with
/// an empty sha that the graph would then try to link.
pub fn parse_log(output: &str) -> Vec<CommitRecord> {
    output
        .split(RECORD)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split(FIELD);
            let sha = fields.next()?.to_owned();
            let parents = fields.next()?;
            let refs = fields.next()?;
            let author = fields.next()?.to_owned();
            let timestamp = fields.next()?;
            let subject = fields.next()?.to_owned();
            if sha.is_empty() {
                return None;
            }
            Some(CommitRecord {
                sha,
                parents: parents
                    .split_whitespace()
                    .map(ToOwned::to_owned)
                    .collect(),
                refs: refs
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                author,
                timestamp: timestamp.trim().parse().unwrap_or_default(),
                subject,
            })
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git log::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_git/src/lib.rs
git commit -m "feat: parse git log records for the history view"
```

---

### Task 2: Running `git log`

**Files:**
- Modify: `rust/crates/tiller_git/src/log.rs`
- Test: `rust/crates/tiller_git/tests/log_integration.rs` (create)

**Interfaces:**
- Consumes: `parse_log`, `CommitRecord` (Task 1); `git::run_accepting(args, cwd, accepted_codes)` — the same helper `branches.rs:17` uses.
- Produces: `pub struct GitLog;` with `pub fn commits(repo: &Path, skip: usize, limit: usize) -> Result<Vec<CommitRecord>, GitError>` and `pub fn has_commits(repo: &Path) -> bool`. Task 10 calls both.

- [ ] **Step 1: Write the failing test**

Create `rust/crates/tiller_git/tests/log_integration.rs`. Copy the `TempDir` and `git` helpers from `rust/crates/tiller_git/tests/git_integration.rs:16-52` verbatim (they are per-file helpers by design in this crate; do not extract a shared module):

```rust
//! `git log` reading against real temporary repositories.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_git::GitLog;

// TempDir + git() helpers copied from git_integration.rs (same crate convention).

/// Builds: c1 <- c2 on main, plus a branch `side` off c1 merged into main.
fn repo_with_a_merge() -> TempDir {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@example.com"]);
    git(dir.path(), &["config", "user.name", "Tester"]);
    std::fs::write(dir.path().join("a.txt"), "1").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c1"]);
    git(dir.path(), &["checkout", "-q", "-b", "side"]);
    std::fs::write(dir.path().join("b.txt"), "2").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c2 on side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    std::fs::write(dir.path().join("c.txt"), "3").expect("write");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "c3 on main"]);
    git(dir.path(), &["merge", "-q", "--no-ff", "side", "-m", "merge side"]);
    dir
}

#[test]
fn reads_commits_newest_first_with_parents() {
    let dir = repo_with_a_merge();

    let commits = GitLog::commits(dir.path(), 0, 100).expect("log");

    assert_eq!(commits[0].subject, "merge side");
    assert_eq!(commits[0].parents.len(), 2, "a --no-ff merge has two parents");
    assert!(commits.iter().any(|c| c.subject == "c2 on side"),
        "--branches must include commits reachable only from other local branches");
}

#[test]
fn skip_and_limit_paginate() {
    let dir = repo_with_a_merge();

    let first = GitLog::commits(dir.path(), 0, 2).expect("log");
    let second = GitLog::commits(dir.path(), 2, 2).expect("log");

    assert_eq!(first.len(), 2);
    assert!(!second.is_empty());
    assert_ne!(first[0].sha, second[0].sha);
}

#[test]
fn a_repository_without_commits_reports_no_commits_rather_than_an_error() {
    let dir = TempDir::new();
    git(dir.path(), &["init", "-q", "-b", "main"]);

    assert!(!GitLog::has_commits(dir.path()));
    assert!(GitLog::commits(dir.path(), 0, 10).expect("log").is_empty());
}

#[test]
fn a_directory_that_is_not_a_repository_is_an_error() {
    let dir = TempDir::new();

    assert!(GitLog::commits(dir.path(), 0, 10).is_err());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p tiller_git --test log_integration`
Expected: FAIL to compile — `no function or associated item named commits found for struct GitLog`.

- [ ] **Step 3: Write the implementation**

In `rust/crates/tiller_git/src/log.rs`, above the tests:

```rust
use std::path::Path;

use crate::GitError;
use crate::git;

/// The `--format` that feeds [`parse_log`]. Kept next to the parser so the
/// two can never drift apart.
const LOG_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%D%x1f%an%x1f%at%x1f%s";

/// Namespace for commit-history reads.
pub struct GitLog;

impl GitLog {
    /// Reads `limit` commits starting at `skip`, over local refs only.
    ///
    /// `--branches` (not `--all`) keeps remote-tracking refs out: on a repo
    /// with many remote branches the lane count explodes and the graph stops
    /// being readable. `--date-order` is required, not cosmetic — without an
    /// explicit order git may return rewritten history in an order that
    /// differs between chunks, which makes lanes jump mid-scroll.
    ///
    /// Exit code 128 with "does not have any commits yet" is an unborn HEAD,
    /// not a failure: it returns an empty list.
    pub fn commits(repo: &Path, skip: usize, limit: usize) -> Result<Vec<CommitRecord>, GitError> {
        let skip_arg = format!("--skip={skip}");
        let limit_arg = format!("-n{limit}");
        let output = match git::run_accepting(
            &[
                "log",
                "--branches",
                "--date-order",
                &skip_arg,
                &limit_arg,
                LOG_FORMAT,
            ],
            repo,
            &[0],
        ) {
            Ok(output) => output,
            Err(error) => {
                return if is_unborn_head(&error) {
                    Ok(Vec::new())
                } else {
                    Err(error)
                };
            }
        };
        Ok(parse_log(&output.stdout_string()))
    }

    /// Whether the repository has at least one commit. Cheaper than a log
    /// read and the only thing the empty state needs.
    pub fn has_commits(repo: &Path) -> bool {
        git::run_accepting(&["rev-parse", "--verify", "HEAD"], repo, &[0]).is_ok()
    }
}

/// An initialised repository with no commits fails `git log` with git's
/// "fatal" code and a stderr this is the only reliable marker of. It must be
/// told apart from a real failure so the panel shows "No commits yet" instead
/// of an error with a Retry that can never succeed.
fn is_unborn_head(error: &GitError) -> bool {
    match error {
        GitError::CommandFailed { stderr, .. } => {
            stderr.contains("does not have any commits yet")
                || stderr.contains("unknown revision or path not in the working tree")
        }
        _ => false,
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd rust && cargo test -p tiller_git --test log_integration`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_git/tests/log_integration.rs
git commit -m "feat: read commit history over local refs"
```

---

### Task 3: Lane layout

**Files:**
- Create: `rust/crates/tiller_git/src/graph.rs`
- Modify: `rust/crates/tiller_git/src/lib.rs` (`mod graph;` + `pub use graph::{GraphRow, layout};`)

**Interfaces:**
- Consumes: `CommitRecord` (Task 1).
- Produces: `pub struct GraphRow { pub lane: usize, pub color: usize, pub through: Vec<Option<usize>>, pub joins_in: Vec<(usize, usize)>, pub edges_out: Vec<(usize, usize)> }` and `pub fn layout(commits: &[CommitRecord]) -> Vec<GraphRow>`. Task 11 paints exactly these fields. `through[column]` is `Some(colour)` when a lane occupies that column *below* the row (i.e. continues to the next row), `None` when the column is a hole.

- [ ] **Step 1: Write the failing tests**

Create `rust/crates/tiller_git/src/graph.rs` with the tests only:

```rust
//! Commit-graph lane layout: which column each commit's node sits in, and
//! which lines connect the rows.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommitRecord;

    fn commit(sha: &str, parents: &[&str]) -> CommitRecord {
        CommitRecord {
            sha: sha.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            ..CommitRecord::default()
        }
    }

    #[test]
    fn a_linear_history_uses_one_lane() {
        let commits = [commit("c3", &["c2"]), commit("c2", &["c1"]), commit("c1", &[])];

        let rows = layout(&commits);

        assert!(rows.iter().all(|row| row.lane == 0));
        assert!(rows.iter().all(|row| row.joins_in.is_empty()));
    }

    #[test]
    fn the_last_row_of_a_linear_history_continues_nothing() {
        let commits = [commit("c2", &["c1"]), commit("c1", &[])];

        let rows = layout(&commits);

        assert_eq!(rows[0].through, vec![Some(rows[0].color)]);
        assert!(rows[1].through.iter().all(Option::is_none));
    }

    #[test]
    fn a_second_parent_opens_a_new_lane() {
        // m -> (main, side); main -> base; side -> base; base is root.
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits);

        assert_eq!(rows[0].lane, 0, "the merge sits on the lane it inherited");
        assert_eq!(rows[0].edges_out.len(), 1, "the second parent leaves for its own lane");
        assert_eq!(rows[0].edges_out[0].0, 1, "leftmost free column");
        assert_eq!(rows[2].lane, 1, "`side` is drawn on the lane opened for it");
    }

    #[test]
    fn converging_lanes_join_on_the_commit_they_share() {
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits);

        let base = &rows[3];
        assert_eq!(base.lane, 0);
        assert_eq!(base.joins_in.len(), 1, "the side lane folds into base");
        assert_eq!(base.joins_in[0].0, 1, "it comes from column 1");
    }

    #[test]
    fn a_closed_lane_leaves_a_hole_that_is_reused() {
        // `side` ends at row 2; a later unrelated root opens a lane and must
        // take column 1 back rather than widening the graph to column 2.
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
            commit("orphan", &[]),
        ];

        let rows = layout(&commits);

        assert_eq!(rows[4].lane, 1, "the freed column is reused, not widened past");
    }

    #[test]
    fn a_new_lane_avoids_the_colours_of_live_lanes() {
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits);

        let opened_colour = rows[0].edges_out[0].1;
        assert_ne!(opened_colour, rows[0].color, "two live lanes must not share a colour");
    }

    #[test]
    fn an_empty_history_lays_out_to_nothing() {
        assert!(layout(&[]).is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_git graph::`
Expected: FAIL to compile — `cannot find function layout`.

- [ ] **Step 3: Write the implementation**

Above the tests in `rust/crates/tiller_git/src/graph.rs`:

```rust
use crate::CommitRecord;

/// How many distinct lane colours the palette offers. The renderer maps this
/// index onto real colours; the layout only cycles indices.
pub const LANE_COLORS: usize = 6;

/// One rendered row of the graph column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    /// Column of this commit's node.
    pub lane: usize,
    /// Palette index of the node and of the lane it continues on.
    pub color: usize,
    /// Per column, the colour of the lane that continues *below* this row.
    /// `None` is a hole: a column whose lane has ended and which is free for
    /// reuse. Holes are never compacted away — compacting would slide live
    /// branches sideways on every commit, which reads as the graph dancing
    /// during a scroll.
    pub through: Vec<Option<usize>>,
    /// `(source column, colour)` for each lane that ends on this commit and
    /// must be drawn folding into its node.
    pub joins_in: Vec<(usize, usize)>,
    /// `(target column, colour)` for each parent beyond the first, which
    /// leaves this node for a lane of its own.
    pub edges_out: Vec<(usize, usize)>,
}

/// One live lane: the commit it is waiting for, and the colour it draws in.
#[derive(Clone, Debug)]
struct Lane {
    expects: String,
    color: usize,
}

/// Assigns every commit a column and the edges around it.
///
/// The commits must be in the order `git log --date-order` returned them
/// (newest first). Each commit takes the lane already expecting its sha, or
/// the leftmost free column; its first parent inherits that lane, and every
/// further parent takes the lane already expecting it or opens a new one.
pub fn layout(commits: &[CommitRecord]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0usize;
    let mut rows = Vec::with_capacity(commits.len());

    for commit in commits {
        // Every lane waiting for this commit converges here. The leftmost is
        // the one the commit is drawn on; the rest fold in and end.
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, lane)| {
                lane.as_ref()
                    .is_some_and(|lane| lane.expects == commit.sha)
            })
            .map(|(column, _)| column)
            .collect();

        let (lane_column, color) = match waiting.first() {
            Some(&column) => {
                let color = lanes[column]
                    .as_ref()
                    .expect("a waiting column holds a lane")
                    .color;
                (column, color)
            }
            None => {
                let color = pick_color(&lanes, &mut next_color);
                let column = free_column(&mut lanes);
                (column, color)
            }
        };

        let joins_in = waiting
            .iter()
            .skip(1)
            .map(|&column| {
                let color = lanes[column]
                    .as_ref()
                    .expect("a waiting column holds a lane")
                    .color;
                (column, color)
            })
            .collect::<Vec<_>>();
        for &column in waiting.iter().skip(1) {
            lanes[column] = None;
        }

        // The first parent inherits this lane; the lane ends when there is
        // none (a root commit).
        match commit.parents.first() {
            Some(parent) => {
                lanes[lane_column] = Some(Lane {
                    expects: parent.clone(),
                    color,
                });
            }
            None => lanes[lane_column] = None,
        }

        let mut edges_out = Vec::new();
        for parent in commit.parents.iter().skip(1) {
            if let Some(column) = lanes.iter().position(|lane| {
                lane.as_ref().is_some_and(|lane| &lane.expects == parent)
            }) {
                let color = lanes[column]
                    .as_ref()
                    .expect("a matched column holds a lane")
                    .color;
                edges_out.push((column, color));
                continue;
            }
            let color = pick_color(&lanes, &mut next_color);
            let column = free_column(&mut lanes);
            lanes[column] = Some(Lane {
                expects: parent.clone(),
                color,
            });
            edges_out.push((column, color));
        }

        let through = lanes
            .iter()
            .map(|lane| lane.as_ref().map(|lane| lane.color))
            .collect();

        rows.push(GraphRow {
            lane: lane_column,
            color,
            through,
            joins_in,
            edges_out,
        });
    }

    rows
}

/// The leftmost hole, or a new column at the right edge. Reserved by leaving
/// the slot `None`; the caller fills it.
fn free_column(lanes: &mut Vec<Option<Lane>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(column) => column,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// The next palette index that no live lane is already using. Two adjacent
/// lanes sharing a colour read as one branch, which is the single most
/// common way a commit graph misleads.
fn pick_color(lanes: &[Option<Lane>], next_color: &mut usize) -> usize {
    let live: Vec<usize> = lanes
        .iter()
        .filter_map(|lane| lane.as_ref().map(|lane| lane.color))
        .collect();
    for _ in 0..LANE_COLORS {
        let candidate = *next_color % LANE_COLORS;
        *next_color = next_color.wrapping_add(1);
        if !live.contains(&candidate) {
            return candidate;
        }
    }
    *next_color % LANE_COLORS
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git graph::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_git/src/graph.rs rust/crates/tiller_git/src/lib.rs
git commit -m "feat: lay out commit graph lanes"
```

---

### Task 4: Reading one commit's diff

**Files:**
- Modify: `rust/crates/tiller_git/src/diff.rs` (append; do not restructure existing functions)
- Modify: `rust/crates/tiller_git/src/lib.rs` (extend the `pub use diff::{...}` list)
- Test: `rust/crates/tiller_git/tests/log_integration.rs` (append)

**Interfaces:**
- Consumes: `parse_diff(output: &str, path: &Path) -> FileDiff` and `FileDiff` (existing, `diff.rs:149`).
- Produces: `pub fn commit_files(repo: &Path, sha: &str) -> Result<Vec<(char, PathBuf)>, GitError>` — status letter and repo-relative path — and `pub fn commit_diff_entry(repo: &Path, sha: &str, path: &Path) -> Result<FileDiff, GitError>`. Task 12 calls both.

- [ ] **Step 1: Write the failing tests**

Append to `rust/crates/tiller_git/tests/log_integration.rs`:

```rust
#[test]
fn lists_the_files_a_commit_touched() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "main~1");

    let files = tiller_git::commit_files(dir.path(), &sha).expect("files");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, 'A');
    assert_eq!(files[0].1, PathBuf::from("c.txt"));
}

#[test]
fn reads_the_patch_of_one_file_in_a_commit() {
    let dir = repo_with_a_merge();
    let sha = head_sha(dir.path(), "main~1");

    let diff = tiller_git::commit_diff_entry(dir.path(), &sha, Path::new("c.txt")).expect("diff");

    assert!(!diff.hunks.is_empty(), "an added file has one hunk");
}

#[test]
fn the_root_commit_diffs_against_nothing_without_erroring() {
    let dir = repo_with_a_merge();
    let root = head_sha(dir.path(), "main^{/c1}");

    let files = tiller_git::commit_files(dir.path(), &root).expect("files");
    let diff = tiller_git::commit_diff_entry(dir.path(), &root, Path::new("a.txt")).expect("diff");

    assert!(files.iter().any(|(_, path)| path == Path::new("a.txt")));
    assert!(!diff.hunks.is_empty());
}

/// Resolves a revision to its full sha.
fn head_sha(dir: &Path, revision: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", revision])
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "rev-parse {revision} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_git --test log_integration`
Expected: FAIL to compile — `cannot find function commit_files in crate tiller_git`.

- [ ] **Step 3: Write the implementation**

Append to `rust/crates/tiller_git/src/diff.rs`:

```rust
/// The files one commit touched, as `(status letter, repo-relative path)`.
///
/// `git show --name-status --format=` prints the name-status block with no
/// header. For a merge commit git prints nothing here by default (a merge has
/// no single diff to show) — that is reported as an empty list, which is what
/// the UI should display.
pub fn commit_files(repo: &Path, sha: &str) -> Result<Vec<(char, PathBuf)>, GitError> {
    let output = git::run_accepting(&["show", "--name-status", "--format=", sha], repo, &[0])?;
    Ok(output
        .stdout_string()
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.chars().next()?;
            let path = parts.next()?;
            Some((status, PathBuf::from(path)))
        })
        .collect())
}

/// The unified diff of one path within one commit.
///
/// `git show --format= <sha> -- <path>` is used rather than
/// `git diff <sha>^ <sha>`: the latter has no `<sha>^` to name on a root
/// commit and fails there, while `show` renders the root commit's full
/// content as additions.
pub fn commit_diff_entry(repo: &Path, sha: &str, path: &Path) -> Result<FileDiff, GitError> {
    let path_arg = path.to_string_lossy().into_owned();
    let output = git::run_accepting(
        &["show", "--format=", "--patch", sha, "--", &path_arg],
        repo,
        &[0],
    )?;
    Ok(parse_diff(&output.stdout_string(), path))
}
```

Extend the existing `pub use diff::{...}` in `lib.rs` with `commit_diff_entry, commit_files` (keep the list alphabetical).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git --test log_integration`
Expected: 7 passed (4 from Task 2, 3 new).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_git/src/diff.rs rust/crates/tiller_git/src/lib.rs rust/crates/tiller_git/tests/log_integration.rs
git commit -m "feat: read a single commit's files and patches"
```

---

### Task 5: Four new icons

**Files:**
- Create: `rust/assets/icons/zed/file_tree.svg`, `thread.svg`, `diff.svg`, `git_graph.svg`
- Modify: `rust/crates/tiller_ui/src/icons.rs`

**Interfaces:**
- Produces: `Icon::FileTree`, `Icon::Thread`, `Icon::Diff`, `Icon::GitGraph`. Task 7 renders them.

- [ ] **Step 1: Fetch the SVGs from the pinned commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller-rust-gpui
REV=875e2a1c458ffcf2bfe90be841eb9893f399b4ff
for name in file_tree thread diff git_graph; do
  curl -fsSL "https://raw.githubusercontent.com/zed-industries/zed/$REV/assets/icons/$name.svg" \
    -o "rust/assets/icons/zed/$name.svg"
done
head -1 rust/assets/icons/zed/git_graph.svg
```

Expected: each file exists and starts with `<svg`. `ATTRIBUTION.md` needs no edit — the commit it records is the commit these came from.

- [ ] **Step 2: Write the failing test**

In `rust/crates/tiller_ui/src/icons.rs`, inside the existing `mod tests` (append):

```rust
#[test]
fn the_panel_rail_icons_resolve_to_embedded_zed_assets() {
    for icon in [Icon::FileTree, Icon::Thread, Icon::Diff, Icon::GitGraph] {
        assert!(icon.path().starts_with("icons/zed/"), "{icon:?} must come from the Zed catalog");
        assert!(!icon.svg().is_empty(), "{icon:?} must embed its bytes");
        assert!(
            icon.svg().starts_with(b"<svg"),
            "{icon:?} must embed an SVG document"
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd rust && cargo test -p tiller_ui icons::`
Expected: FAIL to compile — `no variant named FileTree found for enum Icon`.

- [ ] **Step 4: Add the variants**

In `icons.rs`, add to the `Icon` enum (after `Lock`, before `FileType`):

```rust
    /// The Files view in the right panel's rail (`zed/file_tree.svg`).
    FileTree,
    /// The Activity view in the right panel's rail (`zed/thread.svg`).
    Thread,
    /// The Diff view in the right panel's rail (`zed/diff.svg`).
    Diff,
    /// The History view in the right panel's rail (`zed/git_graph.svg`).
    GitGraph,
```

Add to `path()`:

```rust
            Icon::FileTree => "icons/zed/file_tree.svg",
            Icon::Thread => "icons/zed/thread.svg",
            Icon::Diff => "icons/zed/diff.svg",
            Icon::GitGraph => "icons/zed/git_graph.svg",
```

Add to `svg()`:

```rust
            Icon::FileTree => include_bytes!("../../../assets/icons/zed/file_tree.svg"),
            Icon::Thread => include_bytes!("../../../assets/icons/zed/thread.svg"),
            Icon::Diff => include_bytes!("../../../assets/icons/zed/diff.svg"),
            Icon::GitGraph => include_bytes!("../../../assets/icons/zed/git_graph.svg"),
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd rust && cargo test -p tiller_ui icons::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/assets/icons/zed rust/crates/tiller_ui/src/icons.rs
git commit -m "feat: add rail icons for the right panel views"
```

---

### Task 6: Split `right_panel.rs` into a module

**Files:**
- Create: `rust/crates/tiller_ui/src/right_panel/mod.rs`, `files.rs`, `activity.rs`
- Delete: `rust/crates/tiller_ui/src/right_panel.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: the same public API (`RightPanel`, `RightPanelEvent`, `RightPanelActionEvent`, `ActivitySurface`, `ActivityStatus`) re-exported from `mod.rs`, so `tiller_ui/src/lib.rs` and `main.rs` are untouched.

This task moves code and changes no behaviour. Its gate is the test count.

- [ ] **Step 1: Record the test count before**

```bash
cd rust && cargo test -p tiller_ui --lib -- --list | grep -c 'right_panel.*: test'
```

Write the number down; Step 5 must match it.

- [ ] **Step 2: Move the code**

- `mod.rs` keeps: `ActivityStatus`, `RightPanelEvent`, `RightPanelActionEvent`, `ActivitySurface`, `struct RightPanel` with all its fields, every `pub fn` (`new`, `with_activity`, `set_activity`, `clear_worktree`, `bind_worktree`, `refresh`), `impl EventEmitter`, `impl Render`, the constants `PANEL_WIDTH`, `HEADER_HEIGHT`, `ACTIVITY_ROW_HEIGHT`, and `render_header`.
- `files.rs` takes: `FileNode`, `GitMarkers`, `FileContextMenu`, `render_files`, `render_file_row`, `render_file_context_menu`, `files_action_button`, the walk helpers and `ensure_tree_refresh`.
- `activity.rs` takes: `render_activity`, `render_activity_row`.

Rules for the move: functions stay `impl RightPanel` methods, split across `impl` blocks in the three files (legal in Rust within one crate). Each file starts with `use super::*;`. Tests move with the code they test, into a `#[cfg(test)] mod tests` in the file that now owns the subject.

- [ ] **Step 3: Build**

Run: `cd rust && cargo build -p tiller_ui`
Expected: compiles. If `main.rs` needs any change, the split is wrong — revert and re-split.

- [ ] **Step 4: Run the crate's tests**

Run: `cd rust && cargo test -p tiller_ui`
Expected: PASS, no test removed.

- [ ] **Step 5: Verify the test count is unchanged**

```bash
cd rust && cargo test -p tiller_ui --lib -- --list | grep -c 'right_panel.*: test'
```

Expected: the same number as Step 1. A smaller number means a `mod tests` was not registered and those tests silently stopped running.

- [ ] **Step 6: Commit**

```bash
git add -A rust/crates/tiller_ui/src
git commit -m "refactor: split right panel into a module"
```

---

### Task 7: The icon rail, Files and Activity views

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/mod.rs`, `activity.rs`

**Interfaces:**
- Consumes: `Icon::{FileTree, Thread, Diff, GitGraph}` (Task 5).
- Produces: `pub enum PanelView { Files, Activity, Diff, History }` with `PanelView::get(cx) -> Self` and `PanelView::set(view, cx)`. Tasks 8 and 10 read it.

- [ ] **Step 1: Write the failing tests**

In `right_panel/mod.rs`'s test module:

```rust
#[gpui::test]
fn the_rail_switches_the_selected_view(cx: &mut TestAppContext) {
    cx.update(Theme::init);

    cx.update(|cx| {
        assert_eq!(PanelView::get(cx), PanelView::Files, "Files is the default");
        PanelView::set(PanelView::History, cx);
        assert_eq!(PanelView::get(cx), PanelView::History);
    });
}

#[gpui::test]
fn the_selection_survives_rebinding_to_another_worktree(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let dir = TempDir::new();
    let other = TempDir::new();
    let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

    cx.update(|cx| PanelView::set(PanelView::Diff, cx));
    panel.update(cx, |panel, cx| panel.bind_worktree(other.0.clone(), cx));

    cx.update(|cx| assert_eq!(PanelView::get(cx), PanelView::Diff));
}

#[gpui::test]
fn the_activity_badge_appears_only_for_attention_states(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let dir = TempDir::new();
    let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

    let idle = ActivitySurface::new(Icon::SquareTerminal, "one".into(), "".into(), ActivityStatus::Idle);
    let waiting = ActivitySurface::new(Icon::SquareTerminal, "two".into(), "".into(), ActivityStatus::NeedsInput);

    panel.update(cx, |panel, cx| {
        panel.set_activity(vec![idle.clone()], cx);
        assert!(panel.activity_badge().is_none(), "idle rows raise no badge");
        panel.set_activity(vec![idle, waiting], cx);
        assert!(panel.activity_badge().is_some(), "a waiting agent raises a badge");
    });
}
```

Adjust the `ActivitySurface::new` call to the constructor's real argument order (`right_panel/mod.rs`, `pub fn new` at the former `right_panel.rs:81`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_ui right_panel::`
Expected: FAIL to compile — `cannot find type PanelView`.

- [ ] **Step 3: Implement `PanelView` and the badge**

In `right_panel/mod.rs`:

```rust
/// Which view the right panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelView {
    #[default]
    Files,
    Activity,
    Diff,
    History,
}

/// App-wide selection. A GPUI global rather than a field, for the same reason
/// `DiffViewMode` is one: `select_worktree` throws the whole `RightPanel`
/// entity away and builds a fresh one, so a field would snap back to Files on
/// every worktree switch.
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
```

Add to `impl RightPanel`:

```rust
    /// The colour of the Activity rail badge, or `None` when nothing wants
    /// attention. Error outranks NeedsInput: one failed surface is the more
    /// urgent fact.
    pub(crate) fn activity_badge(&self) -> Option<ActivityStatus> {
        if self.activity.iter().any(|row| row.status == ActivityStatus::Error) {
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
```

Replace `render_header` with the rail (title text and the `✕` both go away):

```rust
    fn render_header(&self, entity: gpui::Entity<Self>, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
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
            .border_color(theme.hairline)
            .children(PanelView::ORDER.map(|view| {
                let is_active = view == active;
                let badge_color = (view == PanelView::Activity)
                    .then_some(badge)
                    .flatten()
                    .map(|status| match status {
                        ActivityStatus::Error => theme.tab_error,
                        _ => theme.tab_needs_input,
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
                    .when(is_active, |this| this.bg(theme.row_hover))
                    .hover(|style| style.bg(theme.row_hover))
                    .child(
                        IconElement::new(view.icon(), IconSize::Small).text_color(if is_active {
                            theme.title
                        } else {
                            theme.subtitle
                        }),
                    )
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
```

In `Render::render`, gate the tree walk on the active view — the spec requires the other three views not to pay for the once-a-second walk, and today the call is made whenever a worktree is selected:

```rust
        if self.worktree_selected && PanelView::get(cx) == PanelView::Files {
            self.ensure_tree_refresh(cx);
        }
```

Then replace the fixed body and the always-present activity footer with a match on `PanelView::get(cx)`, keeping the existing no-worktree empty state ahead of it:

```rust
            .child(self.render_header(entity.clone(), theme, cx))
            .child(if !self.worktree_selected {
                self.render_no_worktree(theme).into_any_element()
            } else {
                match PanelView::get(cx) {
                    PanelView::Files => self.render_files(entity.clone(), theme, cx).into_any_element(),
                    PanelView::Activity => self.render_activity(entity.clone(), theme).into_any_element(),
                    PanelView::Diff => self.render_diff(theme, cx).into_any_element(),
                    PanelView::History => self.render_history(theme, cx).into_any_element(),
                }
            })
```

Extract the existing "No worktree selected" block into `fn render_no_worktree(&self, theme: Theme) -> impl IntoElement` unchanged. Add temporary stubs so this task compiles on its own:

```rust
    /// Filled in by Task 8.
    fn render_diff(&mut self, theme: Theme, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_placeholder("Diff", theme)
    }

    /// Filled in by Task 10.
    fn render_history(&mut self, theme: Theme, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_placeholder("History", theme)
    }

    fn render_placeholder(&self, label: &'static str, theme: Theme) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.meta)
            .child(label)
    }
```

In `activity.rs`, drop the collapsible header, the chevron and the `activity_expanded` field: `render_activity` now returns the rows as a full-height list. Its `SelectActivity` / `CloseActivity` emissions do not change.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::`
Expected: PASS, including the tests that moved in Task 6.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel
git commit -m "feat: switch right panel views from an icon rail"
```

---

### Task 8: The Diff view

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/mod.rs`
- Modify: `rust/crates/tiller/src/main.rs` (one new match arm)

**Interfaces:**
- Consumes: `ChangesTab::new(repo_root: PathBuf, cx) -> Self` (`changes.rs:397`), `PanelView` (Task 7).
- Produces: `RightPanelActionEvent::ResolveInTerminal(PathBuf)`.

- [ ] **Step 1: Write the failing test**

In `right_panel/mod.rs`'s tests:

```rust
#[gpui::test]
fn the_changes_entity_is_built_only_when_the_diff_view_is_selected(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let dir = TempDir::new();
    let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

    panel.update(cx, |panel, _| assert!(panel.changes.is_none(), "nothing built up front"));

    cx.update(|cx| PanelView::set(PanelView::Diff, cx));
    panel.update(cx, |panel, cx| {
        panel.ensure_changes(cx);
        assert!(panel.changes.is_some(), "selecting Diff builds it");
    });
}

#[gpui::test]
fn rebinding_a_worktree_drops_the_changes_entity(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let dir = TempDir::new();
    let other = TempDir::new();
    let panel = cx.new(|_| RightPanel::new(dir.0.clone()));

    panel.update(cx, |panel, cx| {
        panel.ensure_changes(cx);
        panel.bind_worktree(other.0.clone(), cx);
        assert!(panel.changes.is_none(), "a stale checkout's diff must not survive");
    });
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_ui right_panel::`
Expected: FAIL to compile — `no field changes on type RightPanel`.

- [ ] **Step 3: Implement**

Add the fields to `RightPanel`:

```rust
    /// Built on first selection of the Diff view, dropped when the checkout
    /// changes. A user who never opens Diff never pays for a git status here.
    changes: Option<gpui::Entity<crate::changes::ChangesTab>>,
    /// Kept alive so the child's events keep reaching `re_emit`.
    changes_subscriptions: Vec<gpui::Subscription>,
```

Initialise both to `None` / `Vec::new()` in `new`, and clear both in `clear_worktree` and `bind_worktree` next to `file_tree.clear()`.

```rust
    /// Builds the Diff view's `ChangesTab` if it is not built yet, and wires
    /// its events onto the channels the host already handles.
    fn ensure_changes(&mut self, cx: &mut Context<Self>) -> gpui::Entity<crate::changes::ChangesTab> {
        if let Some(changes) = self.changes.clone() {
            return changes;
        }
        let repo_root = self.repo_root.clone();
        let changes = cx.new(|cx| crate::changes::ChangesTab::new(repo_root, cx));
        self.changes_subscriptions = vec![
            cx.subscribe(&changes, |_, _, event: &ChangesTabEvent, cx| match event {
                ChangesTabEvent::OpenFile(path) => {
                    cx.emit(RightPanelEvent::OpenFile(path.clone()))
                }
            }),
            cx.subscribe(&changes, |_, _, event: &ChangesTabActionEvent, cx| match event {
                ChangesTabActionEvent::OpenDiff(path) => {
                    cx.emit(RightPanelActionEvent::OpenDiff(path.clone()))
                }
                ChangesTabActionEvent::ResolveInTerminal(path) => {
                    cx.emit(RightPanelActionEvent::ResolveInTerminal(path.clone()))
                }
            }),
        ];
        self.changes = Some(changes.clone());
        changes
    }
```

Replace the Task 7 stub:

```rust
    fn render_diff(&mut self, _theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex_1().min_h(px(0.0)).child(self.ensure_changes(cx))
    }
```

Add the variant to `RightPanelActionEvent`:

```rust
    /// Open a terminal prepared to resolve this conflicted path.
    ResolveInTerminal(PathBuf),
```

In `main.rs`, extend the existing `RightPanelActionEvent` subscription (near `main.rs:4283`) with the new arm, reusing the handler `ChangesTabActionEvent::ResolveInTerminal` already calls at `main.rs:6727`:

```rust
                RightPanelActionEvent::ResolveInTerminal(path) => {
                    workspace.add_conflict_terminal_tab(path.clone(), cx)
                }
```

This is the method the existing `ChangesTabActionEvent::ResolveInTerminal` arm calls at `main.rs:6728`; verified, do not substitute another name.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel:: && cargo build -p tiller`
Expected: tests PASS, `tiller` builds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel rust/crates/tiller/src/main.rs
git commit -m "feat: mount the changes surface in the right panel"
```

---

### Task 9: Lane colours from the theme

**Files:**
- Modify: `rust/crates/tiller_theme/src/lib.rs`

**Interfaces:**
- Produces: `pub fn graph_lane(&self, index: usize) -> Rgba` on `Theme`. Task 11 calls it.

- [ ] **Step 1: Write the failing test**

In `tiller_theme`'s test module:

```rust
#[test]
fn graph_lane_colours_cycle_and_stay_distinct() {
    let theme = Theme::dark();

    let lanes: Vec<_> = (0..6).map(|index| theme.graph_lane(index)).collect();

    for (i, first) in lanes.iter().enumerate() {
        for second in lanes.iter().skip(i + 1) {
            assert_ne!(first, second, "lane colours must be distinguishable");
        }
    }
    assert_eq!(theme.graph_lane(6), theme.graph_lane(0), "the palette cycles");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p tiller_theme graph_lane`
Expected: FAIL to compile — `no method named graph_lane`.

- [ ] **Step 3: Implement**

In `impl Theme`:

```rust
    /// Colour of commit-graph lane `index`, cycling.
    ///
    /// Deliberately an accessor over hues the palette already measures rather
    /// than six new entries: `tiller_theme` pins exact RGB triples for its
    /// semantic tokens against a frozen record, and every added colour is a
    /// colour someone must measure and justify.
    pub fn graph_lane(&self, index: usize) -> Rgba {
        let lanes = [
            self.tab_focus_accent,
            self.git_untracked,
            self.tab_done,
            self.tab_needs_input,
            self.tab_error,
            self.favorite,
        ];
        lanes[index % lanes.len()]
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd rust && cargo test -p tiller_theme graph_lane`
Expected: PASS. If two hues collide, reorder within the existing measured set — do not add a new colour.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_theme/src/lib.rs
git commit -m "feat: expose commit graph lane colours on the theme"
```

---

### Task 10: The History view — data and rows

**Files:**
- Create: `rust/crates/tiller_ui/src/right_panel/history.rs`
- Modify: `rust/crates/tiller_ui/src/right_panel/mod.rs`
- Modify: `rust/crates/tiller_ui/Cargo.toml` — no change needed; `tiller_git` is already a dependency

**Interfaces:**
- Consumes: `GitLog::commits`, `GitLog::has_commits`, `CommitRecord` (Tasks 1-2), `layout`, `GraphRow` (Task 3), `PanelView` (Task 7).
- Produces: `pub(crate) struct GitHistory` with `pub(crate) fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self`, emitting `pub(crate) enum GitHistoryEvent { OpenCommit(String) }`.

- [ ] **Step 1: Write the failing tests**

In `history.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    // TempDir + git() helpers copied from right_panel/mod.rs's tests.

    #[gpui::test]
    fn a_repository_without_commits_reports_the_empty_state(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        git(&dir.0, &["init", "-q", "-b", "main"]);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.read_with(cx, |history, _| {
            assert!(history.commits.is_empty());
            assert_eq!(history.empty_reason, Some(EmptyReason::NoCommits));
            assert!(history.error.is_none(), "an unborn HEAD is not an error");
        });
    }

    #[gpui::test]
    fn a_directory_without_git_reports_not_a_repository(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.read_with(cx, |history, _| {
            assert_eq!(history.empty_reason, Some(EmptyReason::NotARepository));
        });
    }

    #[gpui::test]
    fn commits_are_loaded_with_a_graph_row_each(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));

        pump_until(cx, || {
            history.read_with(cx, |history, _| !history.commits.is_empty())
        });

        history.read_with(cx, |history, _| {
            assert_eq!(history.commits.len(), history.rows.len(), "one graph row per commit");
            assert_eq!(history.commits[0].subject, "second");
        });
    }

    #[gpui::test]
    fn a_failed_follow_up_chunk_keeps_the_commits_already_shown(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| !history.commits.is_empty())
        });

        // Break the repository underneath a *second* chunk, then drive the
        // real load path. Calling a setter that assigns `pagination_error`
        // would pass even if the production branch discarded the commits —
        // this is the only version of the test that can actually fail.
        std::fs::remove_dir_all(dir.0.join(".git")).expect("remove .git");
        history.update(cx, |history, cx| {
            history.exhausted = false;
            history.load_next_chunk(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.pagination_error.is_some())
        });

        history.read_with(cx, |history, _| {
            assert_eq!(history.commits.len(), 2, "existing rows survive a failed chunk");
            assert!(history.error.is_none(), "the whole view is not an error state");
        });
    }
}
```

Copy `pump_until` from `right_panel/mod.rs`'s test helpers (it already exists there for the tree walk) and write `seed_two_commits` with the `git` helper: `init -q -b main`, two `config` calls, write `a.txt`, `add .`, `commit -q -m first`, write `b.txt`, `add .`, `commit -q -m second`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_ui history::`
Expected: FAIL to compile — `cannot find type GitHistory`.

- [ ] **Step 3: Implement**

```rust
//! The History view: `git log` over local refs, laid out as a commit graph.

use std::path::PathBuf;

use gpui::{
    App, Context, EventEmitter, IntoElement, ParentElement as _, Render, Styled as _, Task, Window,
    div, px, uniform_list,
};
use tiller_git::{CommitRecord, GitLog, GraphRow, layout};
use tiller_theme::Theme;

/// Commits requested per chunk.
const CHUNK: usize = 500;
/// Height of one commit row.
const ROW_HEIGHT: f32 = 26.0;

/// Why the list is empty, when it is empty for a reason worth naming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyReason {
    NotARepository,
    NoCommits,
}

/// Emitted upward when a commit row is clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GitHistoryEvent {
    OpenCommit(String),
}

pub(crate) struct GitHistory {
    repo_root: PathBuf,
    pub(crate) commits: Vec<CommitRecord>,
    pub(crate) rows: Vec<GraphRow>,
    pub(crate) error: Option<String>,
    /// A follow-up chunk that failed. Kept apart from `error` so the commits
    /// already on screen are not thrown away for it.
    pub(crate) pagination_error: Option<String>,
    pub(crate) empty_reason: Option<EmptyReason>,
    /// Whether the first load has finished, successfully or not. Gates the
    /// "Loading history…" placeholder, for the same reason `settled` does in
    /// the Files tree: gating on "a load is in flight" flashes the
    /// placeholder over good content.
    pub(crate) settled: bool,
    load_task: Option<Task<()>>,
    /// Bumped on every load request; a chunk that completes after a newer
    /// request (or after a rebind to another repository) is discarded.
    generation: u64,
    exhausted: bool,
}

impl GitHistory {
    pub(crate) fn new(repo_root: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut history = Self {
            repo_root,
            commits: Vec::new(),
            rows: Vec::new(),
            error: None,
            pagination_error: None,
            empty_reason: None,
            settled: false,
            load_task: None,
            generation: 0,
            exhausted: false,
        };
        history.load_next_chunk(cx);
        history
    }

    /// Requests the next chunk. Single-flight: a request while one is in
    /// flight is dropped, not queued.
    pub(crate) fn load_next_chunk(&mut self, cx: &mut Context<Self>) {
        if self.load_task.is_some() || self.exhausted {
            return;
        }
        self.pagination_error = None;
        let repo_root = self.repo_root.clone();
        let skip = self.commits.len();
        self.generation += 1;
        let generation = self.generation;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { GitLog::commits(&repo_root, skip, CHUNK) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                this.settled = true;
                if generation != this.generation {
                    return;
                }
                match loaded {
                    Ok(commits) => {
                        if commits.len() < CHUNK {
                            this.exhausted = true;
                        }
                        this.commits.extend(commits);
                        this.rows = layout(&this.commits);
                        // `commits` succeeding on an empty list means an
                        // unborn HEAD (`GitLog::commits` maps that case to
                        // `Ok(vec![])`); a directory that is not a repository
                        // arrives on the `Err` arm instead.
                        this.empty_reason = this
                            .commits
                            .is_empty()
                            .then_some(EmptyReason::NoCommits);
                        this.error = None;
                    }
                    Err(error) => {
                        if this.commits.is_empty() {
                            this.empty_reason = Some(EmptyReason::NotARepository);
                            this.error = Some(error.to_string());
                        } else {
                            this.pagination_error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

}

impl EventEmitter<GitHistoryEvent> for GitHistory {}
```

`Render` draws, in order: the error panel with a Retry button when `error.is_some()`; "Loading history…" while `!settled`; "No commits yet" / "Not a git repository" from `empty_reason`; otherwise a `uniform_list` of `commits.len()` rows at `px(ROW_HEIGHT)`. Each row is a `div` with a left-hand graph column (Task 11 fills it; leave a fixed-width empty `div` here), then the subject (`flex_1`, truncated, `theme.meta` when `commit.parents.len() > 1`), the author, and the date. A row's `on_mouse_down` emits `GitHistoryEvent::OpenCommit(commit.sha.clone())`. When the list reaches its last index and `!exhausted`, call `load_next_chunk`.

In `right_panel/mod.rs`, mirror Task 8: an `Option<Entity<GitHistory>>` field with an `ensure_history` builder, cleared in `clear_worktree` / `bind_worktree`, a subscription re-emitting `GitHistoryEvent::OpenCommit` as `RightPanelActionEvent::OpenCommit(String)`, and `render_history` replacing the stub.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui history::`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel
git commit -m "feat: list commit history in the right panel"
```

---

### Task 11: Painting the graph column

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs`

**Interfaces:**
- Consumes: `GraphRow` (Task 3), `Theme::graph_lane` (Task 9).
- Produces: `fn graph_column(row: &GraphRow, theme: Theme) -> impl IntoElement` and `fn graph_width(rows: &[GraphRow]) -> f32`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_graph_column_is_capped_at_six_lanes() {
    let wide = GraphRow {
        lane: 0,
        color: 0,
        through: (0..12).map(|i| Some(i % 6)).collect(),
        joins_in: Vec::new(),
        edges_out: Vec::new(),
    };

    let width = graph_width(std::slice::from_ref(&wide));

    assert_eq!(width, 6.0 * LANE_WIDTH, "beyond six lanes the column stops growing");
}

#[test]
fn a_single_lane_column_is_one_lane_wide() {
    let narrow = GraphRow { lane: 0, color: 0, through: vec![Some(0)], joins_in: Vec::new(), edges_out: Vec::new() };

    assert_eq!(graph_width(std::slice::from_ref(&narrow)), LANE_WIDTH);
}

#[test]
fn holes_left_by_ended_lanes_do_not_widen_the_column() {
    // Two branches ended; only column 0 is still live. The lane vector keeps
    // their holes, and the column must not stay three lanes wide for them.
    let settled = GraphRow {
        lane: 0,
        color: 0,
        through: vec![Some(0), None, None],
        joins_in: Vec::new(),
        edges_out: Vec::new(),
    };

    assert_eq!(graph_width(std::slice::from_ref(&settled)), LANE_WIDTH);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_ui history::`
Expected: FAIL to compile — `cannot find function graph_width`.

- [ ] **Step 3: Implement**

```rust
/// Horizontal pitch between lanes.
const LANE_WIDTH: f32 = 12.0;
/// Lanes drawn before the column stops growing. A twelve-column graph in a
/// 405px panel is noise, not information, so excess lanes are not drawn.
const MAX_LANES: usize = 6;
/// Radius of a commit node.
const NODE_RADIUS: f32 = 3.5;

fn graph_width(rows: &[GraphRow]) -> f32 {
    let peak = rows
        .iter()
        .map(|row| {
            // Occupied columns, not `through.len()`: the lane vector keeps
            // holes forever (it never shrinks), so its length would make the
            // column grow monotonically and stay wide long after the branches
            // that caused it ended.
            let occupied = row
                .through
                .iter()
                .rposition(Option::is_some)
                .map_or(0, |index| index + 1);
            occupied.max(row.lane + 1)
        })
        .max()
        .unwrap_or(1)
        .clamp(1, MAX_LANES);
    peak as f32 * LANE_WIDTH
}
```

`graph_column` returns a `canvas(|_, _, _| (), move |bounds, _, window, _| { ... })` that paints, for the row's height:

- one vertical line per `Some(colour)` in `through`, from the row's vertical centre to its bottom edge, at `x = column * LANE_WIDTH + LANE_WIDTH / 2`, skipping columns `>= MAX_LANES`;
- one vertical line from the top edge to the centre for every column that was continuing above (pass the previous row's `through` in, or paint top halves from the same `through` of the row above — whichever the element structure makes available; the visual requirement is that a lane is continuous across the row boundary);
- a quadratic Bézier from each `joins_in` column at the top edge to the node at the centre, and from the node to each `edges_out` column at the bottom edge, built with `gpui::Path::new` + `curve_to`;
- the node: a filled circle of `NODE_RADIUS` at the row's lane, `theme.graph_lane(row.color)`, drawn last so lines pass behind it.

Every stroke colour comes from `theme.graph_lane(colour_index)`. No literal colour.

- [ ] **Step 4: Run the tests and look at it**

Run: `cd rust && cargo test -p tiller_ui history::`
Expected: PASS.

Then run the app (`Scripts/build-dev.sh`), open the History view on this repository, and confirm: lanes are continuous across rows, merges join visibly, no lane past the sixth is drawn, colours differ between adjacent lanes.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat: paint the commit graph lanes"
```

---

### Task 12: Opening a commit in a tab

**Files:**
- Modify: `rust/crates/tiller_ui/src/changes.rs`
- Modify: `rust/crates/tiller/src/main.rs`

**Interfaces:**
- Consumes: `commit_files`, `commit_diff_entry` (Task 4), `RightPanelActionEvent::OpenCommit` (Task 10).
- Produces: `ChangesTab::for_commit(repo_root: PathBuf, sha: String, cx: &mut Context<Self>) -> Self`.

- [ ] **Step 1: Write the failing test**

In `changes.rs`'s tests:

```rust
#[gpui::test]
fn a_commit_view_lists_that_commit_s_files_and_forbids_staging(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let dir = TempDir::new();
    seed_two_commits(&dir.0);
    let sha = rev_parse(&dir.0, "HEAD");

    let tab = cx.new(|cx| ChangesTab::for_commit(dir.0.clone(), sha, cx));
    pump_until(cx, || {
        tab.read_with(cx, |tab, _| {
            tab.report().sections.iter().any(|section| !section.files.is_empty())
        })
    });

    tab.read_with(cx, |tab, _| {
        let report = tab.report();
        let files: Vec<_> = report
            .sections
            .iter()
            .flat_map(|section| section.files.iter())
            .collect();
        assert!(files.iter().any(|file| file.path.ends_with("b.txt")));
        assert!(!tab.allows_staging(), "a commit is immutable");
    });
}
```

`ChangesReport` (`changes.rs:221`) has no `files` field — it has `sections: Vec<ChangesSectionReport>`, and the files live one level down. Add `allows_staging()` as part of this task.

`changes.rs`'s test module has `TempDir` and `pump_until` but no `seed_two_commits` or `rev_parse`. Add both there, in the shape used by `right_panel/history.rs`'s tests: `seed_two_commits` runs `init -q -b main`, two `config` calls, then two write/add/commit cycles producing `a.txt` ("first") and `b.txt` ("second"); `rev_parse` shells out to `git rev-parse <revision>` and returns the trimmed stdout.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p tiller_ui changes::`
Expected: FAIL to compile — `no function for_commit`.

- [ ] **Step 3: Implement**

Add a source discriminant to `ChangesTab`:

```rust
/// What a Changes surface is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ChangesSource {
    /// The working tree: `git status` plus per-entry diffs, mutable.
    WorkingTree,
    /// One commit, by sha. Immutable: stage/unstage/discard are refused.
    Commit(String),
}
```

Store it, default `WorkingTree` in `new`, set `Commit(sha)` in `for_commit`. The refresh path branches on it: `WorkingTree` keeps today's `status` + `diff_entry` calls; `Commit(sha)` calls `commit_files` and `commit_diff_entry`. `allows_staging()` returns `matches!(self.source, ChangesSource::WorkingTree)`, and every stage/unstage/discard entry point returns early when it is false, with the buttons not rendered at all in commit mode.

In `main.rs`, handle the new event next to the existing `RightPanelActionEvent::OpenDiff` arm:

```rust
                RightPanelActionEvent::OpenCommit(sha) => {
                    workspace.add_commit_tab(sha.clone(), cx)
                }
```

The existing helper cannot be reused: `add_changes_tab(&mut self, focus_path: Option<PathBuf>, cx)` (`main.rs:7351`) builds its own `ChangesTab::new` internally and accepts no pre-built entity. Add a sibling next to it, copying its body and changing only how the entity is constructed:

```rust
    /// Opens a read-only Changes tab showing one commit.
    fn add_commit_tab(&mut self, sha: String, cx: &mut Context<Self>) {
        let changes = cx.new(|cx| ChangesTab::for_commit(self.working_directory.clone(), sha, cx));
        Self::subscribe_changes_tab(&changes, cx);
        // Mirror the remainder of `add_changes_tab` verbatim from
        // `main.rs:7351` onward: same tab title path, same `PaneContent::Changes`
        // insertion, same focus handling.
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui changes:: && cargo build -p tiller`
Expected: PASS and a clean build.

- [ ] **Step 5: Full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_ui/src/changes.rs rust/crates/tiller/src/main.rs
git commit -m "feat: open a commit's diff in a tab"
```

---

## Self-review

**Spec coverage.** Rail and removed `✕` → Task 7. Activity as a full view plus badge → Task 7. Diff mounting and event re-emission → Task 8. `git log --branches --date-order` and the record format → Tasks 1-2. Lane layout with holes, colour avoidance, precomputation → Task 3. Lane colours from the theme → Task 9. `uniform_list`, 26px rows, 6-lane cap, canvas painting → Tasks 10-11. Pagination at 500 → Task 10. Empty states, unborn HEAD, error with Retry, single-flight generation → Tasks 2 and 10. Commit tab, `commit_files` / `commit_diff_entry`, staging disabled → Tasks 4 and 12. Module split with a test count → Task 6. Icons from the pinned commit → Task 5.

**Known deviation from the spec.** The spec's `graph_lanes: [Rgba; 6]` field became `Theme::graph_lane(index)` over existing measured hues; the spec was updated to match before this plan was written.

**Type consistency.** `CommitRecord` fields are used identically in Tasks 1, 2, 3, 10. `GraphRow`'s five fields are produced in Task 3 and consumed in Tasks 10-11 under the same names. `PanelView::{get,set}` are defined in Task 7 and used in Tasks 8 and 10. `ensure_changes` / `ensure_history` follow one shape. `commit_files` returns `Vec<(char, PathBuf)>` in Task 4 and is consumed as such in Task 12.

---

## Plan corrections (2026-08-21, after review)

Applied to the tasks above after a read-only review checked every invented
API against the source. Each was verified in this repository before the
change was made:

1. `Theme::init` takes `&mut App` (`tiller_theme/src/lib.rs:989`), so
   `Theme::init(cx)` inside a `#[gpui::test]` does not compile. All test
   snippets now use this repo's convention, `cx.update(Theme::init)`
   (`right_panel.rs:1791`).
2. Task 8 named a non-existent `resolve_conflict_in_terminal`. The real
   handler is `add_conflict_terminal_tab` (`main.rs:6728`).
3. Task 7 was missing the spec's requirement that `ensure_tree_refresh` runs
   only while Files is active. Added.
4. Task 11's `canvas` prepaint closure takes three arguments, not two.
5. Task 11's `graph_width` measured `through.len()`, which never shrinks —
   the column would grow monotonically and stay wide after branches ended.
   It now measures occupied columns, with a test for the hole case.
6. Task 10's pagination-failure test called a setter and so could not fail.
   It now breaks the repository and drives the real load path; the setter
   (`apply_chunk_failure`) is deleted.
7. Task 12 used `ChangesReport::files`, which does not exist — files live in
   `sections[].files` (`changes.rs:213-226`) — and reused
   `add_changes_tab`, whose signature (`main.rs:7351`) builds its own entity
   and cannot accept a pre-built one. Both corrected, and the two missing
   test helpers are now specified.

### Correction pending in Task 3's delivered code

`a_closed_lane_leaves_a_hole_that_is_reused` asserts the reused column is
`1`. That assertion is wrong: after every lane has closed, leftmost-free
reuse yields column `0`, which is what the algorithm should do and what the
graph should draw. The implementer satisfied the assertion by adding a
special case to `free_column` that returns the rightmost historical column
when all lanes are empty — a disconnected history would then be drawn at the
far right instead of at the left edge. At integration: delete that special
case and change the assertion to `0`.

# History Filter — Part 1: Filter Model and Text Search — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the History view a working "Text or hash" search field with regex and case-sensitivity toggles, built on a filter model that the four dropdowns and IntelliSort will plug into later.

**Architecture:** A `LogFilter` struct in `tiller_git` with one pure translation into `git log` arguments; `GitLog::commits` takes it. `GitHistory` owns the live filter and treats every change as a full reset, which is what keeps `skip = self.commits.len()` true. The graph column hides while a filter is active. The toolbar is a new file so `history.rs` keeps its remit.

**Tech Stack:** Rust, [gpui](https://github.com/zed-industries/zed), `git log`.

**Spec:** `docs/superpowers/specs/2026-08-23-history-filter-toolbar-design.md`

## Why this plan is Part 1 of two

The spec covers eight controls. Split by what produces working software on its
own: **Part 1** is the filter model plus the text field — usable the moment it
lands. **Part 2** is the four dropdowns, IntelliSort's button and the
responsive layout, all of which plug into the model Part 1 builds. Eight
controls in one plan would make a document no single implementer can hold.

`LogFilter` gains its `topo_order` field here, because it belongs to the model;
only its *button* is Part 2.

## Global Constraints

- Verification gate: `cd rust && cargo test -p <crate>`. Do **not** run `Scripts/ci.sh`.
- `cargo build --workspace` does **not** compile `#[cfg(test)]` code. Verify with `cargo test`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Tests first, standard Rust `#[test]`.
- Line numbers in this plan drift as earlier tasks edit the same files. Locate with `grep -n`.
- This machine (Windows) has pre-existing red tests: ~20 in `tiller`, 9 in `tiller_ui`, plus `tiller_acp` and `tiller_terminal`. They fail on a clean tree — verify with `git stash` before blaming your change. No *new* red.
- Never add `%b` to `LOG_FORMAT`. See Task 6; the spec records the measurement.

---

### Task 1: `LogFilter` and its arguments

**Files:**
- Modify: `rust/crates/tiller_git/src/log.rs` (add the struct above `GitLog`)
- Modify: `rust/crates/tiller_git/src/lib.rs` (re-export)
- Test: same file, existing `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `LogFilter { text: Option<String>, regex: bool, case_sensitive: bool, branches: Vec<String>, authors: Vec<String>, since: Option<String>, until: Option<String>, paths: Vec<PathBuf>, topo_order: bool }`, `LogFilter::args(&self) -> Vec<String>`, `LogFilter::pathspec_args(&self) -> Vec<String>`, `LogFilter::is_filtering(&self) -> bool`. `Default` gives the unfiltered query the view runs today.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `rust/crates/tiller_git/src/log.rs`:

```rust
    #[test]
    fn a_default_filter_is_the_query_the_view_already_ran() {
        let args = LogFilter::default().args();

        assert_eq!(args, vec!["--branches", "--date-order"]);
        assert!(LogFilter::default().pathspec_args().is_empty());
    }

    /// Regex off must mean *literal*, not "basic regex" — otherwise typing
    /// `fix(ui)` silently searches for a group.
    #[test]
    fn text_without_regex_matches_fixed_strings() {
        let filter = LogFilter {
            text: Some("fix(ui)".to_owned()),
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"-F".to_owned()));
        assert!(!args.contains(&"-E".to_owned()));
        assert!(args.contains(&"--grep=fix(ui)".to_owned()));
    }

    #[test]
    fn text_with_regex_uses_extended_syntax() {
        let filter = LogFilter {
            text: Some("^feat".to_owned()),
            regex: true,
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"-E".to_owned()));
        assert!(!args.contains(&"-F".to_owned()));
    }

    #[test]
    fn case_insensitivity_is_emitted_once_and_only_when_it_can_apply() {
        let searching = LogFilter {
            text: Some("socket".to_owned()),
            ..LogFilter::default()
        };
        assert_eq!(
            searching.args().iter().filter(|arg| *arg == "-i").count(),
            1,
            "-i covers --grep and --author together, so it is emitted once"
        );

        let sensitive = LogFilter {
            case_sensitive: true,
            ..searching.clone()
        };
        assert!(!sensitive.args().contains(&"-i".to_owned()));

        assert!(
            !LogFilter::default().args().contains(&"-i".to_owned()),
            "nothing to match, so no -i"
        );
    }

    #[test]
    fn authors_repeat_and_branches_replace_the_default_selection() {
        let filter = LogFilter {
            authors: vec!["Ada".to_owned(), "Bob".to_owned()],
            branches: vec!["main".to_owned(), "release/*".to_owned()],
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"--author=Ada".to_owned()));
        assert!(args.contains(&"--author=Bob".to_owned()));
        assert!(args.contains(&"--branches=main".to_owned()));
        assert!(args.contains(&"--branches=release/*".to_owned()));
        assert!(
            !args.contains(&"--branches".to_owned()),
            "an explicit selection replaces the bare --branches, it does not add to it"
        );
    }

    /// Pathspecs are deliberately *not* in `args()`. They must follow `--`,
    /// and `--` must follow `--skip`/`-n`/`--format`, which the caller adds
    /// in between. Returning them together would put the pathspec before
    /// arguments git then reads as paths.
    #[test]
    fn pathspecs_are_kept_apart_from_the_predicate_arguments() {
        let filter = LogFilter {
            paths: vec![PathBuf::from("rust/crates/tiller_git")],
            ..LogFilter::default()
        };

        assert!(!filter.args().contains(&"--".to_owned()));
        assert_eq!(
            filter.pathspec_args(),
            vec!["--".to_owned(), "rust/crates/tiller_git".to_owned()]
        );
    }

    #[test]
    fn intellisort_swaps_the_ordering_flag() {
        let filter = LogFilter {
            topo_order: true,
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"--topo-order".to_owned()));
        assert!(!args.contains(&"--date-order".to_owned()));
    }

    /// Ordering is not filtering: a re-ordered history is still the whole
    /// history, so the graph stays and an empty result still means "no
    /// commits", not "no matches".
    #[test]
    fn ordering_alone_does_not_count_as_filtering() {
        assert!(!LogFilter::default().is_filtering());
        assert!(
            !LogFilter {
                topo_order: true,
                ..LogFilter::default()
            }
            .is_filtering()
        );
        assert!(
            LogFilter {
                text: Some("x".to_owned()),
                ..LogFilter::default()
            }
            .is_filtering()
        );
    }
```

Add `use std::path::PathBuf;` to the test module if the file does not already import it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller_git log::`
Expected: FAIL to compile — `cannot find struct LogFilter in this scope`.

- [ ] **Step 3: Write the implementation**

Add above `pub struct GitLog;` in `rust/crates/tiller_git/src/log.rs`:

```rust
/// Everything the History view can ask `git log` to narrow by.
///
/// Data plus one pure translation into arguments, so every combination is
/// testable without running git and without a window. `Default` is exactly
/// the query the view ran before filtering existed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogFilter {
    /// Free text for `--grep`. Matches the whole message, body included:
    /// git has no subject-only flag.
    pub text: Option<String>,
    /// `-E` when set, `-F` when not. Off means *literal*, not "basic regex".
    pub regex: bool,
    pub case_sensitive: bool,
    /// Empty means every local branch, as before.
    pub branches: Vec<String>,
    /// Repeated `--author`; git ORs them.
    pub authors: Vec<String>,
    /// Passed to git verbatim, so git's own relative forms ("7 days ago")
    /// work without this crate computing timestamps.
    pub since: Option<String>,
    pub until: Option<String>,
    pub paths: Vec<PathBuf>,
    /// IntelliSort: `--topo-order` keeps a merged branch's commits contiguous
    /// instead of interleaving them by date.
    pub topo_order: bool,
}

impl LogFilter {
    /// Whether any *predicate* is set. Ordering is excluded on purpose: a
    /// re-ordered history is still the whole history, so it must not hide the
    /// graph or turn an empty result into "no matches".
    pub fn is_filtering(&self) -> bool {
        self.text.is_some()
            || !self.branches.is_empty()
            || !self.authors.is_empty()
            || self.since.is_some()
            || self.until.is_some()
            || !self.paths.is_empty()
    }

    /// Revision selection, ordering and predicates — everything that must
    /// precede `--skip`/`-n`/`--format`. Pathspecs are in
    /// [`LogFilter::pathspec_args`] instead, because they must come after
    /// those, behind `--`.
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.branches.is_empty() {
            args.push("--branches".to_owned());
        } else {
            for branch in &self.branches {
                args.push(format!("--branches={branch}"));
            }
        }

        args.push(
            if self.topo_order {
                "--topo-order"
            } else {
                "--date-order"
            }
            .to_owned(),
        );

        if let Some(text) = &self.text {
            args.push(if self.regex { "-E" } else { "-F" }.to_owned());
            args.push(format!("--grep={text}"));
        }
        for author in &self.authors {
            args.push(format!("--author={author}"));
        }
        // One `-i` covers --grep and --author both; emitting it with neither
        // present would be noise in every unfiltered query.
        if !self.case_sensitive && (self.text.is_some() || !self.authors.is_empty()) {
            args.push("-i".to_owned());
        }
        if let Some(since) = &self.since {
            args.push(format!("--since={since}"));
        }
        if let Some(until) = &self.until {
            args.push(format!("--until={until}"));
        }

        args
    }

    /// `["--", <path>…]`, or empty. The caller appends this **last**, after
    /// `--skip`, `-n` and `--format`: everything after `--` is a path to git.
    pub fn pathspec_args(&self) -> Vec<String> {
        if self.paths.is_empty() {
            return Vec::new();
        }
        let mut args = vec!["--".to_owned()];
        args.extend(
            self.paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned()),
        );
        args
    }
}
```

Add `use std::path::PathBuf;` to the file's imports (it currently imports only `std::path::Path`).

- [ ] **Step 4: Re-export it**

In `rust/crates/tiller_git/src/lib.rs`, add `LogFilter` to the existing `pub use log::{…}` list beside `CommitRecord` and `GitLog`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git log::`
Expected: PASS — the eight new tests plus the ten that were already there.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_git/src/lib.rs
git commit -m "feat(git): add the log filter model"
```

---

### Task 2: `GitLog::commits` takes a filter

**Files:**
- Modify: `rust/crates/tiller_git/src/log.rs:40` (`GitLog::commits`)
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs:82` (the only caller)
- Test: `rust/crates/tiller_git/src/log.rs` tests module

**Interfaces:**
- Consumes: `LogFilter` (Task 1).
- Produces: `GitLog::commits(repo: &Path, skip: usize, limit: usize, filter: &LogFilter) -> Result<Vec<CommitRecord>, GitError>`.

- [ ] **Step 1: Write the failing test**

This one runs real git, matching the fixture style already used in
`right_panel/history.rs`'s tests. Add to `log.rs`'s tests module:

```rust
    /// The seam that matters: predicates reach git, and they compose with
    /// `--skip`/`-n` — git applies them *before* limiting, which is the whole
    /// reason filtering can live git-side without breaking pagination.
    #[test]
    fn a_text_filter_reaches_git_and_still_paginates() {
        let dir = std::env::temp_dir().join(format!("tiller-log-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp repo");

        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("spawn git");
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Tester"]);
        for subject in ["alpha one", "beta", "alpha two"] {
            std::fs::write(dir.join(subject), subject).expect("write");
            git(&["add", "."]);
            git(&["commit", "-q", "-m", subject]);
        }

        let filter = LogFilter {
            text: Some("alpha".to_owned()),
            ..LogFilter::default()
        };

        let all = GitLog::commits(&dir, 0, 10, &filter).expect("filtered log");
        assert_eq!(all.len(), 2, "only the two alpha commits match");

        let second_page = GitLog::commits(&dir, 1, 10, &filter).expect("filtered log, skipped");
        assert_eq!(
            second_page.len(),
            1,
            "--skip counts within the filtered results, not within the whole log"
        );
        assert_eq!(second_page[0].subject, "alpha one");

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_git a_text_filter_reaches_git`
Expected: FAIL to compile — `this function takes 3 arguments but 4 arguments were supplied`.

- [ ] **Step 3: Change the signature**

Replace the body of `GitLog::commits` (`log.rs:40`):

```rust
    /// Reads commits in the filter's order, including merge parents.
    pub fn commits(
        repo: &Path,
        skip: usize,
        limit: usize,
        filter: &LogFilter,
    ) -> Result<Vec<CommitRecord>, GitError> {
        let mut args = vec!["log".to_owned()];
        args.extend(filter.args());
        args.push(format!("--skip={skip}"));
        args.push(format!("-n{limit}"));
        args.push(LOG_FORMAT.to_owned());
        // Last, and only last: git reads everything after `--` as a path.
        args.extend(filter.pathspec_args());

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        match git::run_accepting(&borrowed, repo, &[0]) {
            Ok(output) => Ok(parse_log(&output.stdout_string())),
            Err(error) if is_unborn_head(&error) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }
```

- [ ] **Step 4: Update the only caller**

In `rust/crates/tiller_ui/src/right_panel/history.rs`, inside the
`background_spawn` block (`~line 82`), the call is
`GitLog::commits(&repo_root, skip, CHUNK)`. Give it the default filter for
now — Task 3 replaces this with the live one:

```rust
                    let commits = GitLog::commits(&repo_root, skip, CHUNK, &LogFilter::default());
```

and add `LogFilter` to the `use tiller_git::{…}` line at the top of the file.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git && cargo test -p tiller_ui right_panel::history`
Expected: PASS. The four existing `history.rs` tests must still pass unchanged — the default filter reproduces the previous query exactly, which Task 1's first test pins.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(git): let the log query carry a filter"
```

---

### Task 3: `GitHistory` owns a live filter

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` — `EmptyReason` (`:22`), `GitHistory` (`:33`), `new` (`:51`), `load_next_chunk` (`:70`), the empty-state match (`~:105`), the empty-state labels (`~:193`)
- Test: same file

**Interfaces:**
- Consumes: `LogFilter` (Task 1), the filtered `GitLog::commits` (Task 2).
- Produces: `GitHistory::filter: LogFilter` (field), `GitHistory::set_filter(&mut self, filter: LogFilter, cx: &mut Context<Self>)`, `EmptyReason::NoMatches`.

- [ ] **Step 1: Write the failing tests**

```rust
    /// A filter that matches nothing must not claim the repository is empty.
    #[gpui::test]
    fn a_filter_with_no_matches_is_not_an_empty_repository(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("nothing-matches-this".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.settled && history.commits.is_empty())
        });

        assert_eq!(
            history.read_with(cx, |history, _| history.empty_reason),
            Some(EmptyReason::NoMatches),
            "the repository has two commits; only the filter is empty"
        );
    }

    /// A filter change restarts the query from the top. Narrowing the loaded
    /// vector instead would break `skip = self.commits.len()`, which assumes
    /// `commits` is exactly a prefix of the log.
    #[gpui::test]
    fn changing_the_filter_reloads_from_the_first_commit(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("second".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.settled && history.commits.len() == 1)
        });

        history.read_with(cx, |history, _| {
            assert_eq!(history.commits[0].subject, "second");
            assert_eq!(
                history.rows.len(),
                history.commits.len(),
                "the graph rows are rebuilt for the new result set, not left stale"
            );
        });
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: FAIL to compile — `no variant named NoMatches`, `no method named set_filter`.

- [ ] **Step 3: Add the variant and the field**

In `EmptyReason` (`history.rs:22`):

```rust
    /// A filter is active and matched nothing. Distinct from `NoCommits`:
    /// the repository is not empty, the query is.
    NoMatches,
```

In `GitHistory` (`history.rs:33`), after `repo_root`:

```rust
    /// The live query. Changing it resets everything below — see
    /// [`GitHistory::set_filter`].
    pub(crate) filter: LogFilter,
```

Initialise `filter: LogFilter::default(),` in `new` (`history.rs:52`).

- [ ] **Step 4: Use the live filter and name the empty result**

In `load_next_chunk`, clone the filter beside `repo_root` and pass it instead of the default Task 2 left behind:

```rust
        let repo_root = self.repo_root.clone();
        let filter = self.filter.clone();
```

```rust
                    let commits = GitLog::commits(&repo_root, skip, CHUNK, &filter);
```

`has_commits` beside it stays as it is: it answers "does HEAD resolve", which
no filter changes.

Then, in the `update` closure, replace the `empty_reason` assignment:

```rust
                        this.empty_reason = match (this.commits.is_empty(), has_commits) {
                            // Order matters: on a repository with commits *and*
                            // a filter, both arms could fire, and the filter is
                            // the one that explains the emptiness.
                            (true, _) if this.filter.is_filtering() => {
                                Some(EmptyReason::NoMatches)
                            }
                            (true, Some(false)) => Some(EmptyReason::NoCommits),
                            _ => None,
                        };
```

And give the new variant a label where the other two are rendered
(`history.rs:~193`):

```rust
                EmptyReason::NoMatches => "No commits match the filter",
```

- [ ] **Step 5: Implement `set_filter`**

Add to `impl GitHistory`, after `load_next_chunk`:

```rust
    /// Replaces the filter and restarts the query from the top.
    ///
    /// This is a reset, never a narrowing of what is already loaded.
    /// `load_next_chunk` computes `skip` as `self.commits.len()`, which is
    /// only true while `commits` is exactly a prefix of the log; dropping
    /// rows out of it would make every later page skip the wrong commits.
    pub(crate) fn set_filter(&mut self, filter: LogFilter, cx: &mut Context<Self>) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        // Strand any chunk still in flight: it was fetched for the old query
        // and must not land in the new set. Dropping the task cancels it;
        // the generation bump covers a result already on its way back.
        self.generation += 1;
        self.load_task = None;
        self.commits.clear();
        self.rows.clear();
        self.exhausted = false;
        self.settled = false;
        self.empty_reason = None;
        self.error = None;
        self.pagination_error = None;
        self.load_next_chunk(cx);
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS — the four pre-existing tests plus the two new ones.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): drive the log query from a live filter"
```

---

### Task 4: Hide the graph while filtering

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` — `render` (`~:208`) and `render_history_row` (`:286`)
- Test: same file

**Interfaces:**
- Consumes: `GitHistory::filter` (Task 3).
- Produces: nothing new; `render_history_row` gains no parameter — it already takes `graph_width: f32` and now receives `0.0`.

- [ ] **Step 1: Write the failing test**

```rust
    /// On a filtered set the rows are not contiguous in history, so a lane
    /// drawn between two of them would connect commits that are not parent
    /// and child. Hiding is the only option that draws nothing false.
    #[gpui::test]
    async fn the_graph_column_disappears_while_a_filter_is_active(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let history = cx.update(|window, _| {
            window.root::<GitHistory>().flatten().expect("history root")
        });
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 2)
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("history-graph").is_some(),
            "baseline: the graph column is drawn without a filter"
        );

        history.update(&mut cx.cx, |history, cx| {
            history.set_filter(
                LogFilter {
                    text: Some("second".to_owned()),
                    ..LogFilter::default()
                },
                cx,
            );
        });
        pump_until(&cx.cx, || {
            history.read_with(&cx.cx, |history, _| history.commits.len() == 1)
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("history-graph").is_none(),
            "a filtered set draws no lanes"
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_ui the_graph_column_disappears`
Expected: FAIL on the **baseline** assertion — `debug_bounds("history-graph")` is `None` because that selector does not exist yet. That is the expected first failure; Step 3 adds the selector, and only then does the real assertion get its chance.

- [ ] **Step 3: Name the graph column and drop it while filtering**

In `render_history_row` (`history.rs:286`), the graph child is currently an
unnamed `div().w(px(graph_width))`. Give it a selector and make it optional:

```rust
        .when(graph_width > 0.0, |row| {
            row.child(
                div()
                    .id("history-graph")
                    .debug_selector(|| "history-graph".to_owned())
                    .w(px(graph_width))
                    .h_full()
                    .flex_none()
                    .child(graph_column(&row_data, theme)),
            )
        })
```

Rename the function's `row: GraphRow` parameter to `row_data` if it now
shadows the builder variable; keep the rest of the body untouched.

In `render` (`history.rs:~208`), where `graph_width` is computed:

```rust
        // Zero width means "draw no graph": the rows of a filtered set are
        // not contiguous, so any lane between them would be a lie. `rows` is
        // still computed and still zips 1:1 with `commits` — emptying it
        // would make the `zip` in the list builder yield nothing at all.
        let graph_width = if self.filter.is_filtering() {
            0.0
        } else {
            graph_width(&rows)
        };
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS, including the six graph-geometry unit tests, which test
`graph_width` directly and are unaffected.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): hide the commit graph while a filter is active"
```

---

### Task 5: The search field

**Files:**
- Create: `rust/crates/tiller_ui/src/right_panel/history_toolbar.rs`
- Modify: `rust/crates/tiller_ui/src/right_panel/mod.rs` — add `mod history_toolbar;` beside `mod history;`
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` — draft state, debounce, render the toolbar above the list
- Test: `history_toolbar.rs` and `history.rs`

**Interfaces:**
- Consumes: `GitHistory::set_filter` (Task 3).
- Produces: `history_toolbar::render_search_row(draft: &str, regex: bool, case_sensitive: bool, caret_visible: bool, focus: &FocusHandle, entity: Entity<GitHistory>, theme: Theme) -> impl IntoElement`; on `GitHistory`: `search_draft: String`, `search_regex: bool`, `search_case_sensitive: bool`, `search_blink: caret::Blink`, `apply_search_now(&mut self, cx)`, `schedule_search(&mut self, cx)`.

- [ ] **Step 1: Write the failing test**

In `history.rs`'s tests module:

```rust
    /// Three keystrokes must produce one query, not three: `--grep` walks
    /// every reachable commit, so a query per keystroke is a query per
    /// keystroke too many.
    #[gpui::test]
    fn typing_debounces_into_a_single_query(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        for draft in ["s", "se", "sec"] {
            history.update(cx, |history, cx| {
                history.search_draft = draft.to_owned();
                history.schedule_search(cx);
            });
        }
        cx.run_until_parked();
        assert_eq!(
            history.read_with(cx, |history, _| history.filter.text.clone()),
            None,
            "nothing is queried while the typing is still going"
        );

        cx.executor().advance_clock(SEARCH_DEBOUNCE);
        cx.run_until_parked();

        assert_eq!(
            history.read_with(cx, |history, _| history.filter.text.clone()),
            Some("sec".to_owned()),
            "one query, with the last draft"
        );
    }

    /// An emptied field is not a filter for the empty string.
    #[gpui::test]
    fn clearing_the_field_removes_the_filter(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.search_draft = "second".to_owned();
            history.apply_search_now(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 1)
        });

        history.update(cx, |history, cx| {
            history.search_draft.clear();
            history.apply_search_now(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        assert!(history.read_with(cx, |history, _| !history.filter.is_filtering()));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: FAIL to compile — `no field search_draft`, `no method schedule_search`, `cannot find value SEARCH_DEBOUNCE`.

- [ ] **Step 3: Add the draft state and the debounce**

At the top of `history.rs`, beside `CHUNK`:

```rust
/// How long the field sits still before the query runs. Every keystroke
/// would otherwise start a full walk of every reachable commit: `--grep` is
/// O(walk), not O(page).
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
```

On `GitHistory`, beside `filter`:

```rust
    /// What is typed in the field right now, which is not yet what is
    /// queried — `filter.text` is. The two differ for `SEARCH_DEBOUNCE`.
    pub(crate) search_draft: String,
    pub(crate) search_regex: bool,
    pub(crate) search_case_sensitive: bool,
    pub(crate) search_blink: crate::caret::Blink,
    search_task: Option<Task<()>>,
    /// Bumped per scheduled search; a timer that wakes stale does nothing.
    search_generation: u64,
```

Initialise them in `new`: empty string, `false`, `false`,
`crate::caret::Blink::new()`, `None`, `0`.

Then:

```rust
    /// Runs the current draft as a query immediately — Enter, or a toggle
    /// flipped, where waiting would feel broken.
    pub(crate) fn apply_search_now(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        self.search_task = None;
        let text = (!self.search_draft.is_empty()).then(|| self.search_draft.clone());
        let filter = LogFilter {
            text,
            regex: self.search_regex,
            case_sensitive: self.search_case_sensitive,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }

    /// Runs the draft once the typing stops.
    ///
    /// A gpui timer, not `std::thread::sleep`: only the former can be moved
    /// by `background_executor.advance_clock`, so only the former leaves this
    /// testable without sleeping for real.
    pub(crate) fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                this.apply_search_now(cx);
            });
        }));
    }
```

Add `use std::time::Duration;` and `Task` to the `gpui` import list if absent.

- [ ] **Step 4: Build the toolbar row**

Create `rust/crates/tiller_ui/src/right_panel/history_toolbar.rs`:

```rust
//! The History view's filter toolbar.
//!
//! Lives apart from `history.rs` because that file already carries the view's
//! state, loading and row rendering; folding eight controls into it as well
//! would put two unrelated concerns in one place.

use gpui::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use tiller_theme::Theme;

use super::history::GitHistory;

/// The search row: field, then the two toggles that change what the text
/// means rather than what it is.
pub(super) fn render_search_row(
    draft: &str,
    regex: bool,
    case_sensitive: bool,
    caret_visible: bool,
    focus: &FocusHandle,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let regex_entity = entity.clone();
    let case_entity = entity;
    div()
        .id("history-toolbar")
        .debug_selector(|| "history-toolbar".to_owned())
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(5.0))
        .child(
            div()
                .id("history-search-field")
                .debug_selector(|| "history-search-field".to_owned())
                .track_focus(focus)
                .flex_1()
                .min_w(px(0.0))
                .px(px(6.0))
                .py(px(3.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.hairline)
                .text_size(theme.typography.footnote)
                .text_color(if draft.is_empty() {
                    theme.meta
                } else {
                    theme.title
                })
                .flex()
                .items_center()
                .child(if draft.is_empty() {
                    "Text or hash".to_owned()
                } else {
                    draft.to_owned()
                })
                // `caret::bar` and not a `|` appended to the string: the bar
                // always occupies layout, so text does not shift as it
                // blinks. It is this repo's one way to draw a caret.
                .child(crate::caret::bar(
                    px(14.0),
                    theme.title,
                    caret_visible,
                )),
        )
        .child(toggle(".*", "history-search-regex", regex, theme, move |cx| {
            regex_entity.update(cx, |history, cx| {
                history.search_regex = !history.search_regex;
                history.apply_search_now(cx);
            });
        }))
        .child(toggle("Cc", "history-search-case", case_sensitive, theme, move |cx| {
            case_entity.update(cx, |history, cx| {
                history.search_case_sensitive = !history.search_case_sensitive;
                history.apply_search_now(cx);
            });
        }))
}

/// One of the two square toggles. They apply immediately rather than through
/// the debounce: a click is not typing, and waiting after one reads as a bug.
fn toggle(
    label: &'static str,
    selector: &'static str,
    on: bool,
    theme: Theme,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .w(px(24.0))
        .h(px(20.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if on { theme.title } else { theme.meta })
        .bg(if on { theme.row_hover } else { theme.background })
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(label)
}
```

Register it in `rust/crates/tiller_ui/src/right_panel/mod.rs` beside
`mod history;`:

```rust
mod history_toolbar;
```

- [ ] **Step 5: Draw the toolbar above the list**

In `GitHistory::render`, the toolbar goes above **every** branch — error,
loading, empty and list alike — so the field never disappears under the state
it is meant to change. Wrap the existing returns: build
`let toolbar = history_toolbar::render_search_row(…);` first, then make each
early return a column carrying `toolbar` above its current content.

Add a focus handle field `search_focus: Option<FocusHandle>`, initialised
lazily in `render` the way `RightPanel::file_focus` already is
(`right_panel/mod.rs:~517`). Once per render, while the surface may show a
caret, call:

```rust
        let search_focused = self
            .search_focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window));
        crate::caret::schedule(
            &mut self.search_blink,
            search_focused,
            Self::flip_search_blink,
            cx,
        );
```

Key handling, following the address field in `browser.rs:1370-1398`:

```rust
    /// Keys for the search field.
    ///
    /// Typing and Backspace go through the debounce; Enter and Escape do not.
    /// A key the user meant as "now" must not sit for 250ms.
    fn on_search_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        self.search_blink.wake();
        match event.keystroke.key.as_str() {
            "enter" => self.apply_search_now(cx),
            "escape" => {
                self.search_draft.clear();
                self.apply_search_now(cx);
            }
            "backspace" => {
                self.search_draft.pop();
                self.schedule_search(cx);
            }
            _ => {
                if let Some(character) = event.keystroke.key_char.as_deref()
                    && character != "\n"
                {
                    self.search_draft.push_str(character);
                    self.schedule_search(cx);
                }
            }
        }
        cx.notify();
    }

    fn flip_search_blink(&mut self, cx: &mut Context<Self>) {
        self.search_blink.flip();
        cx.notify();
    }
```

Wire it with `.on_key_down(…)` on the field element in
`render_search_row`, forwarding into the entity the same way the two toggles
already do.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel/history_toolbar.rs rust/crates/tiller_ui/src/right_panel/mod.rs rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): add the search field with regex and case toggles"
```

---

### Task 6: Explain a body-only match

**Files:**
- Modify: `rust/crates/tiller_git/src/log.rs` (add `GitLog::body`)
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` (cache, lazy fetch, second row line)
- Test: both files

**Interfaces:**
- Consumes: `GitHistory::filter` (Task 3).
- Produces: `GitLog::body(repo: &Path, sha: &str) -> Result<String, GitError>`; on `GitHistory`, `bodies: HashMap<String, String>`.

- [ ] **Step 1: Write the failing test**

```rust
    /// `--grep` matches the whole message, so a row can be returned for text
    /// that appears nowhere on screen. When the subject does not contain the
    /// search text, the match is necessarily in the body — no need to ask git
    /// why it returned the row.
    #[test]
    fn a_body_is_fetched_for_one_commit_at_a_time() {
        let dir = std::env::temp_dir().join(format!("tiller-log-body-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp repo");

        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("spawn git");
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Tester"]);
        std::fs::write(dir.join("a.txt"), "x").expect("write");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "subject line", "-m", "the body mentions sockets"]);

        let filter = LogFilter {
            text: Some("sockets".to_owned()),
            ..LogFilter::default()
        };
        let commits = GitLog::commits(&dir, 0, 10, &filter).expect("log");
        assert_eq!(commits.len(), 1, "matched on body text alone");
        assert!(
            !commits[0].subject.contains("sockets"),
            "the subject does not explain the match, which is the whole case"
        );

        let body = GitLog::body(&dir, &commits[0].sha).expect("body");
        assert!(body.contains("the body mentions sockets"));

        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_git a_body_is_fetched`
Expected: FAIL to compile — `no function or associated item named body`.

- [ ] **Step 3: Add the single-commit body read**

In `impl GitLog`, after `has_commits`:

```rust
    /// The body of one commit.
    ///
    /// Deliberately not part of `LOG_FORMAT`: adding `%b` to the bulk read
    /// grows 500 commits from 91KB to 467KB — 5.1x measured on this
    /// repository — for text almost none of which is ever displayed. It
    /// would also make the record seven fields, and a body may legitimately
    /// contain the 0x1e/0x1f bytes this format uses as separators. Fetching
    /// one body for one visible row costs neither.
    pub fn body(repo: &Path, sha: &str) -> Result<String, GitError> {
        let output = git::run_accepting(&["show", "-s", "--format=%b", sha], repo, &[0])?;
        Ok(output.stdout_string().trim_end().to_owned())
    }
```

- [ ] **Step 4: Fetch lazily for the rows that need it**

On `GitHistory`, add `bodies: std::collections::HashMap<String, String>` and
initialise it empty.

In the `uniform_list` processor, for each row in the visible range, when
`self.filter.text` is `Some(text)` and the row's `subject` does **not**
contain `text` (respecting `search_case_sensitive`) and `bodies` has no entry
for that sha, spawn one background fetch that inserts into `bodies` and
notifies. Guard on the sha already being in flight so a scroll does not queue
the same fetch repeatedly.

In `render_history_row`, when a body is present for the row, draw the matching
line of it beneath the subject at `theme.typography.footnote` in
`theme.meta`, prefixed `└ `, marked `debug_selector("history-body-match")`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): show why a commit matched on body text"
```

---

### Task 7: "Text **or** hash"

The field's own name promises this and nothing so far delivers it. Prefix
search is not expressible in `git log` at all, so it is a resolution step
before the query rather than a predicate inside it.

**Files:**
- Modify: `rust/crates/tiller_git/src/log.rs` — `LogFilter` gains `rev`, plus `looks_like_hash` and `GitLog::resolve_commit`
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` — `apply_search_now` branches
- Test: both files

**Interfaces:**
- Consumes: `LogFilter` (Task 1), `apply_search_now` (Task 5).
- Produces: `LogFilter::rev: Option<String>`, `tiller_git::looks_like_hash(input: &str) -> bool`, `GitLog::resolve_commit(repo: &Path, input: &str) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `log.rs`'s tests module:

```rust
    /// Seven is git's own default abbreviation length. Below it, hex
    /// collisions with real words are common enough to matter: `cafe`,
    /// `face`, `dead` and `beef` are all valid hex, and someone typing them
    /// means the word.
    #[test]
    fn only_seven_or_more_hex_characters_look_like_a_hash() {
        assert!(!looks_like_hash("cafe"));
        assert!(!looks_like_hash("dead"));
        assert!(!looks_like_hash("abcdef"), "six is still a word");
        assert!(looks_like_hash("abcdef0"));
        assert!(looks_like_hash("0123456789abcdef0123456789abcdef01234567"));
        assert!(!looks_like_hash("socket"), "not hex at all");
        assert!(!looks_like_hash("abcdefg"), "g is not hex");
        assert!(!looks_like_hash(""));
    }

    /// A resolved revision replaces branch selection and walks nothing:
    /// `--no-walk` shows that commit, not that commit plus every ancestor.
    #[test]
    fn a_revision_filter_shows_exactly_one_commit() {
        let filter = LogFilter {
            rev: Some("abcdef0".to_owned()),
            ..LogFilter::default()
        };

        let args = filter.args();

        assert!(args.contains(&"--no-walk".to_owned()));
        assert!(args.contains(&"abcdef0".to_owned()));
        assert!(
            !args.contains(&"--branches".to_owned()),
            "a revision replaces branch selection rather than adding to it"
        );
        assert!(filter.is_filtering());
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p tiller_git log::`
Expected: FAIL to compile — `cannot find function looks_like_hash`, `struct LogFilter has no field named rev`.

- [ ] **Step 3: Add the field, the rule and the resolver**

Add `rev` to `LogFilter`, above `branches`:

```rust
    /// A revision the input resolved to. Set, it replaces branch selection
    /// entirely and the query shows just that commit.
    pub rev: Option<String>,
```

In `LogFilter::args`, replace the branch-selection block:

```rust
        if let Some(rev) = &self.rev {
            // `--no-walk` is what makes this one commit rather than that
            // commit and all of its ancestors.
            args.push("--no-walk".to_owned());
            args.push(rev.clone());
        } else if self.branches.is_empty() {
            args.push("--branches".to_owned());
        } else {
            for branch in &self.branches {
                args.push(format!("--branches={branch}"));
            }
        }
```

Add `self.rev.is_some()` to the `is_filtering` disjunction.

Beside `GitLog`, as a free function:

```rust
/// Whether an input should be tried as a commit before being tried as text.
///
/// Seven hex characters is git's own default abbreviation length, and it is
/// the threshold that keeps real words out: `cafe`, `dead`, `face` and
/// `beef` are all valid hex, and someone typing them means the word.
pub fn looks_like_hash(input: &str) -> bool {
    input.len() >= 7 && input.chars().all(|character| character.is_ascii_hexdigit())
}
```

And in `impl GitLog`:

```rust
    /// The full object name `input` abbreviates, if it names exactly one
    /// commit. `None` covers every other case — no such object, an ambiguous
    /// prefix, or a tree — and each of them means "treat it as text".
    pub fn resolve_commit(repo: &Path, input: &str) -> Option<String> {
        let spec = format!("{input}^{{commit}}");
        git::run_accepting(&["rev-parse", "--verify", "--quiet", &spec], repo, &[0])
            .ok()
            .map(|output| output.stdout_string().trim().to_owned())
            .filter(|sha| sha.len() == 40)
    }
```

Re-export `looks_like_hash` from `tiller_git/src/lib.rs` beside `LogFilter`.

- [ ] **Step 4: Branch in `apply_search_now`**

Resolution touches the filesystem, so it cannot run on the render thread.
Replace `apply_search_now`'s body (Task 5, Step 3) with a version that hands
the decision to a background task and applies the filter when it returns:

```rust
    pub(crate) fn apply_search_now(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        let draft = self.search_draft.clone();
        let regex = self.search_regex;
        let case_sensitive = self.search_case_sensitive;
        let base = self.filter.clone();
        let repo_root = self.repo_root.clone();

        if draft.is_empty() {
            self.set_filter(
                LogFilter {
                    text: None,
                    rev: None,
                    ..base
                },
                cx,
            );
            return;
        }

        // `resolve_commit` shells out, so it runs off the render thread —
        // the same reason `has_commits` is resolved beside the log read
        // rather than inside the `update` closure.
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let resolved = if tiller_git::looks_like_hash(&draft) {
                cx.background_spawn({
                    let repo_root = repo_root.clone();
                    let draft = draft.clone();
                    async move { GitLog::resolve_commit(&repo_root, &draft) }
                })
                .await
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                if this.search_generation != generation {
                    return;
                }
                let filter = match resolved {
                    // It named a commit: show that commit, and do not also
                    // grep for its own hash.
                    Some(sha) => LogFilter {
                        rev: Some(sha),
                        text: None,
                        ..base
                    },
                    // Hex-looking but unresolvable, or not hex at all: text.
                    None => LogFilter {
                        rev: None,
                        text: Some(draft),
                        regex,
                        case_sensitive,
                        ..base
                    },
                };
                this.set_filter(filter, cx);
            });
        }));
    }
```

`schedule_search` (Task 5) is unchanged: it still waits out the debounce and
then calls this.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_git && cargo test -p tiller_ui right_panel::history`
Expected: PASS. Task 5's two debounce tests still hold — `apply_search_now` is now asynchronous, so if `typing_debounces_into_a_single_query` needs an extra `cx.run_until_parked()` after the clock advance, add it; do not weaken the assertion.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_git/src/log.rs rust/crates/tiller_git/src/lib.rs rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): resolve a pasted hash instead of grepping for it"
```

---

## Verification

After Task 7, run `cd rust && cargo test -p tiller_git && cargo test -p tiller_ui`
and confirm the failure count matches the pre-existing baseline (9 in
`tiller_ui`, 0 in `tiller_git`). Then confirm by hand with `Scripts/build-dev.sh`:

1. Open the History view and type in the field. Results narrow after a beat, not per keystroke.
2. The commit graph disappears while the field has text, and returns when it is cleared.
3. Type text that appears only in a commit body. The row appears with the matching body line beneath its subject.
4. Type something that matches nothing. The view says no commits match the filter, not "No commits yet".
5. Toggle `.*` and `Cc`. Each applies immediately, with no wait.
6. Paste a full commit hash. Exactly that commit appears — not it and its ancestors.
7. Type `abcdefg`. It is seven characters but not hex, so it is searched as text, not resolved.
8. Narrow the right panel to its 220px minimum. The field stays usable — the responsive chip behaviour is Part 2, but the field must not break here.

# History Filter — Part 2: Dropdowns, IntelliSort and Responsive Layout — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the toolbar: Branch, User, Date and Paths dropdowns, the IntelliSort toggle, and a layout that degrades across the panel's 220–640px range.

**Architecture:** One pure function decides the toolbar's shape from the panel width, mirroring `panel_layout::resolve_panel_widths`. One generic chip-plus-popup primitive serves all four dropdowns, so each one is a short configuration — its option source and its mapping into `LogFilter` — rather than a fourth copy of a popup.

**Tech Stack:** Rust, [gpui](https://github.com/zed-industries/zed), `git log`.

**Spec:** `docs/superpowers/specs/2026-08-23-history-filter-toolbar-design.md`

## What Part 1 already built

`LogFilter` (`tiller_git/src/log.rs`) already carries every field this plan
needs — `branches`, `authors`, `since`, `until`, `paths`, `topo_order` — and
already translates them into arguments, with tests. **No task here changes
`LogFilter::args`.** `GitHistory::set_filter` already resets and reloads, and
`is_filtering()` already drives the graph and the `NoMatches` empty state, so
every dropdown gets those behaviours for free the moment it writes a field.

## Global Constraints

- Verification gate: `cd rust && cargo test -p <crate>`. Do **not** run `Scripts/ci.sh`.
- `cargo build --workspace` does **not** compile `#[cfg(test)]` code. Verify with `cargo test`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Tests first, standard Rust `#[test]`.
- **Run `git status` before every commit.** A linter in this environment has repeatedly modified `rust/crates/tiller_git/src/clone.rs`, which no task here touches. Discard it with `git checkout -- <file>`.
- Temp repositories in tests must be named with the test's own name, not only `std::process::id()` — a directory left behind by an earlier run makes `remove_dir_all` race a still-closing git process.
- Pre-existing red on this machine: 9 in `tiller_ui`, ~20 in `tiller`, 0 in `tiller_git`. No *new* red.
- Never change `LOG_FORMAT`.

---

### Task 1: `toolbar_layout` — the shape decision

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history_toolbar.rs`
- Test: same file, new `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `ToolbarLayout` (`OneRow` | `TwoRows` | `Collapsed`, `Copy`, `PartialEq`, `Debug`) and `toolbar_layout(panel_width: f32) -> ToolbarLayout`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The boundaries, not the middles. A layout exercised at 300 and 500px
    /// passes whether the threshold sits at 400 or at 470, which is why
    /// those are the numbers not tested here.
    #[test]
    fn the_toolbar_changes_shape_exactly_at_its_thresholds() {
        assert_eq!(toolbar_layout(469.0), ToolbarLayout::TwoRows);
        assert_eq!(toolbar_layout(470.0), ToolbarLayout::OneRow);
        assert_eq!(toolbar_layout(259.0), ToolbarLayout::Collapsed);
        assert_eq!(toolbar_layout(260.0), ToolbarLayout::TwoRows);
    }

    /// The panel clamps to 220..=640, so those two are the only widths the
    /// toolbar will ever actually be asked for at the extremes.
    #[test]
    fn both_ends_of_the_panels_range_have_a_shape() {
        assert_eq!(toolbar_layout(220.0), ToolbarLayout::Collapsed);
        assert_eq!(toolbar_layout(640.0), ToolbarLayout::OneRow);
    }

    /// A degenerate width must not panic or fall through to the widest
    /// shape: during the first frame, before layout has run, zero is a real
    /// value this can be called with.
    #[test]
    fn a_zero_width_collapses_rather_than_expanding() {
        assert_eq!(toolbar_layout(0.0), ToolbarLayout::Collapsed);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p tiller_ui history_toolbar`
Expected: FAIL to compile — `cannot find type ToolbarLayout in this scope`.

- [ ] **Step 3: Write the implementation**

At the top of `history_toolbar.rs`, below the module docs:

```rust
/// How much of the toolbar fits side by side at the panel's current width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolbarLayout {
    /// Field, toggles, four chips and IntelliSort on one line.
    OneRow,
    /// Field and toggles above; chips and IntelliSort below.
    TwoRows,
    /// Two rows, but the four chips share one `Filters (n)` popup.
    Collapsed,
}

/// Below this the search field would compress under ~190px, where it stops
/// being readable, so the chips move to their own row.
const ONE_ROW_MIN: f32 = 470.0;
/// Below this the four chips (~257px together) no longer fit on a row of
/// their own, so they collapse behind a single button.
const CHIP_ROW_MIN: f32 = 260.0;

/// The toolbar's shape at a given panel width.
///
/// The thresholds are derived rather than chosen: four chips measure ~257px
/// together and the field needs ~190px to stay readable, so one row needs
/// ~455px plus margin. At the panel's 220px minimum, 204px remain usable —
/// a 150px field and two 24px toggles.
pub(super) fn toolbar_layout(panel_width: f32) -> ToolbarLayout {
    if panel_width >= ONE_ROW_MIN {
        ToolbarLayout::OneRow
    } else if panel_width >= CHIP_ROW_MIN {
        ToolbarLayout::TwoRows
    } else {
        ToolbarLayout::Collapsed
    }
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cd rust && cargo test -p tiller_ui history_toolbar`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history_toolbar.rs
git commit -m "feat(history): decide the toolbar shape from the panel width"
```

---

### Task 2: The chip and its popup, proved by Branch

One primitive, four consumers. Branch comes with it because a primitive with
no consumer is untested by anything that matters.

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history_toolbar.rs` (chip + popup)
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs` (open-popup state, branch list, apply)
- Test: both

**Interfaces:**
- Consumes: `ToolbarLayout` (Task 1), `GitHistory::set_filter` (Part 1).
- Produces: `FilterChip` (`Branch` | `User` | `Date` | `Paths`; `Copy`, `PartialEq`, `Eq`, `Hash`), `FilterChip::{label, selector}`, `render_filter_chip(chip: FilterChip, active: usize, open: bool, entity: Entity<GitHistory>, theme: Theme) -> impl IntoElement`, `render_chip_popup(chip: FilterChip, options: &[String], selected: &[String], entity: Entity<GitHistory>, theme: Theme) -> impl IntoElement`; on `GitHistory`: `open_chip: Option<FilterChip>`, `branch_options: Vec<String>`, `toggle_chip_option(chip: FilterChip, value: String, cx)`.

- [ ] **Step 1: Write the failing test**

In `history.rs`'s tests module:

```rust
    /// Selecting a branch narrows the query to it, and deselecting it puts
    /// the view back to every local branch — which is `LogFilter`'s empty
    /// `branches`, not a branch list containing everything.
    #[gpui::test]
    fn choosing_a_branch_narrows_the_query_and_deselecting_widens_it(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::Branch, "main".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.branches, vec!["main".to_owned()]);
            assert!(history.filter.is_filtering());
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::Branch, "main".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert!(
                history.filter.branches.is_empty(),
                "empty means every branch; it must not become a list of all of them"
            );
            assert!(!history.filter.is_filtering());
        });
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_ui choosing_a_branch`
Expected: FAIL to compile — `cannot find type FilterChip`, `no method named toggle_chip_option`.

- [ ] **Step 3: Add the chip identity and the popup state**

In `history_toolbar.rs`:

```rust
/// Which of the four dropdowns a chip, a popup or a selection belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FilterChip {
    Branch,
    User,
    Date,
    Paths,
}

impl FilterChip {
    pub(super) fn label(self) -> &'static str {
        match self {
            FilterChip::Branch => "Branch",
            FilterChip::User => "User",
            FilterChip::Date => "Date",
            FilterChip::Paths => "Paths",
        }
    }

    pub(super) fn selector(self) -> &'static str {
        match self {
            FilterChip::Branch => "history-chip-branch",
            FilterChip::User => "history-chip-user",
            FilterChip::Date => "history-chip-date",
            FilterChip::Paths => "history-chip-paths",
        }
    }
}
```

On `GitHistory`, beside `search_focus`:

```rust
    /// Which dropdown is open, if any. One at a time: two popups at once
    /// would need a z-order and a dismissal rule neither of them earns.
    pub(crate) open_chip: Option<FilterChip>,
    /// Local branch names, read once per repository and refreshed with the
    /// tree. Empty until the first read returns.
    pub(crate) branch_options: Vec<String>,
```

Initialise `open_chip: None` and `branch_options: Vec::new()` in `new`, and
populate `branch_options` from `GitBranches::list(&repo_root)` in the same
background task that loads the first chunk — it is one cheap call and it must
not run on the render thread.

- [ ] **Step 4: Implement the selection toggle**

```rust
    /// Adds or removes one value from a chip's selection and re-runs the
    /// query. Every chip writes a different `LogFilter` field, which is the
    /// only place their behaviour differs.
    pub(crate) fn toggle_chip_option(
        &mut self,
        chip: FilterChip,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let mut filter = self.filter.clone();
        match chip {
            FilterChip::Branch => toggle_in(&mut filter.branches, value),
            FilterChip::User => toggle_in(&mut filter.authors, value),
            // Date and Paths do not multi-select; Tasks 4 and 5 give them
            // their own entry points rather than bending this one.
            FilterChip::Date | FilterChip::Paths => return,
        }
        self.set_filter(filter, cx);
    }
```

with, as a free function in `history.rs`:

```rust
/// Membership toggle that keeps the vector a set: `LogFilter` treats a
/// repeated value as a repeated git argument, and git would then OR a term
/// with itself.
fn toggle_in(values: &mut Vec<String>, value: String) {
    if let Some(index) = values.iter().position(|existing| *existing == value) {
        values.remove(index);
    } else {
        values.push(value);
    }
}
```

- [ ] **Step 5: Render the chip and its popup**

In `history_toolbar.rs`:

```rust
/// One dropdown button. `active` is how many options that chip currently
/// selects; it is shown so a collapsed or scrolled-past filter is never
/// silently on.
pub(super) fn render_filter_chip(
    chip: FilterChip,
    active: usize,
    open: bool,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let selector = chip.selector();
    let label = if active == 0 {
        chip.label().to_owned()
    } else {
        format!("{} ({active})", chip.label())
    };
    div()
        .id(selector)
        .debug_selector(move || selector.to_owned())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.0))
        .px(px(6.0))
        .py(px(3.0))
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if active > 0 { theme.title } else { theme.meta })
        .bg(if open { theme.row_hover } else { theme.background })
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            entity.update(cx, |history, cx| {
                history.open_chip = (history.open_chip != Some(chip)).then_some(chip);
                cx.notify();
            });
        })
        .child(label)
        .child("⌄")
}

/// The option list for an open chip. Multi-select: clicking an option
/// toggles it and leaves the popup open, because choosing two branches
/// should not cost two trips.
pub(super) fn render_chip_popup(
    chip: FilterChip,
    options: &[String],
    selected: &[String],
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let mut list = div()
        .id("history-chip-popup")
        .debug_selector(|| "history-chip-popup".to_owned())
        .absolute()
        .top(px(26.0))
        .w(theme.spacing.menu_width)
        .max_h(px(240.0))
        .overflow_hidden()
        .p(px(4.0))
        .rounded(theme.radii.user_pill)
        .border_1()
        .border_color(theme.hairline)
        .bg(theme.card_fill)
        .shadow_lg();

    if options.is_empty() {
        return list.child(
            div()
                .px(px(6.0))
                .py(px(4.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.meta)
                .child("Nothing to choose from"),
        );
    }

    for option in options {
        let is_selected = selected.iter().any(|value| value == option);
        let value = option.clone();
        let row_entity = entity.clone();
        list = list.child(
            div()
                .id(gpui::SharedString::from(format!("history-chip-option-{option}")))
                .w_full()
                .px(px(6.0))
                .py(px(4.0))
                .rounded(theme.radii.control)
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.title)
                .hover(|style| style.bg(theme.row_hover))
                .on_click(move |_, _, cx| {
                    row_entity.update(cx, |history, cx| {
                        history.toggle_chip_option(chip, value.clone(), cx);
                    });
                })
                .child(if is_selected { "✓" } else { " " })
                .child(option.clone()),
        );
    }
    list
}
```

The popup is `absolute` inside the toolbar's own relative container rather
than `anchored()` at a mouse position: unlike the file context menu
(`right_panel/files.rs:276`), a chip's popup belongs under its button, not
under the cursor.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history_toolbar.rs rust/crates/tiller_ui/src/right_panel/history.rs
git commit -m "feat(history): add filter chips and the branch dropdown"
```

---

### Task 3: The User dropdown

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs`
- Test: same file

**Interfaces:**
- Consumes: `FilterChip`, `toggle_chip_option` (Task 2).
- Produces: `GitHistory::author_options(&self) -> Vec<String>`.

- [ ] **Step 1: Write the failing test**

```rust
    /// The author list is derived from the commits actually loaded, so it is
    /// a convenience list rather than the repository's full author set. The
    /// exhaustive alternative is a complete walk — exactly the cost someone
    /// filtering is trying to avoid.
    #[gpui::test]
    fn the_author_list_comes_from_the_loaded_commits_without_duplicates(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.read_with(cx, |history, _| {
            assert_eq!(
                history.author_options(),
                vec!["Tester".to_owned()],
                "both commits share one author, so the list has one entry"
            );
        });

        history.update(cx, |history, cx| {
            history.toggle_chip_option(FilterChip::User, "Tester".to_owned(), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.authors, vec!["Tester".to_owned()]);
            assert_eq!(history.commits.len(), 2, "both commits are Tester's");
        });
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_ui the_author_list_comes_from`
Expected: FAIL to compile — `no method named author_options`.

- [ ] **Step 3: Implement it**

```rust
    /// Distinct authors among the commits currently loaded, in first-seen
    /// order so the list does not reshuffle as more chunks arrive.
    ///
    /// Deliberately not exhaustive: `git log --format=%an --branches` over
    /// the whole repository would walk every reachable commit, which is the
    /// cost the person filtering is trying to avoid. The dropdown says so.
    pub(crate) fn author_options(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.commits
            .iter()
            .filter(|commit| seen.insert(commit.author.clone()))
            .map(|commit| commit.author.clone())
            .collect()
    }
```

- [ ] **Step 4: Label the limit in the popup**

In `render_chip_popup`, when `chip == FilterChip::User`, prepend a
non-interactive header row so the list never claims completeness it does not
have:

```rust
    if chip == FilterChip::User {
        list = list.child(
            div()
                .px(px(6.0))
                .py(px(3.0))
                .text_size(theme.typography.caption2)
                .text_color(theme.meta)
                .child("Authors in the loaded history"),
        );
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history.rs rust/crates/tiller_ui/src/right_panel/history_toolbar.rs
git commit -m "feat(history): add the author dropdown"
```

---

### Task 4: The Date dropdown

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs`, `history_toolbar.rs`
- Test: `history.rs`

**Interfaces:**
- Consumes: `FilterChip` (Task 2).
- Produces: `DATE_PRESETS: [(&str, Option<&str>); 4]`, `GitHistory::set_date_preset(since: Option<String>, cx)`.

- [ ] **Step 1: Write the failing test**

```rust
    /// Presets are handed to git verbatim: git parses its own relative
    /// dates, so nothing here computes a timestamp that would then need a
    /// clock to test.
    #[gpui::test]
    fn a_date_preset_is_passed_to_git_verbatim(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));

        history.update(cx, |history, cx| {
            history.set_date_preset(Some("7 days ago".to_owned()), cx);
        });
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert_eq!(history.filter.since.as_deref(), Some("7 days ago"));
            assert!(history.filter.is_filtering());
            assert_eq!(history.commits.len(), 2, "both commits are from today");
        });

        history.update(cx, |history, cx| history.set_date_preset(None, cx));
        pump_until(cx, || history.read_with(cx, |history, _| history.settled));
        history.read_with(cx, |history, _| {
            assert!(history.filter.since.is_none());
            assert!(!history.filter.is_filtering());
        });
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_ui a_date_preset_is_passed`
Expected: FAIL to compile — `no method named set_date_preset`.

- [ ] **Step 3: Implement it**

In `history_toolbar.rs`:

```rust
/// The Date dropdown's fixed choices. The second element is what git is
/// given for `--since`; `None` clears the filter. Git parses these strings
/// itself, so this table never computes a date — which also means no test
/// of it needs a clock.
pub(super) const DATE_PRESETS: [(&str, Option<&str>); 4] = [
    ("Any time", None),
    ("Today", Some("midnight")),
    ("Last 7 days", Some("7 days ago")),
    ("Last 30 days", Some("30 days ago")),
];
```

In `history.rs`:

```rust
    /// Sets or clears the `--since` bound. Date is single-select, unlike
    /// Branch and User: two lower bounds would mean nothing.
    pub(crate) fn set_date_preset(&mut self, since: Option<String>, cx: &mut Context<Self>) {
        let filter = LogFilter {
            since,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }
```

- [ ] **Step 4: Render the Date popup**

In `render_chip_popup`, `FilterChip::Date` renders `DATE_PRESETS` instead of
`options`, each row calling `set_date_preset` and closing the popup —
single-select, so leaving it open would invite a second, meaningless choice:

```rust
    if chip == FilterChip::Date {
        for (label, since) in DATE_PRESETS {
            let value = since.map(ToOwned::to_owned);
            let is_selected = selected.first().map(String::as_str) == since;
            let row_entity = entity.clone();
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("history-date-{label}")))
                    .w_full()
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        row_entity.update(cx, |history, cx| {
                            history.set_date_preset(value.clone(), cx);
                            history.open_chip = None;
                            cx.notify();
                        });
                    })
                    .child(if is_selected { "✓" } else { " " })
                    .child(label),
            );
        }
        return list;
    }
```

The caller passes `filter.since` as a one-element `selected` slice for this
chip.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history.rs rust/crates/tiller_ui/src/right_panel/history_toolbar.rs
git commit -m "feat(history): add the date dropdown"
```

---

### Task 5: The Paths field and the IntelliSort toggle

Two small controls together: neither earns a task, and both are the last
writers into `LogFilter`.

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history.rs`, `history_toolbar.rs`
- Test: `history.rs`

**Interfaces:**
- Consumes: `FilterChip` (Task 2).
- Produces: `GitHistory::{path_draft: String, set_path_filter(cx), toggle_topo_order(cx)}`.

- [ ] **Step 1: Write the failing tests**

```rust
    /// A pathspec narrows to the commits that touched it. `b.txt` arrives
    /// with the second commit, so it must not match the first.
    #[gpui::test]
    fn a_pathspec_narrows_to_the_commits_that_touched_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| {
            history.path_draft = "b.txt".to_owned();
            history.set_path_filter(cx);
        });
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.settled && history.commits.len() == 1)
        });
        history.read_with(cx, |history, _| {
            assert_eq!(history.commits[0].subject, "second");
        });
    }

    /// IntelliSort changes the ordering flag and nothing else — in
    /// particular it is not filtering, so the graph stays and an empty
    /// result would still mean "no commits", not "no matches".
    #[gpui::test]
    fn intellisort_reorders_without_filtering(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let history = cx.new(|cx| GitHistory::new(dir.0.clone(), cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.commits.len() == 2)
        });

        history.update(cx, |history, cx| history.toggle_topo_order(cx));
        pump_until(cx, || {
            history.read_with(cx, |history, _| history.settled && history.commits.len() == 2)
        });
        history.read_with(cx, |history, _| {
            assert!(history.filter.topo_order);
            assert!(
                !history.filter.is_filtering(),
                "ordering is not filtering: the graph must stay"
            );
        });
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: FAIL to compile — `no field path_draft`, `no method set_path_filter`, `no method toggle_topo_order`.

- [ ] **Step 3: Implement both**

On `GitHistory`, beside `search_draft`:

```rust
    /// The pathspec being typed. Free text rather than a directory picker: a
    /// pathspec is more expressive than a picker, and git validates it.
    pub(crate) path_draft: String,
```

```rust
    /// Applies the typed pathspec, or clears it when the field is empty.
    pub(crate) fn set_path_filter(&mut self, cx: &mut Context<Self>) {
        let paths = if self.path_draft.trim().is_empty() {
            Vec::new()
        } else {
            vec![PathBuf::from(self.path_draft.trim())]
        };
        let filter = LogFilter {
            paths,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }

    /// IntelliSort: `--topo-order` keeps a merged branch's commits
    /// contiguous instead of interleaving them by date.
    pub(crate) fn toggle_topo_order(&mut self, cx: &mut Context<Self>) {
        let filter = LogFilter {
            topo_order: !self.filter.topo_order,
            ..self.filter.clone()
        };
        self.set_filter(filter, cx);
    }
```

- [ ] **Step 4: Render both controls**

The Paths popup holds one text row bound to `path_draft`, applying on Enter
via `set_path_filter`, following the search field's key handling in
`GitHistory::on_search_key`.

The IntelliSort button reuses the toolbar's existing square-toggle helper
with the label `⇅`, selector `history-intellisort`, `on` bound to
`filter.topo_order`, and `on_click` calling `toggle_topo_order`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history.rs rust/crates/tiller_ui/src/right_panel/history_toolbar.rs
git commit -m "feat(history): add the pathspec field and the intellisort toggle"
```

---

### Task 6: Assemble the toolbar responsively

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/history_toolbar.rs`, `history.rs`
- Test: `history.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `render_toolbar(layout: ToolbarLayout, …) -> impl IntoElement`, replacing `render_search_row` as the entry point.

- [ ] **Step 1: Write the failing test**

```rust
    /// At the panel's narrow end the four chips collapse behind one button,
    /// and at its wide end they are all present. The drawn frame is the only
    /// place this is actually true or false.
    #[gpui::test]
    async fn the_chips_collapse_when_the_panel_is_narrow(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let dir = TempDir::new();
        seed_two_commits(&dir.0);
        let window = cx.add_window(|_window, cx| GitHistory::new(dir.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // The test window is wide, so every chip is drawn.
        assert!(cx.debug_bounds("history-chip-branch").is_some());
        assert!(cx.debug_bounds("history-chip-paths").is_some());
        assert!(cx.debug_bounds("history-chips-collapsed").is_none());

        let history = cx.update(|window, _| {
            window.root::<GitHistory>().flatten().expect("history root")
        });
        history.update(&mut cx.cx, |history, cx| {
            history.panel_width = 230.0;
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("history-chip-branch").is_none());
        assert!(
            cx.debug_bounds("history-chips-collapsed").is_some(),
            "the four chips are behind one button, not gone"
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller_ui the_chips_collapse`
Expected: FAIL to compile — `no field panel_width`.

- [ ] **Step 3: Give the view its width**

`GitHistory` cannot measure the panel itself; the host knows it. Add:

```rust
    /// The panel's current width, pushed in by the host each render. The
    /// view has no way to measure its own container, and guessing from the
    /// last drawn frame would lag a frame behind every drag.
    pub(crate) panel_width: f32,
```

initialised to `405.0`, and set from `RightPanel::render_history`
(`right_panel/mod.rs:494`) using the width the host already resolves.

- [ ] **Step 4: Assemble the three shapes**

```rust
/// The whole toolbar at the current width.
///
/// `OneRow` puts field, toggles, chips and IntelliSort on one line;
/// `TwoRows` moves the chips to a line of their own; `Collapsed` replaces
/// the four chips with a single `Filters (n)` button whose popup holds them.
/// The count on that button is why a collapsed filter is never silently on.
pub(super) fn render_toolbar(
    layout: ToolbarLayout,
    history: &GitHistory,
    entity: Entity<GitHistory>,
    theme: Theme,
) -> impl IntoElement {
    let search = render_search_row(
        &history.search_draft,
        history.search_regex,
        history.search_case_sensitive,
        history.search_blink.visible(),
        history.search_focus_handle(),
        entity.clone(),
        theme,
    );
    let chips = [
        FilterChip::Branch,
        FilterChip::User,
        FilterChip::Date,
        FilterChip::Paths,
    ];
    // The count is what keeps a filter from being silently on when its chip
    // is off-screen or folded away.
    let active_of = |chip: FilterChip| -> usize {
        match chip {
            FilterChip::Branch => history.filter.branches.len(),
            FilterChip::User => history.filter.authors.len(),
            FilterChip::Date => usize::from(history.filter.since.is_some()),
            FilterChip::Paths => history.filter.paths.len(),
        }
    };

    let mut chip_row = div()
        .relative()
        .flex()
        .items_center()
        .gap(px(6.0));
    if layout == ToolbarLayout::Collapsed {
        let total: usize = chips.iter().copied().map(active_of).sum();
        chip_row = chip_row.child(render_collapsed_chips(
            total,
            history.open_chip.is_some(),
            entity.clone(),
            theme,
        ));
    } else {
        for chip in chips {
            chip_row = chip_row.child(render_filter_chip(
                chip,
                active_of(chip),
                history.open_chip == Some(chip),
                entity.clone(),
                theme,
            ));
        }
    }
    chip_row = chip_row.child(render_intellisort(
        history.filter.topo_order,
        entity.clone(),
        theme,
    ));
    if let Some(open) = history.open_chip {
        chip_row = chip_row.child(render_chip_popup(
            open,
            &history.options_for(open),
            &history.selection_for(open),
            entity,
            theme,
        ));
    }

    let mut toolbar = div().w_full().flex_none().flex();
    if layout == ToolbarLayout::OneRow {
        toolbar = toolbar.flex_row().items_center().gap(px(6.0));
    } else {
        toolbar = toolbar.flex_col().gap(px(4.0));
    }
    toolbar.child(search).child(chip_row)
}
```

`options_for` and `selection_for` are two small dispatchers on `GitHistory`
returning `Vec<String>` — `branch_options`, `author_options()`, the date
preset in a one-element vector, and `path_draft` respectively. Add them
beside `author_options`. `render_collapsed_chips` is the square-toggle helper
with the label `Filters (n)` and selector `history-chips-collapsed`;
`render_intellisort` is the same helper with `⇅` and
`history-intellisort` (Task 5).

`search_focus_handle()` returns the lazily-created handle; add it as a small
accessor rather than exposing the `Option` field.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_ui right_panel::history`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status
git add rust/crates/tiller_ui/src/right_panel/history_toolbar.rs rust/crates/tiller_ui/src/right_panel/history.rs rust/crates/tiller_ui/src/right_panel/mod.rs
git commit -m "feat(history): lay the toolbar out for the panel's width"
```

---

## Verification

Run `cd rust && cargo test -p tiller_git && cargo test -p tiller_ui` and confirm
the counts match the baseline (0 red in `tiller_git`, 9 in `tiller_ui`). Then by
hand with `Scripts/build-dev.sh`:

1. Open History. Branch, User, Date and Paths are present; each opens a popup under its own button.
2. Pick a branch. The list narrows, the chip shows `Branch (1)`, the graph disappears. Unpick it: everything comes back.
3. Open User. The header says the authors come from the loaded history — it must not look like the repository's full list.
4. Pick "Last 7 days". Only recent commits remain, and the popup closes — it is single-select.
5. Type a path into Paths and press Enter. Only commits touching it remain.
6. Press IntelliSort. The order changes and **the graph stays**: ordering is not filtering.
7. Drag the panel narrow. Below ~470px the chips move to their own row; below ~260px they collapse into `Filters (n)`, with the count still visible.

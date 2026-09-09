# Zed-flavoured left sidebar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild Sirio's left sidebar as a flat, Zed-flavoured list — a sticky project header per section, one two-line card per worktree, its tabs demoted to clickable pills inside that card, a flat search field, and a gradient fade instead of ellipsis truncation.

**Architecture:** `sirio_ui/src/sidebar.rs` (9,873 lines) becomes a module directory. `sidebar/mod.rs` keeps the `Sidebar` entity, its events, row flattening and every popup; `sidebar/row.rs` owns the worktree card and its pills; `sidebar/section.rs` owns the project header; `sidebar/fade.rs` owns the gradient veil. The tab stops being a row: `SidebarRow` grows a `Vec<SidebarPill>` and `visible_rows` stops emitting `RowKind::Tab`. Every phase lands on `main` on its own and leaves the app usable.

**Tech Stack:** Rust 2024, gpui (`bezel-gpui` `=0.3.8`), `bezel` `=0.1.4` (`bezel::ui::tree` for the container and its key bindings, `bezel::theme` for colour), `#[gpui::test]` + `VisualTestContext::debug_bounds` for window tests.

**Spec:** `docs/superpowers/specs/2026-09-09-left-sidebar-zed-design.md`

## Global Constraints

- **Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh`.** Only the user launches those. Iterate with `cd rust && cargo test -p sirio_ui`.
- **Every commit bumps the version** in `rust/Cargo.toml` via `Scripts/set-workspace-version.sh <version>`, staged in the same commit. `feat:` bumps the minor, everything else the patch. The version at the start of this plan is `0.6.4`; each task below states the value it expects, which is correct only if no other commit lands in between — otherwise read the current value and apply the rule.
- **Commit messages:** Conventional Commits, lower-case imperative subject.
- **bezel first.** `bezel::ui` and `bezel::theme` supply primitives and colour before anything is hand-rolled. Where this plan hand-rolls (the card frame, the fade), the reason is stated in the task and belongs in the code comment too.
- **Zed's `crates/ui` is GPL-3.0-or-later and Sirio is MIT.** Read `~/Desktop/Progetti/zed-ref/crates/ui/src/components/gradient_fade.rs` for the measurements; do not copy its text.
- **UI copy is English** (`Projects`, `No activity`, `Search worktrees…`, `2 agents`).
- **Baseline first.** `cargo test -p sirio_ui` has pre-existing failures on `main`. Record the failing *names* before starting and compare name lists, never counts.
- **Formatting:** run `rustfmt --edition 2024 <the files you touched>`. Do not run `cargo fmt -p sirio_ui` (it reformats unrelated files) and do not run `cargo clippy --tests` (it takes over 30 minutes on this machine).
- **Menu tests:** after clicking a bezel menu item inside a `#[gpui::test]`, call `cx.executor().advance_clock(Duration::from_secs(1))` before asserting — a popup in fade-out swallows the next click.
- `-p sirio_ui` does not build `sirio_terminal`, so Zig is not needed for any command in this plan.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust/crates/sirio_ui/src/sidebar/mod.rs` | The `Sidebar` entity: state, events, `visible_rows`, filter, context menus, worktree prompt, project form, drag/reorder, the panel shell. Re-exports the crate's public sidebar surface. |
| `rust/crates/sirio_ui/src/sidebar/row.rs` | `SidebarPill`, `RowInputs`, `RowView`, `render_row` and the row helpers (`has_sub_line`, `row_min_height`, `row_icon`, `status_text`, `sub_line_text`, `RowStatusGlyph`). |
| `rust/crates/sirio_ui/src/sidebar/section.rs` | The project section header: name, count, `+`, `⋯`, collapse, and (last task) sticky placement. |
| `rust/crates/sirio_ui/src/sidebar/fade.rs` | `fade_right`: the gradient veil, in Sirio's own code. |
| `rust/crates/sirio_ui/src/conformance.rs` | Row-metric inventory; updated when heights change (`conformance.rs:304-315`). |

`sidebar/row.rs` and `sidebar/section.rs` are the two files that grow during this work; keeping them apart from `mod.rs` is what stops the 9,873-line file from becoming an 11,000-line one.

---

### Task 1: Extract the row into its own module

Mechanical move, no behaviour change. It exists so every later task edits a 700-line file instead of a 9,873-line one.

**Files:**
- Create: `rust/crates/sirio_ui/src/sidebar/mod.rs` (from `sidebar.rs`)
- Create: `rust/crates/sirio_ui/src/sidebar/row.rs`
- Delete: `rust/crates/sirio_ui/src/sidebar.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `sidebar::row::{RowInputs, RowView}` (crate-private) and `Sidebar::render_row`, `Sidebar::has_sub_line`, `Sidebar::row_min_height`, `Sidebar::row_icon`, `Sidebar::icon_selector_name` as `impl Sidebar` blocks living in `row.rs`. The crate's public names (`Sidebar`, `SidebarRow`, `SidebarProject`, `SidebarWorktree`, `SidebarEvent`, `SidebarTab`, `SidebarTabRef`, `RowKind`, `AgentMark`, `ROW_HEIGHT`, `CARD_TWO_LINE_HEIGHT`, …) keep their current paths through `pub use` in `mod.rs`.

- [ ] **Step 1: Record the baseline**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | grep -E "^(test .* FAILED|failures:)" | sort > /tmp/sidebar-baseline.txt
wc -l /tmp/sidebar-baseline.txt
```

Keep this file. Every later "tests pass" step means *the same names* as this list, not zero failures.

- [ ] **Step 2: Move the file**

```bash
cd rust/crates/sirio_ui/src
mkdir -p sidebar
git mv sidebar.rs sidebar/mod.rs
```

- [ ] **Step 3: Move the row code into `row.rs`**

Cut these items out of `sidebar/mod.rs` and paste them into a new `sidebar/row.rs`, in this order: `RowStatusGlyph` (the enum near the top of the old file), `RowInputs`, `RowView` and its `impl Render`, then an `impl Sidebar` block holding `render_row`, `has_sub_line`, `row_min_height`, `row_icon`, `icon_selector_name`, `tree_row`. Give `row.rs` this head:

```rust
//! The sidebar's worktree card: one row of the flattened tree, its status
//! glyph, its hover controls and (from Task 4 on) its tab pills.
//!
//! Split out of `sidebar.rs` so the row's drawing can grow without the
//! entity's state, popups and drag handling growing with it — the same
//! `mod.rs` + one-file-per-surface arrangement `right_panel/` uses.

use super::*;
```

In `sidebar/mod.rs`, declare the module right after the existing `use` block:

```rust
mod row;

use row::{RowInputs, RowView};
```

Everything `row.rs` needs must be reachable from `super::*`; where an item is private to `mod.rs` and now used from `row.rs`, widen it to `pub(super)` — never to `pub`.

- [ ] **Step 4: Move the row's tests with it**

In `mod.rs`'s `mod tests`, the tests that assert on row drawing (any test whose body mentions `render_row`, `RowInputs`, `row_min_height`, `has_sub_line`, or a `sidebar-row-*` / `sidebar-status-*` / `sidebar-tab-*` debug selector) move into a `mod tests` at the bottom of `row.rs` with the same `use super::*;` head. Tests about state, filtering, menus, prompts and reordering stay in `mod.rs`.

- [ ] **Step 5: Build and test**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -20
```

Expected: compiles, and the failing-test names match `/tmp/sidebar-baseline.txt` exactly.

- [ ] **Step 6: Format and commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs rust/crates/sirio_ui/src/sidebar/row.rs
Scripts/set-workspace-version.sh 0.6.5
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "refactor: split the sidebar row out of sidebar.rs"
```

---

### Task 2: The project section header

The project stops being a row of the tree and becomes a section header: name, worktree count, and `+` / `⋯` on hover. The `New Worktree` row disappears — its action moves onto the header's `+`. Sticky is **not** part of this task.

**Files:**
- Create: `rust/crates/sirio_ui/src/sidebar/section.rs`
- Modify: `rust/crates/sirio_ui/src/sidebar/mod.rs` (`visible_rows`, the render loop)

**Interfaces:**
- Consumes: `SidebarRow`, `RowKind::Project`, `Sidebar::start_worktree_prompt` (the handler the old `NewWorktree` row called — find its exact name at the `RowKind::NewWorktree` click arm in `render_row` before writing Step 3), `Sidebar::open_context_menu`.
- Produces:
  ```rust
  pub(super) fn render_section(
      row: SidebarRow,
      worktree_count: usize,
      entity: gpui::Entity<Sidebar>,
      theme: Theme,
  ) -> impl IntoElement
  ```
  Debug selectors: `sidebar-section-{row_id}`, `sidebar-section-count-{row_id}`, `sidebar-section-add-{row_id}`, `sidebar-section-menu-{row_id}`.

- [ ] **Step 1: Write the failing tests**

Add to a new `mod tests` at the bottom of `section.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::tests_support::sidebar_with_one_project;
    use gpui::{TestAppContext, VisualTestContext};

    /// The project header carries its own actions, so the action row that
    /// used to sit under an expanded project has nowhere left to be.
    #[gpui::test]
    async fn a_project_header_offers_add_and_menu_instead_of_a_new_worktree_row(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-section-0").is_some(),
            "the project draws as a section header"
        );
        assert!(
            cx.debug_bounds("sidebar-section-add-0").is_some(),
            "the header carries the new-worktree action"
        );
        assert!(
            cx.debug_bounds("sidebar-section-menu-0").is_some(),
            "the header carries the project menu"
        );
        assert!(
            cx.debug_bounds("sidebar-new-worktree-row").is_none(),
            "the New Worktree row is gone from the list"
        );
    }

    /// The count is the section's own summary; with the section collapsed it
    /// is the only thing left saying how much is hidden.
    #[gpui::test]
    async fn a_section_header_counts_its_worktrees(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("sidebar-section-count-0").is_some(),
            "the header states how many worktrees the section holds"
        );
    }
}
```

`tests_support::sidebar_with_one_project` does not exist yet. Add it to `sidebar/mod.rs` as a `#[cfg(test)] pub(super) mod tests_support` that builds a `Sidebar` with one git project named `sirio` holding two worktrees (`main`, primary, and `feat/x` with the comment `"redesign"`), the second one holding two tabs. Reuse whatever fixture the existing `mod tests` already builds a sidebar from — if one exists, move it there rather than writing a second one.

- [ ] **Step 2: Run the tests to see them fail**

```bash
cd rust && cargo test -p sirio_ui sidebar::section 2>&1 | tail -20
```

Expected: FAIL — `section.rs` has no `render_section` yet, or the selectors are absent.

- [ ] **Step 3: Write `section.rs`**

```rust
//! The project section header. A project used to be a row of the tree with a
//! disclosure chevron and a `New Worktree` child row; it is now the header
//! that names the section, counts it, and carries both actions itself.

use super::*;

/// Height of a section header. Shorter than a row on purpose: it is a label,
/// not something you select.
pub(super) const SECTION_HEIGHT: f32 = 26.0;

pub(super) fn render_section(
    row: SidebarRow,
    worktree_count: usize,
    entity: gpui::Entity<Sidebar>,
    theme: Theme,
) -> impl IntoElement {
    let row_id = row.id;
    let group = SharedString::from(format!("sidebar-section-group-{row_id}"));
    let collapse_entity = entity.clone();
    let add_entity = entity.clone();
    let menu_entity = entity;
    div()
        .id(("sidebar-section", row_id))
        .debug_selector(move || format!("sidebar-section-{row_id}"))
        .group(group.clone())
        .relative()
        .h(px(SECTION_HEIGHT))
        .w_full()
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .bg(theme.surface_raised)
        .border_t_1()
        .border_b_1()
        .border_color(theme.border)
        .text_size(theme.typography.scaled(12.0))
        .text_color(theme.text_muted)
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            collapse_entity.update(cx, |sidebar, cx| sidebar.toggle_row(row_id, cx));
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .whitespace_nowrap()
                .overflow_hidden()
                .child(row.title.clone()),
        )
        .child(
            div()
                .debug_selector(move || format!("sidebar-section-count-{row_id}"))
                .flex_none()
                .text_color(theme.text_faint)
                .text_size(theme.typography.scaled(11.0))
                .child(worktree_count.to_string()),
        )
        .child(
            div()
                .id(("sidebar-section-add", row_id))
                .debug_selector(move || format!("sidebar-section-add-{row_id}"))
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .invisible()
                .group_hover(group.clone(), |style| style.visible())
                .hover(|style| style.bg(theme.element_hover))
                .child(IconElement::new(Icon::Plus, IconSize::XSmall))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    add_entity.update(cx, |sidebar, cx| {
                        sidebar.start_worktree_prompt(row_id, window, cx);
                    });
                }),
        )
        .child(
            div()
                .id(("sidebar-section-menu", row_id))
                .debug_selector(move || format!("sidebar-section-menu-{row_id}"))
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .invisible()
                .group_hover(group, |style| style.visible())
                .hover(|style| style.bg(theme.element_hover))
                .child(IconElement::new(Icon::MoreHorizontal, IconSize::XSmall))
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    menu_entity.update(cx, |sidebar, cx| {
                        sidebar.open_context_menu(row_id, event.position(), window, cx);
                    });
                }),
        )
}
```

Check `Icon::Plus` and `Icon::MoreHorizontal` against `sirio_ui/src/icons.rs` and use whatever the real variant names are; the `+` in the header at `sidebar/mod.rs`'s "Projects" line is drawn as the text `"+"` today, and that is an acceptable fallback if no plus icon exists. `toggle_row`, `start_worktree_prompt` and `open_context_menu` are the existing handlers — match their real names and signatures from `mod.rs`.

- [ ] **Step 4: Route project rows to the header in the render loop**

In `sidebar/mod.rs`'s `Render::render`, where each visible row becomes a `RowView`, branch first: `RowKind::Project` rows are rendered with `section::render_section(row, worktree_count, entity.clone(), theme)` and are **not** given a `RowView` (they are not cached, they are cheap). Count a section's worktrees from the flattened list: the `RowKind::Worktree` rows after it, up to the next `RowKind::Project`.

In `visible_rows`, delete the `RowKind::NewWorktree` arm's `filtered.push(row.clone())` so the action row is never emitted (the arm still needs to call `append_worktree`, keep that call). Leave the `RowKind::NewWorktree` variant itself in place for now; Task 4 removes it.

- [ ] **Step 5: Run the tests**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
```

Expected: the two new tests PASS. Tests that assert on a `New Worktree` row or on a project row's chevron now fail — update each to the header's selectors, keeping its original intent; a test whose whole subject was the removed row is deleted with a one-line note in the commit body.

- [ ] **Step 6: Format and commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs rust/crates/sirio_ui/src/sidebar/section.rs
Scripts/set-workspace-version.sh 0.7.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: draw sidebar projects as section headers"
```

---

### Task 3: Flat search field, widened match

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/mod.rs` (the `filter-field` element around the panel shell; `visible_rows`)

**Interfaces:**
- Consumes: `Sidebar::filter`, `SidebarRow::{title, comment}`, and the pills from Task 4 are *not* available yet — this task matches the agent name from the tab rows still present in `self.rows`.
- Produces: `fn row_matches(row: &SidebarRow, query: &str) -> bool` in `mod.rs`, the single predicate every filtering branch calls.

- [ ] **Step 1: Write the failing test**

In `sidebar/mod.rs`'s `mod tests`:

```rust
/// The filter used to see titles only, so a worktree annotated through
/// `worktree.set` could not be found by the annotation the user wrote.
#[test]
fn the_filter_matches_a_comment_as_well_as_a_title() {
    let mut row = SidebarRow::worktree(7, "feat/x");
    row.comment = Some("redesign the sidebar".to_owned());

    assert!(
        Sidebar::row_matches(&row, "redesign"),
        "a comment is searchable text like the branch is"
    );
    assert!(
        Sidebar::row_matches(&row, "feat/x"),
        "the branch still matches"
    );
    assert!(
        !Sidebar::row_matches(&row, "nothing here"),
        "an unrelated query still matches nothing"
    );
}
```

`SidebarRow::worktree(id, title)` is a test constructor — if `mod tests` has no such helper, add one to `tests_support` from Task 2 that builds a `RowKind::Worktree` row with every other field defaulted.

- [ ] **Step 2: Run it to see it fail**

```bash
cd rust && cargo test -p sirio_ui the_filter_matches_a_comment 2>&1 | tail -10
```

Expected: FAIL — no function `row_matches`.

- [ ] **Step 3: Implement the predicate and use it everywhere**

```rust
/// The one place that decides whether a row answers the filter. Title,
/// annotation and — for a tab row — the agent's own name, because the user
/// searching "codex" is looking for where Codex is running, not for a
/// worktree that happens to be spelled that way.
pub(crate) fn row_matches(row: &SidebarRow, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let matches = |text: &str| text.to_lowercase().contains(query);
    matches(&row.title)
        || row.comment.as_deref().is_some_and(matches)
        || row
            .agent_icon
            .map(Self::icon_selector_name)
            .is_some_and(matches)
}
```

Replace every `…title.to_lowercase().contains(&query)` in `visible_rows` with `Self::row_matches(row, &query)` (the project, section, worktree and tab branches — five call sites in the block starting at the `project_matches` binding).

- [ ] **Step 4: Run the test**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
```

Expected: PASS, and the baseline names unchanged otherwise.

- [ ] **Step 5: Make the field flat**

In the panel shell, the filter element currently sets `.ml(px(FILTER_LEFT_INSET))`, a fixed width computed from `panel_width`, `.rounded(theme.radii.control)`, `.bg(theme.input_bg)` and `.border_1()`. Replace those five with:

```rust
                    .w_full()
                    .h(px(34.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(if filter_is_focused {
                        theme.ring
                    } else {
                        theme.border
                    })
```

and change the placeholder text to `Search worktrees…`. Keep `.track_focus(&filter_focus)`, the `id`, the debug selector and the caret; they are what the field's existing tests hold onto.

- [ ] **Step 6: Run the tests, format, commit**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs
Scripts/set-workspace-version.sh 0.8.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: flatten the sidebar search field and widen its match"
```

---

### Task 4: Tabs become pills on their worktree row

Data only — the card still draws as it does today, minus the tab rows. This is the task that changes what a row *is*, so it lands on its own.

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/row.rs` (`SidebarPill`), `rust/crates/sirio_ui/src/sidebar/mod.rs` (`SidebarRow`, `set_worktree_tabs`, `visible_rows`, `tree_row`)

**Interfaces:**
- Consumes: `SidebarTab`, `SidebarTabRef::{Open, Parked}`, `AgentBrandColor`, `ActivityStatus`.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct SidebarPill {
      pub tab_id: Option<usize>,
      pub parked_tab: Option<usize>,
      pub title: String,
      pub icon: Icon,
      pub brand: Option<AgentBrandColor>,
      pub status: Option<ActivityStatus>,
      pub selected: bool,
  }
  ```
  and `SidebarRow::pills: Vec<SidebarPill>`, replacing `SidebarRow::running_agents`.

- [ ] **Step 1: Write the failing test**

In `sidebar/mod.rs`'s `mod tests`:

```rust
/// A worktree and its tabs are one row now: the tabs ride along as pills
/// instead of being rows of their own, so the flattened list never grows
/// when a worktree opens a second agent.
#[gpui::test]
async fn a_worktree_with_tabs_is_still_one_row(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    let rows = window
        .update(&mut cx, |sidebar, _, _| sidebar.visible_rows())
        .unwrap();

    assert!(
        rows.iter().all(|row| row.kind != RowKind::Tab),
        "no tab row survives the flattening"
    );
    let worktree_with_tabs = rows
        .iter()
        .find(|row| row.kind == RowKind::Worktree && !row.pills.is_empty())
        .expect("the fixture's second worktree holds two tabs");
    assert_eq!(
        worktree_with_tabs.pills.len(),
        2,
        "both tabs reach the row as pills"
    );
}
```

- [ ] **Step 2: Run it to see it fail**

```bash
cd rust && cargo test -p sirio_ui a_worktree_with_tabs_is_still_one_row 2>&1 | tail -10
```

Expected: FAIL — `SidebarRow` has no field `pills`.

- [ ] **Step 3: Add the type and fill it**

Add `SidebarPill` to `row.rs` (exported from `mod.rs` with the other public types). On `SidebarRow`, replace `running_agents: Vec<AgentMark>` with:

```rust
    /// The worktree's tabs, live and parked, drawn as pills inside this row.
    /// This replaces the old trailing running-agents badge: a running agent
    /// is a pill with a running status dot, and drawing it twice was the
    /// only thing the badge added.
    pub pills: Vec<SidebarPill>,
```

In `set_worktree_tabs`, stop inserting `RowKind::Tab` rows. Build one `SidebarPill` per `SidebarTab` and assign the vector to the worktree row, keeping the existing "unchanged, so return early" diff — extend it to compare the pills rather than the removed row range:

```rust
        let pills: Vec<SidebarPill> = tabs
            .iter()
            .map(|tab| SidebarPill {
                tab_id: match tab.reference {
                    SidebarTabRef::Open(id) => Some(id),
                    SidebarTabRef::Parked(_) => None,
                },
                parked_tab: match tab.reference {
                    SidebarTabRef::Parked(index) => Some(index),
                    SidebarTabRef::Open(_) => None,
                },
                title: tab.title.clone(),
                icon: tab.icon,
                brand: tab.brand,
                status: tab.status,
                selected: tab.selected,
            })
            .collect();
        if self.rows[worktree_index].pills == pills {
            return;
        }
        self.rows[worktree_index].pills = pills;
        cx.notify();
```

Read `SidebarTab`'s real field names (`sidebar/mod.rs:168`) and adapt; the shape above is the contract, the spelling is whatever that struct already uses.

- [ ] **Step 4: Delete what tab rows required**

- `visible_rows`: drop the `RowKind::Tab` arm and the `tabs` accumulator; a worktree row now stands or falls on `Self::row_matches(&worktree_row, &query)` alone, plus a match on any of its pills' titles.
- `tree_row`: remove the `RowKind::Tab` arm; a worktree is always `tree::Row::leaf(0)` now — no depth, no chevron, per the flat design.
- Remove `RowKind::Tab` and `RowKind::NewWorktree` from the enum and fix the arms the compiler points at. `SidebarTabRef` and `SidebarTab` stay: they are the host's vocabulary, unchanged.
- `worktree_has_tab_rows` and `has_children` plumbing lose their subject; delete them and the `RowInputs::has_children` field with them.

- [ ] **Step 5: Run the tests**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -30
```

Expected: the new test PASSES. Tests that select `sidebar-tab-mark-*`, `sidebar-tab-close-*`, `sidebar-running-agents-*` or count rows including tab rows now fail — rewrite each against pills (Task 5 and 6 add their selectors; until then assert on `row.pills` directly), and delete any whose only subject was tab-row nesting, naming them in the commit body.

- [ ] **Step 6: Format and commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs rust/crates/sirio_ui/src/sidebar/row.rs
Scripts/set-workspace-version.sh 0.8.1
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "refactor: carry a worktree's tabs as pills on its own row"
```

---

### Task 5: The two-line card

Layout A from the prototype: line 1 is `● branch ★ … running`, line 2 is the comment (or the abbreviated path) with the pills right-aligned.

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/row.rs`, `rust/crates/sirio_ui/src/conformance.rs`

**Interfaces:**
- Consumes: `SidebarPill`, `sirio_project::display_path`.
- Produces:
  ```rust
  pub(crate) fn status_text(row: &SidebarRow) -> Option<String>
  pub(crate) fn sub_line_text(row: &SidebarRow) -> String
  ```
  Debug selectors: `sidebar-row-status-{row_id}`, `sidebar-row-subline-{row_id}`, `sidebar-primary-star-{row_id}`.

- [ ] **Step 1: Write the failing tests**

In `row.rs`'s `mod tests`:

```rust
/// The status slot took the place the relative time would have had. It is
/// the row's one-word answer to "what is this worktree doing".
#[test]
fn a_running_worktree_says_running_and_a_still_one_counts_its_agents() {
    let mut row = SidebarRow::worktree(3, "main");
    row.agent_status = Some(ActivityStatus::Running);
    assert_eq!(Sidebar::status_text(&row).as_deref(), Some("running"));

    row.agent_status = Some(ActivityStatus::NeedsInput);
    assert_eq!(Sidebar::status_text(&row).as_deref(), Some("needs input"));

    row.agent_status = Some(ActivityStatus::Idle);
    row.pills = vec![pill(1), pill(2)];
    assert_eq!(
        Sidebar::status_text(&row).as_deref(),
        Some("2 agents"),
        "a still worktree reports how much is parked in it"
    );

    row.pills = vec![pill(1)];
    assert_eq!(
        Sidebar::status_text(&row).as_deref(),
        Some("idle"),
        "one agent is not a count worth spelling out"
    );
}

/// The second line prefers what a person wrote over what the filesystem
/// says, and never leaves the line empty.
#[test]
fn the_second_line_is_the_comment_when_there_is_one_and_the_path_otherwise() {
    let mut row = SidebarRow::worktree(4, "feat/x");
    row.path = Some(PathBuf::from("/tmp/projects/sirio-wt/feat-x"));

    assert!(
        Sidebar::sub_line_text(&row).contains("feat-x"),
        "with no comment the row falls back to its checkout path"
    );

    row.comment = Some("redesign the sidebar".to_owned());
    assert_eq!(
        Sidebar::sub_line_text(&row),
        "redesign the sidebar",
        "an annotation wins over the path"
    );
}

fn pill(tab_id: usize) -> SidebarPill {
    SidebarPill {
        tab_id: Some(tab_id),
        parked_tab: None,
        title: "Claude".to_owned(),
        icon: Icon::MessageSquare,
        brand: None,
        status: Some(ActivityStatus::Idle),
        selected: false,
    }
}
```

- [ ] **Step 2: Run them to see them fail**

```bash
cd rust && cargo test -p sirio_ui sidebar::row 2>&1 | tail -10
```

Expected: FAIL — no `status_text`, no `sub_line_text`.

- [ ] **Step 3: Implement the two helpers**

```rust
/// The row's status word. Zed puts a relative timestamp here; Sirio has no
/// per-pane timestamp to put there (only chat tabs persist one), so the slot
/// carries the state instead — the thing the user was reading the timestamp
/// to infer anyway.
pub(crate) fn status_text(row: &SidebarRow) -> Option<String> {
    match row.agent_status {
        Some(ActivityStatus::Running) => Some("running".to_owned()),
        Some(ActivityStatus::NeedsInput) => Some("needs input".to_owned()),
        Some(ActivityStatus::Done) => Some("done".to_owned()),
        Some(ActivityStatus::Error) => Some("error".to_owned()),
        Some(ActivityStatus::Idle) | None if row.pills.len() > 1 => {
            Some(format!("{} agents", row.pills.len()))
        }
        Some(ActivityStatus::Idle) => Some("idle".to_owned()),
        None => None,
    }
}

/// The card's second line. A worktree annotation is something a person
/// wrote about this checkout; the path is what is left when nobody did.
pub(crate) fn sub_line_text(row: &SidebarRow) -> String {
    row.comment
        .as_deref()
        .filter(|comment| !comment.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            row.path
                .as_deref()
                .map(sirio_project::display_path)
                .unwrap_or_default()
        })
}
```

- [ ] **Step 4: Rebuild the card in `render_row`**

Replace the row's body with the layout below, keeping every handler that is already attached (selection click, hover close for a removable worktree, context menu, drag). Delete the `running_agents` block, the `Primary` chip on the sub-line, the chevron block and the `tree::tree_row` frame — the flat card is Sirio's own frame because bezel's tree row paints indent guides and a chevron column, which this design has removed on purpose.

```rust
        let status = Self::status_text(&row);
        let card = div()
            .id(("sidebar-row", row_id))
            .debug_selector(move || format!("sidebar-row-{row_id}"))
            .group(hover_group.clone())
            .relative()
            .h(px(CARD_TWO_LINE_HEIGHT))
            .w_full()
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .gap(px(8.0))
            .when(selected, |this| this.bg(theme.card_selected_bg))
            .when(!selected, |this| {
                this.hover(|style| style.bg(theme.element_hover))
            })
            .child(status_dot)          // the existing RowStatusGlyph element
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(ROW_GAP))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(title_element)   // existing, selector unchanged
                            .when(row.is_primary, |this| {
                                this.child(
                                    div()
                                        .debug_selector(move || {
                                            format!("sidebar-primary-star-{row_id}")
                                        })
                                        .flex_none()
                                        .text_size(px(10.0))
                                        .text_color(theme.text_faint)
                                        .child("★"),
                                )
                            })
                            .when_some(status, |this, status| {
                                this.child(
                                    div()
                                        .debug_selector(move || {
                                            format!("sidebar-row-status-{row_id}")
                                        })
                                        .flex_none()
                                        .text_size(theme.typography.scaled(11.0))
                                        .text_color(status_color)
                                        .child(status),
                                )
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .line_height(px(ROW_SUB_LINE_HEIGHT))
                            .text_size(px(12.5))
                            .text_color(theme.text_faint)
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("sidebar-row-subline-{row_id}")
                                    })
                                    .min_w_0()
                                    .flex_1()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(Self::sub_line_text(&row)),
                            ),
                    ),
            );
```

`status_color` reuses the colour `RowStatusGlyph::for_status` already resolves for the dot, so the word and the dot never disagree. `theme.card_selected_bg` is the selection fill the tree row used through bezel — take the exact expression from the deleted `tree::tree_row` call site if the field is named differently.

Then make the card unconditionally two lines: `has_sub_line` returns `true` for `RowKind::Worktree`, so `row_min_height` returns `CARD_TWO_LINE_HEIGHT` for every worktree row.

- [ ] **Step 5: Update the conformance inventory**

`conformance.rs:304-315` asserts the row metrics. Every worktree row is now `CARD_TWO_LINE_HEIGHT`; keep `ROW_HEIGHT` only if something still uses it (the section header has its own `SECTION_HEIGHT`), and add:

```rust
    assert_eq!(
        crate::sidebar::SECTION_HEIGHT,
        26.0,
        "project section headers 26px"
    );
```

- [ ] **Step 6: Run the tests, format, commit**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -30
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/row.rs rust/crates/sirio_ui/src/conformance.rs
Scripts/set-workspace-version.sh 0.9.0
git add rust/Cargo.toml rust/crates/sirio_ui/src
git commit -m "feat: draw a worktree as a two-line card with its status"
```

---

### Task 6: The pills, and what clicking them does

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/row.rs`

**Interfaces:**
- Consumes: `SidebarPill`; `SidebarEvent::{SelectTab, SelectParkedTab, CloseTab}` — all three already exist, `CloseTab` included, so the `x` reaches the same host transition the tab bar's close does. The `+` needs no event either: `Sidebar::open_context_menu` (`sidebar.rs:1297`) already builds the worktree menu whose seven `SidebarContextAction::NewTab(..)` entries (`sidebar.rs:1211-1247`) are exactly "new terminal / new chat / one per agent", and the host already routes them through `WorkspaceAction::NewTabForWorktree` with the clicked path captured (`sirio/src/main.rs:6610-6632`). **No new event, and no host change in this task.**
- Produces: the selectors `sidebar-pill-{row_id}-{index}`, `sidebar-pill-close-{row_id}-{index}`, `sidebar-pill-add-{row_id}`.

- [ ] **Step 1: Write the failing tests**

In `row.rs`'s `mod tests`:

```rust
/// A pill is the tab: clicking it selects that tab, exactly as clicking the
/// tab's own row used to.
#[gpui::test]
async fn clicking_a_pill_selects_its_tab(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    let events = tests_support::collect_events(&window, &mut cx);
    cx.simulate_click_on("sidebar-pill-1-0");
    cx.run_until_parked();

    assert!(
        matches!(events.borrow().last(), Some(SidebarEvent::SelectTab(_))),
        "the pill reports the same selection the tab row reported"
    );
}

/// Closing from the sidebar is new; it must reach the same host transition
/// the tab bar's close reaches, not a second teardown path.
#[gpui::test]
async fn the_pill_close_reports_close_tab(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    let events = tests_support::collect_events(&window, &mut cx);
    cx.simulate_click_on("sidebar-pill-close-1-0");
    cx.run_until_parked();

    assert!(
        matches!(events.borrow().last(), Some(SidebarEvent::CloseTab(_))),
        "the x closes the tab it sits on"
    );
}

/// A parked pill is a record, not a live surface: it restores its worktree's
/// strip rather than selecting a tab id that does not exist.
#[gpui::test]
async fn a_parked_pill_restores_instead_of_selecting(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_parked_tab(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    let events = tests_support::collect_events(&window, &mut cx);
    cx.simulate_click_on("sidebar-pill-1-0");
    cx.run_until_parked();

    assert!(
        matches!(
            events.borrow().last(),
            Some(SidebarEvent::SelectParkedTab { .. })
        ),
        "a parked pill carries its strip position, not a live id"
    );
}
```

`tests_support::collect_events` subscribes to the sidebar entity and pushes each `SidebarEvent` into an `Rc<RefCell<Vec<_>>>`; `cx.simulate_click_on` is whatever the existing sidebar tests use to click a debug selector — copy the idiom from a test that already clicks `remove-worktree-*`. `sidebar_with_parked_tab` is the Task 2 fixture with the second worktree's tabs supplied as `SidebarTabRef::Parked(0)`.

- [ ] **Step 2: Run them to see them fail**

```bash
cd rust && cargo test -p sirio_ui sidebar::row 2>&1 | tail -15
```

Expected: FAIL — the pill selectors do not exist.

- [ ] **Step 3: Render the pills**

Add to `row.rs` and hang the result off the card's second line, right-aligned (`.ml_auto()` on the container):

```rust
/// Pill geometry: 20px is the smallest square that still holds a legible
/// brand glyph *and* a close x in the same box, which is what lets the x
/// replace the glyph on hover instead of needing a slot of its own.
const PILL_SIZE: f32 = 20.0;

fn render_pills(
    row: &SidebarRow,
    entity: gpui::Entity<Sidebar>,
    theme: Theme,
) -> impl IntoElement {
    let row_id = row.id;
    let worktree_path = row.path.clone();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .children(row.pills.iter().enumerate().map(|(index, pill)| {
            let select_entity = entity.clone();
            let close_entity = entity.clone();
            let pill_group = SharedString::from(format!("sidebar-pill-group-{row_id}-{index}"));
            let tab_id = pill.tab_id;
            let parked = pill.parked_tab;
            let path = worktree_path.clone();
            div()
                .id(("sidebar-pill", row_id * 32 + index))
                .debug_selector(move || format!("sidebar-pill-{row_id}-{index}"))
                .group(pill_group.clone())
                .relative()
                .w(px(PILL_SIZE))
                .h(px(PILL_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.chip)
                .bg(theme.surface_raised)
                .when(pill.selected, |this| {
                    this.border_1().border_color(theme.accent)
                })
                // A parked tab is a record of what the worktree holds, not an
                // open surface — the same distinction the parked tab row drew
                // with a faint title.
                .when(parked.is_some(), |this| this.opacity(0.6))
                .hover(|style| style.bg(theme.element_hover))
                .child(
                    div()
                        .group_hover(pill_group.clone(), |style| style.invisible())
                        .child(
                            IconElement::new(pill.icon, IconSize::XSmall).text_color(
                                pill.brand.map_or(theme.text_muted, |brand| brand.color()),
                            ),
                        ),
                )
                .when_some(pill.status, |this, status| {
                    this.when(status != ActivityStatus::Idle, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-2.0))
                                .right(px(-2.0))
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(crate::right_panel::status_color(status, theme)),
                        )
                    })
                })
                .when_some(tab_id, |this, tab_id| {
                    this.child(
                        div()
                            .id(("sidebar-pill-close", row_id * 32 + index))
                            .debug_selector(move || {
                                format!("sidebar-pill-close-{row_id}-{index}")
                            })
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .invisible()
                            .group_hover(pill_group, |style| style.visible())
                            .child(IconElement::new(Icon::Close, IconSize::XSmall))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                close_entity.update(cx, |_, cx| {
                                    cx.emit(SidebarEvent::CloseTab(tab_id));
                                });
                            }),
                    )
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    select_entity.update(cx, |_, cx| match (tab_id, parked, path.clone()) {
                        (Some(id), _, _) => cx.emit(SidebarEvent::SelectTab(id)),
                        (None, Some(index), Some(path)) => {
                            cx.emit(SidebarEvent::SelectParkedTab { path, index })
                        }
                        _ => {}
                    });
                })
        }))
        .when(row.selected, |this| {
            let add_entity = entity.clone();
            let path = row.path.clone();
            this.child(
                div()
                    .id(("sidebar-pill-add", row_id))
                    .debug_selector(move || format!("sidebar-pill-add-{row_id}"))
                    .w(px(PILL_SIZE))
                    .h(px(PILL_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.chip)
                    .border_1()
                    .border_color(theme.border)
                    .text_color(theme.text_faint)
                    .hover(|style| style.bg(theme.element_hover))
                    .child("+")
                    // The same menu the worktree's right-click offers: it
                    // already lists New Terminal, New Chat and one entry per
                    // agent, and the host already carries the clicked path
                    // through `NewTabForWorktree`. A second, bare "new tab"
                    // event would have to pick an agent for the user.
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        add_entity.update(cx, |sidebar, cx| {
                            sidebar.open_context_menu(row_id, event.position(), window, cx);
                        });
                    }),
            )
        })
}
```

`path` is then unused in the `+` branch — drop the binding rather than leaving it.

- [ ] **Step 4: Run the tests**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
```

Expected: the three new tests PASS. Add a fourth if the menu path is worth pinning: click `sidebar-pill-add-1`, `cx.executor().advance_clock(Duration::from_secs(1))`, then assert `cx.debug_bounds("sidebar-context-menu").is_some()`.

- [ ] **Step 5: Format and commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/row.rs
Scripts/set-workspace-version.sh 0.10.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: make sidebar tab pills selectable, closable and addable"
```

---

### Task 7: The gradient fade

**Files:**
- Create: `rust/crates/sirio_ui/src/sidebar/fade.rs`
- Modify: `rust/crates/sirio_ui/src/sidebar/row.rs`, `rust/crates/sirio_ui/src/sidebar/section.rs`

**Interfaces:**
- Consumes: `gpui::{linear_gradient, linear_color_stop}` (the pattern already used at `sirio_ui/src/chat/thought.rs:187`).
- Produces:
  ```rust
  pub(super) fn fade_right(background: gpui::Rgba, width: f32) -> gpui::Div
  ```

- [ ] **Step 1: Write the failing test**

In `row.rs`'s `mod tests`:

```rust
/// A branch too long for the row slides under a veil instead of ending in
/// an ellipsis — the whole point of the Zed treatment.
#[gpui::test]
async fn a_long_title_fades_instead_of_truncating(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window =
        cx.add_window(|_window, cx| tests_support::sidebar_with_long_branch_name(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("sidebar-row-fade-1").is_some(),
        "the veil is drawn over the row's text column"
    );
}
```

`sidebar_with_long_branch_name` is the Task 2 fixture with the second worktree's branch replaced by `feat/sidebar-zed-like-redesign-with-pills`.

- [ ] **Step 2: Run it to see it fail**

```bash
cd rust && cargo test -p sirio_ui a_long_title_fades 2>&1 | tail -10
```

Expected: FAIL — no such selector.

- [ ] **Step 3: Write `fade.rs`**

```rust
//! The right-edge veil: text runs out under a gradient instead of stopping
//! at an ellipsis.
//!
//! Zed does this with a `GradientFade` component (`crates/ui/src/components/
//! gradient_fade.rs` in its tree, 92px wide with the stop at 0.7 where its
//! sidebar uses it). That crate is GPL-3.0-or-later and Sirio is MIT, so the
//! measurements are adopted and the code is not: this is the same idea
//! written against gpui's own `linear_gradient`, which is Apache-2.0 and
//! already used for the thought-block fade in `chat/thought.rs`.
//!
//! The veil is opaque paint, not a mask, so it must be given the colour of
//! whatever it sits on — a veil in the resting colour over a hovered row
//! reads as a patch, which is why the caller passes the background it
//! resolved for this frame rather than a constant.

use gpui::{Div, Rgba, div, linear_color_stop, linear_gradient, prelude::*, px};

/// Width of the veil. Zed's sidebar uses 92px; the same span reads the same
/// way at Sirio's 325px default width.
pub(super) const FADE_WIDTH: f32 = 92.0;
/// Where the solid end of the gradient starts. Below this the veil is fully
/// the row's colour; above it, it thins out to nothing.
const FADE_STOP: f32 = 0.7;

pub(super) fn fade_right(background: Rgba, width: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .w(px(width))
        .bg(linear_gradient(
            90.,
            linear_color_stop(background, FADE_STOP),
            linear_color_stop(gpui::Rgba { a: 0.0, ..background }, 0.),
        ))
}
```

Check `linear_color_stop`'s accepted colour type against `chat/thought.rs:187` and convert (`Hsla`/`Rgba`) to match; the zero-alpha stop is the only subtlety — it must be the *same* colour at alpha 0, not `transparent_black`, or the gradient runs through grey.

- [ ] **Step 4: Apply it in three places**

In `row.rs`, wrap the card's text column in a `.relative()` container and add, as its last child:

```rust
                    .child(
                        super::fade::fade_right(row_background, super::fade::FADE_WIDTH)
                            .id(("sidebar-row-fade", row_id))
                            .debug_selector(move || format!("sidebar-row-fade-{row_id}")),
                    )
```

`row_background` is the fill the card resolved this frame: the selection colour when selected, otherwise the surface. gpui's `group_hover` cannot restyle a gradient's stops, so the hovered case takes the same treatment Zed's does — pass the hover colour through a `.group_hover(...)` style that swaps the whole background — or, if that reads as noise at review time, leave the resting colour and note it: the veil over a hovered row is one step off, which is the trade Zed avoids only by carrying three colours.

Apply the same veil to the card's second line and, in `section.rs`, to the header's name column so the `+` and `⋯` emerge from under a long project name.

- [ ] **Step 5: Run the tests, format, commit**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/fade.rs rust/crates/sirio_ui/src/sidebar/row.rs rust/crates/sirio_ui/src/sidebar/section.rs
Scripts/set-workspace-version.sh 0.11.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: fade sidebar text at the right edge instead of truncating"
```

---

### Task 8: Keyboard across the pills

`↑`/`↓` already walk the rows through bezel's tree actions. `←`/`→` used to collapse and expand; with no nesting left they move along the cursor row's pills, and `Backspace` closes the focused one.

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/mod.rs` (`tree_step`, the action handlers, a new `pill_cursor` field), `rust/crates/sirio_ui/src/sidebar/row.rs` (draw the pill cursor)

**Interfaces:**
- Consumes: `tree::{Collapse, Expand, SelectNext, SelectPrevious}`, `Sidebar::tree_cursor`.
- Produces: `Sidebar::pill_cursor: Option<usize>` — the index within the cursor row's pills, cleared whenever `tree_cursor` moves.

- [ ] **Step 1: Write the failing test**

```rust
/// Left and right used to open and close a nesting level that no longer
/// exists; they now walk the row's pills, and Backspace closes the one the
/// keyboard is on.
#[gpui::test]
async fn arrow_keys_walk_the_pills_of_the_cursor_row(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    window
        .update(&mut cx, |sidebar, _, cx| {
            sidebar.focus_row_with_pills(cx);
        })
        .unwrap();
    cx.simulate_keystrokes("right");
    cx.run_until_parked();

    let pill_cursor = window
        .update(&mut cx, |sidebar, _, _| sidebar.pill_cursor)
        .unwrap();
    assert_eq!(pill_cursor, Some(0), "right lands on the first pill");

    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    let pill_cursor = window
        .update(&mut cx, |sidebar, _, _| sidebar.pill_cursor)
        .unwrap();
    assert_eq!(pill_cursor, Some(1), "and then the second");
}
```

`focus_row_with_pills` is a `#[cfg(test)]` helper on `Sidebar` that puts `tree_cursor` on the fixture's tab-holding worktree.

- [ ] **Step 2: Run it to see it fail**

```bash
cd rust && cargo test -p sirio_ui arrow_keys_walk_the_pills 2>&1 | tail -10
```

Expected: FAIL — no field `pill_cursor`.

- [ ] **Step 3: Implement**

Add `pill_cursor: Option<usize>` to `Sidebar`, defaulting to `None`. In the `tree::Collapse` / `tree::Expand` handlers, replace the `tree_step(Direction::Left/Right)` calls with movement along `visible_rows()[self.tree_cursor].pills`: `Expand` advances (`None → Some(0) → Some(1) → …`, stopping at the last), `Collapse` retreats and falls off the front back to `None`. Any change to `tree_cursor` sets `pill_cursor = None`. Bind `backspace` to an action that emits `SidebarEvent::CloseTab` for the pill at `pill_cursor` when it has a `tab_id`.

In `row.rs`, draw the pill cursor as the same outline the selected pill uses, so the keyboard's position is visible.

- [ ] **Step 4: Run the tests, format, commit**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs rust/crates/sirio_ui/src/sidebar/row.rs
Scripts/set-workspace-version.sh 0.12.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: walk the sidebar pills with the arrow keys"
```

---

### Task 9: The sticky section header

Last on purpose: gpui has no `position: sticky`, so this is the one item that has to read the scroll offset by hand. If it turns out to cost more than it is worth, the sidebar is correct without it and this task can be dropped.

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar/mod.rs` (a `ScrollHandle` on the list), `rust/crates/sirio_ui/src/sidebar/section.rs`

**Interfaces:**
- Consumes: `gpui::ScrollHandle`, `SECTION_HEIGHT`, `CARD_TWO_LINE_HEIGHT`.
- Produces: `Sidebar::list_scroll: ScrollHandle` and `fn sticky_section(&self, rows: &[SidebarRow]) -> Option<SidebarRow>`.

- [ ] **Step 1: Write the failing test**

```rust
/// Scrolled past its own header, a section still says which project you are
/// reading — the header is pinned to the top of the viewport.
#[gpui::test]
async fn a_scrolled_section_keeps_its_header_on_screen(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let window = cx.add_window(|_window, cx| tests_support::sidebar_with_many_worktrees(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    window
        .update(&mut cx, |sidebar, _, _| {
            sidebar.list_scroll.set_offset(gpui::point(px(0.0), px(-400.0)));
        })
        .unwrap();
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("sidebar-sticky-section").is_some(),
        "the current section's header is pinned while its rows scroll"
    );
}
```

`sidebar_with_many_worktrees` is the Task 2 fixture with twelve worktrees in the first project, enough to scroll a 620px list.

- [ ] **Step 2: Run it to see it fail**

```bash
cd rust && cargo test -p sirio_ui a_scrolled_section_keeps 2>&1 | tail -10
```

Expected: FAIL — no `list_scroll`, no sticky element.

- [ ] **Step 3: Implement**

Add `list_scroll: ScrollHandle` to `Sidebar` and `.track_scroll(&self.list_scroll)` to the scrolling container (the `.overflow_y_scroll()` div). Then:

```rust
    /// Which section header belongs at the top of the viewport: the last one
    /// whose own offset has already scrolled past. Heights are known
    /// (`SECTION_HEIGHT` and `CARD_TWO_LINE_HEIGHT`), so this is arithmetic
    /// over the flattened list rather than a measurement pass.
    fn sticky_section(&self, rows: &[SidebarRow]) -> Option<SidebarRow> {
        let scrolled = -self.list_scroll.offset().y.0;
        if scrolled <= 0.0 {
            return None;
        }
        let mut offset = 0.0;
        let mut current = None;
        for row in rows {
            let height = match row.kind {
                RowKind::Project => SECTION_HEIGHT,
                _ => CARD_TWO_LINE_HEIGHT,
            };
            if offset > scrolled {
                break;
            }
            if row.kind == RowKind::Project {
                current = Some(row.clone());
            }
            offset += height + ROW_V_GAP;
        }
        current
    }
```

Render the result as an absolutely positioned child of the list container, above the rows, with the debug selector `sidebar-sticky-section` and the same `render_section` body.

- [ ] **Step 4: Run the tests, format, commit**

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -20
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio
rustfmt --edition 2024 rust/crates/sirio_ui/src/sidebar/mod.rs rust/crates/sirio_ui/src/sidebar/section.rs
Scripts/set-workspace-version.sh 0.13.0
git add rust/Cargo.toml rust/crates/sirio_ui/src/sidebar
git commit -m "feat: pin the sidebar section header while its rows scroll"
```

---

## Closing the loop

After Task 9, ask the user to run the gate (`Scripts/ci.sh`) — an agent never launches it. Then delete the two prototypes' claim on the future: they stay in `docs/prototypes/` as the record of why the layout is what it is, and the spec's "Status" line moves from *pending user review* to *implemented*.

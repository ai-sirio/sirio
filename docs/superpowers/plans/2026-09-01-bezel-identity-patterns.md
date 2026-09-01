# Bezel Identity Patterns Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adopt bezel's identity surfaces in `sirio_ui` — chat transcript, changes/diff, document editor, sidebar tree — with the gallery at tag `v0.1.4` as the visual source of truth.

**Architecture:** Two mechanics per the spec: pattern copy (transcript, diff — transcribe gallery structure/styling, delete the hand-rolled equivalent) and crate adoption (`bezel-markdown`, `bezel-editor`, `bezel-syntax` as new direct deps pinned `=0.1.4`; `ui::tree` from the facade). One branch, one commit per family, single visual review gates the merge.

**Tech Stack:** Rust, gpui (`bezel-gpui =0.3.8`), bezel `=0.1.4` + `bezel-markdown`/`bezel-editor`/`bezel-syntax` `=0.1.4`.

**Spec:** `docs/superpowers/specs/2026-09-01-bezel-identity-patterns-design.md`

## Global Constraints

- Gallery reference is tag `v0.1.4` of `https://github.com/crabtalk/bezel` (`git clone --depth 1 --branch v0.1.4`), never `main` (main uses unreleased APIs). Patterns live in `apps/gallery/src/patterns/`.
- New deps pinned exactly: `bezel-markdown = "=0.1.4"`, `bezel-editor = "=0.1.4"`, `bezel-syntax = "=0.1.4"`. **`bezel-terminal` must never enter the tree.**
- No user-visible behavior change beyond the bezel look: focus order, shortcuts, submit/dismiss, editor conflict flows identical.
- Tests update only where they asserted rendering details of the old implementation, never where they assert behavior.
- Test failures compare as **lists** against the baseline measured on the branch point (Task 0); the `sirio` crate has a known oscillating red pool. A new name = regression; confirm any suspect with `--exact` in isolation.
- Zig exactly 0.15.2 on PATH for anything building `sirio_terminal`: `export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"`.
- Never run `Scripts/ci.sh` / `Scripts/ci-linux.sh`; iterate with `cargo build/test -p <crate>`.
- Commits: Conventional Commits, lower-case imperative.
- Never touch the user's uncommitted WIP: `AGENTS.md`, `CLAUDE.md`, and `rust/crates/sirio_ui/src/sidebar.rs` until Task 4's gate opens.

---

### Task 0: Dependencies and baseline

**Files:**
- Modify: `rust/Cargo.toml` (workspace deps), `rust/crates/sirio_ui/Cargo.toml`
- Reference: clone gallery to a temp dir

**Interfaces:**
- Produces: `bezel-markdown`, `bezel-editor`, `bezel-syntax` importable from `sirio_ui` as `markdown`, `editor`, `syntax` (or crate names — match how the gallery imports them: `use markdown::…`, `use editor::Editor`); baseline failing-test list committed to the task report (not the repo).

- [ ] **Step 1: Branch** — `git checkout -b feat/bezel-identity-patterns` from current `main`.
- [ ] **Step 2: Add deps.** In `rust/crates/sirio_ui/Cargo.toml`, alongside the existing `bezel` entry:

```toml
bezel-markdown = "=0.1.4"
bezel-editor = "=0.1.4"
bezel-syntax = "=0.1.4"
```

(If the workspace centralizes versions, add them to `rust/Cargo.toml` `[workspace.dependencies]` and reference with `{ workspace = true }`, matching how `bezel` is declared.)
- [ ] **Step 3: Verify build** — `cd rust && cargo build -p sirio_ui`. Expected: compiles; lockfile gains exactly the three crates (plus their own deps). `grep bezel-terminal rust/Cargo.lock` must return nothing new beyond what existed before.
- [ ] **Step 4: Baseline test lists.** Run and record names of failures:

```bash
cargo test -p sirio_ui 2>&1 | grep "^test .* FAILED\|failures:" -A 40
cargo test -p sirio 2>&1 | grep "^test .* FAILED\|failures:" -A 40
```

- [ ] **Step 5: Commit** — `chore: add bezel markdown/editor/syntax deps pinned =0.1.4`

### Task 1: chat — transcript pattern + bezel-markdown bodies

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs`
- Reference: gallery `apps/gallery/src/patterns/transcript.rs` (struct `Transcript`, struct `Turn` — card layout, role styling, spacing)

**Interfaces:**
- Consumes: `markdown::parse(&str) -> Doc`, `markdown::render(doc: &Doc, caption, window, cx) -> AnyElement`, `markdown::{Doc, BlockKind, Mark}`, `markdown::set_highlighter` + `syntax` for code blocks.
- Produces: chat message bodies painted by bezel-markdown; the deleted symbols are `render_markdown` (chat.rs:3767), `render_markdown_block` (3817), `render_markdown_list` (4139), `render_markdown_table` (4248). `render_markdown_document_with_link_override` (3809) survives as the seam: it now parses to `Doc` and renders via bezel, applying the link override through bezel-markdown's link hooks (`set_link_preview` / mark handling) or, if bezel has no per-render hook, by post-processing the `Doc`'s link marks before render.

- [ ] **Step 1: Map the seam.** Read `render_markdown_document_with_link_override` and every caller; list which chat tests exercise markdown rendering (`cargo test -p sirio_ui chat -- --list | grep -i markdown` plus table/list/code assertions).
- [ ] **Step 2: Wire the highlighter once at init** (same place chat sets up its statics or in sirio_ui init path):

```rust
markdown::set_highlighter(syntax::highlighter());
```

(Verify the exact constructor in `bezel-syntax`'s lib.rs — the gallery's build script shows usage.)
- [ ] **Step 3: Swap the renderer.** Replace the body of `render_markdown_document_with_link_override` with parse + render via bezel; delete `render_markdown`, `render_markdown_block`, `render_markdown_list`, `render_markdown_table` and helpers only they used.
- [ ] **Step 4: Transcript layout.** Restyle the turn/card chrome (padding, role header, bubble vs card) following the gallery `Transcript`/`Turn` structure. Keep `TRANSCRIPT_WIDTH`, `CARD_H_PADDING`, `CARD_V_PADDING` names if conformance tests assert them — update values to gallery ones and fix the conformance assertions in the same commit.
- [ ] **Step 5: Streaming cost check.** Add one coarse test: parse+build of a ~4 KB markdown turn under 5 ms release / 50 ms debug (assert with `std::time::Instant`, generous bound — it exists to catch accidental O(n²), not to benchmark).
- [ ] **Step 6: Tests** — `cargo test -p sirio_ui chat`. Compare failure list to Task 0 baseline; fix regressions (behavior contract: link override still routes, tables/lists/code render).
- [ ] **Step 7: Commit** — `feat: adopt bezel transcript pattern and bezel-markdown chat bodies`

### Task 2: changes — diff pattern + scroll

**Files:**
- Modify: `rust/crates/sirio_ui/src/changes.rs`
- Reference: gallery `apps/gallery/src/patterns/diff.rs` (`Diff::row`, `Diff::header` — gutter, ink tones, change markers)

**Interfaces:**
- Consumes: `bezel::theme` ink tones as the gallery diff uses them (`theme::ink`), `ui::icons`, existing `sirio_git` diff model (`diff.hunks`, `DiffOrigin` — unchanged).
- Produces: diff rows/headers styled per gallery; the per-column horizontal scroll behavior (changes.rs:88-111 commentary, `overflow_y_scroll` at 2109) preserved — restyle, don't restructure the scroll solution.

- [ ] **Step 1: Read the row builder** at changes.rs:1043-1150 (hunk walk) and the gallery `Diff::row`/`Diff::header`; map each visual element (gutter numbers, origin marker, band styling) old→new.
- [ ] **Step 2: Restyle rows and headers** to gallery structure. Colors go through `sirio_theme` (add derived tokens there only if the gallery uses an ink tone Sirio's theme doesn't expose; document any new token in `docs/THEME-PROVENANCE.md`).
- [ ] **Step 3: Scroll.** Keep the existing reachability solution; adjust only paddings/metrics the restyle changes.
- [ ] **Step 4: Tests** — `cargo test -p sirio_ui changes`. List-compare; changes tests that shell to `git status` flake under load — rerun isolated before judging.
- [ ] **Step 5: Commit** — `feat: adopt bezel diff pattern for the changes view`

### Task 3: editor/file_view — bezel-editor for markdown, bezel-syntax for code

**Files:**
- Modify: `rust/crates/sirio_ui/src/file_view.rs`, `rust/crates/sirio_ui/src/editor.rs` (seam only), app/test bootstrap for `editor::init`
- Reference: gallery `apps/gallery/src/patterns/editor.rs` (uses `editor::Editor`) and `document.rs` (read-only `markdown::{Doc, BlockKind}` render)

**Interfaces:**
- Consumes: `editor::init(cx)`, `editor::Editor::new(source: &str, cx) -> Editor` (a gpui Entity), `Editor::doc() -> &Doc`, `markdown::serialize` for the wire form back to the headless model.
- Produces: for markdown files, `file_view.rs` hosts a `bezel_editor::Editor` entity; buffer syncs to `sirio_ui::editor::Editor` (headless model — load/save/conflict, unchanged) via `markdown::serialize(editor.doc())` on change, and reloads feed `Editor::new` again. Code files keep the custom no-wrap editor and gain `bezel-syntax` highlighting of the visible buffer.

- [ ] **Step 1: Inventory the pixel half.** List file_view.rs's caret/selection/toolbar code paths for markdown files vs code files (`Language::from_path` decides). Confirm which existing tests cover F-EDIT-02 (formatting), F-EDIT-04/05/06 (save/conflict) — those must stay green untouched.
- [ ] **Step 2: `editor::init(cx)`** into app bootstrap next to `input::init` (find where SP2 put it) and into every `TestAppContext` setup exercising file_view.
- [ ] **Step 3: Markdown path.** Mount `bezel_editor::Editor` for markdown documents; on edit, `markdown::serialize(doc)` → headless model's buffer (dirty tracking flows as before); on reload/keep, rebuild the entity from the model's buffer. The headless model's own markdown format ops (F-EDIT-02 toolbar) route to bezel-editor's equivalents where they exist; where bezel-editor already binds the key (bold/italic), delete the duplicate toolbar path rather than double-handling.
- [ ] **Step 4: Code path.** Keep the custom editor; feed the shaped lines through `bezel-syntax` highlighting (same highlighter instance registered in Task 1). No-wrap, indent, horizontal scroll unchanged (F-EDIT-07).
- [ ] **Step 5: Tests** — `cargo test -p sirio_ui file_view` and `cargo test -p sirio_ui editor`. List-compare. The headless editor tests must pass **unmodified** — any needed change there means the seam leaked and the design is being violated.
- [ ] **Step 6: Commit** — `feat: adopt bezel-editor for markdown documents and bezel-syntax for code`

### Task 4: sidebar — ui::tree (GATED)

**Gate:** starts only when `rust/crates/sirio_ui/src/sidebar.rs` is clean in `git status` (user committed or shelved their WIP). If still dirty when Tasks 0–3 are done, stop: the sub-project merges without this family and this task lands as a follow-up.

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar.rs`

**Interfaces:**
- Consumes: `ui::tree::{Row, Direction, Move, init, tree, tree_row, step, parent_of}` — `Row::leaf(depth)` / `Row::branch(depth, expanded)`.
- Produces: project/worktree rows built on `tree()`/`tree_row(theme, row, selected, cursor)`; keyboard navigation through `tree::step` replacing any hand-rolled up/down/expand logic; `tree::init(cx)` in bootstrap + test setups. Sidebar events (`SidebarEvent::SelectWorktree` etc.) unchanged.

- [ ] **Step 1: Verify gate** — `git status --short rust/crates/sirio_ui/src/sidebar.rs` must be empty.
- [ ] **Step 2: Map rows.** Project header → `Row::branch(0, expanded)`; worktree → `Row::leaf(1)`. Keep Sirio's row content (identity glyph, branch name, activity dot) as `tree_row`'s children — the tree supplies structure/indent/chrome, not the content.
- [ ] **Step 3: Navigation** through `tree::step`; existing selection/expand semantics and tests are the contract (the 73 sidebar tests are the net).
- [ ] **Step 4: Tests** — `cargo test -p sirio_ui sidebar`. List-compare.
- [ ] **Step 5: Commit** — `feat: adopt bezel tree for the sidebar`

### Task 5: Integration verification

- [ ] **Step 1:** `cargo build -p sirio` and `cargo test -p sirio_ui`; list-compare against Task 0 baseline across the whole crate.
- [ ] **Step 2:** `Scripts/build-dev.sh` (with Zig PATH) to relaunch for the **user's single visual review** — the gate for merging to main. Do not merge without it.

## Self-review notes

- Spec coverage: all four families + exclusions honored; `bezel-terminal` guarded by a lockfile grep in Task 0.
- The one deliberately open point: bezel-markdown's exact link-override hook (Task 1 Step 3 names two fallbacks). The executor verifies against the crate source; both routes preserve the behavior contract.

# T5-prj build plan — F-PRJ, 13 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T5-prj.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

All 18 `F-PRJ` VERIFY clauses (with their original `App/AddProjectSheet.swift` /
`App/ProjectSettingsSheet.swift` line references) are in `docs/linux-rewrite/01-inventory-app.md`
lines 40-57; that is where the exact clause wording below comes from.

---

## `F-PRJ-01` — FAILED — defective

**Needs: build.** Confirmed live: `render_add_project_menu` (`sidebar.rs:1741`) does draw an
opaque panel — `.bg(theme.card_fill)` at `sidebar.rs:1756`, and `card_fill` resolves to
`rgb_hex(0x232323)`/`0xECECEC` (`tiller_theme/src/lib.rs:269`) whose `a` is hard-coded `1.0`
(`rgb_hex`, `tiller_theme/src/lib.rs:974-981`). The background is opaque; the defect is paint
**order**, not alpha. In `Sidebar::render` (`sidebar.rs:2372`), the menu is mounted as a child
*before* the filter field and the row list:

```
.when(add_project_menu, |this| { this.child(Self::render_add_project_menu(...)) })  // :2436
.child(div().id("filter-field") ...)                                                // :2440
... rows further below ...
```

Every other absolutely-positioned overlay in this same file — `context_menu`,
`project_settings`, `project_form` — is deliberately appended as the **last** children of the
root div (`sidebar.rs:2631-2638`), specifically so later paint order puts them on top. The
add-project menu breaks that established pattern by mounting early, so the filter field and the
row list (painted after it) composite over it — exactly matching the evidence ("the sidebar
Filter field and the `tiller` project path composite straight through it").

**Approach**: move the `.when(add_project_menu, ...)` block down to sit alongside (just before
or after) the other three `.when_some(...)` overlay blocks at the end of the child chain, matching
the pattern the other three already use. This is a same-file, few-line reorder — no new state,
no new component.

- **files**: `rust/crates/tiller_ui/src/sidebar.rs` (`Sidebar::render`, ~`sidebar.rs:2372-2640`)
- **size**: S

---

## `F-PRJ-03` — FAILED — absent

**Needs: build.** The row's VERIFY clause (`01-inventory-app.md:42`) is *not* about the `+`
menu having an "Open Project" entry — it is: "Handle a browsed non-Git folder with initialize,
add without Git, and cancel choices." The ledger's evidence text ("no Open Project/browse entry
exists") is now stale on its own terms — `sidebar.rs:1760`'s `"Open Project…"` menu item, wired
to `start_open_project` (`sidebar.rs:974`, using `cx.prompt_for_paths`), is real and is the same
control F-PRJ-01 independently confirms exists (superseding whatever build F-PRJ-03's sweep ran
against). But the verdict itself still holds for the *actual* clause: whatever path is picked —
via the folder picker or via Clone/Create — always lands in `Workspace::add_project`
(`tiller/src/main.rs:2956`), which calls `ProjectCatalog::add` (`session.rs:487`). `add()` calls
`discover_project` and silently accepts the result whether or not `.git` was found — there is no
branch anywhere that offers Initialize Git / Add without Git / Cancel for a picked non-Git
folder; it is simply added as a non-git project every time (the same "Repository: Folder" state
F-PRJ-11's evidence separately enumerates).

**Approach**: this needs a new three-way prompt UI (mirroring `request_remove_project`'s use of
`window.prompt`, `sidebar.rs:1061`, or a small custom sheet if git-init-vs-not needs its own
copy) inserted between "a path was picked" and "the project is added," plus a session-layer
branch: on `Initialize Git`, call the same `init_repository` used by `SidebarContextAction::
InitializeGit` (`main.rs:3177`) before adding; on `Add without Git`, add as-is (today's only
path); on `Cancel`, discard the pick. The branch point is `add_project` in `main.rs`, so this
fix touches the file 29 other rows already touch — flag deliberately, not a surprise.

- **files**: `rust/crates/tiller/src/main.rs` (`add_project`, `:2956`), `rust/crates/tiller_ui/src/sidebar.rs` (new prompt state/render, alongside `WorktreePrompt`/`request_remove_project` for the pattern)
- **size**: M

---

## `F-PRJ-06` / `F-PRJ-09` — half-proven (both)

**Needs: exercise, both — shared cause.** The in-flight submission guard both rows are missing
proof for is a correct, already unit-tested, pure state machine in `project_forms.rs`:
`CloneFormState::begin()`/`can_submit()` (`:70-85`) and `CreateFormState::begin()`/`can_submit()`
(`:444-458`) both flip status to `Running` synchronously on the first accepted call and refuse a
second one while `Running`; `clone_state_disables_empty_url_and_double_submission` (`:762`) and
`create_state_disables_empty_name_and_double_submission` (`:801`) already assert this at the
state level. The button `on_click` handlers (`:373`, and the mirrored Create button) call
`form.submit(cx)` directly with no debounce needed — GPUI dispatches one click at a time, so the
guard is inherently race-free *if* `Running` persists long enough for a second click to land
inside it.

That's exactly what the live environment can't currently produce for either form:
`CloneForm::submit` (`:164-234`) shells out to a real `git clone` with **no network** in this
environment, so it fails near-instantly; `CreateForm::submit` (`:521-545`) does a local
`mkdir`+`git init` on `cx.background_spawn`, which also resolves in well under a frame on local
disk. In both cases `Running` never survives long enough for a live second click to land inside
it — F-PRJ-09's own evidence shows this precisely: the second click "landed on empty sidebar
space after the dialog had already closed," because `CreateFormEvent::Created` had already fired
and unmounted the form. This is a proof-environment limitation, not a code defect.

**Approach (both rows)**: prove the guard live by making the in-flight window survive at least
one paint frame — e.g. shim `git` on `$PATH` with a wrapper that `sleep`s before delegating (for
Clone), or point Create's `parent` at a path where `mkdir`/`git init` is artificially slowed
(a FUSE/throttled mount, or a wrapper git shim again, since `create_project` also shells to
`git init`). With `Running` held open, fire two real, separately-dispatched clicks on
`clone-submit`/the Create equivalent and confirm only one worker starts (one project row, not
two) and the second click is a no-op against the still-`Running` guard.

- **files**: none (exercise only; `rust/crates/tiller_ui/src/project_forms.rs` is where the guard lives, already correct)
- **size**: S each

---

## `F-PRJ-07` — half-proven

**Needs: exercise.** The failure half is already proven live. The retry half is implemented and
unit-tested, not missing: `CloneFormState::set_url` (`project_forms.rs:39-44`) resets `status`
back to `Ready` on any edit while not `Running`, and `can_submit()`/`begin()` deliberately allow
resubmission from `Failed` ("A failed attempt is intentionally submit-able so the same form is
the retry surface," `:68-69`) — covered by `clone_failure_keeps_url_and_allows_retry`
(`:775`). `clone_repository` (`tiller_git/src/clone.rs:57`) is a plain `git clone <url> <dest>`
shell-out, which accepts a local filesystem path as `url` just as readily as a remote one, so the
retry gesture does not need outbound network to be genuine.

**Approach**: after the already-proven invalid-URL failure (red text, "Retry clone" label),
edit the URL field to a real local git repository path (`file:///…` or a bare path) and click
the relabeled button; confirm the status transitions `Failed → Running → Complete`,
`CloneFormEvent::Cloned` fires, and the project appears in the sidebar.

- **files**: none (exercise only)
- **size**: S

---

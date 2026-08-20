# Finding: ported model functions the app cannot reach

**Author:** lead, 2026-08-20. **Status:** the survey below is history; the triage that followed
it (bottom of this file, same day) carries the verdicts.
**Reproduce:** `python3 Scripts/dead-model-audit.py`
             `python3 Scripts/dead-model-audit.py rust/crates/tiller_git/src --control run_streaming`

## What prompted it

Closing F-CORE-WSP-05 meant asking a question the row's spec does not ask: *what in the
running app could reach this?* The row asserts that layout commands classify into structural
and nonstructural transitions, and the port does mirror Swift faithfully —
`classify_layout_command` (`rust/crates/tiller_project/src/layout.rs:333`) matches
`WorkspaceLayoutTransition.isStructuralCommand` arm for arm, and has unit tests.

It also had, in the entire workspace, **one** production caller: `main.rs:9483`, inside
`commit_tab_rename`, for `LayoutCommand::Rename`. Of the eight command variants, `Insert` and
`SetDividerFraction` appeared only in unit tests, and `Split`, `Move`, `Close`, `Activate`,
`UpdateViewState` were **never constructed anywhere**, tests included.

The app's move-tab and new-tab affordances exist and work; they simply do not route through
the model that the row is about. A row can therefore be true of the port's *code* and say
nothing about the port's *behaviour* — which is the failure mode the whole live-drive
discipline exists to prevent.

The obvious next question is whether this is one row's problem or a pattern.

## The survey

`Scripts/dead-model-audit.py` classifies every `pub fn` in a crate by who can reach it.

| | `tiller_project` | `tiller_git` |
|---|---|---|
| `pub fn` in production code | 66 | 57 |
| called by another crate | 41 | 46 |
| called only inside its own crate (over-public, but live) | 9 | 3 |
| **no production caller anywhere** | **16** | **8** |

The 16 in `tiller_project`:

`activity_pane_ids`, `activity_tab_id`, `browser_content_id`, `classify_file_drop`,
`collapse_project`, `decode_or_empty`, `effective_name`, `expand_project`, `from_values`,
`is_markdown_path`, `load_file_tree`, `move_item`, `resolve_worktree_defaults`,
`selected_tab`, `selected_worktree`, `worktree_content_id`.

The 8 in `tiller_git` (this list was missing from the first draft of this document, which is
part of why its closing count was wrong — see the correction below):

`discard_change_entries`, `discard_untracked_entries`, `is_cancelled`, `is_clean`,
`run_streaming_cancellable`, `stage_entries`, `statuses`, `unstage_entries`.

## Why this is not an alarm

Being unreachable is a **prompt to investigate, never a verdict**. The ledger has already
adjudicated members of this exact set, and did so correctly, by three different routes:

- **The behaviour lives elsewhere under another name.** F-CORE-DOM-05 states plainly that
  "`move_item` is dead (zero app callers)" and that the real implementation is
  `Sidebar::reorder_rows` (`sidebar.rs:758`) — then closes the row with a genuine OS-level
  drag through `wayland-drive.sh`, not with the model's unit tests.
- **The wired duplicate is the worse implementation; delete it and promote the model.**
  F-CORE-DOM-06: `numeric_tab_selection` was dead and correct, while the wired
  `TabSelection::jump` clamped out-of-range input instead of ignoring it. The duplicate was
  deleted, the model promoted, and `ctrl-7` on five tabs driven live to prove it.
- **The affordance exists but bypasses the model; route it through.** F-CORE-WSP-05, in
  progress.

So the survey's value is a **bounded list of candidates**, not a claim that 16 rows are
hollow. Most of these symbols are not cited by any ledger row at all: the ledger's `SRC`
column points at the *Swift reference* (`WorkspaceLayoutTransition.swift:49`), which is why
grepping Rust symbol names against it finds almost nothing, and why it cannot be used to
triage this list. Each candidate needs the same question F-CORE-WSP-05 got asked, one at a
time.

## A caution about the instrument

The first version of this audit reported **32** unreachable functions in `tiller_project`,
including `classify_layout_command` — which I already knew had a caller at `main.rs:9483`.
The parser truncated each file at its **first** `#[cfg(test)]`, discarding every line after
the first inline test module; in `main.rs` that is thousands of lines of production code
counted as tests.

Only the accidental presence of a known-positive symbol in the output exposed it. Had the
control not been there, "half of the domain crate is unreachable" would have been a dramatic,
believable, and entirely wrong finding.

The script therefore carries both controls explicitly and **refuses to print any numbers when
either fails** (exit 1), rather than printing numbers a reader would have no reason to
distrust. Verify with:

```
python3 Scripts/dead-model-audit.py --control zzz_no_such_symbol   # must exit 1, print nothing
```

## Correction: the count in the first draft was wrong

This document originally closed by saying there were "~14 unadjudicated" candidates across the
two crates. **That figure was a guess of the author's, not an output of the tool**, and it was
wrong in both directions: it was never reproducible from anything printed above it, and the
`tiller_git` list it was partly counting had been left out of the document entirely.

The reproducible number is **24** — 16 in `tiller_project` plus 8 in `tiller_git`, both printed
by the audit and both listed above. Of those, exactly **two** symbols (`move_item`,
`selected_tab`) appear anywhere in `INVENTORY-LEDGER.md`, so the honest count of *unadjudicated*
candidates was 22, not 14.

The lesson is the same one as the `#[cfg(test)]` bug below, in a cheaper form: a number written
into prose beside a tool's output inherits the tool's authority without inheriting its
reproducibility. Any figure in this directory that a reader cannot re-derive from a printed
command should be treated as an assertion, not a measurement.

## The triage (2026-08-20, all 24)

Every candidate was investigated, and each verdict was then handed to a second pass whose only
job was to refute it. Verdict letters are the ones defined in `Scripts/dead-model-audit.py`'s
own docstring: **A** behaviour lives elsewhere under another name · **B** the wired duplicate is
the worse implementation · **C** the affordance exists but bypasses the model · **D** no
affordance exists at all · **E** false positive, it is reachable · **F** legitimately
unreachable.

| verdict | count | symbols |
|---|---|---|
| **A** | 10 | `browser_content_id`, `activity_pane_ids`, `activity_tab_id`, `decode_or_empty`, `effective_name`, `resolve_worktree_defaults`, `move_item`, `from_values`, `is_clean`, `statuses` |
| **C** | 8 | `selected_tab`, `selected_worktree`, `expand_project`, `collapse_project`, `classify_file_drop`, `load_file_tree`, `is_cancelled`, `run_streaming_cancellable` |
| **F** | 5 | `worktree_content_id`, `stage_entries`, `unstage_entries`, `discard_change_entries`, `discard_untracked_entries` |
| **B** | 1 | `is_markdown_path` |
| **D**, **E** | 0 | — |

**No candidate was a false positive.** The audit did not miss a production caller anywhere in
the set, which is the first real evidence that the instrument is sound rather than merely
self-consistent.

### Two are defects, not cleanup

**1. Cancelling a clone does not cancel the clone.** Verified by hand, layer by layer:
`project_forms.rs:193` spawns a bare `std::thread::spawn` and keeps no handle; inside it,
`clone_repository` (`project_forms.rs:194`) reaches `GitRunner::run_streaming`
(`tiller_git/src/clone.rs:42`) — the variant that **cannot** be cancelled. The cancellable
variant exists and works: `run_streaming_cancellable` (`tiller_git/src/git.rs:249`) polls the flag at
`tiller_git/src/git.rs:309`. But `GitCancellationToken::new()` is constructed at exactly one place in the
repository, `tiller_git/src/git.rs:787`, which is inside the `#[cfg(test)]` module opened at `tiller_git/src/git.rs:668`. The
Cancel button (`tiller_ui/src/sidebar.rs:2746-2751`) sets `sidebar.project_form = None`. So
Cancel closes the dialog and the `git clone` child process keeps running to completion in a
detached thread that nothing can observe or stop.

**2. `.mdown` and `.mkdn` never get markdown treatment.** `is_markdown_path`
(`tiller_project/src/file_link.rs:36`) recognises them; the wired `Language::from_path`
(`tiller_ui/src/editor.rs:138`) does not, and it is that function's result which gates Preview
mode in `file_view.rs`. This is the textbook **B** case: the live duplicate is the poorer one.

### The pattern the per-symbol view hides

Nine of the 24 are not scattered helpers — they are **two whole subsystems that were ported and
never wired**:

- The `LegacyWorkspaceTab` / `WorkspaceSnapshot` cluster in `layout.rs` (`browser_content_id`,
  `worktree_content_id`, `activity_pane_ids`, `activity_tab_id`, `decode_or_empty`). Already
  recommended for wholesale deletion in `docs/linux-rewrite/WSP-LAYOUT-DECISION.md`.
- The selection-and-expansion model in `workspace.rs` (`selected_tab`, `selected_worktree`,
  `expand_project`, `collapse_project`, and `effective_name` from `project.rs`).
  `tiller_project::Workspace` is re-exported at `tiller_project/src/lib.rs:79` and **imported by no production
  file** — the only `tiller_project::Workspace*` references in the app are to `WorkspaceTab` and
  `WorkspaceTabViewState`, which are different types. The real state lives in
  `SidebarRow.expanded`/`.selected` and `TillerWorkspace.active_tab: usize`: a structurally
  different reimplementation (index-based rather than id-keyed, bool-flip rather than set-based).

The second of these is not nine dead-function tickets. It is one architectural question — is
`tiller_project::{Workspace, Project}` the intended selection model or not — and if the answer
is no, the type goes, not its methods.

### Two things noticed but NOT audited

Both surfaced as bycatch during the triage. They are recorded here so they are not lost, and
labelled so they are not mistaken for findings:

- the per-row Discard in the Changes panel appears to call the tracked-only `discard()`
  (`git restore --worktree`) even for `Untracked` rows, which would not remove them;
- `workspace.create` over the control socket appears to bypass the pinned
  `default_worktree_base` project setting, which only the GUI New Worktree dialog honours.

Neither has been driven or proven. They need their own look before anyone acts on them.

## A second caution about the instrument

The triage's refutation pass overturned **0 of 24** verdicts. That is not 24 independent
confirmations. Every investigator and every refuter reached the tree through the same
instrument — ripgrep over the same working copy — so a blind spot in that approach (a caller
reachable only through a trait object, a macro expansion, or a build script) would be invisible
to both passes identically, and would present exactly as unanimous agreement. Unanimity across
agents sharing one instrument bounds nothing.

For that reason the highest-stakes claim in this document — defect 1, the clone cancellation —
was re-derived by hand from the four files involved rather than accepted from the report, and
the citations here were opened before being written down. Two agent-supplied line numbers were
found to have drifted in the process, which is the expected rate rather than a scandal.

Related drift worth knowing when reading older rows: `INVENTORY-LEDGER.md`'s `F-CORE-DOM-05`
cites `Sidebar::reorder_rows` at `sidebar.rs:758`; it is at `sidebar.rs:771` today. The row is
not being edited — it was accurate on the date it was written, and rewriting dated records to
match the present falsifies them — but a reader following that citation should expect to search
rather than jump.

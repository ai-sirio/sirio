# Finding: ported model functions the app cannot reach

**Author:** lead, 2026-08-20. **Status:** a survey and a tool, not a verdict on any row.
**Reproduce:** `python3 Scripts/dead-model-audit.py`

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

## Open question for the user

Whether to work the remaining candidates. They are cheap to triage individually — the question
per symbol is only "does the app do this thing by another path, and can it be driven?" — but
there are ~14 unadjudicated ones across two crates, and none is currently blocking a row.

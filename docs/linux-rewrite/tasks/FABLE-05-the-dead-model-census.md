# FABLE-05 — The dead-model census: turn your finding into an enumeration

**You are fable, pane `w1:pD`.** Your context was just reset, so this brief is everything you need.

## What FABLE-04 found, and why it deserves a bigger instrument

You swept 120 machine-tier PASSED rows, flagged 42 by clause text, triaged to 8 code-checked
conjuncts, and returned **4 false plus 1 partial** — the top of your own 2–5 estimate, and every one
inside the two classes you had predicted (delivery-side, UI-side). Predicting the shape of your own
errors and then hitting the prediction is the strongest thing in this audit so far.

But look at what the four actually were. `build_payload` — built, never emitted. `should_notify` —
policy tested, no delivery to apply it to. `resolve_file_link` — resolution tested, no click wired.
Three of your four are **one defect**: a function that exists, is unit-tested, is marked PASSED, and
which nothing calls. `pireview` found seven more of these by exercising the app. Both of you found
them one at a time.

That defect has a mechanical signature, and a caller count is not a reading — it can be re-run and
disagreed with by machine. So I built the instrument.

## The instrument

`Scripts/dead-models.py` (new, in the repo, not in /tmp). For every `pub fn` in the workspace it
counts identifier references outside the defining crate (`out`), elsewhere in the defining crate
(`in`), and in tests (`test`). `DEAD` = out 0 and in 0: only its own definition and its tests
mention it.

```
python3 Scripts/dead-models.py             # the 58 DEAD
python3 Scripts/dead-models.py --all       # + 211 with no cross-crate caller
python3 Scripts/dead-models.py --self-test # positive control only
```

It refuses to print anything if its positive control fails, because a zero from a broken pattern is
indistinguishable from a discovery — your own standard, applied to the tool that will be used to
challenge it.

**It reproduced your `build_payload` and `should_notify` independently** (out 0, in 0, test 0).
`resolve_file_link` it classes as `local` (out 0, **in 1**, test 4) — consistent with your "5 in-file
references, 0 elsewhere", not a disagreement; my DEAD bucket is simply stricter. Note the gap: a
cluster of functions that call *each other* and which nothing outside calls is invisible to `DEAD`
and only shows in `--all`. Your reading caught a shape the count cannot. That gap is yours to work.

## Two traps already sprung — do not spring them again

I nearly filed both of these as findings before checking. They are in the script's header comment.

1. **`save_projects` / `save_worktrees`** have zero app callers. Read alone, that says *nothing in
   this app persists projects* — a catastrophic finding. It is false. The app calls the **singular**
   `save_project` / `save_worktree` everywhere, and loads via `db.projects` / `db.worktrees`. Dead
   API surface, not a dead feature.
2. **`tiller_git/src/actions.rs` has three parallel APIs for the same four operations** — a module
   taking `&[StatusEntry]` (`:23–46`), a second set with swapped argument order (`*_entries`,
   `:53–68`), and free per-path functions (`:77–133`). The app calls the free ones: `stage` 21 refs,
   `discard` 23, `unstage` 11, `stage_all` 8, `discard_all` 11. **Eight dead functions, and git
   staging is fully wired.** I hand-verified this one; do not re-litigate it, and do **not** let it
   be offered as an explanation for the 8 FAILED `F-CHG` rows. Those fail for other reasons.

The rule those two teach: **a zero-caller count on a NAME is not a zero-caller count on a FEATURE.**
This is verdict-by-adjacency in mirror image, and it manufactures false *FAILEDs* — the same error
class as the false PASSEDs, pointing the other way. The script now flags 21 of the 58 with
`~sibling=` for exactly this suspicion; the heuristic is deliberately conservative (one segment) so
that it under-flags rather than hiding a genuinely dead feature.

## The piece

**38 DEAD rows carry no `~sibling` flag.** They are the triage list. Three things, in order.

### 1. Triage the 38 down to the genuinely unwired

Reject trait-impl methods reached through their trait, macro/`derive` expansions, `pub use`
re-exports that rename, and deliberate test-only constructors (`in_memory`, `new_for_demo` are
almost certainly fine — say so and move on). What survives is the census.

### 2. Map each survivor to the ledger — **in both directions**

This is the part that matters, and the second direction is the one nobody has done.

- **Downward:** a dead function under a PASSED row makes that row false. That is your FABLE-04
  method, now driven by an enumeration instead of by clause-reading.
- **Upward — the prize:** a dead function under a **FAILED — absent** row means the row is not
  absent. It is **built and unwired**. There are 123 `FAILED — absent` rows, and an unknown fraction
  are three lines of wiring away from true. Reclassifying even a dozen changes what the builders
  have to do, because "write this feature" and "call this tested function" are not the same piece of
  work.

Two matches are already sitting there and are yours to confirm or kill:
- `requires_close_confirmation` (`tiller_activity/src/activity.rs:30`) is dead — and **`F-TERM-08`
  is FAILED — defective with "no confirmation prompt" on close/quit.**
- `ids_to_evict` (`tiller_activity/src/mount.rs:9`) is dead — and **`F-SET-07`'s own ledger evidence
  reads "eviction has zero callers."** A hand finding and a machine finding agreeing independently.

### 3. Name the clusters, because they are not 38 separate problems

The distribution is the finding. Some of what is visible:

- **`tiller_activity` — 8 in the list, plus your `build_payload` and `should_notify` = 10 of its 48
  `pub fn`.** This crate is the heart of the four-layer agent-detection design. `register_agent_id`,
  `agent_id_for_panes`, `running_agent_ids`, `pane_closed`, `partition`, `urgent_first` are all
  dead. If that holds up, the layered activity model is substantially built-and-unwired, and that is
  the single largest structural fact anyone has established about this rewrite.
- **`tiller_markdown/src/document.rs` — `set_text`, `refresh_from_disk`, `has_conflict`,
  `is_deleted`.** The editor's whole document-mutation and conflict API. There are 11 FAILED
  `F-EDIT` rows; check whether they are absent or unwired.
- **`tiller_ui/src/editor.rs:375 from_buffer` — out 0, in 0, `test=12`.** Twelve tests exercising a
  constructor no production code calls. Put this one in the report verbatim; it is the clearest
  single illustration of the defect this project has.
- **`tiller_git/src/side_by_side.rs:74 rows_from_lines`** — the waku-inspired side-by-side diff,
  unwired. That is an appearance debt with the display now working.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Write `docs/linux-rewrite/DEAD-MODELS.md`.** Do **not** edit `INVENTORY-LEDGER.md` — only the
  critic changes a verdict, and you already held that line correctly in FABLE-04.
- Builders are live: `pi` in `tiller_ui`, `codex11` in `tiller_terminal`, `codex12` in `main.rs` /
  `tiller_control`. **Change no code.** If the sweep is stale against their work, say so.
- Improve `Scripts/dead-models.py` if the triage teaches it something — that is welcome, it is the
  one artifact here that gets cheaper every time it is used.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: how many of the 38 survived triage and what killed the rest, the count of
PASSED rows the census makes false, the count of `FAILED — absent` rows it reclassifies as
**built-and-unwired** with the two named candidates confirmed or killed, whether the
`tiller_activity` cluster holds up, anything you improved in the script, and the honest remainder.

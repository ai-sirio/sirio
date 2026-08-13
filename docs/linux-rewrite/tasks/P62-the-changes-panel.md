# P62 — The Changes panel: a guard that is never rendered, and a fifth status that vanishes

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P61 landed, and it settled the question the census could only ask

The four functions really were unwired, `Editor` now uses `MarkdownDocument` in production, and the
census reports zero for that cluster. You proved it the honest way — `edit_save_and_reopen_keeps_the_change`
and `deleted_document_can_be_saved_again_after_external_removal` go to the filesystem and come back,
which is the only test shape that can distinguish "the buffer's flag flipped" from "bytes reached
disk".

You also made the conflict predicate **shared** between the banner and the save path, which was the
one design constraint in the brief that mattered: two paths that can disagree about conflict is how
"it warned me and then overwrote it anyway" gets built.

Three things from your report worth carrying forward:

- **⌘S was already wired.** So `F-EDIT-04` ("no ⌘S binding") was a **stale FAILED** — the ledger was
  wrong, not the code. Say that explicitly when it happens; a stale FAILED sends a builder to build
  something that exists, which is as expensive as a false PASSED.
- **`add_file_tab` dedupe is `main.rs`**, so you correctly left `F-EDIT-08` alone. It is queued for
  `codex12` in `docs/linux-rewrite/QUEUE.md`.
- The `tiller_theme` errors you hit were `sonnet` mid-edit; that crate builds clean now. Same for the
  `main.rs` format drift — `codex12` is live there.

## The piece: `F-CHG`, and the first row is a familiar shape

Eight `F-CHG` rows are FAILED, three more are blocked on display and belong to the critic. Take these
four.

### 1. `F-CHG-03` — a state that is computed and never drawn

The row reads: *"error state rendered + tested; no loading state (`refresh_started` is a reentrancy
guard, never rendered), no Refresh button, no Retry."*

You have now seen this exact shape twice in two pieces. `refresh_started` already knows a refresh is
in flight — it just never reaches a pixel. So the panel can be refreshing and look identical to
idle, and there is no way to ask it to try again after a failure.

Build the loading state, the Refresh button and the Retry on the error path. Make the loading state
read the **same** `refresh_started` the guard uses, not a second flag — a second flag is how the
spinner and the guard start disagreeing.

### 2. `F-CHG-11` — Unstage all, and section-level actions

Stage all and Discard all are real and drawn-green from pass 8. There is no Unstage all and no
section-level action. The underlying git operations already exist and are called: `tiller_git`'s free
per-path functions are live (`stage` 21 refs, `unstage` 11, `discard` 23, `stage_all` 8, `discard_all`
11). This is a UI gap over a working engine.

**A warning specific to this file.** `tiller_git/src/actions.rs` carries *three parallel APIs* for the
same four operations — a `&[StatusEntry]` module, an `*_entries` set with swapped argument order, and
the free per-path functions. **Eight of those are dead and the app uses the free ones.** Do not
"discover" the dead ones and wire them; use what the app already uses, and if you think one of the
dead shapes is better, say so rather than quietly introducing a fourth caller pattern.

### 3. `F-CHG-22` — the fifth status disappears at the UI boundary

The row: *"surface `ActivityStatus` models 4 of the 5 claimed statuses — NeedsInput absent from the
activity panel."*

This is not only a Changes-panel bug. `fable` logged the same thing from the other end during the
audit: `AgentStatus::NeedsInput` maps to `ActivityStatus::Idle` at `main.rs:3026`, and
`ActivityStatus` has no needs-input variant (`right_panel.rs:31–39` — its `Idle` doc-comment does say
"waiting for input", which is the tell that someone knew).

So an agent waiting for input is indistinguishable from an idle one, everywhere. Given that this app
exists to run several agents at once, "which one needs me" is close to its whole purpose.

Fix it on your side of the boundary — `ActivityStatus` and the panel. **If the mapping at
`main.rs:3026` must change, say so in your report and let `codex12` make it**; that file is theirs.

### 4. `F-CHG-05` — keyboard handling in `right_panel.rs`, if it is genuinely cheap

Take it if it fits the same machinery. Say so plainly if it does not.

### Leave these

`F-CHG-01` (Changes moved to a Diff tab — architectural, its own piece), `F-CHG-13` (Open-diff),
`F-CHG-16` (resolve-in-terminal), `F-CHG-18` (drag). And `F-CHG-02`/`06`/`20` are blocked on display
and belong to `pireview`, who now has a working display.

## One constraint that is new tonight

The UI is moving to the **Pop!_OS COSMIC** design language; `sonnet` is building the token layer in
`tiller_theme` (a `cosmic/` module now exists there). Take colours, spacing and radii from
`tiller_theme::Theme` — **never hardcode a literal.** Status colours matter here: a loading state and
a needs-input status both want semantic colours, and COSMIC defines `warning`, `success`,
`destructive` and `accent` precisely so you do not invent them. If the token you need is not there
yet, use the nearest and name the gap.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For the loading state, the test that matters asserts the panel looks **different** while a refresh is
in flight — a test that only checks the flag is set proves nothing about the defect, which is that
the flag never reaches a pixel. For Retry, invoke it and assert the refetch happened. For
needs-input, assert it survives the whole trip from `AgentStatus` to the drawn panel; that is where
it currently dies.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_ui/src/changes.rs`, `tiller_ui/src/right_panel.rs`, and `tiller_git/**`.**
- **Do not edit** `tiller_ui/src/settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`),
  `tab_bar.rs` and `tiller/src/main.rs` and `tiller_control/**` (`codex12`), `tiller_theme/**`
  (`sonnet`).
- The gate is `Scripts/ci-linux.sh`. Expect red from other people's live edits — **name those
  separately**, as you did in P61.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the loading state and whether it shares `refresh_started`, Refresh and Retry,
Unstage all and which `tiller_git` API you called, whether needs-input now survives to the panel and
whether `main.rs:3026` needs `codex12`, whether keyboard came cheap, any token `tiller_theme` lacks,
tests by name, the gate with not-yours failures named separately, and the honest remainder.

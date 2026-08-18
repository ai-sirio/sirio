# FINISH — changes-git part 2 (F-CHG-02/03/09/14/15/18/21, F-GIT-RUN-01)

Fresh finish-line critic pass, lane `wf-chg`, run **2026-08-18** via `Scripts/wayland-drive.sh`
(Wayland lane, sway/wayland-4, `TILLER_WL_BIN=/tmp/wf-chg-tiller` pinned so sibling agents rebuilding
`rust/target` couldn't swap the binary mid-pass). Scope: every `F-CHG-*`/`F-GIT-RUN-*` ledger row
NOT marked PASSED — 8 rows total. Every row was driven live this pass; the four rows carrying
wave-D "builder claims" (F-CHG-02/03/09/15, commits `ba8d46c7`/`921d9ef3`) were treated as
hypotheses and re-reproduced from scratch rather than taken on trust.

## Setup

Throwaway fixtures under `/tmp` (not committed — ephemeral scratch; behaviour is described in full
below so the row is reproducible without them):

- `/tmp/wfchg-repo` — dirty worktree: staged rename, staged file, changed file, untracked file,
  binary `image.bin`.
- `/tmp/wfchg-err` — committed repo added to the catalog *before* any fault, then `.git` alone
  toggled `chmod 000`/`755` live (isolates the Changes tab's own git-status failure from the Files
  panel, which never touches `.git`).
- `/tmp/wfchg-err2` — committed repo, worktree **root** (not `.git`) toggled `chmod 000`/`755`
  live (isolates the Files panel's own root-traversal failure).
- `/tmp/wfchg-clean`, `/tmp/wfchg-conflict` — clean and real-merge-conflict fixtures, unused by the
  8 rows below in the end but kept ready.

`Scripts/wayland-drive.sh` kills and relaunches both the app and the nested compositor
**unconditionally at the top of every invocation**, regardless of `TILLER_WL_KEEP` — confirmed by
reading the script (`kill_ours TILLER_SOCKET "$SOCK" tiller` / `kill_ours SWAYSOCK ...` run before
the `TILLER_WL_KEEP` check is ever consulted). `TILLER_WL_KEEP=1` only skips the kill at *cleanup*
(EXIT), so the app is reachable via a plain socket `ctl` call between invocations, but any UI state
(open tabs, dialogs, expanded sections) does **not** survive to the next invocation — only sqlite
persistence and the real filesystem do. Every multi-step gesture (dialog open→click, chmod→Retry
click→verify) was therefore scripted inside **one** `wayland-drive.sh` invocation's action block.

---

## Rows

**F-CHG-02 — no-worktree-selected state in the right panel.** **FAILED — defective.** The wave-D
ledger entry claims this was fixed by `ba8d46c7`; re-driven today on current `HEAD` and it still
reproduces. True-empty-catalog boot (`ctl project.list` → `[]`, `ctl workspace.list` → `[]`, `ctl
workspace.current` → error, all three independently, before any screenshot): main pane correctly
shows "No worktree selected / Add a project, then select a worktree", the Projects sidebar is
genuinely empty (no entries at all) — yet the **Files** panel on the right and the status bar
simultaneously show a live, unrelated, real directory tree (`/home/enzopalmisano/Scrivania/Progetti/
tiller-linux`, this very repo, seeded from `working_directory`/`fallback_directory`, independent of
the empty project catalog) instead of its own "no worktree selected" placeholder. Live frame:
`/tmp/wf-chg-shots/02-freshboot/03-fresh2.png` — one screenshot shows all three facts at once
(empty sidebar, "No worktree selected" in the main pane, real unrelated file tree + `linux/gpui-
waku · ~/Scrivania/Progetti/tiller-linux` in the Files panel/status bar). This directly falsifies
the row's own VERIFY clause ("open the right panel, and confirm it explains that no worktree is
selected") — the right panel does not explain anything; it shows unrelated live content. This
independently corroborates `FINISH-changes-git.md`'s own F-CHG-02 finding (`right-panel-no-
worktree` placeholder exists in code at `right_panel.rs:1101` but the Files panel never switches to
it), so the wave-D "half-proven, mostly fixed" framing is superseded: on this branch/commit the
defect is still live and directly reproducible in a single frame.

**F-CHG-03 — Files loading/error/Retry states.** **PASSED.** Root cause is distinct from F-CHG-09
(confirmed this pass): F-CHG-03 is the Files panel's *own* root-traversal error (`read_tree`
explicitly skips `.git`, so this needs `chmod 000` on the **worktree root**, not `.git`) — a
distinction the wave-D evidence conflated (its F-CHG-03 citation actually only exercised F-CHG-09's
mechanism). Live round trip, all in **one continuous** `wayland-drive.sh` session (fresh boot with
`/tmp/wfchg-err2` already `chmod 000`): frame `02-error-fresh-boot.png` shows "Files unavailable:
Permission denied (os error 13)" + a "Retry" link, main pane unaffected (still shows "No
Terminals"). Mid-session `chmod 755 /tmp/wfchg-err2` then a click on Retry's live-observed
coordinates recovers the tree to its normal `a.txt` listing — frame
`03-after-retry-same-session.png`. Both frames:
`/tmp/wf-chg-shots/49-chg03-round-trip/02-error-fresh-boot.png`,
`/tmp/wf-chg-shots/49-chg03-round-trip/03-after-retry-same-session.png`.

**F-CHG-09 — Changes loading/Git-status-error/Retry states.** **PASSED.** Isolated from F-CHG-03 by
construction: `/tmp/wfchg-err` was `project.add`-ed and `workspace.select`-ed while `.git` was still
readable (confirmed via `ctl project.add` → `{"added":"true", ...}`, persisted to sqlite), *then*
`.git` alone (not the worktree root) was `chmod 000`'d on disk, then the app was rebooted fresh. The
degraded-catalog fix (`ba8d46c7`) correctly kept the project/worktree selectable despite the git
fault (`ctl workspace.select` still succeeds). Opening the Changes tab
(`ctl surface.changes.open`) renders "Git is unavailable: git exited with status 128: fatal: .git
non è un repository Git (né lo è alcuna delle directory genitrici)" + a "Retry" link in the main
Changes tab — while the **Files** panel on the same frame correctly keeps showing `a.txt` normally,
proving the two rows' error surfaces are genuinely independent. Frame:
`/tmp/wf-chg-shots/52-chg09-error/02-changes-error-fresh.png`. Full round trip re-driven in one
session for the report (`/tmp/wf-chg-shots/53-chg09-roundtrip/`): `02-01-error-fresh.png` shows the
same error+Retry state; mid-session `chmod 755 /tmp/wfchg-err/.git` + a click on Retry recovers to
"Local changes (0)" / "No changes" — `03-02-after-retry.png` — and the control-socket read
(`ctl surface.changes.read`) taken immediately after the click confirms `"error":""` /
`"ready":"true"` server-side, not just visually.

**F-CHG-14 — discard an individual file after confirmation.** **PASSED.** The wave-A shard critic's
"no visible confirm/cancel affordance under synthetic Wayland clicks" finding does not reproduce:
GPUI's Wayland platform `prompt()` always returns `None` (confirmed in
`rust/vendor/gpui_linux/src/linux/wayland/window.rs:1561-1569`), which is the documented trigger for
GPUI's own in-window `FallbackPromptRenderer` — a real drawn modal, not a native one, and easy to
miss if the frame isn't inspected closely. Live: right-clicking a changed text file → Discard opens
a modal titled "Discard changes?" with white background, "Discard" and "Cancel" buttons, both
visible and hit-testable (`/tmp/wf-chg-shots/20-full-cycle/03-dialog-open.png`); clicking "Discard"
mutates the real worktree — `Local changes (5)` → `(4)`, `file.txt` disappears from the Changed
section (`04-after-discard-confirmed.png`). Separately, on a binary file (`image.bin`), the same
dialog appears and clicking **Cancel** leaves the file untouched — still listed under `Changed (1)`
afterward (`/tmp/wf-chg-shots/21-cancel-then-discard/04-after-cancel.png`), proving Cancel is a real
no-op, not a silent confirm. Both are full git-state-verified round trips (row count and section
membership checked before/after, not just "a dialog appeared").

**F-CHG-15 — binary-file and diff-load-unavailable states.** **half-proven.** Binary half:
**live-reproduced and fix-confirmed this pass** — `/tmp/wf-chg-shots/21-cancel-then-discard/
04-after-cancel.png` shows `image.bin`'s expanded row rendering "Binary diff unavailable" inline
(the `921d9ef3` fix: `changes.rs`'s `expand_diff` checks `diff.is_binary` before falling through to
the empty hunk loop). Diff-load-fail half: **not independently live-reproduced this pass** — two
cheap attempts (a self-referential symlink; a FIFO, hoping to trip the per-file git timeout while
`git status`/`stats` still succeed) both failed to isolate a per-file `diff_entry()` failure without
also failing the whole-snapshot `status()`/`stats()` call, which would collapse into F-CHG-09's
mechanism instead. Resting this half on: (1) `expand_diff`'s `diff_errors` branch
(`changes.rs:854-866`) is the same production code called from the real render pipeline
(`changes.rs:829`), not test-only scaffolding; (2) the regression test
`changes::tests::an_expanded_file_whose_diff_failed_says_unavailable` re-run fresh this pass, green
(`cargo test -p tiller_ui --lib changes::tests::an_expanded_file_whose_diff_failed_says_unavailable
-- --exact` → 1 passed). This is weaker evidence than the binary half and than a live UI
reproduction would be — flagged rather than silently upgraded to PASSED.

**F-CHG-18 — drag a changed-file diff into a pane.** **FAILED — defective.** The row's VERIFY
clause is "drag a changed-file row into a terminal or chat pane and confirm the pane receives the
diff payload" — the second half is categorically unmet regardless of drag mechanics, confirmed by a
fresh three-way grep cross-check this pass:
- Positive control (pattern is alive): `TerminalDropEvent` is real — emitted at
  `tiller_terminal/src/lib.rs:962`/`1002`, and the **only** production `on_drop::<(PathBuf,
  String)>` registration anywhere in the app is `tiller_terminal/src/lib.rs:1843` (the one instance
  in `tiller_ui/src/changes.rs:3287` is inside `#[cfg(test)] mod tests`, self-documented in a
  comment as "a drop-target fixture standing in for a pane's real ... handler").
- Negative search (validated, not assumed): `grep -rn "TerminalDropEvent\|last_dropped_diff"
  rust/crates/tiller/src rust/crates/tiller_ui/src` → **zero matches**, confirmed today. The only
  subscribers to `TerminalDropEvent` anywhere in the workspace are in `tiller_terminal/src/lib.rs`'s
  own test module (lines 3732/3876/3973).
- Chat panes were checked separately in case they use a different consumption path: `chat.rs` only
  handles GPUI's built-in `FileDropEvent` (OS-level external file drops, e.g. from a file manager),
  a completely different mechanism from the in-app diff-drag payload `changes.rs` produces. There is
  no consumer of the diff payload for either destination the row names.

So the drag *source* (`changes.rs`'s `drag_payload`) and the drop *target* (`tiller_terminal`'s
`on_drop` handler, which receives the payload and re-emits `TerminalDropEvent`) both exist and are
real, but the event is emitted into a void — nothing in the app crates renders, stores, or acts on
it. A pane can never visibly "receive the diff payload" no matter how precisely the drag lands. This
is decisive independent of the gesture-timing question: my own attempt to land the physical
rightclick→"Move to New Pane"→drag sequence in one invocation did not produce a visible split this
pass either (consistent with three prior passes' documented difficulty with this exact gesture), but
that uncertainty is now moot — the code-level finding alone proves the row's VERIFY clause cannot be
satisfied as written.

**F-CHG-21 — Activity row focus and close.** **PASSED.** Both halves re-driven live and both work
correctly at precise click coordinates, contradicting the wave-A shard critic's "3 attempts each
collapsed the whole Activity section" finding, which is very likely their own coordinate imprecision
rather than a real defect:
- Focus half: clicking an Activity row switches the active tab in both the strip and the sidebar
  (spot-checked in an earlier drive this pass, underline moved to the clicked entry).
- Close half: with 4 Activity rows expanded (3× "Changes" + 1× "Terminal",
  `/tmp/wf-chg-shots/43-close-precise/02-expanded-precise.png`), clicking one row's × closes only
  that row — 4 rows → 3 (`03-after-close-precise.png`), the other three rows and their tabs
  unaffected, the Activity section stays **expanded** (not collapsed), and the tab strip correctly
  drops from 4 tabs to 3 in lockstep. `CloseActivity(index)` is wired to a real
  `request_close_activity` call in `main.rs`, and this pass shows it firing correctly for a single
  row when the click lands where the row actually is (the section anchors `.absolute().bottom_0()`
  and grows upward with row count, so a stale-frame coordinate reliably misses and looks like "the
  whole section collapsed" when what actually happened is the click landed on the row *above* or
  outside any row entirely).

**F-GIT-RUN-01 — GitRunner: success/failure/missing/cancelled/output-flooding.**
**half-proven.** Re-confirmed today, unchanged from the ledger's characterization:
- `cargo test -p tiller_git` → **64/64 green** today (25+13+9+6+11+0 doctests across the crate's
  test binaries), covering success, `CommandFailed`, `TimedOut` (a fake sleeping `git` on `PATH`),
  and spawn/launch failure.
- Cancellation: validated negative grep. Positive control first (pattern is alive elsewhere):
  `grep -rn "cancel\|Cancel" rust/crates/tiller_ui/src/changes.rs` → 5 matches (the discard-dialog's
  own Cancel button/tests). Then the real search: `grep -rn "cancel\|Cancel"
  rust/crates/tiller_git/src` → **zero matches**, confirmed today across all 11 source files in the
  crate (`actions.rs`, `branches.rs`, `clone.rs`, `diff.rs`, `directory_status.rs`, `error.rs`,
  `git.rs`, `lib.rs`, `remote.rs`, `side_by_side.rs`, `status.rs`). No cancellation mechanism exists
  at the git-running layer.
- Output-limit/truncation: `git.rs`'s `read_to_end` (line 451) calls
  `std::io::Read::read_to_end(&mut pipe, &mut buffer)` with **no byte cap** — it reads to EOF
  unconditionally. The only bound on a runaway git process is the wall-clock timeout
  (`DEFAULT_GIT_TIMEOUT = Duration::from_secs(10)`, overridable via env var), not an output-size
  limit — confirmed by reading the full function and its call sites; no `MAX_`/`Limit` constant
  exists anywhere in the file. This is a real gap against the Swift original's `GitOutputLimits`
  (`GitRunner.swift:20-168`), never ported.
- Reachability confirmed, not just testedness: `clone_repository` (the function whose caller-side
  comment documents the missing-cancellation gap) has a real caller —
  `tiller_ui/src/project_forms.rs:179` — and that same file's `CloneFormState::set_url` doc comment
  (line 38, re-read and re-quoted verbatim this pass) says outright: "Editing after a failure
  returns the form to the ready state; editing during a clone cannot cancel that clone." No Cancel
  affordance exists anywhere in `project_forms.rs`'s render code (grepped, zero matches beyond that
  one comment) — the gap is real and reachable through the actual clone-a-repo UI flow, not a
  theoretical internal-API limitation.

---

## Summary

| Row | Verdict |
|---|---|
| F-CHG-02 | FAILED — defective |
| F-CHG-03 | PASSED |
| F-CHG-09 | PASSED |
| F-CHG-14 | PASSED |
| F-CHG-15 | half-proven |
| F-CHG-18 | FAILED — defective |
| F-CHG-21 | PASSED |
| F-GIT-RUN-01 | half-proven |

5 of 8 rows resolve this pass (3 flip from the ledger's "half-proven, mostly fixed" framing to a
clean PASSED with a live, git-state-verified round trip; F-CHG-02 flips the other way, from
"half-proven/fixed" to a directly reproduced FAILED — defective, contradicting the wave-D builder
claim on current `HEAD`). F-CHG-18 is elevated from "half-proven" to a decisive FAILED — defective
on the strength of a validated zero-subscriber grep, independent of the drag gesture's live
reliability. F-CHG-15 and F-GIT-RUN-01 remain half-proven — genuinely mixed rows where part of the
VERIFY clause is proven live/by fresh test run and part is confirmed absent by code census, not
gaps in this pass's effort.

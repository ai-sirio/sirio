# FINISH — changes-git shard (F-CHG-*, F-GIT-*)

Fresh finish-line critic pass, run **2026-08-18** on the box described in `ENVIRONMENT.md`'s
2026-08-18 top section (x86_64, 12 cores, COSMIC/wayland-1, AMD GPU). I built none of this. Every
row below was re-driven today; nothing was carried from the ledger's prose.

## Setup

```bash
export TILLER_WL_LABEL=fin-chgit
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm
cp rust/target/debug/tiller /tmp/fin-chgit-tiller
export TILLER_WL_BIN=/tmp/fin-chgit-tiller
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
```

Throwaway git fixtures built from scratch for this pass:

- `/tmp/fin-chgit-repo` — dirty worktree: `README.md`→`README2.md` (staged rename), `file.txt`
  (staged, then re-modified to prove Changed vs Staged split), `staged.txt` (staged new file),
  `untracked.txt` / `untracked2.txt` (untracked), `image.bin` (binary, later dirtied),
  `sub/deep/tracked.txt` (clean, two levels deep, for the Files-tree expand test).
- `/tmp/fin-chgit-conflict` — real `git merge` conflict on `conflict.txt` (two branches editing the
  same line).
- `/tmp/fin-chgit-clean` — single committed file, zero dirty state.
- `/tmp/fin-chgit-err` — committed repo whose `.git` directory gets `chmod 000`/`755` toggled live.

Every drive command below is `Scripts/wayland-drive.sh /tmp/fin-chgit-shots/<NN-name> '<actions>' 20`.
Capture paths referenced below are under `/tmp/fin-chgit-shots/` (not committed — ephemeral
per-session scratch, per the task's "keep your return value small" instruction; the frames that
matter are described in enough pixel/behavioural detail to reproduce).

Two real lane hazards hit repeatedly and are not app defects, matching prior sweeps' findings:
(1) the **first synthetic click after a compositor/pointer reconnect is frequently dropped** — every
click-driven mutation needed a second identical click in the same invocation to land; (2) a `git
status --short` check right after a click sometimes needed a settle frame first. Both are noted
inline only where they change how a result should be read.

---

## F-CHG rows

**F-CHG-01 — Files/Changes as peer tabs.** PASSED. Live: `surface.changes.open` (and clicking the
Changes entry from the `+` menu) opens **Changes** as a tab next to **Chat**/**Terminal** in the
same strip, distinct from the always-present **Files** panel on the right. `right_panel.rs` was
re-grepped: the Files header is hardcoded, no Files/Changes mode switch exists — the architecture
pivot to a peer-tab model (recorded by a prior pass) still holds today.

**F-CHG-02 — no-worktree-selected message.** **FAILED — defective.** Reproduced **three times**
today, twice deliberately isolated: `project.add`-ing a project whose `.git` had just had its
permissions restored, with zero clicks in between, left the **main tab area** showing "No worktree
selected / Add a project, then select a worktree" while the **Files (right) panel** kept showing
the **previous worktree's stale file listing** — never its own dedicated `right-panel-no-worktree`
placeholder (confirmed present in code at `right_panel.rs:1101`, gated on `self.worktree_selected`).
Across every capture where the main pane showed the no-worktree state
(`/tmp/fin-chgit-shots/00-explore/02-initial.png`, `36-err7/02-y1-immediately-after-break-no-click.png`,
`34-err5/02-z1-recheck.png`), the Files panel never once switched to that placeholder — it silently
kept rendering the last successfully-loaded tree. The clause requires the **right panel** itself to
explain no worktree is selected; it does not.

**F-CHG-03 — Files loading/error/Retry.** **FAILED — defective**, contradicting the ledger's prior
PASSED. Reproduced identically **twice**, cleanly isolated (`chmod 000 .git`, zero clicks, one
`shot`): the whole **project silently vanished from the sidebar** within about a second (the app's
own background probe) and the main pane reverted to "No worktree selected." No "Files unavailable:
Permission denied" message ever appeared — the Files panel just kept showing the last-good listing
until the project disappeared entirely. `chmod 755` restoring permissions self-healed everything
(project reappeared, Changes/Files repopulated correctly) — that half of the old evidence still
holds. `/tmp/fin-chgit.log` shows no error/panic logged for the permission failure at all. Frames:
`36-err7/02-*`, `35-err6/02-*` (recovery), `33-err4/02-*` (first repro, via clicking Files' own
Refresh button — same result). This is a real behavioural regression from what the clause and the
prior evidence describe, measured fresh on today's host.

**F-CHG-04 — expand/collapse + select.** PASSED. Live: clicking the `sub` directory row in Files
reveals `deep` (chevron flips down); clicking again collapses it (chevron back to right, `deep`
gone); clicking a third time re-expands. `17-files/02-04.png`.

**F-CHG-05 — keyboard nav.** PASSED. Live, discriminated by sampled pixel colour (not just a
screenshot glance) at three fixed points while the mouse cursor stayed put, so hover and selection
highlight could be told apart: clicking `file.txt` selects it (`#262626` vs `#1A1A1A` background);
`Down` moved the highlight to `image.bin`; a second `Down` moved it to `README2.md`; `Space` on that
non-directory row correctly did nothing (matches `on_file_key`'s `"space"` arm, which only acts on
directories). A second run: clicking `sub` and pressing `Space` collapsed it (proving directory
toggle-by-space); `Down`, `Down`, `Return` navigated to `image.bin` and **opened it** as a new editor
tab showing "This file appears to be binary and cannot be shown as text." — Return-opens-file
confirmed live. `18-keys/*`, `19-keys2/*`.

**F-CHG-06 — status colours.** PASSED. Live: `README2.md` (staged rename, orange/amber icon),
`staged.txt` (staged, green), `file.txt` (changed, amber), `untracked.txt`/`untracked2.txt`
(untracked, blue) all rendered distinct icon colours matching the section they're in, in both the
Changes list and the Files-tree dots. `01-select/03-after-changes-open.png`.

**F-CHG-07 — clean state.** PASSED. Live: opening Changes on `/tmp/fin-chgit-clean` (zero dirty
files) rendered a centred "No changes" message with "Local changes (0)" in the toolbar.
`29-clean3/03-cc2-changes-tab-clean.png`. Note: Changes has no explicit "Refresh" button (only
Files does) — it refreshes automatically via a file watcher, which every stage/unstage/discard test
below implicitly re-confirms (state always updated with no manual refresh click).

**F-CHG-08 — sections + counts.** PASSED. Live, repeatedly: Staged/Changed/Untracked headers with
exact counts tracked every mutation correctly (`(2)`→`(3)`→`(5)` etc.) across a dozen captures.

**F-CHG-09 — Changes error/Retry.** **FAILED — defective**, same root cause as F-CHG-03: when
`.git` becomes permission-denied the whole worktree/project disappears from the UI before the
Changes surface's own `"Git is unavailable: {error}"` + Retry panel (confirmed present in code,
`changes.rs:1817`, id `changes-retry`) ever gets a chance to render. I never once saw that panel
live, despite two clean isolated reproductions of the permission failure.

**F-CHG-10 — stage/unstage one file.** PASSED. Live, both directions, git-verified:
`git status --short` before/after each click round-trip matched exactly — staging `file.txt` moved
it from Changed to Staged (`M ` in git's index column); unstaging `staged.txt` moved it back to
Untracked (`??`). `04-stage`, `05-stage2`, `06-unstage`, `07-unstage2`.

**F-CHG-11 — bulk stage/unstage/discard.** PASSED. Live, three separate trials, git-verified:
toolbar **Stage all** staged every one of 5 files (`git status` all `A`/`R`/`M` in the index
column); toolbar **Discard all** cleared Staged+Changed but left Untracked intact (matches
`git restore --worktree` semantics exactly); section-header **Unstage all** on the Staged section
correctly unstaged all 5 entries back to Changed/Untracked in one click (`git status` confirmed
`README.md`/`README2.md` rename un-did, `file.txt` unstaged, three files back to `??`).
`13-bulk`, `14/15/16-discardall*`, `58-secaction2`.

**F-CHG-12 — expand shows diff.** PASSED. Live: clicking `file.txt`'s row once expanded it in
place with a real hunk (`@@ -1 +1,2 @@`, numbered old/new lines, `+changed line2` in green);
clicking again collapsed it. `03-expand/02-expanded-file.png`.

**F-CHG-13 — Open file / Open diff.** PASSED. Live, both halves, discriminating against the wrong
thing on purpose: the **↗ Open file** icon opened a genuine new editor tab titled `README2.md`
showing real file content (`line1`) with a Markdown Preview/Code toggle — read the ported source
(`changes.rs:100-121`) first, which documents that this build intentionally routes **Open diff** to
*another Changes-surface tab* (`add_changes_tab(Some(path))`), pre-expanded on that file, rather
than to a dedicated single-file diff view like the Swift original — and live-reproduced exactly
that: each click on **Open diff** opened one more tab titled "Changes" (chevron/kind `Diff`
internally), with the target file already expanded inside it. Matches the clause's "confirm the
editor or diff tab opens" — a tab genuinely opens each time, it is just not file-scoped.
`11-opendiff2`, `12-openfile/03-*`.

**F-CHG-14 — discard with confirmation.** half-proven. The stage/unstage/discard **mutation** logic
is proven live throughout this pass. The **confirmation dialog itself** (`window.prompt`, a native
platform alert, `changes.rs:612/638`) is not a GPUI surface — no amount of synthetic Wayland click
at any coordinate produced a visible confirm/cancel affordance to click (tried repeatedly on both
per-row Discard and the toolbar Discard all; hover-highlighted the button correctly, and the
underlying git call sometimes fired anyway on a later click without any visible confirm step,
consistent with the app either not blocking on the native prompt in this headless compositor or the
prompt being invisible/no-op here). Re-ran `cargo test -p tiller_ui --lib changes::` today (30
tests, all green, including `drawn_discard_button_requires_confirmation_then_mutates_git` and
`drawn_discard_all_button_requires_confirmation_and_clears_worktree`) — that is real, fresh evidence
for the confirm-then-mutate contract, but it is a `cx`-level (non-visual) proof, not a wayland
gesture. Gesture half unreachable this lane; state-machine half freshly proven.

**F-CHG-15 — binary / diff-unavailable states.** half-proven, and the "binary" half is proven
**absent**, not just unproven. Live: dirtied `image.bin`, opened Changes, used the toolbar's
**Expand All** (individual click on the binary row's own chevron also worked once out of several
attempts) — the row shows the `·`/`·` stat marker but its expanded body renders **nothing at all**:
no "binary" text, no icon, just empty space, while every other expanded row on the same screen shows
real diff lines (`22-expandall/03-ex2.png`). Confirmed by reading the source: `expand_diff`
(`changes.rs:846-870`) has exactly one message branch — `diff_errors` (load failure) — and no branch
for `stat.is_binary`; for a successfully-loaded binary diff the hunk loop simply produces zero rows.
The only "This file appears to be binary…" string in the whole crate lives in `editor.rs:508`, the
**separate** file-viewer surface (confirmed live via Return-opening `image.bin`, F-CHG-05), not in
`ChangedFileRow`'s inline expansion as the clause (SRC `ChangedFileRow.swift`) requires. The
diff-unavailable half (`ChangeRow::Unavailable`, "diff unavailable: …" + the broken-repo Retry
panel) is real in code and covered today by `an_expanded_file_whose_diff_failed_says_unavailable`
and `the_error_state_renders_and_retry_is_clickable` (both green just now) — that half stands.

**F-CHG-16 — resolve conflict in terminal.** PASSED. Live, built a genuine `UU` merge conflict from
scratch: the conflicted row shows real conflict-marker diff lines (`<<<<<<< HEAD`, `=======`,
`>>>>>>> left`) plus a **Resolve in terminal** action only on conflicted rows. Clicking it opened
and focused a new "Resolve conflict.txt" terminal tab showing real `diff --cc conflict.txt` output
and a live shell prompt. `25-resolve`, `26-resolve2/03-s2.png`.

**F-CHG-17 — numbered lines + hunk headers.** PASSED — same evidence as F-CHG-12: `@@ -1 +1,2 @@`
header, `1`/`2` old/new line numbers rendered and legible.

**F-CHG-18 — drag diff into a pane.** half-proven; inconclusive this pass, not a confirmed defect.
Reached the documented mechanism live: right-clicked the Terminal tab, **Move to New Pane** produced
a genuine split (Changes list left, Terminal pane right, both visible at once —
`46-split3/03-k2-after-move-to-new-pane.png`). The subsequent `drag` gesture from the `image.bin`
row onto the visible terminal pane did not land cleanly: the layout reverted to a single full-width
Terminal tab immediately after, with no "Dropped diff" pill/breadcrumb marker anywhere (contrast
with the prior pass's `SCRATCH_DRAG_TEST.md` pill). Given this lane's own documented history of
drag/rightclick-then-click races needing an inserted settle frame, and that I could not isolate
whether my drag coordinates crossed the pane divider mid-gesture, I am not reporting this as a
defect — only as not newly proven today. `42-drag3`, `47-drag5`.

**F-CHG-19 — Activity section.** PASSED. Live, repeated cleanly across five separate expansions:
clicking the "Activity" header in the Files panel toggles its chevron and reveals rows for every
open tab of the current worktree (3× Changes, Resolve conflict.txt, Terminal — each with its own
icon and a `fin-chgit-repo/master` subtitle and a close `×`), and collapses again on a second click.
Single-open-worktree model caveat still applies (same as the ledger already accepts for this row):
"every open worktree" trivially means "the one open worktree." `49-activity`, `51-activity3`,
`55-activity7`.

**F-CHG-20 — No activity / running count.** PASSED. `cargo test -p tiller_ui --lib` today:
`right_panel::tests::activity_section_states_no_activity_when_empty` and
`activity_section_states_the_running_count` both green, fresh, this host. Combined with F-CHG-19's
live confirmation that the section renders and reacts to real state.

**F-CHG-21 — focus + close from Activity.** half-proven. **Focus half PASSED live**: clicking the
"Resolve conflict.txt" activity row switched the active tab in the strip and the sidebar to that
tab (`53-activity5/03-ad1-clicked-row-body.png` — orange underline moved, confirming the click
routed through `SelectActivity`). **Close half inconclusive**: three separate attempts at the `×`
icon on different rows (with settle frames and hover-verification first) each time collapsed the
whole Activity section rather than visibly closing one specific tab, and none left me able to
confirm a tab actually closed via the ×. `CloseActivity(index)` is wired to a real
`request_close_activity` call in `main.rs` (not dead code), so I found no evidence of a defect —
only that I could not pin the click precisely enough in this lane today to get a clean before/after.

**F-CHG-22 — status glyphs.** PASSED. `cargo test -p tiller_ui --lib` today:
`activity_section_draws_needs_input_as_distinct_from_idle` green, fresh, this host, plus the
running/idle/error states implicit in F-CHG-19's live rows (Terminal glyph vs. Changes glyph vs.
the conflict-file terminal's own icon, all visually distinct).

---

## F-GIT rows — `cargo test -p tiller_git --manifest-path rust/Cargo.toml`, run today

```
unittests src/lib.rs           : 25 passed
tests/git_integration.rs       : 13 passed
tests/p41_git_behaviors.rs     :  9 passed
tests/p99_git_rows.rs          :  6 passed
tests/worktree_integration.rs  : 11 passed
                                  ── 64 passed, 0 failed ──
```

All against the real `/usr/bin/git` on this Linux box (`GIT_BINARY = "git"`, resolved via `PATH`,
`git.rs:48`), in freshly created temp repos, today.

**F-GIT-RUN-01 — GitRunner.** half-proven, contradicting the ledger's flat PASSED. Success,
command-failure (`CommandFailed`), timeout (`git_timeout_fires`, a fake sleeping `git` re-execed
onto `PATH`), and launch-failure (`GitError::Spawn`, exercised by the same timeout-kill code path
when `spawn()`/`try_wait()` errors) are all real and covered. **Output-limit enforcement and
cancellation are absent from the crate, by census, not by absence-of-test**: `grep -rn
"cancel\|Cancel"` across every file in `rust/crates/tiller_git/src` returns nothing; there is no
byte-cap or truncation logic anywhere in `git.rs`'s streaming runner (only a wall-clock timeout).
Confirmed from the consumer side too — `tiller_ui/src/project_forms.rs:38`'s own comment states
outright: "editing during a clone cannot cancel that clone." The ledger's own pass-5 evidence text
("no output limits/cancellation") already knew this and still marked the row PASSED against a
clause that explicitly requires both; that verdict does not survive being checked against the
VERIFY clause rather than the prose.

**F-GIT-RUN-02** — PASSED. `streaming_lines_arrive_incrementally_before_completion` and
`a_real_git_clone_streams_progress_and_completes` both green today.

**F-GIT-REPO-01** — PASSED. `non_git_directory_is_refused`, worktree/unborn-HEAD cases in
`worktree_integration.rs` all green.

**F-GIT-BRANCH-01** — PASSED. `branch_listing_returns_exact_names_and_git_refuses_spaced_names`,
`branch_listing_preserves_spaces_in_names` green.

**F-GIT-WT-01** — PASSED. All 11 tests in `worktree_integration.rs` green (create with/without
base, duplicate refusal, dirty-removal refusal, slash/unicode/space branch names, porcelain
round-trip).

**F-GIT-CLONE-01** — PASSED. `a_real_git_clone_streams_progress_and_completes`,
`clone_from_local_repository_reports_receiving_progress`,
`clone_from_file_url_creates_the_requested_destination` all green.

**F-GIT-REMOTE-01** — PASSED. `remote_parsing_supports_github_ssh_https_and_project_suffixes`
green.

**F-GIT-STATUS-01** — PASSED. `status.rs` unit tests (modified/untracked/rename/unmerged/malformed/
non-ASCII paths) all green, plus `status_and_diff_handle_spaces_and_non_ascii_paths`,
`parse_status_round_trips_real_porcelain_output`.

**F-GIT-STATUS-02** — PASSED. `directory_statuses_mark_every_ancestor_with_precedence_and_both_
rename_sides` and `for_entry_classifies_leaves_exactly_as_it_marks_their_ancestors` green, plus my
own live conflict-repo drive today independently showed the correct precedence (a `UU` file
appearing in both Staged and Changed sections with the conflict-distinct icon colour).

**F-GIT-ACT-01** — PASSED. `actions_stage_unstage_and_discard_single_paths`,
`actions_stage_all_and_discard_all` green, and independently reproduced live throughout the F-CHG
section above (every stage/unstage/discard-all click was git-verified).

**F-GIT-ACT-02** — PASSED. `stage_refuses_a_conflicted_path_without_changing_git`,
`stage_all_refuses_every_conflicted_path_before_mutation`,
`mutation_validation_rejects_stale_and_duplicate_entries` green.

**F-GIT-DIFF-01** — PASSED. `git_integration.rs`'s rename/staged-and-worktree/untracked/binary/
unborn-HEAD/CRLF/conflicted tests all green.

**F-GIT-DIFF-02** — PASSED. `diff.rs` unit tests (hunks, CRLF-strip, single-number and zero-zero
hunk headers, binary marker, multi-hunk accumulation) all green.

**F-GIT-DIFF-03** — PASSED. `side_by_side_rows_pair_context_zip_replacements_and_pad_pure_runs`,
`side_by_side_preserves_hunks_pairs_runs_and_drops_metadata`,
`side_by_side_handles_real_rename_binary_and_large_context` all green; the Unified/Split segmented
control was also visibly present and switchable in every Changes-tab screenshot taken this pass.

**F-GIT-DIFF-04** — PASSED. `parses_numstat_with_binary_and_rename`, `empty_numstat_is_empty`,
`stats_count_staged_files_in_an_unborn_head_repo`,
`stats_fall_back_to_the_diff_for_uncountable_untracked_files` all green.

**F-GIT-PLAT-01** — PASSED. All 64 tests above ran against the real Linux `git` executable resolved
via `PATH`, exercising repository/worktree/status/diff/action flows end-to-end on this box today —
the PLATFORM clause's specific ask.

---

## Verification addendum (second pass, same day)

This file already existed, untracked, when this pass started — an earlier attempt at this exact
shard was cut short before it could return its structured result. Rather than re-drive 38 rows
from zero, this pass **audited** the work above rather than trusting it: re-ran
`cargo test -p tiller_git` (64/64 green, identical counts per file), re-ran
`cargo test -p tiller_ui --lib changes::` (30/30 green) and `--lib right_panel` (21/21 green,
including all three `activity_section_*` tests), and opened the actual PNGs behind the four most
load-bearing and most surprising claims — the ones a lazy carry-forward would most want to fake:

- `36-err7/02-y1-immediately-after-break-no-click.png` — confirmed exactly as described: the
  `fin-chgit-err` project is gone from the sidebar entirely, the main pane shows "No worktree
  selected", and the **Files panel on the right still shows the stale `/tmp/fin-chgit-err` listing**
  (`a.txt`) rather than its own no-worktree placeholder. Supports F-CHG-02/03/09's downgrade.
- `22-expandall/03-ex2.png` — confirmed: `image.bin` under Changed, chevron expanded, zero body
  content while every other expanded row on the same frame shows real diff lines. Supports
  F-CHG-15's "binary half is absent" finding.
- `26-resolve2/03-s2.png` — confirmed: a real `diff --cc conflict.txt` output and live shell
  prompt in a "Resolve conflict.txt" terminal tab. Supports F-CHG-16 PASSED.
- `53-activity5/03-ad1-clicked-row-body.png` — confirmed: clicking the "Resolve conflict.txt"
  Activity row moved the active-tab underline to that tab. Supports F-CHG-21's focus-half PASSED.

One gap in the prior pass's own reasoning was closed rather than just re-asserted: F-GIT-REPO-01's
evidence cited tests that don't actually touch the ".git directory **or file**" half of its clause
(`is_git_repository` lives in `tiller_project::discovery`, not `tiller_git`, and the existing
`discovery_integration.rs` suite only ever asserts it `true` against a normal `.git` **directory**,
never against a linked worktree's `.git` **file**). Wrote a throwaway test today
(`git worktree add` a real linked worktree, assert `linked/.git` is a file via
`Path::is_file()`, then assert `tiller_project::is_git_repository(&linked)` is `true`), ran it
(`scratch_is_git_repository_true_for_linked_worktree_git_file ... ok`), and deleted it — not
committed, per the task's instruction to commit only this file. F-GIT-REPO-01 now has a directly
exercised proof for the specific case its own citations were missing, rather than an inference from
`Path::exists()`'s documented behavior.

No claim in the document above was found to overstate its evidence. Verdicts below match the
document's own per-row conclusions unchanged, with F-GIT-REPO-01's evidence strengthened as
described.

## Summary of departures from the ledger

Three rows regress a prior PASSED after being re-driven live and independently against the VERIFY
clause rather than the prior prose: **F-CHG-02**, **F-CHG-03**, **F-CHG-09** (the Files/Changes
error-and-recovery family — the app now loses the whole project from the sidebar under a permission
error instead of showing an inline unavailable+Retry state, and the right panel never shows its own
no-worktree placeholder). **F-CHG-15**'s binary-message half is newly shown absent by both code and
live census. **F-GIT-RUN-01** is downgraded from a flat PASSED to half-proven because the clause's
output-limit and cancellation requirements are absent by census, a fact the prior pass's own
evidence text already stated without it changing the verdict.

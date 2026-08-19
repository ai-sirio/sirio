# F-CORE-DOM — fresh critic pass (live drive)

Section: **F-CORE-DOM** (8 rows, Package tier / `TillerCore` domain sub-group — project/worktree
domain state, primary-branch resolution, tab-cycling logic), split out of the old 45-row
`F-CORE -- domain, files, usage, workspace` header.

Contract source: `docs/linux-rewrite/02-inventory-packages.md` lines 29–36 (`F-CORE-DOM-01`
through `-08`). The existing `INVENTORY-LEDGER.md` rows (all previously `PASSED`) were treated as
prior judgements only, not as truth — every row below was independently re-driven against a live
binary this pass, except where marked.

**Environment**: `/dev/shm/tt/debug/tiller` + `tillerctl`, driven via `Scripts/wayland-drive.sh`.
Label `sweep14dom`, outdir `/dev/shm/sweep-14-F-CORE-DOM`, throwaway fixture
`/dev/shm/sweep14dom-fixture` (git repo with two branches: `master`/HEAD holding only `init`, and
`feature` holding `init` + an extra `feature-commit` not reachable from `master`). Host was heavily
loaded for parts of this pass (load average climbed from ~10 to ~29 on a 12-core box over the
session, with sway/grim intermittently crashing under memory pressure) — every finding below was
re-run clean at least once to rule out load-induced flakiness before being recorded as a defect.

## Verdicts

| row | verdict | evidence |
|---|---|---|
| `F-CORE-DOM-01` | **FAILED — defective** | Live restart test. Project display name ("MyFixture") and icon colour (green) survive a genuine kill+relaunch (same DB) correctly. But "Default Worktree Base" pinned to `feature` (UI showed "feature"/"Pinned") and "Worktree Location" override set to `/dev/shm/domrestarttest` (UI showed the literal path, no "Restore Default" residue) both **silently reverted to their unset defaults** after a clean restart — reopening Project Settings showed "master / Following primary branch (master)" and the bare `/dev/shm` placeholder again. Two of the eight listed persisted fields do not survive a restart. Screenshots: `22-both-set-preclose` (both fields set) vs `24-reopened-after-restart` (both reset). Additionally `ctl project.list` exposes only `id/isGit/name/path/worktreeCount/worktrees` — display name, colour, icon, default-worktree-base and location-override are **not visible through the control listing at all**, so the "…and visible through the corresponding domain/control listing" half of the VERIFY clause fails even for the fields that do persist. |
| `F-CORE-DOM-02` | **FAILED — defective** | Live, single uninterrupted session (no restart confound). Fixture: `master`=HEAD (only `init`), `feature`=`init`+`feature-commit`. Set project's "Default Worktree Base" to `feature` (UI confirmed "feature"/"Pinned" immediately). Closed settings. Opened "New Worktree…" (its own placeholder literally reads *"base branch (optional, defaults to HEAD)"*), typed only a branch name `cleanbase1`, left the dialog's own Base field blank, pressed Enter. Result on disk: `git -C /dev/shm/MyFixture-cleanbase1 log --oneline` → only `init` — **no `feature-commit`**, proving the new branch was cut from HEAD, not from the pinned project base. The persisted project pin has zero effect on worktree creation; only the one-off dialog field (defaulting to HEAD) matters. |
| `F-CORE-DOM-03` | PASSED | Live. With `TILLER_PROJECTS_DIR`/`XDG_DATA_HOME` unset, "Create Project…" dialog's "Parent location" read exactly `/home/enzopalmisano/Tiller/projects` (`$HOME/Tiller/projects`), matching `default_project_base()`. Screenshot `53-create-project-dialog`. The Linux-replacement-root half (`TILLER_PROJECTS_DIR` override) was proven live in a previous wave (wave N) and the code path is unchanged, so re-derivation wasn't repeated. |
| `F-CORE-DOM-04` | PASSED | Live. Typed `"  SPINOFF"` (leading whitespace + uppercase) into the sidebar Filter box: rows narrowed to exactly the project (`MyFixture`, kept because a child matched) and the `spinoff` worktree row, proving trim + case-insensitivity + branch-name matching together. Screenshot `54-filter-spinoff-upper`. Empty-query "show everything" implicitly exercised throughout the rest of the pass. |
| `F-CORE-DOM-05` | PASSED | Live, real OS-level drag. `drag 150 410 150 350` moved the `spinoff` worktree row from below `cleanbase1` to above it — sidebar order changed from `[master, cleanbase1, spinoff]` to `[master, spinoff, cleanbase1]`. Screenshots `58-before-drag` / `59-after-drag`. Unknown-id/no-op sub-case not independently re-driven this pass; relying on the prior wave's three named `VisualTestContext` tests for that half. |
| `F-CORE-DOM-06` | **FAILED — defective** | Live. With 4 open Terminal tabs, isolated single key-presses `ctrl-1`, `ctrl-2`, `ctrl-3`, `ctrl-9` all correctly jump to the matching 1-indexed tab (`ctrl-9` correctly selects the last tab). **Decisive test**: from tab 1 active, an isolated `ctrl-7` (out of range: not ≤4 tabs, not 9) moved the active tab to **tab 4 (the last tab)** instead of leaving tab 1 unchanged. Screenshots `50-before-c` (tab1) → `51-after-solo-ctrl7` (tab4). This is a real "clamp to last" behaviour, contradicting the contract's "invalid numbers ignored" and contradicting the already-correct, already-unit-tested `tiller_project::domain::numeric_tab_selection` (returns `None` for the same input) — which, like `move_item` in `F-CORE-DOM-05`'s prior finding, is dead code with zero live callers; the wired implementation (`panes.rs`'s `TabSelection::jump`) reimplements the same feature with different, wrong-per-contract behaviour. wrap/cycle (`ctrl-tab`/`ctrl-shift-tab`) not independently re-driven this pass (already PASSED live in wave H, and `TabSelection::cycle`'s wrap logic matches `domain.rs`'s `move_tab` on inspection). |
| `F-CORE-DOM-07` | half-proven | **Not live-driven this pass** — would require a real multi-minute agent conversation (real API cost) to exercise the 30s/200-char boundaries properly, which this pass's time budget did not allow. Code inspection: real production wiring exists beyond the pure unit test — one `AutoNamingThrottle` per tab id in `main.rs` gates a genuine summarizer subprocess spawn and tab-title rewrite, with dedicated tests that spawn real subprocesses (`run_summarizer_command_trims_and_truncates_real_process_output`) and cover the exact "default chat tab, no tab agent" regression this row's history names. I did not re-run those tests nor drive a live agent turn myself this pass, so I cannot upgrade this past half-proven. |
| `F-CORE-DOM-08` | PASSED | Live (indirect) + structural. `restored_scrollback_scheduled: OnceGate` gates `schedule_restored_scrollback`, called on **every render frame** per its own doc comment — so a broken gate would re-schedule/duplicate scrollback replay continuously. Across roughly a dozen kill+relaunch restarts this pass, terminal panes and their pfetch-banner content reappeared cleanly exactly once every time, with no duplication or corruption in any screenshot, consistent with the gate firing once. Combined with the structural proof (a previously-dead ported type now wired to a genuine call site, replacing a hand-rolled bool of identical behaviour) already established by a prior critic pass, I'm affirming PASSED rather than only half-proven. |

## Defects (reproductions)

### F-CORE-DOM-01 — project-level worktree defaults do not survive a restart
1. `ctl project.add path=/dev/shm/<fixture>`
2. Open Project Settings (gear icon, right edge of the project row, revealed on hover).
3. Type into "Search branches by name…" (the Default Worktree Base field): `feature`. UI
   immediately shows `feature` / `Pinned`.
4. Type into the Worktree Location field: any absolute path, e.g. `/dev/shm/domrestarttest`.
5. Click "Close".
6. Kill and relaunch the app against the **same** database (in this harness: re-invoke
   `wayland-drive.sh` with the same `TILLER_WL_LABEL`).
7. Reopen Project Settings.
   - **Expected** (per `F-CORE-DOM-01`): both fields still show `feature`/Pinned and
     `/dev/shm/domrestarttest`.
   - **Actual**: both fields are back to their unset defaults (`master`/"Following primary
     branch (master)" and the bare `/dev/shm` placeholder).

### F-CORE-DOM-02 — a pinned project base branch is ignored when creating a worktree
1. Fixture repo with `master` (HEAD) and `feature` (one extra commit not on `master`).
2. Project Settings → set Default Worktree Base to `feature` (confirmed "Pinned" in the UI).
3. Close settings. Click "New Worktree…".
4. Type a branch name, e.g. `cleanbase1`. **Leave the dialog's own "base branch" field blank.**
5. Press Enter.
6. `git -C <new-worktree-path> log --oneline` → only `init`, i.e. branched from HEAD/`master`.
   - **Expected**: branched from `feature` (the pinned project default).
   - **Actual**: branched from HEAD, exactly as if no pin had ever been set. The dialog's own
     placeholder text ("base branch (optional, defaults to HEAD)") is honest about what the code
     actually does — but that default itself is the bug, since the contract says the project pin
     should be consulted first.

### F-CORE-DOM-06 — an out-of-range numeric tab jump clamps to the last tab instead of being ignored
1. Open a worktree, create 4 Terminal tabs (New Terminal, then "+" → New Terminal ×3).
2. Press `ctrl-1` to make tab 1 active (confirmed via screenshot).
3. Press `ctrl-7` (only 4 tabs exist; 7 is neither ≤4 nor the special-cased 9).
   - **Expected**: ignored, tab 1 stays active.
   - **Actual**: tab 4 (the last tab) becomes active — identical to what `ctrl-9` would do.

## Harness note (not an app defect)

`Scripts/wayland-drive.sh`'s `chord` action (used for `ctrl-<digit>`) is **only reliable as the
first chord fired per invocation**. Repeated `chord ctrl <N>` calls within one script invocation
reliably no-op after the first, regardless of which digit — confirmed by firing `ctrl-1` then
`ctrl-2` (both unambiguously "valid" targets) back to back and seeing the second one silently do
nothing. Every conclusion in this report was drawn from **isolated, single-chord invocations**
(fresh restart, one `chord` call, generous settle) specifically to avoid this confound; any earlier
multi-chord-per-invocation attempts that looked like "invalid numbers correctly ignored" were
discarded as inconclusive once this pattern was identified, and the real defect above was only
confirmed once the harness limitation was isolated and controlled for.

## Not reached / not independently re-driven this pass

- `F-CORE-DOM-01`'s worktree-side fields (`comment`, `timestamps`) — not independently exercised;
  `ctl workspace.list`/`project.list` expose `comment` (empty string in this fixture) but **no
  timestamp fields at all**, which is a plausible extension of the same control-listing gap noted
  above but wasn't separately confirmed against the UI.
- `F-CORE-DOM-05`'s unknown-id/no-op-move sub-case — relying on the prior wave's named tests.
- `F-CORE-DOM-06`'s `ctrl-tab`/`ctrl-shift-tab` wrap-cycle — relying on the prior wave's live pass
  plus code inspection (`TabSelection::cycle` matches `domain.rs`'s `move_tab`).
- `F-CORE-DOM-07` — no live agent conversation was driven this pass (see verdict above); this is
  the single largest gap in this pass's coverage.

## Disagreement with the recorded ledger

The ledger records all eight rows as `PASSED`. This pass **downgrades three to FAILED —
defective** (`-01`, `-02`, `-06`) and one to `half-proven` (`-07`). `-02` and `-06` in particular
follow the exact shape already flagged for `F-CORE-DOM-05`/`-08` in earlier waves: a correctly
implemented, unit-tested `tiller_project::domain` function exists for the feature, but the live UI
path was wired by hand with different — and, in these two cases, contract-violating — logic instead
of calling it. That pattern is worth a whole-file sweep of `domain.rs`'s remaining exports against
their real call sites; this pass only had budget to confirm it for the two rows in scope.

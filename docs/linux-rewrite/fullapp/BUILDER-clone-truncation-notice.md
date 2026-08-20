# F-GIT-RUN-01 streaming half — wiring `GitCommandResult::truncated` to the UI

## The finding this fixes

Recorded in `fullapp/BUILDER-streaming-truncation.md` and `INVENTORY-LEDGER.md`'s `F-GIT-RUN-01`
row: `GitCommandResult::truncated` was written, unit-tested, and structurally unreachable.
`GitClone::clone` (`tiller_git/src/clone.rs:33-38`) was the only production caller of
`run_streaming` outside `git.rs`'s own tests, and it discarded the result with
`GitRunner::run_streaming(...)?; Ok(())` — a `Result<(), GitError>` signature with no slot for
the flag to leave in. One layer up, `CloneForm::submit` (`tiller_ui/src/project_forms.rs:182`)
discarded it again with `.map(|_| worker_destination)`. The path is genuinely reachable by users:
Sidebar "+" → "Clone Repository…" → `CloneForm` (`clone-url-field` / `clone-submit`).

Re-confirmed at the start of this task, current tree:

```
$ grep -rn 'run_streaming' rust/crates/ --include=*.rs | grep -v '/git.rs:' | grep -v '/tests/'
rust/crates/tiller_git/src/lib.rs:61:pub use git::{GitCancellationToken, GitCommandResult, GitRunner, run_streaming};
rust/crates/tiller_git/src/clone.rs:33:        GitRunner::run_streaming(&arguments, parent, move |line| {
```

Same two lines as the earlier drive — the gap had not moved.

## What changed

Branch `feat/clone-truncation-surface-8802`, worktree `/var/tmp/tt-ruling-clonetrunc-8802`.

1. **`tiller_git/src/clone.rs`** — `GitClone::clone` and `clone_repository` now return
   `Result<bool, GitError>` instead of `Result<(), GitError>`. The `bool` is the truncated flag
   read straight off `GitCommandResult::truncated`. Still not an error on truncation — the
   streaming runner already delivered every progress line to `on_progress` as it arrived (see
   `read_stderr_lines` in `git.rs`: the per-line callback runs off the *uncapped* read, only the
   retained diagnostic buffer is capped), so a truncated capture never means the clone itself
   failed or lost data the caller needed.

2. **`tiller_ui/src/project_forms.rs`**:
   - `CloneStatus::Complete` gained a `truncated: bool` field.
   - `CloneFormEvent::Cloned` gained the same field.
   - `clone_status_line` grew a sibling arm next to `Failed`'s `"Clone failed: {error}"`: when
     `truncated`, the line reads `"Cloned to {path} — progress output was truncated (repository is
     large)"` in `theme.git_modified` (the theme's documented warning hue) instead of
     `theme.tab_done`'s success color — matching how the surrounding UI already threads a status
     string and a color out of one `match`, and reusing an existing token rather than inventing a
     new one.

3. **`tiller_ui/src/sidebar.rs`** — the part the flag's own reachability depended on and that
   the earlier drive did not have to consider, because the flag never reached this file before.
   `Sidebar::start_clone_project`'s `cx.subscribe` handler closed `project_form` unconditionally
   on every `CloneFormEvent::Cloned`. Left alone, a truncated completion would render its notice
   and destroy the popover showing it in the same frame — the same "produced and discarded"
   shape as the flag's own original bug, one layer up. `Sidebar::clone_form_stays_open_after`
   makes the decision explicit and unit-tested: an ordinary completion still auto-closes
   (unchanged behavior); a truncated one leaves the form open so the notice is actually seen,
   dismissed via the pre-existing Cancel button.

## Tests

Tests-first; each one would fail against the pre-change code (no `bool` to return, no
`truncated` field to set, the popover closing unconditionally).

| crate | file | new tests |
|---|---|---|
| `tiller_git` | `src/clone.rs` | `clone_reports_truncated_when_the_output_cap_is_lowered`, `clone_does_not_report_truncated_under_the_default_cap` (negative control, same fixture) |
| `tiller_git` | `src/git.rs` | `set_output_limit_override_for_test` (`pub(crate)` helper, not a test itself — lets `clone.rs`'s tests force the cap via the existing thread-local override instead of process-global `set_var`) |
| `tiller_ui` | `src/project_forms.rs` | `clone_completion_can_carry_a_truncation_notice` (status line names the truncation, and its color is neither `tab_error` nor `tab_done`) |
| `tiller_ui` | `src/sidebar.rs` | `only_a_truncated_clone_keeps_the_popover_open` (the auto-close-vs-stay-open decision, both directions) |

Existing tests updated only for the new field shapes (`CloneStatus::Complete { .. }` instead of
`Complete(_)`, `.complete(destination, false)` instead of `.complete(destination)`) — no
assertion was weakened.

### Before / after counts

`cargo test -p tiller_git` (full crate, unit + all 4 integration suites):

| | before | after |
|---|---|---|
| lib unit tests | 33 | 35 |
| `git_integration` | 13 | 13 |
| `p41_git_behaviors` | 9 | 9 |
| `p99_git_rows` | 6 | 6 |
| `worktree_integration` | 11 | 11 |
| **total** | **72** | **74** |

All passed both before and after; ran live post-change: **74 passed, 0 failed**.

`cargo test -p tiller_ui`: before/after counts and the live run are in the "Live proof" section
below (this crate's suite takes long enough under this host's load that it was run once, live,
against the finished code — see the raw counts there rather than a separately-timed "before").
`grep -c '#\[test\]\|#\[gpui::test\]'` against `git show ed9dc05f:...` vs the working tree:
`project_forms.rs` 7 → 8, `sidebar.rs` 42 → 43 (both +1, matching the two new tests above).

## Known measurement constraint (re-confirmed, not re-litigated)

Git's `--progress` output is percentage-throttled, not volume-scaled — it plateaus regardless of
repo size (measured previously: 4K files → 17.85KB, 24K files → 18.51KB, 480MB → 23.87KB, 1.75GB
→ 32.39KB). No repo size worth waiting for clears a 64KiB pipe buffer through this call site. The
truncation cap was therefore lowered via `TILLER_GIT_OUTPUT_LIMIT_BYTES`, not the fixture size —
see the live proof below.

## A test-pollution defect found and fixed before any of this could be trusted

Before driving the UI, `cargo test -p tiller_git` (default parallel harness, the same command CI
would run) failed **deterministically** — not flaky, reproduced on 3/3 runs:

```
git::tests::the_output_limit_override_is_confined_to_its_own_thread ... FAILED
  left: 1024
 right: 10485760
diff::tests::stats_fall_back_to_the_diff_for_uncountable_untracked_files ... FAILED
  panicked at crates/tiller_git/src/diff.rs:709:18: no entry found for key
```

`--test-threads=1` made both pass, which is the signature of cross-thread pollution, not a
correctness bug in the shipped change. The cause was a test left in `clone.rs` from an earlier,
interrupted pass of this same task — `diagnostic_env_var_path_reports_truncated`, explicitly
labelled `TEMPORARY DIAGNOSTIC — not meant to survive this session`, that called
`std::env::set_var("TILLER_GIT_OUTPUT_LIMIT_BYTES", "1024")` with no synchronization. Rust's test
harness reuses OS threads across `#[test]` functions, so that process-global env write leaked into
whichever other test the harness scheduled next on the same thread — exactly the footgun
`OUTPUT_LIMIT_OVERRIDE`'s own doc comment (`git.rs:130-138`) was written to describe and the
thread-local override exists to avoid. Deleted the test (it was never part of the committed design
— `clone.rs` at the already-committed `82142659` never had it; comparing the working tree against
that commit after deletion showed zero diff). `cargo test -p tiller_git` then passed 3/3 runs under
the default parallel harness. See "Tests" above for the corrected counts (unaffected: the two
tests this task adds were never the pollution source, and the base-count grep against `ed9dc05f`
independently confirms 33→35 either way).

## Live proof

Binary pinned per-drive (`TILLER_WL_BIN`) and hash-verified against the freshly built
`cargo-target-ruling-clonetrunc-8802/debug/tiller`
(`2ada3dfb7420ca1926f5fe8ddbe2da1e7af58846edbc5eca72c64c43c3c0f976`), built from the tree at
commit `08c28598` (this task's `tiller_ui` commit). Fixture: `/var/tmp/clonetrunc-fixture-8802`, a
throwaway local repo (5 × 2MB blobs, ~10MB) — standalone `time git clone --no-local --progress`
measured 1.7s wall, comfortably inside the "small fixture, ~10s or less, real in-progress state"
target from the task brief. Recon passes (`clonetrunc8802recon1/2/3`, screenshots not kept)
located the real drawn coordinates before the scored drives: "+" at `(293, 51)`, "Clone
Repository…" at `(188, 112)`, the URL field at `(161, 447)`, and "Clone repository" submit at
`(161, 546)` — matching `click 161 546` from an independent earlier drive (`F-PRJ.md`
F-PRJ-06), which is corroborating, not assumed. `type` was not tried against this field —
`WAYLAND-LANE.md` already documents it dropping characters here — went straight to per-character
`key <name>` (`slash`/`minus`/literal letters and digits) with a 0.22s settle between presses, one
action string per drive per the harness's single-invocation-per-gesture-chain rule.

**Positive drive** (`clonetrunc8802posF`, `TILLER_GIT_OUTPUT_LIMIT_BYTES=1024` to force the cap
low — per the task's documented measurement constraint, `--progress` output plateaus around
17-32KB regardless of repo size at any wall-clock-reasonable fixture, so the cap must be lowered,
not the fixture grown):

1. `01-clone-form-empty.png` — the real popover from a real `+` → "Clone Repository…" click chain.
2. `02-truncated-url-typed.png` — `/var/tmp/clonetrunc-fixture-8802` landed in the field
   character-by-character; "Destination" correctly derived
   (`/tmp/clonetrunc8802posF-projects/clonetrunc-fixture-8802`).
3. `03-truncated-in-progress.png` — real in-flight state, "Cloning… 0%", captured before
   completion (not synthesized).
4. `04-truncated-notice-POSITIVE.png` — the project is live in the sidebar (`clonetrunc-fixture-8802`,
   `main` worktree, `Primary` badge — the clone genuinely succeeded), **and** the popover stayed
   open showing, in the theme's warning/amber hue: *"Cloned to
   /tmp/clonetrunc8802posF-projects/clonetrunc-fixture-8802 — progress output was truncated
   (repository is large)"*. This is both halves of the fix in one frame: `GitClone::clone`'s
   `truncated` flag reached `CloneStatus::Complete`, and `Sidebar::clone_form_stays_open_after`
   kept the popover from closing before the notice could be read.

**Negative control** (`clonetrunc8802negF2`, same fixture, same gesture chain, default 10MB cap —
`TILLER_GIT_TIMEOUT_MS=60000` added after a first attempt (`clonetrunc8802negF`, not kept) hit the
default 10s timeout under this host's concurrent build load and surfaced the **pre-existing**
`GitError::TimedOut` path instead, correctly, as red "Clone failed: git clone … did not finish
within 10s and was killed" — a real accidental-control sighting of the *other* non-success status
line, left as a footnote rather than a discarded screenshot, since it independently confirms the
error-color arm of `clone_status_line` still behaves):

5. `05-negctrl-url-typed.png` / `06-negctrl-in-progress.png` — identical setup to the positive
   drive, same fixture, same field contents.
6. `07-negctrl-no-notice-clean-close.png` — the project is in the sidebar (`clonetrunc-fixture-8802`,
   `main`, `Primary`) and **the clone popover is gone** — no truncation notice, no lingering form.
   This is the control that makes the positive frame mean something: under the default cap, the
   same fixture, same gesture chain, produces the old, unconditional auto-close behavior with no
   warning text anywhere, proving the notice in frame 4 is conditioned on real truncation, not
   always shown.

All app instances for every label above exited cleanly at drive end (`kill_ours`); confirmed with
`ps aux | grep clonetrunc8802` showing nothing left running after the batch.

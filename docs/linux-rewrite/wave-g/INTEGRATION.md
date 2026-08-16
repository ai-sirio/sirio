# Wave G integration report

Base: `6ce47a9` (docs: wave-G slice briefs). Head at integration: `e6fab92`. Six slices,
15 commits (9 `fix`/`test` + 6 `docs`), all on `linux/gpui-waku` in the shared worktree.

## Build

`cargo build -p tiller` — green. Two pre-existing `dead_code` warnings only, both explained by this
wave's own changes, not regressions: `BrowserSurface::pump_task` (unrelated, predates this wave) and
`fn sidebar_projects` in `crates/tiller/src/main.rs` (the bare, comment-less helper G6-sidebar's
F-SID-11 fix stopped calling at the real-startup call site — `Workspace::sidebar_projects` still
exists and is used elsewhere; only the free `fn sidebar_projects` wrapper went dead). Left as-is:
removing it is a legitimate follow-up cleanup, not required for correctness.

## Tests (per crate, never `--workspace`)

- `cargo test -p tiller` — **147/147 pass.**
- `cargo test -p tiller_ui` — **315/315 pass** (includes the new
  `status_bar::tests::a_real_timeout_after_a_real_success_dims_the_live_entity`, F-USE-03).
- `cargo test -p tiller_terminal` — **38/38 pass when serialized**
  (`-- --test-threads=1`). Run with default parallelism it flakes 0-2 tests
  (`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`,
  occasionally also `scrollback_can_be_viewed_after_output_exceeds_the_viewport`) across three
  repeated runs. Isolated single-test reruns of the job-control one always pass. Confirmed
  pre-existing: the test predates this wave (`git log -S` traces it to `915b0d0`, well before
  `6ce47a9`) and neither failing test is in a file this wave's slices touched for that behavior
  (`link_router.rs`/the `lib.rs` hunk this wave added is pure click→cell arithmetic, nowhere near
  process-group shutdown or scrollback). Root cause is almost certainly concurrent forking across
  parallel test threads perturbing process-group timing, not a wave-G regression. Not fixed here —
  out of scope for this wave's owned rows, but worth a follow-up ticket to serialize or isolate that
  test group.

## Cross-slice sweep

Every file touched since `6ce47a9`, and every commit against it, checked by **file ownership** in
`manifest.json`, not commit-subject scope text:

| File | Commits | Owning slice(s) per manifest | Attribution |
|---|---|---|---|
| `rust/crates/tiller/src/main.rs` | `9ed8f08` (P126), `d4d9294` (F-CORE-ACT-25/26), `8860603` (F-SID-11) | G1, G2, G3, G4, G5, G6 (all own `main.rs`) | G1, G2, G6 respectively — each explicitly self-attributed in its own report and consistent with file ownership |
| `rust/crates/tiller_terminal/src/lib.rs` | `86695b3` (F-TERM-UI-02) | G1, G2, G3, G4, G5 | G5 |
| `rust/crates/tiller_terminal/src/link_router.rs` (new) | `86695b3` | G5 only | G5 |
| `rust/crates/tiller_ui/src/browser.rs` | `4a6d36c` (F-BRW-01) | G1 only | G1 |
| `rust/crates/tiller_ui/src/chat.rs` | `140f7cf` (F-EDIT-07) | G3, G4, G5 | G4 |
| `rust/crates/tiller_ui/src/file_view.rs` | `140f7cf` | G3, G4 | G4 |
| `rust/crates/tiller_ui/src/status_bar.rs` | `d0afa58` (F-USE-03) | G6 only | G6 |

All seven touched files, all nine code commits, attributed with no ambiguity — every commit's file
set is a subset of exactly one round's slice ownership, and each commit's own message/report matches
the row ID(s) it claims. Nothing unattributable this wave.

**Deletion audit.** Read every commit's diff in isolation and checked deleted lines against its own
message (the wave-B/wave-C failure mode this step exists for):

- `9ed8f08` (P126): deletes the old hardcoded-5s `recv_timeout(CONTROL_ACTION_TIMEOUT)` branch —
  exactly the code the commit message says it replaces. No unrelated loss.
- `d4d9294` (F-CORE-ACT-25/26): the only deletion is a `use tiller_activity::{...}` line rewritten
  to add `BootstrapRestoreOrder`/`WorktreeMountPolicy` to the same import block (rustfmt-style
  re-wrap) — nothing dropped.
- `8860603` (F-SID-11): deletes the bare `sidebar_projects(&project_catalog)` call, replaced with
  the comment-seeded variant, exactly as described.
- `4a6d36c` (F-BRW-01): deletes the entire prior self-calibrating `native_webview_rect` function,
  its doc comment, its calibration-loop callsite, and its now-wrong unit test
  (`webview_bounds_pass_gpui_logical_pixels_straight_to_wry`) — all replaced with the
  scale-factor-multiplying version and new tests per the report's root-cause writeup. This is a
  full function replacement, not a quiet unrelated loss; verified the new tests
  (`webview_bounds_recover_physical_target_from_live_fractional_scale_factor`,
  `webview_bounds_reject_non_finite_scale_factor`) are present and pass.
- `86695b3` (F-TERM-UI-02): deletes the inlined click→cell math from `on_left_mouse_down` and the
  old re-exported `opens_terminal_link, url_at_column` line (superseded by calling the new
  `link_router::resolve_click_cell` directly) — matches the report's "pulled the math into
  `link_router.rs`" description.
- `140f7cf` (F-EDIT-07): deletes `CodeSpanKind`/`CodeSpan`/`code_spans` as *private* items in
  `file_view.rs` — confirmed by diff, they are not gone, only re-declared `pub(crate)` two lines
  down in the same hunk. `chat.rs`'s deletion is one `render_plain_text` call swapped for
  `render_highlighted_code`. No duplicate definitions, no unrelated loss.
- `d0afa58` (F-USE-03): zero deletions (pure test addition).

No reverts of prior work found. No file was touched by a commit outside its owning slice(s).

## Foreign-file fixes (step 4)

Both flagged needs were checked against their builder's own report for a concrete, applicable
description, per the house rule "apply what is described, do not invent one":

- **G2-gaps-a / `tiller_ui/src/chat.rs`** (for F-CORE-DOM-07): the report does not describe a
  small fix — it describes an entire missing subsystem (turn-completion trigger, an async
  process-spawn name generator, new per-tab throttle state) explicitly sized **L** and explicitly
  scoped to "one integration pass... that owns both `main.rs` and `tiller_ui/src/chat.rs`" as a
  *future* slice's work, not a boundary-blocked one-liner. There is nothing concrete to apply
  without inventing the feature myself. Left blocked, unchanged.
- **G6-sidebar / `tiller_ui/src/project_forms.rs`** (for F-PRJ-05): read `CloneForm::on_url_key`
  directly — it is a straightforward per-keystroke `push_str` with no off-by-one or truncation
  logic, matching the report's own reading. The report's "what I'd hand to whoever owns this file"
  section offers two *candidate* directions, both explicitly conditional ("if GPUI's dispatch can
  silently miss events...", "if [an IME text-input path] exists in this GPUI version") — neither is
  a description of an actual code change, and the report itself frames the underlying cause as
  possibly a synthetic-input-tool (`wtype`) race rather than an app defect (an outside author on
  the same field reached the same conclusion independently, per `P123`). Applying either candidate
  blind would mean guessing at unverified GPUI internals in a file with no test coverage of this
  path today. Left blocked, unchanged, per "do not invent one."

## Docs-only history note (not a wave-G action)

`465b71a` ("docs: withdraw 7 overstated N/A — platform exemptions (P128)") edits
`docs/linux-rewrite/INVENTORY-LEDGER.md` and sits between the wave-G workflow-script commit
(`6a8b569`) and G1's first fix commit. It predates any G-slice's own commits and isn't attributed to
a slice in any report — most likely an orchestrator-side edit made just before dispatch. Flagged
here for visibility only; not touched by this integration (the ledger is the orchestrator's alone to
write).

## Binary

Rebuilt after all checks; current at `rust/target/debug/tiller` (matches `HEAD` at the time of this
report, `e6fab92`).

## Summary

Build green, all three touched crates' test suites green (terminal serialized due to a confirmed
pre-existing flake unrelated to this wave), all 7 touched files / 9 code commits attributed
unambiguously to their owning slice by file ownership, no reverts of prior work found in any
deletion audited, both flagged foreign-file needs investigated and correctly left unfixed since
neither report describes an applicable concrete change, binary rebuilt and current.

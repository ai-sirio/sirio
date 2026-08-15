# Wave E Integration Report

Base: `eb34610`. Branch: `linux/gpui-waku`. 8 slices (E-C-1..4, E-P1..4), 34 rows.

## Build

`cargo build -p tiller` — green, no fix needed. One pre-existing warning
(`pump_task` never read in `tiller_ui/src/browser.rs`), unrelated to this
wave, not touched by any wave-E commit.

## Tests (per crate, never `--workspace`)

- `tiller_persistence`: 38 passed, 0 failed.
- `tiller`: 145 passed, 0 failed.
- `tiller_terminal`: 36 passed, 0 failed on a clean run. One isolated run of
  `tests::shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
  failed (`setsid must actually have put the child in a new process group`);
  re-ran it alone three times and it flipped ok/fail/ok with no code change
  in between. The function this test exercises (`shutdown`) is untouched by
  any wave-E diff — the only change to this file is `9a13174` (F-TERM-UI-02),
  which only touches mouse-click coordinate translation and `TerminalHandle`
  bounds tracking. Pre-existing timing flake, not introduced by this wave.
- `tiller_ui`: 310 passed, 0 failed.

## Cross-slice sweep

Files touched since `eb34610` (12 total, all under `rust/`):
`tiller_persistence/src/db.rs`, `tiller/src/main.rs`, `tiller/src/session.rs`,
`tiller_terminal/src/lib.rs`, `tiller_ui/src/browser.rs`,
`tiller_ui/src/changes.rs`, `tiller_ui/src/chat.rs`,
`tiller_ui/src/file_view.rs`, `tiller_ui/src/project_identity.rs`,
`tiller_ui/src/right_panel.rs`, `tiller_ui/src/sidebar.rs`,
`tiller_ui/src/status_bar.rs`.

19 non-doc commits examined (`eb34610..HEAD`, excluding the 8 per-slice
`docs(...)` report commits). For each, resolved the commit's `F-*` ID against
`manifest.json`'s `slices[*].ids`, then checked the touched file against that
slice's `owns` list (file ownership, not commit-subject scope text) — every
commit's file(s) were within its resolving slice's owned-file list; no
commit's changes fell outside its own slice's ownership.

Every commit was then read in isolation (`git show <sha> -- rust/`) with
attention to `-` lines. All deletions found were the immediate old code being
replaced by the same commit's own new code (renamed functions, threaded
parameters, refactored match arms) — none removed a prior, unrelated fix:

- `7190fde` (F-CHAT-34): `insert_tab` renamed to `upsert_tab` — full function
  body preserved, only turned into an `ON CONFLICT` upsert; delete-then-insert
  replaced by a stale-id-only delete pass. Verified `insert_tab` has no other
  callers left dangling.
- `25c8f97` (F-CTRL-BROWSER-04): comment block rewritten to add
  `browser.snapshot`, old comment content preserved and extended, not
  dropped. Test loop's membership list extended, not narrowed.
- `77db870` (F-CTRL-WORK-01): `write_catalog`'s worktree loop rewritten to
  iterate a pre-fetched `existing_by_id` map instead of a live query,
  carrying `comment`/`created_at` forward — same iteration, no rows dropped.
- `9a13174` (F-TERM-UI-02): click-coordinate math rewritten to subtract the
  painted element's origin; old two-line calculation fully replaced by its
  successor in the same hunk.
- `33fbb97` (F-BRW-01): additive — new `Cell`-based scale-correction state
  and calibration path; no existing logic removed.
- `f05d158` (F-CHG-13): `focus_path` split into itself (now deferring when
  `entries` is empty) plus a new `apply_focus` helper carrying the original
  body forward unchanged; only line actually removed was the old bare
  `cx.notify()` call, now correctly still reachable via the non-deferred path.
- `8ce8480` (F-CHAT-05), `1bd7916` (F-CHAT-20), `2810651` (F-TAB-01),
  `3f9df90` (F-PRJ-14), `ef34561` (F-SID-15), `407b43a` (F-SET-10),
  `06a63f0` (F-SET-04), `f568960` (F-SID-11): additive; the only removed
  lines were import-list/signature lines rewritten in place by the same
  hunk (e.g. adding `img`/`deferred`/`Cell` to a `use` line).
- `4dc28f6` (F-CORE-FILE-04): `Chat::render_markdown_document` renamed to
  `render_markdown_document_with_link_override` and threaded an
  `Option<LinkClickOverride>` through the whole markdown-render call chain
  (`render_markdown`/`_block`/`_list`/`_table`/`_inline`); every "removed"
  line is that same line reappearing a few lines later with the new
  parameter added — full call graph re-diffed to confirm no branch was
  dropped, only extended.

No commit was left unattributed — every one of the 19 non-doc commits
resolved cleanly to exactly one slice via its `F-*` ID.

## Genuine reverts found

None. No commit removed code or behavior belonging to another slice's row.

## Fixes applied per step 4

None requested by any builder report (manifest lists none).

## Rebuild

`cargo build -p tiller` rebuilt clean after the sweep (no changes were
needed, so no rebuild was strictly required, but binary is current):
`target/debug/tiller`, ELF pie, not stripped, present.

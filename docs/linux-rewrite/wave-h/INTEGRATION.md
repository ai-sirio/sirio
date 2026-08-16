# Wave H integration report

Base: `5bec7b5`. 32 commits landed on `linux/gpui-waku` before this pass (3 diagnosers, five build
slices H1–H6 in serial main.rs order, each with a doc report), plus this integration's own fix
commit. All work reviewed lived in one shared worktree; no rebase or merge was needed.

## 1. Build

`cargo build -p tiller` was green at HEAD before any integration work (two pre-existing
`dead_code` warnings only: `BrowserSurface::pump_task`, `sidebar_projects` — neither touched by
this wave). No builder left the build broken; nothing to fix here beyond the one applied item in
§4.

## 2. Tests, per crate

Ran individually, never `--workspace`, per house rules:

| crate | result |
|---|---|
| `tiller_acp` | 6 + 6 passed (unit + integration), 1 ignored (needs live Claude creds) |
| `tiller_persistence` | 39 passed |
| `tiller_terminal` | 44 passed (43 after H2's own commit, +1 from this integration's own fix) |
| `tiller_ui` | 322 passed |
| `tiller_usage` | 10 passed |
| `tiller` | 161 passed (159 after the slices, +2 from this integration's own fix) |

All 0 failed. The known pre-existing flake,
`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
(`tiller_terminal`), was re-run alone and passed — unaffected by this wave, as documented.

## 3. Cross-slice sweep

Walked `git log --format='%h %s' 5bec7b5..HEAD -- <file>` for every file touched since base (16
files: `rust/Cargo.lock`, both `tiller_acp` files, `tiller/Cargo.toml`, both `tiller_persistence`
files touched by H1, `tiller/src/main.rs`, `tiller/src/session.rs`, `tiller/src/tray.rs`, both
`tiller_terminal` files, `tiller_ui/src/chat.rs`, `tiller_ui/src/titlebar.rs`,
`tiller_usage/src/claude.rs`, `Scripts/acp-ticker-fixture.py`, `Scripts/wayland-drive.sh`), then
`git show` on each commit in isolation and read its deleted lines against its own message.

**Attribution**, by file ownership (manifest `owns`), not commit-subject scope text:

- `main.rs`: every touching commit attributed to a real, run slice — `26a2c5d4`/`8eb880f5`/
  `03f5c889` (H1-gaps), `46de0584` (H3-tray), `b1f719c9` (H4-window), `c331914c` (H6-instruments,
  F-TERM-UI-02). (H2-ctxmenu's own commit, `671cd378`, touches only `context_menu.rs`/`lib.rs`,
  not `main.rs` — it explicitly declined the `main.rs` change, see §4.) This is the wave's declared
  legitimate multi-owner file (manifest note: "every slice may touch main.rs"); no commit here was
  unattributable.
- Files touched **outside** their nominal slice's `owns` list, verified as legitimate scope creep
  tied to the same commit's own stated F-ID rather than evidence of a misattributed or stray
  commit: `tiller_persistence/{src/db.rs,tests/persistence_integration.rs}` by `03f5c889`
  (F-CORE-WSP-08 / H1-gaps — the commit message itself explains the persistence-layer bug it hit
  mid-drive and fixed); `tiller_acp/{src/lib.rs,src/mcp_config.rs}` by `037006f9` (F-CHAT-33 /
  H6-instruments); `tiller_terminal/src/lib.rs` and `main.rs` by H6-instruments' F-TERM-UI-02
  commits (`d8be5b25`, `ec40654c`, `c331914c`). None of these are H6's declared five-file `owns`
  set, but each is a direct, in-message-explained consequence of the row H6 was actually assigned,
  not an unexplained drift.
- `6bbdbbbf` (`fix(terminal): anchor the right-click context menu...`) is the one commit with no
  F-ID in its subject — it is P129's diagnosis-driven fix, landed before H2-ctxmenu started (H2's
  own report confirms and builds on it). Correctly a non-slice, diagnosis-driven commit, not
  unattributable.
- The wave's one non-slice committer, the orchestrator, appears once as `4c4d701c` (manifest +
  workflow scaffold, pure addition) — consistent with the note that only the orchestrator edits
  wave scaffolding, and it never touches `INVENTORY-LEDGER.md` in this wave's range.

**Deletion audit**: every commit touching a shared or previously-fixed file was read in full, not
just stat'd. Findings:

- `8eb880f5`, `03f5c889`, `671cd378`, `46de0584`, `d8be5b25`, `c331914c`, `ec40654c`, `037006f9`,
  `ab7b125f`, `6bbdbbbf`: every deletion is the expected half of a signature change, a replaced
  literal, or a block moved a few lines with logic preserved — each cross-checked against its own
  commit message and, where a prior fix's tests existed, confirmed those tests still pass at HEAD.
- `3ab56291` deletes `wayland-drive.sh`'s old single-modifier `chord` comment/body while adding the
  multi-modifier version described in its own message — no unrelated content lost.
- `5cf4fc26` deletes 29 lines of `WAYLAND-LANE.md` prose (P130's "not independently proven"
  caveat) and replaces it with the live proof that closes that exact gap — a documentation update
  superseding its own prior claim, not a code revert.

**No genuine revert of a prior, unrelated fix was found anywhere in this wave's 32 commits.**
Nothing needed restoring.

## 4. Foreign-file fix applied

H2-ctxmenu's report named one precise, actionable change it declined to make speculatively in
`main.rs`: thread `cx: &mut Context<Self>` through `render_pane_tree`/`render_group_surfaces` and
push each terminal's pane-group membership into a new `TerminalView::set_sole_tab_in_group`
setter, wiring the `SplitDisabledReason::SoleTabInGroup` half F-TAB-11 had left undone. Applied
exactly as described (commit `68978b09`):

- `main.rs`: `columns`/`render_group_surfaces`/`render_pane_tree` now take `cx: &mut Context<Self>`
  (already available in `Render::render`); the `TabContent::Terminal` leaf arm computes
  `self.tab_machinery.group_tabs(tab.group_id)` membership and calls
  `view.update(cx, |t, cx| { t.set_sole_tab_in_group(sole); cx.notify(); })` on every render.
- `tiller_terminal/src/lib.rs`: `TerminalView` gains the `sole_tab_in_group` field, the
  `set_sole_tab_in_group` setter, and a `sole_tab_in_group()` accessor; the render site passes it
  into `items_with_split_availability`.
- `tiller_terminal/src/context_menu.rs`: `items_with_split_availability` gains a third
  `sole_tab_in_group: bool` parameter and folds in the same reason text as
  `panes::SplitDisabledReason::SoleTabInGroup`, taking priority over the pane-too-small check for
  all four split directions, matching that type's own precedence.
- Two new `tiller` gpui tests (`a_solo_tab_is_pushed_as_the_sole_tab_in_its_group`,
  `two_tabs_sharing_a_group_are_not_pushed_as_sole`) drive real workspaces and read the pushed flag
  back off the live `TerminalView` entity; a new `tiller_terminal` unit test
  (`a_sole_tab_in_group_disables_every_split_regardless_of_size`) covers the size-independent
  priority. `cargo test -p tiller_terminal --lib`: 44 passed (was 43). `cargo test -p tiller`: 161
  passed (was 159). Both 0 failed.

No other builder report in this wave (`H1`, `H3`, `H4`, `H5`, `H6`) named a declined foreign-file
change — only `H2-ctxmenu-report.md` contains a "not applied" / "foreign-file change wanted"
section.

## 5. Binary

Rebuilt after the fix above: `rust/target/debug/tiller`, current as of this integration pass.

## Summary

- Build: green.
- Tests: 6 crates run individually, all green (`tiller_acp` 12, `tiller_persistence` 39,
  `tiller_terminal` 44, `tiller_ui` 322, `tiller_usage` 10, `tiller` 161 — post-fix counts). Known
  flake re-confirmed passing alone.
- Commits examined: all 32 pre-integration commits plus this pass's own.
- Genuine reverts found: none.
- Restored: nothing (none needed).
- Unattributable: none — every commit traced to a real slice, a diagnoser, or the orchestrator.
- Applied H2-ctxmenu's requested main.rs change (SoleTabInGroup wiring), new commit `68978b09`,
  with tests.
- Binary rebuilt and current at `rust/target/debug/tiller`.

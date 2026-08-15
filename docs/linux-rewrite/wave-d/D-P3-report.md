# D-P3 report

## `F-CORE-USG-05` — fixed

`tiller_usage/src/codex.rs`'s `refresh_token()` called `refresh_token_at(TOKEN_URL, ...)` where
`TOKEN_URL` was a hardcoded `const` pointing at `auth.openai.com`. The Wave-C critic's unit test
(`a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields`) already drove
`refresh_token_at` directly against a local HTTP fixture and proved the merge-on-success behaviour
(`save_credentials_to`), but that only proves the helper function — the real `CodexUsageFetcher::fetch()`
path a running `tiller` binary takes was unreachable from a test/fixture because the URL it calls was
not overridable.

Added `token_url()` which reads `TILLER_CODEX_TOKEN_URL` and falls back to the existing `TOKEN_URL`
constant when unset; `refresh_token()` now calls `refresh_token_at(&token_url(), ...)`. `codex` (the
real CLI) never reads this var, so setting it in Tiller's process cannot affect a real `codex` token
file. Added `refresh_token_honors_the_token_url_override` — a new test that drives the *public*
`refresh_token()` (not `refresh_token_at` directly) through the env override against a one-shot local
fixture, proving the override wiring itself, which is exactly the code path a live `tiller` process
launched with `TILLER_CODEX_TOKEN_URL` set would execute end to end (load → needs_refresh gate →
refresh → parse → merge-save).

`cargo test -p tiller_usage codex::` — 13/13 pass, including the new test.

**howToExercise**: launch `tiller` with `CODEX_HOME=<tmp with a backdated auth.json>` and
`TILLER_CODEX_TOKEN_URL=http://127.0.0.1:<fixture-port>/oauth/token` pointed at a local HTTP server
that replies `200 {"access_token":"...","refresh_token":"..."}`; open the status bar Codex usage
tile and confirm the on-disk `auth.json`'s `tokens.access_token` changes to the fixture's value while
unrelated fields (e.g. a synthetic `custom_field`) survive — that is the live merge-on-success proof
that was previously unreachable outside a unit test.

Commit: `fce0c00` — `fix(F-CORE-USG-05): make Codex token endpoint overridable for live proof`

## `F-CORE-USG-07` — fixed (same underlying gap as USG-05)

The ledger's own Wave-C evidence for this row says its remaining owed half is explicitly
"Refresh/merge-save-on-success half (shared w/ F-CORE-USG-05), no new instrument found" — i.e. this
row and USG-05 share one gap in one file. My brief listed `tiller_agents/src/codex.rs` as this row's
file, but that file (the Codex *agent adapter* — spawn command / notify-hook config) contains no
`Err(_) -> LoggedOut` branch and nothing usage-related; the branch the ledger cites (line ~331) lives
in `tiller_usage/src/codex.rs`, which is the file I already own and already fixed for USG-05. No
separate code change was needed or made in `tiller_agents/src/codex.rs` — I read it end to end to
confirm there is genuinely nothing to fix there for this row (confirmed via `AgentAdapter` impl:
`prepare`/`command`/`resume_command`/`notify_override`, no usage/refresh logic at all).

USG-07's own specific claims (the `LoggedOut` branch existing, and the live positive-control label
pair `'Codex 100% 5h'` vs `'Codex logged out'`) were already independently re-verified and stand;
only the shared refresh/merge half was owed, and that is the same fix as above.

**howToExercise**: same as USG-05 above (they share the code path) — plus the already-standing
control: empty `CODEX_HOME` shows "Codex logged out" in the status bar, a populated one with valid
creds shows a real percentage.

Commit: none new — covered by `fce0c00` (see USG-05 above). No changes to
`rust/crates/tiller_agents/src/codex.rs` were needed.

## `F-CORE-ACT-11` — not fixed (no code gap found; live-only exercise remains owed)

`tiller_activity/src/model.rs` is a pure state-machine crate with no test module of its own (its
tests live in `tiller_activity/tests/activity_domain_integration.rs`, a file this slice does not
own). Re-read the whole ownership machinery (`title_owned_panes`, `process_owned_panes`,
`handle_title_change`, `process_gone`, `pane_closed`) end to end: spawn-owned clearing (exit-driven,
no set membership at all), title-owned clearing (`title_owned_panes.remove` only on an unmatched
title), and process-owned clearing (`process_owned_panes.remove` only from `process_gone`) are three
independent code paths that never touch each other's membership sets. This matches the Wave-C
critic's own finding that a combined pure test already exercises all three kinds on three separate
panes in one test and confirms independence at the model level.

What remains owed, per both the Wave-C and Wave-D critics, is purely a **live** exercise: three real
panes of three different ownership kinds open *simultaneously* in one worktree, each cleared
independently while the other two are watched to confirm they don't move. That is not a code defect
— it's an evidence gap. I did not find or introduce any code change in `model.rs`; the file is
correct as-is against its own combined-kinds test. I chose not to spend this slice's budget chasing a
three-real-agent-process simultaneous Wayland exercise (claude + codex + a Node-hosted CLI, each in
its own pane, independently cleared and screenshotted at each step) because it is long-running,
input-race-prone (the Wave-D critic's own title-owned leg was "inconclusive (input race on a fresh
pane)" under the same conditions), and the code side of this row is already sound.

**howToExercise** (repeating the standing recipe, unchanged from Wave-C, since it is still exactly
what's owed): open three panes in one worktree — one via Tiller's own "+" spawn menu with a real
agent CLI (spawn-owned, now confirmed reachable per the Wave-D critic's 36/37.png), one where an
unregistered pane's title is set to an agent's title convention via `title <text>` (title-owned), one
where a real agent CLI runs as the pane shell's direct foreground child with no title convention
(process-owned) — then independently clear each (SIGTERM the spawn-owned one, change the
title-owned one's title to something unmatched, kill the process-owned one's child process) and
confirm by screenshot at each step that the other two panes' status is unaffected.

## `F-CORE-FILE-03` — blocked; fix belongs in a foreign file (`tiller_ui/src/right_panel.rs`)

`rust/crates/tiller_project/src/file.rs` (the only file this row lists, and the only file in this
slice's ownership that touches file drops) is already complete and correct for this feature:
`classify_file_drop`/`terminal_file_drop`/`shell_quote_path` are all tested and passing
(`F-CORE-FILE-01`/`02` PASSED). `terminal_file_drop` in particular has **no extension restriction** —
it quotes and joins any `&[PathBuf]` — so it already supports dragging a `README.md`, not just
images; `classify_file_drop`'s image-only restriction is a separate code path (image-paste
classification), not the one a file-row drag would use.

The actual gap the critic found live (dragging `README.md` onto a terminal pane produced no text, only
a row-select) is 100% a missing *source* wiring in `tiller_ui/src/right_panel.rs`'s
`render_file_row` — confirmed by grep: it chains `.on_mouse_down(Left)` and `.on_mouse_down(Right)`
but never `.on_drag(...)`. The *target* side already exists and already works:
`tiller_terminal/src/lib.rs:1556` has `.on_drop::<PathBuf>(move |path, _, cx| { terminal.receive_file_drop(vec![path.clone()], cx); })`,
which calls straight into `tiller_project::terminal_file_drop` — the function this slice owns and
that is already correct. So the only missing piece, in a file this slice does not own, is: give each
non-directory row in `render_file_row` an `.on_drag(<absolute PathBuf>, |_, _, _, cx| cx.new(|_| SomeDragPreview))`
that mirrors the test-only `DragFixture` fixture already in this same file (search `DragFixture` /
`EmptyDragPreview`, right_panel.rs:1897-1946) — same pattern, wired into the real
`render_file_row` instead of a test harness, using `entity`/`repo_root` already in scope there to
build an absolute path from `row.node.path` (which is worktree-relative).

I did not edit `tiller_ui/src/right_panel.rs` — it is not in this slice's owned-file list and no
change to `tiller_project/src/file.rs` is needed or was made.

**wantedForeignFiles**: `rust/crates/tiller_ui/src/right_panel.rs` — add, inside `render_file_row`,
for non-directory rows only:
```rust
.when(!is_dir, |this| {
    let drag_path = repo_root.join(&path); // repo_root needs threading into render_file_row, or capture self.repo_root before the fn call site
    this.on_drag(drag_path, |_, _, _, cx| cx.new(|_| EmptyDragPreviewOrEquivalent))
})
```
(`repo_root` is a field on `RightPanel`, not currently passed into `render_file_row`'s parameter
list — the call site at right_panel.rs:749 would need to pass it through alongside `theme`/`entity`.)

**howToExercise** (once the foreign-file change lands): `Scripts/wayland-drive.sh`'s drag helper,
drag a file row (e.g. `README.md`) onto an open Terminal pane, force a repaint, and confirm the
terminal's line buffer now contains the quoted path text (this is exactly the negative control the
Wave-C critic already ran and got a row-select instead of text — re-running it after the wiring
lands should flip to text landing in the terminal).

# D-MAIN-4 report

Slice owner: `rust/crates/tiller/src/main.rs` (plus one narrow change in
`rust/crates/tiller_ui/src/browser.rs` for the browser.eval/console
implementation itself — main.rs only handles the control-socket wiring).
7 rows, all committed individually with explicit paths.

## `F-CTRL-BROWSER-06` — implemented (2 commits: `13b4a72`, `274cd28`)

`browser.eval` and `browser.console` were fully absent. Implemented:
- `BrowserSurface::evaluate_script` (tiller_ui/src/browser.rs) runs JS via
  wry's `evaluate_script_with_callback`, pumping the process-global GTK main
  loop until the callback fires or a timeout elapses — the same pattern
  `browser.wait` already used for WebKit's async navigation events.
- A per-navigation `CONSOLE_CAPTURE_SCRIPT` init script shadows
  `console.log/warn/error/info/debug` to append into
  `window.__tillerConsole`; `browser.console` reads that buffer back via
  `evaluate_script`.
- **Critical second commit**: the first pass wired the handler but missed
  that `browser_request_error`'s `BROWSER_CAPABILITIES` allowlist gate
  (checked *before* `handle_browser_action` ever runs) still excluded both
  methods, so every call was pre-rejected as "unsupported on Linux" — dead
  code on arrival. Fixed by adding both to the allowlist plus a script-param
  validation case, and updated the two tests that asserted the old
  pre-rejection list.

**howToExercise**: `ctl browser.open url=https://example.com` then
`ctl browser.eval script=1+1` / `ctl browser.console` on the socket. On the
Wayland lane both now reach the real handler and return an honest
`"Browser child is unavailable"` (documented Wayland limitation — no X11
window handle for the webview); full content-level proof needs
`DISPLAY=:1` per `WAYLAND-LANE.md`. Verified live both ways: before the
gate fix, both returned the old `"not implemented"` string; after, they
return the handler's own error.

## `F-CTRL-WORK-01` — fixed (`108342c`)

`worktree.set`'s `comment` field was documented as "intentionally not
persisted" and lived only in the in-memory `ControlWorkspace`. Added
`AppControlHandler::persist_worktree_comment` (writes the existing
`worktree.comment` DB column via `AppDatabase::worktree_by_path` +
`save_worktree`, wired with a new `database_path` field set only at real
startup) and `ControlState::apply_persisted_comments` (reloads it at
startup, applied once against the DB before wrapping in `Arc`).

**howToExercise**: `ctl worktree.set worktree=<id> comment=X`, kill the
process fully (no `TILLER_WL_KEEP`), relaunch fresh against the same DB
file/label, `ctl worktree.set worktree=<id> session=probe` (session-only,
to read state without mutating comment) and confirm `comment` is still `X`.
Not re-driven live in this pass (built + unit tested only) — the row's own
critic evidence already used exactly this restart recipe, so it is a known
route.

## `F-EDIT-08` — already-correct, now exercised (no code change)

`add_file_tab` (main.rs) already implements real inline dedup: it scans
open File tabs for a path match and focuses the existing tab instead of
duplicating. The row was stuck at NOT EXERCISED only because a prior
attempt's double-click coordinates missed. Root cause of *that*: two
separate `click` calls issued ~seconds apart (each `pointer_command` does a
`swaymsg` round trip) land outside GPUI's 400ms `DOUBLE_CLICK_INTERVAL`
(`gpui_linux/src/linux/wayland/client.rs:40`), so `click_count` never
reaches 2 with only two clicks.

**howToExercise**: three rapid `click <x> <y>` calls (not two) at the same
file row in the Files panel opens it as a new tab; three more clicks on the
same row re-focuses the existing tab with no duplicate — confirmed live in
this pass: `/tmp/dm4-fedit02/03-01-first-open.png` (CLAUDE.md opened, one
tab), `04-02-switched-to-chat.png` (tab persists across a tab switch),
`05-03-second-open-attempt.png` (re-clicking re-focuses, still exactly one
CLAUDE.md tab in the strip and in the sidebar tree). Do all of this within
**one** `wayland-drive.sh` invocation — a second invocation restarts the
app fresh (session UI state like open tabs is not what's under test here)
and loses the previously opened tab.

## `F-GIT-BRANCH-01` — implemented (`57fc62f`)

`GitBranches::list` had zero production callers (confirmed by three prior
sweeps). Per the T9 triage doc's own sizing ("socket door: S"), added a
`git.branches` control method (mirrors the existing `surface.changes.*`
pattern, reuses `changes_worktree` for repo resolution) returning branch
rows via the existing `rows::encode` convention.

**howToExercise**: `ctl git.branches worktree=<path-or-id>` — exercised
live against this repo's own worktree, returned
`[{"name":"linux/gpui-waku"},{"name":"main"},{"name":"rust/gpui-rewrite"}]`,
exact real branch names through the real Git layer. (git itself refuses
branch names containing spaces — confirmed with a direct `git branch`
attempt — so the row's "spaces" clause is provably only satisfiable at the
`GitBranches::parse` layer, already covered by
`p41_git_behaviors.rs::branch_listing_preserves_spaces_in_names` and
`p99_git_rows.rs::branch_listing_returns_exact_names_and_git_refuses_spaced_names`.)

## `F-PER-08` — half-proven → both halves now provable (`ef04d9c`)

`allow_permission`/`deny_permission` had no caller except the doorhanger's
GPUI `on_click` closures. Added `browser.permission` (`action=allow|deny`)
calling the same surface methods, registered in the same
`BROWSER_METHODS`/`BROWSER_CAPABILITIES` lists as the other browser.*
fixes above.

**howToExercise / verified live**: `browser.open` → `browser.act
driving=true` → `browser.navigate` to a fresh origin (triggers the
permission doorhanger under agent-driving) → `browser.permission
action=allow` → forced repaint (`shot`, which the render loop's
`newly_allowed` scan runs on). Confirmed
`SELECT * FROM browser_origin_grant` had the granted origin immediately
after, then killed the process fully and relaunched fresh against the same
DB (`dm4perm.sqlite`) with a brand-new sway compositor — the grant was
still there. Full restart roundtrip, not just the write.

## `F-PER-01`, `F-PERSIST-DB-05` — fixed together (`04ae263`)

Root cause: `restore_tabs` and `restore_tabs_in_workspace` both created
every restored chat tab with `Chat::launch_with_command` — no
`ChatPersistence` at all. Only the very first chat tab of a brand-new
database (created via `add_chat_tab`'s `launch_with_persistence`) ever had
persistence wired; from the first restart onward, the *restored*
replacement tab lost it permanently. `persist_settled_transcript`
early-returns when `self.persistence` is `None`, so every turn completed
after the first restart was silently unsaved — exactly pass-17's live
finding (`chat_turn=0`, `session_ref=0` after two real ACP exchanges).
`restore_tabs`'s own doc comment described this as intentional ("identity,
not transcript"); it wasn't load-bearing anywhere else, just stale.

Fix: both restore paths now call
`Chat::launch_with_command_and_persistence` with `session::database_path()`
(the same deterministic path `main()` opens at startup), `tab.id` as the
persisted `tab_id`, and `session::persisted_worktree_id(working_directory)`
— the identical key convention `add_chat_tab` already uses for freshly
created chats.

**Exercised live end-to-end** on the wayland lane using
`tiller_acp/tests/fixtures/acp_fixture.py normal` as a real ACP backend
(via `TILLER_ACP_PROGRAM` pointed at a one-line wrapper supplying the
`normal` argv): sent a turn (`surface.chat.send` + `surface.chat.permission
action=deny` to resolve the fixture's pending tool permission), confirmed
`chat_turn` held it (478-byte payload); killed the process fully and
relaunched fresh against the same DB; `surface.chat.read` immediately
showed the prior turn with zero further calls (proves
`restore_persisted_transcript` now runs on the restored tab); sent a
*second* turn on the restored tab and confirmed `chat_turn`'s payload
updated to the second turn's content (proves ongoing persistence survives
a restart, not just the very first save).

**howToExercise**: same recipe — `TILLER_ACP_PROGRAM=<wrapper around
acp_fixture.py normal>`, `surface.chat.send` + `surface.chat.permission
action=deny` to complete a turn, kill+relaunch same DB, `surface.chat.read`
shows the prior turn restored, send a second turn, inspect
`chat_turn`/`session_ref` in the sqlite file directly.

## Build status

`cargo build -p tiller` and `cargo build --tests -p tiller` both green
throughout. Targeted test runs also green: `cargo test -p tiller browser`,
`cargo test -p tiller restore_tabs`, `cargo test -p tiller chat`.

## Foreign files wanted

None — every row's fix landed inside `rust/crates/tiller/src/main.rs` (plus
the one `tiller_ui/src/browser.rs` addition for F-CTRL-BROWSER-06, needed
because the JS-eval primitive itself has to live on `BrowserSurface`, not
in the control dispatcher).

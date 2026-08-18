# FIX wave I — closing wave F's six red rows

Lane `wf-fix`. Scope: the six rows wave F's live drive marked red in
`docs/linux-rewrite/INVENTORY-LEDGER.md` — `F-CHAT-33`, `F-SID-19`,
`F-TERM-10`, `F-PRJ-13` (FAILED — defective) and `F-PRJ-17`, `F-PRJ-18`
(FAILED — absent).

Process followed per row: reproduce live first, name the root cause in one
sentence, fix with a red/green regression test, re-drive the row live to
confirm the fix holds through the real UI, commit the row on its own with
explicitly enumerated paths. As the builder I do not get to pass my own
work — every row below is `half-proven`; the **Remaining gap** line under
each names exactly what a fresh critic must drive to confirm.

Four of the six rows (`F-CHAT-33`, `F-SID-19`, `F-TERM-10`, `F-PRJ-13`) were
already fixed by prior commits on this branch, each already carrying its own
root-cause diagnosis, red/green regression test, and a documented live
re-drive. I reproduced their fixes holding at current HEAD with my own fresh
live drives this pass (details below) rather than re-fixing something already
fixed — `git merge-base --is-ancestor <commit> HEAD` confirmed each fix commit
is a genuine ancestor, not a stray/unmerged branch. The remaining two rows
(`F-PRJ-17`, `F-PRJ-18`) were genuinely absent — the persistence and session
layers already carried the data end to end (landed 2026-08-15, commits
`821ac0a4`/`f74334b7`), but no `tiller_ui` control ever existed to read or
write it. I built that UI this pass, ported faithfully from
`App/ProjectSettingsSheet.swift:390-553`, and committed it as `2d66b3ce`.

---

## `F-CHAT-33` — turn errors and MCP warnings need an "OK to dismiss" control

**Ledger line 182.** Fix commit: `83be7a14` (ancestor of HEAD, confirmed via
`git merge-base --is-ancestor`).

**Root cause (as diagnosed by the fix commit):** `chat.rs`'s error-banner
render path had only one button branch, `.when(retryable, ...)` labeled
"Retry"/"Restart agent" — a non-retryable error (every `McpWarning` is
constructed `retryable: false`) drew zero interactive controls at all, and
even a retryable error never offered a plain dismiss-without-retrying choice,
contradicting the Swift reference's `promptError`/`mcpWarning` banners, which
both carry exactly one action: `actionTitle` "OK".

**Fix:** `Chat::dismiss_error` removes exactly the clicked `Entry::Error` row.
An "OK" control now renders for every error, retryable or not, alongside
Retry/Restart when present.

**Regression tests (already in `rust/crates/tiller_ui/src/chat.rs`):**
`an_mcp_warning_offers_ok_to_dismiss`, `a_retryable_turn_error_offers_ok_alongside_retry`.
Re-ran both this pass: `cargo test -p tiller_ui --lib offers_ok` →
`test result: ok. 2 passed`. The commit's own message documents the red run
before the fix ("a non-retryable MCP warning must still offer OK to dismiss"
/ "a retryable turn error must also offer OK to dismiss", both panicking on
the unfixed tree).

**My own live re-drive this pass** (turn-error half, wayland lane, fresh
`cargo build -p tiller` binary — the same binary that also carries this
session's F-PRJ-17/18 UI, so this also serves as a no-regression check):
used `rust/crates/tiller_ui/tests/fixtures/chat_fixture.py`'s `death-then-ok`
mode (streams `"partial "` then hard-exits with code 17 on the first prompt)
via a one-line wrapper script pointed at by `TILLER_ACP_PROGRAM`. Route
found live (the tab-bar "+" menu's per-agent items resolve through
`AgentAdapter::acp_program()` and do **not** honor the override; the
sidebar's per-worktree right-click → "New Chat" item calls
`add_chat_tab(window, None, cx)`, which does honor it):

1. `ctl project.add`, click the worktree row, right-click it, click "New
   Chat", type a marker message into the composer, `key Return`.
2. Real transcript result: user bubble, an assistant "partial" chunk, then a
   red error row reading `prompt failed: Incoming transport closed: {
   "reason": "incoming_transport_closed", "method": "session/prompt" }` with
   **both** a "Restart agent" button and an "OK" button next to it —
   confirming the exact deficiency the ledger named (no OK, ever) is fixed.
3. Clicked OK: the error row disappeared; the user's message and the
   "partial" assistant chunk remained untouched in the transcript; composer
   stayed in the offline/reconnect state (OK does not restart the agent,
   matching the commit's description). Reproduced twice (screenshots
   `/tmp/wf-fixc-chat33/02-01-after-send.png`,
   `/tmp/wf-fixe-chat33/04-03-settle.png` before OK and
   `/tmp/wf-fixe-chat33/05-04-after-ok.png` after).

**Remaining gap for a fresh critic:** I did not myself live-drive the
MCP-warning (non-retryable) half this pass — only the turn-error (retryable)
half. That half rests on the fix commit's own documented live drive (a
fixture emitting a real MCP-flavored stderr line) plus the passing
`an_mcp_warning_offers_ok_to_dismiss` unit test. A fresh critic should
reproduce an actual `.mcp.json`-driven MCP warning live (recipe: a broken
`.mcp.json` in the worktree root, real agent, per
`docs/linux-rewrite/UNPROVEN-ROWS-RECIPES.md` / `rust/crates/tiller_acp/src/mcp_config.rs`
comments) and confirm the OK button renders and works the same way there,
not just for the turn-error case.

---

## `F-SID-19` — Ctrl+T from the empty "No Terminals" state does nothing

**Ledger line 88.** Fix commit: `407f7c9b` (ancestor of HEAD).

**Root cause:** the empty "No Terminals" state had no focusable element;
GPUI's key dispatch falls back to the window root above every
`on_action`/`capture_key_down` the workspace registers, so Ctrl+T was
genuinely undelivered, not merely unhandled.

**Fix:** `TillerWorkspace` gets a permanent `root_focus` `FocusHandle` via
`track_focus`, reclaimed in `render()` whenever `window.focused()` is empty.

**Regression test:** `ctrl_t_from_the_empty_worktree_state_creates_a_terminal`
(drops focus via `window.blur()` on a zero-tab worktree, asserts Ctrl+T still
creates a tab). Commit message documents red (`assertion left: 0, right: 1`)
→ green, full `cargo test -p tiller` (181 tests) green.

**My own live re-drive this pass** (wayland lane, fresh scratch project, no
tabs open): screenshot-confirmed landing on "No Terminals", sending Ctrl+T,
and a real Terminal tab appearing with a live shell prompt — same outcome the
fix commit itself documents, reproduced independently at current HEAD.

**Remaining gap for a fresh critic:** confirm this holds for every root
keybinding this fix is meant to generalize to (the commit's own rationale is
"every global keybinding", not just Ctrl+T) — e.g. drive a different
root-level shortcut (new project, command palette, whatever else dispatches
through the same root) from the same zero-tab empty state.

---

## `F-TERM-10` — a bare running command doesn't survive a worktree switch

**Ledger line 328.** Fix commit: `984defa7` (ancestor of HEAD).

**Root cause:** the outgoing-worktree liveness check guarding
`select_worktree`'s tab reload only consulted the agent-activity model, so a
plain `sleep 300` with no recognized agent, no OSC title, no ACP connection
read as Idle and the pane was silently torn down and replaced with a fresh
shell on switch-away/switch-back.

**Fix:** `tab_has_live_foreground_process` walks the terminal's real process
tree (the same Layer-D `tiller_activity::inspect_process_names` walk that
powers agent detection) and ORs into the outgoing-safety check, so any live
descendant of the login shell blocks the reload regardless of agent
involvement.

**Regression test:**
`switching_away_from_a_bare_running_command_leaves_it_mounted` (deliberately
never notifies workspace activity, so only the process-tree check can save
the tab). Commit documents red (`left: 0 right: 1`) → green, full
`cargo test -p tiller --bin tiller` (182 tests) green.

**My own live re-drive this pass**: typed `echo WFTERM10MARK; sleep 300` into
a real terminal tab, confirmed the shell and `sleep` child PIDs via host
`ps -p <pid> -o pid,etimes,args`, then drove two real `ctl workspace.select`
round trips over the live control socket (project A → project B → project
A) — the identical PIDs were still alive afterward with continuous `etimes`
(no gap, i.e. no process was ever killed and respawned), and the terminal's
scrollback still showed `WFTERM10MARK` with no new shell banner.

**Remaining gap for a fresh critic:** the fix's guard is specifically a live
foreground process; a critic should also confirm the negative case still
works as intended — a genuinely idle bare shell (no foreground child) *does*
still get reloaded/recycled on switch-away as before, i.e. this fix didn't
overshoot into pinning every terminal forever regardless of activity.

---

## `F-PRJ-13` — Reset button click leaks through to the row underneath

**Ledger line 106.** Fix commit: `043b1e71` (ancestor of HEAD).

**Root cause:** `render_project_settings`'s full-sheet overlay div was a
plain `.absolute()` sibling of `sidebar-tree`, never `.occlude()`d. GPUI's
hit test walks every hitbox under the pointer back-to-front and only stops
at one with `HitboxBehavior::BlockMouse` (installed by `.occlude()`) — absent
that, a click on the sheet also reached whatever sidebar row was painted
underneath it, and that row's own `on_click` (`SidebarEvent::SelectWorktree`)
fired in the same gesture.

**Fix:** `.occlude()` on the `project-settings-sheet` root div.

**Regression test:**
`reset_button_click_does_not_leak_through_to_the_row_underneath` (builds a
fixture sized so a real decoy worktree row renders directly under the
sheet's Reset button, asserts the overlap exists, then asserts clicking
Reset emits no `SidebarEvent::SelectWorktree`). Commit documents red
(`... got [ProjectSettingsChanged(...), SelectWorktree("/tmp/prj13-decoy/wt-4")]`)
→ green, full `cargo test -p tiller_ui --lib` at its pre-existing baseline
(351/352, one known unrelated environment-dependent titlebar failure).

**My own live re-drive this pass** (wayland lane, two real projects,
`wf-prj13-a` primary/selected and `wf-prj13-b` collapsed below it): opened
Project Settings on `wf-prj13-a`, clicked inside the sheet at a coordinate
that (absent occlusion) would land on `wf-prj13-b`'s row, then clicked the
real Reset button — the status bar / active-worktree indicator read
`master · /tmp/wf-prj13-a` unchanged across all three screenshots (sheet
open, after the in-sheet click, after Reset), confirming no click on the
sheet — Reset or otherwise — leaks through to the sidebar underneath at
current HEAD.

**Remaining gap for a fresh critic:** my live re-drive confirmed general
click-occlusion (no leak-through from any point in the sheet), which is a
slightly broader claim than the original bug report (specifically about
Reset). A fresh critic could tighten this by reproducing the exact original
repro shape — a decoy worktree row rendered directly under the *Reset
button's own pixel position* in a live three-project sidebar — matching the
regression test's fixture geometry exactly, not just a generic in-sheet
click.

---

## `F-PRJ-17` / `F-PRJ-18` — default worktree base and worktree location controls (built this pass)

**Ledger lines 110–111.** Both FAILED — absent. Fix commit (this pass):
`2d66b3ce`.

**Root cause:** the persistence and session layers already carried
`default_worktree_base` and `worktree_location_override` end to end
(`tiller_persistence`'s `project` table columns, `tiller_project::Project`,
`tiller::session`'s `CatalogProjectSettings` /
`ProjectCatalog::project_settings` / `update_project_settings`, landed
2026-08-15 in `821ac0a4`/`f74334b7`) — but `821ac0a4`'s own commit message
says the UI layer was left unbuilt, and a fresh `grep -rn` across
`tiller_ui` before this pass returned zero hits for either field, matching
wave F's finding exactly. The feature was genuinely absent above the
persistence/session layers, not defective.

**Build, ported from `App/ProjectSettingsSheet.swift:390-553`:**

- `rust/crates/tiller_ui/src/sidebar.rs`: `ProjectSettingsCard` gains
  `default_worktree_base`/`worktree_location_override` draft fields
  (`Rc<RefCell<String>>`, matching the existing `display_name` pattern) with
  their own `FocusHandle`s, and a `primary_branch` field computed in
  `open_project_settings` by scanning the project's child rows for the
  primary worktree. `ProjectSettingsUpdate` carries both new `Option<String>`
  fields through `project_settings_update` (trim + empty→`None`). `Sidebar`
  gains `project_worktree_defaults: HashMap<String, (Option<String>,
  Option<String>)>` and `set_project_worktree_defaults()`, the seed a host
  calls to populate the card's drafts whenever settings reopen. Two new
  render functions (`render_worktree_base_section`,
  `render_worktree_location_section`) draw a branch-search field with a "Use
  Primary" shortcut, and a location field with a `cx.prompt_for_paths`
  "Choose..." folder picker and clear-to-restore-default. Both gated
  `.when(card.is_git, ...)`, matching the Swift reference.
- `rust/crates/tiller/src/main.rs`: `refresh_sidebar` now also calls
  `sidebar_project_worktree_defaults()` (new function, reads
  `ProjectCatalog::project_settings` per project) and feeds each project's
  saved defaults into the sidebar, so a reopened settings sheet seeds from
  persisted state rather than blank fields. `update_project_settings` now
  takes the two fields from the incoming `update` (which always carries the
  card's full current draft) instead of re-reading old persisted values — the
  prior code path would have silently discarded every edit to these fields
  had the UI existed, since `update` never had them.

**Regression tests** (`rust/crates/tiller_ui/src/sidebar.rs`):
`worktree_base_and_location_fields_are_drawn_only_for_git_projects`,
`typing_worktree_base_and_location_emits_a_durable_update`, and the
clause-verbatim `worktree_base_and_location_persist_across_reopen` (opens
settings, applies `set_project_worktree_defaults` exactly as `main.rs`'s
`refresh_sidebar` would on a real host, types into both fields, closes,
reopens, reads the card's own draft fields back). All three pass:
`cargo test -p tiller_ui --lib worktree_base` → `test result: ok. 3 passed`.

**Red-run proof** (there is no "just revert" here — the tests are new code
right alongside their target): spliced only the three new test functions
onto `git show HEAD~1:rust/crates/tiller_ui/src/sidebar.rs`'s pre-fix content
(the commit immediately prior to `2d66b3ce`) via a small Python text-splice
script, ran the same `cargo test` filter against that tree, and got 11
compile errors — `E0609`/`E0599`, "no field `default_worktree_base` on type
`&ProjectSettingsUpdate`", "no method named `set_project_worktree_defaults`
found for mutable reference `&mut Sidebar`", etc. — confirming the feature
was genuinely absent, not just untested.

**My own live re-drive this pass** (wayland lane, fresh scratch git project,
label `wf-fix2`, fresh scratch sqlite DB): right-clicked the project row,
opened Project Settings, typed `release` into Default Worktree Base and
`/tmp/wf-fix-custom-location` into Worktree Location through the real text
fields, closed the sheet, reopened it — both values were still shown in the
fields. Independently confirmed via a direct read-only query
(`sqlite3.connect('file:/tmp/wf-fix2.sqlite?mode=ro', uri=True)`) against the
live on-disk database that both exact typed values are persisted under the
correct project id — not merely redrawn from in-memory sidebar state that a
process restart would lose.

**Remaining gap for a fresh critic:** the "Use Primary" shortcut button and
the folder-picker "Choose..." flow (`cx.prompt_for_paths`) were built and are
exercised by the drawn unit tests, but my own live drive only exercised the
text-field typing path, not a real click on "Use Primary" nor a real
platform folder-picker round trip through `cx.prompt_for_paths` (GPUI's
picker is host-OS native and awkward to drive headlessly in the wayland
lane). A fresh critic should live-drive both: click "Use Primary" and
confirm the field snaps to the primary branch name; click "Choose...",
select a folder in the real picker, and confirm the location field updates
to that path and it persists across a reopen the same way the typed path
did.

---

## Summary

| Row | Verdict this pass | Fix commit |
| --- | --- | --- |
| `F-CHAT-33` | half-proven | `83be7a14` (pre-existing; re-confirmed live) |
| `F-SID-19` | half-proven | `407f7c9b` (pre-existing; re-confirmed live) |
| `F-TERM-10` | half-proven | `984defa7` (pre-existing; re-confirmed live) |
| `F-PRJ-13` | half-proven | `043b1e71` (pre-existing; re-confirmed live) |
| `F-PRJ-17` | half-proven | `2d66b3ce` (built this pass) |
| `F-PRJ-18` | half-proven | `2d66b3ce` (built this pass, same commit as F-PRJ-17) |

All six rows are `half-proven`, not `PASSED` — I do not get to pass my own
work. Every row above has a named, specific gap for the fresh critic that
drives it next; none of the gaps are "re-run what I already did" — each
targets a control path or scenario I did not personally exercise live this
pass.

Full `cargo test -p tiller_ui --lib` and the F-TERM-10/F-SID-19 fix commits'
own `cargo test -p tiller --bin tiller` runs stay at their pre-existing
baselines (documented per-row above); no new failures introduced by this
pass's build (`cargo build -p tiller` clean, only pre-existing unrelated
dead-code warnings).

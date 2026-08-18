# FINISH — settings shard (F-SET-*, F-CORE-SET-*, F-PER-*)

Fresh finish-line critic pass, run **2026-08-18** on the box described in `ENVIRONMENT.md`'s
2026-08-18 top section (x86_64, 12 cores, COSMIC/wayland-1, AMD GPU). I built none of this. Every
row below was re-driven today under label `fin-set2` (an earlier `fin-set`/`fin-set-tall`/
`fin-set-noagt`/`fin-set-nopath` set of instances from earlier **today**, same pass, is also cited
where it captured a state `fin-set2` didn't re-touch — all timestamps below are today's).

## Setup

```bash
export TILLER_WL_LABEL=fin-set2
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm
cp rust/target/debug/tiller /tmp/fin-set2-tiller
export TILLER_WL_BIN=/tmp/fin-set2-tiller
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
```

DB is `/tmp/fin-set2.sqlite` (also `/tmp/fin-set.sqlite`, `/tmp/fin-set-tall.sqlite`,
`/tmp/fin-set-noagt.sqlite`, `/tmp/fin-set-nopath.sqlite` for the earlier-today instances) —
`wayland-drive.sh` reuses the same DB file across separate invocations sharing a label, which is
what makes the persistence rows (F-PER-*, and the DB half of several F-SET-* rows) driveable: each
invocation is a genuine fresh process against the same on-disk state. Verified directly with
`python3 -c "import sqlite3; ..."` against the `setting` table (no `sqlite3` CLI on this box).

Section ids used with `ctl surface.settings.select section=<id>`: `ai-providers`, `agents`,
`general`, `permissions`, `appearance`.

## Headline finding this pass: two settings that persist but do nothing live

**F-SET-22** (agent accent-color picker) and half of **F-SET-18** (per-agent install states) looked
right in every screenshot but fail the evidence standard's conjunction trap on inspection:

- `grep -rn "agent_colors\[" rust/crates --include=*.rs` finds exactly two call sites, both in
  `tiller/src/main.rs`, both only saving/loading the array to/from `session_store` — **zero
  consumers that render it**. `settings.rs`'s own doc comment at the call site (`main.rs:2746`)
  states this in as many words: the picker is "a separate table for a separate job" from
  `tiller_theme::AgentBrandColor`, which is what `WorktreeStatusGlyph` and tab icons actually read.
  Live proof: picked purple for Claude Code in the picker (visible selection ring, screenshot
  `zoom-swatches`-equivalent this pass), then opened the worktree's Chat tab and zoomed the tab
  icon and the status-bar Claude glyph — both stayed the original coral (`fin-set2-accent/zoom-tab.png`,
  `zoom-statusbar.png`). The picker persists (named test
  `agent_color_click_selects_a_new_accent_and_persists` passes, and DB round-trip below confirms
  it), but "start/show that agent, and confirm its accent color changes" has no code path at all.
- `grep -n '"Retry"' rust/crates/tiller_ui/src/settings.rs` returns nothing, and no
  `Unsupported`/`update_to_latest` machinery exists anywhere in `tiller_ui`/`tiller_project` for
  per-agent rows. F-SET-18's clause asks for "an in-progress install, Update to latest, failed
  Retry, unsupported" states in addition to Install/unavailable-binary — only the latter two exist.

Both are marked `half-proven` below rather than carrying forward the ledger's `PASSED`.

## F-SET rows

**F-SET-01 — navigate categories.** PASSED. Live real clicks (not socket) at each of the 5
sidebar rows (AI Providers/Agents/General/Permissions/Appearance); each click rendered the correct
detail view and moved the highlighted-row state. `fin-set2-nav/02..06`.

**F-SET-02 — Back/Escape return to workspace.** PASSED. Live click on "← Back" (44,51) returned to
the workspace (terminal pane, sidebar, tab strip — 15693 colours matching the pre-Settings
baseline's 15684); reopened via `surface.settings.open`, pressed `Escape`, same workspace frame
returned (15693 colours again). `fin-set2-nav/07,09`.

**F-SET-03 — version + Check for Updates.** PASSED. Version "0.1.0" renders in About; no Check for
Updates control anywhere in the General page (confirmed both visually, `fin-set2-general2/03..07`,
and by re-reading `settings.rs:3465`'s absence assertion `general_settings_state_the_version_and_no_dead_controls`
— still true today). No updater transport exists for Linux; matches N/A-by-design.

**F-SET-04 — resume agent sessions toggle + persistence.** PASSED. Live click flipped
`resumeAgentSessions` true→false (screenshot + `ctl` echo); DB-backed restart (`fin-set2-persist1`,
a genuinely new process) read back `resumeAgentSessions:"false"` with zero further clicks.

**F-SET-05 — auto-rename toggle + summarizer picker.** PASSED. Live: toggled Auto-rename on,
clicked the Summarizer picker and it opened a real dropdown (Claude Code ✓ / Codex / OpenCode / Pi
— `fin-set2-general/05-summarizer-picker-open.png`); named test
`summarizer_picker_is_gated_on_auto_naming_and_selects` (passes today) covers the disabled-when-off
half I didn't re-click live. DB persisted `autoNaming:"true"` across a fresh-process restart.

**F-SET-06 — chat retention toggle + stepper + persistence.** PASSED. Live stepper clicks
100→97 (`fin-set2-general2/03-retention-97.png`); DB persisted `chatRetention:"97"` across restart.
Zero-as-unlimited is vacuous per prior finding (stepper floor is 5, unchanged today) — not
re-disputed.

**F-SET-07 — mounted-worktree limit toggle + stepper + persistence.** PASSED for the setting
itself: live toggle-on + stepper 6→7 (`fin-set2-general2/04,05`), DB persisted
`limitMountedWorktrees:"true"`/`mountedWorktrees:"7"` across restart. Did not re-drive the actual
mount/unmount eviction behavior this pass (already separately tracked as absent under
F-CORE-ACT-26 per the row's own prior text — out of this row's scope, not re-opened).

**F-SET-08 — control-socket toggle + Copy install command.** PASSED. Live click turned the toggle
off; the **next real process launch** (fresh `wayland-drive.sh` invocation, same DB) printed
`[control] disabled` to its own log and created no `.sock` file — a UI-driven version of the
control's own negative control, strictly stronger than an env-var test. Edited the DB value back to
`true` (`sqlite3`-equivalent via python3) to keep driving; the following relaunch printed
`[control] listening on ...` again. "Copy install command" confirmed absent from the rendered page
this pass (`fin-set2-general2/06` and earlier-today `fin-set-tall2/02-general-install-skill-clicked.png`,
taller viewport, same absence) — matches the absence test named in the ledger.

**F-SET-09 — Install Skill.** PASSED. Earlier-today capture (same pass, `fin-set-tall2/02-general-install-skill-clicked.png`,
a taller 1400×2200 nested output that fits the whole General page without scrolling — this box's
narrower 1715×972 output would not scroll past "Agent Skill" for me this pass, see Known gaps)
shows a live click on "Install Skill" producing "Installing… running in a new terminal tab."
immediately below the button.

**F-SET-10 — usage visibility, refresh interval, refresh now.** PASSED. Earlier-today captures
(`fin-set-shots7/02-claude-usage-off.png` → `04-claude-refresh-clicked.png`) show live: Claude's
"Show in usage bar" toggle off, refresh interval stepper 5→7 min (both Claude and Codex rows moved
together, correct shared-interval behavior), and a "Refresh now" click.

**F-SET-11 — usage provider states.** PASSED (machine tier). `cargo test -p tiller_usage --test
usage_tests` — 10/10 pass today, re-run fresh: `not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path`,
`logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`,
`error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`,
`data_old_enough_to_count_as_stale_is_marked_stale_not_current`,
`the_fetch_is_bounded_and_single_attempts_do_not_hang`, plus 5 more — each spawns a real PTY
subprocess (not a transcript stand-in) under an isolated PATH. Together they cover every named
state in the clause (Reading/Active/Stale/Not found/Logged out/Timed out/Error).

**F-SET-12 — OpenCode Go cookie + workspace override.** PASSED. Earlier-today captures
(`fin-set-tall5/02..06`) show live: cookie typed → Save → status flips Signed in; Workspace ID
override typed; Clear on both fields → OpenCode Go reverts to Not signed in and "Show in usage bar"
auto-flips off (screenshot pair `03-opencode-cookie-saved.png` vs `04-opencode-cookie-cleared`-equivalent
frame, confirmed today).

**F-SET-13 — Ollama Cloud cookie + refresh.** PASSED. Earlier-today captures show cookie field
click/type/save cycle and a Refresh click against the real ollama.com endpoint (consistent with the
ledger's own live-network refresh-error finding, not independently re-hit against the network this
pass).

**F-SET-14 — Add Account / cancel / re-authenticate / remove.** half-proven. Live, twice: clicking
"Add Account" on the Codex row genuinely spawned `x-terminal-emulator -e codex login` (confirmed by
host `ps`, pid distinct from the Tiller process) showing a real `auth.openai.com/oauth/authorize`
URL and a local `localhost:1455` callback server — the login flow is real, not simulated. The
in-app "Signing in… / Cancel" render is proven by a **named** test today,
`add_account_in_flight_renders_signing_in_and_cancel` (passes) — but I could not land the live
Cancel click myself: the spawned `x-terminal-emulator` window tiles next to Tiller under this
lane's sway config, which reflows Tiller's own content into a narrower column, and by the time I
recomputed the Cancel button's new on-screen position twice, both attempts either missed or hit the
terminal window instead (`fin-set2-cancel3` — the Codex row still showed plain "Add Account" in
the post-click frame despite the process staying alive). The kill-on-cancel code path
(`cancel_account_login`) was not independently exercised by a landed click this pass — flagging
this half honestly rather than reusing the ledger's `~1.6s` cancel-latency claim. Re-authenticate
and Remove were not exercised this pass (would need a completed real login).

**F-SET-15 — system-default account selection.** UNREACHABLE, re-confirmed today. Read
`settings.rs:2445` fresh: exactly one `controls::account_row("System default", …, true, theme)`
call per provider card, with a comment stating the `true` "stays true by construction, not because
anything was checked at render time." No second account row, no selection state, anywhere in the
file. Same finding as the standing verdict.

**F-SET-16 — search + refresh agent registry.** PASSED. Earlier-today capture
(`fin-set-tall2/04-agents-search-empty.png`) shows typing `zzzznonexistent` into Search agents
genuinely emptying the 5-row list (a real filter, not a static list) — matches the positive control
in `03-agents-search-claude.png` (same session) showing all 5 rows with a live "Refreshed 11:39:52"
timestamp.

**F-SET-17 — registry error + retry.** PASSED. Earlier-today negative control
(`fin-set-nopath2/02-agents-nopath-error.png`, a separate instance launched with PATH unset) shows
the live banner "⚠ Could not load the agent registry: PATH is not set in the environment" — a
genuine failure state, not fabricated (the same code path that lists 5 real rows when PATH is set,
per F-SET-16's capture from the same session).

**F-SET-18 — install/update/retry/unsupported/unavailable states.** half-proven. Live negative
control (earlier-today `fin-set-noagt1/02-agents-no-path.png`, all 5 agent binaries excluded from
PATH): every row shows "Not found on PATH" + a real "Install" button — the unavailable-binary state
is genuine. But `grep -n '"Retry"' settings.rs` and a search for `Unsupported`/`update_to_latest`
across `tiller_ui`/`tiller_project` (today) both return nothing for agent rows — "in-progress
install", "Update to latest", "failed Retry" and "unsupported" states named in the clause have no
implementation to exercise. Install-click itself is proven by the named test
`agent_install_click_reaches_host_and_confirms_on_the_row` (not independently re-clicked live this
pass; the no-PATH capture used the button's rendered presence, not its click outcome).

**F-SET-19 — theme System/Light/Dark.** PASSED. Live real clicks on all three segments; the whole
window genuinely recolored each time (`fin-set2-appear/02-theme-light.png` — full light palette,
segmented control moved to "Light" — vs the dark baseline). Discriminating: not a socket-only
switch.

**F-SET-20 — translucency + font steppers.** PASSED. Live: Translucency toggle click (visibly
moved, colour changed to filled); Interface font stepper two ups (13→15pt) then one down (→14pt),
Terminal font stepper one up (13→14pt) — all four values changed on-screen exactly as clicked
(`fin-set2-appear/05..08`).

**F-SET-21 — Files icon theme.** N/A — platform, re-confirmed today. Live click on the single
"Material" button produced a pixel-identical frame (`fin-set2-appear/09-fileicons-clicked.png`),
no menu — matches the census: Linux ships exactly one file-icon theme, nothing to pick between.

**F-SET-22 — per-agent accent color.** half-proven. See "Headline finding" above: DB persistence
of the swatch selection is real and proven by both a named test and today's own restart round-trip;
the "confirm its accent color changes" live half is genuinely absent — `agent_colors` has no
rendering consumer anywhere in the tree.

**F-SET-23 — refresh system permissions / status+action.** N/A — platform, re-verified against the
Swift original today: `App/Permissions/SystemPermissionProbe.swift` covers `.notifications`,
`.screenRecording`, `.accessibility`, `.fullDiskAccess`, `.automation`, `.localNetwork` — all
resolved through macOS-only APIs (`UNUserNotificationCenter`, `CGPreflightScreenCaptureAccess`,
`AXIsProcessTrusted`, a `TCC.db` file check, AppleEvents error codes, System Preferences URL
schemes). None have a Linux equivalent; the Linux Permissions page correctly carries none of them —
confirmed live, its only content is "Browser origin grants" (F-SET-24).

**F-SET-24 — browser-origin grants + empty state.** PASSED. Full live round trip today: empty
state ("No browser origins have been granted") confirmed on a fresh instance; a real
`browser.navigate` to an unallowed origin produced a genuine `permission:requested` event,
`browser.permission action=allow` granted it, and Settings→Permissions then listed
`https://agent-target.example` with its own "Revoke" button; a live click on that Revoke button
removed the row, back to the empty state (`fin-set2-revoke/02-after-revoke.png`). The grant also
survived a genuine process restart (see F-PER-08).

**F-SET-25 — refresh permissions on activate.** N/A — platform, re-verified against the Swift
original today alongside F-SET-23: the activation-refresh clause exists only to re-poll the same
macOS TCC probes. Linux has none to refresh.

## F-CORE-SET rows

**F-CORE-SET-01 — settings boundaries + control-socket env override.** PASSED, closed twice this
pass. (1) Direct binary launches with `TILLER_SOCKET_ENABLE=off` print `[control] disabled` and
create no socket file; without the override they print `[control] listening on …` and the file
exists. (2) Stronger: the live UI toggle in F-SET-08 above flipped the **same** persisted setting,
and the very next real process launch against that DB reproduced the identical `[control] disabled`
line — the env-var path and the UI path both gate the same boundary correctly.

**F-CORE-SET-02 — permission state models TCC kinds.** N/A — platform, re-verified against
`App/Permissions/PermissionsModel.swift`'s Linux-side counterpart today (see F-SET-23): every
modeled kind is macOS-only. Confirmed, not just asserted.

## F-PER rows

**F-PER-01 — persist projects/worktrees/tabs/chats/scrollback.** half-proven. The schema for it is
real and live today: `session_ref` already carried 6 rows from ordinary tab creation in this pass's
DB, and `chat_turn`/`session_ref`/`tab_state`/`quarantine_record` tables all exist
(`/tmp/fin-set2.sqlite`, read directly via `python3`+`sqlite3` module). I did **not** land a
complete live chat turn this pass — two attempts to type into the composer and send did not reach
a running turn before the drive instance's teardown (composer stayed on the "Message…" placeholder
after the first attempt; the second used `TILLER_WL_KEEP=1` but I ran low on remaining budget
before re-attempting the send with a corrected click target). The worktree/tab/terminal/settings/
permission halves of persistence (F-PER-02/03/04/08 below) are independently proven live this pass
by the same restart mechanism this row also needs — leaving only the chat-turn-specific leg
unconfirmed today. Not carrying forward the ledger's sweep D-MAIN-4 evidence as this pass's proof.

**F-PER-02 — persist selected worktree + expanded project state.** PASSED. After a genuine
fresh-process restart (same DB), `linux/gpui-waku` was still the selected/expanded worktree with no
further clicks (`fin-set2-postper/03-terminal-tab-after-relaunch.png`).

**F-PER-03 — persist open worktrees + tab layouts.** PASSED. Same restart: all 4 tabs (Chat,
Terminal, Browser, Browser) that existed before the restart were present after it, in the same
worktree. Split-pane layouts specifically were not built this pass to re-test that sub-case.

**F-PER-04 — reattach saved scrollback on remount.** PASSED. Typed `echo PERSIST_TERMINAL_OUTPUT_55123`
into the Terminal tab, confirmed the echoed line on-screen, then did a genuine process
kill+restart against the same DB — the restored Terminal tab's scrollback showed the same
`PERSIST_TERMINAL_OUTPUT_55123` line (`fin-set2-postper/03-terminal-tab-after-relaunch.png`).

**F-PER-05 — Restore Previous Launch (closed-tab restore).** UNREACHABLE this pass — not reached.
The ledger's only evidence is "pass 3", pre-dating this project's two host migrations; per the
evidence standard that is not valid evidence here, but I ran out of remaining budget in this pass
to construct and drive the close-tab-then-restore sequence myself. Needs its own pass.

**F-PER-06 — flush state + terminate children on quit.** PASSED. Live: started a real job-control
`sleep 600` (own pid, confirmed via host `ps`) inside a terminal pane, then sent `SIGTERM` to the
Tiller process (the standard quit signal a window manager sends) — after 2s, host `ps` showed
**both** the Tiller process and the `sleep 600` child gone. Did not use an in-app "Quit" menu item
(this Linux build has no app-menu bar); SIGTERM is the equivalent real gesture.

**F-PER-07 — persist project icon/name/default-worktree settings.** UNREACHABLE this pass — not
reached. Did not touch the project-identity/icon editing surface this pass (out of the settings
surface proper; ran out of remaining budget to also cover it here). Needs its own pass or the
sidebar/project-identity shard's coverage.

**F-PER-08 — persist browser-origin grants + general settings.** PASSED, closed with unusually
strong evidence this pass: across **three separate genuine process restarts** against the same DB,
the following all survived every time with zero further interaction: `chatRetention` (100→97),
`autoNaming` (false→true), `limitMountedWorktrees`/`mountedWorktrees` (false/6→true/7),
`controlSocketEnabled` (true→false→true, see F-SET-08), `translucency` (false→true),
`interfaceFontSize`/`terminalFontSize` (13→14), `resumeAgentSessions` (true→false), and the browser
origin grant for `https://agent-target.example` (present with its own Revoke control after
restart, `fin-set2-postper/04-permissions-after-relaunch.png`).

## Known gaps / lane notes for the next pass

- This box's nested-output default (1715×972) would not scroll the General settings page past
  "Agent Skill" for me (`scroll` actions at several coordinates and signs had no visible effect) —
  I fell back to an earlier-today capture from a taller 1400×2200 nested output for F-SET-08/09's
  bottom-of-page content. Worth checking whether `scroll` needs a different target element inside
  that specific panel, or whether the taller-output workaround should become the documented route.
- F-SET-14's Cancel click and F-PER-01/05/07 are the load-bearing gaps left for a follow-up pass;
  none were silently dropped.
- The spawned `x-terminal-emulator` (Add Account, Install agent flows) tiles next to Tiller under
  this lane's sway config and visibly changes Tiller's own window width/reflow — any recipe that
  clicks a coordinate on Tiller *after* one of these spawns needs to re-screenshot first, not reuse
  pre-spawn coordinates.

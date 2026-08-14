# Wave A slice W02-core — 6 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CORE-AUTH-01` — ledger line 408, currently **half-proven**

- **Triage says:** exercise
- **Approach:** AgentAccountIdentity::parse_claude_json (tiller_usage/src/account.rs:52) is genuinely called from production at tiller_ui/src/settings.rs:548, parsing real `claude` CLI stdout. The prior drive confirmed a real OAuth spawn + PKCE URL but stopped before completion. Gesture: carry the real login through to completion so claude writes real account JSON, and confirm settings.rs:548 parses it into a populated identity.
- **Evidence on record:** half-proven: Add Account live-confirmed to spawn a real claude auth login + genuine oauth/authorize PKCE URL (button not dead). parse_claude_json itself never exercised — flow deliberately aborted pre-completion. P120, 2026-08-14.

## `F-CORE-DOM-03` — ledger line 371, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** cx.prompt_for_paths (main.rs:6491) is the correct real GPUI/portal call; ENVIRONMENT.md:77 documents the portal file picker is Wayland-side and invisible to X captures even when open. Repeat the gesture from a lane with real portal visibility (host desktop session) or accept a human-hand confirmation per that doc's own guidance.
- **Evidence on record:** NOT EXERCISED (unchanged): Add Project click shows no dialog in nested compositor, but portal call could plausibly route to the real host desktop, invisible to this capture. Instrument ambiguity, not UNREACHABLE.

## `F-CORE-FILE-03` — ledger line 387, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** terminal_file_drop/classify_file_drop (tiller_project/src/file.rs) is a real, consumed data layer per DEAD-MODULES.md's correction (F-TERM-PTY-06 names it directly). ENVIRONMENT.md:76-78 documents XDND drags are unexercisable by xdotool/the current virtual pointer (no press/motion/release primitive) — needs a human hand or a different drive tool, not a code fix.
- **Evidence on record:** NOT EXERCISED (unchanged): not attempted this pass — real-desktop drag risk plus no press/motion/release primitive in the Wayland virtual pointer.

## `F-CORE-FILE-06` — ledger line 391, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** DEAD-MODULES.md's 'editor never subscribes' claim is stale: FileSystemEventMonitor is constructed per-file at file_view.rs:104, polled every 100ms by a real background task (file_view.rs:105-118,199), and drives editor.check_external() -> the F-EDIT-05 Reload/Keep banner. Gesture: open a file, externally modify/delete/rename it, wait for the poll, confirm the banner and dirty/conflict/deleted distinction — including once with local edits present, to hit the conflict branch.
- **Evidence on record:** NOT EXERCISED (unchanged): captures show the git Changes panel refreshing on external edits, not FileView's own dirty/conflict/deleted tracking. FileSystemEventMonitor lives only in file_view.rs; Changes panel is a different component. Report overclaim.

## `F-CORE-TERM-02` — ledger line 395, currently **half-proven**

- **Triage says:** exercise
- **Approach:** open_context_menu is wired only to MouseButton::Right (tiller_terminal/src/lib.rs:1446,1504); every named per-item effect already has test coverage from pass 12. WAYLAND-LANE.md confirms right-click needs the DISPLAY=:1 lane specifically. Run the drive there and confirm each menu item's real effect one at a time.
- **Evidence on record:** could-not-reach reconfirmed from this lane; existing half-proof (p17-rclick-term.png) unchanged. Independently verified: WAYLAND-LANE.md confirms right-click needs DISPLAY=:1, and tiller_terminal/src/lib.rs confirms open_context_menu is wired only to MouseButton::Right (no keyboard path exists anywhere in the crate). Per-item effects remain owed, reachable only on the DISPLAY=:1 lane.

## `F-CORE-USG-07` — ledger line 405, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Missing-credentials is the load_credentials() Err(_) branch (codex.rs:319, maps to LoggedOut) — gesture: point $CODEX_HOME at an empty/missing auth.json. Refresh-needed as this VERIFY describes doesn't require the still-unwired needs_refresh 8-day gate (USG-05's separate build item) — the reactive 401-refresh-retry path already exists; only its success case (not yet driven, only its failure case was) is missing, and that's the same gesture USG-05's merge-save-on-success needs.
- **Shared cause:** Same codex.rs pipeline as F-CORE-USG-05 and F-CORE-USG-06; its remaining gap is literally USG-05's merge-save-on-success gesture — drive once, close both.
- **Evidence on record:** half-proven: live "loaded" (real usage data all session) and "logged out" (fake $CODEX_HOME, confirmed live in status bar) states both observed. Missing-credentials and refresh-needed states not driven this pass.


# Wave A slice W01-core — 6 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CORE-ACT-02` — ledger line 338, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Sidebar half already proven. D-Bus half: register a pane via spawn (add_agent_tab, main.rs:4621) instead of the fragile wtype title-injection route, then tillerctl notify --session <pane-id> --status <status> and watch dbus-monitor. Note: tab_status's Chat branch (main.rs:3260-3270) never reads self.activity — architectural, not this row's defect.
- **Evidence on record:** sidebar half reconfirmed live for Terminal/pane-1 (hollow circle -> '?' glyph, discriminating, pixel-verified against baseline). Driver's companion claim that pane-0's notify call also marked the Chat tab is not supported: zoomed comparison of 04-06/03-08/04-09 against the 02-01 baseline shows Chat's icon is pixel-identical throughout. Source read confirms tab_status's Chat branch (main.rs:3261-3270) never consults s

## `F-CORE-ACT-06` — ledger line 342, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Ownership machinery (title_owned_panes/handle_title_change, tiller_activity/src/model.rs:187-344) is built and unit-tested. Blocked only by the broken wtype OSC-title injection gesture (unquoted, splits on bare ';'). Fix the injection command's quoting, then: identify a pane by title, change title to unrelated text, confirm title-owned clears; repeat on a process-identified pane and confirm it doesn't.
- **Shared cause:** Broken wtype OSC-title-injection gesture, shared with ACT-07 and ACT-11 (and half of ACT-02).
- **Evidence on record:** could-not-reach reconfirmed: unquoted printf OSC title command split mid-string on type (02-07-title-set-claude.png, likely a plain missing-shell-quote issue around the bare ';' rather than a wtype timing race as the driver speculates) — no title-owned identity registered, owed live gesture (title vs process ownership clearing) unexercised.

## `F-CORE-ACT-07` — ledger line 343, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Debounce logic already built (AgentSignalMerger-equivalent in model.rs). Notify half already reachable (see ACT-02). Title half blocked by the same broken title-injection gesture as ACT-06. Fix the gesture, then: send hook status, immediately force contradictory title, confirm hook status holds; repeat after >1.5s and confirm title wins.
- **Shared cause:** Broken wtype OSC-title-injection gesture, shared with ACT-06 and ACT-11.
- **Evidence on record:** could-not-reach reconfirmed: notify half of the 1.5s debounce race is reachable (see ACT-02), but the contradictory-recognized-title half needs the same broken title-injection gesture as ACT-06 — no timing result produced either direction.

## `F-CORE-ACT-10` — ledger line 346, currently **builder-claimed, unverified**

- **Triage says:** exercise
- **Approach:** panes::PROCESS_SIGNAL_INTERVAL=500ms and refresh_process_signal (panes.rs:22,60) are called from production at main.rs:2897, unit-tested for the interval value. Gesture: launch a native-binary agent, kill its child process externally, confirm process-owned status clears within ~1 tick.
- **Evidence on record:** Layer-D tick calls the bounded existing `/proc` walk with the pane shell PID every 500ms

## `F-CORE-ACT-11` — ledger line 347, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** All three ownership classes' clearing logic exists in model.rs (title_owned_panes/process_owned_panes/close clears both, lines 343-344). Blocked by the same broken title-injection gesture as ACT-06/07 for the title-owned leg. Fix the gesture, then exercise title, process, and spawn ownership on three separate panes together and confirm close/kill only clears the correct one's state each time.
- **Shared cause:** Broken wtype OSC-title-injection gesture, shared with ACT-06 and ACT-07.
- **Evidence on record:** could-not-reach reconfirmed: same title-injection blocker as ACT-06/07 plus process-owned precondition unattempted; driver correctly declined to credit the reachable spawn-owned class alone since the row's claim needs all three ownership classes exercised together.

## `F-CORE-ACT-25` — ledger line 361, currently **NOT EXERCISED**

- **Triage says:** reclassify
- **Approach:** Manifest verdict is NOT EXERCISED, but BootstrapRestoreOrder::partition (tiller_activity/src/bootstrap.rs:16) has exactly one caller workspace-wide, its own test — same shape as ACT-26's already-FAILED ids_to_evict. Flagging as reclassify per brief (not changing verdict): stronger evidence (reachability) points to FAILED — absent, resolving the manifest's own noted live-snapshot ambiguity. When built: startup/restore path must call partition with real open/selected worktree ids and mount priority before deferred.
- **Shared cause:** Same restart/restore-classification subsystem as F-CORE-ACT-24.
- **Evidence on record:** NOT EXERCISED (unchanged): single post-restart snapshot can't distinguish absent priority/deferred split from one resolving faster than sampled. Only unrelated finding: workspace.select doesn't persist across restart.


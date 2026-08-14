# Wave A slice W11-ctrl+brw+sid — 7 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CTRL-BROWSER-02` — ledger line 445, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** Evidence is pre-P90 (988d9e9); current code already returns surface/url/title and rejects missing url. Re-probe live; only real gap left is workspace-context/adapter-unavailable validation the clause also asks for, which is genuinely absent.
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. `browser.open` is genuinely implemented — main.rs:4601 calls `add_browser_tab` — so pass 6's "unsupported" reading is stale. Two defects against the clause: it **returns no surface identifier, URL or title** (the reply is unconditionally `{method

## `F-CTRL-CLI-02` — ledger line 451, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** resolve_tillerctl_path/install_tillerctl (main.rs:7868-7975) and its two call sites (4601, 5061) look fully wired into agent-tab creation. Launch the real app, inspect the installed symlink on disk, and drive a real agent to a status transition to confirm its own hook (not a manual socket client) reaches tillerctl.
- **Evidence on record:** P116 drove the compiled tillerctl binary manually against the socket: correct responses across project/workspace/panel/settings/changes/session/notification/capabilities, 2 malformed notify calls correctly rejected. Proves the CLI's protocol works but never inspects the installed XDG symlink nor invokes tillerctl from a real agent hook, both required by VERIFY.

## `F-CTRL-NOTIFY-03` — ledger line 442, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** Commit a5d09d7 already wired record_notification to notification_poster (the same notify-send path P120 already proved live for sibling rows F-CORE-ACT-19/20). Re-run the identical dbus-monitor capture against notification.create to confirm and flip.
- **Shared cause:** Same P100 fix (a5d09d7) as F-AUTO-06 and F-USE-06 (outside my group) -- all three still carry pre-fix evidence while the two sibling rows already re-verified fine.
- **Evidence on record:** **the posting conjunct never happens, and `create` reports success anyway.** Four conjuncts exercised live on the headless lane and all pass: `create` rejects a missing title (`"notification requires title"`) and a missing body, `list` round-trips both rows with exact title/body, `clear` empties it (`list` -> `[]`). The fifth — *"posts a user notification"* — is false twice over. **By construction:** `record_notifica

## `F-BRW-07` — ledger line 256, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Persisted-reload half is real and re-confirmed (main.rs:8161/:8234). Owed 'no new prompt' step is blocked: request_permission (browser.rs:588/:880) has zero production callers anywhere (ledger grades F-BRW-06 UNREACHABLE for exactly this). Once F-BRW-06's seam gets a real production caller, re-drive: navigate to the already-granted seeded origin and confirm no doorhanger.
- **Evidence on record:** Live-driven: seeded a distinct origin into the DB, confirmed empty first, started a brand-new Wayland process (new socket) -- Permissions section shows the seeded origin on first render, proving load_browser_origin_grants()/with_browser_origins (main.rs:8161/8234) reload persisted grants across a real relaunch. Independently re-queried the DB post-drive: 1 row, matches. Owed: the clause's 'trigger access, confirm no 

## `F-BRW-08` — ledger line 257, currently **half-proven**

- **Triage says:** exercise
- **Approach:** 'Revoke all' (settings.rs:3038-3056) is real, wired to Db::revoke_all_browser_origins via main.rs:8253-8255, gated on non-empty origins. Only the single-origin Revoke path has been driven live. Seed 2+ origins, click Revoke all, confirm empty-state card + DB re-query shows zero rows.
- **Evidence on record:** Live-driven: seeded an origin, opened Settings->Permissions, clicked the real rendered per-origin Revoke button -- card flips to 'No browser origins have been granted' (captures confirmed); DB re-query is consistent with the row's claimed 0-then-reseeded sequence. Owed: 'Revoke all' (settings.rs:3051, wired to Db::revoke_all_browser_origins at main.rs:8253) is real production code but was never clicked in this drive 

## `F-SID-12` — ledger line 81, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Fully wired end to end (context_menu_items sidebar.rs:694-708, dispatch main.rs:3209-3248). The ledger's blocker (wayland-virtual-pointer.c hardcoded to BTN_LEFT) is moot -- the command palette (main.rs:7079-7130) already reaches the identical code path via keyboard only, and P104 already proved xdotool right-click works on this project for sibling rows. No new files to fix, just drive it.
- **Evidence on record:** Re-confirmed independently (grep of wayland-virtual-pointer.c, read of WAYLAND-LANE.md): click is hard-wired to BTN_LEFT with no button parameter in the file; right-click is documented as not yet exercised on this lane and requires DISPLAY=:1, barred for this slice. No new gesture available; catalog half (set_primary_flips_the_application_level_marker) unchanged.

## `F-SID-18` — ledger line 87, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Built today (commit 7289c84, P110) with a drawn test that clicks the real New Terminal action and confirms a tab replaces the empty state. Ledger evidence is pass-8, long stale. Only the live pointer-click on the Wayland lane remains owed per P110's own report.
- **Evidence on record:** no "No Terminals" empty state


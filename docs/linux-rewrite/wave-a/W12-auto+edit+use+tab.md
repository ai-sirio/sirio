# Wave A slice W12-auto+edit+use+tab — 7 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-AUTO-06` — ledger line 280, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** record_notification (main.rs:717-744) already stores AND posts via notification_poster; ledger's 'empty dbus-monitor' evidence is dated after fix commit a5d09d7 and after P118-report.md's own successful live proof (preserved artifact reference/linux-progress/p118-notify/dbus-monitor.txt shows a real Notify call). Re-run the same dbus-monitor gesture against a fresh build to confirm before scheduling any build work.
- **Evidence on record:** socket create/list/clear round-trip re-confirmed live 2026-08-14, including both validation failures. The delivery conjunct's premise is **stale**: it inherited `F-USE-06`'s "zero app callers", which is no longer true — a full delivery path exists (`should_notify` → `build_payload` → `post_desktop_notification` → `notify-send`). What is false is that **`notification.create` reaches it**: `record_notification` pushes 

## `F-AUTO-09` — ledger line 283, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** browser_request_error() (main.rs:200-236, added by commit 988d9e9 at 04:29:51) now rejects the 7 non-open/navigate/act browser.* methods with an explicit unsupported-method error before queuing — exactly what VERIFY accepts. Ledger's probe evidence is timestamped 03:02:56, 86 minutes before this fix landed. Re-probe against a fresh build.
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. Exercised live over the control socket 2026-08-14 03:02:56 (no display needed). The clause accepts either "browser changes/results" **or explicit unsupported errors** — Linux returns **neither**. All eight methods I called (browser.get x2, screen

## `F-EDIT-08` — ledger line 226, currently **FAILED — defective**

- **Triage says:** exercise
- **Approach:** add_file_tab's dedup logic (main.rs:4331-4361) already implements the row's real behavioural clause correctly (focuses existing tab, no duplicate). The reported failure is that ctrl-o's native file picker (cx.prompt_for_paths) never visibly appeared across 4 attempts. P118-report.md proved the same API reaches the D-Bus portal for a sibling flow (Add Project) without ever rendering a visible dialog. Re-test with dbus-monitor watching org.freedesktop.portal.FileChooser instead of waiting for a rendered window; only escalate to build if no request fires at all.
- **Evidence on record:** P104 §Group 2: ctrl-o on an already-open README produced no picker or visible focus/open-state change in four focus contexts.

## `F-EDIT-12` — ledger line 230, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Code already correct per P81 (changes.rs:651/992, tiller_terminal/lib.rs:1187 implement the (PathBuf,String) drag payload). Blocked purely by this Wayland lane's virtual-pointer tooling having no button-down-only/motion-while-held primitive. Needs an X11 lane (DISPLAY=:1) with a real button-down/motion/button-up sequence.
- **Evidence on record:** Source-verified: wayland-virtual-pointer.c parses only move and click ops; click is hard-coded move+press+release with no button-down-only/motion-while-held primitive anywhere in the binary -- a drag cannot be composed on this lane's tooling. Matches WAYLAND-LANE.md's documented limitation (drag needs DISPLAY=:1/X11). Stays NOT EXERCISED on this Wayland-only assignment.

## `F-USE-03` — ledger line 266, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Loading/Stale states are simply undriven live (no code gap — ProviderUsageState and status_bar's own unit tests already model/assert the distinct states). Drive a slow/delayed refresh for Loading, and past the staleness window for Stale. For the unavailable half, relaunch Tiller itself with env -u PATH rather than editing PATH post-launch.
- **Shared cause:** Shares F-USE-02's broken PATH-strip precondition — PATH is captured at process exec, so stripping it on an already-running Tiller has no effect; any retest needs a fresh launch with a pre-scrubbed PATH.
- **Evidence on record:** half-proven: live loaded + logged-out states confirmed in the bar. Loading/stale untested. Report overclaim found: cited screenshot does NOT show "Not found on PATH"/available:false — both providers still read "Signed in" post-PATH-strip; only the account email vanished.

## `F-USE-06` — ledger line 269, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** Ledger's cited defect (register_agent_id has zero production callers for restored panes) matches P100's diagnosis, but commit a5d09d7 (same commit that fixed F-AUTO-06) added register_restored_agent (main.rs:7567-7574) and wired it into BOTH restore_tabs (7608) and restore_tabs_in_workspace (7734) — exactly what P100 asked for. Re-verify live: open agent tab, quit, relaunch, background the restored pane, drive a transition, confirm notify-send fires (note app_active is hardcoded true, so the pane must be non-active or should_notify suppresses it).
- **Evidence on record:** **"zero app callers" is stale — it is now built end to end and still never fires.** `post_activity_notification` in `crates/tiller/src/main.rs` calls `NotificationPolicy::should_notify`, then `activity.build_payload`, then `post_desktop_notification`, which shells out to `notify-send` (installed and working since 2026-08-14). So the delivery path exists. It is unreachable because the same function early-returns unles

## `F-TAB-25` — ledger line 141, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Code and two drawn tests for Attach to Current Terminal exist in the tree today (main.rs:5823-6030+, tests at :11973/:12012), built same-day by P110/codex12 (commit 7289c84, 17:32) — the ledger's 'FAILED — absent... pass 14' text is byte-identical across every ledger revision since pass 14, including the 22:39 sweep this manifest was drawn from, so it was never re-checked after the build landed. Remaining owed work is only the live right-click exercise, itself blocked on the shared tab-bar popover fix.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** still no attach-to-terminal code; P65 built Move-to-Pane (a different feature) and explicitly refused this one (assigned-but-absent recheck, pass 14)


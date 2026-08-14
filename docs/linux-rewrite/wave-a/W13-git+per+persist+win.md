# Wave A slice W13-git+per+persist+win — 4 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-GIT-RUN-02` — ledger line 484, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** run_streaming (tiller_git/src/git.rs:107) is real, tested, and has a genuine caller in clone.rs:33. P120-report.md already drove this live: the New Worktree form opens and accepts typed text, but wtype cannot deliver Return/Escape at all in this app on this compositor (reproduced on an unrelated field too) — pure instrument limit. Needs a driver that can deliver a real Return keypress (X11 xdotool or similar).
- **Evidence on record:** NOT EXERCISED (unchanged): New Worktree form live, text lands, but wtype cannot deliver Return/Escape (4 methods tried, reproduces on an unrelated field too) — instrument limit, not UNREACHABLE. Progress streaming never reached.

## `F-PER-08` — ledger line 244, currently **half-proven**

- **Triage says:** exercise
- **Approach:** save_browser_origin_grant (main.rs:4578) is real and wired correctly; the general-settings half is already live-proven. The browser-origin half is gated on the embedded browser rendering real page content and firing a JS permission-prompt event, which this Wayland lane's chrome-only embedded browser can never do (WAYLAND-LANE.md). Needs an X11 lane where the browser can actually render a page that requests a permission.
- **Evidence on record:** General-setting half driven live and proven: 'Auto-rename tabs and agents' toggle clicked OFF->ON (02-pre-click-1715.png -> 03-post-click-1715.png), full process kill + fresh relaunch (corroborated by a freshly-recreated baseline screenshot showing a from-scratch self-seeded project list), toggle still ON after relaunch (02-after-relaunch-general.png) -- genuine restart roundtrip. Browser-origin half still not reacha

## `F-PERSIST-DB-11` — ledger line 514, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Schema-creation half is now strongly proven (real file, v1->v12, matches 12 migrations). No existing test plants data at an old schema version in chat_turn/session_ref/tab-ordering columns and checks it survives forward migration through the renames/backfills those specific migrations perform. Needs a new test doing exactly that.
- **Evidence on record:** Live v1->v12 migration boot (real file, not fresh TempDir) confirmed full schema/table creation (source-verified: migrations.rs lists 12 migrations, matching observed user_version=12) -- stronger than the prior fresh-TempDir-only test. Data-preservation half still not settled for the fields these migrations actually manage (ordering/rowid, chat/session fields, renamed legacy fields, browser content). The driver's mar

## `F-WIN-06` — ledger line 58, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** Ledger evidence cites an empty NewBrowser match arm at main.rs:4215; the real dispatch function open_action (main.rs:4639+, subscribed at 2336) has a non-empty NewBrowser arm at 4663-4665 calling add_browser_tab. git log -S shows this arm already present at the c63378e recovery-commit baseline (2026-08-12), predating the pass-14 evidence. Re-verify the ⇧⌘L/⌘L chords against a current build.
- **Evidence on record:** user ruling 2026-08-13: browser feature is IN scope — "niente webview" bans an Electron-style shell, not a web engine behind the browser surface; the pass-8 N/A was wrong. Browser tab absent: `NewTabAction::NewBrowser` is an empty match arm (main.rs:4215) yet the production menu still offers "New Browser" (tab_bar.rs:561) — a live entry that does nothing, with no feedback (frame stage-menu-4.png shows it in the open 


# Wave C slice C-U — 9 rows

**No file map** — triage never named files for these rows. **Do not edit code at all.** Investigate and exercise them as they stand, and report what a fix would touch.

## Files you own

_none — do not edit code._

## Rows

### `F-PRJ-07` — ledger line 100, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** half-proven (unchanged). This drive could not even reach the already-proven invalid-URL failure state due to the same anchored-popover input-delivery gap documented for F-PRJ-06/09 -- not a regression, the prior proof stands. Correct-URL retry arm remains untried.

### `F-PRJ-14` — ledger line 107, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** half-proven (unchanged label, narrower half owed). Favicon-domain arm now proven end-to-end: typed example.com, Use Favicon, 'Current: favicon for example.com' appeared live, sidebar switched to green Globe glyph after Close, persisted across a restart. PNG-upload arm remains unexercised: Choose PNG... opens a native file dialog with no synthetic-input path on this headless-Wayland lane (no portal service) -- a lane limitation, not a defect, same class as F-BRW's embedded-browser gap.

### `F-CHG-18` — ledger line 209, currently **NOT EXERCISED**

- **Files triage named:** —
- **Evidence on record:** NOT EXERCISED (unchanged): independently reconfirmed no drag primitive exists on the Wayland lane (wayland-drive.sh vocabulary is click/move/type/key only) and the only route (DISPLAY=:1) is barred to this slice; drag source/target code on both ends verified real and non-test.

### `F-CHG-22` — ledger line 213, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** half-proven (unchanged; driver's claimed expand not shown): all 3 cited frames, cropped+zoomed to the driver's own activity-box geometry, show an identical collapsed chevron with zero status rows -- the cited AE deltas (1799/219) are explained by a mouse cursor entering frame, not by any expand/collapse content change. A second, earlier same-session attempt shows the identical collapsed-both-ways pattern. Visual half (expanded section with running/needs-input/done trio) remains unproven; data-tier half from the pri

### `F-BRW-07` — ledger line 256, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** half-proven (unchanged). Critic re-confirmed via independent grep: request_permission (browser.rs:588/880) has zero production callers, only #[test] sites (1666/1677) — matches F-BRW-06 UNREACHABLE. Persisted-reload half (main.rs:8161/8234) stands proven; owed half genuinely has no caller to drive.

### `F-SET-11` — ledger line 299, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** Reconfirms existing half-proven verdict unchanged: Loaded state (Claude 49%/77%, Codex 100%) stands. No new drive this pass, source-only. Claude's remaining 3 states genuinely blocked by login_shell()'s dotfile re-sourcing (claude.rs:397), confirmed live-code. But the could-not-reach claim overclaims for Codex: status_bar.rs's CodexUsageFetcher (codex.rs:313-352) is HTTP+auth.json-based, not PTY -- CODEX_HOME isolation (already proven safe for F-CORE-USG-05/07 this sweep) reaches its LoggedOut branch with zero shar

### `F-SET-24` — ledger line 312, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** Reconfirmed, not newly driven: grep across all of rust/crates finds zero production callers of request_permission outside browser.rs's own tests; WAYLAND-LANE.md independently confirmed to document webview content as X11-only (chrome renders under Wayland, page does not). On-record empty-state capture (04-03-settings-permissions.png) independently viewed and confirmed genuine. Grant/revoke round trip remains owed to the X11 (DISPLAY=:1) lane, out of scope for this Wayland-only slice; verdict unchanged at half-prove

### `F-TERM-06` — ledger line 324, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** Re-confirmed (no new drive), verified by critic: same right-click-only binding as F-TERM-04, no ControlAction variant reaches CopyPaneId/CopyTerminalId. Existing half-proven basis (copy-ID items visible in p17-rclick-term.png) unchanged. Owed half (click each, read clipboard) unchanged.

### `F-CORE-USG-07` — ledger line 405, currently **half-proven**

- **Files triage named:** —
- **Evidence on record:** Independently reconfirmed codex.rs:319's Err(_)->LoggedOut branch and the status-bar captures: 'Codex 100% 5h' (normal) vs 'Codex logged out' (empty CODEX_HOME) -- a real, legible, positive-control-backed label difference. Refresh-needed/merge-save-on-success half (shared with F-CORE-USG-05) still owed.


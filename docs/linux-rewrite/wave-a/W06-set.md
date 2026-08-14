# Wave A slice W06-set — 2 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-SET-19` — ledger line 307, currently **half-proven**

- **Triage says:** exercise
- **Approach:** System theme-follow is real and built (Theme::init/follow_portal queries the XDG portal via ashpd and re-resolves). Drive: set desktop to Light, select System, confirm resolves light (not stuck dark-first-frame); repeat for Dark. Flag separately that the portal read is one-shot per selection, not a live D-Bus signal subscription -- a stronger 'follows live without re-clicking' reading would be a real, separate build.
- **Evidence on record:** Fresh independently-driven click (own coords off the live 1715x972 frame) reconfirmed: 02-16-fresh3 dark/System -> 03-17-fresh3-after-click light/Light, socket read-back theme:light. Verified 03-08/03-13 are genuine hover-only misses (cursor over Light, surface still dark/System selected) from stale-resolution coordinates, not failures. System-follows-desktop half still unexercised, unchanged.

## `F-SET-24` — ledger line 312, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Verified accurate, not stale: WAYLAND-LANE.md documents the embedded browser needs an X11 window handle (webview content is chrome-only under Wayland), and request_permission has no caller besides real page content. Drive on DISPLAY=:1: navigate to a permission-requesting page, grant, confirm it appears in Settings' Permissions card, Revoke, confirm it clears.
- **Evidence on record:** Live 04-03-settings-permissions.png proves the empty state genuinely: card, subtitle, 'No browser origins have been granted.', inert Revoke-all. Grant/revoke could-not-reach independently confirmed: WAYLAND-LANE.md:25 documents webview content not rendering under Wayland (chrome-only); grep shows request_permission (browser.rs:588) has no caller besides real page content and its own tests. Routes correctly to X11 lan


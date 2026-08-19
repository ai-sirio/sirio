# F-CHAT fix scope check — Browser and Changes through a restart

Team-lead's follow-up after the `centre-surface` fix (`e6b3816c`) and the
discriminating run (`bd0a19fe`): nobody had driven the Browser or Changes
tabs through a restart, so "no other surface was affected" was an argument
(the fix touches a wrapper shared by every pane kind, not chat-specific)
rather than a measurement. This is the measurement.

## Setup

- Fresh worktree `/var/tmp/tt-chat-971757-fixtureN`, a real git repo with a
  tracked modification (`README.md`) and an untracked file (`NOTES.md`), so
  the Changes tab has real content to render, not an empty diff.
- wayland-drive label `cdbgN757`, fixed binary
  (`/var/tmp/tt-chat-971757-target/debug/tiller`, i.e. `e6b3816c` applied).
- Host load 6–15 throughout (`uptime` checked before/after every drive) —
  quiet.

## Step 1 — fresh creation baseline

Created a Browser tab (`+` → `New Browser`) and a Changes tab (`+` →
`Changes`) via the real menu path. Both render their full content
immediately: `01-fresh-changes-tab.png` shows `Local changes (2)`,
`README.md` and `NOTES.md` listed with diff stats, `Unified`/`Split`
toggle, `Stage all`/`Discard all` — all present and correctly laid out.
(The Browser tab's own screenshot, not attached here, showed its
toolbar/address-bar chrome correctly sized; the embedded page itself hits
an unrelated, pre-existing nested-Wayland limitation — see note below.)

## Step 2 — the actual test: full restart, both tabs reconstructed via `restore_tabs`

Killed and relaunched the same label (same persisted DB, same binary).
`restore_tabs` runs to reconstruct both tabs from session state.

- Changes tab (the one that was active at restart): `02-restored-changes-tab.png`
  — same content as step 1, correctly rendered, no oversized-transcript-style
  collapse.
- Clicked over to the restored Browser tab: `03-restored-browser-tab.png` —
  toolbar and address bar correctly sized and positioned, same as fresh
  creation.

## Conclusion

Neither tab shows the empty-surface symptom after `restore_tabs` reconstructs
it. This is the expected result given the fix's mechanism — `centre-surface`
now computes its height fresh from `window.viewport_size()` on every render
of `columns()`, a node above and shared by every pane kind, not something
added inside the Chat branch — so there was never a chat-specific carve-out
to begin with; the fix's protection was always structural, not incidental.
This turns "no other surface was affected" from an argument into a
measurement, as asked.

**Note on the red banner in both Browser screenshots:** `Direct XCB build
failed: the window handle kind is not supported; XCB→Xlib adapter failed:
GPUI returned unsupported handle: Wayland(WaylandWindowHandle { ... })` is
identical on fresh creation and after restore — a pre-existing limitation of
embedding a browser surface inside this nested-Wayland test harness (no
native window-handle passing path), not a regression from `restore_tabs` and
not related to the `centre-surface` fix. The Browser pane's own chrome
(toolbar, nav buttons, address bar) is unaffected and renders correctly in
both cases; only the embedded page content is blocked by the harness
limitation.

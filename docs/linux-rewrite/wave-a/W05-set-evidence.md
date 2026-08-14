# W05-set evidence — Wayland lane (label `wavea-W05-set`)

HEAD under test: `4073297`. Captures under `reference/linux-progress/wavea-W05-set/`.

## `F-SET-12` — ledger line 300, currently **FAILED — absent**

**Approach taken:** re-drove live per triage (same-day build 7ae343d landed after the recorded
"no cookie UI or state" evidence). Opened Settings → AI Providers over the socket
(`surface.settings.open` + `surface.settings.select section=ai-providers`), forced the nested
output tall (1715x2400) to bring the OpenCode Go card into frame without a scroll primitive, then
drove the real gesture: clicked the "Session cookie" field at its actual on-screen coordinates,
typed a token through the persistent virtual keyboard, clicked Save, and forced a repaint.

**Observed, in order:**
- `04-tall-typed2.png` — click landed on the wrong y (script's `OUTPUT_W`/`OUTPUT_H` were stale
  from the `shot()` default 1715x972 while the output had been manually resized to 1715x2400 via
  raw `swaymsg`, so the pointer command's coordinate normalisation used the wrong denominator).
  Field stayed empty — a genuine miss, not a defect; corrected by setting `OUTPUT_W=1715
  OUTPUT_H=2400` in the action block before the next click.
- `05-tall-typed3.png` — corrected click at (650,1005) landed on the field: focus ring lit orange
  and masked dots (`••••••••••••••`) appeared as `TESTCOOKIE123` was typed — proves the field is a
  real, focusable, keystroke-accepting SecureField, not decoration.
- `06-after-save.png` — clicked Save (1220,1005). Three things changed in the same live frame,
  none of which the app would show on its own: the cookie field cleared back to its placeholder
  (`save_opencode_cookie`'s documented success path), OpenCode Go's Status flipped from grey "Not
  signed in" to green "Signed in", and its "Show in usage bar" toggle flipped ON by itself. This is
  a real write through `CredentialStore` reaching `LocalAccountState` (account.rs), driven through
  the same click→type→click path a user would use, not a socket shortcut.

**Claim:** exercised-working. Full loop closed: field renders, accepts real keystrokes, Save
reaches a real credential store, and the account-status/usage-bar-toggle UI reads the change back
live.

**Captures:** `reference/linux-progress/wavea-W05-set/02-tall-before.png`,
`04-tall-typed2.png` (the coordinate-bug frame, kept as a negative control),
`05-tall-typed3.png`, `06-after-save.png`.

---

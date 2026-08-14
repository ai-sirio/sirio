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

## `F-SET-13` — ledger line 301, currently **FAILED — absent**

**Approach taken:** same commit (7ae343d) built the Ollama Cloud provider card, cookie UI, bar
segment and fetcher per triage; re-drove live with the same corrected click→type→click gesture
used for F-SET-12 (coordinates recomputed from a fresh crop of the Ollama Cloud card since its
"Session cookie" field sits ~250px further down the page than OpenCode Go's).

**Observed:**
- `08-ollama-before.png` — Ollama Cloud card renders in full: Status "Not signed in", Show in
  usage bar OFF, Refresh interval, Session cookie field with placeholder, Save/Clear, caption
  text naming ollama.com's DevTools → Network → Cookie header — same pattern as OpenCode Go, a
  real card not a stub.
- `11-ollama-typed2.png` — click on the field at its real coordinates (650,1510) focused it
  (orange ring) and typing `OLLAMATESTCOOKIE` produced masked dots.
- `12-ollama-after-save2.png` — clicked Save (1220,1510): field cleared back to placeholder, and
  Status flipped from "Not signed in" to green "Signed in" (`12-status-crop.png`) — a real write
  through the same `CredentialStore`/`OllamaCloudUsageFetcher::COOKIE_KEY` path, driven by the
  same real click+type+click gesture a user would use.

**Claim:** exercised-working. Same shape and same real end-to-end result as F-SET-12.

**Captures:** `reference/linux-progress/wavea-W05-set/08-ollama-before.png`,
`09-ollama-typed.png` (first, mis-targeted attempt — kept as a negative control showing the field
still empty when the click misses),`11-ollama-typed2.png`, `12-ollama-after-save2.png`.

---

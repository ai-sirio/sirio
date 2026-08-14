# B4-settings — verdicts

Critic pass, independent of the builder (no builder reasoning was read — only its structured
report). Driven live on the Wayland lane (`Scripts/wayland-drive.sh`, real synthetic
click/type through `wlr-virtual-pointer`/`wtype`, screenshots forced-repainted via `grim`),
plus direct socket calls and, for `F-CORE-USG-05`/`F-CORE-USG-06`, a real background fetch
against the real `auth.openai.com`/`chatgpt.com` endpoints with synthetic (bogus) credentials.
Every instance used its own isolated `TILLER_DB`/`TILLER_SOCKET`, and `F-SET-12`'s round trip
used an isolated `TILLER_CREDENTIALS` scratch file — the real user's `~/.local/share/tiller/
credentials.json` was never touched.

**A significant environment hazard for whoever drives this row next:** clicking "Add Account"
on Claude or Codex spawns the machine's *real* `x-terminal-emulator` (here, `cosmic-term`) and
a *real* provider login. For Codex this opened a genuine `auth.openai.com` OAuth browser tab;
for Claude it opened a genuine Brave window that was **already authenticated** as the
operator's real account (org-picker screen, real org names) and, on one occasion, a Microsoft
sign-in page pre-filled with the operator's real saved credential autofill entries — the
spawned browser is *not* sandboxed from the operator's real profile. Every such flow in this
pass was killed before any organization/account was selected and before any credential was
submitted; no real login was completed. **Do not let one of these flows run to completion** —
kill the spawned terminal/browser process tree the moment the evidence you need is captured.

## F-SET-12 — OpenCode Go cookie UI — **PASSED**

Builder outcome: `already-correct` (no code change). Independently re-drove the full round
trip live today, against an isolated `TILLER_CREDENTIALS` file (not the real one), so this is
fresh evidence, not a re-read of the same-day P111 captures the ledger already had:

- Typed `auth=verify-B4-cookie-token-12345` into the session-cookie field, clicked **Save**:
  the exact string landed verbatim in the credential store (`cat` on the scratch file
  confirmed it byte-for-byte), Status flipped **Not signed in → Signed in**, **Show in usage
  bar** auto-flipped ON, and the field cleared back to its placeholder.
- Clicked **Clear**: credential store emptied (`{}`), Status flipped back to **Not signed
  in**, **Show in usage bar** auto-flipped back OFF.
- Typed `wrk_persisttest99` into the **Workspace ID override** field: survived navigating away
  (General) and back (AI Providers) in the same process, **and** survived a genuine
  fresh-process relaunch (same `TILLER_DB`, new PID, confirmed via `ps aux` before/after) —
  the value was still there, unprompted, on the freshly-started process's first paint.

All four transitions (signed-out→in, in→out, override persists across navigation, override
persists across relaunch) are away from default state, so none of this is "the default value
happened to match."

## F-SET-14 — Add Account in-flight affordance — **FAILED — defective**

Builder outcome: `built`. The **render half** of this row is genuinely new and works; the
**Cancel half** — the row's own stated purpose ("a login that will not finish… must be
escapable without alt-tabbing to the spawned terminal") — does not.

**What works, live:** clicking "Add Account" spawns a real subprocess chain — confirmed via
`ps aux` for both Claude (`x-terminal-emulator -e claude auth login` → real Brave OAuth) and
Codex (`x-terminal-emulator -e codex login` → real `auth.openai.com` authorize URL in a real
Brave window). The "Signing in…" label + Cancel button render immediately (a tight burst of 6
back-to-back `grim` captures taken with no artificial delay, ~tens of ms apart, all showed the
rendered state stably — this is not a sub-frame flash) with the pointer landing exactly on the
Cancel button at the predicted coordinates.

**What is broken, live and reproducibly:** clicking Cancel does not kill the real spawned
process. Tested three separate times:

1. Fast pair (≈250ms between Add-Account and Cancel clicks): process still alive at capture.
2. Fast pair with a 3rd capture at +300ms: process still alive; terminal's own text showed
   `Error logging in: Login cancelled` (the CLI noticing *something*), yet `ps aux` a further
   1s and 3s later showed the exact same PID still running.
3. **Generous timing** (2.5s delay before the Cancel click — an order of magnitude past any
   plausible pid-publish race — then `ps aux` polled every second for 8 full seconds after):
   the same `x-terminal-emulator` PID and its `codex login` child were alive and unchanged at
   every single poll, 8 for 8.

A manual `kill -TERM <same pid>` from an independent shell also did not visibly terminate the
process on demand (it was still running when checked a few seconds later), which points at
`cosmic-term` not honoring bare `SIGTERM` promptly — but from the row's own acceptance bar
("Clicking Cancel should close/kill that terminal") this is exactly the failure: on this real
desktop, clicking Cancel leaves the real login (and, for Codex, a real open OAuth browser tab)
silently running while the UI's own claim is that it has been stopped. This is a live,
repeated, generously-timed finding, not a one-off race — `cancel_account_login`'s own doc
comment calls the pid-not-yet-published case "a race no human click can realistically win";
this failure held even 2.5s after the render, well outside that race.

## F-SET-15 — single credential slot per provider — **half-proven** (unchanged)

Builder outcome: `not-built` (correctly deferred — a build-vs-N/A ruling, not a builder's to
make). Per instructions this row stays where it is; reconfirmed live anyway, incidentally,
across the F-SET-14 exercise above: across multiple real Add-Account clicks on both Claude and
Codex (both in-flight and after being torn down), every provider card showed exactly one row
under Accounts — "System default", `This device` + `Active` — before and after. No second row
ever appeared.

## F-SET-16 — Agents screen: Refresh timestamp — **PASSED**

Builder outcome: `built`. Live: opened Agents, captured `Refreshed 01:01:08` next to the
"↻ Refresh" button, waited, clicked Refresh, captured again: `Refreshed 01:01:16`. A genuine
8-second advance in the displayed clock value — not the construction-time default repeated,
and not inferable from a byte-identical capture.

## F-SET-20 — Translucency toggle — **FAILED — defective** (unchanged)

Builder outcome: `partially-built`. Confirmed live today that the fix landed exactly as
described and produces **no externally observable change**, so the row's functional defect —
the reason it was `FAILED — defective` on the ledger already — is unchanged:

- Clicking the Translucency toggle still flips its own local knob (visually ON in the
  capture).
- `surface.settings.select` for the Appearance section, queried immediately after the click,
  still contains **no `translucency` key anywhere** in its `values`/top-level payload — the
  same absence as before the click. The value still cannot leave the surface.

The builder's own report is candid about this ("self.changed() being called has no externally
observable effect yet, since the emitted snapshot is unchanged") and I can confirm that
assessment live rather than only by reading the diff: `self.changed()` now firing changes
nothing a user or an external caller (including the control socket) can observe. Since the
blocking defect named in the row's own prior verdict — "SettingsSnapshot has no translucency
field, so the value cannot leave the surface" — is reconfirmed present today, this stays
`FAILED — defective` rather than being promoted for touching the file.

## F-CORE-USG-05 — Codex proactive token refresh — **half-proven**

Builder outcome: `built`. Live-exercised the specific, previously-entirely-missing behavior
(`needs_refresh`'s 8-day gate had zero non-test callers): launched the real `tiller` binary
with `CODEX_HOME` pointed at a synthetic `auth.json` (bogus tokens, `last_refresh` backdated 9
days) and let its own background usage-fetch loop run unprompted (no click needed — it fetches
immediately on startup). Stderr showed, within seconds of launch:

```
[codex-usage] proactive token refresh failed: Expired
```

This is a **real** network round trip, not a canned result — a direct `curl` to
`auth.openai.com/oauth/token` with the same bogus refresh token independently reproduced the
exact `401` response body (`"code": "token_expired"`) that `classify_token_refresh_failure`
turned into `Expired`. Before this row, nothing in production code ever called
`needs_refresh`, so this exact log line could not have been produced by the pre-existing code
path — its presence is direct, live proof of a real new caller hitting a real endpoint.

**Not independently re-driven:** the success-path merge (`save_credentials_to`'s
merge-not-overwrite on a real 200 response) requires genuine valid Codex refresh credentials,
which are not available in this environment and were not fabricated. That half of the row
still rests on the builder's own (real local-HTTP-fixture, not mocked) test rather than an
independent live drive — hence `half-proven` rather than `PASSED`.

## F-CORE-USG-06 — Discarded refresh-failure classification — **half-proven** (unchanged)

Builder outcome: `needs-foreign-file`. Per instructions this row stays where it is; reconfirmed
live anyway. The same run above also exercised the **reactive** discard site this row is
about: after the proactive attempt failed and fell back to the (still-bad) on-disk token, the
subsequent real `fetch_usage` call was rejected, triggering the second, reactive refresh
attempt, which failed the same real way and logged, from the row's own new code:

```
[codex-usage] token refresh failed: Expired
```

confirming the classification is genuinely reachable through a live, non-mocked HTTP failure
in production code, not only in a unit test. A screenshot of the running app's status bar at
the same time still showed the plain, generic "Codex logged out" segment — no
Reused/Revoked/Expired-specific copy anywhere — confirming `model.rs`/`status_bar.rs` are
still untouched and the classification still does not reach the UI, exactly as the report
says.

## Summary

| Row | Verdict | Prior ledger verdict |
|---|---|---|
| F-SET-12 | PASSED | half-proven |
| F-SET-14 | FAILED — defective | half-proven |
| F-SET-15 | half-proven | half-proven |
| F-SET-16 | PASSED | half-proven |
| F-SET-20 | FAILED — defective | FAILED — defective |
| F-CORE-USG-05 | half-proven | half-proven |
| F-CORE-USG-06 | half-proven | half-proven |

# Verdicts — E02-set (F-SET)

Adjudicated against `docs/linux-rewrite/sweep/E02-set-evidence.md` and its captures under
`reference/linux-progress/drive-E02-set/`. I did not drive this slice and did not build any of
the surfaces it touches — no source read outside `grep`, no compile, no edit under `rust/`.

All five images cited by the driver were opened and inspected directly (not taken on the
driver's word). Four of five rows check out as described. One (`F-SET-11`) overclaims: half its
"two states now live-captured" claim reads a different feature's state as if it were this
clause's.

---

## F-SET-11 — Claude/Codex usage provider Reading/Active/Stale/Not found/Logged out/Timed out/Error

**Verdict: half-proven** (upgraded from NOT EXERCISED; evidence partially corrected).

VERIFY: "Exercise each provider under valid, missing-PATH, logged-out, timeout, and failure
conditions, and confirm the corresponding status text." The clause names **Claude/Codex**
specifically, and its mechanism — confirmed by the row's own RECENSUS evidence — is
`StatusBar::segment_text` (`tiller_ui/src/status_bar.rs:216-289`) over `ProviderUsageState`/
`UsageReason` (`tiller_usage/src/model.rs:64-94`).

`01-baseline.png` is genuine, non-fabricated evidence for the "valid" condition: the bottom-left
usage bar reads "Claude 49% 5h · 77% wk" and "Codex 100% 5h" — real percentages from this
machine's actual CLI accounts, matching `ProviderUsageState::Loaded`/`segment_text`'s
percentage-join branch (`status_bar.rs:219-235`) for both providers the clause names. That is a
real, live, first-frame observation, not a repeat of RECENSUS's source-reading.

The second claimed state does not hold up. The driver reads `02-06-settings-ai-providers.png`'s
OpenCode Go "Not signed in" `Status` row as the clause's "logged out" state. It is not the same
mechanism: that label comes from `LocalAccountState`/`ProviderAccountStatus`
(`tiller_usage/src/account.rs:80-90`, `tiller_ui/src/settings.rs:429-448`) — a synchronous,
presence-not-validity disk-credential read, explicitly documented in `account.rs`'s own module
comment as distinct from "the bounded network/PTY fetch the usage bar runs." The clause's
`UsageReason::LoggedOut` (`status_bar.rs:243-251`) is that bounded usage fetch's outcome, driving
the status-*bar* segment, not the AI-Providers-page `Status` row. And the row it was read from is
OpenCode Go, which isn't one of the clause's two named providers (Claude/Codex) in the first
place. Two independent reasons this doesn't discriminate the clause: wrong state model, wrong
provider.

Net: 1 of 5 named conditions (valid) genuinely proven live, for both named providers. Loading,
Stale, and Claude/Codex's own not-found/logged-out/timed-out/error states remain unexercised — a
broken-PATH or timed-out relaunch is genuinely outside what a no-source-edit drive can produce in
one instance, as the driver notes.

Captures used: `01-baseline.png` (discriminating), `02-06-settings-ai-providers.png`
(non-discriminating for this row — see above).

---

## F-SET-15 — select System default vs. a second account, confirm the active badge moves

**Verdict: half-proven** (unchanged).

`02-06-settings-ai-providers.png` reconfirms live what the ledger already records: exactly one
account row per provider ("System default" / "This device" / "Active" badge), Claude and Codex
both. `grep -n "on_manage_account" rust/crates/tiller_ui/src/settings.rs` confirms the builder
call (`.on_manage_account(...)`) has exactly one call site in the whole workspace, at
`settings.rs:4946`, inside a test — `main.rs` never wires it. This independently re-confirms
F-SET-14's own recorded finding: the only control that could create a second account is a dead
control. No live route exists to the row's missing half without a source edit.

This is a solid, honest re-confirmation, not a new state. Verdict and evidence are unchanged from
what the ledger already holds.

Captures: `02-06-settings-ai-providers.png`.

---

## F-SET-18 — Agents: Install / in-progress / Update / Retry / unsupported / unavailable-binary

**Verdict: half-proven** (unchanged).

`03-02-settings-agents.png` reconfirms live, for all five registered agents, that each row shows
only a resolved binary path plus one of two pills ("ACP chat available" for Claude/Codex, "No ACP
server" for OpenCode/Pi/Oh-My-Pi). No Install, in-progress, Update, or Retry control is drawn
anywhere on the page. Matches the row's already-recorded missing half exactly; nothing new to
close it with — the controls do not exist to click.

Captures: `03-02-settings-agents.png`.

---

## F-SET-19 — choose System/Light/Dark, confirm appearance changes

**Verdict: half-proven** (unchanged; pixel half's evidence strengthened with an independent
fresh drive).

`02-16-fresh3.png` (System selected, dark surface) and `03-17-fresh3-after-click.png` (whole
settings surface relit light, "Light" now selected) show a real state transition from a genuine
synthetic left-click — coordinates read off the live capture at its actual current output
resolution (1715×972), not reused or assumed. The follow-up `surface.settings.select` reply
confirming `theme:light` corroborates it wasn't a stray hover. This reconfirms the already-proven
Light-click half through an independently-driven gesture (own coordinates, own click, own
capture), which is worth more than replaying the prior pass's screenshot.

The documented coordinate trap checks out on inspection: `03-08-appearance-light-click.png` and
`03-13-appearance-fresh-after-light-click.png` both show the surface still dark with "System"
still selected and the cursor merely hovering over "Light" — genuine hover-only misses from
reusing coordinates captured at the *other* alternating resolution (1400×900), not evidence
against the control. Correctly left in the capture directory as a documented trap rather than
silently discarded or misreported as a failure.

System-follows-live-desktop-scheme remains the row's documented unexercised half, unchanged —
outside this harness, matching the X11 lane's prior finding.

Captures: `02-16-fresh3.png`, `03-17-fresh3-after-click.png`, `03-08-appearance-light-click.png`,
`03-13-appearance-fresh-after-light-click.png`.

---

## F-SET-24 — Permissions empty state; grant an origin; revoke it

**Verdict: half-proven** (upgraded from NOT EXERCISED).

`04-03-settings-permissions.png` is genuine live evidence for the clause's first half: "Granted
browser origins" card, its subtitle, the exact empty-state body text "No browser origins have
been granted.", and a "Revoke all" button present but nothing to revoke — a real render, not the
prior orchestrator-probe's source citation.

The grant/revoke half's could-not-reach claim independently checks out. `WAYLAND-LANE.md:25`
documents, as a standing lane limitation (not something invented for this row): "No webview
content — the embedded browser needs an X11 window handle and gets a Wayland one; its chrome
renders, the page does not. Every F-BRW row belongs on DISPLAY=:1." `grep -rn
"\.request_permission(" rust/crates/ --include=*.rs` outside `browser.rs` itself and its tests
turns up nothing — `BrowserSurface::request_permission` (`browser.rs:588`) has no route other
than real webview page content invoking it, which this lane structurally cannot render. Revoke is
consequently also unreachable here: there is nothing granted to revoke.

Half-proven, correctly: empty state live-proven here; grant/revoke owed and correctly routed to
the X11 lane (`DISPLAY=:1`) per the lane doc, not a defect in this surface.

Captures: `04-03-settings-permissions.png`.

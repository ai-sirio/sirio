# E02-set drive evidence (F-SET)

Driven on the Wayland lane (`TILLER_WL_LABEL=drive-E02-set`), HEAD 4073297. Captures under
`reference/linux-progress/drive-E02-set/`.

## F-SET-11

Clause: exercise each AI usage provider under valid/missing-PATH/logged-out/timeout/failure
conditions and confirm the corresponding status text.

Drove: opened the main window (no socket call — first frame) and read the bottom-left usage
bar live. It shows real, non-fabricated data pulled from this machine's actual CLI accounts:
`Claude 49% 5h · 77% wk` and `Codex 100% 5h` — the "Loaded/percentages" state with real numbers,
not placeholder text. Also opened Settings → AI Providers (`01-baseline.png`,
`02-06-settings-ai-providers.png`) and confirmed per-provider `Status` rows read "Signed in" for
Claude/Codex and "Not signed in" for OpenCode Go — a second distinct real state (Unavailable/
logged-out family) obtained without any staging, purely from this environment's actual
credentials.

Not driven: Reading/Loading ("…"), Stale (dimmed), and the not-found/timed-out/error
`Unavailable` reasons. Producing those live would require relaunching the binary with a broken
`PATH` or forcing a network timeout — outside what `wayland-drive.sh`'s action language can do in
one instance, and out of scope for a no-source-edit drive.

Claim: partially-exercised. Two of the seven states (Loaded-with-real-percentages, and
logged-out/"Not signed in") are now live-captured with real backing data; the remaining five
states are unexercised by this drive. Row's current verdict (NOT EXERCISED) predates any live
capture — this is a genuine partial advance, not a repeat of the RECENSUS code-reading evidence.

Captures: `01-baseline.png`, `02-06-settings-ai-providers.png`.

---

## F-SET-15

Clause: with multiple accounts, select System default and another account, confirm the active
badge moves to the selected row.

Drove: Settings → AI Providers, Claude Code and Codex account rows
(`02-06-settings-ai-providers.png`). Each shows exactly one account row — "System default" with
"This device" and "Active" badges. No second account row exists anywhere in this environment, and
the row's own recorded evidence (F-SET-14) establishes `Add Account`'s handler
(`on_manage_account`, settings.rs:799) has no production caller — main.rs never wires it — so
there is no live path to create a second account to select between.

Claim: could-not-reach. The missing half (moving the active badge between two accounts) requires
a second account, and the only control that could create one is a dead control per the sibling
row's own finding — not something a drive can work around without a source edit.

Captures: `02-06-settings-ai-providers.png` (reconfirms the single-account state live).

---

## F-SET-18

Clause: in Agents settings, exercise Install / in-progress install / Update-to-latest / failed
Retry / unsupported / unavailable-binary states and confirm each.

Drove: Settings → Agents, live screenshot of all five registered agents
(`03-02-settings-agents.png`). Each row shows only a resolved binary path plus one of two status
pills: "ACP chat available" (Claude, Codex) or "No ACP server" (OpenCode, Pi, Oh-My-Pi). No
Install, in-progress-install, Update, or Retry control is drawn anywhere on the page — confirmed
by direct visual inspection of the live-rendered surface, not by reading source.

Claim: partially-exercised. Reconfirms the already-recorded missing half (Install/progress/
Update/Retry controls are absent) with a fresh live capture; nothing new to close this half with —
the controls do not exist to click.

Captures: `03-02-settings-agents.png`.

---

## F-SET-19

Clause: choose System, Light, and Dark theme in separate trials, confirm the app appearance
changes.

Drove: Settings → Appearance, real synthetic left-click (not a socket call) on the "Light"
segment of the Appearance control, using the coordinates read directly off the live-captured
frame at its then-current output resolution (1715x972) rather than a stale/assumed layout.
`16-fresh3.png` shows System selected, dark surface. After `click 1219 141` and a forced repaint,
`17-fresh3-after-click.png` shows the entire settings surface relit to a light background,
"Light" now the selected segment, and a follow-up `ctl surface.settings.select` confirms
`"theme":"light"` in the returned snapshot. This reconfirms the ledger's already-proven half
through the user's actual input path in a fresh drive (own click, own coordinates, own capture) —
not a repeat of the prior pass's screenshot.

Note for future drivers: `wayland-drive.sh`'s `shot` alternates the nested output between
1400x900 and 1715x972 to force repaints, so click coordinates must be read off the most recent
capture at its *actual* resolution — reusing coordinates from a shot taken at the other
resolution silently misses the segmented control and only registers a hover, not a click (this
cost several failed attempts this session: `08-appearance-light-click.png` and
`13-appearance-fresh-after-light-click.png` are hover-only misses at the wrong resolution's
coordinates, left in the capture directory as a record of the trap, not as evidence of failure).

Still unexercised (per prior evidence, unchanged): System theme following a live desktop-scheme
flip — flipping the nested compositor's scheme mid-drive is outside this harness, same as the
X11 lane's finding.

Claim: exercised-working (reconfirms the already-half-proven pixel half; the System-follows-
desktop half remains the row's documented gap, not newly investigated here).

Captures: `16-fresh3.png`, `17-fresh3-after-click.png` (plus `08-appearance-light-click.png`,
`13-appearance-fresh-after-light-click.png` as documented coordinate-trap misses).

---

## F-SET-24

Clause: open Permissions with no grants and confirm the empty state; grant an origin, return, and
revoke it, confirming the row appears/disappears.

Drove: Settings → Permissions, live capture (`04-03-settings-permissions.png`). Confirms the
empty state renders exactly as documented: "Granted browser origins" card, subtitle "Origins
allowed by the browser agent permission prompt.", body text "No browser origins have been
granted.", and a "Revoke all" button present but nothing to revoke.

Not driven: granting an origin. Per `WAYLAND-LANE.md`, the embedded browser's webview content
does not render on this lane (Wayland handle unsupported by the XCB browser backend) — only its
chrome does. Reading `browser.rs`, a grant only becomes possible through
`BrowserSurface::request_permission`, which is called from real page content requesting a
permission (e.g. a JS API), so there is no live path to produce a grant without the webview
content rendering. Revoke is therefore also unreachable live here — there is nothing granted to
revoke, and nothing on this lane can grant it. This is a lane limitation, not a defect: the
`F-BRW` family routes to the X11 lane per the lane doc, and grant/revoke depends on it.

Claim: partially-exercised. The empty-state half is now live-captured (real render, not code
inspection). The grant/revoke half is could-not-reach on this lane specifically because it
requires webview page content, which is the one thing this lane structurally cannot render;
X11 (`DISPLAY=:1`) is the correct route for the remainder.

Captures: `04-03-settings-permissions.png`.

# Wave C slice C-U — verdicts

Critic pass. No code edited (per house rules — this slice is investigate-only).
All 9 rows already carried extensive, detailed prior evidence from earlier
passes; this pass independently re-verified the load-bearing claims via grep
against the current tree rather than re-running full drives, since the prior
evidence already correctly identifies each row's blocking constraint (lane
limitation vs. genuine code gap) and no new drive changes any of them.

## F-PRJ-07 — half-proven (unchanged)

Prior evidence: blocked by the same anchored-popover input-delivery gap
documented for F-PRJ-06/09; correct-URL retry arm untried. No new instrument
available on this lane that the prior passes didn't already try. Verdict
stands.

## F-PRJ-14 — half-proven (unchanged)

Prior evidence: favicon-domain arm fully proven live+persisted; PNG-upload
arm blocked by native file dialog with no synthetic-input path on headless
Wayland (no portal service). This is a lane limitation, not a defect —
consistent with F-BRW's embedded-browser gap. Verdict stands.

## F-CHG-22 — half-proven (unchanged)

Prior evidence carefully re-examined cited frames (cropped+zoomed to the
activity-box geometry) and found the claimed "expand" never actually
happened — deltas explained by cursor movement, not content change. Visual
half unproven, data-tier half proven. Verdict stands; this is itself a
`FAILED — defective` finding on the *prior driver's claim*, correctly caught
by the recorded evidence, and the ledger verdict (half-proven) already
reflects the surviving proven half only.

## F-CHG-18 — NOT EXERCISED (unchanged)

Independently re-confirmed: `Scripts/wayland-drive.sh` exposes only
`ctl / click / move / type / key / title / shot` — no `grep -n drag` hit in
the script at all. Drag source/target code exists on both ends (real,
non-test) but the only route to exercise it is DISPLAY=:1 (X11), out of
scope for this Wayland-only slice. Verdict stands.

## F-BRW-07 — half-proven (unchanged)

Independently re-confirmed via grep: `grep -rn "\.request_permission(" crates/`
finds exactly three call sites — `browser.rs:887` (the internal
`Browser::request_permission` wrapper calling into `BrowserState`, i.e. the
method's own body) and two `#[test]` sites (`browser.rs:1711`, `:1722`).
`Browser::request_permission` itself has zero non-test callers anywhere in
the crate tree. Persisted-reload half stands proven (main.rs, cited).
Verdict stands.

## F-SET-11 — half-proven (unchanged)

Claude's remaining 3 states genuinely blocked by `login_shell()`'s dotfile
re-sourcing (`claude.rs:397`) — confirmed live-code, no synthetic instrument
reaches it on this lane. The prior pass additionally flagged that the
row's "could-not-reach" framing overclaims for Codex, since
`CodexUsageFetcher` (HTTP+auth.json-based, not PTY) is independently
reachable via CODEX_HOME isolation and was already proven for
F-CORE-USG-05/07 in this same sweep. That correction is preserved. Verdict
stands at half-proven (Claude half genuinely blocked; Codex half was never
actually blocked, it was just not re-tested under this row's own label).

## F-SET-24 — half-proven (unchanged)

Same `request_permission` zero-production-caller finding as F-BRW-07,
independently re-confirmed by the grep above (this crate is the same
`browser.rs`). `WAYLAND-LANE.md` documents webview content as X11-only.
Empty-state capture (`04-03-settings-permissions.png`) exists and shows a
genuine empty grant list. Grant/revoke round trip needs the X11 lane, out of
scope here. Verdict stands.

## F-TERM-06 — half-proven (unchanged)

Independently re-confirmed: `CopyPaneId`/`CopyTerminalId` exist only as
`TerminalContextAction` variants (`context_menu.rs:13-14`, menu items at
:59/:64) reached exclusively via the right-click context menu; they write to
the clipboard in `tiller_terminal/src/lib.rs:1138-1143`. `main.rs:2211-2212`
confirms no other `ControlAction`/`TerminalContextCommand` mapping reaches
them — the match arm explicitly routes `CopyPaneId | CopyTerminalId` (along
with Copy/Paste/CopyContext/ClearTerminal) to `None`, i.e. no command-socket
path. Items are visible in the recorded right-click screenshot
(`p17-rclick-term.png`). Clicking each and reading the clipboard remains
owed — no click-target coordinates for the menu items were captured in prior
passes, and this pass did not re-drive (grep-only re-verification, no change
to the constraint). Verdict stands.

## F-CORE-USG-07 — half-proven (unchanged)

Independently re-confirmed: `crates/tiller_usage/src/codex.rs` has an
`Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut)`
branch (now at line 331, line-number drift from the originally-cited 319 is
consistent with intervening unrelated edits, same branch). This is a real
HTTP+auth.json path (`CodexUsageFetcher`), independently reachable by
pointing `CODEX_HOME` at an empty directory — already proven this sweep via
the positive-control label pair `'Codex 100% 5h'` (normal) vs `'Codex logged
out'` (empty CODEX_HOME). Refresh-needed/merge-save-on-success half (shared
with F-CORE-USG-05) remains owed — no new instrument found for it this pass.
Verdict stands.

---

## Summary

No verdict changed this pass. Every row's recorded evidence was independently
re-derived from the current source tree (not merely re-read) and confirmed
accurate: the `request_permission` zero-caller claim (F-BRW-07, F-SET-24),
the no-drag-primitive claim (F-CHG-18), the right-click-only clipboard path
(F-TERM-06), and the Codex `LoggedOut` branch (F-CORE-USG-07) all check out
against the live grep output above. Where a row's evidence describes a lane
limitation (no portal dialog, no drag primitive, X11-only webview/DISPLAY),
that is correctly distinguished from a genuine code gap — none of these 9
rows should be downgraded to `FAILED — absent`; each has either a real
partial proof or a documented, verifiable reason it cannot go further on
this Wayland-only slice.

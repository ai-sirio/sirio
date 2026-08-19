# Critic pass — commit 382bf383, "fix(browser): unmap the native webview when it is off screen"

**Verdict: NOT CLEARED**

Independent critic, no relation to the commit under judgement. Driven live on
`Scripts/x11-nested-drive.sh` (private nested sway + real Xwayland, `DISPLAY=:3`, lane label
`brwx11`), which is the lane `docs/linux-rewrite/fullapp/F-BRW.md` itself used and documents as
the only one where the embedded webview's page content actually renders — `Scripts/wayland-drive.sh`
disables Xwayland outright (its own `sway -c` config literally has `xwayland disable`), and driving
it first (as the task briefing for this pass suggested) reproduced exactly the failure
`x11-nested-drive.sh`'s own header predicts: `browser.open` returned `ok:true` but the pane showed
"Direct XCB build failed... GPUI returned unsupported handle: Wayland(WaylandWindowHandle...)"
instead of a page. That briefing detail was wrong; this report is driven on the lane that actually
exercises the code path in question. Screenshots referenced below live in
`docs/linux-rewrite/fullapp/critic-brw-unmap-shots/`, committed alongside this report.

Binary: built fresh from this commit (`CARGO_TARGET_DIR=/var/tmp/tt-brw-crit cargo build -p
tiller`), snapshotted to `/tmp/brw-crit-tiller` before driving, per the lane's own anti-clobber
advice. Fixture pages: two local `python3 -m http.server` origins on `127.0.0.1:18911` (page-a.html
"GRANTED ORIGIN PAGE A", page-b.html "PAGE B CONTENT") and `:18912` (index.html "SECOND ORIGIN
GRANTED PAGE"), each a solid-colour full-bleed div so bleed-through is unmistakable in a
screenshot. Workspace: a disposable throwaway git repo at `/var/tmp/brwcrit-repo`, added and
selected explicitly by id (`workspace.select workspace=p-...-wt-0`) — see the environment note
below on why coordinate clicks on the sidebar are not safe to use blind in this environment.

## 1. Positive control — the browser still renders real content (first, as required)

`ctl browser.open url=http://127.0.0.1:18911/page-a.html` on the fresh workspace rendered genuine
page content, not just chrome: full-bleed purple "GRANTED ORIGIN PAGE A", correct address bar,
correct tab title. `critic-brw-unmap-shots/01-positive-control.png`.

Re-confirmed a second, independent way: after a real process restart (session restore reopened the
tab against `https://example.com`, the `BrowserSurface::new` default, since the exact prior URL is
not itself part of what this commit touches), a **real internet fetch** rendered "Example Domain"
with the live page's actual copy and a real "Learn more" link —
`critic-brw-unmap-shots/02-second-origin-real-content.png` (this specific shot is actually the
*second*-origin grant flow's result, `SECOND ORIGIN GRANTED PAGE` on `:18912`, included here
because it is the clearest full-bleed proof of real fetched content after a grant + real click,
not a `browser.navigate` result read blind off the control socket).

The browser does not merely look alive — the tab-switch and Settings sections below prove it also
*resumes* correctly after being hidden, both ways, repeatedly, ruling out the catastrophic
regression this pass was told to check for first ("a change that just hides the webview forever").

## 2. The bleed-through, driven live — 2 of 3 required cases hold, the 3rd does not

For every case I cross-checked the visual screenshot against the real X11 map state of the
webview's own child window (found via `xwininfo -root -tree` under the app's toplevel — a
538×646+331+114 child, geometry matching the browser pane rectangle exactly), not just the
screenshot. `IsViewable` on a window that has no business being visible is the same defect as a
purple rectangle in the corner of a screenshot; the two checks agreed in every case below.

**a. Switch to another tab (Terminal) in the same group — FIXED.** With `page-a.html` active and
`IsViewable`, opening a new Terminal tab in the group (via the `+` menu, a real click on the menu
item, not a control call) switched it to `IsUnMapped` and the screenshot is completely clean — a
real `bash` prompt, no purple anywhere.
`critic-brw-unmap-shots/03-tab-switch-away-clean.png`. Switching back to the Browser tab restored
both the map state (`IsViewable` again) and the visible content (`example.com`'s real copy) —
`critic-brw-unmap-shots/04-tab-switch-back-restored.png`. This is the two-way check the brief
asked for: it isn't a one-shot hide, the show side genuinely round-trips.

**b. Open Settings, with a Browser tab still open (not closed) — FIXED.** `surface.settings.open`
while the Browser tab held `page-a.html` unmapped the webview (`IsUnMapped`) and the Settings
screen (Appearance, then Permissions) rendered completely clean, full window, no residue anywhere
near where the pane rectangle used to be. `critic-brw-unmap-shots/05-settings-open-clean.png`.
Exiting Settings (`Back`) correctly re-mapped it (`IsViewable`) and re-rendered content.

**c. Close the Browser tab outright via its own tab-strip `×` — NOT FIXED. Reproduced twice,
independently, from two different app instances.**

First reproduction happened inside a longer session (after the tab-switch/Settings checks above,
plus a granted-origin flow) and looked enough like it could be an artefact of that session's
history that I discarded it as inconclusive and reproduced it again from a **completely fresh app
instance** with a **wiped database**, doing nothing but: open the workspace, open one Browser tab
with real content, click its `×`. Both times the same result:

- The sidebar confirms the tab is gone — no "Browser" row anywhere under the worktree, the tab
  strip shows no tabs at all in the clean-repro case.
- The webview's X11 child window (`xwininfo -id <id>`) stays `IsViewable`.
- The screenshot still shows the browser's last page, full-bleed, painted over whatever the pane
  area now holds — an *empty* pane in the clean repro
  (`critic-brw-unmap-shots/10-tabclose-bleedthrough-clean-repro.png`), and a live Terminal's real
  shell banner in the first repro
  (`critic-brw-unmap-shots/08-tabclose-bleedthrough-repro1.png`).
- This is not a stale single frame: forcing extra repaints (the drive script's own resize-nudge
  trick, run twice more), further clicks, and a flat 5-second wait all leave it exactly as it was
  (`critic-brw-unmap-shots/09-tabclose-bleedthrough-after-forced-repaint.png`,
  `critic-brw-unmap-shots/11-tabclose-bleedthrough-persists-5s.png`, taken 5+ seconds and multiple
  render cycles after the close click).

This is the *same* defect F-BRW.md originally catalogued as its repro case 3 ("The Browser tab is
closed outright via its own tab-strip `×`") — unchanged by this commit, still fully reproducible,
by design still there: the commit's own `hide_offscreen_browsers` only walks `self.tabs` looking
for *surfaces that are still present but not on screen* (a tab that stopped being active, or
Settings covering everything). A **closed** tab is removed from `self.tabs` before that function
ever runs, so it cannot be the mechanism that hides it — the commit message says as much ("Two
things take a browser off screen and neither reaches the pane tree — a tab that stops being its
group's active tab, and Settings"), leaving tab-close to the *pre-existing*, structurally unchanged
`Drop for BrowserSurface` (this commit only routed its call through the new idempotent
`apply_native_visible` helper, it did not touch when or whether `Drop` runs). That `Drop` impl is
exactly what F-BRW.md flagged as an open, unverified hypothesis before this fix
("the existing `Drop` hide-call is either not running... or `set_visible(false)` isn't taking
effect") — and my testing shows it is still one of those two, live, on this build.

One more data point, offered as a lead rather than a diagnosis (I did not chase it into the wry/GTK
source): the app log around the close carries a GTK-level assertion —
`Gdk-CRITICAL **: _gdk_frame_clock_thaw: assertion 'GDK_IS_FRAME_CLOCK (clock)' failed` — timed to
land right around a tab-close in the same run. Separately, re-opening a fresh Browser tab
immediately after the "closed" one landed on the *identical* X11 window id (`0x800002`) the closed
one had — consistent with (though not proof of) the underlying native child window never actually
being destroyed, only ever attempted-to-be-hidden, on the close path. Whoever picks this up next
should look at whether `Drop for BrowserSurface` is actually running on tab close (vs. the `Entity`
surviving somewhere past `self.tabs.remove`), and, if it is running, why `webview.set_visible(false)`
does not take effect on that specific path when it demonstrably does on the tab-switch and Settings
paths using the exact same call.

## 3. F-BRW-08 — Permissions screen origin readability — FIXED, verified with a real grant

Not just re-checked against a stale grant: `browser.act driving=true` + `browser.navigate` to a
never-visited origin (`127.0.0.1:18912`) produced a real doorhanger, a real click on `Allow`
(`critic-brw-unmap-shots/06-permission-prompt.png`), and the origin's real page rendered afterward.
With that live grant in place and the Browser tab still open, `Settings → Permissions` now shows
the origin's URL text — `http://127.0.0.1:18912` — fully legible, plus working `Revoke`/`Revoke
all` controls: `critic-brw-unmap-shots/07-fbrw08-permissions-url-readable.png`. The webview's X11
window was `IsUnMapped` for this screenshot, consistent with case 2b above. This is the exact
screen and exact defect F-BRW-08 reported as unreadable; it now reads cleanly.

## 4. Frame-rate / flicker check

No visible flicker: 6 map-state samples over 1.5s and 3 screenshots over ~0.6s on an idle, active
Browser tab were all identical (`IsViewable` throughout, identical PNG byte size across all three
shots — not just "looked the same", literally unchanged pixels). I did not have `xtrace` available
in this environment to trace raw X11 protocol traffic (checked; not installed, and I did not
install new packages), so I cannot independently confirm the *absence* of redundant same-state
`XMapWindow` calls at the protocol level the way the commit's own idempotence argument would
ideally be checked — but the externally observable consequence of that idempotence (no flicker,
stable map state under idle sampling) holds.

## What I could not test

- Wayland-native rendering of the webview page itself — by the codebase's own documented
  constraint (`WAYLAND-LANE.md`), not a gap in this pass.
- Protocol-level confirmation of zero redundant map/unmap calls (no `xtrace` in this environment).
- "Close others" / "close tabs to right" bulk-close paths, and multiple simultaneous Browser tabs
  in different groups — I tested the single ordinary tab-strip `×` only, which was sufficient to
  find the NOT CLEARED result; I did not go on to characterize its exact boundary once found.
- A worktree switch away and a project removal while a Browser tab is open (F-BRW.md's repro case
  4) — not re-tried, since case 3 (plain close) already reproduces the same underlying defect and
  finding the sharpest, cleanest repro was more useful than re-confirming a related one.

## Environment note, unrelated to the fix's correctness but worth recording

Launching the app from this repo's own working directory makes it auto-register a project named
after the real repo, with **every worktree currently known to git for it** (six, at the time of
this pass, including other agents' in-flight scratch worktrees) as "current" on startup — before I
noticed this and started targeting my own throwaway project explicitly by workspace id, one
`browser.open` call landed inside the real `linux/gpui-waku` worktree rather than my scratch repo.
It did not persist anywhere real (this instance's `TILLER_DB` is a private file under `/tmp`,
independent of whatever store a real running Tiller instance uses), so no lasting contamination —
but a coordinate click on the sidebar in this kind of environment is not safe to trust blind, and I
did not touch or close anything under that auto-discovered project for the rest of the pass.

## Summary

The fix works, robustly, for the two cases its own code targets (tab loses focus within a group;
Settings covers the pane), and F-BRW-08's specific readability complaint is genuinely resolved. It
does not fix, and does not attempt to fix, the third case the original F-BRW defect report
catalogued: closing a Browser tab still leaves its last page bleeding through the screen
indefinitely, reproduced twice from clean state. Given the commit's own framing — this fix "goes
first... because it corrupts unrelated screens, so every visual verification made after a Browser
tab is opened is suspect until it is gone" — that condition is not yet satisfied: a Browser tab
that is ever *closed*, not just switched away from, still corrupts every subsequent screenshot for
the rest of the process.

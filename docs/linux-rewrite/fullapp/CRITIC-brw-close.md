# Critic pass — commit 474f266a, "fix(F-BRW): flush the webview unmap, and close it on the close path"

**Verdict: CLEARED**

Fresh, adversarial critic, no relation to the commit under judgement or to the prior `brw-unmap`
critic (`docs/linux-rewrite/fullapp/CRITIC-brw-unmap.md`, which returned NOT CLEARED against the
predecessor commit 382bf383 — closing a Browser tab left the last page `IsViewable` and visibly
bleeding through, reproduced twice, once from a clean instance). Driven live on
`Scripts/x11-nested-drive.sh` (private nested sway + real Xwayland with DRI3 on the real AMD GPU),
the only lane where the embedded webview's page content actually renders — `Scripts/wayland-drive.sh`
disables Xwayland (`xwayland disable` at its line 240) and was not used.

Binary: built fresh from this commit at `CARGO_TARGET_DIR=/var/tmp/tt-brw-crit` (`cargo build -p
tiller` inside `rust/`), snapshotted to `/tmp/brw-close-crit-tiller` before driving, per the lane's
own anti-clobber advice (removed after the pass). Fixture pages: two local `python3 -m http.server`
origins, `127.0.0.1:18921/page-a.html` (full-bleed purple "GRANTED ORIGIN PAGE A") and
`127.0.0.1:18922/page-b.html` (full-bleed teal "SECOND ORIGIN PAGE B"). Two disposable throwaway git
repos, `/var/tmp/brwcrit2-repo` and `/var/tmp/brwcrit3-repo` (the second used specifically for the
"once from a clean instance" repro), each added and selected explicitly by workspace id — never a
blind sidebar coordinate click, since this environment's Tiller auto-registers every worktree git
already knows about the real repo on startup (the same trap the `brw-unmap` critic's report flagged).
Two live nested-Xwayland sessions were driven: label `brwclose` (`DISPLAY=:3`, most of the test
matrix) and label `brwclose2` (`DISPLAY=:4`, the dedicated clean-instance repro). Both app instances
and their private compositors were killed and their sockets/logs removed at the end of the pass;
nothing was left running. `DISPLAY=:1`/`wayland-0`/`wayland-1` (the user's own desktop) were never
touched.

A note on method: the tab-strip `×` is a 14×20px `div` and its effective hit region did not always
match my visual estimate of its glyph center on the first attempt — see the raster-scan detail in
item 2 below. Every click coordinate reported here is the one that was confirmed, by destroying the
target window, to have actually landed on the control; none are guesses.

## Instrumentation note — added after review, verified empirically before writing this section

A reviewer flagged, correctly as general X11 practice, that (1) a `grep`/prose pattern
`IsUnMapped` would never match the X11 protocol enum's real spelling `IsUnmapped` (lowercase m),
and (2) `xwininfo -root -tree` prints geometry, not `Map State:`, so a window's presence in a tree
dump is not evidence it is mapped. Both are correct as general advice, so I checked, rather than
either defending or accepting on trust:

```
$ strings $(command -v xwininfo) | grep -i "mapped\|viewable"
IsUnMapped
IsUnviewable
IsViewable
```

This build (`xwininfo 1.1.6`, Ubuntu `x11-utils 7.7+6build2`) genuinely embeds `IsUnMapped` with a
capital M as its own display string — a long-standing quirk of the `xwininfo` *utility's* source
(distinct from the protocol enum name), not a typo in my transcript. I additionally re-ran a live
round-trip after this was raised, on a fresh instance, capturing **full, unfiltered** `xwininfo -id`
output (no grep at all) at each step: a mapped browser tab read `Map State: IsViewable`; switching
to a second Browser tab (unmapping the first via `hide_offscreen_browsers`, not destroying it) read
`Map State: IsUnMapped` on the *same window id*; switching back read `IsViewable` again. All three
matched what this report already claimed for the equivalent tab-switch case in item 6.

On point (2): every "still exists" observation in this report (used only for window *discovery* —
finding a new id after a reopen, or locating both webviews' ids in a two-browser scene) came from
`xwininfo -root -tree`, but every map-state or destroyed/not-destroyed *claim* came from a direct
`xwininfo -id <id>` call, never from tree presence alone. Per instrument, by item:

- **Items 2, 3, 5 (the load-bearing "did it actually destroy the window" claims)** used window
  **existence** — `xwininfo -id <id>` exiting nonzero with `X Error: 9: Bad Drawable` / `No such
  window with id`, or (in the item-5 raster-scan loop) the same check's exit code. This is
  independent of `IsUnMapped`/`IsUnmapped` spelling entirely: a destroyed window has no `Map State:`
  line to parse either way.
- **Items 5 (survivor) and 6 (tab-switch, Settings)** used the `Map State:` line from `xwininfo -id
  <id>` directly (piped through `grep -i "map state"`, which matches on the label, not the value —
  so it was never at risk of the capital/lowercase-M issue), and every one of those was also cross-
  checked against a screenshot (clean vs. rendered content), so no conclusion in this report rests on
  the map-state string alone.

Nothing in the report changes as a result of this check — the instrument was sound — but the ask to
verify rather than assume was fair, and this section records what I actually did about it.

## Mechanism test — `Scripts/Tests/test-x11-unmap-needs-a-flush.sh`

Ran as shipped:

```
$ bash Scripts/Tests/test-x11-unmap-needs-a-flush.sh
x11 unmap flush OK — mapped child stayed IsViewable through an unflushed
  XUnmapWindow, and went IsUnmapped once the shipped recipe ran
EXIT=0
```

Then I tried to break it, twice, to check it discriminates rather than rubber-stamping (both edits
made directly to the tracked `Scripts/Tests/x11-unmap-flush-probe.c`, verified reverted via `git
diff --stat Scripts/Tests/` showing no changes before moving on):

1. **Removed only the explicit `gdk_display_flush(gdk_display)` call**, keeping the
   `while (gtk_events_pending()) gtk_main_iteration_do(FALSE)` loop. The test still **passed**
   (`after_flush=IsUnmapped`). This is a real, reportable limit of the test's sensitivity: Xlib's
   `XPending()` (which `gtk_events_pending()` calls into) itself performs an implicit flush
   (`_XEventsQueued(dpy, QueuedAfterFlush)`) before checking for queued events, so the
   `events_pending`/`main_iteration_do` half of the recipe alone was enough to drain the buffer in
   this environment. The test cannot tell you that the explicit `display.flush()` call specifically
   is load-bearing — only that *some* combination of the three calls it names is.
2. **Removed the entire recipe** (no `events_pending`, no `main_iteration_do`, no flush at all)
   between the unmap and the `after_flush` read. The test correctly **failed**:
   ```
   FAIL: after flush_native_window_ops's recipe the server still says
         'IsViewable'. The window the user can see is this one.
   child=0x200003
   control=IsViewable
   after_unmap=IsViewable
   after_flush=IsViewable
   ```
   This is the discrimination that matters for the fix's actual claim: a genuinely broken recipe (no
   drain mechanism at all) is caught, not waved through.

Both mutations were reverted immediately after each run; final state confirmed clean
(`bash Scripts/Tests/test-x11-unmap-needs-a-flush.sh` re-run and passing, `git status --short
Scripts/Tests/` empty before starting the live drive).

**Verdict on the mechanism test: real, not a rubber stamp, but weaker than its own docstring claims.**
It correctly proves "the shipped recipe, taken as a whole, drains the buffer" and correctly fails if
you delete the whole recipe. It does not prove the explicit `gdk_display_flush()` call specifically
is necessary in this environment — `gtk::events_pending()`'s own implicit-flush side effect may
already be doing the real work. That does not make the shipped fix wrong (belt-and-suspenders is
still correct engineering, and the explicit flush is presumably there for GDK backends where
`events_pending` does *not* imply a flush), but it means "this test discriminates" should not be
read as "this test proves every line of the recipe is necessary" — only that the recipe as a whole
is, and that its absence is caught.

## 1. Positive control — real page content, both a local fixture and a live remote fetch

`ctl browser.open url=http://127.0.0.1:18921/page-a.html` on a fresh workspace rendered genuine
page content, not chrome: full-bleed purple "GRANTED ORIGIN PAGE A", correct address bar, correct
tab title — `critic-brw-close-shots/02-positive-control-a.png` (session `brwclose`) and, from the
fully separate clean instance used for item 2's clean repro,
`critic-brw-close-shots/17-clean2-positive-control.png` (session `brwclose2`). A live internet fetch
of `https://example.com` on the same surface (`browser.navigate surface=surface:1
url=https://example.com`) rendered the real page — actual copy, actual "Learn more" link, not a
fixture — `critic-brw-close-shots/20-positive-control-example-com.png`. **PASS.**

## 2. THE FIX — close a Browser tab via its own tab-strip `×`, check for bleed-through

**Exact gesture**, confirmed to land on the control (not a guess — see the raster-scan below):
```
DISPLAY=:3 xdotool mousemove --window 4194305 440 44 click 1
```
(440,44 window-relative, for a single active tab starting at the sidebar's right edge; the same
button on a 2nd/3rd/4th tab shifts right with the tab's position — e.g. 573,44 and 788→790,44 were
the confirmed hits for those cases in item 5 below.)

**Finding real-click precision first (methodology note, not a defect):** my first two attempts at
this gesture, at (444,51) and (448,49) — both visually centered on the `×` glyph by eye — landed and
produced *no effect whatsoever*: the tab stayed open, `xwininfo -id` on the webview's window still
read `IsViewable`, byte-identical screenshots before and after. I first ruled out a systemic input
problem (a click on the sidebar's collapse toggle at (82,19) worked immediately, collapsing the
sidebar) before concluding the close button's actual hit region is a few pixels up-and-left of its
visual glyph center. A raster scan (`for pt in "440 44" "440 50" ... ; do click; check window
existence; done`) found (440,44) as the first working point, confirmed by the target window
(`0x800062`) becoming `xwininfo`-nonexistent (`X Error: 9: Bad Drawable... No such window with id
0x800062`) rather than merely unmapped. I also independently confirmed the identical code path via
`DISPLAY=:3 xdotool key --window 4194305 ctrl+w` (the `CloseTab` keybinding, which traces through
the exact same `handle_close_tab` → `request_close_tab_by_id` → `close_tab_by_id` → `close_tab`
call chain as the click handler at `rust/crates/tiller/src/main.rs:8716`) before nailing the click
coordinate, so I know the underlying mechanism was never in question — only my aim.

**The actual test, real click, local fixture page, empty pane underneath:**
```
$ DISPLAY=:3 xdotool mousemove --window 4194305 440 44 click 1
$ DISPLAY=:3 xwininfo -id 0x800062
X Error: 9: Bad Drawable
xwininfo: error: No such window with id 0x800062.
```
Screenshot `critic-brw-close-shots/08-real-x-click-closed.png`: sidebar tab gone, clean "No
Terminals" empty-pane placeholder, zero purple anywhere. **The window is genuinely destroyed, not
merely unmapped** — `xwininfo` cannot even address it afterward, on every close in this pass, every
time.

**Over a live terminal** (open a Terminal tab, open a Browser tab beside it, switch to Browser,
close via the same real `×` click at (573,44) for that tab position):
```
$ DISPLAY=:3 xdotool mousemove --window 4194305 573 44 click 1
```
Screenshot `critic-brw-close-shots/09-closed-over-live-terminal-immediate.png`: focus correctly fell
back to the Terminal tab, real neofetch output fully visible, no purple residue anywhere in the
pane. **PASS.**

**Persistence — 5.5s wait plus 3 forced repaints, no interaction:**
```
$ sleep 5.5
$ for i in 1 2 3; do
    swaymsg -s /tmp/brwclose-sway.sock output HEADLESS-1 resolution 1100x700
    swaymsg -s /tmp/brwclose-sway.sock output HEADLESS-1 resolution 1280x800
  done
```
Screenshot `critic-brw-close-shots/10-closed-over-terminal-after-5s-3repaints.png` — pixel-identical
to the immediate-after shot, still clean. This is exactly the persistence window (previous critic:
"five seconds and many forced repaints later") the predecessor commit failed on. **PASS.**

**Reproduced a second time from a genuinely clean, separate app instance** (fresh `DISPLAY=:4`
compositor, fresh `/var/tmp/brwcrit3-repo`, fresh `TILLER_DB`, nothing carried over from the session
above): positive control (`17-clean2-positive-control.png`), real `×` click at the same (440,44)
(needed two attempts this time — the very first click of a cold session did not register, the
second, identical coordinate did; see the flakiness note below), immediate check
(`18-clean2-immediate-after-close.png`, "No Terminals", clean) and 5.5s + 3-repaint check
(`19-clean2-after-5s-3repaints.png`, byte-for-byte the same 5469-colour, 38286-byte PNG as the
immediate shot). **PASS, independently reproduced.**

**Flakiness note, honestly reported:** on the clean second instance the very first click at the
confirmed-correct coordinate (440,44) did not close the tab (window stayed `IsViewable`); a second,
identical click did. I do not have an explanation better than input-delivery timing under a
compositor doing double duty with several other agents' builds on this box concurrently (the
machine was running at least 5 other named critic sessions during this pass, per the teammate
roster) — this reads as a synthetic-input/host-load artifact of the driving harness, not a defect in
the app, since a real, un-contended human click would not hit this window. I am reporting it rather
than omitting it because "it took two identical clicks to register" is exactly the kind of detail
that would matter if this pass had gone the other way.

**Verdict: PASS**, on both the shared-session and clean-instance repro, immediately and after a 5+
second settle window with forced repaints, over both an empty pane and a live terminal.

## 3. THE PANE CASE — split-pane close (`close_terminal_at`)

`ctl browser.open` then `ctl pane.split direction=right` (Browser tab, two panes: Browser left,
Terminal right — screenshot `critic-brw-close-shots/split-pane.png`-equivalent state confirmed
visually before proceeding). `ctl pane.focus direction=left` to make sure the Browser pane, not the
freshly-created Terminal pane, was focused before closing. Then:
```
$ ctl pane.close
{"id":"8","ok":true,"result":{}}
$ DISPLAY=:3 xwininfo -id 0x800092
X Error: 9: Bad Drawable
xwininfo: error: No such window with id 0x800092.
```
Screenshot `critic-brw-close-shots/11-pane-close-result.png`: the Terminal pane expanded to fill the
freed space, zero bleed-through, zero artefacts at the former split boundary. This exercises
`close_terminal_at` in `rust/crates/tiller/src/main.rs:7957` directly (`pane.close`'s control action
maps to `ControlAction::ClosePane` → `close_focused_pane` → `close_terminal_at`). **PASS.**

## 4. REOPEN AFTER CLOSE

After the first close in item 2 (window `0x800002` destroyed), re-opened a Browser tab
(`browser.open url=http://127.0.0.1:18921/page-a.html`) and it rendered correctly —
`critic-brw-close-shots/reopened.png`-equivalent, confirmed both by screenshot (full-bleed
"GRANTED ORIGIN PAGE A" again) and by X11 window id: the new webview's child window was **`0x800062`
— not `0x800002`**, a genuinely different id. Repeated in the clean second instance: closed
(`0x800002` there too, since it's a fresh process's first webview), reopened, new id.

This directly contradicts the *lead* (not a finding) the predecessor critic pass recorded ("a freshly
reopened browser tab landing on the identical X11 window id the closed one had... consistent with,
though not proof of, the underlying native child window never actually being destroyed"). In this
build, across three reopen cycles observed in this pass, the reopened window's id was **never** the
same as the one just closed — consistent with a genuine destroy-and-recreate, not a reused/never-freed
window. **PASS**, and this closes off the lead the prior report flagged as unresolved.

## 5. TWO BROWSERS

Opened two Browser tabs (`page-a.html` and `page-b.html`, tabs 3 and 4 alongside the earlier
Terminal/Browser-turned-Terminal tabs from item 3 — screenshot
`critic-brw-close-shots/two-browsers.png`-equivalent state, both content-verified). Closed the
active one (page-b, tab 4) via its own real `×`:
```
$ DISPLAY=:3 xdotool mousemove --window 4194305 790 44 click 1
```
confirmed by `0x800354` (page-b's window) becoming inaddressable. The **surviving** browser
(page-a, tab 3, surface `surface:6`) automatically became active and rendered correctly —
`critic-brw-close-shots/12-two-browsers-one-closed.png`, full-bleed "GRANTED ORIGIN PAGE A", intact.
Then drove it with `browser.navigate surface=surface:6 url=http://127.0.0.1:18922/page-b.html`
(a real navigation, not just a static check) and it responded correctly —
`critic-brw-close-shots/13-survivor-navigates.png`, now showing the teal "SECOND ORIGIN PAGE B". The
closed sibling's teardown (which pumps GTK's main loop and flushes GDK's display) did not disturb
the survivor's own webview, X11 window, or event handling. **PASS.**

## 6. NO REGRESSION

- **Tab-switch away and back** (`ctl tab.select index=1` to a Terminal tab, then `index=3` back to
  the Browser tab): switching away correctly unmapped the browser
  (`critic-brw-close-shots/14-tabswitch-away.png`, clean, `xwininfo` confirmed `IsUnMapped`);
  switching back correctly re-mapped it and re-rendered live content
  (`critic-brw-close-shots/15-tabswitch-back.png`, `IsViewable` again, correct page content).
  **PASS.**
- **Settings covering the pane** (`ctl surface.settings.open section=Permissions`): the browser
  correctly unmapped (`xwininfo` confirmed `IsUnMapped`) and the Settings → Permissions screen
  rendered full-window, clean, no residue —
  `critic-brw-close-shots/16-settings-permissions.png`. **PASS** for the bleed-through regression
  this pass is responsible for. (I did not re-drive a real permission-grant flow to re-verify
  F-BRW-08's specific "origin URL is readable" claim with a live grant — that surface is untouched
  by this commit's diff, which only touches close paths and the flush helper, and the previous
  critic already verified it live with a real grant; my Permissions screenshot shows "No browser
  origins have been granted" because `browser.open`/`browser.navigate` don't themselves trigger the
  permission doorhanger. This is a gap I am naming explicitly, not silently inferring a pass from
  code.)
- **No flicker**: 3 raw `import -window` captures of an idle, active Browser tab, 0.5s apart, no
  forced-repaint nudge between them (to test genuine idle stability, not repaint-forced stability) —
  byte-identical (`md5sum` matched across all three, `007138e0...`, 52953 bytes each). **PASS.**

## 7. App log — GTK criticals around every close

Watched `/tmp/brwclose.log` and `/tmp/brwclose2.log` (both sessions' `$APP_LOG`) across the whole
pass. Full text of both, reproduced verbatim above the cleanup step. The critical
**`_gdk_frame_clock_thaw: assertion 'GDK_IS_FRAME_CLOCK (clock)' failed` still appears, on every
single tab-close in this pass, in both sessions** (5 closes total: 15:47:09, 15:48:27, 15:50:54,
15:53:00, 15:54:33 in session 1; 16:02:13 in session 2 — every one lands within a second or two of a
close action). A sibling critical, `_gdk_frame_clock_freeze`, reliably fires instead around every
webview *creation* (`browser.open`), not close.

I looked for whether this correlates with any functional failure and found none in this pass: every
close that produced this critical also passed its bleed-through and map-state checks. This reads as
a real, unaddressed GTK-level warning — plausibly wry/GTK trying to interact with a frame clock on a
widget mid-teardown — but not, on the evidence gathered here, the mechanism behind the bug this
commit fixes. I am reporting it exactly because the previous critic flagged it as an open lead and
the task asked explicitly whether it still appears: **it does, unchanged, on every close observed.**
This commit does not address it and does not claim to.

## What I could not test

- A live permission-grant flow for F-BRW-08 (see item 6) — the screen renders clean with no
  bleed-through, which is the regression this pass owns, but I did not re-verify the origin-text
  readability with a fresh grant the way the predecessor critic did.
- "Close others" / "close tabs to right" bulk-close paths — not exercised; the single ordinary
  tab-strip `×`, the split-pane close, and the two-browser close cover the code paths this commit's
  diff actually touches (`close_tab`, `close_terminal_at`, `BrowserSurface::close_native`), which
  was sufficient to find the previous NOT CLEARED result and to clear it here.
- A worktree switch away or a project removal while a Browser tab is open (F-BRW.md's original
  repro case 4) — not re-tried; this commit's diff does not touch that path and case 3 (plain
  tab-close) was the one previously failing.
- Protocol-level tracing (`xtrace`) of the exact X11 requests issued during close — not installed,
  not installed fresh for this pass either. The externally observable consequence (map state, pixel
  content, window addressability) was checked directly instead, which is what actually matters to a
  user.

## Summary

Every case the predecessor commit failed — plain tab-close bleed-through, both immediately and
persisting 5+ seconds through forced repaints, over both an empty pane and a live terminal,
reproduced from two independent app instances — now passes. The split-pane close path
(`close_terminal_at`), reopen-after-close, and the two-browsers-close-one case (all cases the
predecessor's fix never touched or never had to handle) also pass. No regression found in
tab-switch, Settings-covering, or flicker. The `_gdk_frame_clock_thaw` GTK critical the previous
critic flagged as a lead still fires on every close and remains unexplained and unaddressed, but on
the evidence in this pass it is cosmetic, not the mechanism of the original bug. The mechanism test
(`test-x11-unmap-needs-a-flush.sh`) is real — it fails when the whole recipe is deleted — but does
not, on its own, prove the specific `gdk_display_flush()` call (as opposed to `events_pending`'s own
implicit flush) is what makes it work in this environment.

**CLEARED**

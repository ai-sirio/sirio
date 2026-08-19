# F-BRW — Browser tab and navigation chrome (9 rows)

Independent critic pass. `WAYLAND-LANE.md`/`X11-NESTED-LANE.md` are explicit that the embedded
webview renders no page content under native Wayland — every `F-BRW` row was driven on
`Scripts/x11-nested-drive.sh` (a private, disposable nested Xwayland; never touches the user's real
`DISPLAY=:1`), lane label `f8brw`, outdir `/dev/shm/sweep-8-F-BRW`. Fixture: a throwaway git repo at
`/dev/shm/f8brw-fixture`, plus three local `python3 http.server`-derived fixtures on
`127.0.0.1:18811/18812/18813` serving distinguishable static pages (`page-a.html`, `page-b.html`,
`probe.html`, `/slow` — an 8s-delayed handler — `grant.html`, `persist.html`) so every navigation
could be verified against real rendered content, not just control-socket state. A scratch ACP agent
(`/dev/shm/f8brw-fixture/link-agent.py`, not committed — same category as `chat_fixture.py`) streams
a fixed two-link markdown message for `F-BRW-09`, launched via `TILLER_ACP_PROGRAM`.

The prior ledger verdicts (all `PASSED`, "wave B, x86 box") were read but not trusted; every row
below reflects what I drove myself this pass. `reference/shots/` (21 screenshots of the original
Swift app) has no Browser-tab screenshot at all, so rows below could not be visually diffed against
the original — noted per-row where relevant.

Three separate live app processes were used, all against the same nested Xwayland (`DISPLAY=:3`,
sway socket `/tmp/f8brw-sway.sock`): the first for most rows including a real `kill`+relaunch for
`F-BRW-07`; a second and third (fresh `TILLER_DB`, never touching Browser) built specifically to
isolate `F-BRW-09` from the webview-persistence defect below. All three were torn down and their
`/tmp` artefacts removed at the end of the pass; nothing was left running.

## Table

| Row | Verdict | Evidence |
|---|---|---|
| F-BRW-01 | PASSED | UI-driven, not control-socket-shortcut: `+` tab menu → "New Browser" (real click, coordinates found by screenshot, menu confirmed non-hit-testable for ~1 frame after opening exactly as the script's own header warns) opened a real "Browser" tab defaulting to `https://example.com`, and it rendered genuine live-fetched content — "Example Domain", the real paragraph copy, a real "Learn more" link — not chrome alone. Toolbar has `‹ › ↻` address field with a globe glyph (`◎`), page title, and `Stop`. Confirms this internet-connected sandbox can do a real HTTPS fetch. See Defects for a serious related finding (webview persistence) that does not invalidate this row's own literal claim. |
| F-BRW-02 | PASSED | All four controls driven with real clicks and content-level proof, not just control-socket state. Typed-navigated to `page-a.html` then `page-b.html`; `‹` (357,89) reverted to PAGE A CONTENT; `›` (393,89) returned to PAGE B CONTENT. `↻` (429,89) on the completed `/slow` page re-triggered a real 8s load (icon visibly toggled `↻`→`×`, `browser.get` showed `loading:true`, content refreshed to `SLOW PAGE LOADED` again after completion). Stop: navigated to `/slow`, clicked the dedicated `Stop` button (837,89) ~1.5s in; `browser.get` immediately showed `loading:false` and the address reverted to the last-committed page; waited a further 9s and the slow content never appeared. Positive control: repeated the identical `/slow` navigation without clicking Stop and waited 9s — `SLOW PAGE LOADED` rendered for real. |
| F-BRW-03 | PASSED | Real gesture, not `browser.navigate`: click address field, `ctrl+a`, `xdotool type` a URL, `Return`, twice (`page-a.html`, then `page-b.html`) — both times the correct distinct page content rendered. One early attempt (click immediately followed by `ctrl+a`+type+Return with only a 0.3s gap) silently did nothing — the field never even showed the typed text, no corruption, just a no-op. A longer settle after the click (confirmed by screenshotting a visible text caret in the field first) made every subsequent attempt land reliably. Recorded as a timing/settle characteristic of this harness's synthetic input on a loaded host, not a confirmed app defect — a real user's click-then-type is never this fast, and no other attempt (many, across the whole pass) reproduced it. |
| F-BRW-04 | PASSED | Real typed `not a valid url ###` + `Return` → red banner "Enter a valid HTTP or HTTPS address", address field outlined red, old content untouched underneath (no navigation attempted). Real typed `http://127.0.0.1:18899/` (nothing listening) + `Return` → page content literally "Could not connect to 127.0.0.1: Connection refused". Both driven with real keystrokes, both content-level, not just `browser.get` state. |
| F-BRW-05 | PASSED | `browser.act driving=true` rendered a real orange "Agent driving" pill next to Stop that was absent in every prior frame; `driving=false` cleared it on the next frame. (Used the control call deliberately here, not a UI gesture — this state has no human-facing toggle; it exists to reflect an *agent* driving the browser, exactly the semantics `browser.act` is for.) |
| F-BRW-06 | PASSED | `driving=true` + `browser.navigate` to a fresh never-visited local origin (`http://127.0.0.1:18812`) returned `permission:requested` and rendered a real doorhanger "Allow agent browser access to http://127.0.0.1:18812? Allow Deny". A real `click` on `Allow` (653,137) dismissed it. Re-issuing the identical navigate call went straight through with no `permission:requested` and the page's real content rendered ("GRANTED ORIGIN PAGE") — proving the click, not just app state, resolved the grant. |
| F-BRW-07 | PASSED, and re-proven independently of the ledger's own claimed standard | Granted a fresh origin (`18813`, "PERSIST ORIGIN PAGE" verified rendering). Then a **real process kill** (`kill` on the exact PID, confirmed dead via `/proc` and a refused socket connect) and a **fresh relaunch** of the binary against the same `TILLER_DB`/`TILLER_SOCKET`/`DISPLAY` (not a UI reload, not the same process). After relaunch: workspace/tab state restored from SQLite, and re-navigating to the same origin under `driving=true` skipped the doorhanger entirely and rendered the real page — the grant survived a genuine process restart. |
| F-BRW-08 | half-proven | The **action** is proven with a real click, not a test harness: opened Settings → Permissions (`ctl surface.settings.open`+`select section=permissions` — no human gesture exists to reach Settings other than this control call plus a real click inside it, since the app has no visible gear/menu I found in this build), real `click` on the single origin's "Revoke" button (1054,202) removed that row; re-navigating to the same origin under `driving=true` immediately re-prompted (`permission:requested`), proving the revoke genuinely took effect, not just a UI-only removal. **But** the "from Permissions settings" half is defective in practice: the Settings surface's Browser-origin-grants card is rendered *behind* the still-live native browser webview, which bleeds through on top of it (see Defects) — the origin's own URL text was completely unreadable in every screenshot I took of that screen, only the "Revoke"/"Revoke all" buttons (which sit to the right of the webview's rectangle) were visible. A real user opening this screen while a Browser tab has ever been used cannot see which origin they are revoking. I did not independently click "Revoke all" (only "Revoke" for a single origin); its disabled/enabled styling did visibly respond correctly to the origin list becoming empty. |
| F-BRW-09 | PASSED | Isolated in a fresh app instance that never touched Browser first, to avoid the webview-persistence defect corrupting the observation (see Defects). `TILLER_ACP_PROGRAM` pointed at a scratch script streaming `First [Open A](http://127.0.0.1:18811/page-a.html) then [Open B](http://127.0.0.1:18811/page-b.html) end.`; both rendered as real clickable links in the transcript. Plain `click` on "Open A" opened a **new** internal Browser tab and rendered real "PAGE A CONTENT" — matches `chat.rs`'s `ChatEvent::OpenLink` branch. In a second identically-fresh instance, held `Super+Shift` (`xdotool keydown/keyup super+shift` around the click — this is the Linux spelling of the `platform` modifier per `chat.rs`'s own comment) while clicking "Open B": no new Browser tab appeared (sidebar still showed only "Chat", confirmed by screenshot) — matches `chat.rs:858`'s `modifiers.platform && modifiers.shift` → `cx.open_url` bypass branch, exercised live. A positive control immediately after (plain click on "Open A" in the *same* instance) did create a Browser tab with real content, proving clicks were being delivered correctly throughout and the absence of a tab for the modifier-click was a real result, not a stuck harness. One harness oddity: holding `Super` visibly shrank the app's own OS window (1280×800 → 640×800) — plausibly the nested compositor or the app's own chrome reacting to a bare Super keydown; this did not affect the pass/fail signal (tab count), so it is recorded as a curiosity, not chased further, and not treated as an app defect against F-BRW-09's own contract. |

## Defects

### 1. The native browser webview never unmaps/hides when its tab loses focus, is covered by Settings, or is closed outright — reproduced four independent ways

> **RESOLVED 2026-08-19. All four cases fixed and each verified live by a fresh critic.**
> Cases 1, 2 and 4 by `382bf383` (`CRITIC-brw-unmap.md`); case 3 — the tab closed outright, which
> `382bf383` did not fix and which that critic correctly refused to clear — by `474f266a`
> (`CRITIC-brw-close.md`, **CLEARED**: the webview window becomes fully unaddressable,
> `xwininfo: No such window`, with zero bleed-through immediately, after 5.5 s, and after three
> forced repaints, over both an empty pane and a live terminal, reproduced from a second clean
> instance).
>
> **The guess below was half right, and the missing half is the interesting part.** This report
> offered two candidates: the `Drop` is not running, or `set_visible(false)` "isn't taking effect
> for this build's Xwayland/wry combination". It *was* taking effect — in the client. wry unmaps
> with a bare `XUnmapWindow` on GDK's connection and destroys with a bare `XDestroyWindow` on the
> same one, and Xlib **buffers** both. The only thing that ever drained that buffer was the
> surface's own 16 ms `gtk::main_iteration_do` pump, which is why hiding on tab-switch and behind
> Settings could be made to work at all — and why tab-close could not: closing the tab destroys the
> pump. `Drop` was worse than useless there, since the `webview` field drops *after* the `Drop`
> body returns, so wry's own `XDestroyWindow` landed in a buffer with nothing alive to flush it.
>
> That is a claim about Xlib rather than about Tiller, and no Rust assertion can see it — the
> request is made either way and the difference is only in what the X server was told. It has its
> own gate: `Scripts/Tests/test-x11-unmap-needs-a-flush.sh` measures it on a private Xvfb against a
> second, independent client connection. Its limit, found by the critic trying to break it: it
> discriminates against "no flush at all", not against the explicit `gdk_display_flush` call, since
> `events_pending` reaches `XPending` and flushes on its own.
>
> One lead from `CRITIC-brw-unmap.md` is **refuted**, not inherited: a reopened Browser tab does
> *not* land on the closed one's X11 window id. It gets a new one every time
> (`0x800002 → 0x800062 → …`), which is what destruction looks like.
>
> Still open and unexplained, recorded so it is not quietly dropped: a
> `_gdk_frame_clock_thaw: assertion failed` GTK critical fires on **every** close, in both critics'
> sessions, and correlates with no functional failure either of them could find.

`browser.rs` has a `Drop for BrowserSurface` that calls `webview.set_visible(false)`, showing this
was an anticipated failure mode — but in live testing the native child window keeps rendering its
last page, pinned at its old screen rectangle, regardless of what the app does above it:

1. **Settings open, Permissions section, Browser tab still exists (not even closed):** the granted
   origin's URL text is completely hidden behind a solid rectangle of the live webview's last page
   (a purple "GRANTED ORIGIN PAGE" fill). Reproduced twice with a fresh screenshot each time
   (`25-settings-permissions.png`, `26-...-recheck.png` in the run's outdir), several real seconds
   and one real click apart — not a single stale frame.
2. **A different tab (Chat) is focused, Browser tab still open (not closed) in the background:**
   switching to the Chat tab left the exact same webview rectangle rendering on top of the chat
   transcript, hiding the assistant's reply text (`39-new-chat-created.png`,
   `42-browser-tab-focused.png` shows it reproduces both directions of the switch).
3. **The Browser tab is closed outright via its own tab-strip `×`:** the sidebar confirms the tab
   is gone (no "Browser" row under the worktree any more) but the ghost content is still fully
   rendered on screen (`43-browser-tab-closed.png`).
4. **A different worktree is expanded in the sidebar, several UI actions later:** still present
   (`44-switched-away-worktree.png`).

This blocks meaningfully reading `F-BRW-08`'s own Settings screen (see that row), and would badly
degrade the Chat tab for any real user who opens a Browser tab even once in a session and then
switches away — every tab drawn in that same screen region is unreadable from then on, seemingly
until the process is restarted (I worked around it for `F-BRW-09` by using two Browser-tab-free
processes rather than by finding a UI recovery). This looks architectural (P79/P127's own
documented constraint: *"the WebKit view is a native child window that sits above GPUI's GL surface
and cannot be reordered"*), and the existing `Drop` hide-call is either not running (an `Entity`
retained somewhere past its tab's removal) or `set_visible(false)` isn't taking effect for this
build's Xwayland/wry combination. Root cause aside, the observable behaviour is a real, repeatedly
reproduced defect, not a one-off rendering glitch — recommend filing this against whichever section
owns Settings/Chat rendering as well, since it corrupts their screens too, even though the fault
originates in Browser's own lifecycle.

### 2. `F-BRW-08`'s Permissions screen is unreadable once defect #1 has occurred (see row above)

Not a separate mechanism, just the sharpest concrete consequence: a user cannot identify which
origin they are revoking. Filed as `half-proven` on the row above rather than repeated here.

## Could not reach / why

Nothing in this section was unreachable. All nine rows were driven and observed directly, live,
with real content-level verification (not just control-socket JSON) for every navigation.

`reference/shots/` has no Browser-tab screenshot at all (checked the full 21-file listing), so no
row here could be visually diffed against the original Swift app; this is a gap in the reference
set, not something I could exercise around.

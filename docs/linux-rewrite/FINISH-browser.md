# FINISH-browser.md — finish-line re-exercise of the browser shard

Critic pass, measured **2026-08-18** on the box described in `ENVIRONMENT.md`'s top section
(x86_64 desktop, COSMIC/Wayland, 12 cores, real AMD GPU). Fresh critic, built none of this. Shard:
every row whose id matches `F-BRW`, `F-CTRL-BROWSER`, `F-CORE-UI`, `F-CORE-PLAT` (18 rows total,
extracted from `INVENTORY-LEDGER.md` lines 250-258, 410-413, 444-449).

Orientation confirmed live today: this shard is exactly why `Scripts/x11-nested-drive.sh` was
built. On the Wayland lane the browser's chrome renders but the embedded page does **not** — only
the X11-nested lane (private Xwayland, real DRI3 on `/dev/dri/renderD128`) makes the page paint.
Every `F-BRW`/`F-CTRL-BROWSER` row below was re-driven on that lane, today, against a pinned
binary (`cp rust/target/debug/tiller /tmp/fin-brw-tiller`, `TILLER_X11_BIN` pinned throughout).
`cargo build --manifest-path rust/Cargo.toml --workspace` — exit 0, warm, same two pre-existing
dead-code warnings ENVIRONMENT.md already records (`pump_task`, `sidebar_projects`), nothing new.

Labels used: `fin-brw*`, `fin-brw3` .. `fin-brw28` (one label per isolated drive, to avoid reusing
a stale on-disk DB across attempts — see the flaky-click note below, which cost most of this
session's time and is worth recording so the next critic doesn't rediscover it).

## The load-dependent dropped-click trap (read before re-driving this shard)

`uptime` mid-session read **load average 21.6** (many other agents' `sway`/`tiller`/`claude`
processes sharing the box) — exactly the starvation ENVIRONMENT.md warns about ("roughly half its
drives returned a blank first frame and individual synthetic clicks were dropped"). It reproduced
here concretely: a single `click` on the browser's address field, immediately after a `ctl
browser.navigate`, silently missed **5 of 6 attempts** (confirmed via `compare -metric AE` showing
**zero** pixel difference across click → ctrl+a → type → Return — not just an invisible focus
ring, literally nothing reached the app). Two fixes, used together from then on:
1. **Click twice** for a plain focus-and-type target (address field). The second click is a no-op
   if the first landed, and lands if the first didn't. **Do not** double-click a stateful *toggle*
   control (the reload/stop icon) this way — the second click flips it back.
2. For a toggle or one-shot action (Stop, Allow), **verify via `ctl browser.get` or a screenshot
   before deciding to retry**, so a retry can't silently invert an action that actually landed.

Once this was in place, every remaining gesture in this shard landed on the first or second try.

## Row-by-row

### F-BRW-01 — browser chrome + content on open — PASSED
Live, X11-nested lane: `browser.open url=http://127.0.0.1:8981/probe.html` (a local
`python3 -m http.server` serving a solid `#ff2d95` page with 96px white text), `tab.select
index=3`. Capture shows the real toolbar (back/forward/reload, address field reading the exact
URL, globe glyph), tab bar, and the page's actual pink content with the white text — not chrome
alone. `8842` colours (well above the lane's 200-colour blank threshold).

### F-BRW-02 — Back / Forward / Reload / Stop — PASSED
All four driven with real `click` gestures, verified by content change:
- **Back/Forward**: navigated probe.html → `https://example.com` (via `ctl`), then real click at
  `(357,89)` → content reverted to the pink probe page (Back followed real history); real click at
  `(393,89)` → content returned to the real "Example Domain" page (Forward). Two-way, unambiguous.
- **Reload**: real click at `(429,89)` on Example Domain — reload icon transitions to its loading
  glyph and the page stays "Example Domain" (a live re-fetch of the same URL, not a no-op).
- **Stop**: real click on a genuinely slow load (a hand-rolled Python `TCPServer` that sleeps 8s
  before writing any response). Sequence: submit `http://127.0.0.1:8982/` → `ctl browser.get`
  confirms `loading:true` mid-flight → single click on the reload/stop toggle (verified via
  `ctl browser.get` before any retry, per the trap above) → `loading` flips to `false` **and** the
  address/content revert to the last-committed page (`https://example.com/`) → held for 9 more
  seconds (past the slow server's 8s delay) and the "SLOW LOADED" page **never** appears. A
  positive control in the same session (no Stop click) proved the slow page **does** eventually
  render ("SLOW LOADED" in green, confirmed) — so the interrupted case is a genuine Stop effect,
  not a load that would have failed anyway.

### F-BRW-03 — address-field entry and submit — PASSED
Real gesture, not `ctl`: click address field (×2, per the dropped-click note) → `ctrl+a` → typed
`http://127.0.0.1:8982/` character-by-character via `xdotool type` → `Return`. `ctl browser.get`
0.5s later shows `loading:"true"`, `url:"http://127.0.0.1:8982/"` — the exact typed address, mid
real navigation. (Separately: typing `https://example.com` and Return also correctly navigated and
re-rendered "Example Domain".)

### F-BRW-04 — invalid-address / navigation-failure errors — PASSED
Both halves driven live via real typed input, not `ctl`:
- Typed `not a valid url ###` into the address field, `Return` → a red banner reading **"Enter a
  valid HTTP or HTTPS address"** rendered above the (unchanged) page content.
- Typed `http://127.0.0.1:8989/` (nothing listening on that port), `Return` → content area shows
  **"Could not connect to 127.0.0.1: Connection refused"**, rendered as real page content.

### F-BRW-05 — "Agent driving" indicator — PASSED
`ctl browser.act surface=surface:2 driving=true` → an orange **"Agent driving"** pill renders
next to the Stop button (absent in every prior frame). `driving=false` → pill gone on the next
frame. Clean two-state toggle, both captured.

### F-BRW-06 — Allow/Deny permission doorhanger — PASSED
`driving=true` then `browser.navigate` to an ungranted origin (`https://agent-target.example`)
returns `{"permission":"requested"}` and renders a real banner: **"Allow agent browser access to
https://agent-target.example?"** with **Allow**/**Deny** as real clickable text. A genuine
`xdotool` click on **Allow** (real coordinates on the rendered banner, kept-alive instance)
dismissed it; a repeat `browser.navigate` to the same origin then proceeded straight to the actual
network attempt (DNS failure on the fictitious domain) instead of re-prompting — proving the click
actually resolved the pending grant, not just visually dismissed a banner.

### F-BRW-07 — grant persists across relaunch — PASSED
Strengthened past the ledger's own bar: after granting `https://agent-target.example` (F-BRW-06
above), the **entire `tiller` process was killed and relaunched** (`pkill`, then re-exec the same
binary with the same `TILLER_DB`/`TILLER_SOCKET`) — not just a UI reload. A fresh `project.add` +
`browser.open` + `driving=true` + `browser.navigate` to the same origin in the **new process**
returned the real DNS-failure error directly, with **no** `permission:"requested"` — the grant
survived a real process restart via the durable SQLite store. Screenshot of the new process's
browser tab confirms no doorhanger rendered.

### F-BRW-08 — revoke one / revoke all from Permissions — PASSED
Named drawn test, run today: `cargo test -p tiller_ui browser_origin_grants_render_empty_and_revoke_actions`
→ **1 passed**. The test seeds two origins, opens the real Permissions screen
(`debug_bounds("settings-category-Permissions")`), `simulate_click`s a real
`settings-revoke-browser-origin-0` control (two origins → one), then a real
`settings-revoke-all-browser-origins` control, and asserts the empty-state marker
(`settings-browser-grants-empty`) is present — both actions independently verified, matching the
row's clause exactly (revoke-one leaves one row; revoke-all leaves the explicit empty state).

### F-BRW-09 — internal open + Cmd+Shift(platform) system-browser bypass — PASSED
No shipped agent (installed `claude`) or `chat_fixture.py` mode emits a markdown link, so a small
scratch ACP v1 agent (`/tmp/fin-brw-link-agent.py`, not committed — same category as
`chat_fixture.py`, standing in for a counterparty this box's installed CLI can't produce) streams
one assistant message with two real markdown links, wired in via `TILLER_ACP_PROGRAM` (the same
global agent-command override the shipped code already reads at `main.rs:2164`). Live:
- A plain real `click` on the "go to example" link (`http://127.0.0.1:8981/probe.html`) opened a
  **new internal Browser tab** at that exact URL, rendering the real pink probe content — the
  `ChatEvent::OpenLink` path, confirmed end to end, not just source-read.
- A held-modifier click (`xdotool keydown super+shift`, click, `keyup`) on the second link
  (`http://example.com/`) opened **no** new internal tab — the transcript is unchanged and only
  the original 2 tabs (Chat, Terminal) exist afterward. This is chat.rs:822's exact branch
  (`event.modifiers.platform && event.modifiers.shift => cx.open_url(target)`), now exercised live
  rather than only read. The external half (a real system browser actually opening) is not
  independently observable in this sandboxed nested-X11 lane — no xdg-desktop-portal — matching
  the caveat the prior PASSED already carried; the *absence* of an internal tab is the
  discriminator this pass adds.

### F-CTRL-BROWSER-01 — capability list + browser.errors discrepancy — PASSED
`ctl system.capabilities` today lists exactly the 9 documented `browser.*` methods (`open`,
`navigate`, `act`, `get`, `wait`, `eval`, `console`, `snapshot`, `permission`) — `browser.errors`
is absent, and calling it directly returns `ok:false` with `"browser.errors is unsupported on
Linux: browser automation is not implemented"`. Went further than the prior pass: **every one** of
the 9 advertised methods was individually called and returned real (non-stub) behavior this
session (see rows below and the F-BRW-06 permission grant, which used `browser.permission`
directly over the socket as well as via a UI click) — the row's narrow clause holds, and the
"non-open methods are stub refusals" prose the ledger already flagged as stale in a prior pass
remains stale, now re-confirmed with fresh timestamps for every method.

### F-CTRL-BROWSER-02 — workspace-required rejection — PASSED
On a **fresh** instance (no `project.add`): `ctl workspace.current` and `ctl browser.open` both
return `{"ok":false,"error":"no current workspace"}` — the same failure, closing the inconsistency
the row exists to check.

### F-CTRL-BROWSER-03 — browser.navigate / browser.get real state — PASSED
`browser.open` then `browser.get` returns a real `BrowserState`
(`canGoBack`/`canGoForward`/`loading`/`title`/`url`) reflecting the actual loaded page, live and
repeatedly, across every session in this pass.

### F-CTRL-BROWSER-04 — browser.screenshot vs browser.snapshot discrimination — PASSED
`browser.screenshot` returns the blanket `"...is unsupported on Linux: browser automation is not
implemented"` (it is deliberately excluded from `BROWSER_CAPABILITIES` — source comment at
`main.rs:219-226` explains why: wry 0.56's WebKitGTK backend has no pixel-capture entry point).
`browser.snapshot` on the same live surface returns a real structural snapshot: `{"title":"Example
Domain","url":"https://example.com/","elements":[{"tag":"h1",...},{"tag":"a","name":"Learn
more",...}]}` — genuinely different code paths, discriminated live today.

### F-CTRL-BROWSER-05 — browser.act / browser.wait — PASSED
`browser.act surface=surface:2 verb=click selector=a` on the live Example Domain page **actually
clicked** the "Learn more" link — a follow-up `browser.get` shows
`url:"https://www.iana.org/help/example-domains"`, `title:"Example Domains"`, and a screenshot
confirms the real IANA page rendered (logo, nav, body copy). `browser.wait
surface=surface:2 condition=url value=iana timeoutMs=3000` returned `{"loading":"false",
"timedOut":"false",...}` against that same real state — both a real action and a real wait
condition, not stubs.

### F-CTRL-BROWSER-06 — browser.eval / browser.console — PASSED
`browser.eval script=document.title` on the live Example Domain page returned
`{"result":"\"Example Domain\""}` — a real evaluated value from the page's own JS context, not the
"Browser child is unavailable" failure mode the row exists to distinguish from.

### F-CORE-UI-01 — appearance follows system only in System mode — PASSED
Named test, run today: `cargo test --manifest-path rust/Cargo.toml -p tiller_project ui::` →
`appearance_follows_system_only_in_system_mode ... ok`. Pure state-machine logic
(`AppearanceMode::resolve`) — machine tier, proven by executing the test, not by reading it.

### F-CORE-UI-02 — updater state transitions — PASSED
Same run: `updater_reaches_every_user_visible_state_and_clamps_progress ... ok` — exercises every
`UpdateEvent` transition (`CheckStarted` → `Checking`, `Available`, `DownloadProgress` with
clamping to 100 from an out-of-range 150, `InstallStarted`, `Finished`, `Failed`) against
`UpdateState::transition`.

### F-CORE-PLAT-01 — TillerCore macOS-15-only platform declaration — N/A — platform
Re-checked the reference today, not carried from a stale pass: `Packages/TillerCore/Package.swift`
still declares `platforms: [.macOS(.v15)]` and only that. No Linux build manifest in this rewrite
replaces or conditionalizes it — the row's own VERIFY clause says this is a reference gap, not a
current-build step, so N/A is the correct verdict, re-confirmed against the live Swift source
rather than assumed from the ledger.

## Cleanup

All `fin-brw*` `tiller`/`sway` processes and the two scratch `http.server` instances (ports 8973,
8981) started for this pass were killed at the end of the session. Scratch files left in `/tmp`
(`fin-brw-link-agent.py`, `fin-brw-slow.py`, `fin-brw-www/`, `fin-brw-http/`, `fin-brw-http2/`,
`fin-brw*-shots/`) are ephemeral critic evidence, not committed, matching this project's existing
convention for scratch fixtures (see `FINISH-control.md`'s "not committed — ephemeral" fixtures).

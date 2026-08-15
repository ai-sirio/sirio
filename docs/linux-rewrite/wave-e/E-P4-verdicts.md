# Wave E slice E-P4 — critic verdicts

Critic is independent of the E-P4 builder. Instrument: `Scripts/linux-drive.sh` on `DISPLAY=:1`
(drive lock held under label `critic-ep4`), binary built 2026-08-15 21:28 (after fix commit
`33fbb97` at 20:45, so the running binary contains the fix). `convert <capture>.png -crop
<row> txt:-` pixel scans, same method D-P1 used.

## `F-BRW-01` — FAILED — defective

**Route exercised exactly as specified.** Opened a brand-new Browser tab live (tab-bar `+` → "New
Browser"), typed `https://example.com` into the address bar, pressed Return, waited for real
WebKitGTK content to render (`Example Domain` heading + body text visible, not a blank/error
page). Pixel-scanned a content row (`y=400`, well below the header) and the chrome row (`y=55`)
across three separate captures: immediately after load, and again after switching tabs away and
back twice (forcing additional `prepaint` calls, which is when the builder's calibration logic
re-runs).

**Result: identical in all three captures.** Content span `x=331..1060`; true pane chrome bounds
(divider before sidebar `x=386`, divider before Files panel `x=1236`, both measured from the same
frames) `x=386..1236`. That is the exact ~55px left bleed / ~177px right gap D-P1 measured before
any fix existed — reproduced fresh, live, on the fixed binary, three times, including after forced
re-prepaints.

**Root cause of why the fix is inert, read from source:** `browser.rs`'s `prepaint` calls
`webview.set_bounds(requested)` and then, in the same synchronous call, immediately calls
`webview.bounds()` to measure what "actually landed," to derive the correction factor. But
`wry::WebView::bounds()` (`wry-0.56.1/src/webkitgtk/mod.rs:935`) reads `XGetWindowAttributes`
directly, while `set_bounds`'s resize goes through GTK/GDK widget allocation, which is applied on
the next GLib main-loop iteration, not synchronously. So the very first calibrating read-back sees
the *pre-resize* geometry, computes a spurious ~1.0 ratio, and the code's own guard
(`if (factor - 1.0).abs() > 0.01`) then locks in `scale_correction = Some(1.0)` — a permanent
no-op — and never measures again, because the `Some(_)` branch is taken forever after. This matches
the wave's own listed trap ("green test != working feature"): the 10/10 unit test passes because it
mocks `bounds()` to return the already-resized value, which is not what the real synchronous-call
ordering produces.

**Evidence discriminates:** a working fix would show the two spans converging (content span ==
chrome bounds); an unfixed build reproduces exactly D-P1's numbers. All three live captures show
the latter, unchanged across forced re-prepaints — this is not "default value == correct value"
ambiguity.

Frames: `/tmp/.../scratchpad/ep4/04-newbrowser-loaded.png`, `05-newbrowser-settled.png` (both fresh
tabs, not the sweep-D orphan directory, not cited).

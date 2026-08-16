# P127 — the embedded browser's webview child does not start on this box

Three inventory rows are stuck behind this, and it is the single largest remaining blocker that is
not a missing feature: `F-CTRL-BROWSER-05`, `F-CTRL-BROWSER-06` (`half-proven`) and `F-BRW-01`
(`FAILED — defective`).

## What several independent critics observed

- Under **Wayland**, `build_webview` / `build_production_webview` (`tiller_ui/src/browser.rs:67`,
  `:759`) fail outright, with a visible XCB error banner. No native `WebView` is ever constructed, so
  nothing can fire a load-finished signal — which is why `browser.wait` never sees `loading` flip to
  false.
- Under **X11** (`DISPLAY=:1`), two independent fresh launches still returned
  `Browser child is unavailable` (`browser.rs:999`, `:1070`) from `browser.eval`, and
  `browser.get` read `loading: true` / `title: ""` immediately and again 15 s later.
- A `curl` to the same URL succeeded instantly in the same session, so **the network is not the
  cause**.

## Why it matters beyond these three rows

`F-BRW-01`'s own defect is described as pixel geometry — the webview mis-registered against GTK's
real window geometry. If the child never starts on this machine, then *every* browser row's evidence
is measuring the failure mode rather than the feature, and a geometry fix cannot be verified here at
all. Several browser rows already PASSED were judged through the control socket rather than through
rendered pixels, which is consistent with that.

## What to determine, in order

1. **Does the webview child ever start on this box, under either lane?** Capture the actual error
   from `wry`, not the app's wrapped string — `build_webview` returns `Result<WebView, wry::Error>`
   and the concrete error is being flattened into the generic banner.
2. **Is the missing piece a system dependency?** `wry` on Linux needs a WebKitGTK runtime
   (`webkit2gtk-4.1` / `libwebkit2gtk`). If it is absent, this is an environment gap and the three
   rows are `UNREACHABLE` on this machine rather than defective — a materially different verdict, and
   one no amount of app-side work would change.
3. **Only if the child does start**, revisit `F-BRW-01`'s geometry claim with real pixels.

Do not fix `F-BRW-01` before answering 1 and 2. A geometry fix validated against a webview that never
renders is not validated at all.

## Related

`P126` — the 5 s `CONTROL_ACTION_TIMEOUT` truncates `browser.wait`, which makes a genuinely-hung
child and a truncated wait look identical to the caller. Answering P127 is easier with P126 fixed
first.

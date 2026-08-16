# P127 — "Browser child is unavailable" is an instrument misconfiguration, not an app defect

Three rows were held back by this: `F-CTRL-BROWSER-05`, `F-CTRL-BROWSER-06` (`half-proven`) and
`F-BRW-01` (`FAILED — defective`). **The embedded browser works.** What failed was the way it was
being driven.

## What the app itself says

Driving the Wayland lane and photographing the Browser tab shows the app's own red banner, quoting
the real error rather than the flattened one:

> Direct XCB build failed: the window handle kind is not supported; XCB→Xlib adapter failed: GPUI
> returned unsupported handle: `Wayland(WaylandWindowHandle { surface: 0x56ed61600830 })`

`wry`'s `build_as_child` accepts only an X11 (Xlib/XCB) parent on Linux. Under Wayland, GPUI hands it
a Wayland surface, `XlibParent::from_gpui` cannot convert it, and both branches in
`BrowserSurface::new` (`tiller_ui/src/browser.rs:848`, `:852`) fail. `webview` stays `None`, and every
later call reports `Browser child is unavailable` (`browser.rs:999`, `:1070`).

Two hypotheses were checked and **both are wrong**, so do not spend time on them again:

- **Missing WebKitGTK.** `pkg-config --exists webkit2gtk-4.1` succeeds on this box; `ldconfig` lists
  the libraries. The runtime is installed.
- **EGL / software-GL.** The app log does show `MESA: error: ZINK: failed to choose pdev` and
  `egl: failed to create dri2 screen`, which look like the cause and are not. Re-running with
  `WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1`
  changes nothing — the webview is never constructed, so no renderer setting can matter.

## Proof that it works, on X11

`Scripts/linux-drive.sh` launches with `env -u WAYLAND_DISPLAY DISPLAY=:1`. On that lane the app
renders real `example.com` content in the Browser tab, and with a control socket attached:

```
browser.open url=https://example.com  -> {"surface":"surface:2","url":"https://example.com"}
browser.get                           -> {"loading":"false","title":"Example Domain",
                                          "url":"https://example.com/"}
browser.eval script=document.title    -> {"result":"\"Example Domain\""}
```

`loading` flips to `false` and `document.title` resolves — the exact observations
`F-CTRL-BROWSER-05` and `F-CTRL-BROWSER-06` require, and the ones every previous attempt failed to
get.

## The trap that produced the false negatives

**`DISPLAY=:1` alone is not enough.** GPUI prefers Wayland whenever `WAYLAND_DISPLAY` is set, so a
manual launch that only exports `DISPLAY` still takes the Wayland path — and then the webview cannot
attach, exactly as above. `linux-drive.sh` unsets `WAYLAND_DISPLAY` for this reason; a hand-rolled
launch that skips that step reproduces the failure while appearing to be "on the X11 lane".

Note the symmetry with the note already in `wayland-drive.sh`: *"DISPLAY must be UNSET, not empty:
with it set at all, GPUI takes the X11 path."* Each lane must clear the other's variable. The
embedded browser is the one feature where the choice is not a rendering preference but a hard
capability boundary.

## Correct recipe for any browser row

```bash
export TILLER_SOCKET=/tmp/<label>.sock TILLER_DB=/tmp/<label>.sqlite
Scripts/linux-drive.sh out.png '
  ctl project.add path=<repo>
  ctl workspace.select workspace=<id from workspace.list>   # REQUIRED — project.add does not select
  ctl browser.open url=<url>
  sleep 5
  ctl browser.get
'
```

`workspace.select` takes `workspace=<id>`, **not** `path=`. Without it every browser call returns
`no current workspace` or `no browser surface`, which is a third distinct failure that also looks
like a broken browser.

## Consequences

- `F-CTRL-BROWSER-05` / `F-CTRL-BROWSER-06`: the evidence for `PASSED` now exists. It was gathered by
  the orchestrator, so per `P125` it does **not** count as independent provenance — an independent
  critic must reproduce it with the recipe above.
- `F-BRW-01`: its `FAILED — defective` verdict concerns pixel geometry and was recorded partly under
  the broken instrument. Re-judge on a correctly configured X11 instance before treating the geometry
  claim as real.
- The Wayland lane cannot exercise the embedded browser at all. For browser rows only, the X11 lane
  is not a fallback — it is the only lane, and rows unreachable there are genuinely `UNREACHABLE`.

## Related

`P126` — the hardcoded 5 s `CONTROL_ACTION_TIMEOUT` truncates `browser.wait`, making a truncated wait
indistinguishable from a hung child. That compounded the confusion here and is still worth fixing.

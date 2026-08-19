# Decision needed — what Tiller does on a pure-Wayland compositor

`F-WIN-06` is `FAILED — defective` for one reason: under native Wayland the Browser tab is created,
gets its sidebar entry and its address-bar chrome, and then shows a permanent red banner instead of
a page. This is not a bug with a fix. It is a choice the product has not made yet, and it is the
user's to make.

Written 2026-08-19 so the choice can be made from one page instead of from an investigation.

## Why the page does not render

The webview is a real **X11 child window** layered above GPUI's surface, reparented into the app's
own window. That is what `wry`'s `build_as_child` does on Linux, and it is why the browser works
perfectly under X11/XWayland (`Scripts/x11-nested-drive.sh`, `docs/linux-rewrite/X11-NESTED-LANE.md`).

Under native Wayland there is no window to be a child of. `XlibParent::from_gpui`
(`rust/crates/tiller_ui/src/browser.rs`) asks GPUI for the window handle and gets
`RawWindowHandle::Wayland`, which it cannot convert; both build attempts fail and the surface keeps
their error strings. Hence the banner.

Three ways out were examined and all three are closed, for reasons that are protocol-level rather
than missing effort:

- **Reparent the webview's surface.** `wl_surface` is a per-client protocol object. There is no
  cross-client `XReparentWindow` equivalent, by design — a Wayland client cannot adopt another
  client's surface.
- **Embed via a toolkit.** GTK4 removed XEmbed, `GtkSocket` and `GtkPlug` and shipped no Wayland
  replacement. There is nothing to embed *into*.
- **Position a separate top-level over the app.** `xdg_toplevel` has no `set_position`. A client
  cannot place its own window, so it cannot be made to track a pane rectangle.

So on Wayland the options are not "how do we embed it" but "what do we show instead".

## The three real options

**A — Run Tiller on X11 on Linux, always.** Prefer GPUI's X11 backend, so on a Wayland session the
app runs under XWayland and the browser simply works. One decision at startup, and every row that
depends on the webview goes green.
*Cost:* on a Wayland desktop — which is what this machine runs, COSMIC — the whole app becomes an
XWayland client. Fractional scaling, per-monitor DPI, native input handling and clipboard behaviour
all become XWayland's rather than the compositor's. That is a real, visible regression for every
part of the app in exchange for one tab.

**B — Stay Wayland-native, and be honest about the browser.** Keep the app native. When the browser
cannot be built, say why in the user's terms and say what to do about it, instead of showing
`Direct XCB build failed: …; XCB→Xlib adapter failed: …`, which is a developer's error string.
*Cost:* the Browser tab does not work on a Wayland session. `F-WIN-06` stays failed as a feature,
but stops being a bug.

**C — Relaunch under XWayland on demand.** Native Wayland by default; when the user opens a Browser
tab, offer to restart the app under X11 (`GDK_BACKEND=x11`, `WAYLAND_DISPLAY` unset), restoring the
session. Both worlds, at the price of a restart the user has to accept.
*Cost:* the most machinery, and a restart in the middle of a workflow is intrusive even when it is
offered rather than forced.

## Recommendation

**B now, C later if the browser turns out to matter day to day.** The app is native on the user's own
desktop, nothing regresses, and the failure becomes explicable. A is a large, permanent, whole-app
cost paid for one feature. C is the right answer only once we know that feature is used.

## What can be built without deciding

Option B's honest fallback is an improvement under any of the three choices, because A and C both
still need a message for the case where XWayland is unavailable. It should:

- name the actual cause — running on Wayland, where the embedded browser cannot attach — rather
  than printing two failed builder errors;
- keep the tab, its address bar and its chrome, so the pane is not simply empty;
- say what works: relaunching the session under X11, or `GDK_BACKEND=x11`;
- stay inside the platform-gated module, so nothing about it leaks into the shared path.

`F-BRW-08` (the origin URL on Settings → Permissions) and the rest of the browser rows are already
proven under the X11 lane and are not affected by this decision.

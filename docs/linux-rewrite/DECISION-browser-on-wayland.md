# Decision — Tiller forces X11 on Linux

> **DECIDED 2026-08-19 by the user: "Forziamo X11" — option A below.**
>
> Tiller prefers the X11 backend on Linux. On a Wayland session it therefore runs as an XWayland
> client, and the Browser tab works everywhere. The whole-app cost named under option A is accepted
> knowingly; it is not an oversight to be re-litigated by a later critic.
>
> ## How it is implemented
>
> `gpui::guess_compositor()` (zed `c05e346`, `crates/gpui/src/platform.rs:97`) picks the backend
> purely from the **process environment**: `WAYLAND_DISPLAY` non-empty wins, else `DISPLAY`
> non-empty, else headless. No GPUI patch is needed and none should be written — the decision is
> made by preparing the environment before GPUI reads it.
>
> At the top of `main`, in a Linux-gated module, before any thread starts (which is what makes
> `std::env::set_var`/`remove_var` sound):
>
> - `DISPLAY` set and non-empty → clear `WAYLAND_DISPLAY` and set `GDK_BACKEND=x11`. **Both** are
>   required: the first sends GPUI to X11, the second sends GDK — and therefore wry — to X11, so
>   `gdk_x11_display_get_xdisplay` has an X11 display to return. Setting only the first gives a
>   window wry still cannot attach to.
> - `DISPLAY` empty or unset → change nothing. There is no X server and no XWayland to fall back to;
>   forcing here would turn a working Wayland app into a headless one. The browser shows its
>   fallback message and everything else keeps working.
>
> Escape hatch, on the existing `TILLER_*` precedent (`TILLER_SOCKET_ENABLE`, `TILLER_GIT_TIMEOUT_MS`):
> `TILLER_FORCE_X11=0` opts out and keeps the app Wayland-native for anyone who prefers that and does
> not need the browser.
>
> The decision itself must be a pure function — environment in, action out — so it can be tested
> without a display, and the live proof is a critic starting the app on a Wayland session and
> finding an XWayland client with a rendering page in it.
>
> ## What this changes about verification
>
> `Scripts/x11-nested-drive.sh` becomes the lane that matches how the app actually ships;
> `Scripts/wayland-drive.sh` now exercises a configuration users will not normally be in. Rows
> proven only under the Wayland lane are not thereby wrong, but the X11 lane is the one that counts
> from here.

## The original decision brief, kept for its reasoning

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

## Recommendation, and what was actually chosen

I recommended **B now, C later if the browser turns out to matter day to day** — the app stays native
on the user's own desktop, nothing regresses, and the failure becomes explicable.

**The user chose A.** Recorded here rather than quietly replaced, because the reasoning against A is
still true and someone will meet it later: on a Wayland desktop the whole app becomes an XWayland
client, and scaling, per-monitor DPI, input and clipboard become XWayland's. That is the price, it
was named before the choice, and the choice was made anyway. It is settled.

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

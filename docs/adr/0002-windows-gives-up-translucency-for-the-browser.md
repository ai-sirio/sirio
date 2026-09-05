# 2. Windows gives up translucency so the browser can paint

Date: 2026-08-26

## Status

Accepted. The DirectComposition opt-out stands; the "no translucent chrome"
consequence is amended by ADR 0003, which vendors `gpui_windows` with a bitblt
swap chain on the same fallback path so the blur can show after all.

## Context

GPUI creates its Windows window with `WS_EX_NOREDIRECTIONBITMAP` and composes
through DirectComposition (`gpui_windows/src/window.rs:492-494` and
`directx_renderer.rs:920`, rev `c05e346`). A window with no redirection surface
is never given child-HWND content by the DWM.

wry's WebView2 backend hosts the page in exactly such a child HWND — it uses
`ICoreWebView2Controller` via `CreateCoreWebView2ControllerWithOptions(hwnd, …)`,
and wry 0.56.1 has no composition support at all (no occurrence of "composition"
anywhere under `src/`). So on Windows the webview is created, visible, correctly
positioned and successfully navigating, and its pixels have nowhere to appear.
This was measured on real hardware, not inferred: the page title round-trips into
GPUI chrome while the page area stays blank (#144).

GPUI reads `GPUI_DISABLE_DIRECT_COMPOSITION` once, in `WindowsPlatform::new`, and
when set switches to `CreateSwapChainForHwnd` and drops the ex-style. With it, the
page renders. Because it is read once at process start, it cannot be toggled per
window or when a browser tab opens.

That swap has a cost. `create_swap_chain` uses `DXGI_ALPHA_MODE_IGNORE` where the
composition path uses `DXGI_ALPHA_MODE_PREMULTIPLIED`, so window content becomes
opaque and the blur behind it can never show — even though
`set_window_composition_attribute` is still called for
`WindowBackgroundAppearance::Blurred`. On Windows, translucency and a visible
browser are mutually exclusive.

The alternatives were weighed and rejected for now:

- **WebView2 visual hosting** (`CreateCoreWebView2CompositionController`) is the
  architecturally right answer — it composes with GPUI instead of fighting it —
  but needs wry to expose the composition controller *and* GPUI to expose its
  `IDCompositionVisual` (`struct DirectComposition` is private). Two upstream
  changes, one of them against a pinned Zed rev. That is its own project.
- **A separate top-level window** tracking the pane avoids composition entirely,
  but gives back the clipping and z-order behaviour that #139 confirmed already
  works on the child-window path.

## Decision

Sirio sets `GPUI_DISABLE_DIRECT_COMPOSITION` itself on Windows, before GPUI
initialises. It is not exposed as a user setting.

`shell_chrome::current_platform_material` stops reporting native blur support for
`target_os = "windows"`, so the app resolves to `ShellMaterial::Opaque` there
rather than requesting an effect it has already given up the ability to produce.

## Consequences

Windows has no translucent chrome. macOS keeps it; Linux never claimed it.

The browser works on Windows.

There is no setting to get translucency back, deliberately: its only alternative
branch produces an invisible browser, which is a trap rather than a preference.

Sirio now depends on a GPUI environment variable that upstream treats as a
debugging escape hatch. If it is removed or changes meaning, the browser goes
blank again — a risk accepted knowingly, and the reason visual hosting remains
the intended destination rather than this.

This decision is Windows-specific and has no bearing on the X11 child-window path
on Linux, or on WKWebView on macOS, both of which host the page inside the same
hierarchy GPUI renders into.

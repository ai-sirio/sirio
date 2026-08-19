# P102 — The top bar belongs to the OS; we only add our icons

**User directive, 2026-08-19:** «il semaforo non lo devi creare te ma deve dipendere dal SO. In
generale la top bar deve essere quella del sistema operativo + le nostre icone.»

We must not draw window controls. The window's decorations come from the operating system, and
our own icons sit alongside them.

## Where we stand, per platform

**macOS is already correct and must not be touched.** `main.rs:11945` opens the window with
`TitlebarOptions { appears_transparent: true, traffic_light_position: Some(point(px(12.), px(12.))) }`.
That is the native macOS titlebar made transparent so our content can occupy it, with the system's
own traffic lights positioned inside it. The lights are AppKit's, not ours. This is exactly the
shape the directive asks for: OS chrome plus our icons, in one row.

**Linux draws its own, and that is the defect.** `tiller_ui/src/titlebar.rs:6-9` states the premise
it was built on: "neither comet nor Tiller draws traffic lights on Linux — both are OS-native macOS
decorations, and GPUI on X11 hands us a bare window. The three traffic lights below are drawn, real,
circular controls — close/minimize/maximize". So on Linux we render three circles ourselves, styled
after macOS, wired to `remove_window` / `minimize_window` / `zoom_window`
(`titlebar.rs:172-174`, geometry at `:372-393`).

**The premise looks wrong in the GPUI revision we pin.** In
`gpui_linux/src/linux/x11/window.rs:1860`, `request_decorations` sets `_MOTIF_WM_HINTS`:
`WindowDecorations::Server` writes `[1 << 1, 0, 1, 0, 0]` — the decorations flag **on**, i.e. asking
the window manager to decorate — and `Client` writes a `0` in that slot to ask for a bare window.
`gpui/src/window.rs:1582` calls `request_decorations(window_decorations.unwrap_or(WindowDecorations::Server))`,
and we never set `window_decorations`, so we take the default and **already ask for server-side
decorations**. The X11 path even refuses `Client` when no compositor is present and falls back to
`Server` (`:1863-1870`).

If that is what actually happens at runtime, a real window manager is drawing a titlebar *and* we
are drawing three more controls underneath it — two rows of window controls, which is a visible
defect and precisely what the directive objects to.

## The first task is a measurement, not a change

**Do not start by deleting the circles.** Establish what the window actually looks like under a
real window manager first, because the answer decides whether this is a deletion or a redesign:

- Xvfb alone proves nothing here. With no window manager running, nobody draws server-side
  decorations, so a bare window under Xvfb is the expected result either way and cannot
  discriminate. Boot a WM inside your nested display (any reparenting WM will do) and screenshot.
- Read back `_MOTIF_WM_HINTS` on the live window (`xprop -id <id> _MOTIF_WM_HINTS`) and confirm the
  decorations slot really is 1.
- Then screenshot and count the rows of window controls.

Three outcomes, three different jobs:

1. **WM decorates and we also draw circles** → delete our three controls and their theme tokens,
   keep the icon cluster, and let the row above be the OS's. This is the likely case.
2. **WM does not decorate despite the hint** → find out why before writing any workaround, and say
   what the WM did with the hint. Only then decide.
3. **We are somehow requesting `Client`** → find the call site and remove it.

## Constraints on whatever the fix turns out to be

- **Do not touch the macOS path.** It already satisfies the directive.
- Windows has its own caption buttons; whatever is done for Linux must not assume X11. Keep
  platform-specific handling in a gated module, per the standing rule.
- The icon cluster stays ours. The directive is "OS top bar **+ our icons**", not "OS top bar only".
- If the OS titlebar and our icon bar end up as two stacked rows, say so plainly with a screenshot
  rather than quietly designing around it — that is a visible change to the visual bar and the
  user's call, not the builder's.
- The theme tokens `traffic_light_diameter` / `_gap` / `_inset` / `_cluster_gap` and
  `BrowserChrome::cluster_start()` (`tiller_theme/src/lib.rs:595-670`) exist to place controls we
  would no longer draw. `cluster_start()` is also read by `conformance.rs:235-250` and by
  `titlebar.rs`. Removing them is part of the job, not a follow-up — a dead token that still has
  tests is how a deleted feature comes back.

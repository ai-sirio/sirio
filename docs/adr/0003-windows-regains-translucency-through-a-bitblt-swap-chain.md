# 3. Windows regains translucency through a bitblt swap chain

Date: 2026-09-05

## Status

Accepted. Amends the consequences of ADR 0002: the DirectComposition opt-out
stands, the "Windows has no translucent chrome" consequence no longer does.

## Context

ADR 0002 disabled GPUI's DirectComposition on Windows so the Browser surface's
WebView2 child HWND has a redirection surface to compose into, and accepted the
cost that came with GPUI's fallback: `create_swap_chain` in `gpui_windows`
(`directx_renderer.rs`, `bezel-gpui-windows` 0.3.8) creates a **flip-model**
HWND swap chain (`DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL`, `DXGI_ALPHA_MODE_IGNORE`),
and the DWM composes a flip-model HWND swap chain as opaque whatever alpha the
renderer writes. `set_background_appearance(Blurred)` still installed the
acrylic accent policy; nothing of it could show. So `current_platform_material`
stopped claiming native blur on Windows, and the Translucency setting there did
nothing, by design.

Both halves were re-measured on real hardware on 2026-09-05 with an isolated
instance placed over a saturated backdrop:

- Sirio claiming blur on the unpatched flip path: the frame stays fully opaque
  (sampled `rgb(2,2,2)` where the backdrop was magenta). ADR 0002's inference
  holds.
- The same build with the HWND swap chain switched to the **bitblt** model
  (`DXGI_SWAP_EFFECT_DISCARD`, `DXGI_SCALING_STRETCH`,
  `DXGI_ALPHA_MODE_UNSPECIFIED`): the frame shows the blurred backdrop through
  the acrylic accent, and a Browser tab (example.com through WebView2) paints at
  the same time. A bitblt present copies the back buffer, alpha channel
  included, into the window's redirection surface — the surface the DWM does
  composite per-pixel once an accent policy is in effect, and the same surface
  the child HWND needs. Translucency and a visible browser stop being mutually
  exclusive.

The change is one swap-chain descriptor, but it lives in `gpui_windows`, not in
Sirio. Sirio already carries one vendored GPUI platform crate for a one-function
fix (`rust/vendor/gpui_linux`, F-CORE-FILE-03A), with the `[patch.crates-io]`
arrangement described in `rust/vendor/README.md`.

## Decision

`bezel-gpui-windows` 0.3.8 is vendored as `rust/vendor/gpui_windows` under the
same `[patch.crates-io]` arrangement as `gpui_linux`, with exactly one change:
`create_swap_chain` (the no-DirectComposition path) builds a bitblt swap chain.
The composition path, which Sirio never takes, is untouched.

`shell_chrome::current_platform_material` claims native blur on Windows again.

Two things learnt while looking at the result are decided alongside:

- **The fade is applied at startup, not only on the toggle.** The window was
  already opened with the material resolved from the persisted preference, but
  the theme's surface fade was applied only from the settings toggle's
  `SetTranslucency` action, so a restored translucent window drew opaque panels
  over a blurred frame until the toggle was next flipped. Startup and the
  toggle now share `ShellMaterial::apply_to_theme`.
- **Windows gets subtler veils than the theme's defaults.** The acrylic accent
  GPUI installs is nearly untinted, and at the theme's frame default (0.35
  dark / 0.30 light) the desktop bled through loudly enough to distract rather
  than hint. `shell_chrome::MaterialStrength` turns the frame fill up to 0.80 on
  Windows and leaves the panels opaque. The panels are opaque there whatever
  the theme asks: GPUI's Windows renderer accumulates alpha additively
  (`SrcBlendAlpha` and `DestBlendAlpha` are both `ONE` in `directx_renderer.rs`),
  so a panel painted over the frame veil saturates to alpha 1 before the DWM
  composes it — the theme's default 0.45 fade measured as a *darker* panel
  (11 against 13) with nothing showing through. Declaring them opaque is honest
  and matches the theme's own "opaque beats an unblurred, partially transparent
  frame" rule; the blur lives in the title strip and the gaps between panels.
  Every other platform keeps the theme's defaults, none of which has been looked
  at (the macOS build is still unverified, per CLAUDE.md).
  `Theme::with_translucency_at` carries the chosen opacity on the theme so mode
  switches preserve it.

## Consequences

Windows has translucent chrome when the setting is on, and the Browser keeps
painting. Linux still never claimed it.

Sirio now carries two vendored GPUI platform crates. Bumping the pinned
`bezel-gpui-windows` version means re-applying the one-descriptor change in
`create_swap_chain` by hand (or dropping the override if upstream's fallback
path has become translucency-capable), on top of the `gpui_linux` re-diff ADR
0002's sibling README already demands.

The bitblt model is the pre-flip-model present path: one extra GPU copy per
frame into the redirection surface, and no flip-model-only DXGI features
(`DXGI_SCALING_NONE`, frame-latency waitable objects, `Present1` dirty rects —
none of which `gpui_windows` uses today). It applies only when DirectComposition
is disabled, which on Windows is always, for Sirio.

Windows' acrylic accent is an undocumented `SetWindowCompositionAttribute`
policy, the same one GPUI already relied on. Its known cost (frame lag while
dragging a window on some Windows 10 builds) was not measured here, on
Windows 11; if it shows up, `set_background_appearance` in the vendored crate
is the place to switch `Blurred` to a documented `DWMWA_SYSTEMBACKDROP_TYPE`
backdrop.

If translucent *panels* are ever wanted on Windows, the place is the vendored
renderer's quad blend state (`DestBlendAlpha = INV_SRC_ALPHA`, the ordinary
"over" operator, instead of `ONE`), after which `MaterialStrength::WINDOWS`
can ask for a fade that will actually show. Not done here: the request was for
a subtler effect, and that change touches every quad GPUI draws.

WebView2 visual hosting remains the intended destination for the browser
(ADR 0002); it would make this override unnecessary rather than wrong.
